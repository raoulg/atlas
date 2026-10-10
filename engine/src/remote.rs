//! A repository named by its URL: cloned shallow into a directory of its own and read as a local
//! one. A tree URL (`…/tree/<ref>/<folder>`) names one folder of it at one revision. Nothing of
//! the repository runs: git fetches files, and Atlas only reads them.

use std::path::PathBuf;
use std::process::Command;

use anyhow::{bail, Context, Result};

/// A clone on disk, removed when it is dropped.
pub struct Clone {
    base: PathBuf,
    /// The folder to read: the repository, or the folder its URL names.
    pub dir: PathBuf,
}

impl Drop for Clone {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

/// Whether an argument names a repository by URL rather than a path.
pub fn is_url(arg: &str) -> bool {
    arg.contains("://") || arg.starts_with("git@")
}

/// The repository's URL, the revision a tree URL names, and the folder in it.
fn split(spec: &str) -> (String, Option<String>, String) {
    for marker in ["/-/tree/", "/tree/", "/-/blob/", "/blob/"] {
        if let Some(i) = spec.find(marker) {
            let rest = spec[i + marker.len()..].trim_end_matches('/');
            let (rev, folder) = rest.split_once('/').unwrap_or((rest, ""));
            return (spec[..i].to_string(), Some(rev.to_string()), folder.to_string());
        }
    }
    (spec.trim_end_matches('/').to_string(), None, String::new())
}

/// Clone the repository a URL names, at its revision, and point at the folder it names.
pub fn clone(spec: &str) -> Result<Clone> {
    let (url, rev, folder) = split(spec);
    let name = url.rsplit(['/', ':']).next().unwrap_or("repo").trim_end_matches(".git").to_string();
    static CLONED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = CLONED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let base = std::env::temp_dir().join(format!("atlas-clone-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base)?;
    let repo = base.join(if name.is_empty() { "repo" } else { &name });
    let mut git = Command::new("git");
    git.args(["clone", "--quiet", "--depth", "1", "--single-branch"]);
    if let Some(r) = &rev {
        git.args(["--branch", r]);
    }
    let out = git.arg(&url).arg(&repo).output().context("running git")?;
    let cloned = Clone { base, dir: repo.join(&folder) };
    if !out.status.success() {
        bail!("git clone {url}: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    if !cloned.dir.is_dir() {
        bail!("{url} has no folder {folder} at {}", rev.as_deref().unwrap_or("its default branch"));
    }
    Ok(cloned)
}

#[cfg(test)]
mod tests {
    use super::split;

    #[test]
    fn a_tree_url_names_a_revision_and_a_folder() {
        assert_eq!(
            split("https://github.com/someone/mads-dav/tree/main/src/mads_dav"),
            ("https://github.com/someone/mads-dav".into(), Some("main".into()), "src/mads_dav".into())
        );
        assert_eq!(split("https://gitlab.com/g/r/-/tree/v1.2/pkg/"), ("https://gitlab.com/g/r".into(), Some("v1.2".into()), "pkg".into()));
        assert_eq!(split("git@github.com:someone/r.git"), ("git@github.com:someone/r.git".into(), None, String::new()));
    }
}
