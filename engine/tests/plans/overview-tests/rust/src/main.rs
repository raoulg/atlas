//! A shelf of books: the store reads it, the report shows it, and tests stand beside both.

mod report;
mod store;

fn main() {
    report::render(&store::load("shelf.csv"));
}
