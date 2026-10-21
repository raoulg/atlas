//! What the club keeps in files: a log of visits and its roster.

use std::fs;
use std::io;
use std::path::PathBuf;

use crate::{Member, Visit};

/// Appends each visit to the file and gives back the lines it is kept as.
pub struct Log {
    path: PathBuf,
}

impl Log {
    pub fn new(path: PathBuf) -> Log {
        Log { path }
    }

    pub fn add(&self, visit: &Visit) -> io::Result<()> {
        let mut text = fs::read_to_string(&self.path)?;
        text.push_str(&format!("{} {}\n", visit.member, visit.day));
        fs::write(&self.path, text)
    }

    pub fn lines(&self) -> io::Result<Vec<String>> {
        Ok(fs::read_to_string(&self.path)?.lines().map(str::to_string).collect())
    }
}

/// The members, kept in a file, a name a line: `load` gives back a Roster that holds the lines.
pub struct Roster {
    path: PathBuf,
    lines: Vec<String>,
}

impl Roster {
    pub fn load(path: PathBuf) -> io::Result<Roster> {
        let lines = fs::read_to_string(&path)?.lines().map(str::to_string).collect();
        Ok(Roster { path, lines })
    }

    pub fn add(&self, who: &Member) -> io::Result<()> {
        let names: Vec<&str> = self.lines.iter().map(String::as_str).chain([who.name.as_str()]).collect();
        fs::write(&self.path, names.join("\n"))
    }
}
