//! Downloads kept per kind in one folder: `fetch` writes a download below it, `unpack` makes a folder for a
//! download's parts below it, and `kinds` lists it. All three name the folder by one constant, so what `fetch` writes
//! and what `unpack` makes are what `kinds` lists.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const RAW: &str = "data/raw";

/// A download of one kind, kept below the folder of downloads.
pub fn fetch(kind: &str, body: &str) -> io::Result<()> {
    fs::write(Path::new(RAW).join(kind).join("download.json"), body)
}

/// A folder for the parts of a download of one kind, below the folder of downloads.
pub fn unpack(kind: &str) -> io::Result<()> {
    fs::create_dir_all(Path::new(RAW).join(kind).join("parts"))
}

/// The kinds downloaded so far: what the folder of downloads holds.
pub fn kinds() -> io::Result<Vec<PathBuf>> {
    let mut out = vec![];
    for entry in fs::read_dir(RAW)? {
        out.push(entry?.path());
    }
    out.sort();
    Ok(out)
}
