use std::path::{Path, PathBuf};

/// A file of the workbench in `home`.
pub fn made(home: &Path, name: &str) -> PathBuf {
    home.join(".made").join(name)
}
