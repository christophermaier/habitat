// Copyright (c) 2017 Chef Software Inc. and/or applicable contributors
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

use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TryRecvError};
use std::thread;
use std::time::Duration;

use time::{SteadyTime, Duration as TimeDuration};

use common::ui::{Coloring, UI};
use env;
use hcore::package::{PackageIdent, PackageInstall};
use util;

pub const SUP_PKG_IDENT: &'static str = "core/hab-sup";
const DEFAULT_FREQUENCY: i64 = 60_000;
const FREQUENCY_ENVVAR: &'static str = "HAB_SUP_UPDATE_MS";

pub struct SelfUpdater {
    rx: Receiver<PackageInstall>,
    current: PackageIdent,
    update_url: String,
    update_channel: String,
}

impl SelfUpdater {
    pub fn new(current: PackageIdent, update_url: String, update_channel: String) -> Self {
        let rx = Self::init(current.clone(), &update_url, update_channel.clone());
        SelfUpdater {
            rx: rx,
            current: current,
            update_url: update_url,
            update_channel: update_channel,
        }
    }

    fn init(
        current: PackageIdent,
        update_url: &str,
        update_channel: String,
    ) -> Receiver<PackageInstall> {
        let (tx, rx) = sync_channel(0);
        let url = update_url.to_string(); // eww
        thread::Builder::new()
            .name("self-updater".to_string())
            .spawn(move || Self::run(tx, current, url, update_channel))
            .expect("Unable to start self-updater thread");
        rx
    }

    fn run(
        sender: SyncSender<PackageInstall>,
        current: PackageIdent,
        builder_url: String,
        channel: String,
    ) {
        debug!("Self updater current package, {}", current);
        loop {
            let next_check = SteadyTime::now() + TimeDuration::milliseconds(update_frequency());

            match util::pkg::install(
                &mut UI::default_with(Coloring::Never, None),
                &builder_url,
                SUP_PKG_IDENT,
                &channel,
            ) {
                Ok(package) => {
                    if current < *package.ident() {
                        debug!(
                            "Self updater installing newer supervisor, {}",
                            package.ident()
                        );
                        sender.send(package).expect("Main thread has gone away!");
                        break;
                    } else {
                        debug!("Supervisor package found is not newer than ours");
                    }
                }
                Err(err) => {
                    warn!("Self updater failed to get latest, {}", err);
                }
            }

            let time_to_wait = (next_check - SteadyTime::now()).num_milliseconds();
            if time_to_wait > 0 {
                thread::sleep(Duration::from_millis(time_to_wait as u64));
            }
        }
    }

    pub fn updated(&mut self) -> Option<PackageInstall> {
        match self.rx.try_recv() {
            Ok(package) => Some(package),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                error!("Self updater crashed, restarting...");
                self.rx = Self::init(
                    self.current.clone(),
                    &self.update_url,
                    self.update_channel.clone(),
                );
                None
            }
        }
    }
}

fn update_frequency() -> i64 {
    match env::var(FREQUENCY_ENVVAR) {
        Ok(val) => val.parse::<i64>().unwrap_or(DEFAULT_FREQUENCY),
        Err(_) => DEFAULT_FREQUENCY,
    }
}
