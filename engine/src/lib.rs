//! Atlas engine: `parse: Repo -> Graph` and `view: Graph -> Graph`.
//!
//! The engine knows languages and libraries, never a project.

pub mod body;
pub mod builder;
pub mod changes;
pub mod effects;
pub mod entries;
pub mod knowledge;
pub mod families;
pub mod flow;
pub mod language;
pub mod view;
pub mod model;
pub mod parts;
pub mod paths;
pub mod principles;
pub mod remote;
pub mod python;
pub mod pytypes;
pub mod rust;
pub mod story;
pub mod tree;
pub mod typescript;

use std::path::{Path, PathBuf};

use anyhow::Result;

pub use model::*;

/// What a reading of a repository takes besides the repository itself. Everything `parse`
/// depends on is here or on disk, so the same options on the same files give the same graph.
#[derive(Clone, Debug)]
pub struct Options {
    /// How far to read into the libraries the code calls: 0 not at all, 1 the library modules the
    /// code's own calls go into, and in each further round the modules of the same library that
    /// the last round's calls go into.
    pub libraries: usize,
    /// Files named on the command line, canonical: each is read as one of its project's scripts
    /// or notebooks, even where git ignores it.
    pub named: Vec<PathBuf>,
    /// The Python package directory, when discovery picks the wrong one.
    pub package: Option<PathBuf>,
}

impl Default for Options {
    fn default() -> Self {
        Options { libraries: 1, named: vec![], package: None }
    }
}

/// Parse a repository into its graph: `Disk(repo) -> Language.parse -> Graph`, then the
/// routes its Makefile adds and its parts. Nothing about the repository is declared to Atlas:
/// what the picture shows is what the code says. A repository whose parts are read in several
/// languages is `parts::analyze_repo`.
pub fn analyze(repo: &Path, opts: &Options) -> Result<Graph> {
    let lang = language::detect(repo, opts)
        .ok_or_else(|| anyhow::anyhow!("no language recognised in {} ({})", repo.display(), language::looked_for()))?;
    let mut g = lang.parse(repo, opts)?;
    entries::read_makefile(&mut g);
    parts::assign(&mut g);
    Ok(view::View::apply(&view::reached(&g), &g))
}
