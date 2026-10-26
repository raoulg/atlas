//! One step writes a log line, a later step reads it and hands it on.

use std::fs;

fn save() {
    fs::write("data/log.txt", "step").unwrap();
}

fn load() -> String {
    fs::read_to_string("data/log.txt").unwrap()
}

fn main() {
    save();
    println!("{}", load());
}
