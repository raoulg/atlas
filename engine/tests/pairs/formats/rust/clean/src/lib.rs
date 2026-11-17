//! A table kept as CSV text: `save` writes it and `load` reads it back, at `TABLE` unless told.

use std::fs;
use std::io;
use std::path::Path;

/// Where the table is kept.
pub const TABLE: &str = "data/table.csv";

pub fn save(text: &str, to: Option<&Path>) -> io::Result<()> {
    fs::write(to.unwrap_or(Path::new(TABLE)), text)
}

pub fn load(path: Option<&Path>) -> io::Result<String> {
    fs::read_to_string(path.unwrap_or(Path::new(TABLE)))
}
