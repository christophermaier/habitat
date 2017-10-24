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

use std::fmt;
use std::io::{self, BufRead, BufReader, Read, Write};
#[cfg(unix)]
use std::process::{ChildStderr, ChildStdout, ExitStatus};
use std::thread;

use ansi_term::Colour;
#[cfg(windows)]
use core::os::process::windows_child::{ChildStderr, ChildStdout, ExitStatus};
use core::os::process::Pid;
use protocol;

pub use sys::service::*;
use error::Result;

use time::SteadyTime;


// TODO (CM): want a top-level module for this
//
// along with a Trait that defines how the ServiceTable interacts with
// these things.
//
// along with an enum over all types of services, which ServiceTable
// is implemented in terms of.

// TODO (CM): I don't like the naming of this, but it captures the
// concept. I'd prefer to not just have bool values flying around,
// since named Enum values are more self-describing.

pub enum ServiceStatus {
    Running,
    ShuttingDown,
    Restarting,

    // Starting?
}

impl Default for ServiceStatus {
    fn default() -> Self {
        ServiceStatus::Running
    }
}

pub struct Service {
    args: protocol::Spawn,
    process: Process,
    status: Option<ExitStatus>,

    current_status: ServiceStatus,
    kill_time: Option<SteadyTime>, // OK, this is when we don't have None == Infinite
}








impl Service {
    pub fn new(
        spawn: protocol::Spawn,
        process: Process,
        stdout: Option<ChildStdout>,
        stderr: Option<ChildStderr>,
    ) -> Self {
        if let Some(stdout) = stdout {
            let id = spawn.get_id().to_string();
            thread::Builder::new()
                .name(format!("{}-out", spawn.get_id()))
                .spawn(move || pipe_stdout(stdout, id))
                .ok();
        }
        if let Some(stderr) = stderr {
            let id = spawn.get_id().to_string();
            thread::Builder::new()
                .name(format!("{}-err", spawn.get_id()))
                .spawn(move || pipe_stderr(stderr, id))
                .ok();
        }
        Service {
            args: spawn,
            process: process,
            status: None,

            current_status: Default::default(),
            kill_time: None, // TODO (CM): really None, not Infinite
        }
    }

    pub fn args(&self) -> &protocol::Spawn {
        &self.args
    }

    pub fn id(&self) -> Pid {
        self.process.id()
    }

    // TODO (CM): rename to "shutdown"

    /// Attempt to gracefully terminate a proccess and then forcefully kill it after
    /// 8 seconds if it has not terminated.

    // TODO (CM): return shutdown time instead of ShutdownMethod, I think.


    // TODO (CM): or return nothing?

    pub fn kill(&mut self) {
        let kill_time = self.process.kill();
        self.kill_time = kill_time;
    }

    pub fn name(&self) -> &str {
        self.args.get_id()
    }

    pub fn take_args(self) -> protocol::Spawn {
        self.args
    }

    pub fn try_wait(&mut self) -> Result<Option<ExitStatus>> {
        self.process.try_wait()
    }

    pub fn wait(&mut self) -> Result<ExitStatus> {
        self.process.wait()
    }

    pub fn set_current_status(&mut self, status: ServiceStatus) {
        // TODO (CM): any kind of FSM-style logic we want to have
        // around this?
        self.current_status = status;
    }

    pub fn get_current_status(&self) {
        self.current_status
    }
}

impl fmt::Debug for Service {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Service {{ status: {:?}, pid: {:?} }}",
            self.status,
            self.process.id()
        )
    }
}

/// Consume output from a child process until EOF, then finish
fn pipe_stdout<T>(out: T, id: String)
where
    T: Read,
{
    let mut reader = BufReader::new(out);
    let mut buffer = String::new();
    while reader.read_line(&mut buffer).unwrap() > 0 {
        let mut line = output_format!(preamble &id, logkey "O");
        line.push_str(&buffer);
        write!(&mut io::stdout(), "{}", line).expect("unable to write to stdout");
        buffer.clear();
    }
}

/// Consume standard error from a child process until EOF, then finish
fn pipe_stderr<T>(err: T, id: String)
where
    T: Read,
{
    let mut reader = BufReader::new(err);
    let mut buffer = String::new();
    while reader.read_line(&mut buffer).unwrap() > 0 {
        let mut line = output_format!(preamble &id, logkey "E");
        let c = format!("{}", Colour::Red.bold().paint(buffer.clone()));
        line.push_str(c.as_str());
        write!(&mut io::stderr(), "{}", line).expect("unable to write to stderr");
        buffer.clear();
    }
}
