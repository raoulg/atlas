//! One step writes a log line, a later step opens it and reads it into a string it returns.

use std::fs::{self, File};
use std::io::Read;

fn save() {
    fs::write("data/log.txt", "step").unwrap();
}

fn load() -> String {
    let mut f = File::open("data/log.txt").unwrap();
    let mut s = String::new();
    f.read_to_string(&mut s).unwrap();
    s
}

fn main() {
    save();
    println!("{}", load());
}
