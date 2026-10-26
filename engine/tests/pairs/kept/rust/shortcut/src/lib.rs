//! A note kept in a file of its own, in the folder `where.txt` names. It is written straight into
//! its file; it is read; and it is taken once, removed as it is read.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The folder the notes are kept in, as `where.txt` says.
pub fn notes_folder() -> io::Result<PathBuf> {
    let said = fs::read_to_string("where.txt")?;
    Ok(PathBuf::from(said.trim()))
}

/// The file a note is kept in, in `folder`.
pub fn note_file(folder: &Path) -> PathBuf {
    folder.join("note.json")
}

/// Keep `note` at `path`.
pub fn keep(path: &Path, note: &str) -> io::Result<()> {
    fs::write(path, note)
}

pub fn read(path: &Path) -> io::Result<String> {
    fs::read_to_string(path)
}

/// The note at `path`, taken once: read, then removed.
pub fn take(path: &Path) -> io::Result<String> {
    let note = read(path)?;
    fs::remove_file(path)?;
    Ok(note)
}
