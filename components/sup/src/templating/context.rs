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

use std::borrow::Cow;
use std::collections::HashMap;
use std::result;

use serde::{Serialize, Serializer};
use serde::ser::SerializeMap;
use toml;

use butterfly::rumor::service::SysInfo;
use hcore::service::ServiceGroup;
use hcore::package::PackageIdent;

use census::{CensusGroup, CensusMember, CensusRing, ElectionStatus, MemberId};
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
            members: group.members().iter().map(|m| SvcMember::from_census_member(m)).collect(),
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
        self.svc.service_group.group()
    }
}
// TODO (CM): move this to a separate module


// effectively a wrapper around a CensusGroup, for templating
#[derive(Clone, Debug)]
struct Svc<'a> {
    service_group: Cow<'a, ServiceGroup>,
    election_status: Cow<'a, ElectionStatus>,
    update_election_status: Cow<'a, ElectionStatus>,
    members: Cow<'a, Vec<&'a CensusMember>>,
    leader: Cow<'a, Option<&'a CensusMember>>,
    update_leader: Cow<'a, Option<&'a CensusMember>>,
    me: Cow<'a, CensusMember>,

    // TODO (CM): this will need to be optional soon
    first: SvcMember<'a>,
}

impl<'a> Svc<'a> {
    fn new(census_group: &'a CensusGroup) -> Self {
        Svc {
            service_group: Cow::Borrowed(&census_group.service_group),
            election_status: Cow::Borrowed(&census_group.election_status),
            update_election_status: Cow::Borrowed(&census_group.update_election_status),
            members: Cow::Owned(census_group.members()),
            me: Cow::Borrowed(&census_group.me().expect("Missing 'me'")),
            leader: Cow::Owned(census_group.leader()),
            update_leader: Cow::Owned(census_group.update_leader()),

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

        map.serialize_entry("service", &self.service_group.service())?;
        map.serialize_entry("group", &self.service_group.group())?;
        map.serialize_entry("org", &self.service_group.org())?;
        // TODO (CM): need to add application, environment, and
        // complete service_group as a string

        map.serialize_entry("election_is_running", &(self.election_status.as_ref() == &ElectionStatus::ElectionInProgress))?;
        map.serialize_entry("election_is_no_quorum", &(self.election_status.as_ref() == &ElectionStatus::ElectionNoQuorum))?;
        map.serialize_entry("election_is_finished", &(self.election_status.as_ref() == &ElectionStatus::ElectionFinished))?;
        map.serialize_entry("update_election_is_running",  &(self.update_election_status.as_ref() == &ElectionStatus::ElectionInProgress))?;
        map.serialize_entry("update_election_is_no_quorum", &(self.update_election_status.as_ref() == &ElectionStatus::ElectionNoQuorum))?;
        map.serialize_entry("update_election_is_finished", &(self.update_election_status.as_ref() == &ElectionStatus::ElectionFinished))?;

        map.serialize_entry("me", &SvcMember::from_census_member(&self.me))?;
        map.serialize_entry("members", &self.members
                            .iter()
                            .map(|m| SvcMember::from_census_member(m))
                            .collect::<Vec<SvcMember<'a>>>())?;
        map.serialize_entry("leader", &self.leader.map(|m| SvcMember::from_census_member(m)))?;
        map.serialize_entry("first", &self.first)?;
        map.serialize_entry("update_leader", &self.update_leader.map(|m| SvcMember::from_census_member(m)))?;

        map.end()
    }
}

#[derive(Clone, Debug)]
pub struct SvcMember<'a> {
    pub member_id: Cow<'a, MemberId>,
    pub pkg: Cow<'a, Option<PackageIdent>>,
    pub application: Cow<'a, Option<String>>,
    pub environment: Cow<'a, Option<String>>,
    pub service: Cow<'a, String>,
    pub group: Cow<'a, String>,
    pub org: Cow<'a, Option<String>>,
    pub persistent: bool,
    pub leader: bool,
    pub follower: bool,
    pub update_leader: bool,
    pub update_follower: bool,
    pub election_is_running: bool,
    pub election_is_no_quorum: bool,
    pub election_is_finished: bool,
    pub update_election_is_running: bool,
    pub update_election_is_no_quorum: bool,
    pub update_election_is_finished: bool,
    pub sys: Cow<'a, SysInfo>,
    pub alive: bool,
    pub suspect: bool,
    pub confirmed: bool,
    pub departed: bool,
    pub cfg: Cow<'a, toml::value::Table>
}

impl<'a> SvcMember<'a> {
    pub fn from_census_member(c: &'a CensusMember) -> Self {
        SvcMember {
            member_id: Cow::Borrowed(&c.member_id),
            pkg: Cow::Borrowed(&c.pkg),
            application: Cow::Borrowed(&c.application),
            environment: Cow::Borrowed(&c.environment),
            service: Cow::Borrowed(&c.service),
            group: Cow::Borrowed(&c.group),
            org: Cow::Borrowed(&c.org),
            persistent: c.persistent,
            leader: c.leader,
            follower: c.follower,
            update_leader: c.update_leader,
            update_follower: c.update_follower,
            election_is_running: c.election_is_running,
            election_is_no_quorum: c.election_is_no_quorum,
            election_is_finished: c.election_is_finished,
            update_election_is_running: c.update_election_is_running,
            update_election_is_no_quorum: c.update_election_is_no_quorum,
            update_election_is_finished: c.update_election_is_finished,

            // TODO (CM): unify this with other sys
            sys: Cow::Borrowed(&c.sys),
            alive: c.alive(),
            suspect: c.suspect(),
            confirmed: c.confirmed(),
            departed: c.departed(),
            cfg: Cow::Borrowed(&c.cfg),
        }

    }

}

impl<'a> Serialize for SvcMember<'a> {
    fn serialize<S>(&self, serializer: S) -> result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(24))?;

        map.serialize_entry("member_id", &self.member_id)?;

        // NOTE (CM): pkg is actually optional on CensusMember

        // TODO (CM): pkg is currently serialized as a map with
        // origin, name, version, and release keys. We should also add
        // another field (e.g. "pkg_ident"?) that exposes a single
        // string.
        //
        // We should also normalize this pattern across our templating data.

        // TODO (CM): assuming these are all meant to be Some and
        // fully-qualified once we get to this point, right?
        map.serialize_entry("pkg", &self.pkg)?;

        // TODO (CM): add entry for entire service_group name in a
        // single string
        map.serialize_entry("service", &self.service)?;
        map.serialize_entry("group", &self.group)?;
        map.serialize_entry("application", &self.application)?;
        map.serialize_entry("environment", &self.environment)?;
        map.serialize_entry("org", &self.org)?;

        // TODO (CM): we actually spell it correctly here... it's not "permanent"
        // TODO (CM): add an "is_persistent" field to make it clear it's a boolean
        map.serialize_entry("persistent", &self.persistent)?;
        // TODO (CM): add an "is_leader" field to make it clear it's a boolean
        map.serialize_entry("leader", &self.leader)?;
        // TODO (CM): is_follower
        map.serialize_entry("follower", &self.follower)?;
        // TODO (CM): is_update_leader
        map.serialize_entry("update_leader", &self.update_leader)?;
        // TODO (CM): is_update_follower
        map.serialize_entry("update_follower", &self.update_follower)?;

        map.serialize_entry("election_is_running", &self.election_is_running)?;
        map.serialize_entry("election_is_no_quorum", &self.election_is_no_quorum)?;
        map.serialize_entry("election_is_finished", &self.election_is_finished)?;
        map.serialize_entry("update_election_is_running",  &self.update_election_is_running)?;
        map.serialize_entry("update_election_is_no_quorum", &self.update_election_is_no_quorum)?;
        map.serialize_entry("update_election_is_finished", &self.update_election_is_finished)?;

        // TODO (CM): this is a SysInfo, not a Sys or
        // SystemInfo... ugh; NORMALIZE IT ALL
        map.serialize_entry("sys", &self.sys)?;

        // TODO (CM): ugh, these aren't public on
        // CensusMember... actually, why are they private, and nothing
        // else is?
        map.serialize_entry("alive", &self.alive)?;
        map.serialize_entry("suspect", &self.suspect)?;
        map.serialize_entry("confirmed", &self.confirmed)?;
        map.serialize_entry("departed", &self.departed)?;

        map.serialize_entry("cfg", &self.cfg)?;

        map.end()
    }
}


/// Helper for pulling the leader or first member from a census group. This is used to populate the
/// `.first` field in `bind` and `svc`.
fn select_first(census_group: &CensusGroup) -> Option<SvcMember> {
    match census_group.leader() {
        Some(member) => Some(SvcMember::from_census_member(member)),
        None => {
            census_group.members().first().and_then(
                |m| Some(SvcMember::from_census_member(m)),
            )
        }
    }
}
