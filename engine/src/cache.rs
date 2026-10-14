//! What does not change, kept on disk between readings, so that reading is fast without reading
//! less: the parts of a revision, by its commit, and what ty says of a library's file, by the file
//! and the environment it is read in. A kept answer is the answer a reading gives anew: its key
//! holds everything that answer depends on, and an entry holds its key whole, so a key that only
//! shares a hash with another finds nothing.
//!
//! The cache is `$XDG_CACHE_HOME/atlas`, else the platform's cache directory (`~/Library/Caches`,
//! `%LOCALAPPDATA%`, `~/.cache`), with a directory per build of Atlas. An entry is written to a file
//! of its own and renamed into place, so two processes that keep one entry at once leave one whole
//! entry.

use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// Where one build of Atlas keeps what does not change.
#[derive(Clone, Debug)]
pub struct Cache {
    dir: PathBuf,
    /// How many answers it gave, shared by its clones.
    hits: Arc<AtomicUsize>,
}

impl Cache {
    /// The cache of the build `build` (`0.47.0 (be3090f)`) in the user's cache directory; none where
    /// there is no such directory.
    pub fn new(build: &str) -> Option<Cache> {
        Some(Cache::at(&root()?, build))
    }

    /// The cache of the build `build` under `root`. A build from a commit with edits (`+`) is told
    /// apart from another by its binary, its size and when it was written; once another such build
    /// keeps something, the entries of the one before it are removed, since no binary reads them.
    pub fn at(root: &Path, build: &str) -> Cache {
        let name: String = build.chars().map(|c| if c.is_ascii_alphanumeric() || c == '.' { c } else { '-' }).collect();
        let mut name = name.split('-').filter(|s| !s.is_empty()).collect::<Vec<_>>().join("-");
        if build.contains('+') {
            let stamp = std::env::current_exe().and_then(std::fs::metadata).map(|m| format!("{} {}", m.len(), modified(&m)));
            name.push_str(&format!("{EDITED}{}", &digest(stamp.unwrap_or_default().as_bytes())[..16]));
        }
        Cache { dir: root.join(name), hits: Arc::default() }
    }

    /// The value kept of `kind` under `key`.
    pub fn get(&self, kind: &str, key: &str) -> Option<Vec<u8>> {
        let bytes = std::fs::read(self.file(kind, key)).ok()?;
        let at = bytes.iter().position(|b| *b == b'\n')?;
        let value = (bytes[..at] == *head(key).as_bytes()).then(|| bytes[at + 1..].to_vec())?;
        self.hits.fetch_add(1, Ordering::Relaxed);
        Some(value)
    }

    /// How many values it has given.
    pub fn hits(&self) -> usize {
        self.hits.load(Ordering::Relaxed)
    }

    /// Keep `value` of `kind` under `key`: written beside its place and renamed into it.
    pub fn put(&self, kind: &str, key: &str, value: &[u8]) {
        static WRITTEN: AtomicUsize = AtomicUsize::new(0);
        static SWEPT: std::sync::Once = std::sync::Once::new();
        SWEPT.call_once(|| self.sweep());
        let file = self.file(kind, key);
        let Some(dir) = file.parent() else { return };
        if std::fs::create_dir_all(dir).is_err() {
            return;
        }
        let partial = dir.join(format!(
            ".{}-{}-{}",
            file.file_name().and_then(|n| n.to_str()).unwrap_or("entry"),
            std::process::id(),
            WRITTEN.fetch_add(1, Ordering::Relaxed)
        ));
        let mut bytes = head(key).into_bytes();
        bytes.push(b'\n');
        bytes.extend_from_slice(value);
        if std::fs::write(&partial, bytes).and_then(|_| std::fs::rename(&partial, &file)).is_err() {
            let _ = std::fs::remove_file(&partial);
        }
    }

    fn file(&self, kind: &str, key: &str) -> PathBuf {
        self.dir.join(kind).join(format!("{}.json", digest(key.as_bytes())))
    }

    /// Remove the entries of the builds with edits before this one, when this is one.
    fn sweep(&self) {
        let (Some(parent), Some(own)) = (self.dir.parent(), self.dir.file_name()) else { return };
        if !own.to_string_lossy().contains(EDITED) {
            return;
        }
        for e in std::fs::read_dir(parent).into_iter().flatten().flatten() {
            if e.file_name() != own && e.file_name().to_string_lossy().contains(EDITED) {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }
}

/// What the directory of a build with edits has in its name.
const EDITED: &str = "-edited-";

/// An entry's first line: its key, as a JSON string.
fn head(key: &str) -> String {
    serde_json::to_string(key).unwrap_or_default()
}

/// The user's cache directory for Atlas: `$XDG_CACHE_HOME/atlas`, else the platform's.
pub fn root() -> Option<PathBuf> {
    let var = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    let base = var("XDG_CACHE_HOME").filter(|p| p.is_absolute()).or_else(|| match () {
        _ if cfg!(target_os = "macos") => var("HOME").map(|h| h.join("Library").join("Caches")),
        _ if cfg!(windows) => var("LOCALAPPDATA"),
        _ => var("HOME").map(|h| h.join(".cache")),
    })?;
    Some(base.join("atlas"))
}

/// A fingerprint of bytes: 128 bits, in hex.
pub fn digest(bytes: &[u8]) -> String {
    let half = |seed: u8| {
        let mut h = DefaultHasher::new();
        h.write_u8(seed);
        h.write(bytes);
        h.finish()
    };
    format!("{:016x}{:016x}", half(1), half(2))
}

/// When a file was last written, in nanoseconds.
fn modified(m: &std::fs::Metadata) -> u128 {
    m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos())
}

/// Something a reading takes from outside what it reads, to be asked how it stands now.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Check {
    /// A variable of the environment Atlas runs in.
    Var(String),
    /// A file's bytes.
    File(PathBuf),
    /// The names in a directory.
    Names(PathBuf),
    /// The names in a directory, each with when it was last written, but the names listed.
    Listing(PathBuf, Vec<String>),
    /// Every file under a directory, by its path below it, with its size and when it was last
    /// written.
    Tree(PathBuf),
    /// What a directory offers a language's imports.
    Offered(PathBuf, Imports),
}

/// What a language's imports find in a directory: its directories, each with which of the marks
/// (`__init__.py`) it holds, and its files with one of the source extensions, by name; not what is
/// skipped (what the language writes there itself).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Imports {
    pub sources: Vec<String>,
    pub marks: Vec<String>,
    pub skip: Vec<String>,
}

impl Check {
    /// The path it asks about, when it asks about one.
    pub fn path(&self) -> Option<&Path> {
        match self {
            Check::Var(_) => None,
            Check::File(p) | Check::Names(p) | Check::Listing(p, _) | Check::Tree(p) | Check::Offered(p, _) => Some(p),
        }
    }

    /// What it asks, in a word.
    pub fn kind(&self) -> &'static str {
        match self {
            Check::Var(_) => "var",
            Check::File(_) => "file",
            Check::Names(_) => "names",
            Check::Listing(..) => "listing",
            Check::Tree(_) => "tree",
            Check::Offered(..) => "offered",
        }
    }

    /// The same check of the path as the file system resolves it, where it is there.
    fn resolved(self) -> Check {
        let canonical = |p: &PathBuf| std::fs::canonicalize(p).unwrap_or_else(|_| p.clone());
        match &self {
            Check::Var(_) => self,
            Check::File(p) => Check::File(canonical(p)),
            Check::Names(p) => Check::Names(canonical(p)),
            Check::Listing(p, skip) => Check::Listing(canonical(p), skip.clone()),
            Check::Tree(p) => Check::Tree(canonical(p)),
            Check::Offered(p, imports) => Check::Offered(canonical(p), imports.clone()),
        }
    }

    /// How it stands now, as a fingerprint; `absent` where there is nothing to ask.
    pub fn now(&self) -> String {
        let listed = |p: &Path, with_time: bool, skip: &[String]| -> Option<String> {
            let mut names: Vec<String> = std::fs::read_dir(p)
                .ok()?
                .flatten()
                .filter(|e| !skip.iter().any(|s| e.file_name() == s.as_str()))
                .map(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    match with_time {
                        true => format!("{name} {}", e.metadata().map(|m| modified(&m)).unwrap_or(0)),
                        false => name,
                    }
                })
                .collect();
            names.sort();
            Some(digest(names.join("\n").as_bytes()))
        };
        let found = match self {
            Check::Var(k) => Some(std::env::var_os(k).map_or_else(|| "unset".to_string(), |v| digest(v.to_string_lossy().as_bytes()))),
            Check::File(p) => std::fs::read(p).ok().map(|b| digest(&b)),
            Check::Names(p) => listed(p, false, &[]),
            Check::Listing(p, skip) => listed(p, true, skip),
            Check::Tree(p) => p.is_dir().then(|| {
                let mut files: Vec<String> = walkdir::WalkDir::new(p)
                    .into_iter()
                    .flatten()
                    .filter(|e| e.file_type().is_file())
                    .map(|e| {
                        let m = e.metadata().ok();
                        let rel = e.path().strip_prefix(p).unwrap_or(e.path()).to_string_lossy().to_string();
                        format!("{rel} {} {}", m.as_ref().map_or(0, |m| m.len()), m.as_ref().map_or(0, modified))
                    })
                    .collect();
                files.sort();
                digest(files.join("\n").as_bytes())
            }),
            Check::Offered(p, Imports { sources, marks, skip }) => std::fs::read_dir(p).ok().map(|r| {
                let mut offered: Vec<String> = r
                    .flatten()
                    .filter_map(|e| {
                        let (path, name) = (e.path(), e.file_name().to_string_lossy().to_string());
                        if name.starts_with('.') || skip.contains(&name) {
                            return None;
                        }
                        if path.is_dir() {
                            let held: Vec<&str> = marks.iter().filter(|m| path.join(m.as_str()).is_file()).map(String::as_str).collect();
                            return Some(format!("{name}/ {}", held.join(" ")));
                        }
                        let ext = path.extension().and_then(|x| x.to_str()).unwrap_or("");
                        sources.iter().any(|x| x == ext).then_some(name)
                    })
                    .collect();
                offered.sort();
                digest(offered.join("\n").as_bytes())
            }),
        };
        found.unwrap_or_else(|| "absent".to_string())
    }
}

/// The checks with how each stands now, each once, its path as the file system resolves it.
pub fn witness(checks: Vec<Check>) -> Vec<(Check, String)> {
    let mut checks: Vec<Check> = checks.into_iter().map(Check::resolved).collect();
    checks.sort();
    checks.dedup();
    checks
        .into_iter()
        .map(|c| {
            let now = c.now();
            (c, now)
        })
        .collect()
}

/// Whether everything a witness saw stands as it did.
pub fn holds(witness: &[(Check, String)]) -> bool {
    witness.iter().all(|(c, was)| c.now() == *was)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_entry_answers_only_its_own_key() {
        let root = std::env::temp_dir().join(format!("atlas-cache-unit-{}", std::process::id()));
        let cache = Cache::at(&root, "0.0.0 (abc)");
        assert_eq!(cache.get("kind", "a"), None);
        cache.put("kind", "a", b"value");
        assert_eq!(cache.get("kind", "a").as_deref(), Some(&b"value"[..]));
        assert_eq!(cache.get("kind", "b"), None);
        // a file under a's name that holds another key answers nothing
        std::fs::write(cache.file("kind", "a"), format!("{}\nother", head("b"))).unwrap();
        assert_eq!(cache.get("kind", "a"), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_check_sees_a_file_change() {
        let dir = std::env::temp_dir().join(format!("atlas-cache-check-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub/a.py"), "x = 1\n").unwrap();
        let w = witness(vec![
            Check::Tree(dir.clone()),
            Check::Names(dir.clone()),
            Check::File(dir.join("sub/a.py")),
            Check::File(dir.join("none")),
        ]);
        assert!(holds(&w));
        std::fs::write(dir.join("sub/a.py"), "x = 22\n").unwrap();
        assert!(!holds(&w), "a file grew");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
