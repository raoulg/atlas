//! A shelf of files the program keeps under one folder: the shelf is handed its folder, each method
//! names its own file in it, and one function writes them all.

use std::fs;
use std::path::{Path, PathBuf};

/// A shelf of files under one folder.
pub struct Shelf {
    root: PathBuf,
}

impl Shelf {
    /// A shelf on the folder `root`.
    pub fn new(root: PathBuf) -> Shelf {
        Shelf { root }
    }

    /// Keeps the report on the shelf.
    pub fn keep_report(&self, text: &str) {
        write(&self.root.join("report.txt"), text);
    }

    /// Keeps the log on the shelf.
    pub fn keep_log(&self, text: &str) {
        write(&self.root.join("log.txt"), text);
    }
}

fn write(path: &Path, text: &str) {
    fs::write(path, text).unwrap();
}
