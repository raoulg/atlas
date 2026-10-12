//! `parse: Repo -> Graph`, one implementation per language.
//!
//! What a language module owns is small: which repos it recognises, how files map to
//! module ids, how names resolve, which calls are boundaries, what counts as an entry
//! point. The grammar itself is reused from tree-sitter.

use std::path::Path;

use anyhow::Result;

use crate::model::{Graph, Leaves};
use crate::{entries, python, rust, typescript, Options};

pub trait Language {
    fn name(&self) -> &'static str;
    /// The file that roots a part in the language.
    fn manifest(&self) -> &'static str;
    /// Does this repo look like one of mine?
    fn detect(&self, repo: &Path, opts: &Options) -> bool;
    /// `Disk(repo) -> Graph`: every file, its symbols, the calls and types between them, the
    /// routes into the program, and the calls that leave it.
    fn parse(&self, repo: &Path, opts: &Options) -> Result<Graph>;
    /// What the language leaves its author to write.
    fn leaves(&self) -> Leaves;
}

pub struct Python;

impl Language for Python {
    fn name(&self) -> &'static str {
        "python"
    }
    fn manifest(&self) -> &'static str {
        "pyproject.toml"
    }
    fn detect(&self, repo: &Path, opts: &Options) -> bool {
        repo.join("pyproject.toml").exists() || python::discover_package(repo).is_ok() || !python::named_under(repo, &opts.named).is_empty()
    }
    fn parse(&self, repo: &Path, opts: &Options) -> Result<Graph> {
        // a project of scripts and notebooks has no package: a file named on the command line
        // in a folder without one, or a project file with none beside it
        let pkg_dir = match &opts.package {
            Some(p) => Some(p.clone()),
            None => match python::discover_package(repo) {
                Ok(p) => Some(p),
                Err(_) if repo.join("pyproject.toml").exists() || !python::named_under(repo, &opts.named).is_empty() => None,
                Err(e) => return Err(e),
            },
        };
        let project_root = pkg_dir.as_deref().and_then(python::project_root).or_else(|| (repo.join("pyproject.toml").exists() || pkg_dir.is_none()).then(|| repo.to_path_buf()));
        let (mut b, root) = python::parse(pkg_dir.as_deref(), project_root.as_deref(), opts)?;
        entries::mark_scripts(&mut b, project_root.as_deref());
        Ok(b.finish(root, project_root.as_deref(), self.name()))
    }
    /// A signature may say nothing, and a package offers whatever its root binds unless
    /// `__all__` names it.
    fn leaves(&self) -> Leaves {
        Leaves { exports: true, types: true, returns: true }
    }
}

pub struct Rust;

impl Language for Rust {
    fn name(&self) -> &'static str {
        "rust"
    }
    fn manifest(&self) -> &'static str {
        "Cargo.toml"
    }
    fn detect(&self, repo: &Path, _opts: &Options) -> bool {
        repo.join("Cargo.toml").exists()
    }
    fn parse(&self, repo: &Path, _opts: &Options) -> Result<Graph> {
        let (b, root) = rust::parse(repo)?;
        Ok(b.finish(root, Some(repo), self.name()))
    }
    /// The compiler asks for every type, and `pub` is the export.
    fn leaves(&self) -> Leaves {
        Leaves::default()
    }
}

pub struct TypeScript;

impl Language for TypeScript {
    fn name(&self) -> &'static str {
        "typescript"
    }
    fn manifest(&self) -> &'static str {
        "package.json"
    }
    fn detect(&self, repo: &Path, _opts: &Options) -> bool {
        typescript::detect(repo)
    }
    fn parse(&self, repo: &Path, _opts: &Options) -> Result<Graph> {
        let (b, root) = typescript::parse(repo)?;
        Ok(b.finish(root, Some(repo), self.name()))
    }
    /// The compiler works out what a function gives, which a reader of its declaration does not
    /// see (`isolatedDeclarations` asks for it).
    fn leaves(&self) -> Leaves {
        Leaves { returns: true, ..Default::default() }
    }
}

/// What the languages look for: the manifests that root a part, and a package directory.
pub fn looked_for() -> String {
    let manifests: Vec<&str> = languages().iter().map(|l| l.manifest()).collect();
    format!("looked for {}, or a package directory", manifests.join(", "))
}

/// What the language of that name leaves its author to write.
pub fn leaves(name: &str) -> Leaves {
    languages().into_iter().find(|l| l.name() == name).map_or_else(Leaves::default, |l| l.leaves())
}

/// All known languages, in detection order.
pub fn languages() -> Vec<Box<dyn Language>> {
    vec![Box::new(Rust), Box::new(Python), Box::new(TypeScript)]
}

/// The language a repo is written in; an explicit package directory means Python.
pub fn detect(repo: &Path, opts: &Options) -> Option<Box<dyn Language>> {
    if opts.package.is_some() {
        return Some(Box::new(Python));
    }
    languages().into_iter().find(|l| l.detect(repo, opts))
}
