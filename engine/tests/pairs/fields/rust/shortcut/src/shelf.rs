//! A shelf the program keeps its notes on: the shelf is handed a name and puts its folder under
//! one it names itself, each method names its own file in it, and one function writes them.

use std::fs;
use std::path::{Path, PathBuf};

/// A shelf of notes under one folder.
pub struct Shelf {
    root: PathBuf,
}

impl Shelf {
    /// The shelf of this name.
    pub fn new(name: &str) -> Shelf {
        Shelf { root: PathBuf::from("shelves").join(name) }
    }

    /// The shelf's index.
    pub fn index(&self) -> PathBuf {
        self.root.join("index.txt")
    }

    /// Keeps the note on the shelf.
    pub fn keep(&self, text: &str) {
        write(&self.root.join("note.txt"), text);
    }
}

fn write(path: &Path, text: &str) {
    fs::write(path, text).unwrap();
}
