//! The library of a workspace of two crates: a model and the views of it.

pub mod model;
pub mod view;

pub fn analyze(text: &str) -> model::Graph {
    model::Graph { lines: text.lines().count() }
}
