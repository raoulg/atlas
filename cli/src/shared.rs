//! What the reader of a served page and an agent share, kept in `.atlas/` beside the pages, so that
//! `atlas serve` and `atlas mcp` need not know each other:
//!
//! - `shown.json`: the view the page shows, and the boxes and lines it draws, as the page last said;
//! - `look.json`: a view an agent asks the page to go to, taken by the page once;
//! - `notes.json`: what the reader wrote on a box or a line, and what an agent answered.
//!
//! A note is resolved by an agent, never by reading alone: open until an agent reads it, seen until
//! it answers, answered until the reader has read the answer, then closed, and kept as history.

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
    Answered,
    Closed,
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
    pub text: String,
    #[serde(default)]
    pub state: State,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
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
    note.answer = None;
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

/// The notes an agent has not answered, marked seen now that it reads them.
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
