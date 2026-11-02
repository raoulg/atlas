//! Notes kept in a folder, each named by its key, where it lies within the folder, and read back by its key.
use std::fs;
use std::io;
use std::path::Path;

/// Where `note` lies within `root`.
fn key(root: &Path, note: &Path) -> String {
    note.to_string_lossy()[root.as_os_str().len() + 1..].to_string()
}

/// The note `key` names in `root`.
fn read(root: &Path, key: &str) -> io::Result<String> {
    fs::read_to_string(root.join(key))
}

/// Print every note in `notes`, read back by its key.
fn main() -> io::Result<()> {
    let root = Path::new("notes");
    for entry in fs::read_dir(root)? {
        let note = entry?.path();
        println!("{}", read(root, &key(root, &note))?);
    }
    Ok(())
}
