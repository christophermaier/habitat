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

pub mod service;
#[macro_use]
mod debug;
pub mod commands;
mod events;
mod file_watcher;
mod peer_watcher;
mod periodic;
mod self_updater;
mod service_updater;
mod spec_dir;
mod spec_watcher;
mod sys;
mod user_config_watcher;

use std;
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::mem;
use std::net::SocketAddr;
use std::ops::DerefMut;
use std::path::{Path, PathBuf};
use std::result;
use std::str::FromStr;
use std::sync::{Arc, Condvar, Mutex, RwLock};
use std::thread;
use std::time::Duration;

use futures::sync::oneshot;
use num_cpus;

use butterfly;
use butterfly::member::Member;
use butterfly::server::{timing::Timing, ServerProxy, Suitability};
use butterfly::trace::Trace;
use futures::prelude::*;
use futures::{future::Either, sync::mpsc};
use hcore::crypto::SymKey;
use hcore::env;
use hcore::os::process::{self, Pid, Signal};
use hcore::os::signals::{self, SignalEvent};
use hcore::package::{Identifiable, PackageIdent, PackageInstall};
use hcore::service::ServiceGroup;
use launcher_client::{LauncherCli, LAUNCHER_LOCK_CLEAN_ENV, LAUNCHER_PID_ENV};
use protocol;
use rustls::{internal::pemfile, NoClientAuth, ServerConfig};
use serde_json;
use time::{self, Duration as TimeDuration, Timespec};
use tokio::{executor, runtime};

use self::peer_watcher::PeerWatcher;
use self::self_updater::{SelfUpdater, SUP_PKG_IDENT};
use self::service::{health::HealthCheck, DesiredState};
pub use self::service::{
    CompositeSpec, ConfigRendering, Service, ServiceProxy, ServiceSpec, Spec, Topology,
    UpdateStrategy,
};
use self::service_updater::ServiceUpdater;
use self::spec_dir::SpecDir;
use self::spec_watcher::SpecWatcher;

pub use self::sys::Sys;
use self::user_config_watcher::UserConfigWatcher;
use super::feat;
use census::{CensusRing, CensusRingProxy};
use config::{EnvConfig, GossipListenAddr};
use ctl_gateway::{self, CtlRequest};
use error::{Error, Result, SupError};
use http_gateway;
use VERSION;

const MEMBER_ID_FILE: &'static str = "MEMBER_ID";
const PROC_LOCK_FILE: &'static str = "LOCK";

static LOGKEY: &'static str = "MR";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceOperation {
    Start(ServiceSpec),
    Stop(ServiceSpec),
    Restart {
        to_stop: ServiceSpec,
        to_start: ServiceSpec,
    },
}

/// A Supervisor can stop in a handful of ways.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum ShutdownMode {
    /// When the Supervisor is shutting down for normal reasons and
    /// should take all services down with it (i.e., it's actually
    /// shutting down).
    Normal,
    /// When the Supervisor has been manually departed from the
    /// Habitat network. All services should come down, as well.
    Departed,
    /// A Supervisor is updating itself, or is otherwise simply
    /// restarting. Services _do not_ get shut down.
    Updating, // TODO (CM): perhaps Restarting?
}

/// FileSystem paths that the Manager uses to persist data to disk.
///
/// This is shared with the `http_gateway` and `service` modules for reading and writing
/// persistence data.
#[derive(Debug, Serialize)]
pub struct FsCfg {
    pub sup_root: PathBuf,

    data_path: PathBuf,
    specs_path: PathBuf,
    composites_path: PathBuf,
    member_id_file: PathBuf,
    proc_lock_file: PathBuf,
}

impl FsCfg {
    fn new<T>(sup_root: T) -> Self
    where
        T: Into<PathBuf>,
    {
        let sup_root = sup_root.into();
        FsCfg {
            specs_path: sup_root.join("specs"),
            data_path: sup_root.join("data"),
            composites_path: sup_root.join("composites"),
            member_id_file: sup_root.join(MEMBER_ID_FILE),
            proc_lock_file: sup_root.join(PROC_LOCK_FILE),
            sup_root: sup_root,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ManagerConfig {
    pub auto_update: bool,
    pub custom_state_path: Option<PathBuf>,
    pub eventsrv_group: Option<ServiceGroup>,
    pub update_url: String,
    pub update_channel: String,
    pub gossip_listen: GossipListenAddr,
    pub ctl_listen: SocketAddr,
    pub http_listen: http_gateway::ListenAddr,
    pub http_disable: bool,
    pub gossip_peers: Vec<SocketAddr>,
    pub gossip_permanent: bool,
    pub ring_key: Option<SymKey>,
    pub organization: Option<String>,
    pub watch_peer_file: Option<String>,
    pub tls_files: Option<(PathBuf, PathBuf)>,
}

impl ManagerConfig {
    pub fn sup_root(&self) -> PathBuf {
        protocol::sup_root(self.custom_state_path.as_ref())
    }
}

impl Default for ManagerConfig {
    fn default() -> Self {
        ManagerConfig {
            auto_update: false,
            custom_state_path: None,
            eventsrv_group: None,
            update_url: "".to_string(),
            update_channel: "".to_string(),
            gossip_listen: GossipListenAddr::default(),
            ctl_listen: protocol::ctl::default_addr(),
            http_listen: http_gateway::ListenAddr::default(),
            http_disable: false,
            gossip_peers: vec![],
            gossip_permanent: false,
            ring_key: None,
            organization: None,
            watch_peer_file: None,
            tls_files: None,
        }
    }
}

/// This represents an environment variable that holds an authentication token for the supervisor's
/// HTTP gateway. If the environment variable is present, then its value is the auth token and all
/// of the HTTP endpoints will require its presence. If it's not present, then everything continues
/// to work unauthenticated.
#[derive(Debug, Default)]
struct GatewayAuthToken(Option<String>);

impl FromStr for GatewayAuthToken {
    type Err = ::std::string::ParseError;

    fn from_str(s: &str) -> result::Result<Self, Self::Err> {
        Ok(GatewayAuthToken(Some(String::from(s))))
    }
}

impl EnvConfig for GatewayAuthToken {
    const ENVVAR: &'static str = "HAB_SUP_GATEWAY_AUTH_TOKEN";
}

/// This struct encapsulates the shared state for the supervisor. It's worth noting that if there's
/// something you want the CtlGateway to be able to operate on, it needs to be put in here. This
/// state gets shared with all the CtlGateway handlers.
pub struct ManagerState {
    /// The configuration used to instantiate this Manager instance
    pub cfg: ManagerConfig,
    pub services: Arc<RwLock<HashMap<PackageIdent, Service>>>,
    pub gateway_state: Arc<RwLock<GatewayState>>,
}

#[derive(Debug, Default)]
pub struct GatewayState {
    pub census_data: String,
    pub butterfly_data: String,
    pub services_data: String,
    pub health_check_data: HashMap<ServiceGroup, HealthCheck>,
    pub auth_token: Option<String>,
}

pub struct Manager {
    pub state: Arc<ManagerState>,
    butterfly: butterfly::Server,
    census_ring: CensusRing,
    events_group: Option<ServiceGroup>,
    fs_cfg: Arc<FsCfg>,
    launcher: LauncherCli,
    updater: Arc<Mutex<ServiceUpdater>>,
    peer_watcher: Option<PeerWatcher>,
    spec_watcher: SpecWatcher,
    // This Arc<RwLock<>> business is a potentially temporary
    // change. Right now, in order to asynchronously shut down
    // services, we need to be able to have a safe reference to this
    // from another thread.
    //
    // Future refactorings may suggest other ways to achieve the same
    // result of being able to manipulate the config watcher from
    // other threads (e.g., maybe we subscribe to messages to change
    // the watcher)
    user_config_watcher: Arc<RwLock<UserConfigWatcher>>,
    spec_dir: SpecDir,
    organization: Option<String>,
    self_updater: Option<SelfUpdater>,
    service_states: HashMap<PackageIdent, Timespec>,
    sys: Arc<Sys>,
    http_disable: bool,
    /// When we're upgrading a service, we remove it from the
    /// Supervisor and add it back. Due to the behavior of Futures and
    /// the current data structures we have, the most straightforward
    /// thing to do is add the specfile of the service in question to
    /// this vec after we've successfully shut it down. Once it's
    /// there, we can act on them in our (non-Tokio-driven) main loop
    /// to re-load them.
    ///
    /// It's a Mutex because we only ever write.
    specs_to_reload: Arc<Mutex<Vec<PathBuf>>>,
}

impl Manager {
    /// Load a Manager with the given configuration.
    ///
    /// The returned Manager will be pre-populated with any cached data from disk from a previous
    /// run if available.
    pub fn load(cfg: ManagerConfig, launcher: LauncherCli) -> Result<Manager> {
        let state_path = cfg.sup_root();
        let fs_cfg = FsCfg::new(state_path);
        Self::create_state_path_dirs(&fs_cfg)?;
        Self::clean_dirty_state(&fs_cfg)?;
        if env::var(LAUNCHER_LOCK_CLEAN_ENV).is_ok() {
            release_process_lock(&fs_cfg);
        }
        obtain_process_lock(&fs_cfg)?;

        Self::new(cfg, fs_cfg, launcher)
    }

    pub fn term(cfg: &ManagerConfig) -> Result<()> {
        let fs_cfg = FsCfg::new(cfg.sup_root());
        match read_process_lock(&fs_cfg.proc_lock_file) {
            Ok(pid) => {
                process::signal(pid, Signal::TERM).map_err(|_| sup_error!(Error::SignalFailed))?;
                Ok(())
            }
            Err(err) => Err(err),
        }
    }

    fn new(cfg: ManagerConfig, fs_cfg: FsCfg, launcher: LauncherCli) -> Result<Manager> {
        debug!("new(cfg: {:?}, fs_cfg: {:?}", cfg, fs_cfg);
        let current = PackageIdent::from_str(&format!("{}/{}", SUP_PKG_IDENT, VERSION)).unwrap();
        outputln!("{} ({})", SUP_PKG_IDENT, current);
        let cfg_static = cfg.clone();
        let self_updater = if cfg.auto_update {
            if current.fully_qualified() {
                Some(SelfUpdater::new(
                    current,
                    cfg.update_url,
                    cfg.update_channel,
                ))
            } else {
                warn!("Supervisor version not fully qualified, unable to start self-updater");
                None
            }
        } else {
            None
        };
        let mut sys = Sys::new(
            cfg.gossip_permanent,
            cfg.gossip_listen,
            cfg.ctl_listen,
            cfg.http_listen,
        );
        let member = Self::load_member(&mut sys, &fs_cfg)?;
        let services = Arc::new(RwLock::new(HashMap::new()));

        let gateway_auth_token = GatewayAuthToken::configured_value();
        let mut gateway_state = GatewayState::default();
        gateway_state.auth_token = gateway_auth_token.0;

        let server = butterfly::Server::new(
            sys.gossip_listen(),
            sys.gossip_listen(),
            member,
            Trace::default(),
            cfg.ring_key,
            None,
            Some(&fs_cfg.data_path),
            Box::new(SuitabilityLookup(services.clone())),
        )?;
        outputln!("Supervisor Member-ID {}", sys.member_id);
        for peer_addr in &cfg.gossip_peers {
            let mut peer = Member::default();
            peer.address = format!("{}", peer_addr.ip());
            peer.swim_port = peer_addr.port();
            peer.gossip_port = peer_addr.port();
            server.member_list.add_initial_member(peer);
        }

        let peer_watcher = if let Some(path) = cfg.watch_peer_file {
            Some(PeerWatcher::run(path)?)
        } else {
            None
        };

        let spec_dir = SpecDir::new(&fs_cfg.specs_path)?;
        spec_dir.migrate_specs();

        let spec_watcher = SpecWatcher::run(&spec_dir)?;

        Ok(Manager {
            state: Arc::new(ManagerState {
                cfg: cfg_static,
                services: services,
                gateway_state: Arc::new(RwLock::new(gateway_state)),
            }),
            self_updater: self_updater,
            updater: Arc::new(Mutex::new(ServiceUpdater::new(server.clone()))),
            census_ring: CensusRing::new(sys.member_id.clone()),
            butterfly: server,
            events_group: cfg.eventsrv_group,
            launcher: launcher,
            peer_watcher: peer_watcher,
            spec_watcher: spec_watcher,
            user_config_watcher: Arc::new(RwLock::new(UserConfigWatcher::new())),
            spec_dir: spec_dir,
            fs_cfg: Arc::new(fs_cfg),
            organization: cfg.organization,
            service_states: HashMap::new(),
            sys: Arc::new(sys),
            http_disable: cfg.http_disable,
            specs_to_reload: Arc::new(Mutex::new(Vec::new())),
        })
    }

    /// Load the initial Butterly Member which is used in initializing the Butterfly server. This
    /// will load the member-id for the initial Member from disk if a previous manager has been
    /// run.
    ///
    /// The mutable ref to `Sys` will be configured with Butterfly Member details and will also
    /// populate the initial Member.
    // TODO (CM): This functionality can / should be pulled into
    // Butterfly itself; we're already setting the incarnation number
    // in there, so splitting the initialization is needlessly
    // confusing. It's also blurs the lines between the manager and
    // Butterfly.
    fn load_member(sys: &mut Sys, fs_cfg: &FsCfg) -> Result<Member> {
        let mut member = Member::default();
        match File::open(&fs_cfg.member_id_file) {
            Ok(mut file) => {
                let mut member_id = String::new();
                file.read_to_string(&mut member_id).map_err(|e| {
                    sup_error!(Error::BadDataFile(fs_cfg.member_id_file.clone(), e))
                })?;
                member.id = member_id;
            }
            Err(_) => match File::create(&fs_cfg.member_id_file) {
                Ok(mut file) => {
                    file.write(member.id.as_bytes()).map_err(|e| {
                        sup_error!(Error::BadDataFile(fs_cfg.member_id_file.clone(), e))
                    })?;
                }
                Err(err) => {
                    return Err(sup_error!(Error::BadDataFile(
                        fs_cfg.member_id_file.clone(),
                        err
                    )))
                }
            },
        }
        sys.member_id = member.id.to_string();
        member.persistent = sys.permanent;
        Ok(member)
    }

    fn clean_dirty_state(fs_cfg: &FsCfg) -> Result<()> {
        let data_path = &fs_cfg.data_path;
        debug!("Cleaning cached health checks");
        match fs::read_dir(&data_path) {
            Ok(entries) => {
                for entry in entries {
                    if let Ok(entry) = entry {
                        match entry.path().extension().and_then(|p| p.to_str()) {
                            Some("tmp") | Some("health") => {
                                fs::remove_file(&entry.path()).map_err(|err| {
                                    sup_error!(Error::BadDataPath(data_path.clone(), err))
                                })?;
                            }
                            _ => continue,
                        }
                    }
                }
                Ok(())
            }
            Err(err) => Err(sup_error!(Error::BadDataPath(data_path.clone(), err))),
        }
    }

    fn create_state_path_dirs(fs_cfg: &FsCfg) -> Result<()> {
        let data_path = &fs_cfg.data_path;
        debug!("Creating data directory: {}", data_path.display());
        if let Some(err) = fs::create_dir_all(&data_path).err() {
            return Err(sup_error!(Error::BadDataPath(data_path.clone(), err)));
        }
        let specs_path = &fs_cfg.specs_path;
        debug!("Creating specs directory: {}", specs_path.display());
        if let Some(err) = fs::create_dir_all(&specs_path).err() {
            return Err(sup_error!(Error::BadSpecsPath(specs_path.clone(), err)));
        }

        let composites_path = &fs_cfg.composites_path;
        debug!(
            "Creating composites directory: {}",
            composites_path.display()
        );
        if let Some(err) = fs::create_dir_all(&composites_path).err() {
            return Err(sup_error!(Error::BadCompositesPath(
                composites_path.clone(),
                err
            )));
        }

        Ok(())
    }

    fn add_service(&mut self, spec: ServiceSpec) {
        // JW TODO: This clone sucks, but our data structures are a bit messy here. What we really
        // want is the service to hold the spec and, on failure, return an error with the spec
        // back to us. Since we consume and deconstruct the spec in `Service::new()` which
        // `Service::load()` eventually delegates to we just can't have that. We should clean
        // this up in the future.
        let service = match Service::load(
            self.sys.clone(),
            spec.clone(),
            self.fs_cfg.clone(),
            self.organization.as_ref().map(|org| &**org),
            self.state.gateway_state.clone(),
        ) {
            Ok(service) => {
                outputln!("Starting {} ({})", &spec.ident, service.pkg.ident);
                service
            }
            Err(err) => {
                outputln!("Unable to start {}, {}", &spec.ident, err);
                return;
            }
        };

        if let Err(e) = service.create_svc_path() {
            outputln!(
                "Can't create directory {}: {}",
                service.pkg.svc_path.display(),
                e
            );
            outputln!(
                "If this service is running as non-root, you'll need to create \
                 {} and give the current user write access to it",
                service.pkg.svc_path.display()
            );
            outputln!("{} failed to start", &spec.ident);
            return;
        }

        self.gossip_latest_service_rumor(&service);
        if service.topology == Topology::Leader {
            self.butterfly.start_election(&service.service_group, 0);
        }

        if let Err(e) = self
            .user_config_watcher
            .write()
            .expect("user-config-watcher lock is poisoned")
            .add(&service)
        {
            outputln!(
                "Unable to start UserConfigWatcher for {}: {}",
                service.spec_ident,
                e
            );
            return;
        }

        self.updater
            .lock()
            .expect("Updater lock poisoned")
            .add(&service);
        self.state
            .services
            .write()
            .expect("Services lock is poisoned!")
            .insert(service.spec_ident.clone(), service);
    }

    pub fn run(mut self, svc: Option<protocol::ctl::SvcLoad>) -> Result<()> {
        let mut runtime = runtime::Builder::new()
            .name_prefix("tokio-")
            .core_threads(TokioThreadCount::configured_value().into())
            .build()
            .expect("Couldn't build Tokio Runtime!");

        let (ctl_tx, ctl_rx) = mpsc::unbounded();
        let (ctl_shutdown_tx, ctl_shutdown_rx) = oneshot::channel();
        let ctl_handler = CtlAcceptor::new(self.state.clone(), ctl_rx, ctl_shutdown_rx).for_each(
            move |handler| {
                executor::spawn(handler);
                Ok(())
            },
        );
        runtime.spawn(ctl_handler);

        if let Some(svc_load) = svc {
            commands::service_load(&self.state, &mut CtlRequest::default(), svc_load)?;
        }
        // This serves to start up any services that need starting
        // TODO (CM): At the moment, when service startup is still
        // synchronous, we expect take_action_on_services to return an
        // empty vector of futures to spawn (since the futures will
        // only be for service shutdowns, and nothing is running yet
        // to shutdown!).
        //
        // However, once startup is also async, we'll start returning
        // futures to run here.
        for f in self.take_action_on_services()? {
            runtime.spawn(f);
        }

        outputln!(
            "Starting gossip-listener on {}",
            self.butterfly.gossip_addr()
        );
        self.butterfly.start(Timing::default())?;
        debug!("gossip-listener started");
        self.persist_state();
        let http_listen_addr = self.sys.http_listen();
        let ctl_listen_addr = self.sys.ctl_listen();
        let ctl_secret_key = ctl_gateway::readgen_secret_key(&self.fs_cfg.sup_root)?;
        outputln!("Starting ctl-gateway on {}", &ctl_listen_addr);
        ctl_gateway::server::run(ctl_listen_addr, ctl_secret_key, ctl_tx);
        debug!("ctl-gateway started");

        if self.http_disable {
            info!("http-gateway disabled");
        } else {
            // First let's check and see if we're going to use TLS. If so, we'll generate the
            // appropriate config here, where it's easy to propagate errors, vs in a separate
            // thread, where that process is more cumbersome.

            let tls_server_config = match self.state.cfg.tls_files {
                Some((ref key_path, ref cert_path)) => match tls_config(key_path, cert_path) {
                    Ok(c) => Some(c),
                    Err(e) => return Err(e),
                },
                None => None,
            };

            // Here we use a Condvar to wait on the HTTP gateway server to start up and inspect its
            // return value. Specifically, we're looking for errors when it tries to bind to the
            // listening TCP socket, so we can alert the user.
            let pair = Arc::new((
                Mutex::new(http_gateway::ServerStartup::NotStarted),
                Condvar::new(),
            ));

            outputln!("Starting http-gateway on {}", &http_listen_addr);
            http_gateway::Server::run(
                http_listen_addr.clone(),
                tls_server_config,
                self.state.gateway_state.clone(),
                pair.clone(),
            );

            let &(ref lock, ref cvar) = &*pair;
            let mut started = lock.lock().expect("Control mutex is poisoned");

            // This will block the current thread until the HTTP gateway thread either starts
            // successfully or fails to bind. In practice, the wait here is so short as to not be
            // noticeable.
            loop {
                match *started {
                    http_gateway::ServerStartup::NotStarted => {
                        started = match cvar.wait_timeout(started, Duration::from_millis(10000)) {
                            Ok((mutex, timeout_result)) => {
                                if timeout_result.timed_out() {
                                    return Err(sup_error!(Error::BindTimeout(
                                        http_listen_addr.to_string()
                                    )));
                                } else {
                                    mutex
                                }
                            }
                            Err(e) => {
                                error!("Mutex for the HTTP gateway was poisoned. e = {:?}", e);
                                return Err(sup_error!(Error::LockPoisoned));
                            }
                        };
                    }
                    http_gateway::ServerStartup::BindFailed => {
                        return Err(sup_error!(Error::BadAddress(http_listen_addr.to_string())));
                    }
                    http_gateway::ServerStartup::Started => break,
                }
            }

            debug!("http-gateway started");
        }

        let events = match self.events_group {
            Some(ref evg) => Some(events::EventsMgr::start(evg.clone())),
            None => None,
        };

        // On Windows initializng the signal handler will create a ctrl+c handler for the
        // process which will disable default windows ctrl+c behavior and allow us to
        // handle via check_for_signal. However, if the supervsor is in a long running
        // non-run hook, the below loop will not get to check_for_signal in a reasonable
        // amount of time and the supervisor will not respond to ctrl+c. On Windows, we
        // let the launcher catch ctrl+c and gracefully shut down services. ctrl+c should
        // simply halt the supervisor
        if !feat::is_enabled(feat::IgnoreSignals) {
            signals::init();
        }

        // Enter the main Supervisor loop. When we break out, it'll be
        // because we've been instructed to shutdown. The value we
        // break out with governs exactly how we shut down.

        // TODO (CM): If ANYTHING in this loop returns an error, we
        // need to safely shut things down. This means the loop likely
        // needs to be its own method.

        let shutdown_mode = loop {
            if feat::is_enabled(feat::TestExit) {
                if let Ok(exit_file_path) = env::var("HAB_FEAT_TEST_EXIT") {
                    if let Ok(mut exit_code_file) = File::open(&exit_file_path) {
                        let mut buffer = String::new();
                        exit_code_file
                            .read_to_string(&mut buffer)
                            .expect("couldn't read");
                        if let Ok(exit_code) = buffer.lines().next().unwrap_or("").parse::<i32>() {
                            fs::remove_file(&exit_file_path).expect("couldn't remove");
                            outputln!("Simulating abrupt, unexpected exit with code {}", exit_code);
                            std::process::exit(exit_code);
                        }
                    }
                }
            }

            let next_check = time::get_time() + TimeDuration::milliseconds(1000);
            if self.launcher.is_stopping() {
                break ShutdownMode::Normal;
            }
            if self.check_for_departure() {
                break ShutdownMode::Departed;
            }
            if !feat::is_enabled(feat::IgnoreSignals) {
                if let Some(SignalEvent::Passthrough(Signal::HUP)) = signals::check_for_signal() {
                    outputln!("Supervisor shutting down for signal");
                    break ShutdownMode::Updating;
                }
            }

            self.finish_restarting_services();

            if let Some(package) = self.check_for_updated_supervisor() {
                outputln!(
                    "Supervisor shutting down for automatic update to {}",
                    package
                );
                break ShutdownMode::Updating;
            }

            if self.spec_watcher.has_events() {
                for f in self.take_action_on_services()? {
                    runtime.spawn(f);
                }
            }

            self.update_peers_from_watch_file()?;
            self.update_running_services_from_user_config_watcher();

            // TODO (CM): this is really a restart, I think
            for f in self.shutdown_services_for_update() {
                runtime.spawn(f);
            }

            self.restart_elections();
            self.census_ring.update_from_rumors(
                &self.butterfly.service_store,
                &self.butterfly.election_store,
                &self.butterfly.update_store,
                &self.butterfly.member_list,
                &self.butterfly.service_config_store,
                &self.butterfly.service_file_store,
            );

            if self.check_for_changed_services() {
                self.persist_state();
            }

            if self.census_ring.changed() {
                self.persist_state();
                events
                    .as_ref()
                    .map(|events| events.try_connect(&self.census_ring));

                for service in self
                    .state
                    .services
                    .read()
                    .expect("Services lock is poisoned!")
                    .values()
                {
                    if let Some(census_group) =
                        self.census_ring.census_group_for(&service.service_group)
                    {
                        if let Some(member) = census_group.me() {
                            events
                                .as_ref()
                                .map(|events| events.send_service(member, service));
                        }
                    }
                }
            }

            for service in self
                .state
                .services
                .write()
                .expect("Services lock is poisoned!")
                .values_mut()
            {
                if service.tick(&self.census_ring, &self.launcher) {
                    self.gossip_latest_service_rumor(&service);
                }
            }

            // This is really only needed until everything is running
            // in futures.
            let now = time::get_time();
            if now < next_check {
                let time_to_wait = next_check - now;
                thread::sleep(time_to_wait.to_std().unwrap());
            }
        }; // end main loop

        // When we make it down here, we've broken out of the main
        // Supervisor loop, which means it's time to shut down. Based
        // on the value we broke out of the loop with, we may need to
        // shut services down. We do that out here, so we can run the
        // shutdown futures directly on the reactor, and ensure
        // they're all driven to completion before we exit.

        // Stop the ctl gateway; this way we'll stop responding to
        // user commands as we're trying to shut down.
        ctl_shutdown_tx.send(()).ok();

        match shutdown_mode {
            ShutdownMode::Updating => {}
            ShutdownMode::Normal | ShutdownMode::Departed => {
                outputln!("Gracefully departing from butterfly network.");
                self.butterfly.set_departed();

                let mut svcs = self
                    .state
                    .services
                    .write()
                    .expect("Services lock is poisoned!");

                for (_ident, svc) in svcs.drain() {
                    let f = self.stop_service(svc).map(|_| ()).map_err(|_| ());
                    runtime.spawn(f);
                }
            }
        };

        // Allow all existing futures to run to completion.
        runtime
            .shutdown_on_idle()
            .wait()
            .expect("Error waiting on Tokio runtime to shutdown");

        release_process_lock(&self.fs_cfg);
        self.butterfly.persist_data();

        match shutdown_mode {
            ShutdownMode::Normal | ShutdownMode::Updating => Ok(()),
            ShutdownMode::Departed => Err(sup_error!(Error::Departed)),
        }
    }

    /// Any services that were shut down in preparation for a restart,
    /// and have successfully shut down, can now be restarted.
    ///
    /// Spec file paths get added to `self.specs_to_reload` at the end
    /// of the futures that are spawned to stop the services in the
    /// first place.
    ///
    /// All operations to start services are currently *synchronous*;
    /// once service addition is asynchronous, the futures used will
    /// just be chained onto the end of the futures that shut them
    /// down, so this function (and `self.specs_to_reload` itself)
    /// will no longer be needed.
    fn finish_restarting_services(&mut self) {
        let paths = {
            let mut paths = vec![];
            mem::swap(
                self.specs_to_reload
                    .lock()
                    .expect("specs_to_reload lock poisoned")
                    .deref_mut(),
                &mut paths,
            );
            paths
        };

        for spec_file in paths.into_iter() {
            match ServiceSpec::from_file(&spec_file) {
                Ok(mut spec) => {
                    spec.desired_state = DesiredState::Up;
                    self.add_service(spec);
                }
                Err(e) => error!(
                    "Failed to reload an upgraded service for spec file '{:?}': {:?}",
                    spec_file, e
                ),
            }
        }
    }

    fn check_for_updated_supervisor(&mut self) -> Option<PackageInstall> {
        if let Some(ref mut self_updater) = self.self_updater {
            return self_updater.updated();
        }
        None
    }

    // TODO (CM): Really, this is now a "restart" operation.
    // Furthermore, we shouldn't really be doing a "restart" through
    // the Launcher anymore, should we?
    //
    // Is there cause to do a restart _without_ going through the
    // normal shutdown process? I don't _think_so.

    /// Walk each service and check if it has an updated package
    /// installed via the Update Strategy.
    ///
    /// Returns a Vec of futures for shutting down those services and
    /// subsequently queue them up for restart by the manager.
    ///
    /// (The futures need to be spawned.)
    fn shutdown_services_for_update(&mut self) -> Vec<impl Future<Item = (), Error = ()>> {
        // This code removes all Services that need to be restarted
        // for updates from the main list of services and returns
        // those Services for appropriate handling
        let services_to_restart = {
            let mut updater = self.updater.lock().expect("Updater lock poisoned");

            let mut working_set = HashMap::new();
            let mut state_services = self
                .state
                .services
                .write()
                .expect("Services lock is poisoned!");
            mem::swap(state_services.deref_mut(), &mut working_set);

            let (to_restart, mut no_action) =
                working_set.drain().partition(|(current_ident, service)| {
                    match updater.check_for_updated_package(&service, &self.census_ring) {
                        Some(new_package_ident) => {
                            outputln!("Updating from {} to {}", current_ident, new_package_ident);
                            true
                        }
                        None => {
                            trace!("No update found for {}", current_ident);
                            false
                        }
                    }
                });
            mem::swap(&mut no_action, state_services.deref_mut());

            to_restart
                .into_iter()
                .map(|(_ident, service)| service)
                .collect::<Vec<Service>>()
        };

        // At that point, we can just pass all the services to the
        // restart function
        //
        // Also, this is kind of duplicating the logic that the
        // ServiceOperation::Restart is trying to do. Might it be
        // better to translate these services into an operation
        // instead?
        //
        // Implies that Restart might need to take either a
        // ServiceSpec or a Service there, and the need to have a
        // "desired" ServiceSpec in there seems of less use.

        // Now we just turn each service into a Future that restarts
        // that service, and return them for spawning.
        services_to_restart
            .into_iter()
            .map(|service| {
                // TODO (CM): we'd need to make copies of all the
                // necessary Arcs here, if we're making
                // restart_service not require self.
                self.restart_service(service)
            }).collect()
    }

    // Note: `Service` should have already been removed from the
    // internal list of services under management.

    // TODO (CM): If this took the Service, UserConfigWatcher,
    // Updater, and specs_to_reload as arguments, it wouldn't need access to self

    fn restart_service(&self, service: Service) -> impl Future<Item = (), Error = ()> {
        let spec_file_path = service.spec_file.clone();

        // TODO (CM): the reason we can just use &self instead of &mut
        // self is because of this Arc.
        let spec_files = Arc::clone(&self.specs_to_reload);
        self
            .stop_service(service)
            // TODO (CM): should this be `then` rather than `and_then`?
            .and_then(move |_| {
                // NOTE: This serves to mark the service for restart
                // later.
                // In the future, the `stop_service` future could
                // return the Service, or a suitable proxy, to feed
                // directly into this, or the to-be-written future
                // that would start the service directly.
                //
                // The service to-be-started is going to be whatever
                // is defined in the spec file on disk at the time
                // that this gets resolved.
                spec_files
                    .lock()
                    .expect("specs_to_reload lock is poisoned")
                    .push(spec_file_path);
                Ok(())
            }).map_err(|e| {
                outputln!("Error shutting down service for update: {:?}", e);
            })
    }

    // Creates a rumor for the specified service.
    fn gossip_latest_service_rumor(&self, service: &Service) {
        let incarnation = if let Some(rumor) = self
            .butterfly
            .service_store
            .list
            .read()
            .expect("Rumor store lock poisoned")
            .get(&*service.service_group)
            .and_then(|r| r.get(&self.sys.member_id))
        {
            rumor.clone().incarnation + 1
        } else {
            1
        };

        self.butterfly.insert_service(service.to_rumor(incarnation));
    }

    fn check_for_departure(&self) -> bool {
        self.butterfly.is_departed()
    }

    fn check_for_changed_services(&mut self) -> bool {
        let mut service_states = HashMap::new();
        let mut active_services = Vec::new();
        for service in self
            .state
            .services
            .write()
            .expect("Services lock is poisoned!")
            .values_mut()
        {
            service_states.insert(service.spec_ident.clone(), service.last_state_change());
            active_services.push(service.spec_ident.clone());
        }

        for loaded in self
            .spec_dir
            .specs()
            .unwrap()
            .iter()
            .filter(|s| !active_services.contains(&s.ident))
        {
            service_states.insert(loaded.ident.clone(), Timespec::new(0, 0));
        }

        if service_states != self.service_states {
            self.service_states = service_states.clone();
            true
        } else {
            false
        }
    }

    fn persist_state(&self) {
        debug!("Updating census state");
        self.persist_census_state();
        debug!("Updating butterfly state");
        self.persist_butterfly_state();
        debug!("Updating services state");
        self.persist_services_state();
    }

    fn persist_census_state(&self) {
        let crp = CensusRingProxy::new(&self.census_ring);
        let json = serde_json::to_string(&crp).unwrap();
        self.state
            .gateway_state
            .write()
            .expect("GatewayState lock is poisoned")
            .census_data = json;
    }

    fn persist_butterfly_state(&self) {
        let bs = ServerProxy::new(&self.butterfly);
        let json = serde_json::to_string(&bs).unwrap();
        self.state
            .gateway_state
            .write()
            .expect("GatewayState lock is poisoned")
            .butterfly_data = json;
    }

    fn persist_services_state(&self) {
        let config_rendering = if feat::is_enabled(feat::RedactHTTP) {
            ConfigRendering::Redacted
        } else {
            ConfigRendering::Full
        };

        let services = self
            .state
            .services
            .read()
            .expect("Services lock is poisoned!");
        let existing_idents: Vec<PackageIdent> =
            services.values().map(|s| s.spec_ident.clone()).collect();

        // Services that are not active but are being watched for changes
        // These would include stopped persistent services or other
        // persistent services that failed to load
        let watched_services: Vec<Service> = self
            .spec_dir
            .specs()
            .unwrap()
            .iter()
            .filter(|spec| !existing_idents.contains(&spec.ident))
            .flat_map(|spec| {
                Service::load(
                    self.sys.clone(),
                    spec.clone(),
                    self.fs_cfg.clone(),
                    self.organization.as_ref().map(|org| &**org),
                    self.state.gateway_state.clone(),
                ).into_iter()
            }).collect();
        let watched_service_proxies: Vec<ServiceProxy> = watched_services
            .iter()
            .map(|s| ServiceProxy::new(s, config_rendering))
            .collect();
        let mut services_to_render: Vec<ServiceProxy> = services
            .values()
            .map(|s| ServiceProxy::new(s, config_rendering))
            .collect();

        services_to_render.extend(watched_service_proxies);

        let json = serde_json::to_string(&services_to_render).unwrap();
        self.state
            .gateway_state
            .write()
            .expect("GatewayState lock is poisoned")
            .services_data = json;
    }

    // TODO (CM): If this took the Service, UserConfigWatcher, and
    // Updater as arguments, it wouldn't need access to self

    // TODO (CM): Is there benefit of returning a SupError here, or not?

    /// Remove the given service from the manager.
    fn stop_service(&self, service: Service) -> impl Future<Item = (), Error = SupError> {
        // JW TODO: Update service rumor to remove service from cluster
        let user_config_watcher = Arc::clone(&self.user_config_watcher);
        let updater = Arc::clone(&self.updater);

        service
            .stop()
            // TODO (CM): Stop should emit a message about what's
            // being stopped.
            .then(move |_| {
                // We always want to do this cleanup, even if there
                // was an error shutting down the service
                if let Err(e) = user_config_watcher
                    .write()
                    .expect("Watcher lock poisoned")
                    .remove(&service)
                {
                    debug!(
                        "Error stopping user-config watcher thread for service {}: {:?}",
                        service, e
                    )
                }

                // Remove service updater
                updater
                    .lock()
                    .expect("Updater lock poisoned")
                    .remove(&service);
                Ok(())
            })
    }

    /// Check if any elections need restarting.
    fn restart_elections(&mut self) {
        self.butterfly.restart_elections();
    }

    // TODO (CM): Really, spec can be anything that provides an
    // identifier (FQ or otherwise?)
    fn remove_service_from_state(&mut self, spec: &ServiceSpec) -> Option<Service> {
        self.state
            .services
            .write()
            .expect("Services lock is poisoned")
            .remove(&spec.ident)
    }

    /// Start, stop, or restart services to bring what's running in
    /// line with what our spec files say.
    ///
    /// In the future, this will simply convert `ServiceOperation`s
    /// into futures that can be later spawned. Until starting of
    /// services is made asynchronous, however, it performs a mix of
    /// operations; starts are performed synchronously, while
    /// shutdowns and restarts are turned into futures.
    //
    // TODO: (CM) At that point, we may simply generate futures
    // directly, rather than going through a `ServiceOperation` as an
    // intermediary, though it may still be advantageous to keep
    // `ServiceOperation` around for testability and
    // separation-of-concern purposes. Restarting could be come
    // smarter, and we might be able to quietly rearrange our current
    // service metadata (e.g., binds) without having to restart, which
    // could argue for keeping `ServiceOperation`.)
    fn take_action_on_services(&mut self) -> Result<Vec<impl Future<Item = (), Error = ()>>> {
        // Internal implementation note here... we're using Either,
        // because that's how you can return two different types of
        // futures as the "same" future. Yes, we're returning "impl
        // Future"s, but the compiler still has to reconcile that to
        // one thing under the hood.
        //
        // There isn't yet an official "Either3" for us to use once adding
        // services is represented as a future. That would be easy to
        // implement, though, or we could fake it with some kind of
        // Either stack:
        //
        // (i.e., something like
        //   Either::A(first),
        //   Either::B(Either::A(second)),
        //   Either::B(Either::B(third))
        //
        // (though at that point, just implement Either3 and be done
        // with it).
        let mut futures = vec![];
        for op in self.reconcile_spec_files()? {
            match op {
                ServiceOperation::Stop(spec) => {
                    if let Some(service) = self.remove_service_from_state(&spec) {
                        let spec_ident = spec.ident.clone();
                        let f = self.stop_service(service).map_err(move |e| {
                            outputln!("Error shutting down {} for removal: {:?}", spec_ident, e);
                        });

                        futures.push(Either::A(f));
                    } else {
                        // TODO (CM): THIS SHOULD NEVER HAPPEN
                        outputln!(
                            "Tried to remove service for {} but could not find it running, skipping",
                            &spec.ident
                        );
                    }
                }
                ServiceOperation::Start(spec) => {
                    // Note: synchronous operation!  Once this is a
                    // future, it would end up being the
                    // "Either3::C(...)" variant alluded to above
                    self.add_service(spec);
                }
                ServiceOperation::Restart {
                    to_stop: running, ..
                } => {
                    if let Some(service) = self.remove_service_from_state(&running) {
                        let f = self.restart_service(service);
                        futures.push(Either::B(f));
                    } else {
                        // TODO (CM): THIS SHOULD NEVER HAPPEN
                    }
                }
            }
        }
        Ok(futures)
    }

    /// Determine what services we need to start, stop, or restart in
    /// order to be running what our on-disk spec files tell us we
    /// should be running.
    ///
    /// See `specs_to_operations` for the real logic.
    fn reconcile_spec_files(&mut self) -> Result<Vec<ServiceOperation>> {
        let services = self
            .state
            .services
            .read()
            .expect("Services lock is poisoned");
        let currently_running_specs = services.values().map(|s| s.to_spec());
        let on_disk_specs = self.spec_dir.specs()?;
        Ok(Self::specs_to_operations(
            currently_running_specs,
            on_disk_specs,
        ))
    }

    /// Pure utility function to generate a list of operations to
    /// perform to bring what's currently running with what _should_ be
    /// running, based on the current on-disk spec files.
    fn specs_to_operations<C, D>(
        currently_running_specs: C,
        on_disk_specs: D,
    ) -> Vec<ServiceOperation>
    where
        C: IntoIterator<Item = ServiceSpec>,
        D: IntoIterator<Item = ServiceSpec>,
    {
        let mut svc_states = HashMap::new();

        #[derive(Default)]
        struct ServiceState {
            running: Option<ServiceSpec>,
            disk: Option<(DesiredState, ServiceSpec)>,
        }

        for rs in currently_running_specs {
            svc_states.insert(
                rs.ident.clone(),
                ServiceState {
                    running: Some(rs),
                    disk: None,
                },
            );
        }

        for ds in on_disk_specs {
            let ident = ds.ident.clone();
            svc_states
                .entry(ident)
                .or_insert(ServiceState::default())
                .disk = Some((ds.desired_state, ds));
        }

        svc_states
            .into_iter()
            .filter_map(|(ident, ss)| match ss {
                ServiceState {
                    disk: Some((DesiredState::Up, disk_spec)),
                    running: None,
                } => {
                    debug!("Reconciliation: '{}' queued for start", ident);
                    Some(ServiceOperation::Start(disk_spec))
                }

                ServiceState {
                    disk: Some((DesiredState::Up, disk_spec)),
                    running: Some(running_spec),
                } => if running_spec == disk_spec {
                    debug!("Reconciliation: '{}' unchanged", ident);
                    None
                } else {
                    // TODO (CM): In the future, this would be the
                    // place where we can evaluate what has changed
                    // between the spec-on-disk and our in-memory
                    // representation and potentially just bring our
                    // in-memory representation in line without having
                    // to restart the entire service.
                    debug!("Reconciliation: '{}' queued for restart", ident);
                    Some(ServiceOperation::Restart {
                        to_stop: running_spec,
                        to_start: disk_spec,
                    })
                },

                ServiceState {
                    disk: Some((DesiredState::Down, _)),
                    running: Some(running_spec),
                } => {
                    debug!("Reconciliation: '{}' queued for stop", ident);
                    Some(ServiceOperation::Stop(running_spec))
                }

                ServiceState {
                    disk: Some((DesiredState::Down, _)),
                    running: None,
                } => {
                    debug!("Reconciliation: '{}' should be down, and is", ident);
                    None
                }

                ServiceState {
                    disk: None,
                    running: Some(running_spec),
                } => {
                    debug!("Reconciliation: '{}' queued for shutdown", ident);
                    Some(ServiceOperation::Stop(running_spec))
                }

                ServiceState {
                    disk: None,
                    running: None,
                } => unreachable!(),
            }).collect()
    }

    fn update_peers_from_watch_file(&mut self) -> Result<()> {
        if !self.butterfly.need_peer_seeding() {
            return Ok(());
        }
        match self.peer_watcher {
            None => Ok(()),
            Some(ref watcher) => {
                if watcher.has_fs_events() {
                    let members = watcher.get_members()?;
                    self.butterfly.member_list.set_initial_members(members);
                }
                Ok(())
            }
        }
    }

    fn update_running_services_from_user_config_watcher(&mut self) {
        let mut services = self
            .state
            .services
            .write()
            .expect("Services lock is poisoned");

        for service in services.values_mut() {
            if self
                .user_config_watcher
                .read()
                .expect("user_config_watcher lock is poisoned")
                .have_events_for(service)
            {
                outputln!("user.toml changes detected for {}", &service.spec_ident);
                service.user_config_updated = true;
            }
        }
    }
}

fn tls_config<A, B>(key_path: A, cert_path: B) -> Result<ServerConfig>
where
    A: AsRef<Path>,
    B: AsRef<Path>,
{
    let mut config = ServerConfig::new(NoClientAuth::new());
    let key_file = &mut BufReader::new(File::open(&key_path)?);
    let cert_file = &mut BufReader::new(File::open(&cert_path)?);

    // Note that we must explicitly map these errors because rustls returns () as the error from both
    // pemfile::certs() as well as pemfile::rsa_private_keys() and we want to return different errors
    // for each.
    let cert_chain = pemfile::certs(cert_file)
        .and_then(|c| if c.is_empty() { Err(()) } else { Ok(c) })
        .map_err(|_| sup_error!(Error::InvalidCertFile(cert_path.as_ref().to_path_buf())))?;

    let key = pemfile::rsa_private_keys(key_file)
        .and_then(|mut k| k.pop().ok_or(()))
        .map_err(|_| sup_error!(Error::InvalidKeyFile(key_path.as_ref().to_path_buf())))?;

    config.set_single_cert(cert_chain, key)?;
    config.ignore_client_order = true;
    Ok(config)
}

/// Represents how many threads to start for our main Tokio runtime
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq)]
struct TokioThreadCount(usize);

impl Default for TokioThreadCount {
    fn default() -> Self {
        // This is the same internal logic used in Tokio itself.
        // https://docs.rs/tokio/0.1.12/src/tokio/runtime/builder.rs.html#68
        TokioThreadCount(num_cpus::get().max(1))
    }
}

impl FromStr for TokioThreadCount {
    type Err = Error;
    fn from_str(s: &str) -> result::Result<Self, Self::Err> {
        let raw = s
            .parse::<usize>()
            .map_err(|_| Error::InvalidTokioThreadCount)?;
        if raw > 0 {
            Ok(TokioThreadCount(raw))
        } else {
            Err(Error::InvalidTokioThreadCount)
        }
    }
}

impl EnvConfig for TokioThreadCount {
    const ENVVAR: &'static str = "HAB_TOKIO_THREAD_COUNT";
}

impl Into<usize> for TokioThreadCount {
    fn into(self) -> usize {
        self.0
    }
}

#[derive(Debug)]
struct SuitabilityLookup(Arc<RwLock<HashMap<PackageIdent, Service>>>);

impl Suitability for SuitabilityLookup {
    fn get(&self, service_group: &str) -> u64 {
        self.0
            .read()
            .expect("Services lock is poisoned!")
            .values()
            .find(|s| *s.service_group == service_group)
            .and_then(|s| s.suitability())
            .unwrap_or(u64::min_value())
    }
}

fn obtain_process_lock(fs_cfg: &FsCfg) -> Result<()> {
    match write_process_lock(&fs_cfg.proc_lock_file) {
        Ok(()) => Ok(()),
        Err(_) => match read_process_lock(&fs_cfg.proc_lock_file) {
            Ok(pid) => {
                if process::is_alive(pid) {
                    return Err(sup_error!(Error::ProcessLocked(pid)));
                }
                release_process_lock(&fs_cfg);
                write_process_lock(&fs_cfg.proc_lock_file)
            }
            Err(SupError {
                err: Error::ProcessLockCorrupt,
                ..
            }) => {
                release_process_lock(&fs_cfg);
                write_process_lock(&fs_cfg.proc_lock_file)
            }
            Err(err) => Err(err),
        },
    }
}

fn read_process_lock<T>(lock_path: T) -> Result<Pid>
where
    T: AsRef<Path>,
{
    match File::open(lock_path.as_ref()) {
        Ok(file) => {
            let reader = BufReader::new(file);
            match reader.lines().next() {
                Some(Ok(line)) => match line.parse::<Pid>() {
                    Ok(pid) => Ok(pid),
                    Err(_) => Err(sup_error!(Error::ProcessLockCorrupt)),
                },
                _ => Err(sup_error!(Error::ProcessLockCorrupt)),
            }
        }
        Err(err) => Err(sup_error!(Error::ProcessLockIO(
            lock_path.as_ref().to_path_buf(),
            err
        ))),
    }
}

fn release_process_lock(fs_cfg: &FsCfg) {
    if let Err(err) = fs::remove_file(&fs_cfg.proc_lock_file) {
        debug!("Couldn't cleanup Supervisor process lock, {}", err);
    }
}

fn write_process_lock<T>(lock_path: T) -> Result<()>
where
    T: AsRef<Path>,
{
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(lock_path.as_ref())
    {
        Ok(mut file) => {
            let pid = match env::var(LAUNCHER_PID_ENV) {
                Ok(pid) => pid.parse::<Pid>().expect("Unable to parse launcher pid"),
                Err(_) => process::current_pid(),
            };
            match write!(&mut file, "{}", pid) {
                Ok(()) => Ok(()),
                Err(err) => Err(sup_error!(Error::ProcessLockIO(
                    lock_path.as_ref().to_path_buf(),
                    err
                ))),
            }
        }
        Err(err) => Err(sup_error!(Error::ProcessLockIO(
            lock_path.as_ref().to_path_buf(),
            err
        ))),
    }
}

struct CtlAcceptor {
    rx: ctl_gateway::server::MgrReceiver,
    state: Arc<ManagerState>,
    shutdown_trigger: oneshot::Receiver<()>,
}

impl CtlAcceptor {
    fn new(
        state: Arc<ManagerState>,
        rx: ctl_gateway::server::MgrReceiver,
        shutdown_trigger: oneshot::Receiver<()>,
    ) -> Self {
        CtlAcceptor {
            state: state,
            rx: rx,
            shutdown_trigger: shutdown_trigger,
        }
    }
}

impl Stream for CtlAcceptor {
    type Item = CtlHandler;
    type Error = ();

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        match self.shutdown_trigger.poll() {
            Ok(Async::Ready(())) => {
                info!("Signal received; stopping CtlAcceptor");
                Ok(Async::Ready(None))
            }
            Err(e) => {
                error!("Error polling CtlAcceptor shutdown trigger: {:?}", e);
                Ok(Async::Ready(None))
            }
            Ok(Async::NotReady) => match self.rx.poll() {
                Ok(Async::Ready(Some(cmd))) => {
                    let task = CtlHandler::new(cmd, self.state.clone());
                    Ok(Async::Ready(Some(task)))
                }
                Ok(Async::Ready(None)) => Ok(Async::Ready(None)),
                Ok(Async::NotReady) => Ok(Async::NotReady),
                Err(e) => {
                    debug!("CtlAcceptor error, {:?}", e);
                    Err(())
                }
            },
        }
    }
}

struct CtlHandler {
    cmd: ctl_gateway::server::CtlCommand,
    state: Arc<ManagerState>,
}

impl CtlHandler {
    fn new(cmd: ctl_gateway::server::CtlCommand, state: Arc<ManagerState>) -> Self {
        CtlHandler {
            cmd: cmd,
            state: state,
        }
    }
}

impl Future for CtlHandler {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.cmd.run(&self.state) {
            Ok(()) => (),
            Err(err) => {
                debug!("CtlHandler failed, {:?}", err);
                if self.cmd.req.transactional() {
                    self.cmd.req.reply_complete(err);
                }
            }
        }
        Ok(Async::Ready(()))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use protocol::STATE_PATH_PREFIX;
    use std::path::PathBuf;

    #[test]
    fn manager_state_path_default() {
        let cfg = ManagerConfig::default();
        let path = cfg.sup_root();

        assert_eq!(
            PathBuf::from(format!("{}/default", STATE_PATH_PREFIX.to_string_lossy())),
            path
        );
    }

    #[test]
    fn manager_state_path_custom() {
        let mut cfg = ManagerConfig::default();
        cfg.custom_state_path = Some(PathBuf::from("/tmp/peanuts-and-cake"));
        let path = cfg.sup_root();

        assert_eq!(PathBuf::from("/tmp/peanuts-and-cake"), path);
    }

    #[test]
    fn manager_state_path_custom_beats_name() {
        let mut cfg = ManagerConfig::default();
        cfg.custom_state_path = Some(PathBuf::from("/tmp/partay"));
        let path = cfg.sup_root();

        assert_eq!(PathBuf::from("/tmp/partay"), path);
    }

    mod tokio_thread_count {
        use super::*;

        locked_env_var!(HAB_TOKIO_THREAD_COUNT, lock_thread_count);

        #[test]
        fn default_is_number_of_cpus() {
            let tc = lock_thread_count();
            tc.unset();

            assert_eq!(TokioThreadCount::configured_value().0, num_cpus::get());
        }

        #[test]
        fn can_be_overridden_by_env_var() {
            let tc = lock_thread_count();
            tc.set("128");
            assert_eq!(TokioThreadCount::configured_value().0, 128);
        }

        #[test]
        fn cannot_be_overridden_to_zero() {
            let tc = lock_thread_count();
            tc.set("0");

            assert_ne!(TokioThreadCount::configured_value().0, 0);
            assert_eq!(TokioThreadCount::configured_value().0, num_cpus::get());
        }

    }

    mod specs_to_operations {
        //! Testing out the reconciliation of on-disk spec files with
        //! what is currently running.

        use super::super::*;

        /// Helper function for generating a basic spec from an
        /// identifier string
        fn new_spec(ident: &str) -> ServiceSpec {
            ServiceSpec::default_for(
                PackageIdent::from_str(ident).expect("couldn't parse ident str"),
            )
        }

        #[test]
        fn no_specs_yield_no_changes() {
            assert!(Manager::specs_to_operations(vec![], vec![]).is_empty());
        }

        /// If all the currently running services match all the
        /// current specs, we shouldn't have anything to change.
        #[test]
        fn identical_specs_yield_no_changes() {
            let specs = vec![new_spec("core/foo"), new_spec("core/bar")];
            assert!(Manager::specs_to_operations(specs.clone(), specs.clone()).is_empty());
        }

        #[test]
        fn missing_spec_on_disk_means_stop() {
            let running = vec![new_spec("core/foo")];
            let on_disk = vec![];

            let operations = Manager::specs_to_operations(running, on_disk);
            assert_eq!(operations.len(), 1);
            assert_eq!(operations[0], ServiceOperation::Stop(new_spec("core/foo")));
        }

        #[test]
        fn missing_active_spec_means_start() {
            let running = vec![];
            let on_disk = vec![new_spec("core/foo")];

            let operations = Manager::specs_to_operations(running, on_disk);
            assert_eq!(operations.len(), 1);
            assert_eq!(operations[0], ServiceOperation::Start(new_spec("core/foo")));
        }

        #[test]
        fn down_spec_on_disk_means_stop_running_service() {
            let spec = new_spec("core/foo");

            let running = vec![spec.clone()];

            let down_spec = {
                let mut s = spec.clone();
                s.desired_state = DesiredState::Down;
                s
            };

            let on_disk = vec![down_spec];

            let operations = Manager::specs_to_operations(running, on_disk);
            assert_eq!(operations.len(), 1);
            assert_eq!(operations[0], ServiceOperation::Stop(spec));
        }

        #[test]
        fn down_spec_on_disk_with_no_running_service_yields_no_changes() {
            let running = vec![];
            let down_spec = {
                let mut s = new_spec("core/foo");
                s.desired_state = DesiredState::Down;
                s
            };
            let on_disk = vec![down_spec];

            let operations = Manager::specs_to_operations(running, on_disk);
            assert!(operations.is_empty());
        }

        #[test]
        fn modified_spec_on_disk_means_restart() {
            let running_spec = new_spec("core/foo");

            let on_disk_spec = {
                let mut s = running_spec.clone();
                s.update_strategy = UpdateStrategy::AtOnce;
                s
            };
            assert_ne!(running_spec.update_strategy, on_disk_spec.update_strategy);

            let running = vec![running_spec];
            let on_disk = vec![on_disk_spec];

            let operations = Manager::specs_to_operations(running, on_disk);
            assert_eq!(operations.len(), 1);

            match operations[0] {
                ServiceOperation::Restart {
                    to_stop: ref old,
                    to_start: ref new,
                } => {
                    assert_eq!(old.ident, new.ident);
                    assert_eq!(old.update_strategy, UpdateStrategy::None);
                    assert_eq!(new.update_strategy, UpdateStrategy::AtOnce);
                }
                ref other => {
                    panic!("Should have been a restart operation: got {:?}", other);
                }
            }
        }

        #[test]
        fn multiple_operations_can_be_determined_at_once() {
            // Nothing should happen with this; it's already how it
            // needs to be.
            let svc_1_running = new_spec("core/foo");
            let svc_1_on_disk = svc_1_running.clone();

            // Should get shut down.
            let svc_2_running = new_spec("core/bar");
            let svc_2_on_disk = {
                let mut s = svc_2_running.clone();
                s.desired_state = DesiredState::Down;
                s
            };

            // Should get restarted.
            let svc_3_running = new_spec("core/baz");
            let svc_3_on_disk = {
                let mut s = svc_3_running.clone();
                s.update_strategy = UpdateStrategy::AtOnce;
                s
            };

            // Nothing should happen with this; it's already down.
            let svc_4_on_disk = {
                let mut s = new_spec("core/quux");
                s.desired_state = DesiredState::Down;
                s
            };

            // This should get started
            let svc_5_on_disk = new_spec("core/wat");

            // This should get shut down
            let svc_6_running = new_spec("core/lolwut");

            let running = vec![
                svc_1_running.clone(),
                svc_2_running.clone(),
                svc_3_running.clone(),
                svc_6_running.clone(),
            ];

            let on_disk = vec![
                svc_1_on_disk.clone(),
                svc_2_on_disk.clone(),
                svc_3_on_disk.clone(),
                svc_4_on_disk.clone(),
                svc_5_on_disk.clone(),
            ];

            let operations = Manager::specs_to_operations(running, on_disk);

            let expected_operations = vec![
                ServiceOperation::Stop(svc_2_running.clone()),
                ServiceOperation::Restart {
                    to_stop: svc_3_running.clone(),
                    to_start: svc_3_on_disk.clone(),
                },
                ServiceOperation::Start(svc_5_on_disk.clone()),
                ServiceOperation::Stop(svc_6_running.clone()),
            ];

            // Ideally, we'd just sort `operations` and
            // `expected_operations`, but we can't, since that would
            // mean we'd need a total ordering on `PackageIdent`,
            // which we can't do, since identifiers of different
            // packages (say, `core/foo` and `core/bar`) are not
            // comparable.
            //
            // Instead, we'll just do the verification one at a time.
            assert_eq!(
                operations.len(),
                expected_operations.len(),
                "Didn't generate the expected number of operations"
            );
            for op in expected_operations {
                assert!(
                    operations.contains(&op),
                    "Should have expected operation: {:?}",
                    op
                );
            }
        }
    }
}
