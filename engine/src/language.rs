//! `parse: Repo -> Graph`, one implementation per language.
//!
//! What a language module owns is small: which repos it recognises, how files map to
//! module ids, how names resolve, which calls are boundaries, what counts as an entry
//! point. The grammar itself is reused from tree-sitter.

use std::path::Path;

use anyhow::Result;

use crate::model::Graph;
use crate::{entries, python, rust, typescript};

pub trait Language {
    fn name(&self) -> &'static str;
    /// Does this repo look like one of mine?
    fn detect(&self, repo: &Path) -> bool;
    /// `Disk(repo) -> Graph`: every file, its symbols, the calls and types between them, the
    /// routes into the program, and the calls that leave it.
    fn parse(&self, repo: &Path, package: Option<&Path>) -> Result<Graph>;
}

pub struct Python;

impl Language for Python {
    fn name(&self) -> &'static str {
        "python"
    }
    fn detect(&self, repo: &Path) -> bool {
        repo.join("pyproject.toml").exists() || python::discover_package(repo).is_ok()
    }
    fn parse(&self, repo: &Path, package: Option<&Path>) -> Result<Graph> {
        let pkg_dir = match package {
            Some(p) => p.to_path_buf(),
            None => python::discover_package(repo)?,
        };
        let project_root = python::project_root(&pkg_dir).or_else(|| repo.join("pyproject.toml").exists().then(|| repo.to_path_buf()));
        let (mut b, root) = python::parse(&pkg_dir, project_root.as_deref())?;
        entries::mark_scripts(&mut b, project_root.as_deref());
        Ok(b.finish(root, project_root.as_deref(), self.name()))
    }
}

pub struct Rust;

impl Language for Rust {
    fn name(&self) -> &'static str {
        "rust"
    }
    fn detect(&self, repo: &Path) -> bool {
        repo.join("Cargo.toml").exists()
    }
    fn parse(&self, repo: &Path, _package: Option<&Path>) -> Result<Graph> {
        let (b, root) = rust::parse(repo)?;
        Ok(b.finish(root, Some(repo), self.name()))
    }
}

pub struct TypeScript;

impl Language for TypeScript {
    fn name(&self) -> &'static str {
        "typescript"
    }
    fn detect(&self, repo: &Path) -> bool {
        typescript::detect(repo)
    }
    fn parse(&self, repo: &Path, _package: Option<&Path>) -> Result<Graph> {
        let (b, root) = typescript::parse(repo)?;
        Ok(b.finish(root, Some(repo), self.name()))
    }
}

/// Whether a language leaves types and exports to the author: a Python signature may say
/// nothing, and a Python package offers whatever its root binds. Rust's compiler asks for
/// every type, and `pub` is the export.
pub fn author_declares(language: &str) -> bool {
    language == "python"
}

/// All known languages, in detection order.
pub fn languages() -> Vec<Box<dyn Language>> {
    vec![Box::new(Rust), Box::new(Python), Box::new(TypeScript)]
}

/// The language a repo is written in; an explicit package directory means Python.
pub fn detect(repo: &Path, package: Option<&Path>) -> Option<Box<dyn Language>> {
    if package.is_some() {
        return Some(Box::new(Python));
    }
    languages().into_iter().find(|l| l.detect(repo))
}
