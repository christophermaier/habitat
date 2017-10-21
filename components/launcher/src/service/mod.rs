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

pub mod lifecycle;


use std::fmt;
use std::io::{self, BufRead, BufReader, Read, Write};
#[cfg(unix)]
use std::process::{ChildStderr, ChildStdout, ExitStatus};
use std::thread;

use time::SteadyTime;

use ansi_term::Colour;
#[cfg(windows)]
use core::os::process::windows_child::{ChildStderr, ChildStdout, ExitStatus};
use core::os::process::Pid;
use protocol;

pub use sys::service::*;

pub use self::lifecycle::*;

use error::Result;

/// An OS-agnostic abstraction over Habitat services from the
/// Launcher's perspective
pub struct Service {
    /// The arguments used to spawn this service; can be used again to
    /// respawn the same service
    args: protocol::Spawn,

    /// The running process of this service
    process: Process,

    // TODO (CM): Consider pulling this out
    status: Option<ExitStatus>,
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
        }
    }

    pub fn args(&self) -> &protocol::Spawn {
        &self.args
    }

    /// Return the OS-specific process identifier for the service.
    pub fn id(&self) -> Pid {
        self.process.id()
    }

    pub fn shutdown(&mut self) -> StoppingService {}


    // TODO (CM): rewrite docs
    /// Attempt to gracefully terminate a proccess and then forcefully kill it after
    /// 8 seconds if it has not terminated.
    pub fn kill(&mut self) -> StoppingService {
        self.process.kill()
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
