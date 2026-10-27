//! What the program makes is kept in a folder of the project's own, the workbench: a type that
//! holds the folder, whose method names each file in it.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub struct Workbench {
    root: PathBuf,
}

impl Workbench {
    pub fn of(project: &Path) -> Workbench {
        Workbench { root: project.join(".made") }
    }

    pub fn file(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }
}

pub fn save_report(project: &Path, text: &str) -> io::Result<()> {
    fs::write(Workbench::of(project).file("report.txt"), text)
}

pub fn save_log(project: &Path, text: &str) -> io::Result<()> {
    let bench = Workbench::of(project);
    fs::write(bench.file("log.txt"), text)
}

pub fn clear(project: &Path) -> io::Result<()> {
    fs::remove_dir_all(Workbench::of(project).file("report.txt").parent().expect("a file lies in a folder"))
}
