//! `Disk(repo) -> read -> Repo`: the one place the engine reads a repository. The listing is read
//! once (what the author wrote: not a hidden directory, not what git ignores, not a dependency or
//! a build), with the files named on the command line beside it; a file's text is read once,
//! when a reader first asks for it. A part of the repository is a view of the same reading.
//!
//! A part keeps what its reading asked of it: each question of the listing (`Asked`) and each file
//! whose bytes it took. A reading depends on nothing else of the repository, so it stands while
//! each question gets the same answer and each file holds the same bytes.
//!
//! What a library's code and its types are is the environment's, not the repository's: read by ty,
//! from `node_modules`, from where Cargo keeps a crate.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::Result;

use crate::model::Lang;

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

/// A question a reading asks of the listing, each path from the listing's root: whether a path is
/// a file or a directory, which files of some kinds (their extensions) lie under a directory, which
/// directories right in it hold a file of a name (a manifest), which directories lie under it, which
/// files were named on the command line.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum Asked {
    File(PathBuf),
    Dir(PathBuf),
    Files(PathBuf, Vec<String>),
    DirsHolding(PathBuf, String),
    DirsUnder(PathBuf),
    Named,
}

/// The questions a reading asked of the listing, in order, each with its answer: the paths it named.
pub type Answers = Vec<(Asked, Vec<PathBuf>)>;

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
    language: Option<Lang>,
    /// Files whose contents this reading consumed, relative to the listing root.
    read_files: Arc<Mutex<BTreeSet<PathBuf>>>,
    /// The questions this reading asked of the listing.
    questions: Arc<Mutex<BTreeSet<Asked>>>,
}

impl Repo {
    /// A path of a repository as the file system resolves it, where it is there.
    pub fn resolved(path: &Path) -> Option<PathBuf> {
        std::fs::canonicalize(path).ok()
    }

    /// A path of a repository as the file system resolves it, else as it is named.
    pub fn real(path: &Path) -> PathBuf {
        Repo::resolved(path).unwrap_or_else(|| path.to_path_buf())
    }

    /// Whether the directory `dir` of a repository has `name` in it: its `.git`, a manifest.
    pub fn has(dir: &Path, name: &str) -> bool {
        dir.join(name).exists()
    }

    /// What lies beside the repository at `top`, in the folder it is in.
    pub fn beside(top: &Path) -> Vec<PathBuf> {
        let Some(Ok(entries)) = top.parent().map(std::fs::read_dir) else { return vec![] };
        entries.flatten().map(|e| e.path()).filter(|p| p != top).collect()
    }

    /// The text of a file of a repository that no reading lists: a file a prophecy is written into.
    pub fn text_at(path: &Path) -> Option<String> {
        std::fs::read_to_string(path).ok()
    }

    /// Write `text` into the file at `path` of a repository, with the directories it is in: what a
    /// vision or a prophecy puts in its tree.
    pub fn write_at(path: &Path, text: &str) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, text)
    }

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
        Ok(Repo {
            root,
            at: PathBuf::new(),
            listing: Arc::new(listing),
            asked: None,
            language: None,
            read_files: Arc::default(),
            questions: Arc::default(),
        })
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
    pub fn read_as(self, language: Lang) -> Repo {
        Repo { language: Some(language), ..self }
    }

    /// The language a part is read in, when its manifest names it.
    pub fn language(&self) -> Option<Lang> {
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

    /// The part of the repository under `rel`, read in the same reading; what its reading asks and
    /// reads is its own.
    pub fn under(&self, rel: &Path) -> Repo {
        let (root, at) = match rel.as_os_str().is_empty() {
            true => (self.root.clone(), self.at.clone()),
            false => (self.root.join(rel), self.at.join(rel)),
        };
        Repo { root, at, listing: self.listing.clone(), asked: None, language: None, read_files: Arc::default(), questions: Arc::default() }
    }

    /// Where a path the caller writes from the root stands in the listing: `root/x` and `x` alike.
    fn key(&self, p: &Path) -> PathBuf {
        let p = normal(p);
        let rel = p.strip_prefix(&self.root).unwrap_or(&p);
        normal(&self.at.join(rel))
    }

    /// Keep `question` among what this reading asked of the listing.
    fn ask(&self, question: &Asked) {
        if let Ok(mut asked) = self.questions.lock() {
            if !asked.contains(question) {
                asked.insert(question.clone());
            }
        }
    }

    /// What the listing answers `question`: the paths, from the listing's root, in order.
    fn answer(&self, question: &Asked) -> Vec<&PathBuf> {
        let Listing { files, dirs, named, .. } = &*self.listing;
        let kind = |f: &Path, kinds: &[String]| f.extension().and_then(|x| x.to_str()).is_some_and(|x| kinds.iter().any(|k| k == x));
        match question {
            Asked::File(p) => files.get(p).into_iter().collect(),
            Asked::Dir(p) => dirs.get(p).into_iter().collect(),
            Asked::Files(dir, kinds) => files.iter().filter(|f| f.starts_with(dir) && kind(f, kinds)).collect(),
            Asked::DirsHolding(dir, name) => dirs
                .iter()
                .filter(|d| d.parent() == Some(dir.as_path()) && !d.as_os_str().is_empty() && files.contains(&d.join(name)))
                .collect(),
            Asked::DirsUnder(dir) => dirs.iter().filter(|d| d.starts_with(dir)).collect(),
            Asked::Named => named.iter().collect(),
        }
    }

    /// The answer to `question`, asked: each path as the root's path joined with it.
    fn listed(&self, question: Asked) -> Vec<PathBuf> {
        self.ask(&question);
        self.answer(&question).into_iter().filter_map(|p| p.strip_prefix(&self.at).ok()).map(|p| self.root.join(p)).collect()
    }

    /// What this reading asked of the listing, each question with its answer.
    pub fn answers(&self) -> Answers {
        let asked = self.questions.lock().map(|a| a.clone()).unwrap_or_default();
        asked
            .into_iter()
            .map(|question| {
                let answer = self.answer(&question).into_iter().cloned().collect();
                (question, answer)
            })
            .collect()
    }

    /// Whether this listing answers each question of `answers` as it was answered then; asking
    /// them here keeps none.
    pub fn answers_alike(&self, answers: &Answers) -> bool {
        answers.iter().all(|(question, was)| self.answer(question).into_iter().eq(was.iter()))
    }

    pub fn is_file(&self, p: impl AsRef<Path>) -> bool {
        !self.listed(Asked::File(self.key(p.as_ref()))).is_empty()
    }

    pub fn is_dir(&self, p: impl AsRef<Path>) -> bool {
        !self.listed(Asked::Dir(self.key(p.as_ref()))).is_empty()
    }

    pub fn exists(&self, p: impl AsRef<Path>) -> bool {
        self.is_file(p.as_ref()) || self.is_dir(p.as_ref())
    }

    /// The files read as they are now: each listed file by its path, with its size and the time it
    /// was written. Two readings with the same stamps read the same files.
    pub fn stamps(&self) -> BTreeMap<PathBuf, Stamp> {
        self.listing.files.iter().map(|f| (f.clone(), self.stamp(f))).collect()
    }

    /// A path of the listing as it is now, from the listing's root: its size and the time it was
    /// written.
    pub fn stamp(&self, path: &Path) -> Stamp {
        std::fs::metadata(self.listing.base.join(path)).ok().map(|m| (m.len(), m.modified().ok()))
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

    /// A file's bytes, read once. Whether it is there is asked of the listing.
    pub fn bytes(&self, p: impl AsRef<Path>) -> Option<Arc<Vec<u8>>> {
        let key = self.key(p.as_ref());
        if self.listed(Asked::File(key.clone())).is_empty() {
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

    /// The files under `dir` (a path from the root), at any depth, whose extension is one of
    /// `kinds`, as the root's path joined with them, in order.
    pub fn files_under(&self, dir: impl AsRef<Path>, kinds: &[&str]) -> Vec<PathBuf> {
        self.listed(Asked::Files(self.key(dir.as_ref()), kinds.iter().map(|k| k.to_string()).collect()))
    }

    /// That the reading rests on the files under `dir` whose extension is one of `kinds`, as though
    /// it had listed them: the files a tool that looks on disk by itself may find (ty, the modules
    /// of a project).
    pub fn rests_on(&self, dir: impl AsRef<Path>, kinds: &[&str]) {
        self.ask(&Asked::Files(self.key(dir.as_ref()), kinds.iter().map(|k| k.to_string()).collect()));
    }

    /// The directories right under `dir` that hold a file `name` (a manifest, `__init__.py`), as the
    /// root's path joined with them, in order.
    pub fn dirs_holding(&self, dir: impl AsRef<Path>, name: &str) -> Vec<PathBuf> {
        self.listed(Asked::DirsHolding(self.key(dir.as_ref()), name.to_string()))
    }

    /// The files named on the command line under the repository, as the root's path joined with
    /// them.
    pub fn named(&self) -> Vec<PathBuf> {
        self.listed(Asked::Named)
    }

    /// Every directory under `dir`, at any depth and the directory itself, in order.
    pub fn dirs_under(&self, dir: impl AsRef<Path>) -> Vec<PathBuf> {
        self.listed(Asked::DirsUnder(self.key(dir.as_ref())))
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

/// A path as one key whatever way it is written: with `/` between its parts, without Windows's
/// verbatim prefix (`\\?\`), and on Windows in one case, as the file system compares it.
pub fn keyed(p: &Path) -> String {
    let s = plain(p);
    if cfg!(windows) {
        s.to_lowercase()
    } else {
        s
    }
}

/// A path written with `/` between its parts and without Windows's verbatim prefix.
fn plain(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    match s.strip_prefix("//?/UNC/") {
        Some(share) => format!("//{share}"),
        None => s.strip_prefix("//?/").unwrap_or(&s).to_string(),
    }
}

/// The path of `file` from `dir`, however each is written: where `file` lies in `dir` as their
/// keys say, the rest of `file` as it is written.
pub fn below(file: &Path, dir: &Path) -> Option<PathBuf> {
    if let Ok(rest) = file.strip_prefix(dir) {
        return Some(rest.to_path_buf());
    }
    let (file, dir) = (plain(file), plain(dir));
    let dir = dir.trim_end_matches('/');
    let rest = file.get(dir.len()..)?.strip_prefix('/')?;
    (keyed(Path::new(&file[..dir.len()])) == keyed(Path::new(dir))).then(|| PathBuf::from(rest))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file's path from a directory is the same whichever way the two are written: with or without
    /// the verbatim prefix, with `\` or `/` between their parts.
    #[test]
    fn a_path_below_a_directory_does_not_depend_on_how_either_is_written() {
        let plainly = below(Path::new("C:/t/copy/src/a.py"), Path::new("C:/t/copy"));
        assert_eq!(plainly, Some(PathBuf::from("src/a.py")));
        for (file, dir) in [
            (r"\\?\C:\t\copy\src\a.py", r"C:\t\copy"),
            (r"C:\t\copy\src\a.py", r"\\?\C:\t\copy"),
            (r"\\?\C:\t\copy\src\a.py", r"\\?\C:\t\copy\"),
            (r"\\?\UNC\host\share\copy\src\a.py", r"\\host\share\copy"),
        ] {
            assert_eq!(below(Path::new(file), Path::new(dir)).map(|p| keyed(&p)), Some(keyed(Path::new("src/a.py"))), "{file} in {dir}");
        }
        assert_eq!(
            below(Path::new("C:/t/copyist/a.py"), Path::new("C:/t/copy")),
            None,
            "a name that only begins as the directory's is not in it"
        );
        assert_eq!(below(Path::new("C:/t/other/a.py"), Path::new("C:/t/copy")), None);
    }
}
