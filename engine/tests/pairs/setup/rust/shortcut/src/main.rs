//! The program sets the shelf up in a step of its own, which decides nothing, and then keeps its
//! report and its log on it.

mod store;

use store::Shelf;

fn open_shelf() -> Shelf {
    Shelf::new()
}

fn main() {
    let shelf = open_shelf();
    shelf.keep_report("all well");
    shelf.keep_log("ran");
}
