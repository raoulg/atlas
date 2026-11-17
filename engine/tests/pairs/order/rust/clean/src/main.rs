//! The program keeps its report on the shelf and reads it back from that shelf; only then does it
//! set up the drafts, whose folder it names.

mod store;

use std::path::PathBuf;

use store::Shelf;

fn open_shelf() -> Shelf {
    Shelf::new(PathBuf::from("out"))
}

fn open_drafts() -> Shelf {
    Shelf::new(PathBuf::from("drafts"))
}

fn review() -> String {
    let shelf = open_shelf();
    shelf.keep_report("all well");
    let report = shelf.report();
    let drafts = open_drafts();
    println!("{}", drafts.root.display());
    report
}

fn main() {
    println!("{}", review());
}
