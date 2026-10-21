//! Write a log line and read it back to show it, in one body: what is read back is used.

use std::fs;

fn note() -> String {
    fs::write("data/log.txt", "step").unwrap();
    fs::read_to_string("data/log.txt").unwrap()
}

fn main() {
    println!("{}", note());
}
