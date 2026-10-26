//! A reading in steps: the language a repository is in is found, it parses the repository into a
//! graph, and the graph is completed in two steps that run in order.

pub mod builder;
pub mod collect;
pub mod language;
pub mod model;

use anyhow::Result;

use crate::model::{Graph, Options, Repo};

pub fn graph_of(repo: &Repo, opts: &Options) -> Result<Graph> {
    let lang = language::of(repo, opts)?;
    let mut g = lang.parse(repo, opts)?;
    read_makefile(repo, &mut g);
    assign(&mut g);
    Ok(g)
}

fn read_makefile(repo: &Repo, g: &mut Graph) {
    g.names.push(repo.root.clone());
}

fn assign(g: &mut Graph) {
    g.names.sort();
}
