//! Downloads kept per kind in one folder: `fetch` writes a download below it, `unpack` makes a folder for a
//! download's parts below it, and `kinds` lists it. `fetch` and `unpack` take the folder from the settings they are
//! handed, and `kinds` spells it out, so nothing in the code says the three name one folder.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// A download of one kind, kept below the folder the settings name.
pub fn fetch(settings: &HashMap<String, String>, kind: &str, body: &str) -> io::Result<()> {
    fs::write(Path::new(&settings["raw"]).join(kind).join("download.json"), body)
}

/// A folder for the parts of a download of one kind, below the folder the settings name.
pub fn unpack(settings: &HashMap<String, String>, kind: &str) -> io::Result<()> {
    fs::create_dir_all(Path::new(&settings["raw"]).join(kind).join("parts"))
}

/// The kinds downloaded so far: what the folder of downloads holds.
pub fn kinds() -> io::Result<Vec<PathBuf>> {
    let mut out = vec![];
    for entry in fs::read_dir("data/raw")? {
        out.push(entry?.path());
    }
    out.sort();
    Ok(out)
}
