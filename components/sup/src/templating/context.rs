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
            system_info: SystemInfo::from_sys(sys),
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
// TODO (CM): move this to a separate module

#[derive(Clone, Debug)]
struct Svc<'a> {
    census_group: &'a CensusGroup,
    // TODO (CM): Only keeping this for group_name function above
    group: &'a str,
    // TODO (CM): this will need to be optional soon
    first: SvcMember<'a>,
}

impl<'a> Svc<'a> {
    fn new(census_group: &'a CensusGroup) -> Self {
        Svc {
            census_group: census_group,
            group: census_group.service_group.group(),

            first: select_first(census_group).expect("First should always be present on svc"),
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
        map.serialize_entry("election_is_no_quorum", &(self.census_group.election_status == ElectionStatus::ElectionNoQuorum))?;
        map.serialize_entry("election_is_finished", &(self.census_group.election_status == ElectionStatus::ElectionFinished))?;
        map.serialize_entry("update_election_is_running",  &(self.census_group.update_election_status == ElectionStatus::ElectionInProgress))?;
        map.serialize_entry("update_election_is_no_quorum", &(self.census_group.update_election_status == ElectionStatus::ElectionNoQuorum))?;
        map.serialize_entry("update_election_is_finished", &(self.census_group.update_election_status == ElectionStatus::ElectionFinished))?;

        map.serialize_entry("me", &SvcMember(self.census_group.me().expect("Missing 'me'")))?;
        map.serialize_entry("members", &self.census_group
                            .members()
                            .iter()
                            .map(|m| SvcMember(m))
                            .collect::<Vec<SvcMember<'a>>>())?;
        map.serialize_entry("leader", &self.census_group.leader().map(|m| SvcMember(m)))?;
        map.serialize_entry("first", &self.first)?;
        map.serialize_entry("update_leader", &self.census_group.update_leader().map(|m| SvcMember(m)))?;

        map.end()
    }
}

#[derive(Clone, Debug)]
pub struct SvcMember<'a>(&'a CensusMember);

impl<'a> Serialize for SvcMember<'a> {
    fn serialize<S>(&self, serializer: S) -> result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(24))?;

        map.serialize_entry("member_id", &self.0.member_id)?;

        // NOTE (CM): pkg is actually optional on CensusMember

        // TODO (CM): pkg is currently serialized as a map with
        // origin, name, version, and release keys. We should also add
        // another field (e.g. "pkg_ident"?) that exposes a single
        // string.
        //
        // We should also normalize this pattern across our templating data.

        // TODO (CM): assuming these are all meant to be Some and
        // fully-qualified once we get to this point, right?
        map.serialize_entry("pkg", &self.0.pkg)?;

        // TODO (CM): add entry for entire service_group name in a
        // single string
        map.serialize_entry("service", &self.0.service)?;
        map.serialize_entry("group", &self.0.group)?;
        map.serialize_entry("application", &self.0.application)?;
        map.serialize_entry("environment", &self.0.environment)?;
        map.serialize_entry("org", &self.0.org)?;

        // TODO (CM): we actually spell it correctly here... it's not "permanent"
        // TODO (CM): add an "is_persistent" field to make it clear it's a boolean
        map.serialize_entry("persistent", &self.0.persistent)?;
        // TODO (CM): add an "is_leader" field to make it clear it's a boolean
        map.serialize_entry("leader", &self.0.leader)?;
        // TODO (CM): is_follower
        map.serialize_entry("follower", &self.0.follower)?;
        // TODO (CM): is_update_leader
        map.serialize_entry("update_leader", &self.0.update_leader)?;
        // TODO (CM): is_update_follower
        map.serialize_entry("update_follower", &self.0.update_follower)?;

        map.serialize_entry("election_is_running", &self.0.election_is_running)?;
        map.serialize_entry("election_is_no_quorum", &self.0.election_is_no_quorum)?;
        map.serialize_entry("election_is_finished", &self.0.election_is_finished)?;
        map.serialize_entry("update_election_is_running",  &self.0.update_election_is_running)?;
        map.serialize_entry("update_election_is_no_quorum", &self.0.update_election_is_no_quorum)?;
        map.serialize_entry("update_election_is_finished", &self.0.update_election_is_finished)?;

        // TODO (CM): this is a SysInfo, not a Sys or
        // SystemInfo... ugh; NORMALIZE IT ALL
        map.serialize_entry("sys", &self.0.sys)?;

        // TODO (CM): ugh, these aren't public on
        // CensusMember... actually, why are they private, and nothing
        // else is?
        map.serialize_entry("alive", &self.0.alive())?;
        map.serialize_entry("suspect", &self.0.suspect())?;
        map.serialize_entry("confirmed", &self.0.confirmed())?;
        map.serialize_entry("departed", &self.0.departed())?;

        map.serialize_entry("cfg", &self.0.cfg)?;

        map.end()
    }
}


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
