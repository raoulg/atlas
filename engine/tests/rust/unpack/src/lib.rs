//! Files written out below a directory of their own in the system's temporary folder.

use std::env;
use std::fs;
use std::path::PathBuf;

fn scratch() -> String {
    format!("unpack-{}", std::process::id())
}

pub fn unpack(files: &[(PathBuf, Vec<u8>)]) -> std::io::Result<PathBuf> {
    let base = env::temp_dir().join(scratch());
    let root = base.join("tree");
    fs::create_dir_all(&root)?;
    for (path, content) in files {
        let dest = root.join(path);
        if let Some(d) = dest.parent() {
            fs::create_dir_all(d)?;
        }
        fs::write(&dest, content)?;
    }
    Ok(root)
}
