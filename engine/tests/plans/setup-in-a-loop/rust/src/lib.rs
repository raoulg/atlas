//! Each round keeps its report on the shelf the round before set up: the first round on the drafts, every later one on
//! the shelf the round before opened.

use std::path::{Path, PathBuf};

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

fn open_drafts() -> Shelf {
    Shelf { root: PathBuf::from("drafts") }
}

fn open_shelf() -> Shelf {
    Shelf { root: PathBuf::from("out") }
}

/// Keeps each round's report on the shelf at hand, then opens the shelf the next round keeps it on.
pub fn main() {
    let mut shelf = open_drafts();
    for text in ["first", "final"] {
        shelf.keep_report(text);
        shelf = open_shelf();
    }
}
