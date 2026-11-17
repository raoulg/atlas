//! The program sets the shelf up in a step of its own, which decides its folder, and then keeps
//! its report and its log on it.

mod store;

use std::path::PathBuf;

use store::Shelf;

fn open_shelf() -> Shelf {
    Shelf::new(PathBuf::from("out"))
}

fn main() {
    let shelf = open_shelf();
    shelf.keep_report("all well");
    shelf.keep_log("ran");
}
