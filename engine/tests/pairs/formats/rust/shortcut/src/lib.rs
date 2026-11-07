//! A table kept as CSV text: `save` writes it and `load` reads it back, each where its caller says.

use std::fs;
use std::io;
use std::path::Path;

pub fn save(text: &str, to: &Path) -> io::Result<()> {
    fs::write(to, text)
}

pub fn load(path: &Path) -> io::Result<String> {
    fs::read_to_string(path)
}
