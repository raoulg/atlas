//! `parse: Repo -> Graph` for Python packages.
//!
//! Two passes over tree-sitter trees: symbols and imports first, then resolution of
//! bases, signatures and calls into edges. The engine knows the language; it never
//! knows a project.
//!
//! The reader stands in layers, each built on those below it: a file and the vocabulary its
//! definitions are read with (`module.rs`); the values an expression can have (`paths.rs`); a
//! function's body, with what ty says of it (`body.rs`); the passes that read a package's modules
//! into the graph (`passes.rs`); and here, on top, the project's packages and scripts, the
//! libraries' modules the program reaches, and the order the passes run in.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use indexmap::IndexMap;
use tree_sitter::{Node as TsNode, Parser};
use walkdir::WalkDir;

mod body;
mod module;
mod passes;
mod paths;

pub use crate::builder::{foreign_type, ClassInfo, Declared, GraphBuilder, SigInfo};
use crate::knowledge;
use crate::model::{Entry, EntryKind, Node, NodeKind};
use crate::pytypes::{Ty, Types};
use crate::repo::Repo;
use crate::tree::named_children;

use module::{first_docline, is_main_guard, load_module, new_parser, package_of, Module, Own};
use passes::{collect_imports, collect_symbols, link, mark_exports, route_prefixes, star_imports};

/// Whether a module runs as a program when it is run itself: `if __name__ == "__main__":` at its
/// top level.
fn runs_as_main(root: TsNode, src: &[u8]) -> bool {
    named_children(root).into_iter().any(|s| is_main_guard(s, src))
}

/// The scripts and notebooks of a project: every `.py` and `.ipynb` outside its packages, at any
/// depth, and the files named on the command line, even where git ignores them. Not its tests,
/// what git ignores, hidden directories or environments, and not another project inside it (a
/// directory with a `pyproject.toml`: a part of its own).
fn scripts(repo: &Repo, root_dir: &Path, packages: &[PathBuf]) -> Vec<PathBuf> {
    let named: Vec<PathBuf> = repo.named().into_iter().filter(|p| p.starts_with(root_dir)).collect();
    // without a package or a project file, the project is the files named
    if packages.is_empty() && !repo.is_file(root_dir.join("pyproject.toml")) {
        return named;
    }
    // what a tool made, tests, and another project inside the project
    let elsewhere = |d: &Path| {
        let name = d.file_name().and_then(|n| n.to_str()).unwrap_or("");
        knowledge::made_dir(name) || knowledge::TEST_DIRS.contains(&name) || repo.is_file(d.join("pyproject.toml"))
    };
    let packaged: HashSet<PathBuf> = packages.iter().flat_map(|d| repo.files_under(d)).collect();
    let mut out: Vec<PathBuf> = repo
        .files_under(root_dir)
        .into_iter()
        .filter(|p| matches!(p.extension().and_then(|x| x.to_str()), Some("py" | "ipynb")))
        .filter(|p| !packaged.contains(p))
        .filter(|p| !p.ancestors().skip(1).take_while(|a| *a != root_dir).any(elsewhere))
        .filter(|p| !knowledge::python_test_file(p.file_name().and_then(|n| n.to_str()).unwrap_or("")))
        .collect();
    for f in named {
        if !out.contains(&f) {
            out.push(f);
        }
    }
    out.sort();
    out
}

/// How many directories, from a package up, are looked through for its `pyproject.toml`: the
/// package, `src`, the project, and one more for a nested layout.
pub const PROJECT_ABOVE: usize = 4;

/// The nearest ancestor of the package that holds a `pyproject.toml`.
pub fn project_root(repo: &Repo, pkg_dir: &Path) -> Option<PathBuf> {
    let mut cur = Some(pkg_dir);
    for _ in 0..PROJECT_ABOVE {
        let p = cur?;
        if repo.is_file(p.join("pyproject.toml")) {
            return Some(p.to_path_buf());
        }
        cur = p.parent();
    }
    None
}

/// The packages of a project, the one it is named after first: the packages its manifest names
/// (a directory, or a module of one file), then every other directory with an `__init__.py`
/// where the project keeps its packages (the directories its manifest names for them, `src/`,
/// the root), each read as the package it is. Not its tests, what a tool made, or another
/// project inside it. A directory with an `__init__.py` of its own is a package named on its own.
pub fn packages(repo: &Repo) -> Vec<PathBuf> {
    if repo.is_file("__init__.py") {
        return vec![repo.root().to_path_buf()];
    }
    let manifest = repo.text("pyproject.toml").and_then(|t| t.parse::<toml::Table>().ok()).unwrap_or_default();
    let mut out = named_packages(repo, &manifest);
    for base in package_bases(&manifest) {
        for d in repo.dirs_in(&base) {
            let name = d.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let elsewhere = knowledge::made_dir(name) || knowledge::TEST_DIRS.contains(&name) || repo.is_file(d.join("pyproject.toml"));
            // a package inside another is read with it
            if !elsewhere && repo.is_file(d.join("__init__.py")) && !out.iter().any(|p| d.starts_with(p) || p.starts_with(&d)) {
                out.push(d);
            }
        }
    }
    out
}

/// The packages a project's manifest names, in its order, where they are: by the settings of its
/// build backend (knowledge::PACKAGE_SETTINGS), else by the project's name as an import name
/// (`my-pkg` is `my_pkg`). A package named by its import name is found under one of the
/// directories the project keeps its packages in as Python imports it: a directory with an
/// `__init__.py`, else a module of one file (`my_pkg.py`, a package of one module), else a
/// directory without one (a namespace package). A package inside another named one is read with it.
fn named_packages(repo: &Repo, manifest: &toml::Table) -> Vec<PathBuf> {
    let bases = package_bases(manifest);
    let found = |name: &str| {
        let rel = name.replace('.', "/");
        let at = |b: &PathBuf| {
            let (dir, module) = (b.join(&rel), b.join(format!("{rel}.py")));
            if repo.is_file(dir.join("__init__.py")) {
                Some(dir)
            } else if repo.is_file(&module) {
                Some(module)
            } else {
                repo.is_dir(&dir).then_some(dir)
            }
        };
        bases.iter().find_map(at).map(|d| repo.root().join(d))
    };
    let mut out: Vec<PathBuf> = vec![];
    for (path, names) in knowledge::PACKAGE_SETTINGS {
        let Some(v) = setting(manifest, path) else { continue };
        match names {
            knowledge::Names::Dirs => out.extend(strings(v).iter().map(|d| repo.root().join(d))),
            knowledge::Names::Imports => out.extend(strings(v).iter().filter_map(|n| found(n))),
            knowledge::Names::DirsByName => out.extend(
                v.as_table()
                    .into_iter()
                    .flatten()
                    .filter(|(name, _)| !name.is_empty())
                    .filter_map(|(_, d)| d.as_str())
                    .map(|d| repo.root().join(d)),
            ),
            knowledge::Names::Tables { name: by_name, dir: by_dir } => {
                for t in v.as_array().into_iter().flatten().filter_map(|t| t.as_table()) {
                    let Some(name) = t.get(*by_name).and_then(|n| n.as_str()) else { continue };
                    out.push(repo.root().join(t.get(*by_dir).and_then(|d| d.as_str()).unwrap_or("")).join(name));
                }
            }
        }
    }
    out.retain(|d| repo.is_dir(d) || single_module(repo, d));
    if out.is_empty() {
        let names = knowledge::PROJECT_NAMES.iter().filter_map(|p| setting(manifest, p)?.as_str());
        out.extend(names.map(|n| n.replace(['-', '.'], "_")).find_map(|n| found(&n).or_else(|| found(&n.to_lowercase()))));
    }
    let mut packages: Vec<PathBuf> = vec![];
    for d in out {
        if !packages.iter().any(|p| d.starts_with(p)) {
            packages.retain(|p| !p.starts_with(&d));
            packages.push(d);
        }
    }
    packages
}

/// Whether a package is a module of one file: `solo.py`, which a project named `solo` builds.
fn single_module(repo: &Repo, d: &Path) -> bool {
    d.extension().and_then(|x| x.to_str()) == Some("py") && repo.is_file(d)
}

/// The name a package is imported by: its directory's, or its one module's without `.py`.
fn import_name(repo: &Repo, d: &Path) -> Option<String> {
    let full = std::fs::canonicalize(d).unwrap_or_else(|_| d.to_path_buf());
    let name = if single_module(repo, d) { full.file_stem() } else { full.file_name() };
    name.and_then(|n| n.to_str()).map(str::to_string)
}

/// The directories a project keeps its packages in, from its root: those its manifest names
/// (knowledge::PACKAGE_ROOTS), then the usual layouts.
fn package_bases(manifest: &toml::Table) -> Vec<PathBuf> {
    let mut bases: Vec<PathBuf> =
        knowledge::PACKAGE_ROOTS.iter().filter_map(|p| setting(manifest, p)).flat_map(strings).map(PathBuf::from).collect();
    bases.extend(knowledge::PACKAGE_LAYOUTS.iter().map(PathBuf::from));
    let mut seen = HashSet::new();
    bases.retain(|b| seen.insert(b.clone()));
    bases
}

/// The value at a path of keys in a manifest.
fn setting<'t>(manifest: &'t toml::Table, path: &[&str]) -> Option<&'t toml::Value> {
    let (first, rest) = path.split_first()?;
    rest.iter().try_fold(manifest.get(*first)?, |v, k| v.get(*k))
}

/// A setting's strings: one, or a list.
fn strings(v: &toml::Value) -> Vec<String> {
    match v {
        toml::Value::String(s) => vec![s.clone()],
        toml::Value::Array(a) => a.iter().filter_map(|s| s.as_str()).map(str::to_string).collect(),
        _ => vec![],
    }
}

/// The directory ty reads a project from: the project's root, else the directory its first
/// package is in.
pub fn types_root(packages: &[PathBuf], project_root: Option<&Path>) -> PathBuf {
    project_root.or_else(|| packages.first().and_then(|d| d.parent())).unwrap_or(Path::new("")).to_path_buf()
}

/// Parse a project's packages (and the scripts next to them) into a `GraphBuilder`. Returns the
/// store and the name of the first package, the root.
pub fn parse(repo: &Repo, packages: &[PathBuf], project_root: Option<&Path>, opts: &crate::Options) -> Result<(GraphBuilder, String)> {
    // a project without a package is named after its folder
    let named = packages.first().map(PathBuf::as_path).or(project_root).context("no package and no project")?;
    let root = import_name(repo, named).context("package dir has no name")?;
    let mut own = Own(vec![]);
    for name in std::iter::once(root.clone()).chain(packages.iter().filter_map(|d| import_name(repo, d))) {
        if !own.0.contains(&name) {
            own.0.push(name);
        }
    }
    let mut parser = new_parser()?;
    let mut modules: IndexMap<String, Module> = IndexMap::new();
    let mut store = GraphBuilder::default();

    // each package's modules are named from the directory it is in; a package of one module is
    // that module
    let mut files: Vec<(PathBuf, &Path, bool)> = vec![];
    for d in packages {
        let mut found: Vec<PathBuf> =
            repo.files_under(d).into_iter().filter(|p| p.extension().and_then(|x| x.to_str()) == Some("py")).collect();
        found.sort();
        let single = single_module(repo, d);
        files.extend(found.into_iter().map(|p| (p, d.parent().unwrap_or(Path::new("")), single)));
    }
    let types_root = types_root(packages, project_root);
    for (p, at, single) in files {
        let rel = p.strip_prefix(at).unwrap_or(&p).with_extension("");
        let mut parts: Vec<String> = rel.components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect();
        let init = parts.last().map_or(false, |l| l == "__init__");
        if init {
            parts.pop();
        }
        let is_pkg = init || single;
        let mid = parts.join(".");
        // a package found twice, by two names for its directory, is read once
        if modules.contains_key(&mid) {
            continue;
        }
        let m = load_module(&mut parser, &mid, &p, is_pkg, &repo.bytes(&p).with_context(|| format!("reading {}", p.display()))?)?;
        store.files.insert(crate::tree::path_str(&p), String::from_utf8_lossy(&m.src).to_string());
        store.add_node(Node {
            id: mid.clone(),
            kind: NodeKind::Module,
            name: parts.last().cloned().unwrap_or_default(),
            file: crate::tree::path_str(&p),
            span: Some((1, m.tree.root_node().end_position().row as u32 + 1)),
            module: mid.clone(),
            doc: first_docline(Some(m.tree.root_node()), &m.src),
            is_pkg,
            // a module of a package that runs as a command, by `python -m`
            entry: runs_as_main(m.tree.root_node(), &m.src).then(|| Entry::new(EntryKind::Cli, format!("python -m {mid}"))),
            test: knowledge::python_test_file(p.file_name().and_then(|n| n.to_str()).unwrap_or("")),
            ..Default::default()
        });
        modules.insert(mid, m);
    }
    // the project's scripts and notebooks are routes into its packages
    if let Some(root_dir) = project_root {
        for p in scripts(repo, root_dir, packages) {
            let rel = p.strip_prefix(root_dir).unwrap_or(&p).to_path_buf();
            let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("script").to_string();
            // a dot in a file's name is no module boundary
            let dirs: Vec<String> = rel
                .parent()
                .map(|d| d.components().map(|c| c.as_os_str().to_string_lossy().replace('.', "_")).collect())
                .unwrap_or_default();
            let mid = match dirs.is_empty() {
                false => format!("{}.{}", dirs.join("."), stem.replace('.', "_")),
                true => format!("_root.{}", stem.replace('.', "_")),
            };
            let m = load_module(&mut parser, &mid, &p, false, &repo.bytes(&p).with_context(|| format!("reading {}", p.display()))?)?;
            store.files.insert(crate::tree::path_str(&p), String::from_utf8_lossy(&m.src).to_string());
            store.add_node(Node {
                id: mid.clone(),
                kind: NodeKind::Module,
                name: stem,
                file: crate::tree::path_str(&p),
                span: Some((1, m.tree.root_node().end_position().row as u32 + 1)),
                module: mid.clone(),
                doc: first_docline(Some(m.tree.root_node()), &m.src),
                entry: Some(Entry::new(
                    if p.extension().and_then(|x| x.to_str()) == Some("ipynb") { EntryKind::Notebook } else { EntryKind::Script },
                    crate::tree::path_str(&rel),
                )),
                ..Default::default()
            });
            modules.insert(mid, m);
        }
    }

    let types = read_libraries(&mut parser, &mut modules, &mut store, &types_root, opts)?;
    for m in modules.values_mut() {
        collect_symbols(m, &mut store);
    }
    let module_ids: HashSet<String> = modules.keys().cloned().collect();
    for m in modules.values_mut() {
        collect_imports(m, &module_ids, &mut store, &own);
    }
    star_imports(&mut modules);
    for m in modules.values() {
        store.bindings.insert(m.id.clone(), m.bindings.clone());
    }
    link(&modules, &mut store, &own, &types, &types_root);
    mark_exports(&modules, &mut store, &own);
    route_prefixes(&modules, &mut store, &types, &own);
    store.mark_library();
    Ok((store, root))
}

/// The libraries the project's code calls, read as code as far as those calls reach into them,
/// `opts.libraries` rounds deep: the library modules the code's calls go into, then the modules
/// of the same library their own calls go into. What a library calls in a module not read, or
/// in another library, stays a library's call, as the project's own calls into an unread
/// library do. A library is read
/// only where its source can be followed: not the standard library (ty knows it by its stubs),
/// not a package the effect tables describe, and not one with compiled code. Returns ty's
/// answers for every module read.
fn read_libraries(
    parser: &mut Parser,
    modules: &mut IndexMap<String, Module>,
    store: &mut GraphBuilder,
    root_dir: &Path,
    opts: &crate::Options,
) -> Result<Types> {
    let entry = |m: &Module| (crate::tree::path_str(&m.path), m.path.clone());
    let mut files: Vec<(String, PathBuf)> = modules.values().map(entry).collect();
    let Some(ty) = Ty::new(root_dir, &files, opts.cache.as_ref()) else { return Ok(Types::default()) };
    let mut types = ty.read(&files, &files);
    let mut reached = types.libs.clone();
    let used: HashSet<String> = reached.iter().map(|(m, _)| package_of(m)).collect();
    let mut readable = Readable::default();
    let here = std::fs::canonicalize(root_dir).unwrap_or_else(|_| root_dir.to_path_buf());
    let project = files.len();
    let depth = opts.libraries;
    for round in 1..=depth {
        let mut fresh = vec![];
        for (mid, path) in std::mem::take(&mut reached) {
            if modules.contains_key(&mid) || !used.contains(&package_of(&mid)) || !readable.reads(&mid, &path) {
                continue;
            }
            // a library in the project's environment is named from the project, like its own files
            let shown = path.strip_prefix(&here).map(|r| root_dir.join(r)).unwrap_or_else(|_| path.clone());
            let is_pkg = path.file_stem().and_then(|s| s.to_str()) == Some("__init__");
            // a library's code is the environment's, not the repository's
            let Ok(text) = std::fs::read(&path) else { continue };
            let Ok(m) = load_module(parser, &mid, &shown, is_pkg, &text) else { continue };
            store.files.insert(crate::tree::path_str(&shown), String::from_utf8_lossy(&m.src).to_string());
            store.add_node(Node {
                id: mid.clone(),
                kind: NodeKind::Module,
                name: mid.rsplit('.').next().unwrap_or(&mid).to_string(),
                file: crate::tree::path_str(&shown),
                span: Some((1, m.tree.root_node().end_position().row as u32 + 1)),
                module: mid.clone(),
                doc: first_docline(Some(m.tree.root_node()), &m.src),
                is_pkg,
                library: true,
                ..Default::default()
            });
            fresh.push(entry(&m));
            modules.insert(mid, m);
        }
        if fresh.is_empty() {
            break;
        }
        if round < depth {
            reached = ty.libs(&fresh, &fresh);
        }
        files.extend(fresh);
    }
    // read together, a call into a library module read as code goes to its definition
    if files.len() > project {
        types = ty.read(&files, &files);
    }
    Ok(types)
}

/// Which library packages Atlas reads, each decided once by its directory.
#[derive(Default)]
struct Readable {
    compiled: HashMap<PathBuf, bool>,
}

impl Readable {
    /// Whether the module `mid` at `path` is read: Python source, in a package the effect
    /// tables do not describe and that holds no compiled code.
    fn reads(&mut self, mid: &str, path: &Path) -> bool {
        if path.extension().and_then(|x| x.to_str()) != Some("py") {
            return false;
        }
        let parts = mid.split('.').count();
        if crate::effects::described(&package_of(mid), knowledge::Registry::PyPi) {
            return false;
        }
        let is_pkg = path.file_stem().and_then(|s| s.to_str()) == Some("__init__");
        let Some(top) = path.ancestors().nth(parts - 1 + usize::from(is_pkg)).map(Path::to_path_buf) else { return false };
        let compiled = self.compiled.entry(top.clone()).or_insert_with(|| {
            let compiled = knowledge::compiled(knowledge::Registry::PyPi);
            WalkDir::new(&top)
                .into_iter()
                .flatten()
                .any(|e| e.path().extension().and_then(|x| x.to_str()).map_or(false, |x| compiled.contains(&x)))
        });
        !*compiled
    }
}
