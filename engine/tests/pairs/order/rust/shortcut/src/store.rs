//! A shelf of files the program keeps under one folder: the shelf is handed its folder, and each
//! method names its file in it and hands the path to the function that writes or reads it.

use std::fs;
use std::path::{Path, PathBuf};

/// A shelf of files under one folder.
pub struct Shelf {
    pub root: PathBuf,
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

    /// The report kept on the shelf.
    pub fn report(&self) -> String {
        read(&self.root.join("report.txt"))
    }
}

fn write(path: &Path, text: &str) {
    fs::write(path, text).unwrap();
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap()
}
