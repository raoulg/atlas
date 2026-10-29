//! `parse: Repo -> Graph` for TypeScript and JavaScript apps and packages.
//!
//! Two passes over tree-sitter trees, as for Python: symbols and imports first, then
//! resolution of types, calls, JSX and boundaries into edges. A module is a file under the
//! package (`src/` is a layout folder, not a name), a directory is a package module, and an
//! `index` file stands for its directory. The engine knows the language and its common
//! libraries; it never knows a project.
//!
//! The reader stands in layers, each built on those below it: a file and the vocabulary its
//! declarations are read with (`module.rs`); the values an expression can have (`paths.rs`); a
//! function's body (`body.rs`); the passes that read a package's modules into the graph
//! (`passes.rs`); the libraries' code, read with those passes (`libraries.rs`); and here, on top,
//! the package and the order the passes run in.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use indexmap::IndexMap;

mod body;
mod libraries;
mod module;
mod passes;
mod paths;

use crate::builder::GraphBuilder;
use crate::knowledge;
use crate::model::{Entry, EntryKind, Kind, Node};
use crate::repo::{normal, Repo};
use crate::tree::span;

pub(crate) use module::EXTENSIONS;
use module::{is_code, module_id, segment, ts_parser, Module};
use passes::{
    collect_imports, collect_routes, collect_symbols, collect_wrapped, container_fields, link, mark_bins, mark_exports, route_prefixes,
};

/// Does this look like a JavaScript or TypeScript package: a `package.json` with a
/// `tsconfig.json`, a `jsconfig.json` or a `src/` beside it.
pub fn detect(repo: &Repo) -> bool {
    repo.is_file("package.json") && (repo.is_file("tsconfig.json") || repo.is_file("jsconfig.json") || repo.is_dir("src"))
}

/// The package's name for the graph: the `name` its `package.json` gives it, which code imports it
/// by, as a Python part is the package its manifest names and a Rust part its crate; its directory
/// where the `package.json` names none. A scoped name (`@acme/client`) stands as it is written.
fn root_name(repo: &Repo) -> String {
    let named = repo
        .text("package.json")
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|j| j["name"].as_str().filter(|n| !n.is_empty()).map(str::to_string));
    segment(&named.or_else(|| repo.name()).unwrap_or_else(|| "app".into()))
}

/// Where module paths start: `src/` when there is one.
pub(crate) fn code_base(repo: &Repo) -> PathBuf {
    if repo.is_dir("src") {
        repo.root().join("src")
    } else {
        repo.root().to_path_buf()
    }
}

/// tsconfig `paths`: alias prefix -> directory, `"@/*": ["./src/*"]` -> (`@/`, `<repo>/src/`).
fn path_aliases(repo: &Repo) -> Vec<(String, PathBuf)> {
    let mut out = vec![];
    for cfg in ["tsconfig.json", "jsconfig.json"] {
        let Some(text) = repo.text(cfg) else { continue };
        let Ok(json) = serde_json::from_str::<serde_json::Value>(&jsonc(&text)) else { continue };
        let opts = &json["compilerOptions"];
        let base = repo.root().join(opts["baseUrl"].as_str().unwrap_or("."));
        if let Some(paths) = opts["paths"].as_object() {
            for (alias, targets) in paths {
                let Some(first) = targets.as_array().and_then(|t| t.first()).and_then(|t| t.as_str()) else { continue };
                out.push((alias.trim_end_matches('*').to_string(), base.join(first.trim_end_matches('*'))));
            }
        }
    }
    out
}

/// JSON with comments and trailing commas, as tsconfig allows, made plain JSON. Comments are
/// recognised outside strings only: `"@/*"` is a path, not the start of one.
fn jsonc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            match c {
                '\\' => out.extend(chars.next()),
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match (c, chars.peek()) {
            ('"', _) => {
                in_string = true;
                out.push(c);
            }
            ('/', Some('/')) => while chars.next_if(|&n| n != '\n').is_some() {},
            ('/', Some('*')) => {
                chars.next();
                let mut prev = ' ';
                for n in chars.by_ref() {
                    if prev == '*' && n == '/' {
                        break;
                    }
                    prev = n;
                }
            }
            _ => out.push(c),
        }
    }
    regex::Regex::new(r",(\s*[}\]])").unwrap().replace_all(&out, "$1").to_string()
}

/// What a reading of the package at `repo` takes from outside it: the packages installed in a
/// `node_modules` above it, which Node finds from its files as well.
pub fn environment(repo: &Repo) -> Vec<crate::cache::Check> {
    let root = std::fs::canonicalize(repo.root()).unwrap_or_else(|_| repo.root().to_path_buf());
    root.ancestors().skip(1).map(|a| crate::cache::Check::Tree(a.join(knowledge::NODE_PACKAGES))).collect()
}

pub fn parse(repo: &Repo, opts: &crate::Options) -> Result<(GraphBuilder, String)> {
    let root = root_name(repo);
    let base = code_base(repo);
    let aliases = path_aliases(repo);
    let mut store = GraphBuilder::default();
    // not under what a tool made
    let made = |p: &Path| {
        p.strip_prefix(&base).map_or(false, |r| r.components().any(|c| knowledge::made_dir(c.as_os_str().to_str().unwrap_or(""))))
    };
    let mut paths: Vec<PathBuf> = repo.files_under(&base).into_iter().filter(|p| is_code(p) && !made(p)).map(|p| normal(&p)).collect();
    paths.sort();
    let files: HashMap<PathBuf, String> = paths.iter().map(|p| (p.clone(), module_id(&root, &base, p))).collect();
    // every directory is a package module, so the module quotient follows the folders
    let mut dirs: Vec<String> = vec![root.clone()];
    for p in &paths {
        let mut cur = p.parent();
        while let Some(d) = cur.filter(|d| d.starts_with(&base) && *d != base.as_path()) {
            dirs.push(module_id(&root, &base, &d.join("index.ts")));
            cur = d.parent();
        }
    }
    dirs.sort();
    dirs.dedup();
    for d in &dirs {
        store.add_node(Node {
            id: d.clone(),
            kind: Kind::Module(crate::model::Module { is_pkg: true, ..Default::default() }),
            name: d.rsplit('.').next().unwrap_or(d).to_string(),
            module: d.clone(),
            ..Default::default()
        });
    }
    let mut modules: IndexMap<String, Module> = IndexMap::new();
    let (mut ts, mut tsx) = (ts_parser(false)?, ts_parser(true)?);
    for p in &paths {
        let id = files[p].clone();
        let src = repo.bytes(p).with_context(|| format!("reading {}", p.display()))?.to_vec();
        let jsx = matches!(p.extension().and_then(|e| e.to_str()), Some("tsx" | "jsx" | "js" | "mjs" | "cjs"));
        let tree = (if jsx { &mut tsx } else { &mut ts }).parse(&src, None).with_context(|| format!("parsing {}", p.display()))?;
        store.files.insert(crate::tree::path_str(p), String::from_utf8_lossy(&src).to_string());
        let is_index = p.file_stem().and_then(|s| s.to_str()) == Some("index");
        let rel = crate::tree::path_str(p.strip_prefix(repo.root()).unwrap_or(p));
        // a test file is test code, and a route of its own: its body runs the tests
        let test = test_file(p);
        let entry = test.then(|| Entry::new(EntryKind::Test, rel.clone()));
        let node = Node {
            id: id.clone(),
            kind: Kind::Module(crate::model::Module { is_pkg: is_index, ..Default::default() }),
            name: p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string(),
            file: crate::tree::path_str(p),
            span: span(tree.root_node()),
            module: id.clone(),
            test,
            entry,
            ..Default::default()
        };
        store.add_node(node);
        modules.insert(id.clone(), Module::new(id, p.clone(), src, tree));
    }
    for m in modules.values_mut() {
        collect_imports(m, &aliases, &files);
        collect_wrapped(m);
        collect_symbols(m, &mut store);
        collect_routes(m, &mut store);
    }
    libraries::read(repo, &mut modules, &mut store, opts.libraries);
    for m in modules.values_mut() {
        m.containers = container_fields(m);
    }
    route_prefixes(&modules, &mut store);
    mark_bins(repo, &files, &mut store);
    mark_exports(repo, &files, &modules, &mut store);
    link(&modules, &mut store, repo.root());
    store.mark_library();
    Ok((store, root))
}

fn test_file(p: &Path) -> bool {
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
    name.contains(".test.") || name.contains(".spec.") || p.components().any(|c| c.as_os_str() == "__tests__")
}
