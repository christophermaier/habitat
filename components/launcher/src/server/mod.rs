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

mod handlers;

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::str::FromStr;
use std::thread;
use std::time::Duration;

use core;
use core::package::{PackageIdent, PackageInstall};
use core::os::process::{self, Pid, Signal};
use core::os::signals::{self, SignalEvent};
use ipc_channel::ipc::{IpcOneShotServer, IpcReceiver, IpcSender};
use protobuf;
use protocol::{self, ERR_NO_RETRY_EXCODE, OK_NO_RETRY_EXCODE};

use self::handlers::Handler;
use {SUP_CMD, SUP_PACKAGE_IDENT};
use error::{Error, Result};
use service::Service;

use time::SteadyTime;

// TODO (CM): This struct needs to be generic over Linux and Windows
use service::StoppingService;

const SUP_CMD_ENVVAR: &'static str = "HAB_SUP_BINARY";
static LOGKEY: &'static str = "SV";

type Receiver = IpcReceiver<Vec<u8>>;
type Sender = IpcSender<Vec<u8>>;

enum TickState {
    Continue,
    Exit(i32),
}

pub struct Server {
    services: ServiceTable,
    tx: Sender,
    rx: Receiver,
    supervisor: Child,
    args: Vec<String>,
}

impl Server {
    pub fn new(args: Vec<String>) -> Result<Self> {
        let ((rx, tx), supervisor) = Self::init(&args, false)?;
        Ok(Server {
            services: ServiceTable::default(),
            tx: tx,
            rx: rx,
            supervisor: supervisor,
            args: args,
        })
    }

    /// Spawn a Supervisor and setup a bi-directional IPC connection to it.
    ///
    /// Passing a value of true to the `clean` argument will force the Supervisor to clean the
    /// Launcher's process LOCK before starting. This is useful when restarting a Supervisor
    /// that terminated gracefully.
    fn init(args: &[String], clean: bool) -> Result<((Receiver, Sender), Child)> {
        let (server, pipe) = IpcOneShotServer::new().map_err(Error::OpenPipe)?;
        let supervisor = spawn_supervisor(&pipe, args, clean)?;
        let channel = setup_connection(server)?;
        Ok((channel, supervisor))
    }

    #[allow(unused_must_use)]
    fn reload(&mut self) -> Result<()> {
        self.supervisor.kill();
        self.supervisor.wait();
        let ((rx, tx), supervisor) = Self::init(&self.args, true)?;
        self.tx = tx;
        self.rx = rx;
        self.supervisor = supervisor;
        Ok(())
    }

    fn forward_signal(&self, signal: Signal) {
        if let Err(err) = core::os::process::signal(self.supervisor.id() as Pid, signal) {
            error!(
                "Unable to signal Supervisor, {}, {}",
                self.supervisor.id(),
                err
            );
        }
    }

    fn handle_message(&mut self) -> Result<TickState> {
        match self.rx.try_recv() {
            Ok(bytes) => {
                dispatch(&self.tx, &bytes, &mut self.services);
                Ok(TickState::Continue)
            }
            Err(_) => {
                match self.supervisor.try_wait() {
                    Ok(None) => Ok(TickState::Continue),
                    Ok(Some(status)) => {
                        debug!("Supervisor exited: {}", status);
                        match status.code() {
                            Some(ERR_NO_RETRY_EXCODE) => {
                                self.services.kill_all();
                                return Ok(TickState::Exit(ERR_NO_RETRY_EXCODE));
                            }
                            Some(OK_NO_RETRY_EXCODE) => {
                                self.services.kill_all();
                                return Ok(TickState::Exit(0));
                            }
                            _ => (),
                        }
                        Err(Error::SupShutdown)
                    }
                    Err(err) => {
                        warn!("Unable to wait for Supervisor, {}", err);
                        Err(Error::SupShutdown)
                    }
                }
            }
        }
    }

    fn reap_zombies(&mut self) {
        self.services.reap_zombies()
    }

    fn handle_stopping_services(&mut self) {
        self.services.handle_stopping_services()
    }

    fn shutdown(&mut self) {
        debug!("Shutting down...");
        if send(&self.tx, &protocol::Shutdown::new()).is_err() {
            warn!("Forcefully stopping Supervisor: {}", self.supervisor.id());
            if let Err(err) = self.supervisor.kill() {
                warn!(
                    "Unable to kill Supervisor, {}, {}",
                    self.supervisor.id(),
                    err
                );
            }
        }
        self.supervisor.wait().ok();
        self.services.kill_all();
        outputln!("Hasta la vista, services.");
    }

    fn tick(&mut self) -> Result<TickState> {
        self.reap_zombies();
        self.handle_stopping_services();

        match signals::check_for_signal() {
            Some(SignalEvent::Shutdown) => {
                self.shutdown();
                return Ok(TickState::Exit(0));
            }
            Some(SignalEvent::Passthrough(signal)) => self.forward_signal(signal),
            None => (),
        }
        self.handle_message()
    }
}

// TODO (CM): maybe wrap a Service with additional "stopping" metadata?

#[derive(Debug, Default)]
struct StoppingServices(HashMap<Pid, StoppingService>);

impl StoppingServices {
    /// Add a service to track.
    // TODO (CM): This should be *removed* from the ServiceTable when
    // it's added to this structure.
    fn add(&mut self, service: StoppingService) {
        self.0.insert(service.pid(), service);
    }

    /// Runs through the services that are queued for stopping and
    /// returns two Vectors; the first containing services that have
    /// exceeded their shutdown timeout, and the second containing
    /// services that have *not* exceeded their timeout, but have
    /// already stopped.
    ///
    /// Note that the services that have exceeded their timeout may or
    /// may not be running still; we don't check that here.
    ///
    /// The intention here is that all the stopped processes will have
    /// been waited on, and all the "to_kill" processes will need to
    /// be killed, and then wea
    fn i_like_my_butt(&mut self) -> (Vec<StoppingService>, Vec<StoppingService>) {

        if self.0.is_empty() {
            return (vec![], vec![]);
        }

        let mut to_kill = vec![];
        let mut stopped = vec![];

        let now = SteadyTime::now();

        for (pid, stopping_service) in self.0.iter_mut() {
            match stopping_service.kill_time() {
                Some(kill_time) => {
                    if kill_time <= now {
                        to_kill.push(pid.clone());
                    } else {
                        // not expired, but might have already exited

                        // TODO (CM): Need to augment StoppingService
                        // with the status, and set it here
                        if let Ok(Some(_status)) = stopping_service.try_wait() {
                            // TODO (CM): status is kept in stopping_service
                            stopped.push(pid.clone());
                        }
                    }
                }
                None => {
                    // "infinite" timeout; has it stopped?
                    if let Ok(Some(_status)) = stopping_service.try_wait() {
                        stopped.push(pid.clone());
                    }
                }
            }
        }

        // TODO (CM): can the unwrap be pulled into the collect, somehow?
        let to_kill = to_kill
            .into_iter()
            .map(|ref pid| self.0.remove(pid).unwrap())
            .collect();

        let stopped = stopped
            .into_iter()
            .map(|ref pid| self.0.remove(pid).unwrap())
            .collect();

        (to_kill, stopped)
    }
}

#[derive(Debug, Default)]
pub struct ServiceTable {
    live_services: HashMap<Pid, Service>,
    shutting_down: StoppingServices,
}

impl ServiceTable {
    // TODO (CM): consider renaming the "old" ServiceTable functions
    // to reflect the larger scope of the service table
    //
    // get, get_mut, insert, remove
    pub fn get(&self, pid: Pid) -> Option<&Service> {
        self.live_services.get(&pid)
    }

    pub fn get_mut(&mut self, pid: Pid) -> Option<&mut Service> {
        self.live_services.get_mut(&pid)
    }

    pub fn insert(&mut self, service: Service) {
        self.live_services.insert(service.id(), service);
    }

    pub fn remove(&mut self, pid: Pid) -> Option<Service> {
        self.live_services.remove(&pid)
    }






    pub fn register_stopping_service(&mut self, svc: StoppingService) {
        // TODO (CM): remove from live_services
        self.shutting_down.add(svc);
    }


    // TODO (CM): Ensure that kill_all goes through the stopping
    // services and kill them all.
    fn kill_all(&mut self) {
        for service in self.live_services.values_mut() {
            outputln!(preamble service.name(), "Stopping...");

            // TODO (CM): Hrmm... this is a synchronous call

            // TODO (CM): we can take the timer that's returned here
            // and handle things in our service table (thinking that's
            // where we stick things)
            let stopping_service = service.kill();
            // TODO (CM): INSERT THIS INTO A DATA STRUCTURE

            // TODO (CM): need to remove this pid from the ServiceTable


            // TODO (CM):  this output will be lifted to the
            // higher-level thing watching out for dying processes

            // Could try to do something along the lines of reap_zombies

            //outputln!(preamble service.name(), "Shutdown OK: {}", shutdown_method);
        }
    }

    // TODO (CM): rename / normalize name
    fn handle_stopping_services(&mut self) {
        let (to_kill, stopped) = self.shutting_down.i_like_my_butt();

        for kill_it in to_kill.into_iter() {
            // kill
        }

        for stopped_svc in stopped.into_iter() {
            // message, basically
        }
    }

    fn reap_zombies(&mut self) {
        let mut dead: Vec<Pid> = vec![];
        for service in self.live_services.values_mut() {
            match service.try_wait() {
                Ok(None) => (),
                Ok(Some(code)) => {
                    outputln!(
                        "Child for service '{}' with PID {} exited with code {}",
                        service.name(),
                        service.id(),
                        code
                    );
                    dead.push(service.id());
                }
                Err(err) => {
                    warn!("Error waiting for child, {}, {}", service.id(), err);
                    dead.push(service.id());
                }
            }
        }
        for pid in dead {
            self.live_services.remove(&pid);
        }
    }
}

////////////////////////
// Public Func
//

pub fn reply<T>(tx: &Sender, txn: &protocol::NetTxn, msg: &T) -> Result<()>
where
    T: protobuf::MessageStatic,
{
    let bytes = txn.build_reply(msg)
        .map_err(Error::Serialize)?
        .to_bytes()
        .map_err(Error::Serialize)?;
    tx.send(bytes).map_err(Error::Send)?;
    Ok(())
}

pub fn run(args: Vec<String>) -> Result<i32> {
    let mut server = Server::new(args)?;
    signals::init();
    loop {
        match server.tick() {
            Ok(TickState::Continue) => thread::sleep(Duration::from_millis(100)),
            Ok(TickState::Exit(code)) => {
                return Ok(code);
            }
            Err(_) => {
                while server.reload().is_err() {
                    thread::sleep(Duration::from_millis(1_000));
                }
            }
        }
    }
}

pub fn send<T>(tx: &Sender, msg: &T) -> Result<()>
where
    T: protobuf::MessageStatic,
{
    let bytes = protocol::NetTxn::build(msg)
        .map_err(Error::Serialize)?
        .to_bytes()
        .map_err(Error::Serialize)?;
    tx.send(bytes).map_err(Error::Send)?;
    Ok(())
}

////////////////////////
// Private Func
//

fn dispatch(tx: &Sender, bytes: &[u8], services: &mut ServiceTable) {
    let msg = match protocol::NetTxn::from_bytes(bytes) {
        Ok(msg) => msg,
        Err(err) => {
            error!("Unable to decode NetTxn from Supervisor, {}", err);
            return;
        }
    };
    let func = match msg.message_id() {
        "Restart" => handlers::RestartHandler::run,
        "Spawn" => handlers::SpawnHandler::run,
        "Terminate" => handlers::TerminateHandler::run,
        unknown => {
            warn!("Received unknown message from Supervisor, {}", unknown);
            return;
        }
    };
    func(tx, msg, services);
}

fn setup_connection(server: IpcOneShotServer<Vec<u8>>) -> Result<(Receiver, Sender)> {
    let (rx, raw) = server.accept().map_err(|_| Error::AcceptConn)?;
    let txn = protocol::NetTxn::from_bytes(&raw).map_err(
        Error::Deserialize,
    )?;
    let mut msg = txn.decode::<protocol::Register>().map_err(
        Error::Deserialize,
    )?;
    let tx = IpcSender::connect(msg.take_pipe()).map_err(Error::Connect)?;
    send(&tx, &protocol::NetOk::new())?;
    Ok((rx, tx))
}

/// Start a Supervisor as a child process.
///
/// Passing a value of true to the `clean` argument will force the Supervisor to clean the
/// Launcher's process LOCK before starting. This is useful when restarting a Supervisor
/// that terminated gracefully.
fn spawn_supervisor(pipe: &str, args: &[String], clean: bool) -> Result<Child> {
    let binary = supervisor_cmd()?;
    let mut command = Command::new(&binary);
    if clean {
        command.env(protocol::LAUNCHER_LOCK_CLEAN_ENV, clean.to_string());
    }
    debug!("Starting Supervisor...");
    let child = command
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .env(protocol::LAUNCHER_PIPE_ENV, pipe)
        .env(
            protocol::LAUNCHER_PID_ENV,
            process::current_pid().to_string(),
        )
        .args(args)
        .spawn()
        .map_err(Error::SupSpawn)?;
    Ok(child)
}

/// Determines the most viable Supervisor binary to run and returns a `PathBuf` to it.
///
/// Setting a filepath value to the `HAB_SUP_BINARY` env variable will force that binary to be used
/// instead.
fn supervisor_cmd() -> Result<PathBuf> {
    if let Ok(command) = core::env::var(SUP_CMD_ENVVAR) {
        return Ok(PathBuf::from(command));
    }
    let ident = PackageIdent::from_str(SUP_PACKAGE_IDENT).unwrap();
    match PackageInstall::load_at_least(&ident, None) {
        Ok(install) => {
            match core::fs::find_command_in_pkg(SUP_CMD, &install, "/") {
                Ok(Some(cmd)) => Ok(cmd),
                _ => Err(Error::SupBinaryNotFound),
            }
        }
        Err(_) => Err(Error::SupPackageNotFound),
    }
}
