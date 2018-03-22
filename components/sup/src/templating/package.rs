// Copyright (c) 2018 Chef Software Inc. and/or applicable contributors
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

use std::collections::HashMap;
use std::path::PathBuf;

use hcore::package::PackageIdent;
use manager::service::{Env, Pkg};

#[derive(Clone, Debug, Serialize)]
pub struct Package {
    pub ident: String,

    pub origin: String,
    pub name: String,
    pub version: String,
    pub release: String,

    // TODO (CM): makes no sense
    pub deps: Vec<PackageIdent>,
    pub env: Env,

    // TODO (CM): Should be Vec<uint>
    pub exposes: Vec<String>,

    pub exports: HashMap<String, String>,
    pub path: PathBuf,

    pub svc_path: PathBuf,
    pub svc_config_path: PathBuf,
    pub svc_data_path: PathBuf,
    pub svc_files_path: PathBuf,
    pub svc_static_path: PathBuf,
    pub svc_var_path: PathBuf,
    pub svc_pid_file: PathBuf,
    pub svc_run: PathBuf,

    pub svc_user: String,
    pub svc_group: String,
}

impl Package {
    pub fn from_pkg(pkg: &Pkg) -> Self {
        Package {
            ident: pkg.ident.to_string(),
            origin: pkg.origin.clone(),
            name: pkg.name.clone(),
            version: pkg.version.clone(),
            release: pkg.release.clone(),

            // NOTE: These are transitive deps
            deps: pkg.deps.clone(),
            env: pkg.env.clone(),

            exposes: pkg.exposes.clone(),
            exports: pkg.exports.clone(),
            path: pkg.path.clone(),

            svc_path: pkg.svc_path.clone(),
            svc_config_path: pkg.svc_config_path.clone(),
            svc_data_path: pkg.svc_data_path.clone(),
            svc_files_path: pkg.svc_files_path.clone(),
            svc_run: pkg.svc_path.clone(),
            svc_static_path: pkg.svc_static_path.clone(),
            svc_var_path: pkg.svc_var_path.clone(),
            svc_pid_file: pkg.svc_pid_file.clone(),
            svc_user: pkg.svc_user.clone(),
            svc_group: pkg.svc_group.clone(),
        }
    }
}
