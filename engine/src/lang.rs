//! The languages Atlas reads, by name: what a part, a repository and a graph are read in.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A language Atlas reads, written by its name: `python`, `rust`, `typescript`.
#[derive(Serialize, Deserialize, TS, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Lang {
    Python,
    Rust,
    TypeScript,
}

impl Lang {
    /// Every language, in the order of their names.
    pub const ALL: [Lang; 3] = [Lang::Python, Lang::Rust, Lang::TypeScript];

    /// Its name, as the JSON writes it.
    pub fn name(self) -> &'static str {
        match self {
            Lang::Python => "python",
            Lang::Rust => "rust",
            Lang::TypeScript => "typescript",
        }
    }
}

impl std::fmt::Display for Lang {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}
