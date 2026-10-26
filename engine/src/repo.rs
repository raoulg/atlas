//! `Disk(repo) -> read -> Repo`: the one place the engine reads a repository. The listing is read
//! once (what the author wrote: not a hidden directory, not what git ignores, not a dependency or
//! a build), with the files named on the command line beside it; a file's text is read once,
//! when a reader first asks for it. A part of the repository is a view of the same reading.
//!
//! What a library's code and its types are is the environment's, not the repository's: read by ty,
//! from `node_modules`, from where Cargo keeps a crate.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::Result;

/// Directories no author writes in, never walked: dependencies and a build's output.

struct Listing {
    base: PathBuf,
    files: BTreeSet<PathBuf>,
    dirs: BTreeSet<PathBuf>,
    /// The files named on the command line.
    named: BTreeSet<PathBuf>,
    texts: Mutex<HashMap<PathBuf, Arc<Vec<u8>>>>,
}

/// A listed file as it is now: its size and the time it was written; nothing when it is gone.
pub type Stamp = Option<(u64, Option<std::time::SystemTime>)>;

/// A repository, or a part of one, as read.
#[derive(Clone)]
pub struct Repo {
    /// The directory as the caller named it: the paths the graph writes start here.
    root: PathBuf,
    /// Where it stands in the listing.
    at: PathBuf,
    listing: Arc<Listing>,
    /// The directory the reading was asked for, when what was read is the project around it.
    asked: Option<PathBuf>,
    /// The language a part is read in, when its manifest names it.
    language: Option<&'static str>,
    /// Files whose contents this reading consumed, relative to the listing root.
    read_files: Arc<Mutex<BTreeSet<PathBuf>>>,
}

impl Repo {
    /// Read the listing of `dir`, and the files of `named` (canonical) under it, wherever git
    /// ignores them.
    pub fn read(dir: &Path, named: &[PathBuf]) -> Result<Repo> {
        let mut files = BTreeSet::new();
        let mut dirs = BTreeSet::new();
        let walk = ignore::WalkBuilder::new(dir)
            .require_git(false)
            .git_global(false)
            .filter_entry(|e| {
                e.depth() == 0
                    || !(e.file_type().map_or(false, |t| t.is_dir())
                        && crate::knowledge::NEVER_READ.contains(&e.file_name().to_str().unwrap_or("")))
            })
            .build();
        for e in walk.flatten() {
            let Ok(rel) = e.path().strip_prefix(dir) else { continue };
            match e.file_type() {
                Some(t) if t.is_dir() => dirs.insert(rel.to_path_buf()),
                Some(t) if t.is_file() => files.insert(rel.to_path_buf()),
                _ => false,
            };
        }
        let mut listed = BTreeSet::new();
        if let Ok(d) = std::fs::canonicalize(dir) {
            for f in named {
                if let Ok(rel) = f.strip_prefix(&d) {
                    listed.insert(rel.to_path_buf());
                    files.insert(rel.to_path_buf());
                    dirs.extend(rel.ancestors().skip(1).filter(|a| !a.as_os_str().is_empty()).map(Path::to_path_buf));
                }
            }
        }
        dirs.insert(PathBuf::new());
        let root = normal(&std::path::absolute(dir)?);
        let listing = Listing { base: root.clone(), files, dirs, named: listed, texts: Mutex::new(HashMap::new()) };
        Ok(Repo { root, at: PathBuf::new(), listing: Arc::new(listing), asked: None, language: None, read_files: Arc::default() })
    }

    /// The project read for a directory inside it, `asked`.
    pub fn asked_for(self, asked: PathBuf) -> Repo {
        Repo { asked: Some(asked), ..self }
    }

    /// The directory the reading was asked for, when what was read is the project around it.
    pub fn asked(&self) -> Option<&Path> {
        self.asked.as_deref()
    }

    /// The part read in `language`, the language of the manifest that roots it.
    pub fn read_as(self, language: &'static str) -> Repo {
        Repo { language: Some(language), ..self }
    }

    /// The language a part is read in, when its manifest names it.
    pub fn language(&self) -> Option<&'static str> {
        self.language
    }

    /// The directory, from the root of the file system: `api`, `./api` and `../atlas/api`, named
    /// in `~/code/atlas`, are each `~/code/atlas/api`.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Where it stands in the repository it was read with: `orders` for the part under `orders/`,
    /// empty for the repository itself.
    pub fn at(&self) -> &Path {
        &self.at
    }

    /// The directory's own name: `atlas` for `.` read in `~/code/atlas`.
    pub fn name(&self) -> Option<String> {
        std::fs::canonicalize(&self.root).ok().and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
    }

    /// The part of the repository under `rel`, read in the same reading.
    pub fn under(&self, rel: &Path) -> Repo {
        if rel.as_os_str().is_empty() {
            return self.clone();
        }
        Repo {
            root: self.root.join(rel),
            at: self.at.join(rel),
            listing: self.listing.clone(),
            asked: None,
            language: None,
            read_files: Arc::default(),
        }
    }

    /// Where a path the caller writes from the root stands in the listing: `root/x` and `x` alike.
    fn key(&self, p: &Path) -> PathBuf {
        let p = normal(p);
        let rel = p.strip_prefix(&self.root).unwrap_or(&p);
        normal(&self.at.join(rel))
    }

    pub fn is_file(&self, p: impl AsRef<Path>) -> bool {
        self.listing.files.contains(&self.key(p.as_ref()))
    }

    pub fn is_dir(&self, p: impl AsRef<Path>) -> bool {
        self.listing.dirs.contains(&self.key(p.as_ref()))
    }

    pub fn exists(&self, p: impl AsRef<Path>) -> bool {
        self.is_file(p.as_ref()) || self.is_dir(p.as_ref())
    }

    /// The files read as they are now: each listed file by its path, with its size and the time it
    /// was written. Two readings with the same stamps read the same files.
    pub fn stamps(&self) -> BTreeMap<PathBuf, Stamp> {
        let stamp = |f: &PathBuf| std::fs::metadata(self.root.join(f.strip_prefix(&self.at).unwrap_or(f))).ok();
        self.listing.files.iter().map(|f| (f.clone(), stamp(f).map(|m| (m.len(), m.modified().ok())))).collect()
    }

    /// Bytes of files this part has consumed, from the listing root.
    pub fn read_contents(&self) -> BTreeMap<PathBuf, Arc<Vec<u8>>> {
        let Ok(paths) = self.read_files.lock() else { return BTreeMap::new() };
        let Ok(texts) = self.listing.texts.lock() else { return BTreeMap::new() };
        paths.iter().filter_map(|path| texts.get(path).map(|bytes| (path.clone(), bytes.clone()))).collect()
    }

    /// A listed path's bytes as they stand on disk, from the listing root.
    pub fn listed_bytes(&self, path: &Path) -> Option<Arc<Vec<u8>>> {
        self.listing.files.contains(path).then(|| std::fs::read(self.listing.base.join(path)).ok()).flatten().map(Arc::new)
    }

    /// Listed files below this part, from the listing root.
    pub fn listed_files(&self) -> BTreeSet<PathBuf> {
        self.listing.files.iter().filter(|f| f.starts_with(&self.at)).cloned().collect()
    }

    /// A file's bytes, read once.
    pub fn bytes(&self, p: impl AsRef<Path>) -> Option<Arc<Vec<u8>>> {
        let key = self.key(p.as_ref());
        if !self.listing.files.contains(&key) {
            return None;
        }
        self.read_files.lock().ok()?.insert(key.clone());
        let mut texts = self.listing.texts.lock().ok()?;
        if let Some(t) = texts.get(&key) {
            return Some(t.clone());
        }
        let bytes = Arc::new(std::fs::read(self.root.join(key.strip_prefix(&self.at).unwrap_or(&key))).ok()?);
        texts.insert(key, bytes.clone());
        Some(bytes)
    }

    /// A file's text, read once.
    pub fn text(&self, p: impl AsRef<Path>) -> Option<String> {
        self.bytes(p).map(|b| String::from_utf8_lossy(&b).into_owned())
    }

    /// A hidden file at the top of the repository, which the listing leaves out (`.atlasignore`):
    /// its path from the root of the file system, and its text as it is now.
    pub fn hidden(&self, name: &str) -> Option<(PathBuf, String)> {
        let path = self.listing.base.join(name);
        std::fs::read_to_string(&path).ok().map(|text| (path, text))
    }

    /// Every file under `dir` (a path from the root), at any depth, as the root's path joined
    /// with it, in order.
    pub fn files_under(&self, dir: impl AsRef<Path>) -> Vec<PathBuf> {
        let at = self.key(dir.as_ref());
        self.listing
            .files
            .iter()
            .filter(|f| f.starts_with(&at))
            .filter_map(|f| f.strip_prefix(&self.at).ok())
            .map(|f| self.root.join(f))
            .collect()
    }

    /// The directories right under `dir`, as the root's path joined with them, in order.
    pub fn dirs_in(&self, dir: impl AsRef<Path>) -> Vec<PathBuf> {
        let at = self.key(dir.as_ref());
        self.listing
            .dirs
            .iter()
            .filter(|d| d.parent() == Some(at.as_path()) && !d.as_os_str().is_empty())
            .filter_map(|d| d.strip_prefix(&self.at).ok())
            .map(|d| self.root.join(d))
            .collect()
    }

    /// The files named on the command line under the repository, as the root's path joined with
    /// them.
    pub fn named(&self) -> Vec<PathBuf> {
        self.listing.named.iter().filter_map(|f| f.strip_prefix(&self.at).ok()).map(|f| self.root.join(f)).collect()
    }

    /// Every directory under `dir`, at any depth and the directory itself, in order.
    pub fn dirs_under(&self, dir: impl AsRef<Path>) -> Vec<PathBuf> {
        let at = self.key(dir.as_ref());
        self.listing
            .dirs
            .iter()
            .filter(|d| d.starts_with(&at))
            .filter_map(|d| d.strip_prefix(&self.at).ok())
            .map(|d| self.root.join(d))
            .collect()
    }
}

/// A path with its `.` parts taken out and each `..` taken back over the part before it; a `..`
/// with nothing before it stays: `../api/./src/../x` is `../api/x`.
pub fn normal(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir if out.file_name().is_some() => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}
