//! `parse: Repo -> Graph` for Rust workspaces.
//!
//! Crates become root modules, files become modules, structs/enums/traits become
//! classes, `impl` blocks attach methods to their type and `impl Trait for` edges,
//! `use` declarations become bindings. Calls resolve through paths, `self`, typed
//! struct fields and typed parameters. A static the functions change is a place (`globals.rs`).
//!
//! The reader stands in layers, each built on those below it: a file and the vocabulary its
//! items are read with (`module.rs`); the values an expression can have and the statics a crate
//! keeps (`paths.rs`, `globals.rs`); a function's body (`body.rs`); the passes that read a
//! crate's modules into the graph (`passes.rs`); the libraries' code, read with those passes
//! (`libraries.rs`); and here, on top, the workspace's crates and the order the passes run in.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use indexmap::IndexMap;

use crate::builder::GraphBuilder;
use crate::model::{Node, NodeKind};
use crate::repo::Repo;
use crate::tree::span;

mod body;
mod globals;
mod libraries;
mod module;
mod passes;
mod paths;

use module::{definitions, module_doc, new_parser, type_parameters, Crate, Module};
use passes::{collect_impls, collect_symbols, collect_uses, link, mark_exports, mark_tests};

pub(crate) fn crates(repo: &Repo) -> Result<Vec<Crate>> {
    let manifest = repo.root().join("Cargo.toml");
    let doc: toml::Table = repo
        .text(&manifest)
        .with_context(|| format!("reading {}", manifest.display()))?
        .parse()
        .with_context(|| format!("parsing {}", manifest.display()))?;
    let mut out = vec![];
    let mut dirs: Vec<PathBuf> = vec![];
    if let Some(members) = doc.get("workspace").and_then(|w| w.get("members")).and_then(|m| m.as_array()) {
        for m in members.iter().filter_map(|m| m.as_str()) {
            match m.strip_suffix("/*") {
                Some(prefix) => dirs.extend(repo.dirs_holding(prefix, "Cargo.toml")),
                None => dirs.push(repo.root().join(m)),
            }
        }
    }
    if doc.get("package").is_some() {
        dirs.push(repo.root().to_path_buf());
    }
    for dir in dirs {
        let Some(t) = repo.text(dir.join("Cargo.toml")) else { continue };
        let Ok(d) = t.parse::<toml::Table>() else { continue };
        let Some(name) = d.get("package").and_then(|p| p.get("name")).and_then(|n| n.as_str()) else { continue };
        out.push(Crate { name: name.to_string(), dir });
    }
    if out.is_empty() {
        anyhow::bail!("no crates found in {}", manifest.display());
    }
    Ok(out)
}

/// A file's module, and the binary it is the root of, by Cargo's targets: `src/lib.rs` -> crate
/// (the library), `src/main.rs` -> crate when there is no library, else crate.main (the binary
/// named after the package), `src/bin/x.rs` and `src/bin/x/main.rs` -> crate.bin.x (the binary
/// x), `src/a/mod.rs` -> crate.a, `src/a/b.rs` -> crate.a.b, `tests/x.rs` -> crate.tests.x.
fn module_id(repo: &Repo, krate: &str, dir: &Path, path: &Path) -> Option<(String, Option<String>)> {
    let rel = path.strip_prefix(dir).ok()?.with_extension("");
    let mut parts: Vec<String> = rel.components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect();
    let mut bin = None;
    if parts.first().map(String::as_str) == Some("src") {
        parts.remove(0);
        if parts.last().map(String::as_str) == Some("mod") {
            parts.pop();
        }
        match parts.iter().map(String::as_str).collect::<Vec<_>>()[..] {
            ["lib"] => parts.clear(),
            ["main"] => {
                bin = Some(krate.to_string());
                if !repo.is_file(dir.join("src/lib.rs")) {
                    parts.clear();
                }
            }
            ["bin", name] | ["bin", name, "main"] => {
                bin = Some(name.to_string());
                parts.truncate(2);
            }
            _ => {}
        }
    }
    let mut id = krate.to_string();
    for p in parts {
        id.push('.');
        id.push_str(&p);
    }
    Some((id, bin))
}

/// What a reading of the workspace at `repo` takes from outside it: the crates its crates depend
/// on, where Cargo keeps them.
pub fn environment(repo: &Repo) -> Vec<crate::cache::Check> {
    crates(repo).map(|cs| libraries::environment(repo, &cs)).unwrap_or_default()
}

/// Parse a Cargo workspace or crate into a `GraphBuilder`, with the crates its calls go into read
/// as far as `opts.libraries` says; returns the store and the workspace name.
pub fn parse(repo: &Repo, opts: &crate::Options) -> Result<(GraphBuilder, String)> {
    let mut parser = new_parser()?;
    let mut modules: IndexMap<String, Module> = IndexMap::new();
    let mut store = GraphBuilder::default();
    let crate_list = crates(repo)?;
    // a single crate is named after itself; a workspace after its directory
    let root_name = if crate_list.len() == 1 { crate_list[0].name.clone() } else { repo.name().unwrap_or_else(|| "workspace".into()) };
    let crate_ids: HashMap<String, String> = crate_list.iter().map(|c| (c.name.replace('-', "_"), c.name.clone())).collect();
    for c in &crate_list {
        // a directory under the crate with a `Cargo.toml` of its own is another package
        let other = |p: &Path| {
            p.ancestors().skip(1).take_while(|a| *a != c.dir.as_path() && a.starts_with(&c.dir)).any(|a| repo.is_file(a.join("Cargo.toml")))
        };
        let mut files: Vec<PathBuf> = repo
            .files_under(&c.dir, &["rs"])
            .into_iter()
            .filter(|p| !other(p))
            .filter(|p| {
                let rel = p.strip_prefix(&c.dir).unwrap_or(p);
                matches!(
                    rel.components().next().map(|x| x.as_os_str().to_string_lossy().to_string()).as_deref(),
                    Some("src") | Some("tests") | Some("benches") | Some("examples")
                )
            })
            .collect();
        files.sort();
        for p in files {
            let _parse = crate::clock::of("parse", &c.name, false);
            let Some((mid, bin)) = module_id(repo, &c.name, &c.dir, &p) else { continue };
            if modules.contains_key(&mid) {
                continue;
            }
            let src = repo.bytes(&p).with_context(|| format!("reading {}", p.display()))?.to_vec();
            let tree = parser.parse(&src, None).with_context(|| format!("parsing {}", p.display()))?;
            let is_root = mid == c.name || bin.is_some();
            let parent = if is_root { mid.clone() } else { mid.rsplit_once('.').map(|x| x.0.to_string()).unwrap_or(mid.clone()) };
            let name = mid.rsplit('.').next().unwrap_or(&mid).to_string();
            store.files.insert(crate::tree::path_str(&p), String::from_utf8_lossy(&src).to_string());
            store.add_node(Node {
                id: mid.clone(),
                kind: NodeKind::Module,
                name,
                file: crate::tree::path_str(&p),
                span: span(tree.root_node()),
                module: mid.clone(),
                doc: module_doc(tree.root_node(), &src),
                is_pkg: is_root,
                // an integration test: Cargo builds what `tests/` holds only to test the crate
                test: p.strip_prefix(&c.dir).is_ok_and(|rel| rel.starts_with("tests")),
                ..Default::default()
            });
            let defs = definitions(&tree, &src);
            let generics = type_parameters(&tree, &src);
            modules.insert(
                mid.clone(),
                Module {
                    id: mid,
                    krate: c.name.clone(),
                    manifest_dir: c
                        .dir
                        .strip_prefix(repo.root())
                        .map(crate::tree::path_str)
                        .ok()
                        .filter(|d| !d.is_empty())
                        .unwrap_or(".".into()),
                    parent,
                    path: p,
                    src,
                    tree,
                    bindings: HashMap::new(),
                    wildcards: vec![],
                    globs: vec![],
                    generics,
                    symbols: HashMap::new(),
                    is_root,
                    bin,
                    defs,
                    library: false,
                },
            );
        }
    }
    for m in modules.values_mut() {
        let _items = crate::clock::of("items", &m.krate, false);
        collect_symbols(m, &mut store);
    }
    let module_ids: HashSet<String> = modules.keys().cloned().collect();
    for m in modules.values_mut() {
        let _items = crate::clock::of("items", &m.krate, false);
        collect_uses(m, &crate_ids, &module_ids, &mut store);
        store.bindings.insert(m.id.clone(), m.bindings.clone());
    }
    {
        let _items = crate::clock::stage("items");
        mark_exports(&modules, &mut store, &crate_ids);
        let own: Vec<String> = modules.keys().cloned().collect();
        collect_impls(&modules, &mut store, &crate_ids, &own);
    }
    {
        let _reach = crate::clock::stage("reach");
        libraries::read(repo, &crate_list, &mut modules, &mut store, &crate_ids, opts.libraries);
    }
    link(&modules, &mut store, &crate_ids);
    let _items = crate::clock::stage("items");
    mark_tests(&modules, &mut store);
    store.mark_library();
    Ok((store, root_name))
}
