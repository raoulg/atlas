//! A store of notes and the line a report makes of it: the route makes the line itself, and throws away what the step
//! that saves it gives back.

use std::fs::OpenOptions;
use std::io::Write;

pub struct Store {
    pub path: String,
    pub count: usize,
}

fn save(store: &Store, line: &str) -> String {
    let mut file = OpenOptions::new().append(true).create(true).open(&store.path).unwrap();
    writeln!(file, "{line}").unwrap();
    line.to_string()
}

fn main() {
    let store = Store { path: "notes.txt".to_string(), count: 3 };
    let line = format!("{}: {} notes", store.path, store.count);
    save(&store, &line);
    println!("{line}");
}
