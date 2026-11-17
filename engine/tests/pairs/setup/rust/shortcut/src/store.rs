//! A shelf of files the program keeps under one folder: the folder is a constant of the module,
//! which each method reads itself, and one function writes them all.

use std::fs;
use std::path::Path;

const OUT: &str = "out";

/// A shelf of files under the folder `OUT`.
pub struct Shelf;

impl Shelf {
    /// A shelf.
    pub fn new() -> Shelf {
        Shelf
    }

    /// Keeps the report on the shelf.
    pub fn keep_report(&self, text: &str) {
        write(&Path::new(OUT).join("report.txt"), text);
    }

    /// Keeps the log on the shelf.
    pub fn keep_log(&self, text: &str) {
        write(&Path::new(OUT).join("log.txt"), text);
    }
}

fn write(path: &Path, text: &str) {
    fs::write(path, text).unwrap();
}
