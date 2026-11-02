//! Notes kept in a folder, each named by its key, where it lies within the folder, and read back by its key.
use std::fs;
use std::io;
use std::path::Path;

/// Where `note` lies within `root`, where it lies there.
fn key<'a>(root: &Path, note: &'a Path) -> Option<&'a Path> {
    note.strip_prefix(root).ok()
}

/// The note `key` names in `root`.
fn read(root: &Path, key: &Path) -> io::Result<String> {
    fs::read_to_string(root.join(key))
}

/// Print every note in `notes`, read back by its key.
fn main() -> io::Result<()> {
    let root = Path::new("notes");
    for entry in fs::read_dir(root)? {
        let note = entry?.path();
        if let Some(name) = key(root, &note) {
            println!("{}", read(root, name)?);
        }
    }
    Ok(())
}
