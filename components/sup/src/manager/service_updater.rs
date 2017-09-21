// Copyright (c) 2016-2017 Chef Software Inc. and/or applicable contributors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::collections::HashMap;
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TryRecvError};
use std::thread;

use butterfly;
use common::ui::{Coloring, UI};
use depot_client;
use env;
use hcore::package::{PackageIdent, PackageInstall};
use hcore::service::ServiceGroup;
use launcher_client::LauncherCli;

use {PRODUCT, VERSION};
use census::CensusRing;
use manager::periodic::Periodic;
use manager::service::{Service, Topology, UpdateStrategy};
use util;

static LOGKEY: &'static str = "SU";
const FREQUENCY_ENVVAR: &'static str = "HAB_UPDATE_STRATEGY_FREQUENCY_MS";
const DEFAULT_FREQUENCY: i64 = 60_000;

type UpdaterStateList = HashMap<ServiceGroup, UpdaterState>;

enum UpdaterState {
    AtOnce(Receiver<PackageInstall>),
    Rolling(RollingState),
}

enum RollingState {
    AwaitingElection,
    InElection,
    Leader(LeaderState),
    Follower(FollowerState),
}

enum LeaderState {
    Polling(Receiver<PackageInstall>),
    Waiting,
}

enum FollowerState {
    Waiting,
    Updating(Receiver<PackageInstall>),
}

pub struct ServiceUpdater {
    states: UpdaterStateList,
    butterfly: butterfly::Server,
}

impl ServiceUpdater {
    pub fn new(butterfly: butterfly::Server) -> Self {
        ServiceUpdater {
            states: UpdaterStateList::default(),
            butterfly: butterfly,
        }
    }

    pub fn add(&mut self, service: &Service) -> bool {
        match service.update_strategy {
            UpdateStrategy::None => false,
            UpdateStrategy::AtOnce => {
                self.states
                    .entry(service.service_group.clone())
                    .or_insert_with(|| {
                        let rx = Worker::new(service).start(&service.service_group, None);
                        UpdaterState::AtOnce(rx)
                    });
                true
            }
            UpdateStrategy::Rolling => {
                self.states.entry(service.service_group.clone()).or_insert(
                    UpdaterState::Rolling(RollingState::AwaitingElection),
                );
                true
            }
        }
    }

    pub fn check_for_updated_package(
        &mut self,
        service: &mut Service,
        census_ring: &CensusRing,
        launcher: &LauncherCli,
    ) -> bool {
        let mut updated = false;
        match self.states.get_mut(&service.service_group) {
            Some(&mut UpdaterState::AtOnce(ref mut rx)) => {
                match rx.try_recv() {
                    Ok(package) => {
                        service.update_package(package, launcher);
                        return true;
                    }
                    Err(TryRecvError::Empty) => return false,
                    Err(TryRecvError::Disconnected) => {
                        debug!("Service Updater worker has died; restarting...");
                        *rx = Worker::new(service).start(&service.service_group, None);
                    }
                }
            }

            Some(&mut UpdaterState::Rolling(ref mut st @ RollingState::AwaitingElection)) => {
                if let Some(census_group) = census_ring.census_group_for(&service.service_group) {
                    if service.topology == Topology::Leader {
                        debug!(
                            "Rolling Update, determining proper suitability because we're in \
                                a leader topology"
                        );
                        match (census_group.me(), census_group.leader()) {
                            (Some(me), Some(leader)) => {
                                let suitability = if me.member_id == leader.member_id {
                                    u64::min_value()
                                } else {
                                    u64::max_value()
                                };
                                self.butterfly.start_update_election(
                                    service.service_group.clone(),
                                    suitability,
                                    0,
                                );
                                *st = RollingState::InElection
                            }
                            _ => return false,
                        }
                    } else {
                        debug!("Rolling update, using default suitability");
                        self.butterfly.start_update_election(
                            service.service_group.clone(),
                            0,
                            0,
                        );
                        *st = RollingState::InElection;
                    }
                }
            }
            Some(&mut UpdaterState::Rolling(ref mut st @ RollingState::InElection)) => {
                if let Some(census_group) = census_ring.census_group_for(&service.service_group) {
                    match (census_group.me(), census_group.update_leader()) {
                        (Some(me), Some(leader)) => {
                            if me.member_id == leader.member_id {
                                debug!("We're the leader");
                                // Start in waiting state to ensure all members agree with our
                                // version before attempting a new rolling upgrade.
                                *st = RollingState::Leader(LeaderState::Waiting);
                            } else {
                                debug!("We're a follower");
                                *st = RollingState::Follower(FollowerState::Waiting);
                            }
                        }
                        (Some(_), None) => return false,
                        _ => return false,
                    }
                }
            }
            Some(&mut UpdaterState::Rolling(RollingState::Leader(ref mut state))) => {
                match *state {
                    LeaderState::Polling(ref mut rx) => {
                        match rx.try_recv() {
                            Ok(package) => {
                                debug!("Rolling Update, polling found a new package");
                                service.update_package(package, launcher);
                                updated = true;
                            }
                            Err(TryRecvError::Empty) => return false,
                            Err(TryRecvError::Disconnected) => {
                                debug!("Service Updater worker has died; restarting...");
                                *rx = Worker::new(service).start(&service.service_group, None);
                            }
                        }
                    }
                    LeaderState::Waiting => {
                        match census_ring.census_group_for(&service.service_group) {
                            Some(census_group) => {
                                if census_group.members().iter().any(|cm| {
                                    cm.pkg.as_ref().unwrap() !=
                                        census_group.me().unwrap().pkg.as_ref().unwrap()
                                })
                                {
                                    debug!("Update leader still waiting for followers...");
                                    return false;
                                }
                                let rx = Worker::new(service).start(&service.service_group, None);
                                *state = LeaderState::Polling(rx);
                            }
                            None => {
                                panic!(
                                    "Expected census list to have service group '{}'!",
                                    &*service.service_group
                                )
                            }
                        }
                    }
                }
                if updated {
                    *state = LeaderState::Waiting;
                }
            }
            Some(&mut UpdaterState::Rolling(RollingState::Follower(ref mut state))) => {
                match *state {
                    FollowerState::Waiting => {
                        match census_ring.census_group_for(&service.service_group) {
                            Some(census_group) => {
                                match (
                                    census_group.update_leader(),
                                    census_group.previous_peer(),
                                    census_group.me(),
                                ) {
                                    (Some(leader), Some(peer), Some(me)) => {
                                        if leader.pkg == me.pkg {
                                            debug!("We're not in an update");
                                            return false;
                                        }
                                        if leader.pkg != peer.pkg {
                                            debug!("We're in an update but it's not our turn");
                                            return false;
                                        }
                                        debug!("We're in an update and it's our turn");
                                        let rx = Worker::new(service).start(
                                            &service.service_group,
                                            leader.pkg.clone(),
                                        );
                                        *state = FollowerState::Updating(rx);
                                    }
                                    _ => return false,
                                }
                            }
                            None => {
                                panic!(
                                    "Expected census list to have service group '{}'!",
                                    &*service.service_group
                                )
                            }
                        }
                    }
                    FollowerState::Updating(ref mut rx) => {
                        match census_ring.census_group_for(&service.service_group) {
                            Some(census_group) => {
                                match rx.try_recv() {
                                    Ok(package) => {
                                        service.update_package(package, launcher);
                                        updated = true
                                    }
                                    Err(TryRecvError::Empty) => return false,
                                    Err(TryRecvError::Disconnected) => {
                                        debug!("Service Updater worker has died; restarting...");
                                        let package =
                                            census_group.update_leader().unwrap().pkg.clone();
                                        *rx = Worker::new(service).start(
                                            &service.service_group,
                                            package,
                                        );
                                    }
                                }
                            }
                            None => {
                                panic!(
                                    "Expected census list to have service group '{}'!",
                                    &*service.service_group
                                )
                            }
                        }
                    }
                }
                if updated {
                    *state = FollowerState::Waiting;
                }
            }
            None => {}
        }
        updated
    }
}

struct Worker {
    current: PackageIdent,
    spec_ident: PackageIdent,
    builder_url: String, // TODO (CM): possibly temporary; depends on
    // if I keep depot_client as a thing.
    depot: depot_client::Client,
    channel: String,
    update_strategy: UpdateStrategy,
    ui: UI,
}

impl Periodic for Worker {
    // TODO (CM): Consider performing this check once and storing it,
    // instead of re-checking every time.
    fn update_period(&self) -> i64 {
        match env::var(FREQUENCY_ENVVAR) {
            Ok(val) => {
                match val.parse::<i64>() {
                    Ok(num) => num,
                    Err(_) => {
                        outputln!(
                            "Unable to parse '{}' from {} as a valid integer. Falling back \
                             to default {} MS frequency.",
                            val,
                            FREQUENCY_ENVVAR,
                            DEFAULT_FREQUENCY
                        );
                        DEFAULT_FREQUENCY
                    }
                }
            }
            Err(_) => DEFAULT_FREQUENCY,
        }
    }
}

impl Worker {
    fn new(service: &Service) -> Self {
        Worker {
            current: service.pkg.ident.clone(),
            spec_ident: service.spec_ident.clone(),
            bldr_url: service.bldr_url.clone(),
            depot: depot_client::Client::new(&service.bldr_url, PRODUCT, VERSION, None).unwrap(),
            channel: service.channel.clone(),
            update_strategy: service.update_strategy.clone(),
            ui: UI::default_with(Coloring::Never, None),
        }
    }

    /// Start a new update worker.
    ///
    /// Passing an optional package identifier will make the worker perform a run-once update to
    /// retrieve a specific version from Builder. If no package identifier is specified,
    /// then the updater will poll until a newer more suitable package is found.
    fn start(mut self, sg: &ServiceGroup, ident: Option<PackageIdent>) -> Receiver<PackageInstall> {
        let (tx, rx) = sync_channel(0);
        thread::Builder::new()
            .name(format!("service-updater-{}", sg))
            .spawn(move || match ident {
                Some(latest) => self.run_once(tx, latest),
                None => self.run_poll(tx),
            })
            .expect("unable to start service-updater thread");
        rx
    }

    fn run_once(&mut self, sender: SyncSender<PackageInstall>, ident: PackageIdent) {
        outputln!("Updating from {} to {}", self.current, ident);
        loop {
            let next_time = self.next_period_start();

            match util::pkg::install(
                &mut self.ui,
                &self.builder_url,
                &ident.to_string(), // UGH
                &self.channel,
            ) {
                Ok(package) => {
                    self.current = package.ident().clone();
                    sender.send(package).expect("Main thread has gone away!");
                    break;
                }
                Err(e) => warn!("Failed to install updated package: {:?}", e),
            }

            self.sleep_until(next_time);
        }
    }

    fn run_poll(&mut self, sender: SyncSender<PackageInstall>) {
        loop {
            let next_time = self.next_period_start();

            match util::pkg::install(
                &mut self.ui,
                &self.builder_url,
                &self.spec_ident.to_string(), // UGH
                &self.channel,
            ) {
                Ok(maybe_newer_package) => {
                    if self.current < *maybe_newer_package.ident() {
                        outputln!(
                            "Updating from {} to {}",
                            self.current,
                            maybe_newer_package.ident()
                        );
                        self.current = maybe_newer_package.ident().clone();
                        sender.send(maybe_newer_package).expect(
                            "Main thread has gone away!",
                        );
                        break; // REALLY!??!
                    } else {
                        // TODO: Add more detail to this
                        debug!("Package found is not newer than ours");
                    }
                }
                Err(e) => warn!("Updater failed to get latest package: {:?}", e),
            }

            self.sleep_until(next_time);
        }
    }
}
