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

use std::path::Path;

use ansi_term::Colour::Yellow;
use common;
use common::ui::UI;
use depot_client::Client;
use hcore::fs::{self, FS_ROOT_PATH};
use hcore::package::{PackageIdent, PackageInstall};

use {PRODUCT, VERSION};
use error::Result;
use manager::ServiceSpec;
use manager::service::UpdateStrategy;

static LOGKEY: &'static str = "PK";

pub fn install(
    ui: &mut UI,
    url: &str,
    ident: &PackageIdent,
    channel: Option<&str>,
) -> Result<PackageInstall> {
    let fs_root_path = Path::new(&*FS_ROOT_PATH);
    let installed_ident = common::command::package::install::start(
        ui,
        url,
        channel,
        &ident.to_string(),
        PRODUCT,
        VERSION,
        fs_root_path,
        &fs::cache_artifact_path(None::<String>),
        false,
    )?;
    Ok(PackageInstall::load(&installed_ident, Some(&fs_root_path))?)
}

// TODO (CM): This is a temporary function (halfway through a
// refactor, at this point!)
pub fn install_v2(
    ui: &mut UI,
    url: &str,
    ident: &str, // TODO (CM): or an enum of PackageIdent, or path
    channel: &str,
) -> Result<PackageInstall> {
    let fs_root_path = Path::new(&*FS_ROOT_PATH);
    let installed_ident = common::command::package::install::start(
        ui,
        url,
        // We currently need this to be an option due to how the depot
        // client is written. Anything that calls the current
        // function, though, should always have a channel. We should
        // push this "Option-ness" as far down the stack as we can,
        // with the ultimate goal of eliminating it altogether.
        Some(channel),
        &ident,
        PRODUCT,
        VERSION,
        fs_root_path,
        &fs::cache_artifact_path(None::<String>),
        false,
    )?;

    // TODO (CM): Ideally, we'd just return the result of
    // PackageInstall::load, but the error type isn't working out
    // correctly. Look into that!
    Ok(PackageInstall::load(&installed_ident, Some(&fs_root_path))?)
}

pub fn install_from_spec(ui: &mut UI, spec: &ServiceSpec) -> Result<PackageInstall> {
    Ok(install(
        ui,
        spec.bldr_url.as_str(),
        &spec.ident,
        Some(&spec.channel),
    )?)
}
