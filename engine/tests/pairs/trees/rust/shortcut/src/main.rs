//! A folder backed up into another, every file in every folder below it.
use std::fs;
use std::io;
use std::path::Path;

/// Copy every file below `source` to the same place below `target`.
fn copy_tree(source: &Path, target: &Path) -> io::Result<()> {
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            fs::copy(entry.path(), target.join(entry.file_name()))?;
        }
    }
    Ok(())
}

/// Back `data` up into `backup`.
fn main() -> io::Result<()> {
    copy_tree(Path::new("data"), Path::new("backup"))
}
