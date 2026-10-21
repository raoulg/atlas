//! `parse: Repo -> Graph`, one implementation per language.
//!
//! What a language module owns is small: which repos it recognises, how files map to
//! module ids, how names resolve, which calls are boundaries, what counts as an entry
//! point. The grammar itself is reused from tree-sitter.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::cache::Check;
use crate::model::{Graph, Leaves};
use crate::repo::Repo;
use crate::{entries, python, pytypes, rust, typescript, Options};

pub trait Language {
    fn name(&self) -> &'static str;
    /// The file that roots a part in the language.
    fn manifest(&self) -> &'static str;
    /// Does this repo look like one of mine?
    fn detect(&self, repo: &Repo, opts: &Options) -> bool;
    /// `Disk(repo) -> Graph`: every file, its symbols, the calls and types between them, the
    /// routes into the program, and the calls that leave it.
    fn parse(&self, repo: &Repo, opts: &Options) -> Result<Graph>;
    /// What the language leaves its author to write.
    fn leaves(&self) -> Leaves;
    /// Whether the language compares types by their members rather than by what they are declared
    /// as.
    fn structural(&self) -> bool {
        false
    }
    /// Whether the part read at `part` reads the directory `dir` below it, which holds a manifest
    /// of the language, as its own; a manifest no part around it reads roots a part.
    fn reads(&self, part: &Repo, dir: &Path) -> bool;
    /// The project a package named on its own is in, when it lies above what was read: its
    /// directory, and the package.
    fn project_above(&self, _repo: &Repo, _opts: &Options) -> Option<(PathBuf, PathBuf)> {
        None
    }
    /// What a reading of the part at `repo` takes from outside the repository: where the code of
    /// the libraries it reads lies, as checks of how each place stands.
    fn environment(&self, _repo: &Repo, _opts: &Options) -> Vec<Check> {
        vec![]
    }
}

/// `Disk(dir) -> read -> Repo`: the repository at `dir`, or for a package named on its own the
/// project it is in, asked for that package. Only this looks at the disk above what it reads;
/// what is read after it is the `Repo`.
pub fn read(dir: &Path, opts: &Options) -> Result<Repo> {
    let repo = Repo::read(dir, &opts.named)?;
    match languages().iter().find_map(|l| l.project_above(&repo, opts)) {
        Some((root, package)) => Ok(Repo::read(&root, &opts.named)?.asked_for(package)),
        None => Ok(repo),
    }
}

pub struct Python;

impl Language for Python {
    fn name(&self) -> &'static str {
        "python"
    }
    fn manifest(&self) -> &'static str {
        "pyproject.toml"
    }
    fn detect(&self, repo: &Repo, _opts: &Options) -> bool {
        repo.is_file("pyproject.toml") || !python::packages(repo).is_empty() || !repo.named().is_empty()
    }
    fn parse(&self, repo: &Repo, opts: &Options) -> Result<Graph> {
        let (packages, project_root) = Python::reading(repo, opts);
        // a project of scripts and notebooks has no package: a file named on the command line
        // in a folder without one, or a project file with none beside it
        if packages.is_empty() && !repo.is_file("pyproject.toml") && repo.named().is_empty() {
            anyhow::bail!(
                "no Python package found under {} (looked for a pyproject.toml, src/<pkg>/__init__.py and <pkg>/__init__.py)",
                repo.root().display()
            );
        }
        let (mut b, root) = python::parse(repo, &packages, project_root.as_deref(), opts)?;
        entries::mark_scripts(repo, &mut b, project_root.as_deref());
        Ok(b.finish(root, project_root.as_deref(), self.name()))
    }
    /// A project reads its packages; a project file anywhere else below it roots a project of
    /// its own (a workspace's member).
    fn reads(&self, part: &Repo, dir: &Path) -> bool {
        python::packages(part).iter().any(|p| dir.starts_with(p))
    }
    /// A signature may say nothing, a package offers whatever its root binds unless `__all__` names
    /// it, and nothing holds an override to its base.
    fn leaves(&self) -> Leaves {
        Leaves { exports: true, types: true, returns: true, overrides: true }
    }
    /// A package without a `pyproject.toml` of its own is in the nearest of the three directories
    /// above it that has one.
    fn project_above(&self, repo: &Repo, opts: &Options) -> Option<(PathBuf, PathBuf)> {
        if repo.is_file("pyproject.toml") {
            return None;
        }
        let package = opts.package.clone().or_else(|| python::packages(repo).into_iter().next())?;
        let root = package.ancestors().take(python::PROJECT_ABOVE).find(|p| match p.starts_with(repo.root()) {
            true => repo.is_file(p.join("pyproject.toml")),
            false => p.join("pyproject.toml").is_file(),
        })?;
        (!root.starts_with(repo.root())).then(|| (root.to_path_buf(), package.clone()))
    }
    /// Where ty finds the modules the project and its libraries import: the project's types come
    /// from there however far its libraries are read.
    fn environment(&self, repo: &Repo, opts: &Options) -> Vec<Check> {
        let (packages, project_root) = Python::reading(repo, opts);
        pytypes::environment(&python::types_root(&packages, project_root.as_deref()))
    }
}

impl Python {
    /// The packages a reading reads, the one asked for first, and the project they are in.
    fn reading(repo: &Repo, opts: &Options) -> (Vec<PathBuf>, Option<PathBuf>) {
        let asked = opts.package.as_deref().or(repo.asked());
        let mut packages: Vec<PathBuf> = asked.map(Path::to_path_buf).into_iter().collect();
        packages.extend(python::packages(repo).into_iter().filter(|p| Some(p.as_path()) != asked));
        let first = packages.first().map(PathBuf::as_path);
        let project_root = first
            .and_then(|p| python::project_root(repo, p))
            .or_else(|| (repo.is_file("pyproject.toml") || first.is_none()).then(|| repo.root().to_path_buf()));
        (packages, project_root)
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
    fn detect(&self, repo: &Repo, _opts: &Options) -> bool {
        repo.is_file("Cargo.toml")
    }
    fn parse(&self, repo: &Repo, opts: &Options) -> Result<Graph> {
        let (b, root) = rust::parse(repo, opts)?;
        Ok(b.finish(root, Some(repo.root()), self.name()))
    }
    /// The compiler asks for every type, `pub` is the export, and a trait's implementation has the
    /// trait's signature.
    fn leaves(&self) -> Leaves {
        Leaves::default()
    }
    /// A workspace reads its members; a crate below it that is none (`fuzz/`) is a part of its own.
    fn reads(&self, part: &Repo, dir: &Path) -> bool {
        rust::crates(part).map_or(false, |cs| cs.iter().any(|c| c.dir == dir))
    }
    /// The crates read where Cargo keeps them, when any are read.
    fn environment(&self, repo: &Repo, opts: &Options) -> Vec<Check> {
        if opts.libraries == 0 {
            return vec![];
        }
        rust::environment(repo)
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
    fn detect(&self, repo: &Repo, _opts: &Options) -> bool {
        typescript::detect(repo)
    }
    fn parse(&self, repo: &Repo, opts: &Options) -> Result<Graph> {
        let (b, root) = typescript::parse(repo, opts)?;
        Ok(b.finish(root, Some(repo.root()), self.name()))
    }
    /// The compiler works out what a function gives, which a reader of its declaration does not
    /// see (`isolatedDeclarations` asks for it), and holds an override to what its base gives but
    /// not to what it takes: it checks a method's parameters both ways.
    fn leaves(&self) -> Leaves {
        Leaves { returns: true, overrides: true, ..Default::default() }
    }
    /// A value stands where a type is asked for when it has the type's members.
    fn structural(&self) -> bool {
        true
    }
    /// A package reads every file under its code, the packages there with it.
    fn reads(&self, part: &Repo, dir: &Path) -> bool {
        dir.starts_with(typescript::code_base(part))
    }
    /// The packages read from a `node_modules` above the package, when any are read.
    fn environment(&self, repo: &Repo, opts: &Options) -> Vec<Check> {
        if opts.libraries == 0 {
            return vec![];
        }
        typescript::environment(repo)
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

/// Whether the language of that name compares types by their members.
pub fn structural(name: &str) -> bool {
    languages().into_iter().find(|l| l.name() == name).is_some_and(|l| l.structural())
}

/// All known languages, in detection order.
pub fn languages() -> Vec<Box<dyn Language>> {
    vec![Box::new(Rust), Box::new(Python), Box::new(TypeScript)]
}

/// The language a repo is written in; a package directory, given or asked for, means Python.
pub fn detect(repo: &Repo, opts: &Options) -> Option<Box<dyn Language>> {
    if opts.package.is_some() || repo.asked().is_some() {
        return Some(Box::new(Python));
    }
    languages().into_iter().find(|l| l.detect(repo, opts))
}

/// The language a repository is read in, or why there is none: what was looked for, and where.
pub fn of(repo: &Repo, opts: &Options) -> anyhow::Result<Box<dyn Language>> {
    detect(repo, opts).ok_or_else(|| anyhow::anyhow!("no language recognised in {} ({})", repo.root().display(), looked_for()))
}
