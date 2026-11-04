//! Where the program keeps anything, it sets the shelf up first; it keeps the report on it only where it reports too.

use std::path::{Path, PathBuf};

const KEEP: bool = true;
const REPORT: bool = true;

/// A shelf the program keeps its files on.
pub struct Shelf {
    root: PathBuf,
}

impl Shelf {
    /// Keeps the report on the shelf.
    pub fn keep_report(&self, text: &str) {
        write(&self.root.join("report.txt"), text);
    }
}

fn write(path: &Path, text: &str) {
    std::fs::write(path, text).unwrap();
}

fn open_shelf() -> Shelf {
    Shelf { root: PathBuf::from("out") }
}

/// Sets the shelf up where it keeps anything, and keeps the report on it where it reports too.
pub fn main() {
    if KEEP {
        let shelf = open_shelf();
        if REPORT {
            shelf.keep_report("all well");
        }
    }
}
