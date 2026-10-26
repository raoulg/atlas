//! A store of notes and the line a report makes of it: each value goes to the step that uses it, and what a step gives
//! back the route uses.

use std::fs::OpenOptions;
use std::io::Write;

pub struct Store {
    pub path: String,
    pub count: usize,
}

fn describe(store: &Store) -> String {
    format!("{}: {} notes", store.path, store.count)
}

fn save(store: &Store, line: &str) -> String {
    let mut file = OpenOptions::new().append(true).create(true).open(&store.path).unwrap();
    writeln!(file, "{line}").unwrap();
    line.to_string()
}

fn main() {
    let store = Store { path: "notes.txt".to_string(), count: 3 };
    let line = describe(&store);
    let kept = save(&store, &line);
    println!("{kept}");
}
