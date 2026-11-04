//! Atlas engine: `parse: Repo -> Graph` and `view: Graph -> Graph`.
//!
//! The engine knows languages and libraries, never a project.

pub mod aside;
pub mod body;
pub mod builder;
pub mod cache;
pub mod changes;
pub mod clock;
pub mod decide;
pub mod diff;
pub mod effects;
pub mod entries;
pub mod environment;
pub mod families;
pub mod flow;
pub mod knowledge;
pub mod lang;
pub mod language;
pub mod model;
pub mod options;
pub mod parts;
pub mod paths;
pub mod places;
pub mod principles;
pub mod prophecy;
pub mod python;
pub mod pytypes;
pub mod remote;
pub mod repo;
pub mod revelation;
pub mod rust;
pub mod shapes;
pub mod sight;
pub mod story;
pub mod tree;
pub mod typescript;
pub mod view;

use std::path::Path;

use anyhow::Result;

pub use model::*;
pub use options::Options;

/// Read a repository into its graph: `Disk(repo) -> read -> Repo -> parse -> Graph`. Nothing
/// about the repository is declared to Atlas: what the picture shows is what the code says. A
/// repository whose parts are read in several languages is `parts::analyze_repo`.
pub fn analyze(dir: &Path, opts: &Options) -> Result<Graph> {
    let repo = language::read(dir, opts)?;
    parts::graph_of(&repo, opts)
}
