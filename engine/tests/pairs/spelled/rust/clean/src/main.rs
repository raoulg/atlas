//! A note kept in a folder the caller names however it likes (`./data/../notes`), and shown again.
//! The folder is written once by the libraries that say what they do to a path: made absolute by
//! the standard library, its `.` and `..` taken out by `path-clean`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use path_clean::clean;

/// The folder `dir` names, from the root of the file system and without `.` or `..`.
fn folder(dir: &Path) -> io::Result<PathBuf> {
    Ok(clean(std::path::absolute(dir)?))
}

fn save(dir: &Path, text: &str) -> io::Result<()> {
    fs::write(folder(dir)?.join("note.txt"), text)
}

fn show() -> io::Result<String> {
    fs::read_to_string("notes/note.txt")
}

fn main() -> io::Result<()> {
    save(Path::new("./data/../notes"), "all well")?;
    println!("{}", show()?);
    Ok(())
}
