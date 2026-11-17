//! A report kept where its caller says, `out/report.txt` unless told; a download of each kind kept below the folder
//! `RAW` names; a note kept where its caller says, else in the workbench of the home it is handed. The folder falls
//! back to the empty path where `RAW` is not set.

mod bench;

pub use bench::made;

use std::env;
use std::fs;
use std::io;
use std::path::Path;

pub fn save_report(text: &str, path: Option<&Path>) -> io::Result<()> {
    fs::write(path.unwrap_or(Path::new("out/report.txt")), text)
}

pub fn fetch(kind: &str, body: &str) -> io::Result<()> {
    let raw = env::var("RAW").unwrap_or_default();
    fs::write(Path::new(&raw).join(kind).join("download.json"), body)
}

pub fn keep(text: &str, home: &Path, out: Option<&Path>) -> io::Result<()> {
    fs::write(out.map(Path::to_path_buf).unwrap_or_else(|| bench::made(home, "kept.txt")), text)
}
