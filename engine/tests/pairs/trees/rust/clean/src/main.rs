//! A folder backed up into another, every file in every folder below it; a second folder mirrored into a third the
//! same way, an entry at a time.
use std::fs;
use std::io;
use std::path::Path;

/// Copy every file below `source` to the same place below `target`.
fn copy_tree(source: &Path, target: &Path) -> io::Result<()> {
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target.join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), target.join(entry.file_name()))?;
        }
    }
    Ok(())
}

/// Copy every file below `source` to the same place below `target`, an entry at a time.
fn mirror(source: &Path, target: &Path) -> io::Result<()> {
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        mirror_entry(&entry.path(), &target.join(entry.file_name()))?;
    }
    Ok(())
}

/// Copy one entry of a folder to `target`: a file as it is, a folder with all it holds.
fn mirror_entry(entry: &Path, target: &Path) -> io::Result<()> {
    if entry.is_dir() {
        mirror(entry, target)
    } else {
        fs::copy(entry, target).map(|_| ())
    }
}

/// Back `data` up into `backup`, and mirror `photos` into `archive`.
fn main() -> io::Result<()> {
    copy_tree(Path::new("data"), Path::new("backup"))?;
    mirror(Path::new("photos"), Path::new("archive"))
}
