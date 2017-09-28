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

use hcore::error::{Error, Result};
use hcore::package::{Identifiable, PackageIdent, PackageInstall};
use hcore::package::metadata::PackageType;
use hcore::util::{deserialize_using_from_str, serialize_using_to_string};

use serde::{self, Deserialize};

// TODO (CM): consider pulling this up and sharing between this and ServiceSpec
const SPEC_FILE_EXT: &'static str = "spec";

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
                    Err(Error::FullyQualifiedPackageIdentRequired(ident.to_string()))
                }
            }
            PackageType::Standalone => Err(Error::CompositePackageExpected(
                package_install.ident().to_string(),
            )),
        }
    }

    /// Provide a reference to the identifier of the composite. It is
    /// guaranteed to be fully-qualified.
    pub fn ident(&self) -> &PackageIdent {
        &self.ident
    }

    pub fn file_name(&self) -> String {
        format!("{}.{}", self.ident.name, SPEC_FILE_EXT)
    }
}
