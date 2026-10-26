//! Graphs read and merged, an index over their edges, and a syntax table of functions.

pub mod flow;

use anyhow::Result;

pub struct Edge {
    pub from: String,
}

pub struct Graph {
    pub edges: Vec<Edge>,
}

pub fn read_all(texts: &[String]) -> Result<Vec<Graph>> {
    Ok(texts.iter().map(|t| Graph { edges: vec![Edge { from: t.clone() }] }).collect())
}

pub fn merged(graphs: Vec<Graph>) -> Graph {
    Graph { edges: graphs.into_iter().flat_map(|g| g.edges).collect() }
}

pub struct Part {
    pub name: String,
}

/// How a language writes a choice: what a child is to it.
pub struct ChoiceSyntax {
    pub part: fn(&str) -> Part,
}
