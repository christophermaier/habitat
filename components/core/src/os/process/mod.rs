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

use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

// TODO (CM): Gross?!
use super::super::Error;

#[cfg(windows)]
pub mod windows_child;

#[allow(unused_variables)]
#[cfg(windows)]
#[path = "windows.rs"]
mod imp;

#[cfg(not(windows))]
#[path = "linux.rs"]
mod imp;

pub use self::imp::*;

pub trait OsSignal {
    fn os_signal(&self) -> SignalCode;
    fn from_signal_code(SignalCode) -> Option<Signal>;
}

#[allow(non_snake_case)]
#[derive(Clone, Copy, Debug)]
pub enum Signal {
    INT,
    ILL,
    ABRT,
    FPE,
    KILL,
    SEGV,
    TERM,
    HUP,
    QUIT,
    ALRM,
    USR1,
    USR2,
}

impl From<i32> for Signal {
    fn from(val: i32) -> Signal {
        match val {
            1 => Signal::HUP,
            2 => Signal::INT,
            3 => Signal::QUIT,
            4 => Signal::ILL,
            6 => Signal::ABRT,
            8 => Signal::FPE,
            9 => Signal::KILL,
            10 => Signal::USR1,
            11 => Signal::SEGV,
            12 => Signal::USR2,
            14 => Signal::ALRM,
            15 => Signal::TERM,
            _ => Signal::KILL,
        }
    }
}

impl From<Signal> for i32 {
    fn from(value: Signal) -> i32 {
        match value {
            Signal::HUP => 1,
            Signal::INT => 2,
            Signal::QUIT => 3,
            Signal::ILL => 4,
            Signal::ABRT => 6,
            Signal::FPE => 8,
            Signal::KILL => 9,
            Signal::USR1 => 10,
            Signal::SEGV => 11,
            Signal::USR2 => 12,
            Signal::ALRM => 14,
            Signal::TERM => 15,
        }
    }
}

impl FromStr for Signal {
    // Error type only needed to satisfy the trait; we don't actually
    // return an error type from this implementation.
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "ABRT" => Ok(Signal::ABRT),
            "ALRM" => Ok(Signal::ALRM),
            "FPE" => Ok(Signal::FPE),
            "HUP" => Ok(Signal::HUP),
            "ILL" => Ok(Signal::ILL),
            "INT" => Ok(Signal::INT),
            "KILL" => Ok(Signal::KILL),
            "QUIT" => Ok(Signal::QUIT),
            "SEGV" => Ok(Signal::SEGV),
            "TERM" => Ok(Signal::TERM),
            "USR1" => Ok(Signal::USR1),
            "USR2" => Ok(Signal::USR2),
            _ => Ok(Signal::KILL),
        }
    }
}

impl Display for Signal {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let s = match *self {
            Signal::ABRT => "ABRT",
            Signal::ALRM => "ALRM",
            Signal::FPE => "FPE",
            Signal::HUP => "HUP",
            Signal::ILL => "ILL",
            Signal::INT => "INT",
            Signal::KILL => "KILL",
            Signal::QUIT => "QUIT",
            Signal::SEGV => "SEGV",
            Signal::TERM => "TERM",
            Signal::USR1 => "USR1",
            Signal::USR2 => "USR2",
        };
        write!(f, "{}", s)
    }
}
