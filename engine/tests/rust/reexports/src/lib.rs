//! A crate whose root re-exports all of its model.

mod model;

pub use model::*;

pub fn analyze(text: &str) -> Graph {
    Graph { lines: text.lines().count() }
}
