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

//! Contents found under the "sys" key of the rendering context.

use std::result;

use serde::{Serialize, Serializer};
use serde::ser::SerializeMap;

use manager::Sys;

// TODO (CM): Consider renaming this to something like SupInfo, since
// it really captures information about how the Supervisor is invoked,
// and consider exposing it under a "sup" key

#[derive(Clone, Debug)]
pub struct SystemInfo<'a> {
    sys: &'a Sys
}

impl<'a> SystemInfo<'a> {
    pub fn from_sys(sys: &'a Sys) -> Self {
        SystemInfo{ sys }
    }
}

impl<'a> Serialize for SystemInfo<'a> {
    fn serialize<S>(&self, serializer: S) -> result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(10))?;

        map.serialize_entry("version", &self.sys.version)?;
        map.serialize_entry("member_id", &self.sys.member_id)?;
        map.serialize_entry("ip", &self.sys.ip)?;
        map.serialize_entry("hostname", &self.sys.hostname)?;
        map.serialize_entry("gossip_ip", &self.sys.gossip_ip)?;
        map.serialize_entry("gossip_port", &self.sys.gossip_port)?;
        map.serialize_entry("http_gateway_ip", &self.sys.http_gateway_ip)?;
        map.serialize_entry("http_gateway_port", &self.sys.http_gateway_port)?;

        // This key is to support the old legacy behavior
        map.serialize_entry("permanent", &self.sys.permanent)?;

        // This is what `permanent` should have been from the beginning
        map.serialize_entry("persistent", &self.sys.permanent)?;

        map.end()
    }
}
