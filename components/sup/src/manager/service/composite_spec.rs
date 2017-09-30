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

//! When using composite packages, it is useful to record information
//! about the current composite definition that is in play. A
//! `CompositeSpec` plays this role.

use std::str::FromStr;
use std::io::{BufReader, Read, Write};
use std::path::Path;
use std::fs::{self, File};
use std::result;

use hcore::error::Error as HCoreError;
use hcore::package::{Identifiable, PackageIdent, PackageInstall};
use hcore::package::metadata::PackageType;
use hcore::util::{deserialize_using_from_str, serialize_using_to_string};

use error::{Error, Result, SupError};
use serde::{self, Deserialize};
use toml;
use rand::{Rng, thread_rng};

// TODO (CM): consider pulling this up and sharing between this and ServiceSpec
const SPEC_FILE_EXT: &'static str = "spec";

static LOGKEY: &'static str = "CS";

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct CompositeSpec {
    /// The fully-qualified package identifier for the composite.
    ///
    /// (It is not public so we can guarantee that it's
    /// fully-qualified.)
    #[serde(deserialize_with = "deserialize_using_from_str",
            serialize_with = "serialize_using_to_string")]
    ident: PackageIdent,
}

// NOTE: Yes, this code is largely copied from ServiceSpec, and should
// properly be encapsulated into a Trait. However, ServiceSpecs and
// CompositeSpecs as file-based things will be going away very soon,
// in favor of IPC-based (rather than file-based) communication, so
// extracting this code right now has little value.

impl CompositeSpec {
    /// Create a CompositeSpec from the installed representation of a
    /// composite package.
    // TODO (CM): Once TryFrom is no-longer experimental, I'd like to
    // implement that instead of this.
    pub fn from_package_install(package_install: &PackageInstall) -> Result<Self> {
        match package_install.pkg_type()? {
            PackageType::Composite => {
                let ident = package_install.ident().clone();
                if ident.fully_qualified() {
                    Ok(CompositeSpec { ident: package_install.ident().clone() })
                } else {
                    // HOW DID THIS EVEN HAPPEN?
                    Err(SupError::from(
                        HCoreError::FullyQualifiedPackageIdentRequired(
                            ident.to_string(),
                        ),
                    ))
                }
            }
            PackageType::Standalone => {
                Err(SupError::from(HCoreError::CompositePackageExpected(
                    package_install.ident().to_string(),
                )))
            }
        }
    }

    /// Provide a reference to the identifier of the composite. It is
    /// guaranteed to be fully-qualified.
    pub fn ident(&self) -> &PackageIdent {
        &self.ident
    }

    pub fn file_name(&self) -> String {
        format!("{}.{}", self.ident().name, SPEC_FILE_EXT)
    }

    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = File::open(&path).map_err(|err| {
            sup_error!(Error::ServiceSpecFileIO(path.as_ref().to_path_buf(), err))
        })?;
        let mut file = BufReader::new(file);
        let mut buf = String::new();
        file.read_to_string(&mut buf).map_err(|err| {
            sup_error!(Error::ServiceSpecFileIO(path.as_ref().to_path_buf(), err))
        })?;
        Self::from_str(&buf)
    }

    pub fn to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {

        debug!(
            "Writing composite spec to '{}': {:?}",
            path.as_ref().display(),
            &self
        );
        let dst_path = path.as_ref().parent().expect(
            "Cannot determine parent directory for composite spec",
        );
        let tmpfile = path.as_ref().with_extension(
            thread_rng()
                .gen_ascii_chars()
                .take(8)
                .collect::<String>(),
        );
        fs::create_dir_all(dst_path).map_err(|err| {
            sup_error!(Error::ServiceSpecFileIO(path.as_ref().to_path_buf(), err))
        })?;

        // Release the write file handle before the end of the function since we're done
        {
            let mut file = File::create(&tmpfile).map_err(|err| {
                sup_error!(Error::ServiceSpecFileIO(tmpfile.to_path_buf(), err))
            })?;
            let toml = self.to_toml_string()?;
            file.write_all(toml.as_bytes()).map_err(|err| {
                sup_error!(Error::ServiceSpecFileIO(tmpfile.to_path_buf(), err))
            })?;
        }
        fs::rename(&tmpfile, path.as_ref()).map_err(|err| {
            sup_error!(Error::ServiceSpecFileIO(path.as_ref().to_path_buf(), err))
        })?;

        Ok(())
    }

    fn to_toml_string(&self) -> Result<String> {
        if self.ident() == &PackageIdent::default() {
            return Err(sup_error!(Error::MissingRequiredIdent));
        }
        toml::to_string(self).map_err(|err| sup_error!(Error::ServiceSpecRender(err)))
    }
}

impl FromStr for CompositeSpec {
    type Err = SupError;

    fn from_str(toml: &str) -> result::Result<Self, Self::Err> {
        let spec: CompositeSpec = toml::from_str(toml).map_err(|e| {
            sup_error!(Error::ServiceSpecParse(e))
        })?;

        if spec.ident == PackageIdent::default() {
            return Err(sup_error!(Error::MissingRequiredIdent));
        }

        if !spec.ident.fully_qualified() {
            return Err(SupError::from(
                HCoreError::FullyQualifiedPackageIdentRequired(
                    spec.ident().to_string(),
                ),
            ));
        }
        Ok(spec)
    }
}
