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

use std::net::IpAddr;
use std::result;

use serde::{Serialize, Serializer};
use serde::ser::SerializeMap;

use manager::Sys;

// TODO (CM): Consider renaming this to something like SupInfo, since
// it really captures information about how the Supervisor is invoked,
// and consider exposing it under a "sup" key

#[derive(Clone, Debug)]
pub struct SystemInfo {
    pub version: String,
    pub member_id: String,
    pub ip: IpAddr,
    pub hostname: String,
    pub gossip_ip: IpAddr,
    pub gossip_port: u16,
    pub http_gateway_ip: IpAddr,
    pub http_gateway_port: u16,
    /// Whether or not the Supervisor was started as a persistent peer
    pub persistent: bool,
}

impl SystemInfo {
    pub fn from_sys(sys: &Sys) -> Self {
        SystemInfo{
            version: sys.version.clone(),
            member_id: sys.member_id.clone(),
            ip: sys.ip.clone(),
            hostname: sys.hostname.clone(),
            gossip_ip: sys.gossip_ip.clone(),
            gossip_port: sys.gossip_port,
            http_gateway_ip: sys.http_gateway_ip.clone(),
            http_gateway_port: sys.http_gateway_port,
            persistent: sys.permanent,
        }
    }
}

impl Serialize for SystemInfo {
    fn serialize<S>(&self, serializer: S) -> result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(10))?;

        map.serialize_entry("version", &self.version)?;
        map.serialize_entry("member_id", &self.member_id)?;
        map.serialize_entry("ip", &self.ip)?;
        map.serialize_entry("hostname", &self.hostname)?;
        map.serialize_entry("gossip_ip", &self.gossip_ip)?;
        map.serialize_entry("gossip_port", &self.gossip_port)?;
        map.serialize_entry("http_gateway_ip", &self.http_gateway_ip)?;
        map.serialize_entry("http_gateway_port", &self.http_gateway_port)?;

        // This key is to support the old legacy behavior
        map.serialize_entry("permanent", &self.persistent)?;
        // This is what `permanent` should have been from the beginning
        map.serialize_entry("persistent", &self.persistent)?;

        map.end()
    }
}
