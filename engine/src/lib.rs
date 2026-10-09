//! Atlas engine: `parse: Repo -> Graph` and `lens: Graph -> Graph`.
//!
//! The engine knows languages and libraries, never a project.

pub mod builder;
pub mod effects;
pub mod entries;
pub mod language;
pub mod lens;
pub mod model;
pub mod principles;
pub mod python;
pub mod roles;
pub mod rust;
pub mod story;
pub mod tree;
pub mod typescript;

use std::path::Path;

use anyhow::Result;

pub use model::*;

/// Parse a repository into its graph: `Disk(repo) -> Language.parse -> Graph`, then the
/// routes its Makefile adds and the roles it declares (or that are inferred).
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
    Ok(g)
}
