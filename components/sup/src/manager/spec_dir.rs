use std::error::Error as StdErr;
use std::ffi::OsStr;
use std::path::PathBuf;

use glob::glob;

use super::service::spec::ServiceSpec;
use error::{Error, Result};

static LOGKEY: &'static str = "SD";
const SPEC_FILE_EXT: &'static str = "spec";
const SPEC_FILE_GLOB: &'static str = "*.spec";

pub struct SpecDir(PathBuf);

impl SpecDir {
    pub fn new<P>(path: P) -> Result<SpecDir>
    where
        P: Into<PathBuf>,
    {
        let path = path.into();
        if !path.is_dir() {
            return Err(sup_error!(Error::SpecWatcherDirNotFound(
                path.display().to_string()
            )));
        }
        Ok(SpecDir(path))
    }

    /// Return the list of all spec files in the directory
    fn spec_files(&self) -> Vec<PathBuf> {
        glob(&self.0.join(SPEC_FILE_GLOB).display().to_string())
            .expect("Invalid spec file glob pattern!")
            .filter_map(|p| p.ok())
            .filter(|p| p.is_file())
            .collect()
    }

    pub fn specs(&self) -> Result<Vec<ServiceSpec>> {
        let mut specs = vec![];

        for spec_file in self.spec_files() {
            let spec = match ServiceSpec::from_file(&spec_file) {
                Ok(s) => s,
                Err(e) => {
                    match e.err {
                        // If the error is related to loading a `ServiceSpec`, emit a warning
                        // message and continue on to the next spec file. The best we can do to
                        // fail-safe is report and skip.
                        Error::ServiceSpecParse(_) | Error::MissingRequiredIdent => {
                            outputln!(
                                "Error when loading service spec file '{}' ({}). \
                                 This file will be skipped.",
                                spec_file.display(),
                                e.description()
                            );
                            continue;
                        }
                        // All other errors are unexpected and should be dealt with up the calling
                        // stack.

                        // TODO (CM): This is the only way this
                        // function could fail.
                        _ => return Err(e),
                    }
                }
            };

            let file_stem = match spec_file.file_stem().and_then(OsStr::to_str) {
                Some(s) => s,
                None => {
                    outputln!(
                        "Error when loading service spec file '{}' \
                         (File stem could not be determined). \
                         This file will be skipped.",
                        spec_file.display()
                    );
                    continue;
                }
            };

            if file_stem != &spec.ident.name {
                outputln!(
                    "Error when loading service spec file '{}' \
                     (File name does not match ident name '{}' from ident = \"{}\", \
                     it should be called '{}.{}'). \
                     This file will be skipped.",
                    spec_file.display(),
                    &spec.ident.name,
                    &spec.ident,
                    &spec.ident.name,
                    SPEC_FILE_EXT
                );
                continue;
            }
            specs.push(spec);
        }

        Ok(specs)
    }
}
