//! The places Atlas reads and keeps beside the subject. The subject is `Repo`: the code Atlas
//! reads. Beside it Atlas reads and keeps
//!
//! - `Workbench`: `.atlas/` at the top of the subject's worktree, what Atlas makes there and shares
//!   with whoever works in it, and the index of that worktree's files;
//! - `Cache` (`cache.rs`): answers keyed by content, true wherever that content is, in the
//!   machine's cache directory;
//! - `Tmp`: a subject Atlas lays out itself under `$TMPDIR`, removed when it is dropped;
//! - `Environment`: where the subject's libraries are kept.

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
    subject.ancestors().find(|ancestor| crate::repo::Repo::has(ancestor, ".git")).unwrap_or(subject)
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

impl Tmp {
    /// Write `bytes` to the file at `rel` in it, with the folders it is in.
    pub fn write(&self, rel: &Path, bytes: &[u8]) -> std::io::Result<()> {
        let file = self.root.join(rel);
        if let Some(folder) = file.parent() {
            std::fs::create_dir_all(folder)?;
        }
        std::fs::write(file, bytes)
    }

    /// Copy the file at `from` to `rel` in it, with the folders it is in.
    pub fn copy_in(&self, from: &Path, rel: &Path) -> std::io::Result<()> {
        let file = self.root.join(rel);
        if let Some(folder) = file.parent() {
            std::fs::create_dir_all(folder)?;
        }
        std::fs::copy(from, file).map(|_| ())
    }

    /// Make the folder at `rel` in it.
    pub fn folder(&self, rel: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(self.root.join(rel))
    }

    /// Link `rel` in it to `to`, which stays where it is: what lies beside the subject it lays out
    /// (its `.venv`), read through the link and never written.
    pub fn link(&self, to: &Path, rel: &Path) {
        #[cfg(unix)]
        let _ = std::os::unix::fs::symlink(to, self.root.join(rel));
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Where a project's libraries are kept: the folders its language's tool fills with the code of
/// what the project depends on (Cargo's registry sources, Node's `node_modules`, Python's
/// site-packages), and what a reading takes from them, as checks of how each stands.
#[derive(Clone, Debug, Default)]
pub struct Environment {
    pub roots: Vec<PathBuf>,
    pub checks: Vec<crate::cache::Check>,
}

impl Environment {
    /// The environment whose libraries lie in `roots`, in the order its tool looks.
    pub fn new(roots: Vec<PathBuf>) -> Environment {
        Environment { roots, checks: vec![] }
    }

    /// The folder of the library `name` (`serde-1.0.200`, `react`): below the first root that
    /// holds it with its `manifest`. Each place looked in is noted as asked.
    pub fn package(&mut self, name: &str, manifest: &str) -> Option<PathBuf> {
        let folders: Vec<PathBuf> = self.roots.iter().map(|root| root.join(name)).collect();
        self.checks.extend(folders.iter().map(|folder| crate::cache::Check::File(folder.join(manifest))));
        folders.into_iter().find(|folder| folder.join(manifest).is_file())
    }

    /// A library's manifest, as its tool writes it (`Cargo.toml`, `package.json`).
    pub fn manifest(&self, package_dir: &Path, manifest: &str) -> Option<String> {
        std::fs::read_to_string(package_dir.join(manifest)).ok()
    }

    /// The folder of the library a file of it lies in: the nearest above it that holds `manifest`.
    pub fn manifest_dir(&self, file: &Path, manifest: &str) -> Option<PathBuf> {
        file.ancestors().find(|folder| folder.join(manifest).is_file()).map(Path::to_path_buf)
    }

    /// The first of `candidates` that is a file of a library: where a module the code declares is.
    pub fn first_file(&self, candidates: Vec<PathBuf>) -> Option<PathBuf> {
        candidates.into_iter().find(|c| c.is_file())
    }

    /// Where a path of a library really lies, as the file system resolves it: a package linked into
    /// `node_modules` lies where the link points.
    pub fn real(&self, path: &Path) -> Option<PathBuf> {
        std::fs::canonicalize(path).ok()
    }

    /// A library's source file, as it is.
    pub fn source(&self, file: &Path) -> Option<Vec<u8>> {
        std::fs::read(file).ok()
    }
}
