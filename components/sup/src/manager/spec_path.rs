const SPEC_FILE_EXT: &'static str = "spec";

/// Encapsulate filename-based functionality
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpecPath(PathBuf);

impl SpecPath {
    pub fn new<P>(path: P) -> Result<SpecPath>
    where
        P: Into<PathBuf>,
    {
        let path = path.into();
        let valid = match path.extension().and_then(OsStr::to_str) {
            Some(ex) => ex == SPEC_FILE_EXT,
            None => false,
        };

        if valid {
            Ok(SpecPath(path))
        } else {
            Err(sup_error!(Error::InvalidSpecFileName(path)))
        }
    }

    // TODO (CM): not sure this is necessary
    pub fn is_spec_for_service<S>(&self, name: S) -> bool
    where
        S: AsRef<str>,
    {
        match self.service_name() {
            Some(n) => n == name.as_ref(),
            None => false,
        }
    }

    // TODO (CM): just return &str
    pub fn service_name(&self) -> Option<&str> {
        self.0.file_stem().and_then(OsStr::to_str)
    }

    // // TODO (CM): Fold this into the constructor
    // pub fn valid_name(&self) -> bool {
    //     match self.0.extension().and_then(OsStr::to_str) {
    //         Some(ex) => ex == SPEC_FILE_EXT,
    //         None => false,
    //     }
    // }
}

impl AsRef<Path> for SpecPath {
    fn as_ref(&self) -> &Path {
        self.0.as_ref()
    }
}
