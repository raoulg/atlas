//! What the shelf looks like on the screen.

use crate::store::Shelf;

pub fn render(shelf: &Shelf) {
    for title in &shelf.titles {
        println!("{title}");
    }
}
