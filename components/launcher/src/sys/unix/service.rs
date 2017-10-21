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

use std::io;
use std::ops::Neg;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::{Command, ExitStatus, Stdio};
use std::result;

use core::os;
use core::os::process::{Pid, signal, Signal};
use libc::{self, c_int, pid_t};
use protocol::{self, ShutdownMethod};
use time::{Duration, SteadyTime};

use error::{Error, Result};
use service::Service;
use service::lifecycle::StoppingService;

pub struct Process {
    pid: pid_t,
    status: Option<ExitStatus>,
    shutdown_signal: String,
    shutdown_timeout: Option<i64>,
}

impl Process {
    fn new(pid: u32, shutdown_signal: String, shutdown_timeout: Option<i64>) -> Self {
        Process {
            pid: pid as pid_t,
            status: None,
            shutdown_signal: shutdown_signal,
            shutdown_timeout: shutdown_timeout,
        }
    }

    pub fn id(&self) -> Pid {
        self.pid
    }

    // TODO (CM): This function is currently a synchronous one,
    // meaning it won't return until the process has been killed. If
    // we're going to allow for arbitrary timeouts, though, then we
    // can't do that anymore. We'll need to enqueue this in some
    // higher-level data structure and check that.
    //
    // Thus, we'll want to return some kind of handle to this process
    // (the pid, likely), as well as a timing indicator (perhaps the
    // time beyond which a still-live process should be killed).
    //
    // We'll want to have something else looking at this datastructure
    // each tick through the loop. If the process has already stopped,
    // yay! If not, and the timer has expired, kill it. In both cases,
    // remove the data from the cache.
    //
    // Also do this for windows code.

    // TODO (CM): rewrite docs

    // TODO (CM): rename 'kill' to 'shutdown'; make 'kill' a targeted function


    /// Attempt to gracefully terminate a proccess and then forcefully kill it after
    /// 8 seconds if it has not terminated.
    pub fn kill(&mut self) -> StoppingService {
        let pid_to_kill = self.pid_to_signal();

        // The FromStr implementation for Signal doesn't actually
        // throw an error, so this unwrap() call is safe.
        //
        // TODO (CM): We should have parsed this before we got down
        // this far
        let shutdown_signal: Signal = self.shutdown_signal.parse().unwrap();

        // JW TODO: Determine if the error represents a case where the process was already
        // exited before we return out and assume so.
        if signal(pid_to_kill, shutdown_signal).is_err() {
            // TODO (CM): really not sure about this... just temporary
            // until we have everything else in place. In reality,
            // this logic likely moves up into the main loop when
            // handling this stuff.

            return StoppingService::new(self.pid, Some(SteadyTime::now()));
            // return ShutdownMethod::AlreadyExited;
        }

        let kill_time = match self.shutdown_timeout {
            Some(timeout) => Some(SteadyTime::now() + Duration::milliseconds(timeout)),
            None => None, // == infinity
        };

        StoppingService::new(self.pid, kill_time)

        // if let Some(timeout) = self.shutdown_timeout {

        //     // TODO (CM): Find a better way to do this... ideally, we
        //     // want some kind of sleep until we get a signal that the
        //     // process has shut down.
        //     //
        //     // In the meantime, we can write tests for what we have here.

        //     let stop_time = SteadyTime::now() + Duration::milliseconds(timeout);

        //     loop {
        //         if let Ok(Some(_status)) = self.try_wait() {
        //             return ShutdownMethod::GracefulTermination;
        //         }
        //         if SteadyTime::now() < stop_time {
        //             continue;
        //         }
        //         // JW TODO: Determine if the error represents a case where the process was already
        //         // exited before we return out and assume so.
        //         if signal(pid_to_kill, Signal::KILL).is_err() {
        //             return ShutdownMethod::GracefulTermination;
        //         }
        //         return ShutdownMethod::Killed;
        //     }
        // } else {
        //     // None case == infinite timeout!
        //     // TODO (CM): this is WRONG WRONG WRONG; just doing it so
        //     // the code will compile at the moment
        //     return ShutdownMethod::Killed;
        // }
    }

    /// When shutting down or killing a process, determine which PID we actually
    /// need to signal. If our PID is equal to the process group ID,
    /// then we will use the *negative* of the PID to send the signal
    /// to the entire group instead. This prevents orphaned processes.
    fn pid_to_signal(&self) -> Pid {
        let pgid = unsafe { libc::getpgid(self.pid) };
        if self.pid == pgid {
            debug!(
                "PID to kill {} is the process group root. Sending signal to process group instead",
                self.pid
            );
            self.pid.neg()
        } else {
            self.pid
        }
    }

    // TODO (CM): rename kill to shutdown
    /// No, really... KILL
    // TODO (CM): return value?
    // TODO (CM): should there be an enum for Stoppingservice and
    // this? Make the interface uniform? Do we only kill shutting-down services?
    pub fn kill_kill_kill(&mut self) {
        signal(self.pid_to_signal(), Signal::KILL);
    }

    // TODO (CM): Aaaaugh, this is still needed for reap_zombies

    pub fn try_wait(&mut self) -> Result<Option<ExitStatus>> {
        if let Some(status) = self.status {
            return Ok(Some(status));
        }
        let mut status = 0 as c_int;
        match unsafe { libc::waitpid(self.pid, &mut status, libc::WNOHANG) } {
            0 => Ok(None),
            -1 => Err(Error::ExecWait(io::Error::last_os_error())),
            _ => {
                self.status = Some(ExitStatus::from_raw(status));
                Ok(Some(ExitStatus::from_raw(status)))
            }
        }
    }

    pub fn wait(&mut self) -> Result<ExitStatus> {
        if let Some(status) = self.status {
            return Ok(status);
        }
        let mut status = 0 as c_int;
        match unsafe { libc::waitpid(self.pid, &mut status, 0) } {
            -1 => Err(Error::ExecWait(io::Error::last_os_error())),
            _ => {
                self.status = Some(ExitStatus::from_raw(status));
                Ok(ExitStatus::from_raw(status))
            }
        }
    }
}

pub fn run(msg: protocol::Spawn) -> Result<Service> {
    debug!("launcher is spawning {}", msg.get_binary());
    let mut cmd = Command::new(msg.get_binary());
    let uid = os::users::get_uid_by_name(msg.get_svc_user()).ok_or(
        Error::UserNotFound(msg.get_svc_user().to_string()),
    )?;
    let gid = os::users::get_gid_by_name(msg.get_svc_group()).ok_or(
        Error::GroupNotFound(msg.get_svc_group().to_string()),
    )?;
    cmd.before_exec(owned_pgid);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .uid(uid)
        .gid(gid);
    for (key, val) in msg.get_env().iter() {
        cmd.env(key, val);
    }
    let child = cmd.spawn().map_err(Error::Spawn)?;

    let timeout = if msg.has_svc_shutdown_timeout() {
        Some(msg.get_svc_shutdown_timeout())
    } else {
        None
    };

    let process = Process::new(
        child.id(),
        msg.get_svc_shutdown_signal().to_string(), // TODO (CM):
        // doesn't really need to be a string... mabye just convert to
        // a signal here
        timeout,
    );
    Ok(Service::new(msg, process, child.stdout, child.stderr))
}

// we want the command to spawn processes in their own process group
// and not the same group as the Launcher. Otherwise if a child process
// sends SIGTERM to the group, the Launcher could be terminated.
fn owned_pgid() -> result::Result<(), io::Error> {
    unsafe {
        libc::setpgid(0, 0);
    }
    Ok(())
}
