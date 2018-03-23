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

use std::collections::HashMap;
use std::result;

use serde::{Serialize, Serializer};
use serde::ser::SerializeMap;

use hcore::service::ServiceGroup;

use census::{CensusGroup, CensusMember, CensusRing, ElectionStatus};
use manager::Sys;
use manager::service::{Cfg, Pkg, ServiceBind};

use templating::system_info::SystemInfo;
use templating::package::Package;

#[derive(Clone, Debug, Serialize)]
pub struct Binds<'a>(HashMap<String, BindGroup<'a>>);

impl<'a> Binds<'a> {
    fn new<T>(bindings: T, census: &'a CensusRing) -> Self
    where
        T: Iterator<Item = &'a ServiceBind>,
    {
        let mut map = HashMap::default();
        for bind in bindings {
            if let Some(group) = census.census_group_for(&bind.service_group) {
                map.insert(bind.name.to_string(), BindGroup::new(group));
            }
        }
        Binds(map)
    }
}

// NOTE: This is exposed to users in templates. Any public member is
// accessible to users, so change this interface with care.
//
// User-facing documentation is available at
// https://www.habitat.sh/docs/reference/#template-data; update that
// as required.
#[derive(Clone, Debug, Serialize)]
pub struct BindGroup<'a> {
    pub first: Option<SvcMember<'a>>,
    pub members: Vec<SvcMember<'a>>,
}

impl<'a> BindGroup<'a> {
    fn new(group: &'a CensusGroup) -> Self {
        BindGroup {
            first: select_first(group),
            members: group.members().iter().map(|m| SvcMember(m)).collect(),
        }
    }
}

/// The context of a render call.
///
/// It stores information on a Service and its configuration.
///
/// NOTE: This is the entrypoint for all data available to users in
/// templates. Any public member _that is serialized_ is accessible to
/// users, so change this interface with care.
///
/// User-facing documentation is available at
/// https://www.habitat.sh/docs/reference/#template-data; update that
/// as required.
#[derive(Clone, Debug, Serialize)]
pub struct RenderContext<'a> {
    #[serde(rename = "sys")]
    pub system_info: SystemInfo<'a>,
    #[serde(rename = "pkg")]
    pub package: Package<'a>,
    pub cfg: &'a Cfg,
    svc: Svc<'a>,
    pub bind: Binds<'a>,
}

impl<'a> RenderContext<'a> {
    pub fn new<T>(
        service_group: &ServiceGroup,
        sys: &'a Sys,
        pkg: &'a Pkg,
        cfg: &'a Cfg,
        census: &'a CensusRing,
        bindings: T,
    ) -> RenderContext<'a>
    where
        T: Iterator<Item = &'a ServiceBind>,
    {
        let census_group = census.census_group_for(&service_group).expect(
            "Census Group missing from list!",
        );
        RenderContext {
            system_info: SystemInfo(sys),
            package: Package::from_pkg(pkg),
            cfg: cfg,
            svc: Svc::new(census_group),
            bind: Binds::new(bindings, census),
        }
    }

    // Exposed only for logging... can probably do this another way
    pub fn group_name(&self) -> &str {
        self.svc.group
    }
}

#[derive(Clone, Debug)]
struct Svc<'a> {
    census_group: &'a CensusGroup,

    election_is_no_quorum: bool,
    election_is_finished: bool,
    update_election_is_running: bool,
    update_election_is_no_quorum: bool,
    update_election_is_finished: bool,
    me: SvcMember<'a>,
    first: SvcMember<'a>,
    members: Vec<SvcMember<'a>>,
    leader: Option<SvcMember<'a>>,
    update_leader: Option<SvcMember<'a>>,
}

impl<'a> Svc<'a> {
    fn new(census_group: &'a CensusGroup) -> Self {
        Svc {
            census_group: census_group,

            election_is_no_quorum: census_group.election_status == ElectionStatus::ElectionNoQuorum,
            election_is_finished: census_group.election_status == ElectionStatus::ElectionFinished,
            update_election_is_running: census_group.election_status ==
                ElectionStatus::ElectionInProgress,
            update_election_is_no_quorum: census_group.election_status ==
                ElectionStatus::ElectionNoQuorum,
            update_election_is_finished: census_group.election_status ==
                ElectionStatus::ElectionFinished,
            me: SvcMember(census_group.me().expect("Missing 'me'")),
            members: census_group
                .members()
                .iter()
                .map(|m| SvcMember(m))
                .collect(),
            leader: census_group.leader().map(|m| SvcMember(m)),
            first: select_first(census_group).expect("First should always be present on svc"),
            update_leader: census_group.update_leader().map(|m| SvcMember(m)),
        }
    }
}

impl<'a> Serialize for Svc<'a> {
    fn serialize<S>(&self, serializer: S) -> result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(14))?;

        map.serialize_entry("service", &self.census_group.service_group.service())?;
        map.serialize_entry("group", &self.census_group.service_group.group())?;
        map.serialize_entry("org", &self.census_group.service_group.org())?;

        map.serialize_entry("election_is_running", &(self.census_group.election_status == ElectionStatus::ElectionInProgress))?;
        map.serialize_entry("election_is_no_quorum", &self.election_is_no_quorum)?;
        map.serialize_entry("election_is_finished", &self.election_is_finished)?;
        map.serialize_entry("update_election_is_running", &self.update_election_is_running)?;
        map.serialize_entry("update_election_is_no_quorum", &self.update_election_is_no_quorum)?;
        map.serialize_entry("update_election_is_finished", &self.update_election_is_finished)?;

        map.serialize_entry("me", &self.me)?;
        map.serialize_entry("members", &self.members)?;
        map.serialize_entry("leader", &self.leader)?;
        map.serialize_entry("first", &self.first)?;
        map.serialize_entry("update_leader", &self.update_leader)?;

        map.end()
    }
}

// NOTE: This is exposed to users in templates. Any public member is
// accessible to users, so change this interface with care.
//
// User-facing documentation is available at
// https://www.habitat.sh/docs/reference/#template-data; update that
// as required.
/// A friendly representation of a `CensusMember` to the templating system.
#[derive(Clone, Debug, Serialize)]
pub struct SvcMember<'a>(&'a CensusMember);

/// Helper for pulling the leader or first member from a census group. This is used to populate the
/// `.first` field in `bind` and `svc`.
fn select_first(census_group: &CensusGroup) -> Option<SvcMember> {
    match census_group.leader() {
        Some(member) => Some(SvcMember(member)),
        None => {
            census_group.members().first().and_then(
                |m| Some(SvcMember(m)),
            )
        }
    }
}
