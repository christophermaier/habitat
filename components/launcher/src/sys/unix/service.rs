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

pub struct Process {
    pid: pid_t,
    status: Option<ExitStatus>,
}

impl Process {
    fn new(pid: u32) -> Self {
        Process {
            pid: pid as pid_t,
            status: None,
        }
    }

    pub fn id(&self) -> Pid {
        self.pid
    }

    // TODO (CM): rename to "shutdown"
    // TODO (CM): we also need a straigh-up "kill" command, though

    /// Return the time beyond which we need to send SIGKILL to this
    /// process (None if we wait indefinitely)
    pub fn kill(&mut self) -> Option<SteadyTime> {
        let pid_to_kill = self.pid_to_signal();

        // JW TODO: Determine if the error represents a case where the process was already
        // exited before we return out and assume so.

        // TODO (CM): what to do with this Result?
        signal(pid_to_kill, Signal::TERM);

        // if signal(pid_to_kill, Signal::TERM).is_err() {
        //     return ShutdownMethod::AlreadyExited;
        // }
        let stop_time = SteadyTime::now() + Duration::seconds(8);
        // loop {
        //     if let Ok(Some(_status)) = self.try_wait() {
        //         return ShutdownMethod::GracefulTermination;
        //     }
        //     if SteadyTime::now() < stop_time {
        //         continue;
        //     }
        //     // JW TODO: Determine if the error represents a case where the process was already
        //     // exited before we return out and assume so.
        //     if signal(pid_to_kill, Signal::KILL).is_err() {
        //         return ShutdownMethod::GracefulTermination;
        //     }
        //     return ShutdownMethod::Killed;
        // }

        Some(stop_time)

    }

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

    /// When shutting down or killing a process, determine which PID
    /// we actually need to signal. If our PID is equal to the process
    /// group ID, then we will use the *negative* of the PID to send
    /// the signal to the entire group instead. This prevents orphaned
    /// processes.
    fn pid_to_signal(&self) -> Pid {
        let target_pid = self.pid;

        let pgid = unsafe { libc::getpgid(target_pid) };
        if target_pid == pgid {
            debug!(
                "PID to kill {} is the process group root. Sending signal to process group instead",
                target_pid
            );
            target_pid.neg()
        } else {
            target_pid
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
    let process = Process::new(child.id());
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
