//! For each word: a word with + replaces the note; every word shows the note.

use std::fs;

fn main() {
    for word in std::env::args().skip(1) {
        if word.starts_with('+') {
            fs::write("data/note.txt", &word).unwrap();
        }
        println!("{}", fs::read_to_string("data/note.txt").unwrap());
    }
}
