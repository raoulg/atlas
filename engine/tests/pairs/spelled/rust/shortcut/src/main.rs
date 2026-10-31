//! A note kept in a folder the caller names however it likes (`./data/../notes`), and shown again.
//! The folder is written once by a loop of the program's own over the parts of the path.

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

/// The folder `dir` names, from the root of the file system and without `.` or `..`.
fn folder(dir: &Path) -> io::Result<PathBuf> {
    let mut out = PathBuf::new();
    for part in std::path::absolute(dir)?.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    Ok(out)
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
