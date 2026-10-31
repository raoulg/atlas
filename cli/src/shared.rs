//! What the reader of a served page and an agent share, kept in `.atlas/` beside the pages, so that
//! `atlas-code serve` and `atlas-code mcp` need not know each other:
//!
//! - `shown.json`: the view the page shows, and the boxes and lines it draws, as the page last said;
//! - `look.json`: a view an agent asks the page to go to, and a box or line it points at there,
//!   taken by the page once;
//! - `notes.json`: what the reader wrote on a box or a line, and whether an agent resolved it;
//! - `serving.json`: where `atlas-code serve` serves the page, for a link to the place of a note;
//! - `vision.json`: what an agent has in view, the working tree with some files written otherwise,
//!   drawn beside the working tree until the agent retracts it;
//! - `annotations.json`: what an agent says of a box or a line (what a condition means, what a step
//!   is for), shown on the page until the agent takes it away.
//!
//! Threads of `atlas-code serve` and the process of `atlas-code mcp` write these files at once. A file is
//! written whole beside its place and renamed into it, so a reader never finds half of it; a change
//! that reads a file and writes it again (a note added, a look taken) holds the file's lock, beside
//! it, so that no change is lost to another; and a file that does not read as Atlas wrote it is an
//! error, never taken for an empty one and written over.
//!
//! A note is resolved by an agent, never by reading alone: open until an agent reads it, seen until
//! it resolves it, by a fix or by what it explains where it works, resolved after. The reader
//! reopens a resolved note with a new line. A note is for the work at hand: what must last, an
//! agent puts in an issue that links to the note's place. A resolved note is shown while the
//! `atlas-code serve` it was resolved under runs; the next one starts without it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result};
use atlas_engine::sight::Sight;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Where a note stands.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Open,
    Seen,
    #[serde(alias = "answered", alias = "closed")]
    Resolved,
}

/// What the reader wrote on a box or a line of the picture.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Note {
    #[serde(default)]
    pub id: u32,
    /// What it is on, as the page names it: a box by its id (a function's own id, `t:` a type,
    /// `w:` a place, `unit:` a package, `n:` an opened frame), a line as `from>to`.
    pub on: String,
    /// The words the page shows for it.
    pub label: String,
    /// The view it was written in.
    #[serde(default)]
    pub view: Value,
    /// The commit the picture was read at.
    #[serde(default)]
    pub rev: String,
    /// What the reader wrote: a line each time they opened it.
    pub text: String,
    #[serde(default)]
    pub state: State,
    /// The commit that resolved it, where a commit did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    /// The issue that keeps it, where an agent made one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issue: Option<String>,
}

/// A file of `.atlas/` at the top of the repository `repo` is in.
/// How long each phase of drawing the page took the last few times `atlas-code serve` started, in
/// seconds, the latest last: what the progress bars expect of the next start. A start that finds the
/// index of its build of Atlas reads little (`warm`), one that does not reads all (`cold`), so the two
/// kinds are kept apart.
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
pub struct Timings {
    #[serde(default)]
    pub cold: BTreeMap<String, Vec<f64>>,
    #[serde(default)]
    pub warm: BTreeMap<String, Vec<f64>>,
}

impl Timings {
    /// The times of starts of one kind, by phase.
    pub fn of(&self, warm: bool) -> &BTreeMap<String, Vec<f64>> {
        if warm {
            &self.warm
        } else {
            &self.cold
        }
    }

    pub fn of_mut(&mut self, warm: bool) -> &mut BTreeMap<String, Vec<f64>> {
        if warm {
            &mut self.warm
        } else {
            &mut self.cold
        }
    }
}

pub fn timings(repo: &Path) -> Timings {
    read(&file(repo, "timings.json")).and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default()
}

pub fn set_timings(repo: &Path, timings: &Timings) -> Result<()> {
    write(&file(repo, "timings.json"), timings)
}

fn file(repo: &Path, name: &str) -> PathBuf {
    crate::home::made(Some(repo), name)
}

fn read(path: &Path) -> Option<Value> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

/// What the file at `path` holds: nothing when there is no file, and an error when it does not read
/// as a `T`, so that what it holds is not written over.
fn kept<T: DeserializeOwned + Default>(path: &Path) -> Result<T> {
    match std::fs::read(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
        Ok(bytes) => serde_json::from_slice(&bytes)
            .with_context(|| format!("{} does not read as Atlas writes it; it is left as it is, to mend or remove", path.display())),
    }
}

/// How many files this process has written: with its id, the name of the next one's partial file.
static WRITTEN: AtomicU64 = AtomicU64::new(0);

/// `value` written beside its place, in a file of this write's own, and renamed into it, so a reader
/// never finds half of it and two writers never write into one file.
fn write(path: &Path, value: &impl Serialize) -> Result<()> {
    let n = WRITTEN.fetch_add(1, Ordering::Relaxed);
    let partial = path.with_extension(format!("{}.{n}.part", std::process::id()));
    let written = std::fs::write(&partial, serde_json::to_vec_pretty(value)?).and_then(|_| std::fs::rename(&partial, path));
    if written.is_err() {
        let _ = std::fs::remove_file(&partial);
    }
    written.with_context(|| format!("writing {}", path.display()))
}

/// `change` made while this thread holds the lock of the file at `path`, which no other thread or
/// process holds at the same time: what it reads of the file is what it writes over.
fn locked<T>(path: &Path, change: impl FnOnce() -> Result<T>) -> Result<T> {
    let at = path.with_extension("lock");
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&at)
        .with_context(|| format!("opening {}", at.display()))?;
    lock.lock().with_context(|| format!("locking {}", at.display()))?;
    // the lock is let go when `lock` is closed, after the change, however it ends
    change()
}

/// A commit as a line shows it: its first seven characters.
pub fn short(commit: &str) -> String {
    commit.chars().take(7).collect()
}

/// What the page last said it shows.
pub fn shown(repo: &Path) -> Option<Value> {
    read(&file(repo, "shown.json"))
}

pub fn set_shown(repo: &Path, shown: &Value) -> Result<()> {
    write(&file(repo, "shown.json"), shown)
}

/// Where an agent asks the page to look: a sight to go to, a box or a line to point at there (by its
/// id, a line as `from>to`), or both.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Look {
    pub view: Option<Sight>,
    pub at: Option<String>,
}

/// Ask the page to look where `look` says.
pub fn look(repo: &Path, look: &Look) -> Result<()> {
    let path = file(repo, "look.json");
    locked(&path, || write(&path, look))
}

/// Where an agent asked the page to look, taken once: by one asker, and a look written while it is
/// taken waits for the next.
pub fn take_look(repo: &Path) -> Result<Option<Look>> {
    let path = file(repo, "look.json");
    locked(&path, || {
        let bytes = match std::fs::read(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            read => read.with_context(|| format!("reading {}", path.display()))?,
        };
        std::fs::remove_file(&path).with_context(|| format!("removing {}", path.display()))?;
        serde_json::from_slice(&bytes).map(Some).with_context(|| format!("{} does not read as a look", path.display()))
    })
}

fn notes_file(repo: &Path) -> PathBuf {
    file(repo, "notes.json")
}

/// Every note, as last written; an error when the file does not read as notes.
pub fn notes(repo: &Path) -> Result<Vec<Note>> {
    kept(&notes_file(repo))
}

/// The notes as `f` changes them, read and written again while no one else changes them; written
/// only when `f` says it changed them.
fn change_notes<T>(repo: &Path, f: impl FnOnce(&mut Vec<Note>) -> (bool, T)) -> Result<T> {
    let path = notes_file(repo);
    locked(&path, || {
        let mut all = kept(&path)?;
        let (changed, out) = f(&mut all);
        if changed {
            write(&path, &all)?;
        }
        Ok(out)
    })
}

/// Keep a new note, open, numbered after the last.
pub fn add(repo: &Path, mut note: Note) -> Result<Note> {
    change_notes(repo, |all| {
        note.id = all.iter().map(|n| n.id).max().unwrap_or(0) + 1;
        note.state = State::Open;
        (note.commit, note.issue) = (None, None);
        all.push(note.clone());
        (true, note)
    })
}

/// Change the note numbered `id`; none when there is no such note.
pub fn change(repo: &Path, id: u32, f: impl FnOnce(&mut Note)) -> Result<Option<Note>> {
    change_notes(repo, |all| match all.iter_mut().find(|n| n.id == id) {
        Some(n) => {
            f(n);
            (true, Some(n.clone()))
        }
        None => (false, None),
    })
}

/// The reader opens a resolved note again, with a new line: open, for an agent to read anew.
pub fn reopen(repo: &Path, id: u32, line: &str) -> Result<Option<Note>> {
    change(repo, id, |n| {
        n.text = format!("{}\n{}", n.text, line.trim());
        n.state = State::Open;
        (n.commit, n.issue) = (None, None);
    })
}

/// An agent has resolved a note: by `commit`, where one did, and kept in `issue`, where it made one.
pub fn resolve(repo: &Path, id: u32, commit: Option<String>, issue: Option<String>) -> Result<Option<Note>> {
    change(repo, id, |n| {
        n.state = State::Resolved;
        (n.commit, n.issue) = (commit, issue);
    })
}

/// A new `atlas-code serve` starts without the notes resolved before it.
pub fn forget_resolved(repo: &Path) -> Result<()> {
    change_notes(repo, |all| {
        let before = all.len();
        all.retain(|n| n.state != State::Resolved);
        (all.len() != before, ())
    })
}

/// what an agent has in view: the working tree with some files written otherwise, under a title. A
/// vision is sketch code (signatures, types, the calls in their order), which has to parse, not
/// to build.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Vision {
    pub title: String,
    /// Each file by its path from the repository's root: its text, or none where it goes.
    pub files: BTreeMap<PathBuf, Option<String>>,
}

/// Where the vision is kept: its file changes when the vision does.
pub fn vision_file(repo: &Path) -> PathBuf {
    file(repo, "vision.json")
}

/// what an agent has in view, if anything.
pub fn vision(repo: &Path) -> Option<Vision> {
    read(&vision_file(repo)).and_then(|v| serde_json::from_value(v).ok())
}

pub fn envision(repo: &Path, p: &Vision) -> Result<()> {
    write(&vision_file(repo), p)
}

/// What `gh` says of an issue of the repository: its state and title. None where it cannot say (no
/// `gh`, no network, no remote, no such issue): the caller says it does not know.
pub fn issue_state(repo: &Path, issue: &str) -> Option<(atlas_engine::revelation::IssueState, String)> {
    use atlas_engine::revelation::IssueState;
    let (on, number) = match issue.rsplit_once('#') {
        Some((repo_name, n)) if !repo_name.is_empty() => (Some(repo_name), n),
        Some((_, n)) => (None, n),
        None => (None, issue),
    };
    let mut gh = std::process::Command::new("gh");
    gh.current_dir(repo).args(["issue", "view", number, "--json", "state,title"]);
    if let Some(r) = on {
        gh.args(["--repo", r]);
    }
    let out = gh.stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null()).output().ok().filter(|o| o.status.success())?;
    let v: Value = serde_json::from_slice(&out.stdout).ok()?;
    let state = match v["state"].as_str()? {
        "CLOSED" => IssueState::Closed,
        "OPEN" => IssueState::Open,
        _ => IssueState::Unknown,
    };
    Some((state, v["title"].as_str().unwrap_or("").to_string()))
}

/// Files written into the tree, each by its path from the repository's root.
pub fn write_files(repo: &Path, files: &std::collections::BTreeMap<PathBuf, String>) -> Result<()> {
    for (rel, text) in files {
        atlas_engine::repo::Repo::write_at(&repo.join(rel), text).with_context(|| format!("writing {}", rel.display()))?;
    }
    Ok(())
}

/// The vision retracted: whether there was one.
pub fn retract(repo: &Path) -> Result<bool> {
    let path = vision_file(repo);
    if !path.exists() {
        return Ok(false);
    }
    std::fs::remove_file(&path).with_context(|| format!("removing {}", path.display()))?;
    Ok(true)
}

/// What an agent says of a box or a line of the picture: its own words, apart from what Atlas
/// traces.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Annotation {
    /// What it is on, as `Note::on` names it.
    pub on: String,
    pub text: String,
}

/// What agents have said of boxes and lines; an error when the file does not read as that.
pub fn annotations(repo: &Path) -> Result<Vec<Annotation>> {
    kept(&file(repo, "annotations.json"))
}

/// Say `text` of what `on` names, in place of what was said of it; nothing takes it away. Gives
/// what is said now.
pub fn annotate(repo: &Path, on: &str, text: Option<&str>) -> Result<Vec<Annotation>> {
    let path = file(repo, "annotations.json");
    locked(&path, || {
        let mut all: Vec<Annotation> = kept::<Vec<Annotation>>(&path)?.into_iter().filter(|a| a.on != on).collect();
        if let Some(text) = text.map(str::trim).filter(|t| !t.is_empty()) {
            all.push(Annotation { on: on.to_string(), text: text.to_string() });
        }
        write(&path, &all)?;
        Ok(all)
    })
}

/// Where `atlas-code serve` serves the page: `http://127.0.0.1:8778/`.
pub fn serving(repo: &Path) -> Option<String> {
    read(&file(repo, "serving.json"))?.get("url")?.as_str().map(str::to_string)
}

pub fn set_serving(repo: &Path, url: &str) -> Result<()> {
    write(&file(repo, "serving.json"), &serde_json::json!({ "url": url }))
}

/// The address of a note's place: the page `atlas-code serve` serves at `url`, in the view the note was
/// written in, pointing at what it is on.
pub fn link(url: &str, note: &Note) -> String {
    let view = if note.view.is_object() { format!("v={}&", component(&note.view.to_string())) } else { String::new() };
    format!("{url}#{view}at={}", component(&note.on))
}

/// `s` as it may stand in an address: every byte but a letter, a digit and `-_.~` as `%XX`.
fn component(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Every note as an agent reads them: the open ones marked seen now that it does.
pub fn read_notes(repo: &Path) -> Result<Vec<Note>> {
    change_notes(repo, |all| {
        let mut changed = false;
        for n in all.iter_mut().filter(|n| n.state == State::Open) {
            (n.state, changed) = (State::Seen, true);
        }
        (changed, all.clone())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A repository of its own for one test, with the workbench at its top.
    fn repository(test: &str) -> PathBuf {
        let repo = std::env::temp_dir().join(format!("atlas-shared-{}-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&repo);
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        repo
    }

    fn note(text: &str) -> Note {
        Note {
            id: 0,
            on: "spots.count".into(),
            label: "count".into(),
            view: Value::Null,
            rev: String::new(),
            text: text.into(),
            state: State::Open,
            commit: None,
            issue: None,
        }
    }

    /// Writers at once lose no note and mix none: every note added by threads that add while others
    /// resolve and read is kept, under an id of its own, with its own text, and every resolved note
    /// stays resolved.
    #[test]
    fn writers_at_once_lose_no_note() {
        let repo = repository("writers");
        let first: Vec<u32> = (0..10).map(|i| add(&repo, note(&format!("first {i}"))).unwrap().id).collect();
        std::thread::scope(|s| {
            for t in 0..6 {
                let repo = &repo;
                s.spawn(move || (0..15).for_each(|i| drop(add(repo, note(&format!("thread {t} note {i}"))).unwrap())));
            }
            let (repo, first) = (&repo, &first);
            s.spawn(move || first.iter().for_each(|id| drop(resolve(repo, *id, Some("3a2b1c9".into()), None).unwrap())));
            s.spawn(move || (0..20).for_each(|_| drop(read_notes(repo).unwrap())));
        });
        let all = notes(&repo).unwrap();
        assert_eq!(all.len(), 10 + 6 * 15);
        let ids: std::collections::BTreeSet<u32> = all.iter().map(|n| n.id).collect();
        assert_eq!(ids.len(), all.len(), "every note has an id of its own");
        for t in 0..6 {
            for i in 0..15 {
                assert!(all.iter().any(|n| n.text == format!("thread {t} note {i}")), "thread {t} note {i} is kept");
            }
        }
        assert!(all.iter().filter(|n| first.contains(&n.id)).all(|n| n.state == State::Resolved), "no resolution is lost");
        let parts: Vec<PathBuf> = std::fs::read_dir(repo.join(".atlas"))
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension() == Some("part".as_ref()))
            .collect();
        assert!(parts.is_empty(), "no partial file is left: {parts:?}");
        let _ = std::fs::remove_dir_all(&repo);
    }

    /// A notes file that does not read as notes is an error, and is left as it is: a note added is
    /// refused rather than written over the ones it holds.
    #[test]
    fn a_notes_file_that_does_not_read_is_left_as_it_is() {
        let repo = repository("unread");
        add(&repo, note("kept")).unwrap();
        let path = repo.join(".atlas/notes.json");
        let mangled = std::fs::read_to_string(&path).unwrap().replacen('}', "", 1);
        std::fs::write(&path, &mangled).unwrap();
        assert!(notes(&repo).is_err());
        assert!(add(&repo, note("new")).is_err());
        assert!(resolve(&repo, 1, None, None).is_err());
        assert!(read_notes(&repo).is_err());
        assert!(forget_resolved(&repo).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), mangled, "what the file holds is not written over");
        let _ = std::fs::remove_dir_all(&repo);
    }

    /// A look is taken at most once, and none is lost: while an agent points the page at one place
    /// after another and the page takes them, no look is taken twice, and the last one pointed at is
    /// taken, now or at the next ask.
    #[test]
    fn a_look_is_taken_once_and_never_lost() {
        let repo = repository("look");
        let looked = |i: usize| Look { view: None, at: Some(format!("box {i}")) };
        let mut taken = std::thread::scope(|s| {
            let repo = &repo;
            let page = s.spawn(move || (0..400).filter_map(|_| take_look(repo).unwrap()).collect::<Vec<Look>>());
            (0..200).for_each(|i| look(repo, &looked(i)).unwrap());
            page.join().unwrap()
        });
        taken.extend(take_look(&repo).unwrap());
        assert!(take_look(&repo).unwrap().is_none(), "a look is taken once");
        let at: Vec<&str> = taken.iter().filter_map(|l| l.at.as_deref()).collect();
        let distinct: std::collections::BTreeSet<&&str> = at.iter().collect();
        assert_eq!(distinct.len(), at.len(), "no look is taken twice: {at:?}");
        assert_eq!(at.last(), Some(&"box 199"), "the last look is taken");
        let _ = std::fs::remove_dir_all(&repo);
    }

    /// A commit is shortened by its characters, whatever they are.
    #[test]
    fn a_commit_is_shortened_by_characters() {
        assert_eq!(short("3a2b1c9d0e"), "3a2b1c9");
        assert_eq!(short("ééééééééé"), "ééééééé");
        assert_eq!(short("3a2"), "3a2");
    }
}
