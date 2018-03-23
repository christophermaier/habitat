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

use std::result;

use serde::{Serialize, Serializer};
use serde::ser::SerializeMap;

use manager::service::Pkg;

#[derive(Clone, Debug)]
pub struct Package<'a> {
    pkg: &'a Pkg
}

impl<'a> Package<'a> {
    pub fn from_pkg(pkg: &'a Pkg) -> Self {
        Package { pkg }
    }
}

impl<'a> Serialize for Package<'a> {
    fn serialize<S>(&self, serializer: S) -> result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(20))?;

        map.serialize_entry("ident", &self.pkg.ident.to_string())?;

        // Break out the components of the identifier, for easy access
        // in templates
        map.serialize_entry("origin", &self.pkg.origin)?;
        map.serialize_entry("name", &self.pkg.name)?;
        map.serialize_entry("version", &self.pkg.version)?;
        map.serialize_entry("release", &self.pkg.release)?;

        map.serialize_entry("deps", &self.pkg.deps)?;
        map.serialize_entry("env", &self.pkg.env)?;

        map.serialize_entry("exposes", &self.pkg.exposes)?;
        map.serialize_entry("exports", &self.pkg.exports)?;
        map.serialize_entry("path", &self.pkg.path)?;

        map.serialize_entry("svc_path", &self.pkg.svc_path)?;
        map.serialize_entry("svc_config_path", &self.pkg.svc_config_path)?;
        map.serialize_entry("svc_data_path", &self.pkg.svc_data_path)?;
        map.serialize_entry("svc_files_path", &self.pkg.svc_files_path)?;
        map.serialize_entry("svc_static_path", &self.pkg.svc_static_path)?;
        map.serialize_entry("svc_var_path", &self.pkg.svc_var_path)?;
        map.serialize_entry("svc_pid_file", &self.pkg.svc_pid_file)?;
        map.serialize_entry("svc_run", &self.pkg.svc_run)?;
        map.serialize_entry("svc_user", &self.pkg.svc_user)?;
        map.serialize_entry("svc_group", &self.pkg.svc_group)?;

        map.end()
    }
}
