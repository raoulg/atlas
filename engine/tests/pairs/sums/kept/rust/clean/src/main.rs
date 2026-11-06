//! The attachments of a knowledge base, each kept in a file of its own or in a folder with an index: where one is
//! kept is a variant, built with its path and taken apart where it is read.

use std::fs;
use std::path::PathBuf;

/// Where an attachment is kept.
enum Plek {
    Bestand(PathBuf),
    Map { dir: PathBuf },
}

impl Plek {
    /// The attachment's text.
    fn lees(&self) -> String {
        match self {
            Plek::Bestand(pad) => fs::read_to_string(pad).unwrap(),
            Plek::Map { dir } => fs::read_to_string(dir.join("index.txt")).unwrap(),
        }
    }
}

fn main() {
    let plekken = [Plek::Bestand(PathBuf::from("bijlagen/bron.txt")), Plek::Map { dir: PathBuf::from("bijlagen/los") }];
    for plek in &plekken {
        println!("{}", plek.lees());
    }
}
