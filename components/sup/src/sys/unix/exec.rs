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

use std::ffi::OsStr;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};

use sys::abilities;

use hcore::os;

use error::{Error, Result};
use manager::service::Pkg;

static LOGKEY: &'static str = "EX";

pub fn run<T, S>(path: S, pkg: &Pkg, _: Option<T>) -> Result<Child>
where
    T: ToString,
    S: AsRef<OsStr>,
{
    let mut cmd = Command::new(path);

    // TODO (CM): these need to be the UID/GID that we're running as,
    // not, the svc user / group....

    //
    //
    // Ugh, this is exactly the same logic that the launcher has...

    let uid;
    let gid;

    // TODO (CM): if we can set the process and group, then let's only
    // call uid() and gid() in that block; set up the cmd beforehand



    if abilities::can_set_process_user_and_group() {
        uid = os::users::get_uid_by_name(&pkg.svc_user).ok_or(sup_error!(
            Error::Permissions(format!(
                "No uid for user '{}' could be found",
                &pkg.svc_user
            ))
        ))?;
        gid = os::users::get_gid_by_name(&pkg.svc_group).ok_or(
            sup_error!(
                Error::Permissions(format!(
                    "No gid for group '{}' could be found",
                    &pkg.svc_group
                ))
            ),
        )?;
    } else {
        uid = os::users::get_effective_uid();
        gid = os::users::get_effective_gid().unwrap(); // // TODO (CM):
        // I know, it's bad
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .uid(uid)
        .gid(gid);
    for (key, val) in pkg.env.iter() {
        cmd.env(key, val);
    }

    let res = cmd.spawn();
    //    debug!(">>>>>>> res = {:?}", res);

    Ok(res?)
}
