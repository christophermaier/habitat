// Copyright (c) 2016-2017 Chef Software Inc. and/or applicable contributors
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

extern crate habitat_common as common;
#[macro_use]
extern crate habitat_core as hcore;
extern crate habitat_launcher_client as launcher_client;
#[macro_use]
extern crate habitat_sup as sup;
extern crate log;
extern crate env_logger;
extern crate ansi_term;
extern crate libc;
#[macro_use]
extern crate clap;
extern crate time;
extern crate url;

use std::io::{self, Write};
use std::net::{SocketAddr, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process;
use std::result;
use std::str::FromStr;

use ansi_term::Colour::{Red, Yellow};
use clap::{App, ArgMatches};
use common::command::package::install::InstallSource;
use common::ui::UI;
use hcore::channel;
use hcore::crypto::{self, default_cache_key_path, SymKey};
#[cfg(windows)]
use hcore::crypto::dpapi::encrypt;
use hcore::env as henv;
use hcore::fs;
use hcore::package::PackageIdent;
use hcore::package::install::PackageInstall;
use hcore::package::metadata::PackageType;
use hcore::service::{ApplicationEnvironment, ServiceGroup};
use hcore::url::{bldr_url_from_env, default_bldr_url};
use launcher_client::{LauncherCli, ERR_NO_RETRY_EXCODE, OK_NO_RETRY_EXCODE};
use url::Url;

use sup::VERSION;
use sup::config::{GossipListenAddr, GOSSIP_DEFAULT_PORT};
use sup::error::{Error, Result, SupError};
use sup::feat;
use sup::command;
use sup::http_gateway;
use sup::manager::{Manager, ManagerConfig};
use sup::manager::service::{DesiredState, ServiceBind, Topology, UpdateStrategy};
use sup::manager::service::{CompositeSpec, ServiceSpec, StartStyle};
use sup::util;

/// Our output key
static LOGKEY: &'static str = "MN";

static RING_ENVVAR: &'static str = "HAB_RING";
static RING_KEY_ENVVAR: &'static str = "HAB_RING_KEY";

fn main() {
    if let Err(err) = start() {
        println!("{}", err);
        match err {
            SupError { err: Error::ProcessLocked(_), .. } => process::exit(ERR_NO_RETRY_EXCODE),
            SupError { err: Error::Departed, .. } => {
                process::exit(ERR_NO_RETRY_EXCODE);
            }
            _ => process::exit(1),
        }
    }
}

fn boot() -> Option<LauncherCli> {
    env_logger::init().unwrap();
    enable_features_from_env();
    if !crypto::init() {
        println!("Crypto initialization failed!");
        process::exit(1);
    }
    match launcher_client::env_pipe() {
        Some(pipe) => {
            match LauncherCli::connect(pipe) {
                Ok(launcher) => Some(launcher),
                Err(err) => {
                    println!("{}", err);
                    process::exit(1);
                }
            }
        }
        None => None,
    }
}

fn start() -> Result<()> {
    let launcher = boot();
    let app_matches = match cli().get_matches_safe() {
        Ok(matches) => matches,
        Err(err) => {
            let out = io::stdout();
            writeln!(&mut out.lock(), "{}", err.message).expect("Error writing Error to stdout");
            process::exit(ERR_NO_RETRY_EXCODE);
        }
    };
    match app_matches.subcommand() {
        ("bash", Some(m)) => sub_bash(m),
        ("config", Some(m)) => sub_config(m),
        ("load", Some(m)) => sub_load(m),
        ("run", Some(m)) => {
            let launcher = launcher.ok_or(sup_error!(Error::NoLauncher))?;
            sub_run(m, launcher)
        }
        ("sh", Some(m)) => sub_sh(m),
        ("start", Some(m)) => {
            let launcher = launcher.ok_or(sup_error!(Error::NoLauncher))?;
            sub_start(m, launcher)
        }
        ("status", Some(m)) => sub_status(m),
        ("stop", Some(m)) => sub_stop(m),
        ("term", Some(m)) => sub_term(m),
        ("unload", Some(m)) => sub_unload(m),
        _ => unreachable!(),
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn cli<'a, 'b>() -> App<'a, 'b> {
    clap_app!(("hab-sup") =>
        (about: "The Habitat Supervisor")
        (version: VERSION)
        (author: "\nAuthors: The Habitat Maintainers <humans@habitat.sh>\n")
        (@setting VersionlessSubcommands)
        (@setting SubcommandRequiredElseHelp)
        (@arg VERBOSE: -v +global "Verbose output; shows line numbers")
        (@arg NO_COLOR: --("no-color") +global "Turn ANSI color off")
        (@subcommand bash =>
            (about: "Start an interactive Bash-like shell")
            (aliases: &["b", "ba", "bas"])
        )
        (@subcommand config =>
            (about: "Displays the default configuration options for a service")
            (aliases: &["c", "co", "con", "conf", "confi"])
            (@arg PKG_IDENT: +required +takes_value
                "A package identifier (ex: core/redis, core/busybox-static/1.42.2)")
        )
        (@subcommand load =>
            (about: "Load a service to be started and supervised by Habitat from a package or \
                artifact. Services started in this manner will persist through Supervisor \
                restarts.")
            (aliases: &["lo", "loa"])
            (@arg PKG_IDENT_OR_ARTIFACT: +required +takes_value
                "A Habitat package identifier (ex: core/redis) or filepath to a Habitat Artifact \
                (ex: /home/core-redis-3.0.7-21120102031201-x86_64-linux.hart)")
            (@arg NAME: --("override-name") +takes_value
                "The name for the state directory if there is more than one Supervisor running \
                [default: default]")
            (@arg APPLICATION: --application -a +takes_value requires[ENVIRONMENT]
                "Application name; [default: not set].")
            (@arg ENVIRONMENT: --environment -e +takes_value requires[APPLICATION]
                "Environment name; [default: not set].")
            (@arg CHANNEL: --channel +takes_value
                "Receive package updates from the specified release channel [default: stable]")
            (@arg GROUP: --group +takes_value
                "The service group; shared config and topology [default: default].")
            (@arg BLDR_URL: --url -u +takes_value {valid_url}
                "Receive package updates from Builder at the specified URL \
                [default: https://bldr.habitat.sh]")
            (@arg TOPOLOGY: --topology -t +takes_value {valid_topology}
                "Service topology; [default: none]")
            (@arg STRATEGY: --strategy -s +takes_value {valid_update_strategy}
                "The update strategy; [default: none] [values: none, at-once, rolling]")
            (@arg BIND: --bind +takes_value +multiple
                "One or more service groups to bind to a configuration")
            (@arg FORCE: --force -f "Load or reload an already loaded service. If the service was \
                previously loaded and running this operation will also restart the service")
        )
        (@subcommand unload =>
            (about: "Unload a persistent or transient service started by the Habitat \
                Supervisor. If the Supervisor is running when the service is unloaded the \
                service will be stopped.")
            (aliases: &["un", "unl", "unlo", "unloa"])
            (@arg PKG_IDENT: +required +takes_value "A Habitat package identifier (ex: core/redis)")
            (@arg NAME: --("override-name") +takes_value
                "The name for the state directory if there is more than one Supervisor running \
                [default: default]")
        )
        (@subcommand run =>
            (about: "Run the Habitat Supervisor")
            (aliases: &["r", "ru"])
            (@arg LISTEN_GOSSIP: --("listen-gossip") +takes_value
                "The listen address for the gossip system [default: 0.0.0.0:9638]")
            (@arg LISTEN_HTTP: --("listen-http") +takes_value
                "The listen address for the HTTP gateway [default: 0.0.0.0:9631]")
            (@arg NAME: --("override-name") +takes_value
                "The name of the Supervisor if launching more than one [default: default]")
            (@arg ORGANIZATION: --org +takes_value
                "The organization that the Supervisor and it's subsequent services are part of \
                [default: default]")
            (@arg PEER: --peer +takes_value +multiple
                "The listen address of an initial peer (IP[:PORT])")
            (@arg PERMANENT_PEER: --("permanent-peer") -I "If this Supervisor is a permanent peer")
            (@arg RING: --ring -r +takes_value "Ring key name")
            (@arg CHANNEL: --channel +takes_value
                "Receive Supervisor updates from the specified release channel [default: stable]")
            (@arg BLDR_URL: --url -u +takes_value {valid_url}
                "Receive Supervisor updates from Builder at the specified URL \
                [default: https://bldr.habitat.sh]")
            (@arg AUTO_UPDATE: --("auto-update") -A "Enable automatic updates for the Supervisor \
                itself")
            (@arg EVENTS: --events -n +takes_value {valid_service_group} "Name of the service \
                group running a Habitat EventSrv to forward Supervisor and service event data to")
        )
        (@subcommand sh =>
            (about: "Start an interactive Bourne-like shell")
            (aliases: &[])
        )
        (@subcommand start =>
            (about: "Start a loaded, but stopped, Habitat service or a transient service from \
                a package or artifact. If the Habitat Supervisor is not already running this \
                will additionally start one for you.")
            (aliases: &["sta", "star"])
            (@arg LISTEN_GOSSIP: --("listen-gossip") +takes_value
                "The listen address for the gossip system [default: 0.0.0.0:9638]")
            (@arg LISTEN_HTTP: --("listen-http") +takes_value
                "The listen address for the HTTP gateway [default: 0.0.0.0:9631]")
            (@arg NAME: --("override-name") +takes_value
                "The name for the state directory if launching more than one Supervisor \
                [default: default]")
            (@arg ORGANIZATION: --org +takes_value
                "The organization that the Supervisor and it's subsequent services are part of \
                [default: default]")
            (@arg PEER: --peer +takes_value +multiple
                "The listen address of an initial peer (IP[:PORT])")
            (@arg PERMANENT_PEER: --("permanent-peer") -I "If this Supervisor is a permanent peer")
            (@arg RING: --ring -r +takes_value "Ring key name")
            (@arg PKG_IDENT_OR_ARTIFACT: +required +takes_value
                "A Habitat package identifier (ex: core/redis) or filepath to a Habitat Artifact \
                (ex: /home/core-redis-3.0.7-21120102031201-x86_64-linux.hart)")
            (@arg APPLICATION: --application -a +takes_value requires[ENVIRONMENT]
                "Application name; [default: not set].")
            (@arg ENVIRONMENT: --environment -e +takes_value requires[APPLICATION]
                "Environment name; [default: not set].")
            (@arg CHANNEL: --channel +takes_value
                "Receive package updates from the specified release channel [default: stable]")
            (@arg GROUP: --group +takes_value
                "The service group; shared config and topology [default: default]")
            (@arg BLDR_URL: --url -u +takes_value {valid_url}
                "Receive package updates from Builder at the specified URL \
                [default: https://bldr.habitat.sh]")
            (@arg TOPOLOGY: --topology -t +takes_value {valid_topology}
                "Service topology; [default: none]")
            (@arg STRATEGY: --strategy -s +takes_value {valid_update_strategy}
                "The update strategy; [default: none] [values: none, at-once, rolling]")
            (@arg BIND: --bind +takes_value +multiple
                "One or more service groups to bind to a configuration")
            (@arg CONFIG_DIR: --("config-from") +takes_value {dir_exists}
                "Use package config from this path, rather than the package itself")
            (@arg AUTO_UPDATE: --("auto-update") -A "Enable automatic updates for the Supervisor \
                itself")
            (@arg EVENTS: --events -n +takes_value {valid_service_group} "Name of the service \
                group running a Habitat EventSrv to forward Supervisor and service event data to")
        )
        (@subcommand status =>
            (about: "Query the status of Habitat services.")
            (aliases: &["stat", "statu", "status"])
            (@arg PKG_IDENT: +takes_value "A Habitat package identifier (ex: core/redis)")
            (@arg NAME: --("override-name") +takes_value
                "The name for the state directory if there is more than one Supervisor running \
                [default: default]")
        )
        (@subcommand stop =>
            (about: "Stop a running Habitat service.")
            (aliases: &["sto"])
            (@arg PKG_IDENT: +required +takes_value "A Habitat package identifier (ex: core/redis)")
            (@arg NAME: --("override-name") +takes_value
                "The name for the state directory if there is more than one Supervisor running \
                [default: default]")
        )
        (@subcommand term =>
            (about: "Gracefully terminate the Habitat Supervisor and all of it's running services")
            (@arg NAME: --("override-name") +takes_value
                "The name of the Supervisor if more than one is running [default: default]")
        )
    )
}

#[cfg(target_os = "windows")]
fn cli<'a, 'b>() -> App<'a, 'b> {
    clap_app!(("hab-sup") =>
        (about: "The Habitat Supervisor")
        (version: VERSION)
        (author: "\nAuthors: The Habitat Maintainers <humans@habitat.sh>\n")
        (@setting VersionlessSubcommands)
        (@setting SubcommandRequiredElseHelp)
        (@arg VERBOSE: -v +global "Verbose output; shows line numbers")
        (@arg NO_COLOR: --("no-color") +global "Turn ANSI color off")
        (@subcommand bash =>
            (about: "Start an interactive Bash-like shell")
            (aliases: &["b", "ba", "bas"])
        )
        (@subcommand config =>
            (about: "Displays the default configuration options for a service")
            (aliases: &["c", "co", "con", "conf", "confi"])
            (@arg PKG_IDENT: +required +takes_value
                "A package identifier (ex: core/redis, core/busybox-static/1.42.2)")
        )
        (@subcommand load =>
            (about: "Load a service to be started and supervised by Habitat from a package or \
                artifact. Services started in this manner will persist through Supervisor \
                restarts.")
            (aliases: &["lo", "loa"])
            (@arg PKG_IDENT_OR_ARTIFACT: +required +takes_value
                "A Habitat package identifier (ex: core/redis) or filepath to a Habitat Artifact \
                (ex: /home/core-redis-3.0.7-21120102031201-x86_64-linux.hart)")
            (@arg NAME: --("override-name") +takes_value
                "The name for the state directory if there is more than one Supervisor running \
                [default: default]")
            (@arg APPLICATION: --application -a +takes_value requires[ENVIRONMENT]
                "Application name; [default: not set].")
            (@arg ENVIRONMENT: --environment -e +takes_value requires[APPLICATION]
                "Environment name; [default: not set].")
            (@arg CHANNEL: --channel +takes_value
                "Receive package updates from the specified release channel [default: stable]")
            (@arg GROUP: --group +takes_value
                "The service group; shared config and topology [default: default].")
            (@arg BLDR_URL: --url -u +takes_value {valid_url}
                "Receive package updates from Builder at the specified URL \
                [default: https://bldr.habitat.sh]")
            (@arg TOPOLOGY: --topology -t +takes_value {valid_topology}
                "Service topology; [default: none]")
            (@arg STRATEGY: --strategy -s +takes_value {valid_update_strategy}
                "The update strategy; [default: none] [values: none, at-once, rolling]")
            (@arg BIND: --bind +takes_value +multiple
                "One or more service groups to bind to a configuration")
            (@arg FORCE: --force -f "Load or reload an already loaded service. If the service was \
                previously loaded and running this operation will also restart the service")
                (@arg PASSWORD: --password +takes_value
                    "Password of the service user")
        )
        (@subcommand unload =>
            (about: "Unload a persistent or transient service started by the Habitat \
                Supervisor. If the Supervisor is running when the service is unloaded the \
                service will be stopped.")
            (aliases: &["un", "unl", "unlo", "unloa"])
            (@arg PKG_IDENT: +required +takes_value "A Habitat package identifier (ex: core/redis)")
            (@arg NAME: --("override-name") +takes_value
                "The name for the state directory if there is more than one Supervisor running \
                [default: default]")
        )
        (@subcommand run =>
            (about: "Run the Habitat Supervisor")
            (aliases: &["r", "ru"])
            (@arg LISTEN_GOSSIP: --("listen-gossip") +takes_value
                "The listen address for the gossip system [default: 0.0.0.0:9638]")
            (@arg LISTEN_HTTP: --("listen-http") +takes_value
                "The listen address for the HTTP gateway [default: 0.0.0.0:9631]")
            (@arg NAME: --("override-name") +takes_value
                "The name of the Supervisor if launching more than one [default: default]")
            (@arg ORGANIZATION: --org +takes_value
                "The organization that the Supervisor and it's subsequent services are part of \
                [default: default]")
            (@arg PEER: --peer +takes_value +multiple
                "The listen address of an initial peer (IP[:PORT])")
            (@arg PERMANENT_PEER: --("permanent-peer") -I "If this Supervisor is a permanent peer")
            (@arg RING: --ring -r +takes_value "Ring key name")
            (@arg CHANNEL: --channel +takes_value
                "Receive Supervisor updates from the specified release channel [default: stable]")
            (@arg BLDR_URL: --url -u +takes_value {valid_url}
                "Receive Supervisor updates from Builder at the specified URL \
                [default: https://bldr.habitat.sh]")
            (@arg AUTO_UPDATE: --("auto-update") -A "Enable automatic updates for the Supervisor \
                itself")
            (@arg EVENTS: --events -n +takes_value {valid_service_group} "Name of the service \
                group running a Habitat EventSrv to forward Supervisor and service event data to")
        )
        (@subcommand sh =>
            (about: "Start an interactive Bourne-like shell")
            (aliases: &[])
        )
        (@subcommand start =>
            (about: "Start a loaded, but stopped, Habitat service or a transient service from \
                a package or artifact. If the Habitat Supervisor is not already running this \
                will additionally start one for you.")
            (aliases: &["sta", "star"])
            (@arg LISTEN_GOSSIP: --("listen-gossip") +takes_value
                "The listen address for the gossip system [default: 0.0.0.0:9638]")
            (@arg LISTEN_HTTP: --("listen-http") +takes_value
                "The listen address for the HTTP gateway [default: 0.0.0.0:9631]")
            (@arg NAME: --("override-name") +takes_value
                "The name for the state directory if launching more than one Supervisor \
                [default: default]")
            (@arg ORGANIZATION: --org +takes_value
                "The organization that the Supervisor and it's subsequent services are part of \
                [default: default]")
            (@arg PEER: --peer +takes_value +multiple
                "The listen address of an initial peer (IP[:PORT])")
            (@arg PERMANENT_PEER: --("permanent-peer") -I "If this Supervisor is a permanent peer")
            (@arg RING: --ring -r +takes_value "Ring key name")
            (@arg PKG_IDENT_OR_ARTIFACT: +required +takes_value
                "A Habitat package identifier (ex: core/redis) or filepath to a Habitat Artifact \
                (ex: /home/core-redis-3.0.7-21120102031201-x86_64-linux.hart)")
            (@arg APPLICATION: --application -a +takes_value requires[ENVIRONMENT]
                "Application name; [default: not set].")
            (@arg ENVIRONMENT: --environment -e +takes_value requires[APPLICATION]
                "Environment name; [default: not set].")
            (@arg CHANNEL: --channel +takes_value
                "Receive package updates from the specified release channel [default: stable]")
            (@arg GROUP: --group +takes_value
                "The service group; shared config and topology [default: default]")
            (@arg BLDR_URL: --url -u +takes_value {valid_url}
                "Receive package updates from Builder at the specified URL \
                [default: https://bldr.habitat.sh]")
            (@arg TOPOLOGY: --topology -t +takes_value {valid_topology}
                "Service topology; [default: none]")
            (@arg STRATEGY: --strategy -s +takes_value {valid_update_strategy}
                "The update strategy; [default: none] [values: none, at-once, rolling]")
            (@arg BIND: --bind +takes_value +multiple
                "One or more service groups to bind to a configuration")
            (@arg CONFIG_DIR: --("config-from") +takes_value {dir_exists}
                "Use package config from this path, rather than the package itself")
            (@arg AUTO_UPDATE: --("auto-update") -A "Enable automatic updates for the Supervisor \
                itself")
            (@arg EVENTS: --events -n +takes_value {valid_service_group} "Name of the service \
                group running a Habitat EventSrv to forward Supervisor and service event data to")
            (@arg PASSWORD: --password +takes_value "Password of the service user")
        )
        (@subcommand status =>
            (about: "Query the status of Habitat services.")
            (aliases: &["stat", "statu", "status"])
            (@arg PKG_IDENT: +takes_value "A Habitat package identifier (ex: core/redis)")
            (@arg NAME: --("override-name") +takes_value
                "The name for the state directory if there is more than one Supervisor running \
                [default: default]")
        )
        (@subcommand stop =>
            (about: "Stop a running Habitat service.")
            (aliases: &["sto"])
            (@arg PKG_IDENT: +required +takes_value "A Habitat package identifier (ex: core/redis)")
            (@arg NAME: --("override-name") +takes_value
                "The name for the state directory if there is more than one Supervisor running \
                [default: default]")
        )
    )
}

fn sub_bash(m: &ArgMatches) -> Result<()> {
    if m.is_present("VERBOSE") {
        hcore::output::set_verbose(true);
    }
    if m.is_present("NO_COLOR") {
        hcore::output::set_no_color(true);
    }

    command::shell::bash()
}

fn sub_config(m: &ArgMatches) -> Result<()> {
    let ident = PackageIdent::from_str(m.value_of("PKG_IDENT").unwrap())?;

    common::command::package::config::start(&ident, "/")?;
    Ok(())
}

fn sub_load(m: &ArgMatches) -> Result<()> {
    if m.is_present("VERBOSE") {
        hcore::output::set_verbose(true);
    }
    if m.is_present("NO_COLOR") {
        hcore::output::set_no_color(true);
    }
    let cfg = mgrcfg_from_matches(m)?;
    let install_source = install_source_from_input(m)?;

    // TODO (CM): should load be able to download new artifacts if
    // you're re-loading with --force?
    // If we've already got a spec for this thing, we don't want to
    // inadvertently download a new version

    let installed = match existing_spec_for_ident(&cfg, install_source.as_ref().clone()) {
        Some(spec) => {
            // We've seen this service / composite before. Thus `load`
            // basically acts as a way to edit spec files on the
            // command line. As a result, we a) check that you
            // *really* meant to change an existing spec, and b) DO
            // NOT download a potentially new version of the package
            // in question

            if !m.is_present("FORCE") {
                // TODO (CM): make this error reflect composites
                return Err(sup_error!(Error::ServiceLoaded(spec.ident().clone())));
            }

            let fs_root_path = Path::new(&*fs::FS_ROOT_PATH); // TODO (CM): baaaaarffff
            match spec {
                Spec::Service(mut service_spec) => {
                    // If we changed our spec AND we're not going to
                    // be updating, we HAVE to install a new package
                    // now to ensure we've got the right code to
                    // run. Otherwise, we're either good to continue
                    // running what we've got, or the ServiceUpdater
                    // will handle things for us.
                    let ident_changed = install_source.as_ref() != &service_spec.ident;

                    service_spec.ident = install_source.as_ref().clone();
                    update_spec_from_user(&mut service_spec, m)?;

                    let strategy_is_none = service_spec.update_strategy == UpdateStrategy::None;

                    if ident_changed && strategy_is_none {
                        util::pkg::install(
                            &mut UI::default(),
                            &service_spec.bldr_url,
                            &install_source,
                            &service_spec.channel,
                        )?;
                    }

                    service_spec.start_style = StartStyle::Persistent;

                    // write the spec
                    Manager::save_spec_for(&cfg, &service_spec)?;
                    outputln!("The {} service was successfully loaded", service_spec.ident);
                    return Ok(());
                }
                Spec::Composite(composite_spec) => {
                    // TODO (CM):  handle reload question here

                    // Did the ident change?... Oh, how would we know?
                    // we don't store the ident in the spec!

                    // Anyway...
                    // If the spec HAS NOT CHANGED, then we need to
                    // tweak all the EXISTING specs for the composite.
                    //
                    // for each spec
                    // update the spec based on the user input. if the
                    // composite didn't change, then these ident's
                    // won't have either (or have they? What if a user
                    // individually tweaked them?)

                    //
                    // If the composite ident HAS changed... (again,
                    // how would we determine that)
                    //
                    // That means that we need to figure out what
                    // services need to stay, which need to go, and
                    // which are new. Ugh... let's clean up the rest
                    // of this before tackling that, eh?


                    // load the composite package from disk
                    PackageInstall::load(composite_spec.ident(), Some(fs_root_path))?
                }
            }
        }
        None => {
            // We've not seen this before; TO THE BUILDER-MOBILE!
            let bldr_url = bldr_url(m);
            let channel = channel(m);
            util::pkg::install(&mut UI::default(), &bldr_url, &install_source, &channel)?
        }
    };


    // TODO (CM): The code below needs to be pulled up

    // TODO (CM): Of course, if we don't want to inadvertently change
    // the version, then the original_ident probably needs to be
    // whatever was in the spec to begin with... and that, of course,
    // only applies to standalones, not composites, since *their*
    // idents are baked in.
    let original_ident = install_source.as_ref();
    let mut specs = specs_from_package(original_ident, &installed, m)?;

    // If this is an error, then it wasn't a composite :P
    // TODO (CM): Is there a better place to put this / better way to
    // express this? I'd like to keep as much special logic out of
    // this as possible...

    // TODO (CM): pull this up? Don't want to inadvertently change the
    // spec if it was already installed... or do we? :thinking_face:
    if let Ok(composite_spec) = CompositeSpec::from_package_install(&installed) {
        Manager::save_composite_spec_for(&cfg, &composite_spec)?;
    }

    for spec in specs.iter_mut() {
        // When you "load" services, that indicates that you want them
        // to be permanent. By default, they're transient.
        spec.start_style = StartStyle::Persistent;
        Manager::save_spec_for(&cfg, spec)?;
        outputln!("The {} service was successfully loaded", spec.ident);
    }
    Ok(())
}

fn sub_unload(m: &ArgMatches) -> Result<()> {
    if m.is_present("VERBOSE") {
        hcore::output::set_verbose(true);
    }
    if m.is_present("NO_COLOR") {
        hcore::output::set_no_color(true);
    }

    let cfg = mgrcfg_from_matches(m)?;
    let ident = PackageIdent::from_str(m.value_of("PKG_IDENT").unwrap())?;

    // While we could determine if an ident referred to a composite or
    // standalone package by trying to load the PackageInstall from
    // disk and interrogating it, that could get us into a weird
    // situation with composites and non-fully-qualified idents, as
    // it's possible that there might be a later version of the
    // composite on disk, but that's now what the specs were generated
    // from.
    //
    // Thus, we try to resolve the ident to a standalone spec first,
    // then to a composite spec next.
    //
    // HOWEVER this could be weird if you have a standalone service
    // with the same name as the composite. Hrmm...
    let spec_file = Manager::spec_path_by_ident(&cfg, &ident);
    if spec_file.is_file() {
        outputln!("Unloading {:?}", spec_file);

        // TODO (CM): This currently fails if any of the spec files
        // don't exist.
        //
        // Should we check that they all exist first, treat failure to
        // remove as "OK", something else?
        std::fs::remove_file(&spec_file).map_err(|err| {
            sup_error!(Error::ServiceSpecFileIO(spec_file, err))
        })?;
    } else {
        let composite_spec_file = Manager::composite_path_by_ident(&cfg, &ident);
        if composite_spec_file.is_file() {
            // TODO (CM): BAAARF
            let fs_root_path = Path::new(&*fs::FS_ROOT_PATH);
            let package = PackageInstall::load(&ident, Some(fs_root_path))?;

            let services = package.pkg_services()?;
            let mut spec_files = Vec::with_capacity(services.len() + 1);

            for service in services.iter() {
                let sf = Manager::spec_path_by_ident(&cfg, service);
                spec_files.push(sf);
            }
            spec_files.push(composite_spec_file);

            for file in spec_files {
                outputln!("Unloading {:?}", file);
                std::fs::remove_file(&file).map_err(|err| {
                    sup_error!(Error::ServiceSpecFileIO(file, err))
                })?;
            }

        } else {
            // TODO (CM): wasn't a spec or a composite!
            ()
        }
    }
    Ok(())
}

fn sub_run(m: &ArgMatches, launcher: LauncherCli) -> Result<()> {
    let cfg = mgrcfg_from_matches(m)?;
    let mut manager = Manager::load(cfg, launcher)?;
    manager.run()
}

fn sub_sh(m: &ArgMatches) -> Result<()> {
    if m.is_present("VERBOSE") {
        hcore::output::set_verbose(true);
    }
    if m.is_present("NO_COLOR") {
        hcore::output::set_no_color(true);
    }

    command::shell::sh()
}

fn sub_start(m: &ArgMatches, launcher: LauncherCli) -> Result<()> {
    if m.is_present("VERBOSE") {
        hcore::output::set_verbose(true);
    }
    if m.is_present("NO_COLOR") {
        hcore::output::set_no_color(true);
    }
    let cfg = mgrcfg_from_matches(m)?;

    let mut ui = UI::default();
    if !fs::am_i_root() {
        ui.warn(
            "Running the Habitat Supervisor with root or superuser privileges is recommended",
        )?;
        ui.br()?;
    }

    let install_source = install_source_from_input(m)?;
    let original_ident: &PackageIdent = install_source.as_ref();

    // NOTE: As coded, if you try to start a service from a hart file,
    // but you already have a spec for that service (regardless of
    // version), you're not going to ever install your hart file, and
    // the spec isn't going to be updated to point to that exact
    // version.

    let specs = match existing_spec_for_ident(&cfg, original_ident.clone()) {
        Some(Spec::Service(mut spec)) => {
            if spec.desired_state == DesiredState::Down {
                spec.desired_state = DesiredState::Up;
                vec![spec]
            } else {
                if !Manager::is_running(&cfg)? {
                    let mut manager = Manager::load(cfg, launcher)?;
                    return manager.run();
                } else {
                    process::exit(OK_NO_RETRY_EXCODE);
                }
            }
        }
        Some(Spec::Composite(x)) => {
            // TODO (CM): need to check to see if you already have the
            // composite in place, and if so, start everything
            vec![]
        }
        None => {
            let bldr_url = bldr_url(m);
            let channel = channel(m);

            let installed_package = match util::pkg::installed(install_source.as_ref()) {
                None => {
                    outputln!("Missing package for {}", install_source.as_ref());
                    util::pkg::install(&mut UI::default(), &bldr_url, &install_source, &channel)?
                }
                Some(package) => package,
            };

            let specs = specs_from_package(&original_ident, &installed_package, m)?;

            // Saving the composite spec here, because we currently
            // need the PackageInstall to create it! It'll only create
            // a composite spec if the package is itself a composite.
            if let Ok(composite_spec) = CompositeSpec::from_package_install(&installed_package) {
                Manager::save_composite_spec_for(&cfg, &composite_spec)?;
            }

            specs
        }
    };

    // TODO (CM):  copied from load
    // Need to tease apart exact differences between start and load
    for spec in specs.iter() {
        Manager::save_spec_for(&cfg, spec)?;
    }
    if Manager::is_running(&cfg)? {
        outputln!(
            "Supervisor starting {}. See the Supervisor output for more details.",
            original_ident
        );
    } else {
        let mut manager = Manager::load(cfg, launcher)?;
        manager.run()?
    }

    Ok(())
}

fn sub_status(m: &ArgMatches) -> Result<()> {
    if m.is_present("VERBOSE") {
        hcore::output::set_verbose(true);
    }
    if m.is_present("NO_COLOR") {
        hcore::output::set_no_color(true);
    }
    let cfg = mgrcfg_from_matches(m)?;
    if !Manager::is_running(&cfg)? {
        println!("The Supervisor is not running.");
        process::exit(3);
    }
    match m.value_of("PKG_IDENT") {
        Some(pkg) => {
            match Manager::service_status(cfg, PackageIdent::from_str(pkg)?) {
                Ok(status) => outputln!("{}", status),
                Err(_) => {
                    println!("{} is not currently loaded.", pkg);
                    process::exit(2);
                }
            }
        }
        None => {
            let statuses = Manager::status(cfg)?;
            if statuses.is_empty() {
                println!("No services loaded.");
                return Ok(());
            }
            for status in statuses {
                println!("{}", status);
            }
        }
    }
    Ok(())
}

fn sub_stop(m: &ArgMatches) -> Result<()> {
    if m.is_present("VERBOSE") {
        hcore::output::set_verbose(true);
    }
    if m.is_present("NO_COLOR") {
        hcore::output::set_no_color(true);
    }
    let cfg = mgrcfg_from_matches(m)?;

    let ident = PackageIdent::from_str(m.value_of("PKG_IDENT").unwrap())?;
    let mut specs = installed_specs_from_ident(&cfg, &ident)?;

    for spec in specs.iter_mut() {
        spec.desired_state = DesiredState::Down;
        Manager::save_spec_for(&cfg, &spec)?;
    }

    Ok(())
}

fn sub_term(m: &ArgMatches) -> Result<()> {
    let cfg = mgrcfg_from_matches(m)?;
    match Manager::term(&cfg) {
        Err(SupError { err: Error::ProcessLockIO(_, _), .. }) => {
            println!("Supervisor not started.");
            Ok(())
        }
        result => result,
    }
}


// Internal Implementation Details
////////////////////////////////////////////////////////////////////////

/// Helper enum to abstract over spec type.
///
/// Currently needed only here. Don't bother moving anywhere because
/// ServiceSpecs AND CompositeSpecs will be going away soon anyway.
enum Spec {
    Service(ServiceSpec),
    Composite(CompositeSpec),
}

impl Spec {
    /// We need to get at the identifier of a spec, regardless of
    /// which kind it is.
    fn ident(&self) -> &PackageIdent {
        match self {
            &Spec::Composite(ref s) => s.ident(),
            &Spec::Service(ref s) => s.ident.as_ref(),
        }
    }
}

/// Given a package identifier, return the `Spec` for that
/// package, if it already exists in this supervisor.
fn existing_spec_for_ident(cfg: &ManagerConfig, ident: PackageIdent) -> Option<Spec> {
    let default_spec = ServiceSpec::default_for(ident.clone());
    let spec_file = Manager::spec_path_for(cfg, &default_spec);

    // Try it as a service first
    if let Ok(spec) = ServiceSpec::from_file(&spec_file) {
        Some(Spec::Service(spec))
    } else {
        // Try it as a composite next
        let composite_spec_file = Manager::composite_path_by_ident(&cfg, &ident);
        // If the file doesn't exist, that'll be an error, which will
        // convert to None, which is fine
        CompositeSpec::from_file(composite_spec_file)
            .and_then(|s| Ok(Spec::Composite(s)))
            .ok()
    }
}

fn mgrcfg_from_matches(m: &ArgMatches) -> Result<ManagerConfig> {
    let mut cfg = ManagerConfig::default();

    cfg.auto_update = m.is_present("AUTO_UPDATE");
    cfg.update_url = bldr_url(m);
    cfg.update_channel = channel(m);
    if let Some(addr_str) = m.value_of("LISTEN_GOSSIP") {
        cfg.gossip_listen = GossipListenAddr::from_str(addr_str)?;
    }
    if let Some(addr_str) = m.value_of("LISTEN_HTTP") {
        cfg.http_listen = http_gateway::ListenAddr::from_str(addr_str)?;
    }
    if let Some(name_str) = m.value_of("NAME") {
        cfg.name = Some(String::from(name_str));
        outputln!("");
        outputln!(
            "{} Running more than one Habitat Supervisor is not recommended for most",
            Red.bold().paint("CAUTION:".to_string())
        );
        outputln!(
            "{} users in most use cases. Using one Supervisor per host for multiple",
            Red.bold().paint("CAUTION:".to_string())
        );
        outputln!(
            "{} services in one ring will yield much better performance.",
            Red.bold().paint("CAUTION:".to_string())
        );
        outputln!("");
        outputln!(
            "{} If you know what you're doing, carry on!",
            Red.bold().paint("CAUTION:".to_string())
        );
        outputln!("");
    }
    cfg.organization = m.value_of("ORGANIZATION").map(|org| org.to_string());
    cfg.gossip_permanent = m.is_present("PERMANENT_PEER");
    // TODO fn: Clean this up--using a for loop doesn't feel good however an iterator was
    // causing a lot of developer/compiler type confusion
    let mut gossip_peers: Vec<SocketAddr> = Vec::new();
    if let Some(peers) = m.values_of("PEER") {
        for peer in peers {
            let peer_addr = if peer.find(':').is_some() {
                peer.to_string()
            } else {
                format!("{}:{}", peer, GOSSIP_DEFAULT_PORT)
            };
            let addrs: Vec<SocketAddr> = match peer_addr.to_socket_addrs() {
                Ok(addrs) => addrs.collect(),
                Err(e) => {
                    outputln!("Failed to resolve peer: {}", peer_addr);
                    return Err(sup_error!(Error::NameLookup(e)));
                }
            };
            let addr: SocketAddr = addrs[0];
            gossip_peers.push(addr);
        }
    }
    cfg.gossip_peers = gossip_peers;
    let ring = match m.value_of("RING") {
        Some(val) => Some(SymKey::get_latest_pair_for(
            &val,
            &default_cache_key_path(None),
        )?),
        None => {
            match henv::var(RING_KEY_ENVVAR) {
                Ok(val) => {
                    let (key, _) =
                        SymKey::write_file_from_str(&val, &default_cache_key_path(None))?;
                    Some(key)
                }
                Err(_) => {
                    match henv::var(RING_ENVVAR) {
                        Ok(val) => {
                            Some(SymKey::get_latest_pair_for(
                                &val,
                                &default_cache_key_path(None),
                            )?)
                        }
                        Err(_) => None,
                    }
                }
            }
        }
    };
    if let Some(ring) = ring {
        cfg.ring = Some(ring.name_with_rev());
    }
    if let Some(events) = m.value_of("EVENTS") {
        cfg.eventsrv_group = ServiceGroup::from_str(events).ok();
    }
    Ok(cfg)
}

// Various CLI Parsing Functions
////////////////////////////////////////////////////////////////////////

/// Resolve a Builder URL. Taken from CLI args, the environment, or
/// (failing those) a default value.
fn bldr_url(m: &ArgMatches) -> String {
    match bldr_url_from_input(m) {
        Some(url) => url.to_string(),
        None => default_bldr_url(),
    }
}

/// A Builder URL, but *only* if the user specified it via CLI args or
/// the environment
fn bldr_url_from_input(m: &ArgMatches) -> Option<String> {
    m.value_of("BLDR_URL")
        .and_then(|u| Some(u.to_string()))
        .or_else(|| bldr_url_from_env())
}

/// Resolve a channel. Taken from CLI args, or (failing that), a
/// default value.
fn channel(matches: &ArgMatches) -> String {
    channel_from_input(matches).unwrap_or(channel::default())
}

/// A channel name, but *only* if the user specified via CLI args.
fn channel_from_input(m: &ArgMatches) -> Option<String> {
    m.value_of("CHANNEL").and_then(|c| Some(c.to_string()))
}

fn install_source_from_input(m: &ArgMatches) -> Result<InstallSource> {
    // PKG_IDENT_OR_ARTIFACT is required in subcommands that use it,
    // so unwrap() is safe here.
    let ident_or_artifact = m.value_of("PKG_IDENT_OR_ARTIFACT").unwrap();
    let install_source: InstallSource = ident_or_artifact.parse()?;
    Ok(install_source)
}

// ServiceSpec Modification Functions
////////////////////////////////////////////////////////////////////////

/// If the user supplied a --group option, set it on the
/// spec. Otherwise, we inherit the default value in the ServiceSpec,
/// which is "default".
fn set_group_from_input(spec: &mut ServiceSpec, m: &ArgMatches) {
    if let Some(g) = m.value_of("GROUP") {
        spec.group = g.to_string();
    }
}

/// If the user provides both --application and --environment options,
/// parse and set the value on the spec. Otherwise, we inherit the
/// default value of the ServiceSpec, which is None
fn set_app_env_from_input(spec: &mut ServiceSpec, m: &ArgMatches) -> Result<()> {
    if let (Some(app), Some(env)) = (m.value_of("APPLICATION"), m.value_of("ENVIRONMENT")) {
        spec.application_environment = Some(ApplicationEnvironment::new(
            app.to_string(),
            env.to_string(),
        )?);
    }
    Ok(())
}

/// Set a spec's Builder URL from CLI / environment variables, falling back
/// to a default value.
fn set_bldr_url(spec: &mut ServiceSpec, m: &ArgMatches) {
    spec.bldr_url = bldr_url(m);
}

/// Set a Builder URL only if specified by the user as a CLI argument
/// or an environment variable.
fn set_bldr_url_from_input(spec: &mut ServiceSpec, m: &ArgMatches) {
    if let Some(url) = bldr_url_from_input(m) {
        spec.bldr_url = url
    }
}

/// Set a channel only if specified by the user as a CLI argument.
fn set_channel_from_input(spec: &mut ServiceSpec, m: &ArgMatches) {
    if let Some(channel) = channel_from_input(m) {
        spec.channel = channel
    }
}

/// Set a spec's channel from CLI values, falling back
/// to a default value.
fn set_channel(spec: &mut ServiceSpec, m: &ArgMatches) {
    spec.channel = channel(m);
}

/// Set a topology value only if specified by the user as a CLI
/// argument.
fn set_topology_from_input(spec: &mut ServiceSpec, m: &ArgMatches) {
    if let Some(t) = m.value_of("TOPOLOGY") {
        // unwrap() is safe, because the input is validated by
        // `valid_topology`
        spec.topology = Topology::from_str(t).unwrap();
    }
}

/// Set an update strategy only if specified by the user as a CLI
/// argument.
fn set_strategy_from_input(spec: &mut ServiceSpec, m: &ArgMatches) {
    if let Some(s) = m.value_of("STRATEGY") {
        // unwrap() is safe, because the input is validated by `valid_update_strategy`
        spec.update_strategy = UpdateStrategy::from_str(s).unwrap();
    }
}

/// Set bind values if given on the command line.
///
/// NOTE: At the moment, binds for composite services should NOT be
/// set using this, as we do not have a mechanism to distinguish
/// between the different services within the composite.
fn set_binds_from_input(spec: &mut ServiceSpec, m: &ArgMatches) -> Result<()> {
    if let Some(bind_strs) = m.values_of("BIND") {
        let mut binds = Vec::new();
        for bind_str in bind_strs {
            binds.push(ServiceBind::from_str(bind_str)?);
        }
        spec.binds = binds;
    }
    Ok(())
}

/// Set a custom config directory if given on the command line.
///
/// NOTE: At the moment, this should not be used for composite
/// services, as we do not have a mechanism to distinguish between the
/// different services within the composite.
fn set_config_from_input(spec: &mut ServiceSpec, m: &ArgMatches) -> Result<()> {
    if let Some(ref config_from) = m.value_of("CONFIG_DIR") {
        spec.config_from = Some(PathBuf::from(config_from));
        outputln!("");
        outputln!(
            "{} Setting '{}' should only be used in development, not production!",
            Red.bold().paint("WARNING:".to_string()),
            Yellow.bold().paint(
                format!("--config-from {}", config_from),
            )
        );
        outputln!("");
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn set_password_from_input(spec: &mut ServiceSpec, m: &ArgMatches) -> Result<()> {
    if let Some(password) = m.value_of("PASSWORD") {
        spec.svc_encrypted_password = Some(encrypt(password.to_string())?);
    }
    Ok(())
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn set_password_from_input(_: &mut ServiceSpec, _: &ArgMatches) -> Result<()> {
    Ok(())
}

// ServiceSpec Generation Functions
////////////////////////////////////////////////////////////////////////
//
// While ServiceSpec has an implementation of the Default trait, we
// want to be sure that specs created in this module unquestionably
// conform to the defaults that our CLI lays out.
//
// Similarly, when ever we update existing specs (e.g., hab svc load
// --force) we must take care that we only change values that the user
// has given explicitly, and not override using default values.
//
// To that end, we have a function that create a "default ServiceSpec"
// as far as this module is concerned, which is to be used when
// creating *new* specs, and another that merges an *existing* spec
// with only command-line arguments.
//
////////////////////////////////////////////////////////////////////////

fn new_service_spec(ident: PackageIdent, m: &ArgMatches) -> Result<ServiceSpec> {
    let mut spec = ServiceSpec::default_for(ident);

    set_bldr_url(&mut spec, m);
    set_channel(&mut spec, m);

    set_app_env_from_input(&mut spec, m)?;
    set_group_from_input(&mut spec, m);
    set_strategy_from_input(&mut spec, m);
    set_topology_from_input(&mut spec, m);

    // TODO (CM): Remove these for composite-member specs
    set_binds_from_input(&mut spec, m)?;
    set_config_from_input(&mut spec, m)?;
    set_password_from_input(&mut spec, m)?;
    Ok(spec)
}

fn update_spec_from_user(mut spec: &mut ServiceSpec, m: &ArgMatches) -> Result<()> {
    // The Builder URL and channel have default values; we only want to
    // change them if the user specified something!
    set_bldr_url_from_input(&mut spec, m);
    set_channel_from_input(&mut spec, m);

    set_app_env_from_input(&mut spec, m)?;
    set_group_from_input(&mut spec, m);
    set_strategy_from_input(&mut spec, m);
    set_topology_from_input(&mut spec, m);

    // TODO (CM): Remove these for composite-member specs
    set_binds_from_input(&mut spec, m)?;
    set_config_from_input(&mut spec, m)?;
    set_password_from_input(&mut spec, m)?;

    Ok(())
}

// CLAP Validation Functions
////////////////////////////////////////////////////////////////////////

fn dir_exists(val: String) -> result::Result<(), String> {
    if Path::new(&val).is_dir() {
        Ok(())
    } else {
        Err(format!("Directory: '{}' cannot be found", &val))
    }
}

fn valid_service_group(val: String) -> result::Result<(), String> {
    match ServiceGroup::validate(&val) {
        Ok(()) => Ok(()),
        Err(err) => Err(err.to_string()),
    }
}

fn valid_topology(val: String) -> result::Result<(), String> {
    match Topology::from_str(&val) {
        Ok(_) => Ok(()),
        Err(_) => Err(format!("Service topology: '{}' is not valid", &val)),
    }
}

fn valid_update_strategy(val: String) -> result::Result<(), String> {
    match UpdateStrategy::from_str(&val) {
        Ok(_) => Ok(()),
        Err(_) => Err(format!("Update strategy: '{}' is not valid", &val)),
    }
}

fn valid_url(val: String) -> result::Result<(), String> {
    match Url::parse(&val) {
        Ok(_) => Ok(()),
        Err(_) => Err(format!("URL: '{}' is not valid", &val)),
    }
}

////////////////////////////////////////////////////////////////////////

fn enable_features_from_env() {
    let features = vec![(feat::List, "LIST")];

    for feature in &features {
        match henv::var(format!("HAB_FEAT_{}", feature.1)) {
            Ok(ref val) if ["true", "TRUE"].contains(&val.as_str()) => {
                feat::enable(feature.0);
                outputln!("Enabling feature: {:?}", feature.0);
            }
            _ => {}
        }
    }

    if feat::is_enabled(feat::List) {
        outputln!("Listing feature flags environment variables:");
        for feature in &features {
            outputln!("     * {:?}: HAB_FEAT_{}=true", feature.0, feature.1);
        }
        outputln!("The Supervisor will start now, enjoy!");
    }
}

/// Given an installed package, generate a spec (or specs, in the case
/// of composite packages!) from it and the arguments passed in on the
/// command line.
fn specs_from_package(
    original_ident: &PackageIdent,
    package: &PackageInstall,
    m: &ArgMatches,
) -> Result<Vec<ServiceSpec>> {
    let specs = match package.pkg_type()? {
        PackageType::Standalone => {
            let spec = new_service_spec(original_ident.clone(), m)?;
            vec![spec]
        }
        PackageType::Composite => {
            let composite_name = &package.ident().name;

            // All specs in a composite currently share a lot of the
            // same information. Here, we create a "base spec" that we
            // can clone and further customize for each individual
            // service as needed.
            //
            // (Note that once we're done setting it up, base_spec is
            // intentionally immutable.)
            let base_spec = {

                // TODO (CM): would like to use new_service_spec if possible

                let mut spec = ServiceSpec::default();
                set_bldr_url(&mut spec, m);
                set_channel(&mut spec, m);

                set_app_env_from_input(&mut spec, m)?;
                set_group_from_input(&mut spec, m);
                set_strategy_from_input(&mut spec, m);
                set_topology_from_input(&mut spec, m);

                // TODO (CM): does this also need to be the same for
                // everything?
                // NOOOOOOO
                set_password_from_input(&mut spec, m)?;

                // TODO (CM): unset binds
                // TODO (CM): unset config?
                // TODO (CM): unset password?

                spec.composite = Some(composite_name.to_string());
                spec
            };

            let services = package.pkg_services()?;
            let bind_map = package.bind_map()?;

            let mut specs: Vec<ServiceSpec> = Vec::with_capacity(services.len());
            for service in services {
                outputln!("Found a service: {:?}", service);
                let mut spec = base_spec.clone();
                spec.ident = service;

                // What else do we need to customize?
                // - topology?
                // - update strategy
                // - optional binds? (we don't even have those yet)
                // - desired state?
                // - start style?

                // TODO (CM): Not sure if config has meaning for composites?
                // set_config_from_input(&mut spec, m)?;
                if let Some(bind_mappings) = bind_map.get(&spec.ident) {
                    let mut service_binds = Vec::with_capacity(bind_mappings.len());

                    // Turn each BindMapping into a ServiceBind and
                    // add them to the spec
                    for bind_mapping in bind_mappings.iter() {
                        let group = ServiceGroup::new(
                            spec.application_environment.as_ref(),
                            &bind_mapping.satisfying_service.name,
                            &spec.group,
                            // NOTE: We are explicitly NOT generating
                            // binds that include "organization". This
                            // is a feature that never quite found its
                            // footing, and will likely be removed
                            // from Habitat Real Soon Now (TM) (as of
                            // September 2017).
                            //
                            // As it exists right now, "organization"
                            // is a supervisor-wide setting, and thus
                            // is only available for `hab sup run` and
                            // `hab svc start`. We don't have a way
                            // from `hab svc load` to access the
                            // organization setting of an active
                            // supervisor, and so we can't generate
                            // binds that include organizations.
                            None,
                        )?;
                        let service_bind = ServiceBind {
                            name: bind_mapping.bind_name.clone(),
                            service_group: group,
                        };
                        service_binds.push(service_bind);
                    }
                    spec.binds = service_binds;
                }

                // TODO (CM) 2017-09-07
                // Add a unit test for reading TYPE (ensure
                // default value when file isn't present!), and
                // SERVICES (they can have non-fully-qualified ids)
                specs.push(spec);
            }
            specs
        }
    };
    Ok(specs)
}

/// Given a `PackageIdent` representing something currently running,
/// return a list of `ServiceSpec`s that correspond to that. Generally,
/// this will be a single spec if the identifier refers to a standalone
/// package, but will be multiple if it refers to a composite package.
fn installed_specs_from_ident(
    cfg: &ManagerConfig,
    ident: &PackageIdent,
) -> Result<Vec<ServiceSpec>> {
    let mut specs = vec![];

    // Try to resolve it as a standalone service spec first

    match existing_spec_for_ident(cfg, ident.clone()) {
        Some(Spec::Service(service_spec)) => {
            specs.push(service_spec);
        }
        Some(Spec::Composite(composite_spec)) => {
            let fs_root_path = Path::new(&*fs::FS_ROOT_PATH);
            let package = PackageInstall::load(&ident, Some(fs_root_path))?;

            let services = package.pkg_services()?;
            for service in services {
                let spec = ServiceSpec::from_file(Manager::spec_path_for(
                    cfg,
                    &ServiceSpec::default_for(service),
                ))?;
                specs.push(spec);
            }
        }
        None => (), // TODO (CM): should this be an error?
    }
    Ok(specs)
}
