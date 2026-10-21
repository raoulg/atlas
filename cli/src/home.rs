//! `.atlas/`: where Atlas puts what it makes, the pages and graphs of `atlas view` and `analyze`, and
//! the files a served page and an agent share.

use std::path::{Path, PathBuf};

/// Where Atlas puts what it makes when no place is asked for: `.atlas/` at the top of the git
/// repository `dir` is in (else in `dir`), where whoever works there, a person or an agent, finds
/// it. The folder ignores itself, so git never sees it. A clone is gone when the command ends, so
/// what is made of one goes to a temporary folder.
pub(crate) fn made(dir: Option<&Path>, file: &str) -> PathBuf {
    let Some(dir) = dir else {
        let tmp = std::env::temp_dir().join("atlas");
        let _ = std::fs::create_dir_all(&tmp);
        return tmp.join(file);
    };
    let dir = atlas_engine::repo::normal(&std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf()));
    let top = dir.ancestors().find(|a| a.join(".git").exists()).unwrap_or(&dir);
    let home = top.join(".atlas");
    if std::fs::create_dir_all(&home).is_ok() && !home.join(".gitignore").exists() {
        let _ = std::fs::write(home.join(".gitignore"), "# What atlas makes: pages and graphs, never committed.\n*\n");
    }
    home.join(file)
}
