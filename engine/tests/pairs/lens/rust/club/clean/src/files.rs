//! What the club keeps in files: a log of visits and its roster.

use std::fs;
use std::io;
use std::path::PathBuf;

use crate::{Member, Visit};

/// An end product: each visit is appended to the file; nothing gives them back.
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
}

/// The members, kept in a file, a name a line: `load` and `reread` give back a Roster that holds them.
pub struct Roster {
    path: PathBuf,
    members: Vec<Member>,
}

impl Roster {
    pub fn load(path: PathBuf) -> io::Result<Roster> {
        let members = fs::read_to_string(&path)?.lines().map(|name| Member { name: name.to_string() }).collect();
        Ok(Roster { path, members })
    }

    pub fn reread(&self) -> io::Result<Roster> {
        let members = fs::read_to_string(&self.path)?.lines().map(|name| Member { name: name.to_string() }).collect();
        Ok(Roster { path: self.path.clone(), members })
    }

    pub fn add(&self, who: &Member) -> io::Result<()> {
        let names: Vec<&str> = self.members.iter().chain([who]).map(|m| m.name.as_str()).collect();
        fs::write(&self.path, names.join("\n"))
    }
}
