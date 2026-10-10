//! Atlas engine: `parse: Repo -> Graph` and `view: Graph -> Graph`.
//!
//! The engine knows languages and libraries, never a project.

pub mod builder;
pub mod changes;
pub mod effects;
pub mod entries;
pub mod language;
pub mod view;
pub mod model;
pub mod parts;
pub mod paths;
pub mod principles;
pub mod python;
pub mod pytypes;
pub mod roles;
pub mod rust;
pub mod story;
pub mod tree;
pub mod typescript;

use std::path::Path;

use anyhow::Result;

pub use model::*;

/// Parse a repository into its graph: `Disk(repo) -> Language.parse -> Graph`, then the
/// routes its Makefile adds, the roles it declares (or that are inferred) and its parts.
/// A repository whose parts are read in several languages is `parts::analyze_repo`.
///
/// `package` overrides package discovery; `roles` points at an `architecture.yml` or a
/// `roles.json`, otherwise the repo's own declaration is used, or roles are inferred.
pub fn analyze(repo: &Path, package: Option<&Path>, roles: Option<&Path>) -> Result<Graph> {
    let lang = language::detect(repo, package)
        .ok_or_else(|| anyhow::anyhow!("no language recognised in {} (looked for Cargo.toml, pyproject.toml, a Python package, package.json)", repo.display()))?;
    let mut g = lang.parse(repo, package)?;
    entries::read_makefile(&mut g);
    let arch = roles::resolve(&g, roles)?;
    roles::assign(&mut g, arch);
    parts::assign(&mut g);
    Ok(g)
}
