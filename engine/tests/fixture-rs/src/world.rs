//! Values from libraries, moving data where they are used.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use reqwest::blocking::Client;
use rusqlite::Connection;

pub struct Page {
    pub body: String,
}

pub struct Log {
    path: String,
}

/// What a line of the log holds: one of its variants.
pub enum Line {
    Text(String),
    Fetched(Page),
    Kept(Log),
    Empty,
}

impl Log {
    pub fn append(&self, line: &str) -> std::io::Result<()> {
        let mut f = OpenOptions::new().append(true).open(&self.path)?;
        f.write_all(line.as_bytes())
    }
}

pub fn client() -> Client {
    Client::builder().build().unwrap()
}

pub fn fetch(url: &str) -> Page {
    let client = Client::new();
    Page { body: client.get(url).send().unwrap().text().unwrap() }
}

pub fn record(n: u32) {
    let conn = Connection::open("ledger.db").unwrap();
    conn.execute("INSERT INTO log (n) VALUES (?1)", [n]).unwrap();
}

pub fn notes() -> String {
    let mut s = String::new();
    File::open("notes.txt").unwrap().read_to_string(&mut s).unwrap();
    s
}

pub fn clean() -> bool {
    Command::new("git").arg("status").status().map(|s| s.success()).unwrap_or(false)
}

pub fn quit(code: i32) {
    std::process::exit(code)
}

pub fn weight(kind: &str) -> u32 {
    if matches!(kind, "page" | "log") {
        2
    } else {
        1
    }
}

const REPORT: &str = "report.html";
const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS seen (id TEXT)";

/// Writes the report into the directory its caller names.
pub fn report(dir: &Path, body: &str) -> std::io::Result<()> {
    std::fs::write(dir.join(REPORT), body)
}

pub fn publish(body: &str) -> std::io::Result<()> {
    report(Path::new("site"), body)
}

pub fn stamp(name: &str) -> std::io::Result<()> {
    std::fs::write(format!("out/{name}.txt"), "")
}

pub fn scratch() -> PathBuf {
    std::env::temp_dir().join("handed").join("scratch.log")
}

pub fn keep_scratch(line: &str) -> std::io::Result<()> {
    std::fs::write(scratch(), line)
}

pub fn prepare(path: &str) {
    let conn = Connection::open(path).unwrap();
    conn.execute(SCHEMA, []).unwrap();
}

/// Runs whatever statement its caller hands it.
pub fn run(path: &str, sql: &str) -> usize {
    let conn = Connection::open(path).unwrap();
    conn.execute(sql, []).unwrap()
}

pub fn forget(path: &str) -> usize {
    run(path, "DELETE FROM seen")
}
