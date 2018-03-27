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
use butterfly::member::Health;
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

// TODO (CM): None of these fields need to be public; making them
// private may be nice to preserve information hiding. Tests will
// prove this out, though.

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
    members: Cow<'a, Vec<SvcMember<'a>>>,
    leader: Cow<'a, Option<SvcMember<'a>>>,
    update_leader: Cow<'a, Option<SvcMember<'a>>>,
    me: Cow<'a, SvcMember<'a>>,
    // TODO (CM): this will need to be optional soon
    first: SvcMember<'a>,
}

impl<'a> Svc<'a> {
    fn new(census_group: &'a CensusGroup) -> Self {
        Svc {
            service_group: Cow::Borrowed(&census_group.service_group),
            election_status: Cow::Borrowed(&census_group.election_status),
            update_election_status: Cow::Borrowed(&census_group.update_election_status),
            members: Cow::Owned(census_group.members()
                                .iter()
                                .map(|m| SvcMember::from_census_member(m))
                                .collect::<Vec<SvcMember<'a>>>()),


            me: Cow::Owned(census_group.me().map(|m| SvcMember::from_census_member(m)).expect("Missing 'me'")),



            leader: Cow::Owned(census_group.leader().map(|m| SvcMember::from_census_member(m))),
            update_leader: Cow::Owned(census_group.update_leader().map(|m| SvcMember::from_census_member(m))),
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

        map.serialize_entry("me", &self.me)?;
        map.serialize_entry("members", &self.members)?;
        map.serialize_entry("leader", &self.leader)?;
        map.serialize_entry("first", &self.first)?;
        map.serialize_entry("update_leader", &self.update_leader)?;

        map.end()
    }
}

// TODO (CM): move this to separate file


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

            alive: c.health == Health::Alive,
            suspect: c.health == Health::Suspect,
            confirmed: c.health == Health::Confirmed,
            departed: c.health == Health::Departed,

            cfg: Cow::Borrowed(&c.cfg),
        }

    }

    // #[cfg(test)]
    // pub constructor_for_test() -> Self {
    // }
}

impl<'a> Serialize for SvcMember<'a> {
    fn serialize<S>(&self, serializer: S) -> result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(24))?;

        map.serialize_entry("member_id", &self.member_id)?;

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

        // TODO (CM): add an "is_permanent" field to make it clear it's a boolean
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::io::Read;
    use json;
    use serde_json;
    use std::fs;
    use valico::json_schema;
    use std::path::PathBuf;
    use tempdir::TempDir;
    use manager::service::config::PackageConfigPaths;
    use std::net::IpAddr;
    use std::net::Ipv4Addr;
    use std::collections::BTreeMap;
    use hcore::package::PackageIdent;
    use manager::service::Cfg;
    use manager::service::Env;

    // pub struct Validator<'a> {
    //     scope: valico::json_schema::Scope,
    //     schema: valico::json_schema::schema::ScopedSchema<'a>
    // }

    // impl<'a> Validator<'a> {
    //     pub fn new() -> Self {
    //         // let mut schema = String::new();


    //         // fs::File::open(&Path::new("render_context_schema.json"))
    //         //     .expect("Cannot open schema file")
    //         //     .read_to_string(&mut schema)
    //         //     .expect("Could not read schema to string");


    //         let schema = include_str!("../../render_context_schema.json");
    //         println!(">>>>>>> schema! = {:?}", schema);

    //         let v: serde_json::Value = serde_json::from_str(&schema)
    //             .expect("Cannot parse schema as JSON");

    //         let mut scope = json_schema::scope::Scope::new();
    //         scope.compile(v.clone(), true).expect("Couldn't compile the schema!");

    //         let schema = scope.compile_and_return(v.clone(), false).expect("wut");

    //         // let v: serde_json::Value = serde_json::from_str(&schema)
    //         //     .expect("Cannot parse schema as JSON");
    //         // let compiled =
    //         //     json_schema::schema::compile(v,
    //         //                                  None,
    //         //                                  json_schema::schema::CompilationSettings::new(
    //         //                                      &json_schema::keywords::default(),
    //         //                                      true
    //         //                                  )
    //         //     )
    //         //     .expect("Cannot compile JSON schema");
    //         println!(">>>>>>> schema = {:?}", schema);


    //         Validator { scope: scope,
    //                     schema: schema }
    //     }

    //     // pub fn validate(&self, input: &serde_json::Value) -> json_schema::ValidationState {
    //     //     let scope = json_schema::scope::Scope::new();
    //     //     let scoped_schema = json_schema::schema::ScopedSchema::new(&scope, &self.schema);
    //     //     let result = scoped_schema.validate(&input);
    //     //     result
    //     // }

    //     pub fn validate_string(&self, input: &str) -> json_schema::ValidationState {
    //         let serde_value: serde_json::Value = serde_json::from_str(input).unwrap();
    //         self.validate(&serde_value)
    //     }

    //     pub fn validate(&self, input: &serde_json::Value) -> json_schema::ValidationState {
    //         self.schema.validate(&input)
    //     }
    // }

    fn validate_string(input: &str) -> json_schema::ValidationState {
        let serde_value: serde_json::Value = serde_json::from_str(input).unwrap();
        validate(&serde_value)
    }

    fn validate(input: &serde_json::Value) -> json_schema::ValidationState {
        let schema = include_str!("../../render_context_schema.json");

        let v: serde_json::Value = serde_json::from_str(&schema)
            .expect("Cannot parse schema as JSON");

        let mut scope = json_schema::scope::Scope::new();
        // NOTE: using `false` instead of `true` allows us to use
        // `$comment` keys
        let schema = scope.compile_and_return(v.clone(), false).expect("Couldn't compile the schema");

        schema.validate(&input)
    }

    struct TestPkg {
        base_path: PathBuf,
    }

    impl TestPkg {
        fn new(tmp: &TempDir) -> Self {
            let pkg = Self { base_path: tmp.path().to_owned() };

            fs::create_dir_all(pkg.default_config_dir()).expect(
                "create deprecated user config dir",
            );
            fs::create_dir_all(pkg.recommended_user_config_dir())
                .expect("create recommended user config dir");
            fs::create_dir_all(pkg.deprecated_user_config_dir())
                .expect("create default config dir");
            pkg
        }
    }

    impl PackageConfigPaths for TestPkg {
        fn name(&self) -> String {
            String::from("testing")
        }
        fn default_config_dir(&self) -> PathBuf {
            self.base_path.join("root")
        }
        fn recommended_user_config_dir(&self) -> PathBuf {
            self.base_path.join("user")
        }
        fn deprecated_user_config_dir(&self) -> PathBuf {
            self.base_path.join("svc")
        }
    }

    fn new_test_pkg() -> (TempDir, TestPkg) {
        let tmp = TempDir::new("habitat_config_test").expect("create temp dir");
        let pkg = TestPkg::new(&tmp);

        let default_toml = pkg.default_config_dir().join("default.toml");
        let mut buffer = fs::File::create(default_toml).expect("couldn't write file");
        buffer.write_all(r#"
foo = "bar"
baz = "boo"

[foobar]
one = 1
two = 2
"#.as_bytes()).expect("Couldn't write default.toml");
        (tmp, pkg)
    }

    fn assert_valid(json_string: &str) {
        let result = validate_string(json_string);
        if !result.is_valid() {
            let error_string = result
                .errors
                .into_iter()
                .map(|x| format!("  {:?}", x))
                .collect::<Vec<String>>()
                .join("\n");
            let pretty_json = json::stringify_pretty(
                json::parse(json_string)
                    .expect("JSON should parse if we get this far"),
                2);
            assert!(false, r#"
JSON does not validate!
Errors:
{}

JSON:
{}
"#,
                    error_string,
                    pretty_json);
        }
    }

    #[test]
    fn sample_context_is_valid() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("sample_render_context.json");


        let mut f = fs::File::open(path).expect("could not open sample_render_context.json");
        let mut json = String::new();
        f.read_to_string(&mut json).expect("could not read sample_render_context.json");

        assert_valid(&json);
    }

    #[test]
    fn constructed_render_context_is_valid() {
        let system_info = SystemInfo {
            version: Cow::Owned("I AM A HABITAT VERSION".into()),
            member_id: Cow::Owned("MEMBER_ID".into()),
            ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            hostname: Cow::Owned("MY_HOSTNAME".into()),
            gossip_ip: IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
            gossip_port: 1234,
            http_gateway_ip: IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
            http_gateway_port: 5678,
            permanent: false
        };

        let ident = PackageIdent::new("core", "test_pkg", Some("1.0.0"), Some("20180321150416"));

        let deps = vec![
            PackageIdent::new("test", "pkg1", Some("1.0.0"), Some("20180321150416")),
            PackageIdent::new("test", "pkg2", Some("2.0.0"), Some("20180321150416")),
            PackageIdent::new("test", "pkg3", Some("3.0.0"), Some("20180321150416"))
        ];

        let mut env_hash = HashMap::new();
        env_hash.insert("PATH".into(), "/foo:/bar:/baz".into());
        env_hash.insert("SECRET".into(), "sooperseekrit".into());

        let pkg = Package {
            ident: Cow::Borrowed(&ident),
            // TODO (CM): have Pkg use FullyQualifiedPackageIdent, and
            // get origin, name, version, and release from it, rather
            // than storing each individually; I suspect that was just
            // for templating
            origin: Cow::Borrowed(&ident.origin),
            name: Cow::Borrowed(&ident.name),
            version: Cow::Owned(ident.version.clone().unwrap()),
            release: Cow::Owned(ident.release.clone().unwrap()),
            deps: Cow::Owned(deps),
            env: Cow::Owned(Env(env_hash)),
            exposes: Cow::Owned(vec!["1234".into(), "8000".into(), "2112".into()]),
            exports: Cow::Owned(HashMap::new()), // TODO (CM): Need real data
            path: Cow::Owned("my_path".into()),
            svc_path: Cow::Owned("svc_path".into()),
            svc_config_path: Cow::Owned("config_path".into()),
            svc_data_path: Cow::Owned("data_path".into()),
            svc_files_path: Cow::Owned("files_path".into()),
            svc_static_path: Cow::Owned("static_path".into()),
            svc_var_path: Cow::Owned("var_path".into()),
            svc_pid_file: Cow::Owned("pid_file".into()),
            svc_run: Cow::Owned("svc_run".into()),
            svc_user: Cow::Owned("hab".into()),
            svc_group: Cow::Owned("hab".into()),
        };

        let group: ServiceGroup = "foo.default".parse().unwrap();

        let (tmp_dir, test_pkg) = new_test_pkg();
        let cfg = Cfg::new(&test_pkg, None).expect("create config");

        use butterfly::rumor::service::SysInfo;
        let sys_info = SysInfo::new();

        // TODO (CM): just create a toml table directly
        let mut svc_member_cfg = BTreeMap::new();
        svc_member_cfg.insert("foo".into(), "bar".into());

        let me = SvcMember {
            member_id: Cow::Owned("MEMBER_ID".into()),
            pkg: Cow::Owned(Some(ident.clone())),
            application: Cow::Owned(None),
            environment: Cow::Owned(None),
            service: Cow::Owned("foo".into()),
            group: Cow::Owned("default".into()),
            org: Cow::Owned(None),
            persistent: true,
            leader: false,
            follower: false,
            update_leader: false,
            update_follower: false,
            election_is_running: false,
            election_is_no_quorum: false,
            election_is_finished: false,
            update_election_is_running: false,
            update_election_is_no_quorum: false,
            update_election_is_finished: false,
            sys: Cow::Owned(sys_info),
            alive: true,
            suspect: false,
            confirmed: false,
            departed: false,
            cfg: Cow::Owned(svc_member_cfg as toml::value::Table),
        };

        let svc = Svc {
            service_group: Cow::Borrowed(&group),
            election_status: Cow::Owned(ElectionStatus::ElectionInProgress),
            update_election_status: Cow::Owned(ElectionStatus::ElectionFinished),
            members: Cow::Owned(vec![me.clone()]),
            leader: Cow::Owned(None),
            update_leader: Cow::Owned(None),
            me: Cow::Owned(me.clone()),
            first: me.clone(),
        };

        let mut bind_map = HashMap::new();
        let bind_group = BindGroup{
            first: Some(me.clone()),
            members: vec![me.clone()]
        };
        bind_map.insert("foo".into(), bind_group);
        let binds = Binds(bind_map);

        let render_context = RenderContext{
            system_info: system_info,
            package: pkg,
            cfg: &cfg,
            svc: svc,
            bind: binds
        };

        let j = serde_json::to_string(&render_context).expect("can't serialize to JSON");
        assert_valid(&j);
    }

}
