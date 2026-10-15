//! What the reader of a served page and an agent share, kept in `.atlas/` beside the pages, so that
//! `atlas serve` and `atlas mcp` need not know each other:
//!
//! - `shown.json`: the view the page shows, and the boxes and lines it draws, as the page last said;
//! - `look.json`: a view an agent asks the page to go to, and a box or line it points at there,
//!   taken by the page once;
//! - `notes.json`: what the reader wrote on a box or a line, and whether an agent resolved it;
//! - `serving.json`: where `atlas serve` serves the page, for a link to the place of a note;
//! - `vision.json`: what an agent has in view, the working tree with some files written otherwise,
//!   drawn beside the working tree until the agent retracts it;
//! - `annotations.json`: what an agent says of a box or a line (what a condition means, what a step
//!   is for), shown on the page until the agent takes it away.
//!
//! A note is resolved by an agent, never by reading alone: open until an agent reads it, seen until
//! it resolves it, by a fix or by what it explains where it works, resolved after. The reader
//! reopens a resolved note with a new line. A note is for the work at hand: what must last, an
//! agent puts in an issue that links to the note's place. A resolved note is shown while the
//! `atlas serve` it was resolved under runs; the next one starts without it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
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
fn file(repo: &Path, name: &str) -> PathBuf {
    crate::made(Some(repo), name)
}

fn read(path: &Path) -> Option<Value> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

/// `value` written beside its place and renamed into it, so a reader never finds half of it.
fn write(path: &Path, value: &impl Serialize) -> Result<()> {
    let partial = path.with_extension(format!("{}.part", std::process::id()));
    std::fs::write(&partial, serde_json::to_vec_pretty(value)?)?;
    std::fs::rename(&partial, path).with_context(|| format!("writing {}", path.display()))
}

/// What the page last said it shows.
pub fn shown(repo: &Path) -> Option<Value> {
    read(&file(repo, "shown.json"))
}

pub fn set_shown(repo: &Path, shown: &Value) -> Result<()> {
    write(&file(repo, "shown.json"), shown)
}

/// Ask the page to go to `view`.
pub fn look(repo: &Path, view: &Value) -> Result<()> {
    write(&file(repo, "look.json"), view)
}

/// The view an agent asked for, taken once.
pub fn take_look(repo: &Path) -> Option<Value> {
    let path = file(repo, "look.json");
    let view = read(&path)?;
    std::fs::remove_file(&path).ok()?;
    Some(view)
}

pub fn notes(repo: &Path) -> Vec<Note> {
    read(&file(repo, "notes.json")).and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default()
}

fn set_notes(repo: &Path, notes: &[Note]) -> Result<()> {
    write(&file(repo, "notes.json"), &notes)
}

/// Keep a new note, open, numbered after the last.
pub fn add(repo: &Path, mut note: Note) -> Result<Note> {
    let mut all = notes(repo);
    note.id = all.iter().map(|n| n.id).max().unwrap_or(0) + 1;
    note.state = State::Open;
    (note.commit, note.issue) = (None, None);
    all.push(note.clone());
    set_notes(repo, &all)?;
    Ok(note)
}

/// Change the note numbered `id`; none when there is no such note.
pub fn change(repo: &Path, id: u32, f: impl FnOnce(&mut Note)) -> Result<Option<Note>> {
    let mut all = notes(repo);
    let Some(n) = all.iter_mut().find(|n| n.id == id) else { return Ok(None) };
    f(n);
    let changed = n.clone();
    set_notes(repo, &all)?;
    Ok(Some(changed))
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

/// A new `atlas serve` starts without the notes resolved before it.
pub fn forget_resolved(repo: &Path) -> Result<()> {
    let all = notes(repo);
    if all.iter().any(|n| n.state == State::Resolved) {
        set_notes(repo, &all.into_iter().filter(|n| n.state != State::Resolved).collect::<Vec<_>>())?;
    }
    Ok(())
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
        let dest = repo.join(rel);
        if let Some(dir) = dest.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&dest, text).with_context(|| format!("writing {}", rel.display()))?;
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

pub fn annotations(repo: &Path) -> Vec<Annotation> {
    read(&file(repo, "annotations.json")).and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default()
}

/// Say `text` of what `on` names, in place of what was said of it; nothing takes it away. Gives
/// what is said now.
pub fn annotate(repo: &Path, on: &str, text: Option<&str>) -> Result<Vec<Annotation>> {
    let mut all: Vec<Annotation> = annotations(repo).into_iter().filter(|a| a.on != on).collect();
    if let Some(text) = text.map(str::trim).filter(|t| !t.is_empty()) {
        all.push(Annotation { on: on.to_string(), text: text.to_string() });
    }
    write(&file(repo, "annotations.json"), &all)?;
    Ok(all)
}

/// Where `atlas serve` serves the page: `http://127.0.0.1:8778/`.
pub fn serving(repo: &Path) -> Option<String> {
    read(&file(repo, "serving.json"))?.get("url")?.as_str().map(str::to_string)
}

pub fn set_serving(repo: &Path, url: &str) -> Result<()> {
    write(&file(repo, "serving.json"), &serde_json::json!({ "url": url }))
}

/// The address of a note's place: the page `atlas serve` serves at `url`, in the view the note was
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

/// The notes an agent has not resolved, marked seen now that it reads them.
pub fn read_open(repo: &Path) -> Result<Vec<Note>> {
    let mut all = notes(repo);
    let open: Vec<Note> = all.iter().filter(|n| matches!(n.state, State::Open | State::Seen)).cloned().collect();
    if open.iter().any(|n| n.state == State::Open) {
        for n in all.iter_mut().filter(|n| n.state == State::Open) {
            n.state = State::Seen;
        }
        set_notes(repo, &all)?;
    }
    Ok(open)
}
