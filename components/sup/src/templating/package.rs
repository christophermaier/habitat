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
use std::result;

use serde::{Serialize, Serializer};
use serde::ser::SerializeMap;

use hcore::package::{Identifiable, PackageIdent};
use manager::service::{Env, Pkg};

#[derive(Clone, Debug)]
pub struct Package {
    pub ident: PackageIdent,

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
            ident: pkg.ident.clone(),

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

impl Serialize for Package {
    fn serialize<S>(&self, serializer: S) -> result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(20))?;

        map.serialize_entry("ident", &self.ident.to_string())?;

        // Break out the components of the identifier, for easy access
        // in templates
        map.serialize_entry("origin", &self.ident.origin())?;
        map.serialize_entry("name", &self.ident.name())?;
        map.serialize_entry("version", &self.ident.version().expect("ident should always have a version here"))?;
        map.serialize_entry("release", &self.ident.release().expect("ident shouls always have a release here"))?;

        map.serialize_entry("deps", &self.deps)?;
        map.serialize_entry("env", &self.env)?;

        map.serialize_entry("exposes", &self.exposes)?;
        map.serialize_entry("exports", &self.exports)?;
        map.serialize_entry("path", &self.path)?;

        map.serialize_entry("svc_path", &self.svc_path)?;
        map.serialize_entry("svc_config_path", &self.svc_config_path)?;
        map.serialize_entry("svc_data_path", &self.svc_data_path)?;
        map.serialize_entry("svc_files_path", &self.svc_files_path)?;
        map.serialize_entry("svc_static_path", &self.svc_static_path)?;
        map.serialize_entry("svc_var_path", &self.svc_var_path)?;
        map.serialize_entry("svc_pid_file", &self.svc_pid_file)?;
        map.serialize_entry("svc_run", &self.svc_run)?;
        map.serialize_entry("svc_user", &self.svc_user)?;
        map.serialize_entry("svc_group", &self.svc_group)?;

        map.end()
    }
}
