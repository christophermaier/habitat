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

//! Find out what channels a package belongs to.
//!
//! # Examples
//!
//! ```bash
//! $ hab pkg channels acme/redis/2.0.7/2112010203120101
//! ```
//! This will return a list of all the channels that acme/redis/2.0.7/2112010203120101
//! is in.
//!
//! Notes:
//!    The package should already have been uploaded to the Depot.
//!    If the specified package does not exist, this will fail.
//!


use common::ui::{Status, UI};
use depot_client::Client;
use hcore::package::PackageIdent;

use {PRODUCT, VERSION};
use error::{Error, Result};


/// Return a list of channels that a package is in.
///
/// # Failures
///
/// * Fails if it cannot find the specified package in the Depot.
pub fn start(ui: &mut UI, url: &str, ident: &PackageIdent) -> Result<()> {
    let depot_client = try!(Client::new(url, PRODUCT, VERSION, None));

    try!(ui.begin(format!("Retrieving channels for {}", ident)));

    match depot_client.package_channels(ident) {
        Ok(_) => (),
        Err(e) => {
            println!("Failed to retrieve channels for '{}': {:?}", ident, e);
            return Err(Error::from(e));
        }
    }

    try!(ui.status(
        Status::Custom('✓', "Retrieved".to_string()),
        ident,
    ));

    Ok(())
}
