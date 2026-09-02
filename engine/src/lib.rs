//! Atlas engine: `parse: Repo -> Graph` and `lens: Graph -> Graph`.
//!
//! The engine knows languages and libraries, never a project.

pub mod effects;
pub mod entries;
pub mod lens;
pub mod model;
pub mod python;
pub mod roles;
pub mod rust;
pub mod store;
pub mod tree;

use std::path::Path;

use anyhow::Result;

pub use model::*;

/// Parse a repository into its graph.
///
/// `package` overrides package discovery; `roles` points at an `architecture.yml` or a
/// `roles.json`, otherwise the repo's own declaration is used, or roles are inferred.
pub fn analyze(repo: &Path, package: Option<&Path>, roles: Option<&Path>) -> Result<Graph> {
    let (mut store, root, project_root) = if package.is_none() && repo.join("Cargo.toml").exists() {
        let (store, root) = rust::parse(repo)?;
        (store, root, Some(repo.to_path_buf()))
    } else {
        let pkg_dir = match package {
            Some(p) => p.to_path_buf(),
            None => python::discover_package(repo)?,
        };
        let project_root = python::project_root(&pkg_dir).or_else(|| repo.join("pyproject.toml").exists().then(|| repo.to_path_buf()));
        let (store, root) = python::parse(&pkg_dir, project_root.as_deref())?;
        (store, root, project_root)
    };
    let scripts = entries::mark_scripts(&mut store, project_root.as_deref());
    let arch = match roles {
        Some(p) => roles::load(p)?,
        None => match roles::find(project_root.as_deref()) {
            Some(a) => a,
            None => roles::infer(&mut store, &root),
        },
    };
    roles::assign(&mut store, &arch);
    let make = entries::read_makefile(&store, project_root.as_deref(), &scripts);
    let mut top: Vec<(String, u32)> = store.unresolved.iter().map(|(k, v)| (k.clone(), *v)).collect();
    top.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let stats = Stats {
        unresolved_calls_total: store.unresolved.values().sum(),
        unresolved_calls_top: top.into_iter().take(25).collect(),
        ..Default::default()
    };
    let mut g = Graph {
        root,
        roles: arch,
        nodes: store.nodes.into_values().collect(),
        edges: store.edges,
        files: store.files,
        make,
        stats,
    };
    g.recount();
    Ok(g)
}
