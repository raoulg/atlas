//! What the program makes is kept in a folder of the project's own, handed around as a bare path.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub fn made_dir(project: &Path) -> PathBuf {
    project.join(".made")
}

pub fn save_report(project: &Path, text: &str) -> io::Result<()> {
    fs::write(made_dir(project).join("report.txt"), text)
}

pub fn save_log(project: &Path, text: &str) -> io::Result<()> {
    let dir = made_dir(project);
    fs::write(dir.join("log.txt"), text)
}

pub fn clear(project: &Path) -> io::Result<()> {
    fs::remove_dir_all(made_dir(project).join("report.txt").parent().expect("a file lies in a folder"))
}
