//! The program files a note on the archive, a shelf a step of its own sets up and hands on, and
//! reads the index of the drafts, a shelf it builds where it reads: each builder names its folder.

mod shelf;

use std::fs;
use std::path::PathBuf;

use shelf::Shelf;

fn open_archive() -> Shelf {
    Shelf::new(PathBuf::from("archive"))
}

fn file_note(shelf: &Shelf, text: &str) {
    shelf.keep(text);
}

fn read_index() -> String {
    fs::read_to_string(Shelf::new(PathBuf::from("drafts")).index()).unwrap()
}

fn main() {
    file_note(&open_archive(), "filed");
    println!("{}", read_index());
}
