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

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;
use std::result;

use serde::{Serialize, Serializer};
use serde::ser::SerializeMap;

use hcore::package::PackageIdent;
use manager::service::{Env, Pkg};

#[derive(Clone, Debug)]
pub struct Package<'a> {
    pub ident: Cow<'a, PackageIdent>,
    pub origin: Cow<'a, String>,
    pub name: Cow<'a, String>,
    pub version: Cow<'a, String>,
    pub release: Cow<'a, String>,
    pub deps: Cow<'a, Vec<PackageIdent>>,
    pub env: Cow<'a, Env>,
    pub exposes: Cow<'a, Vec<String>>,
    pub exports: Cow<'a, HashMap<String, String>>,
    // TODO (CM): Maybe Path instead of Cow<'a PathBuf>?
    pub path: Cow<'a, PathBuf>,
    pub svc_path: Cow<'a, PathBuf>,
    pub svc_config_path: Cow<'a, PathBuf>,
    pub svc_data_path: Cow<'a, PathBuf>,
    pub svc_files_path: Cow<'a, PathBuf>,
    pub svc_static_path: Cow<'a, PathBuf>,
    pub svc_var_path: Cow<'a, PathBuf>,
    pub svc_pid_file: Cow<'a, PathBuf>,
    pub svc_run: Cow<'a, PathBuf>,
    pub svc_user: Cow<'a, String>,
    pub svc_group: Cow<'a, String>,
}

impl<'a> Package<'a> {
    pub fn from_pkg(pkg: &'a Pkg) -> Self {
        Package {
            ident: Cow::Borrowed(&pkg.ident),
            // TODO (CM): have Pkg use FullyQualifiedPackageIdent, and
            // get origin, name, version, and release from it, rather
            // than storing each individually; I suspect that was just
            // for templating
            origin: Cow::Borrowed(&pkg.origin),
            name: Cow::Borrowed(&pkg.name),
            version: Cow::Borrowed(&pkg.version),
            release: Cow::Borrowed(&pkg.release),
            deps: Cow::Borrowed(&pkg.deps),
            env: Cow::Borrowed(&pkg.env),
            exposes: Cow::Borrowed(&pkg.exposes),
            exports: Cow::Borrowed(&pkg.exports),
            path: Cow::Borrowed(&pkg.path),
            svc_path: Cow::Borrowed(&pkg.svc_path),
            svc_config_path: Cow::Borrowed(&pkg.svc_config_path),
            svc_data_path: Cow::Borrowed(&pkg.svc_data_path),
            svc_files_path: Cow::Borrowed(&pkg.svc_files_path),
            svc_static_path: Cow::Borrowed(&pkg.svc_static_path),
            svc_var_path: Cow::Borrowed(&pkg.svc_var_path),
            svc_pid_file: Cow::Borrowed(&pkg.svc_pid_file),
            svc_run: Cow::Borrowed(&pkg.svc_run),
            svc_user: Cow::Borrowed(&pkg.svc_user),
            svc_group: Cow::Borrowed(&pkg.svc_group),
        }
    }
}

impl<'a> Serialize for Package<'a> {
    fn serialize<S>(&self, serializer: S) -> result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(20))?;

        map.serialize_entry("ident", &self.ident.to_string())?;

        // Break out the components of the identifier, for easy access
        // in templates
        map.serialize_entry("origin", &self.origin)?;
        map.serialize_entry("name", &self.name)?;
        map.serialize_entry("version", &self.version)?;
        map.serialize_entry("release", &self.release)?;

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
