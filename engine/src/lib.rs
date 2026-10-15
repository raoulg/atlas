//! Atlas engine: `parse: Repo -> Graph` and `view: Graph -> Graph`.
//!
//! The engine knows languages and libraries, never a project.

pub mod body;
pub mod builder;
pub mod cache;
pub mod changes;
pub mod diff;
pub mod effects;
pub mod entries;
pub mod families;
pub mod flow;
pub mod knowledge;
pub mod language;
pub mod model;
pub mod parts;
pub mod paths;
pub mod principles;
pub mod prophecy;
pub mod python;
pub mod pytypes;
pub mod remote;
pub mod repo;
pub mod rust;
pub mod story;
pub mod tree;
pub mod typescript;
pub mod view;

use std::path::{Path, PathBuf};

use anyhow::Result;

pub use model::*;

/// What a reading of a repository takes besides the repository itself. Everything `parse`
/// depends on is here or on disk, so the same options on the same files give the same graph.
#[derive(Clone, Debug)]
pub struct Options {
    /// How far to read into the libraries the code calls: 0 not at all, 1 the library modules the
    /// code's own calls go into, and in each further round the modules of the same library that
    /// the last round's calls go into. The graph keeps of a library what the code's calls reach,
    /// a round a call; what lies beyond is the effect of the function at the border.
    pub libraries: usize,
    /// Files named on the command line, canonical: each is read as one of its project's scripts
    /// or notebooks, even where git ignores it.
    pub named: Vec<PathBuf>,
    /// The Python package directory, when discovery picks the wrong one.
    pub package: Option<PathBuf>,
    /// Where what does not change is kept between readings; none reads everything anew. What is
    /// kept is what a reading gives anew, so the graph is the same either way.
    pub cache: Option<cache::Cache>,
}

impl Default for Options {
    fn default() -> Self {
        Options { libraries: 1, named: vec![], package: None, cache: None }
    }
}

/// Read a repository into its graph: `Disk(repo) -> read -> Repo -> parse -> Graph`. Nothing
/// about the repository is declared to Atlas: what the picture shows is what the code says. A
/// repository whose parts are read in several languages is `parts::analyze_repo`.
pub fn analyze(dir: &Path, opts: &Options) -> Result<Graph> {
    let repo = language::read(dir, opts)?;
    graph_of(&repo, opts)
}

/// The graph of a repository as read: parsed in its language, with the routes its Makefile adds
/// and its parts, kept to what the program reaches.
pub fn graph_of(repo: &repo::Repo, opts: &Options) -> Result<Graph> {
    let lang = language::of(repo, opts)?;
    let mut g = lang.parse(repo, opts)?;
    entries::read_makefile(repo, &mut g);
    parts::assign(&mut g);
    Ok(view::View::apply(&view::reached(&g, opts.libraries), &g))
}
