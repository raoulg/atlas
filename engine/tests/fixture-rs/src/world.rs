//! Values from libraries, moving data where they are used.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
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
