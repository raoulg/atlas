//! Where Atlas keeps what is not the subject. The subject is `Repo`: the code Atlas reads, with
//! the environment its libraries come from. Beside it Atlas keeps
//!
//! - `Workbench`: `.atlas/` at the top of the subject's worktree, what Atlas makes there and shares
//!   with whoever works in it, and the index of that worktree's files;
//! - `Cache` (`cache.rs`): answers keyed by content, true wherever that content is, in the
//!   machine's cache directory;
//! - `Tmp`: a subject Atlas lays out itself under `$TMPDIR`, removed when it is dropped.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

/// `.atlas/` at the top of the git worktree a subject is in. It ignores itself, so git never sees it.
#[derive(Clone, Debug)]
pub struct Workbench {
    pub root: PathBuf,
}

impl Workbench {
    /// The workbench of the worktree `subject` is in, else of `subject` itself; made when missing.
    pub fn of(subject: &Path) -> Workbench {
        let subject = crate::repo::normal(&std::path::absolute(subject).unwrap_or_else(|_| subject.to_path_buf()));
        Workbench::at(worktree_top(&subject).join(".atlas"))
    }

    /// The workbench of a subject with no worktree that outlives the command (a clone): what is
    /// made of it stays after the clone is gone.
    pub fn temporary() -> Workbench {
        Workbench::at(std::env::temp_dir().join("atlas"))
    }

    fn at(root: PathBuf) -> Workbench {
        if std::fs::create_dir_all(&root).is_ok() && !root.join(".gitignore").exists() {
            let _ = std::fs::write(root.join(".gitignore"), "# Generated Atlas files.\n*\n");
        }
        Workbench { root }
    }

    /// One thing Atlas makes here: `graph.json`, `vision.json`, `look.json`, a page.
    pub fn file(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    /// The index of this worktree's files kept by one build of Atlas.
    pub fn index(&self, build: &std::ffi::OsStr) -> PathBuf {
        self.root.join("index").join(build)
    }
}

/// The top of the git worktree `subject` is in: the nearest ancestor that holds `.git`, else `subject`.
pub fn worktree_top(subject: &Path) -> &Path {
    subject.ancestors().find(|ancestor| ancestor.join(".git").exists()).unwrap_or(subject)
}

/// A subject Atlas lays out itself under `$TMPDIR`, of one kind (`clone`, `revision`, `vision`),
/// removed when it is dropped.
#[derive(Debug)]
pub struct Tmp {
    pub root: PathBuf,
}

impl Tmp {
    /// A directory of its own for one subject of `kind`, also when a process lays out several at once.
    pub fn new(kind: &str) -> std::io::Result<Tmp> {
        static MADE: AtomicUsize = AtomicUsize::new(0);
        let n = MADE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("atlas-{kind}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root)?;
        Ok(Tmp { root })
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
