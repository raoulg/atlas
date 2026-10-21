//! An end product: visits appended to a file; nothing here gives them back.

use std::fs;
use std::io;
use std::path::PathBuf;

pub struct Visit {
    pub member: String,
    pub day: String,
}

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
