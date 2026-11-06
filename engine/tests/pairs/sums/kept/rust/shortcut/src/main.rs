//! The attachments of a knowledge base, each kept in a file of its own or in a folder with an index: where one is
//! kept is a string that says the kind before its path, split again where it is read.

use std::fs;
use std::path::Path;

/// The attachment's text.
fn lees(plek: &str) -> String {
    match plek.split_once(':') {
        Some(("map", dir)) => fs::read_to_string(Path::new(dir).join("index.txt")).unwrap(),
        Some((_, pad)) => fs::read_to_string(pad).unwrap(),
        None => String::new(),
    }
}

fn main() {
    let plekken = [format!("bestand:{}", "bijlagen/bron.txt"), format!("map:{}", "bijlagen/los")];
    for plek in &plekken {
        println!("{}", lees(plek));
    }
}
