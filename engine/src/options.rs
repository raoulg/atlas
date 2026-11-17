//! The options of a reading, which the readers, `language` and `parts` take.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use crate::cache;

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
    /// What the other parts of the repository reach of a library this part reads from the same
    /// directory, which the reading reaches as if its own code did: the project is all its parts.
    pub reach: Reach,
}

impl Default for Options {
    fn default() -> Self {
        Options { libraries: 1, named: vec![], package: None, cache: None, reach: Reach::default() }
    }
}

/// What code outside a reading reaches of the libraries it reads, by the directory each library's
/// code is read from.
pub type Reach = BTreeMap<String, BTreeSet<Entry>>;

/// A node of a library that code reaches directly: by its id below the library's root (`save`,
/// `store.Store`; empty for the root), the module it is in, below the root too, the file that
/// defines it, and whether the code names it as a type rather than calling it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Entry {
    pub id: String,
    pub module: String,
    pub file: String,
    pub named: bool,
}
