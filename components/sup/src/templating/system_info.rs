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

use manager::Sys;

// TODO (CM): Consider renaming this to something like SupInfo, since
// it really captures information about how the Supervisor is invoked,
// and consider exposing it under a "sup" key

#[derive(Clone, Debug, Serialize)]
pub struct SystemInfo {
    pub version: String,
    pub member_id: String,

    pub ip: IpAddr,
    pub hostname: String,

    pub gossip_ip: IpAddr,
    pub gossip_port: u16,

    pub http_gateway_ip: IpAddr,
    pub http_gateway_port: u16,

    // Should really be "persistent" instead
    pub permanent: bool,
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
            permanent: sys.permanent,
        }
    }
}
