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

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use indexmap::IndexMap;

mod body;
mod compiled;
mod libraries;
mod module;
mod passes;
mod paths;

use crate::builder::GraphBuilder;
use crate::knowledge;
use crate::model::{EntryKind, Node, NodeKind};
use crate::repo::Repo;
use crate::tree::span;
use path_clean::clean;

pub(crate) use module::EXTENSIONS;
use module::{is_code, module_id, segment, ts_parser, Module};
use passes::{
    collect_imports, collect_routes, collect_symbols, collect_tests, collect_wrapped, container_fields, link, mark_bins, mark_exports,
    route_prefixes,
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

/// What a reading of the package at `repo` takes from its dependency directories: its own
/// `node_modules` and those above it, which Node finds from its files as well, and the code that
/// a package linked into one (a `file:` dependency) points to, which a reading of it reads from
/// there.
pub fn environment(repo: &Repo) -> Vec<crate::cache::Check> {
    let root = crate::repo::Repo::real(&repo.root());
    let mut out = vec![];
    for dir in root.ancestors().map(|a| a.join(knowledge::NODE_PACKAGES)) {
        out.push(crate::cache::Check::Tree(dir.clone()));
        let Ok(real) = std::fs::canonicalize(&dir) else { continue };
        out.extend(linked(&real, &root).into_iter().map(crate::cache::Check::Tree));
    }
    out
}

/// Where the packages of the `node_modules` directory `dir` that are links point: the directories
/// outside it, and outside the package at `root`, whose own code is read as its own.
fn linked(dir: &Path, root: &Path) -> Vec<PathBuf> {
    let entries = |d: &Path| -> Vec<PathBuf> { std::fs::read_dir(d).into_iter().flatten().flatten().map(|e| e.path()).collect() };
    let mut packages = vec![];
    for entry in entries(dir) {
        match entry.file_name().and_then(|n| n.to_str()) {
            Some(scope) if scope.starts_with('@') => packages.extend(entries(&entry)),
            Some(name) if !name.starts_with('.') => packages.push(entry),
            _ => {}
        }
    }
    packages
        .into_iter()
        .filter_map(|p| std::fs::canonicalize(&p).ok().filter(|real| real.is_dir() && !real.starts_with(dir) && !real.starts_with(root)))
        .collect()
}

/// Where the code of the package `name` lies, imported from the file `from` of the package at
/// `repo`, as Node finds it.
pub fn library_code(repo: &Repo, env: &mut crate::environment::Environment, from: &Path, name: &str) -> Option<PathBuf> {
    libraries::library_code(repo, env, from, name)
}

/// What a reading of the package reads. With a config, the files the compiler reads
/// (`compiled.rs`); without one, every file under the code base. Besides these, the files the
/// manifest runs (`bin`, a file a script runs: `tsx scripts/summarize.ts`) and the tests the runners
/// find, and, closed under imports as the compiler reads them, every file a file read imports. Never
/// what a tool made, nor a package beside the code base, which is a part of its own
/// (`Language::reads`).
struct Reading {
    /// Each file read, with its text and its tree, in path order.
    parsed: BTreeMap<PathBuf, (Vec<u8>, tree_sitter::Tree)>,
    /// The files read as programs: those the config compiles or the manifest runs, but no test and
    /// no file read only because another imports it.
    programs: Vec<PathBuf>,
    /// The files of the package found and not read, by the folder they lie in (`Stats::unread`).
    unread: BTreeMap<String, usize>,
}

impl Reading {
    fn of(
        repo: &Repo,
        name: &str,
        base: &Path,
        aliases: &[(String, PathBuf)],
        ts: &mut tree_sitter::Parser,
        tsx: &mut tree_sitter::Parser,
    ) -> Result<Reading> {
        let top = repo.root();
        let rel = |p: &Path| p.strip_prefix(top).unwrap_or(p).to_path_buf();
        let named = |p: &Path, any: &dyn Fn(&str) -> bool| rel(p).components().any(|c| any(c.as_os_str().to_str().unwrap_or("")));
        // a manifest beside the code base roots a part of its own, as `parts::roots` finds it
        // (asked of the folders that hold code only, so that a reading rests on no other folder)
        let mut own: HashMap<PathBuf, bool> = HashMap::new();
        let mut in_own = |p: &Path| {
            p.ancestors().skip(1).take_while(|d| *d != top && !d.starts_with(base)).any(|d| {
                *own.entry(d.to_path_buf()).or_insert_with(|| {
                    repo.is_file(d.join("package.json"))
                        && !named(d, &|n| n.starts_with('.') || knowledge::made_dir(n) || knowledge::TEST_DIRS.contains(&n))
                })
            })
        };
        let found: Vec<PathBuf> = repo
            .files_under(top, EXTENSIONS)
            .into_iter()
            .filter(|p| is_code(p) && !named(p, &knowledge::made_dir))
            .filter(|p| !in_own(p))
            .map(|p| clean(&p))
            .collect();
        let compiled = compiled::Compiled::of(repo);
        let declared = |p: &Path| match &compiled {
            Some(c) => c.reads(&rel(p)),
            None => p.starts_with(base),
        };
        let run = runs(repo);
        let test = |p: &Path| knowledge::JS_TESTS.test_file(&rel(p));
        let programs: Vec<PathBuf> = found
            .iter()
            .filter(|p| (declared(p) || run.contains(*p)) && !test(p) && !knowledge::JS_TESTS.collects_from(&rel(p)))
            .cloned()
            .collect();
        // the files an import can name, by their place in `found`
        let known: HashMap<PathBuf, String> = found.iter().enumerate().map(|(i, p)| (p.clone(), i.to_string())).collect();
        let mut todo: Vec<PathBuf> = found.iter().filter(|p| declared(p) || run.contains(*p) || test(p)).cloned().collect();
        let mut parsed = BTreeMap::new();
        while let Some(p) = todo.pop() {
            if parsed.contains_key(&p) {
                continue;
            }
            let _parse = crate::clock::of("parse", name, false);
            let src = repo.bytes(&p).with_context(|| format!("reading {}", p.display()))?.to_vec();
            let jsx = matches!(p.extension().and_then(|e| e.to_str()), Some("tsx" | "jsx" | "js" | "mjs" | "cjs"));
            let tree = (if jsx { &mut *tsx } else { &mut *ts }).parse(&src, None).with_context(|| format!("parsing {}", p.display()))?;
            for spec in specifiers(&tree, &src) {
                if let Some(i) = passes::resolve_spec(&spec, &p, aliases, &known).and_then(|i| i.parse::<usize>().ok()) {
                    todo.push(found[i].clone());
                }
            }
            parsed.insert(p, (src, tree));
        }
        let mut unread: BTreeMap<String, usize> = BTreeMap::new();
        for p in found.iter().filter(|p| !parsed.contains_key(*p)) {
            let r = rel(p);
            let folder = match (r.components().next(), r.components().count() > 1) {
                (Some(first), true) => top.join(first),
                _ => top.to_path_buf(),
            };
            *unread.entry(crate::tree::path_str(&folder)).or_default() += 1;
        }
        let programs = programs.into_iter().filter(|p| parsed.contains_key(p)).collect();
        Ok(Reading { parsed, programs, unread })
    }
}

/// The files of the package its manifest runs: what `bin` names, and a file a script's command
/// names (`tsx scripts/summarize.ts`).
fn runs(repo: &Repo) -> HashSet<PathBuf> {
    let json = repo.text("package.json").and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()).unwrap_or_default();
    let mut words: Vec<String> = match &json["bin"] {
        serde_json::Value::String(p) => vec![p.clone()],
        serde_json::Value::Object(o) => o.values().filter_map(|v| v.as_str().map(str::to_string)).collect(),
        _ => vec![],
    };
    for cmd in json["scripts"].as_object().into_iter().flatten().filter_map(|(_, c)| c.as_str()) {
        words.extend(cmd.split_whitespace().map(|w| w.trim_matches(['"', '\'']).to_string()));
    }
    words
        .iter()
        .map(|w| clean(repo.root().join(w)))
        .filter(|p| p.extension().and_then(|e| e.to_str()).is_some_and(|e| EXTENSIONS.contains(&e)) && repo.is_file(p))
        .collect()
}

/// What a module imports, as written: the source of each `import` and `export .. from`, and what
/// `import()` and `require()` are handed.
fn specifiers(tree: &tree_sitter::Tree, src: &[u8]) -> Vec<String> {
    use crate::tree::{named_children, text, unquote, walk};
    let root = tree.root_node();
    let mut out: Vec<String> =
        named_children(root).into_iter().filter_map(|s| s.child_by_field_name("source")).map(|s| unquote(text(s, src))).collect();
    for c in walk(root, &["call_expression"]) {
        let Some(f) = c.child_by_field_name("function") else { continue };
        if f.kind() == "import" || (f.kind() == "identifier" && text(f, src) == "require") {
            let arg = c.child_by_field_name("arguments").and_then(|a| named_children(a).into_iter().find(|a| a.kind() == "string"));
            out.extend(arg.map(|a| unquote(text(a, src))));
        }
    }
    out
}

pub fn parse(repo: &Repo, opts: &crate::Options) -> Result<(GraphBuilder, String)> {
    let root = root_name(repo);
    let base = code_base(repo);
    let aliases = compiled::path_aliases(repo);
    let mut store = GraphBuilder::default();
    let (mut ts, mut tsx) = (ts_parser(false)?, ts_parser(true)?);
    let reading = Reading::of(repo, &root, &base, &aliases, &mut ts, &mut tsx)?;
    store.unread = reading.unread;
    let mut parsed = reading.parsed;
    let paths: Vec<PathBuf> = parsed.keys().cloned().collect();
    // a file beside the code base (`scripts/` beside `src/`) is named from the package's root
    let beside = |p: &Path| !p.starts_with(&base);
    let from = |p: &Path| if beside(p) { repo.root().to_path_buf() } else { base.clone() };
    let files: HashMap<PathBuf, String> = paths.iter().map(|p| (p.clone(), module_id(&root, &from(p), p))).collect();
    // every directory is a package module, so the module quotient follows the folders; a directory the
    // test runners keep for tests (`__tests__`) is test code, as every file below it is
    let mut dirs: Vec<(String, bool)> = vec![(root.clone(), false)];
    for p in &paths {
        let top = from(p);
        let mut cur = p.parent();
        while let Some(d) = cur.filter(|d| d.starts_with(&top) && *d != top.as_path()) {
            let test = d.strip_prefix(repo.root()).is_ok_and(|r| knowledge::JS_TESTS.test_dir(r));
            dirs.push((module_id(&root, &top, &d.join("index.ts")), test));
            cur = d.parent();
        }
    }
    dirs.sort();
    dirs.dedup();
    for (d, test) in &dirs {
        store.add_node(Node {
            id: d.clone(),
            kind: NodeKind::Module,
            name: d.rsplit('.').next().unwrap_or(d).to_string(),
            module: d.clone(),
            is_pkg: true,
            test: *test,
            ..Default::default()
        });
    }
    let mut modules: IndexMap<String, Module> = IndexMap::new();
    for p in &paths {
        let id = files[p].clone();
        let (src, tree) = parsed.remove(p).expect("every path read is parsed");
        store.files.insert(crate::tree::path_str(p), String::from_utf8_lossy(&src).to_string());
        let is_index = p.file_stem().and_then(|s| s.to_str()) == Some("index");
        let node = Node {
            id: id.clone(),
            kind: NodeKind::Module,
            name: p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string(),
            file: crate::tree::path_str(p),
            span: span(tree.root_node()),
            module: id.clone(),
            is_pkg: is_index,
            ..Default::default()
        };
        store.add_node(node);
        modules.insert(id.clone(), Module::new(id, p.clone(), src, tree));
    }
    let items = crate::clock::stage("items");
    for m in modules.values_mut() {
        collect_imports(m, &aliases, &files);
        collect_wrapped(m);
        collect_symbols(m, &mut store);
        collect_routes(m, &mut store);
        let rel = m.path.strip_prefix(repo.root()).unwrap_or(&m.path).to_path_buf();
        collect_tests(m, &mut store, &rel);
    }
    drop(items);
    let reach = crate::clock::stage("reach");
    libraries::read(repo, &opts.reach, &mut modules, &mut store, opts.libraries);
    drop(reach);
    let items = crate::clock::stage("items");
    for m in modules.values_mut() {
        m.containers = container_fields(m);
    }
    route_prefixes(&modules, &mut store);
    mark_bins(repo, &files, &mut store);
    // a file beside the code base that is run as a program, as a Python script beside its package is:
    // entered at its top level, by the path it is run from, where the package's manifest names no command for it
    for p in reading.programs.iter().filter(|p| beside(p)) {
        if let Some(n) = files.get(p).and_then(|id| store.nodes.get_mut(id)).filter(|n| n.entry.is_none()) {
            n.entry = Some(crate::model::Entry::new(EntryKind::Script, crate::tree::path_str(p.strip_prefix(repo.root()).unwrap_or(p))));
        }
    }
    mark_exports(repo, &files, &modules, &mut store);
    drop(items);
    link(&modules, &mut store, repo.root());
    let _items = crate::clock::stage("items");
    store.mark_library();
    Ok((store, root))
}
