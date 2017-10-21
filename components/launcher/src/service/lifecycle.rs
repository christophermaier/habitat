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

//! Manage the lifecycle of a service, from the Launcher's perspective

use std::process::ExitStatus;

use core::os::process::Pid;
use error::{Error, Result};
use service::Service;
use sys::service::Process;

use time::SteadyTime;

pub struct LiveService(Service);
// TODO (CM): Maybe Service is called LiveService instead?


// TO GET FROM a service to a Stopping service, something needs to
// invoke kill (to be named 'shutdown') on the Process, which should
// return a kill time, at which point, we can create a StoppingService





// TODO (CM): OK, I think I want to have an enum over the following
// states:

// "normal" / running service
// shutting-down service (timed)
// shutting-down service (infinite)
// restarting-service (timed)
// restarting-service (infinite)

// Each would be their own type
//
// Having functions to move between states would be nice, too... FSM!

#[derive(Debug, Eq, PartialEq)]
pub struct StoppingService {
    /// The process!
    os_process: Process,

    status: Option<ExitStatus>,

    /// The time at which this service should be killed if it's not
    /// already terminated.
    ///
    /// None means an infinite timeout.
    kill_time: Option<SteadyTime>,
}

impl StoppingService {
    pub fn new(process: Process, kill_time: Option<SteadyTime>) -> Self {
        StoppingService {
            os_process: process,
            status: None,
            kill_time: kill_time,
        }
    }

    pub fn pid(&self) -> Pid {
        self.os_process.pid
    }

    pub fn status(&self) -> Option<ExitStatus> {
        self.status
    }

    pub fn kill_time(&self) -> Option<SteadyTime> {
        self.kill_time
    }

    // TODO (CM): Add kill logic here?

    pub fn try_wait(&mut self) -> Result<Option<ExitStatus>> {
        self.os_process.try_wait()
    }

    // TODO (CM): Add functions for how long the service has been stopping?
}
