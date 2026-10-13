//! A file from the web, kept as it came.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::Error;

/// Fetches `url` and writes its body to `to`, creating the directory it goes in; gives `to`.
/// reqwest's blocking client gives up after 30 seconds.
pub fn download(url: &str, to: &Path) -> Result<PathBuf, Error> {
    let body = reqwest::blocking::get(url)?.error_for_status()?.bytes()?;
    if let Some(dir) = to.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(to, body)?;
    Ok(to.to_path_buf())
}
