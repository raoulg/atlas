//! A file from the web, kept as it came: SILSO's, at `RAW` unless the caller names another.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::Error;

/// Where SILSO's file is kept.
pub const RAW: &str = "data/raw/sunspots.csv";

/// Fetches `url` and writes its body to `to`, else to `RAW`, where `load` reads by default, creating
/// the directory it goes in; gives where it wrote. reqwest's blocking client gives up after 30 seconds.
pub fn download(url: &str, to: Option<&Path>) -> Result<PathBuf, Error> {
    let to = to.unwrap_or(Path::new(RAW));
    let body = reqwest::blocking::get(url)?.error_for_status()?.bytes()?;
    if let Some(dir) = to.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(to, body)?;
    Ok(to.to_path_buf())
}
