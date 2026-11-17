//! The program keeps its report on the shelf, sets up the drafts, and reads the report from the
//! shelf it has at hand last, the drafts, where it never kept one.

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
    let drafts = open_drafts();
    drafts.report()
}

fn main() {
    println!("{}", review());
}
