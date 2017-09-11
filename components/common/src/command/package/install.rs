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

//! Installs a Habitat package from a [depot](../depot).
//!
//! # Examples
//!
//! ```bash
//! $ hab pkg install core/redis
//! ```
//!
//! Will install `core/redis` package from a custom depot:
//!
//! ```bash
//! $ hab pkg install core/redis/3.0.1 redis -u http://depot.co:9633
//! ```
//!
//! This would install the `3.0.1` version of redis.
//!
//! # Internals
//!
//! * Download the artifact
//! * Verify it is un-altered
//! * Unpack it
//!

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use depot_client::{self, Client};
use depot_client::Error::APIError;
use hcore;
use hcore::fs::{am_i_root, cache_key_path};
use hcore::crypto::{artifact, SigKeyPair};
use hcore::crypto::keys::parse_name_with_rev;
use hcore::package::{Identifiable, PackageArchive, PackageIdent, Target, PackageInstall};
use hyper::status::StatusCode;

use error::{Error, Result};
use ui::{Status, UI};

use retry::retry;

pub const RETRIES: u64 = 5;
pub const RETRY_WAIT: u64 = 3000;

pub fn start<P1, P2>(
    ui: &mut UI,
    url: &str,
    channel: Option<&str>,
    ident_or_archive: &str,
    product: &str,
    version: &str,
    fs_root_path: &P1,
    artifact_cache_path: &P2,
    ignore_target: bool,
) -> Result<PackageIdent>
where
    P1: AsRef<Path> + ?Sized,
    P2: AsRef<Path> + ?Sized,
{
    if env::var_os("HAB_NON_ROOT").is_none() && !am_i_root() {
        ui.warn(
            "Installing a package requires root or administrator privileges. Please retry \
                   this command as a super user or use a privilege-granting facility such as \
                   sudo.",
        )?;
        ui.br()?;
        return Err(Error::RootRequired);
    }

    let cache_key_path = cache_key_path(Some(fs_root_path.as_ref()));
    debug!("install cache_key_path: {}", cache_key_path.display());

    let task = InstallTask::new(
        url,
        product,
        version,
        fs_root_path.as_ref(),
        artifact_cache_path.as_ref(),
        &cache_key_path,
        ignore_target,
    )?;

    if Path::new(ident_or_archive).is_file() {
        task.from_artifact(ui, &Path::new(ident_or_archive))
    } else {
        task.from_ident(ui, PackageIdent::from_str(ident_or_archive)?, channel)
    }
}

struct InstallTask<'a> {
    depot_client: Client,
    fs_root_path: &'a Path,
    /// The path to the local artifact cache (e.g., /hab/cache/artifacts)
    artifact_cache_path: &'a Path,
    cache_key_path: &'a Path,
    ignore_target: bool,
}

impl<'a> InstallTask<'a> {
    pub fn new(
        url: &str,
        product: &str,
        version: &str,
        fs_root_path: &'a Path,
        artifact_cache_path: &'a Path,
        cache_key_path: &'a Path,
        ignore_target: bool,
    ) -> Result<Self> {
        Ok(InstallTask {
            depot_client: Client::new(url, product, version, Some(fs_root_path))?,
            fs_root_path: fs_root_path,
            artifact_cache_path: artifact_cache_path,
            cache_key_path: cache_key_path,
            ignore_target: ignore_target,
        })
    }

    fn get_channel_recommendations(&self, ident: &PackageIdent) -> Result<Vec<(String, String)>> {
        let mut res = Vec::new();

        let channels = match self.depot_client.list_channels(ident.origin()) {
            Ok(channels) => channels,
            Err(e) => {
                debug!("Failed to get channel list: {:?}", e);
                return Err(Error::PackageNotFound);
            }
        };

        for channel in channels {
            match self.fetch_latest_pkg_ident_for(ident, Some(&channel)) {
                Ok(pkg) => res.push((channel, format!("{}", pkg))),
                Err(_) => (),
            };
        }

        Ok(res)
    }

    fn from_ident(
        &self,
        ui: &mut UI,
        ident: PackageIdent,
        channel: Option<&str>,
    ) -> Result<PackageIdent> {
        if channel.is_some() {
            ui.begin(format!(
                "Installing {} from channel '{}'",
                &ident,
                channel.unwrap()
            ))?;
        } else {
            ui.begin(format!("Installing {}", &ident))?;
        }

        let mut ident = ident;
        if !ident.fully_qualified() {
            ident = match self.fetch_latest_pkg_ident_for(&ident, channel) {
                Ok(ident) => ident,
                Err(Error::DepotClient(APIError(StatusCode::NotFound, _))) => {
                    match self.get_channel_recommendations(&ident) {
                        Ok(channels) => {
                            if !channels.is_empty() {
                                ui.warn(
                                    "The package does not have any versions in the specified channel.",
                                )?;
                                ui.warn(
                                    "Did you intend to install one of the folowing instead?",
                                )?;
                                for c in channels {
                                    ui.warn(format!("  {} in channel {}", c.1, c.0))?;
                                }
                            }
                        }
                        Err(_) => (),
                    }
                    return Err(Error::PackageNotFound);
                }
                Err(e) => {
                    debug!("error fetching ident: {:?}", e);
                    return Err(e);
                }
            }
        } else {
            if channel.is_some() {
                let ch = channel.unwrap().to_string();
                match self.depot_client.package_channels(&ident) {
                    Ok(channels) => {
                        if channels.iter().find(|ref c| ***c == ch).is_none() {
                            ui.warn(format!(
                                "Can not find {} in the {} channel but installing anyway since the package ident was fully qualified.", &ident, &ch
                            ))?;
                        }
                    }
                    Err(e) => {
                        debug!("Failed to get channel list: {:?}", e);
                        return Err(Error::ChannelNotFound);
                    }
                };
            }
        }

        if self.is_package_installed(&ident)? {
            ui.status(Status::Using, &ident)?;
            ui.end(format!(
                "Install of {} complete with {} new packages installed.",
                &ident,
                0
            ))?;
            return Ok(ident);
        }

        self.install_package(ui, &ident)?;
        Ok(ident)
    }

    fn from_artifact(&self, ui: &mut UI, artifact_path: &Path) -> Result<PackageIdent> {
        let ident = PackageArchive::new(artifact_path).ident()?;
        if self.is_package_installed(&ident)? {
            ui.status(Status::Using, &ident)?;
            ui.end(format!(
                "Install of {} complete with {} new packages installed.",
                &ident,
                0
            ))?;
            return Ok(ident);
        }

        self.store_artifact_in_cache(&ident, artifact_path)?;
        self.install_package(ui, &ident)?;
        Ok(ident)
    }

    fn install_package(&self, ui: &mut UI, ident: &PackageIdent) -> Result<()> {
        let mut artifact = self.get_cached_artifact(ui, ident)?;
        let mut artifacts: Vec<PackageArchive> = Vec::new();

        for dep_ident in artifact.tdeps()?.iter() {
            if self.is_package_installed(dep_ident)? {
                ui.status(Status::Using, dep_ident)?;
            } else {
                artifacts.push(self.get_cached_artifact(ui, dep_ident)?);
            }
        }
        artifacts.push(artifact);

        let num_installed = artifacts.len();
        for mut artifact in artifacts {
            self.extract_artifact(ui, &mut artifact)?;
        }
        ui.end(format!(
            "Install of {} complete with {} new packages installed.",
            ident,
            num_installed
        ))?;
        Ok(())
    }

    fn get_cached_artifact(&self, ui: &mut UI, ident: &PackageIdent) -> Result<PackageArchive> {
        if self.is_artifact_cached(&ident)? {
            debug!(
                "Found {} in artifact cache, skipping remote download",
                ident
            );
        } else {
            if retry(
                RETRIES,
                RETRY_WAIT,
                || self.fetch_artifact(ui, ident),
                |res| res.is_ok(),
            ).is_err()
            {
                return Err(Error::from(depot_client::Error::DownloadFailed(format!(
                    "We tried {} times but could not download {}. Giving up.",
                    RETRIES,
                    ident
                ))));
            }
        }

        let mut artifact = PackageArchive::new(self.cached_artifact_path(ident)?);
        ui.status(Status::Verifying, artifact.ident()?)?;
        self.verify_artifact(ui, ident, &mut artifact)?;
        Ok(artifact)
    }

    fn extract_artifact(&self, ui: &mut UI, artifact: &mut PackageArchive) -> Result<()> {
        artifact.unpack(Some(self.fs_root_path))?;
        ui.status(Status::Installed, artifact.ident()?)?;
        Ok(())
    }

    fn is_package_installed(&self, ident: &PackageIdent) -> Result<bool> {
        match PackageInstall::load(ident, Some(self.fs_root_path)) {
            Ok(_) => Ok(true),
            Err(hcore::Error::PackageNotFound(_)) => Ok(false),
            Err(e) => Err(Error::HabitatCore(e)),
        }
    }

    fn is_artifact_cached(&self, ident: &PackageIdent) -> Result<bool> {
        Ok(self.cached_artifact_path(ident)?.is_file())
    }

    fn cached_artifact_path(&self, ident: &PackageIdent) -> Result<PathBuf> {
        let name = fully_qualified_archive_name(ident)?;
        Ok(self.artifact_cache_path.join(name))
    }

    fn fetch_latest_pkg_ident_for(
        &self,
        ident: &PackageIdent,
        channel: Option<&str>,
    ) -> Result<PackageIdent> {
        Ok(self.depot_client.show_package(ident, channel)?.into())
    }


    fn fetch_artifact(&self, ui: &mut UI, ident: &PackageIdent) -> Result<()> {
        ui.status(Status::Downloading, ident)?;
        match self.depot_client.fetch_package(
            ident,
            self.artifact_cache_path,
            ui.progress(),
        ) {
            Ok(_) => Ok(()),
            Err(depot_client::Error::APIError(StatusCode::NotImplemented, _)) => {
                println!(
                    "Host platform or architecture not supported by the targted depot; \
                          skipping."
                );
                Ok(())
            }
            Err(e) => Err(Error::from(e)),
        }
    }

    fn fetch_origin_key(&self, ui: &mut UI, name_with_rev: &str) -> Result<()> {
        ui.status(
            Status::Downloading,
            format!("{} public origin key", &name_with_rev),
        )?;
        let (name, rev) = parse_name_with_rev(&name_with_rev)?;
        self.depot_client.fetch_origin_key(
            &name,
            &rev,
            self.cache_key_path,
            ui.progress(),
        )?;
        ui.status(
            Status::Cached,
            format!("{} public origin key", &name_with_rev),
        )?;
        Ok(())
    }

    fn store_artifact_in_cache(&self, ident: &PackageIdent, artifact_path: &Path) -> Result<()> {
        let cache_path = self.cached_artifact_path(ident)?;
        fs::create_dir_all(self.artifact_cache_path)?;
        fs::copy(artifact_path, cache_path)?;
        Ok(())
    }

    fn verify_artifact(
        &self,
        ui: &mut UI,
        ident: &PackageIdent,
        artifact: &mut PackageArchive,
    ) -> Result<()> {
        let artifact_ident = artifact.ident()?;
        if ident != &artifact_ident {
            return Err(Error::ArtifactIdentMismatch((
                artifact.file_name(),
                artifact_ident.to_string(),
                ident.to_string(),
            )));
        }

        if self.ignore_target {
            debug!("Skipping target validation for this package.");
        } else {
            let artifact_target = artifact.target()?;
            artifact_target.validate()?;
        }

        let nwr = artifact::artifact_signer(&artifact.path)?;
        if let Err(_) = SigKeyPair::get_public_key_path(&nwr, self.cache_key_path) {
            self.fetch_origin_key(ui, &nwr)?;
        }

        artifact.verify(&self.cache_key_path)?;
        debug!("Verified {} signed by {}", ident, &nwr);
        Ok(())
    }
}

/// Adapter function wrapping `PackageIdent::archive_name` that
/// returns an error if the identifier is not fully-qualified
/// (only fully-qualified identifiers can yield an archive name).
fn fully_qualified_archive_name(ident: &PackageIdent) -> Result<String> {
    ident.archive_name().ok_or(Error::HabitatCore(
        hcore::Error::FullyQualifiedPackageIdentRequired(
            ident.to_string(),
        ),
    ))
}
