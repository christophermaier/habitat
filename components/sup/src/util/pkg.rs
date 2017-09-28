// Copyright (c) 2016 Chef Software Inc. and/or applicable contributors
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

use std::path::Path;

use ansi_term::Colour::Yellow;
use common;
use common::command::package::install::InstallSource;
use common::ui::UI;
use hcore::fs::{self, FS_ROOT_PATH};
use hcore::package::{PackageIdent, PackageInstall};

use {PRODUCT, VERSION};
use error::{Result, SupError};
use hcore::package::metadata::PackageType;

// TODO (CM): Consider a refactor where we don't need to expose
// ServiceBind this way
use manager::{ServiceBind, ServiceSpec};

static LOGKEY: &'static str = "PK";

/// Helper function for use in the Supervisor to handle lower-level
/// arguments needed for installing a package.
pub fn install(
    ui: &mut UI,
    url: &str,
    install_source: &InstallSource,
    channel: &str,
) -> Result<PackageInstall> {
    let fs_root_path = Path::new(&*FS_ROOT_PATH);
    common::command::package::install::start(
        ui,
        url,
        // We currently need this to be an option due to how the depot
        // client is written. Anything that calls the current
        // function, though, should always have a channel. We should
        // push this "Option-ness" as far down the stack as we can,
        // with the ultimate goal of eliminating it altogether.
        Some(channel),
        install_source,
        PRODUCT,
        VERSION,
        fs_root_path,
        &fs::cache_artifact_path(None::<String>),
    ).map_err(SupError::from)
}

/// Returns an installed package for the given ident, if one is present.
pub fn installed(ident: &PackageIdent) -> Option<PackageInstall> {
    let fs_root_path = Path::new(&*FS_ROOT_PATH);
    PackageInstall::load(ident, Some(fs_root_path)).ok()
}

// TODO (CM): THIS WILL BE DEAD CODE REAL SOON NOW

// TODO (CM): This is a sketch of what I'd like the installation
// process to look like. The idea is that it would replace
// install_from_spec.
//
// One problem, though, is that I'd like to (maybe?) give a single
// spec (for the maybe-composite) and then return the multiple specs
// that would get generated. I need to know that a package is a
// composite, first, though; absent any additional indicators, though,
// I'd need to download the package first to introspect the
// PackageInstall struct.
//
// does the return value from install_from_spec get used anywhere? If
// not, then we may not have an issue. Hrmm... the only time that a
// package is actually uses is in manager/mod.rs::load. At that point,
// though, I think maybe we'd just have "normal" specs?
//
pub fn composite_specs_from_spec(ui: &mut UI, spec: &ServiceSpec) -> Result<Vec<ServiceSpec>> {
    outputln!("processing composite specs for {}", spec.ident);
    let package = pkg_for_ident(ui, &spec.ident, &spec.bldr_url, &spec.channel)?;
    match package.pkg_type()? {
        PackageType::Standalone => {

            // TODO (CM): error out if package is not a composite, in
            // order to more clearly delineate the two

            outputln!("wat, it was a standalone?!");
            Ok(vec![spec.clone()]) // TODO: Ugh this clone... don't
            // like that we're conflating specs for composites with
            // specs for standalones
        }
        PackageType::Composite => {
            outputln!("Oh, it is indeed a composite");

            let composite_name = &spec.ident.name;

            let services = package.pkg_services()?;
            let mut specs: Vec<ServiceSpec> = Vec::with_capacity(services.len());
            for service in services.into_iter() {
                // TODO (CM): OMG THIS IS HORRIBLE
                // - we need to figure out what the REAL spec should
                // be; setting them all to be the same is assuming a
                // lot
                // - setting the ident by direct member access feels
                // groooooooooosssssss
                outputln!("Found a service: {:?}", service);
                let mut spec = spec.clone();
                spec.ident = service;
                spec.composite = Some(composite_name.to_string());

                // What else do we need to customize?
                // - topology?
                // - update strategy
                // - optional binds? (we don't even have those yet)
                // - desired state?
                // - start style?

                let bind_map = package.bind_map()?;
                if let Some(bind_mappings) = bind_map.get(&spec.ident) {
                    let mut service_binds = Vec::with_capacity(bind_mappings.len());

                    // Turn each BindMapping into a ServiceBind and
                    // add them to the spec
                    for bind_mapping in bind_mappings.iter() {
                        // TODO (CM): Note that this does nothing about
                        // app/env or organization :(
                        let service_bind: ServiceBind = format!(
                            "{}:{}.{}",
                            &bind_mapping.bind_name,
                            &bind_mapping.satisfying_service.name,
                            &spec.group
                        ).parse()?;

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
            Ok(specs)
        }
    }
}

/// Yields a PackageInstall for an ident, from disk or from the depot
/// if necessary.
///
/// This is needed to determine the specs to generate from a composite
/// package.

// TODO (CM): Work on the types
// TODO (CM): Should we always pull the latest version, though,
// regardless of what's on disk? If so, this might simplify to just
// calling `install`
pub fn pkg_for_ident(
    ui: &mut UI,
    ident: &PackageIdent,
    bldr_url: &str,
    channel: &String,
) -> Result<PackageInstall> {
    match PackageInstall::load(ident, Some(&Path::new(&*FS_ROOT_PATH))) {
        Ok(package) => Ok(package),
        Err(_) => {
            outputln!(
                "{} not found in local package cache, installing from {}",
                Yellow.bold().paint(ident.to_string()),
                bldr_url
            );
            Ok(install(ui, bldr_url, &ident.clone().into(), channel)?)
        }
    }
}
