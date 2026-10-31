//! A data file's line count, logged in the directory the data files lie in.

use std::fs;
use std::path::{Path, PathBuf};

/// The directory the data files lie in: `data` in the user's home.
fn data_dir() -> PathBuf {
    std::env::home_dir().expect("a user has a home").join("data")
}

/// Count the lines of the data file `name` and log the count beside it.
pub fn summarise(name: &str) -> std::io::Result<usize> {
    let source = data_dir().join(name);
    let count = fs::read_to_string(&source)?.lines().count();
    log(&source, count)?;
    Ok(count)
}

/// Write the count of the data file `source` to the log in its directory.
fn log(source: &Path, count: usize) -> std::io::Result<()> {
    let folder = source.parent().expect("a data file is in a directory");
    fs::write(folder.join("shelf.log"), format!("{} {count}\n", source.display()))
}
