//! `parse: Repo -> Graph` for Rust workspaces.
//!
//! Crates become root modules, files become modules, structs/enums/traits become
//! classes, `impl` blocks attach methods to their type and `impl Trait for` edges,
//! `use` declarations become bindings. Calls resolve through paths, `self`, typed
//! struct fields and typed parameters. A static the functions change is a place (`globals.rs`).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use indexmap::IndexMap;
use tree_sitter::{Node as TsNode, Parser, Tree};

use crate::body::{Crosses, Step, Steps, UseSyntax};
use crate::builder::{foreign_type, ClassInfo, GraphBuilder, SigInfo};
use crate::effects::{self, Crossing};
use crate::knowledge;
use crate::knowledge::OwnTypes;
use crate::model::{Bucket, EdgeKind, Entry, EntryKind, Held, Node, NodeKind, Param, Returns, Touch, World};
use crate::repo::Repo;
use crate::tree::{
    beneath, evaluation_order, literals, named_children, nth_of_kind, span, text, unquote, walk, wrapped, ChoiceSyntax, CompareSyntax,
    Part, Wrappers,
};

/// What hands on the value inside it: parentheses, a reference `&x`, `x?` and `x.await`.
const WRAPPERS: Wrappers = &["parenthesized_expression", "reference_expression", "try_expression", "await_expression"];

mod globals;
mod libraries;
mod paths;
use paths::{Arg, At, Eval, Evaluated};

pub(crate) struct Crate {
    name: String,
    pub(crate) dir: PathBuf,
}

struct Module {
    id: String,
    krate: String,
    /// The directory its crate's manifest is in, by its path in the repository (`.` at its top):
    /// what `CARGO_MANIFEST_DIR` holds where the crate is built.
    manifest_dir: String,
    parent: String,
    path: PathBuf,
    src: Vec<u8>,
    tree: Tree,
    bindings: HashMap<String, String>,
    wildcards: Vec<String>,
    /// The libraries' modules a glob import brings every name of: `ext:plotters.prelude` for
    /// `use plotters::prelude::*`.
    globs: Vec<String>,
    /// The names the module's items declare as type parameters (`DB` in `fn draw<DB: Backend>`).
    generics: HashSet<String>,
    symbols: HashMap<String, String>,
    is_root: bool,
    /// For the root of a binary target (`src/main.rs`, `src/bin/x.rs`): the binary's name.
    bin: Option<String>,
    /// The items a node of the graph is defined by, by their first line and name: where they are
    /// in the file.
    defs: HashMap<(u32, String), (usize, usize)>,
    /// Whether the module is a library's, read as far as the program's calls go into it: its
    /// paths are the library's, `ext:` where Atlas does not read them.
    library: bool,
}

/// A trait impl found in a module, resolved once bindings exist.
struct Impl<'t> {
    module: String,
    type_parts: Vec<String>,
    trait_parts: Vec<String>,
    body: Option<TsNode<'t>>,
}

// ---------- crates and modules ----------

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
                Some(prefix) => dirs.extend(repo.dirs_in(prefix).into_iter().filter(|d| repo.is_file(d.join("Cargo.toml")))),
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

fn new_parser() -> Result<Parser> {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter::Language::new(tree_sitter_rust::LANGUAGE)).context("loading the Rust grammar")?;
    Ok(parser)
}

// ---------- tree helpers specific to Rust ----------

/// `a::b::C`, `crate::x`, `Self`, identifiers -> parts.
fn path_parts(n: Option<TsNode>, src: &[u8]) -> Vec<String> {
    let Some(n) = n else { return vec![] };
    match n.kind() {
        "identifier" | "type_identifier" | "crate" | "super" | "self" | "metavariable" => vec![text(n, src).to_string()],
        "scoped_identifier" | "scoped_type_identifier" => {
            let mut p = path_parts(n.child_by_field_name("path"), src);
            if let Some(name) = n.child_by_field_name("name") {
                p.push(text(name, src).to_string());
            }
            p
        }
        "generic_type" | "reference_type" | "dynamic_type" | "abstract_type" => {
            path_parts(n.child_by_field_name("type").or_else(|| named_children(n).into_iter().next()), src)
        }
        "generic_function" => path_parts(n.child_by_field_name("function"), src),
        "bracketed_type" => path_parts(named_children(n).into_iter().next(), src),
        _ => vec![],
    }
}

/// The type names a type mentions as values, as written: each generic's own name, and its
/// arguments only where it holds or stands for them (`Option<Vec<Node>>` -> `[[Option], [Vec],
/// [Node]]`, `impl Iterator<Item = Node>` -> `[[Iterator], [Node]]`), never a generic class's
/// own parameters (`DrawingArea<DB, Shift>` -> `[[DrawingArea]]`). References, slices, arrays,
/// tuples, `impl` and `dyn` go through to what they hold; a `fn(..)` or `Fn(..)` type is a
/// callback and names none.
fn type_names(t: Option<TsNode>, src: &[u8]) -> Vec<Vec<String>> {
    let Some(t) = t else { return vec![] };
    let mut out = vec![];
    collect_types(t, src, &mut out);
    out
}

/// Whether a type changes through a shared reference, or holds one that does: `&RefCell<Log>`,
/// `Rc<RefCell<Log>>`, `&[Cell<u8>]`, `&AtomicUsize`. Its value may change though no `&mut` is
/// written; the type says so as `&mut` does.
fn changes_inside(t: TsNode, src: &[u8]) -> bool {
    match t.kind() {
        "type_identifier" | "scoped_type_identifier" => path_parts(Some(t), src).last().map_or(false, |n| knowledge::rust_interior(n)),
        "generic_type" => {
            let head = path_parts(t.child_by_field_name("type"), src).last().cloned().unwrap_or_default();
            knowledge::rust_interior(&head)
                || (knowledge::rust_container(&head)
                    && t.child_by_field_name("type_arguments")
                        .map_or(false, |a| named_children(a).into_iter().any(|x| changes_inside(x, src))))
        }
        "reference_type" | "array_type" | "tuple_type" => named_children(t).into_iter().any(|x| changes_inside(x, src)),
        _ => false,
    }
}

fn collect_types(n: TsNode, src: &[u8], out: &mut Vec<Vec<String>>) {
    match n.kind() {
        "type_identifier" | "scoped_type_identifier" => {
            let parts = path_parts(Some(n), src);
            if !parts.is_empty() {
                out.push(parts);
            }
        }
        "generic_type" => {
            let parts = path_parts(n.child_by_field_name("type"), src);
            let holds = parts.last().map_or(false, |g| knowledge::rust_container(g));
            if !parts.is_empty() {
                out.push(parts);
            }
            if let Some(args) = n.child_by_field_name("type_arguments").filter(|_| holds) {
                collect_types(args, src, out);
            }
        }
        // `Item = Node` names the value an iterator gives, not a type called `Item`
        "type_binding" => {
            if let Some(t) = n.child_by_field_name("type") {
                collect_types(t, src, out);
            }
        }
        "function_type" => {}
        _ => {
            for c in named_children(n) {
                collect_types(c, src, out);
            }
        }
    }
}

/// Attributes (`#[test]`, `#[derive(..)]`) sit before an item as siblings.
fn attrs_of(item: TsNode, src: &[u8]) -> Vec<String> {
    let mut out = vec![];
    let mut p = item.prev_named_sibling();
    while let Some(a) = p {
        if a.kind() != "attribute_item" {
            break;
        }
        out.push(text(a, src).trim_start_matches("#[").trim_end_matches(']').to_string());
        p = a.prev_named_sibling();
    }
    out
}

/// The first line of the `///` doc comment before an item.
fn doc_of(item: TsNode, src: &[u8]) -> String {
    let mut lines = vec![];
    let mut p = item.prev_named_sibling();
    while let Some(c) = p {
        match c.kind() {
            "line_comment" if text(c, src).starts_with("///") => lines.push(text(c, src).trim_start_matches('/').trim().to_string()),
            "attribute_item" | "line_comment" | "block_comment" => {}
            _ => break,
        }
        p = c.prev_named_sibling();
    }
    lines.reverse();
    lines.into_iter().find(|l| !l.is_empty()).unwrap_or_default()
}

/// Leading `//!` comments of a file.
fn module_doc(root: TsNode, src: &[u8]) -> String {
    for c in named_children(root) {
        match c.kind() {
            "line_comment" => {
                let t = text(c, src);
                if let Some(rest) = t.strip_prefix("//!") {
                    let rest = rest.trim();
                    if !rest.is_empty() {
                        return rest.to_string();
                    }
                } else {
                    return String::new();
                }
            }
            _ => return String::new(),
        }
    }
    String::new()
}

/// Top-level items, with inline `mod x { }` bodies flattened in.
fn items<'t>(root: TsNode<'t>) -> Vec<TsNode<'t>> {
    let mut out = vec![];
    let mut stack: Vec<TsNode<'t>> = named_children(root).into_iter().rev().collect();
    while let Some(n) = stack.pop() {
        if n.kind() == "mod_item" {
            if let Some(body) = n.child_by_field_name("body") {
                for k in named_children(body).into_iter().rev() {
                    stack.push(k);
                }
            }
        } else {
            out.push(n);
        }
    }
    out
}

/// A signature as data: each parameter by name, type and kind, and what it gives back: the value,
/// which for a `Result<T, E>` is `T`, whatever that is; `E` is what it fails with. `self`,
/// `&self`, `&mut self` and `self: Box<Self>` are the value the method is called on; a `&mut`
/// says the body may change what it is handed, and so does a type that changes through a shared
/// reference (`&RefCell<T>`).
fn signature(f: TsNode, src: &[u8]) -> (Vec<Param>, Option<Returns>, SigInfo) {
    let mut params = vec![];
    let mut info = SigInfo::default();
    if let Some(pn) = f.child_by_field_name("parameters") {
        for p in named_children(pn) {
            let mutable = |t: TsNode| t.kind() == "reference_type" && named_children(t).iter().any(|c| c.kind() == "mutable_specifier");
            match p.kind() {
                "self_parameter" => {
                    params.push(Param { name: "self".into(), receiver: true, changes: text(p, src).contains("mut"), ..Default::default() })
                }
                "parameter" => {
                    let pattern = p.child_by_field_name("pattern");
                    let ty = p.child_by_field_name("type");
                    params.push(Param {
                        name: pattern.map(|x| text(x, src).trim_start_matches("mut ").to_string()).unwrap_or_default(),
                        ty: ty.map(|t| text(t, src).to_string()).unwrap_or_default(),
                        receiver: pattern.map_or(false, |x| x.kind() == "self"),
                        changes: ty.map_or(false, |t| mutable(t) || changes_inside(t, src)),
                        ..Default::default()
                    });
                    info.ptypes.extend(type_names(ty, src));
                }
                _ => {}
            }
        }
    }
    let rt = f.child_by_field_name("return_type");
    info.rtypes = type_names(rt.map(|r| value_of(r, src).0), src);
    (params, rt.map(|r| returns(r, src)), info)
}

/// The macros that end a body by panicking.
const PANICS: &[&str] = &["panic", "assert", "assert_eq", "assert_ne", "unreachable", "todo", "unimplemented"];

/// A `panic!` and the macros that panic when what they check does not hold.
fn panics(n: TsNode, src: &[u8]) -> bool {
    n.kind() == "macro_invocation" && n.child_by_field_name("macro").map_or(false, |m| PANICS.contains(&text(m, src)))
}

/// The value a return type gives, and whether it may fail instead: a `Result<T, E>` gives `T`
/// when it does not fail, a reference what it refers to.
fn value_of<'t>(rt: TsNode<'t>, src: &[u8]) -> (TsNode<'t>, bool) {
    let mut t = rt;
    let mut fails = false;
    loop {
        let inner = match t.kind() {
            "reference_type" => t.child_by_field_name("type"),
            "generic_type" if t.child_by_field_name("type").map_or(false, |n| text(n, src).rsplit("::").next() == Some("Result")) => {
                fails = true;
                t.child_by_field_name("type_arguments").and_then(|a| named_children(a).into_iter().next())
            }
            _ => None,
        };
        match inner {
            Some(i) => t = i,
            None => return (t, fails),
        }
    }
}

/// What a return type gives: its value (`T` of a `Result<T, E>`), `()` no value, `Self` a value
/// of the type the `impl` is for.
fn returns(rt: TsNode, src: &[u8]) -> Returns {
    let (t, fails) = value_of(rt, src);
    let (value, own) = match (t.kind(), text(t, src)) {
        ("unit_type", _) => (String::new(), false),
        (_, "Self") => (String::new(), true),
        (_, v) => (v.to_string(), false),
    };
    Returns { written: text(rt, src).to_string(), value, own, fails, ..Default::default() }
}

/// A method whose first parameter is the value it is called on.
fn takes_self(params: &[Param]) -> bool {
    params.first().map_or(false, |p| p.receiver)
}

/// A function on a type that gives a value of it: `new`, `default`, `from_yaml`.
fn constructs(params: &[Param], returns: &Option<Returns>, ty: &str) -> bool {
    !takes_self(params) && returns.as_ref().map_or(false, |r| r.own || r.value == ty)
}

/// Visible outside its module: `pub`, `pub(crate)`, `pub(super)`.
fn visible(item: TsNode) -> bool {
    named_children(item).iter().any(|c| c.kind() == "visibility_modifier")
}

fn fn_name(f: TsNode, src: &[u8]) -> String {
    f.child_by_field_name("name").map(|n| text(n, src)).unwrap_or("").to_string()
}

// ---------- loading ----------

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
            .files_under(&c.dir)
            .into_iter()
            .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("rs") && !other(p))
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
        collect_symbols(m, &mut store);
    }
    let module_ids: HashSet<String> = modules.keys().cloned().collect();
    for m in modules.values_mut() {
        collect_uses(m, &crate_ids, &module_ids, &mut store);
        store.bindings.insert(m.id.clone(), m.bindings.clone());
    }
    mark_exports(&modules, &mut store, &crate_ids);
    let own: Vec<String> = modules.keys().cloned().collect();
    collect_impls(&modules, &mut store, &crate_ids, &own);
    libraries::read(repo, &crate_list, &mut modules, &mut store, &crate_ids, opts.libraries);
    link(&modules, &mut store, &crate_ids);
    mark_tests(&modules, &mut store);
    store.mark_library();
    Ok((store, root_name))
}

/// Whether an attribute compiles what it stands on only to test the crate: `cfg(test)`, or a `cfg`
/// that asks for `test` among all its conditions and negates none (`cfg(all(test, unix))`).
fn cfg_test(attr: &str) -> bool {
    let Some(cond) = attr.strip_prefix("cfg(") else { return false };
    !cond.contains("not(") && !cond.contains("any(") && cond.split(|c: char| !(c.is_alphanumeric() || c == '_')).any(|w| w == "test")
}

/// Whether a node stands under `#[cfg(test)]`: it, or an item around it, is compiled only to test the crate.
fn under_cfg_test(n: TsNode, src: &[u8]) -> bool {
    std::iter::successors(Some(n), |n| n.parent()).any(|a| attrs_of(a, src).iter().any(|x| cfg_test(x)))
}

/// Test code by where it stands: an item under `#[cfg(test)]`, which an inline `mod tests { .. }`
/// gives all it holds (the graph keeps them in the module around it), and a file module declared
/// under one (`#[cfg(test)] mod tests;`), with every module below it.
fn mark_tests(modules: &IndexMap<String, Module>, store: &mut GraphBuilder) {
    // module -> the lines of its items that are compiled only for tests
    let mut lines: HashMap<&str, Vec<(u32, u32)>> = HashMap::new();
    // the files and directories of the test modules declared in a file of their own
    let mut declared: Vec<PathBuf> = vec![];
    for m in modules.values().filter(|m| !m.library) {
        let mut todo = named_children(m.tree.root_node());
        while let Some(item) = todo.pop() {
            let tested = attrs_of(item, &m.src).iter().any(|a| cfg_test(a));
            match (item.kind(), item.child_by_field_name("body")) {
                ("mod_item", Some(body)) if !tested => todo.extend(named_children(body)),
                ("mod_item", None) if tested => {
                    let Some(name) = item.child_by_field_name("name").map(|n| text(n, &m.src)) else { continue };
                    // a crate's root and a `mod.rs` keep their modules beside them, any other file in a directory of its name
                    let stem = m.path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                    let base = if m.is_root || matches!(stem, "lib" | "main" | "mod") {
                        m.path.parent().map(Path::to_path_buf).unwrap_or_default()
                    } else {
                        m.path.with_extension("")
                    };
                    declared.push(base.join(format!("{name}.rs")));
                    declared.push(base.join(name));
                }
                _ if tested => lines.entry(m.id.as_str()).or_default().extend(span(item)),
                _ => {}
            }
        }
    }
    for m in modules.values().filter(|m| declared.iter().any(|d| m.path == *d || m.path.starts_with(d))) {
        if let Some(n) = store.nodes.get_mut(&m.id) {
            n.test = true;
        }
    }
    for n in store.nodes.values_mut().filter(|n| n.kind != NodeKind::Module) {
        let within = |(first, last): &(u32, u32)| n.span.is_some_and(|(at, _)| *first <= at && at <= *last);
        if lines.get(n.module.as_str()).is_some_and(|ls| ls.iter().any(within)) {
            n.test = true;
        }
    }
}

// ---------- pass 1: symbols ----------

/// A `pub` item at the root of a library target is a route into it; a binary exports nothing.
fn exported(m: &Module, item: tree_sitter::Node, src: &[u8]) -> bool {
    m.is_root && m.bin.is_none() && is_pub(item, src)
}

/// Visible outside the crate: `pub`, not `pub(crate)`.
fn is_pub(item: TsNode, src: &[u8]) -> bool {
    named_children(item).iter().any(|c| c.kind() == "visibility_modifier" && text(*c, src) == "pub")
}

/// What a library exports besides its root's own `pub` items: the names its public modules
/// re-export (`pub use`) and the `pub` items of the modules it makes public (`pub mod`), each
/// labelled by its path from the root. A name the root exports keeps the root's label.
fn mark_exports(modules: &IndexMap<String, Module>, store: &mut GraphBuilder, crate_ids: &HashMap<String, String>) {
    let mut public: Vec<&Module> = modules.values().filter(|m| m.is_root && m.bin.is_none()).collect();
    let mut i = 0;
    while i < public.len() {
        let m = public[i];
        for item in named_children(m.tree.root_node()).into_iter().filter(|x| x.kind() == "mod_item" && is_pub(*x, &m.src)) {
            let Some(name) = item.child_by_field_name("name") else { continue };
            if let Some(c) = modules.get(&format!("{}.{}", m.id, text(name, &m.src))) {
                if !public.iter().any(|p| p.id == c.id) {
                    public.push(c);
                }
            }
        }
        i += 1;
    }
    let pub_items = |m: &Module| -> Vec<(String, String)> {
        items(m.tree.root_node())
            .into_iter()
            .filter(|x| is_pub(*x, &m.src))
            .filter_map(|x| x.child_by_field_name("name").map(|n| text(n, &m.src).to_string()))
            .filter_map(|n| m.symbols.get(&n).map(|id| (n, id.clone())))
            .collect()
    };
    for m in &public {
        let path = m.id.strip_prefix(&m.krate).unwrap_or(&m.id).trim_start_matches('.').replace('.', "::");
        let label = |name: &str| if path.is_empty() { name.to_string() } else { format!("{path}::{name}") };
        let mut names: Vec<(String, String)> = vec![];
        for u in named_children(m.tree.root_node()).into_iter().filter(|x| x.kind() == "use_declaration" && is_pub(*x, &m.src)) {
            let (mut out, mut wild) = (vec![], vec![]);
            collect_use(u, &[], &m.src, &mut out, &mut wild);
            for (alias, parts) in out {
                if let Some(r) = resolve(m, &parts, modules, &store.nodes, crate_ids) {
                    names.push((alias, r));
                }
            }
            for w in wild {
                if let Some(t) = resolve(m, &w, modules, &store.nodes, crate_ids).and_then(|r| modules.get(&r)) {
                    names.extend(pub_items(t));
                }
            }
        }
        names.extend(pub_items(m));
        for (name, id) in names {
            store.export(&id, label(&name));
        }
    }
}

/// The route a function is: a test, the `main` of a binary, or what the crate exports.
fn entry_of(m: &Module, item: TsNode, src: &[u8], name: &str, attrs: &[String]) -> Option<Entry> {
    if attrs.iter().any(|a| a == "test") {
        Some(Entry::new(EntryKind::Test, name.to_string()))
    } else if let Some(bin) = m.bin.as_ref().filter(|_| name == "main") {
        Some(Entry::new(EntryKind::Cli, bin.clone()))
    } else if exported(m, item, src) {
        Some(Entry::new(EntryKind::Export, name.to_string()))
    } else {
        None
    }
}

fn collect_symbols(m: &mut Module, store: &mut GraphBuilder) {
    let src = &m.src;
    let file = crate::tree::path_str(&m.path);
    for item in items(m.tree.root_node()) {
        match item.kind() {
            "function_item" => {
                let name = fn_name(item, src);
                let nid = format!("{}.{}", m.id, name);
                m.symbols.insert(name.clone(), nid.clone());
                let (params, returns, sig) = signature(item, src);
                store.sigs.insert(nid.clone(), sig);
                let attrs = attrs_of(item, src);
                let entry = entry_of(m, item, src, &name, &attrs);
                store.add_node(Node {
                    id: nid.clone(),
                    kind: NodeKind::Function,
                    private: !visible(item),
                    name,
                    file: file.clone(),
                    span: span(item),
                    module: m.id.clone(),
                    doc: doc_of(item, src),
                    decorators: attrs,
                    params,
                    returns,
                    entry,
                    ..Default::default()
                });
                store.add_edge(&m.id, &nid, EdgeKind::Contains, None);
            }
            "struct_item" | "enum_item" | "union_item" | "trait_item" => {
                let name = item.child_by_field_name("name").map(|n| text(n, src)).unwrap_or("").to_string();
                let nid = format!("{}.{}", m.id, name);
                m.symbols.insert(name.clone(), nid.clone());
                let mut fields: Vec<(String, Vec<Vec<String>>)> = vec![];
                let mut variants: Vec<Vec<String>> = vec![];
                let mut arms = vec![];
                let mut methods = vec![];
                if let Some(body) = item.child_by_field_name("body") {
                    if item.kind() == "trait_item" {
                        for f in
                            named_children(body).into_iter().filter(|f| matches!(f.kind(), "function_signature_item" | "function_item"))
                        {
                            let mname = fn_name(f, src);
                            let mnid = format!("{nid}.{mname}");
                            let (params, returns, sig) = signature(f, src);
                            store.sigs.insert(mnid.clone(), sig);
                            store.add_node(Node {
                                id: mnid.clone(),
                                kind: NodeKind::Method,
                                name: mname.clone(),
                                file: file.clone(),
                                span: span(f),
                                module: m.id.clone(),
                                doc: doc_of(f, src),
                                cls: Some(nid.clone()),
                                decorators: attrs_of(f, src),
                                on_class: !takes_self(&params),
                                ctor: constructs(&params, &returns, &name),
                                // a trait's method without a body of its own is what its implementations fill in
                                declared: f.kind() == "function_signature_item",
                                params,
                                returns,
                                ..Default::default()
                            });
                            methods.push(mname);
                            store.add_edge(&nid, &mnid, EdgeKind::Contains, None);
                        }
                    } else {
                        for f in walk(body, &["field_declaration"]) {
                            let fname = f.child_by_field_name("name").map(|n| text(n, src)).unwrap_or("").to_string();
                            fields.push((fname, type_names(f.child_by_field_name("type"), src)));
                        }
                        // an enum is one of its variants, a sum of the types they carry
                        for v in walk(body, &["enum_variant"]) {
                            let Some(carried) = v.child_by_field_name("body") else { continue };
                            let vname = v.child_by_field_name("name").map(|n| text(n, src)).unwrap_or("").to_string();
                            variants.extend(type_names(Some(carried), src));
                            arms.push((vname, type_names(Some(carried), src), span(v)));
                        }
                    }
                }
                store.classes.insert(nid.clone(), ClassInfo { fields, variants, arms, ..Default::default() });
                let entry = exported(m, item, src).then(|| Entry::new(EntryKind::Export, name.clone()));
                store.add_node(Node {
                    id: nid.clone(),
                    kind: NodeKind::Class,
                    private: !visible(item),
                    name,
                    file: file.clone(),
                    span: span(item),
                    module: m.id.clone(),
                    doc: doc_of(item, src),
                    decorators: attrs_of(item, src),
                    methods,
                    entry,
                    ..Default::default()
                });
                store.add_edge(&m.id, &nid, EdgeKind::Contains, None);
            }
            "const_item" | "static_item" | "type_item" => {
                if let Some(n) = item.child_by_field_name("name") {
                    m.symbols.insert(text(n, src).to_string(), format!("{}.{}", m.id, text(n, src)));
                }
            }
            _ => {}
        }
    }
}

// ---------- pass 1b: use declarations ----------

fn collect_use(n: TsNode, prefix: &[String], src: &[u8], out: &mut Vec<(String, Vec<String>)>, wild: &mut Vec<Vec<String>>) {
    match n.kind() {
        "use_declaration" => {
            if let Some(a) = n.child_by_field_name("argument") {
                collect_use(a, prefix, src, out, wild);
            }
        }
        "scoped_identifier" => {
            let mut parts = prefix.to_vec();
            parts.extend(path_parts(Some(n), src));
            let alias = parts.last().cloned().unwrap_or_default();
            out.push((alias, parts));
        }
        "identifier" | "crate" | "super" => {
            let mut parts = prefix.to_vec();
            let t = text(n, src).to_string();
            parts.push(t.clone());
            out.push((t, parts));
        }
        "self" => {
            if let Some(last) = prefix.last() {
                out.push((last.clone(), prefix.to_vec()));
            }
        }
        "scoped_use_list" => {
            let mut path = prefix.to_vec();
            path.extend(path_parts(n.child_by_field_name("path"), src));
            if let Some(list) = n.child_by_field_name("list") {
                for c in named_children(list) {
                    collect_use(c, &path, src, out, wild);
                }
            }
        }
        "use_list" => {
            for c in named_children(n) {
                collect_use(c, prefix, src, out, wild);
            }
        }
        "use_as_clause" => {
            let mut parts = prefix.to_vec();
            parts.extend(path_parts(n.child_by_field_name("path"), src));
            let alias = n.child_by_field_name("alias").map(|a| text(a, src)).unwrap_or("").to_string();
            out.push((alias, parts));
        }
        "use_wildcard" => {
            let mut parts = prefix.to_vec();
            parts.extend(path_parts(named_children(n).into_iter().next(), src));
            wild.push(parts);
        }
        _ => {}
    }
}

/// A path as written -> an internal id or `ext:...`.
fn path_to_id(m: &Module, parts: &[String], crate_ids: &HashMap<String, String>, is_module: &dyn Fn(&str) -> bool) -> String {
    let Some(head) = parts.first() else { return String::new() };
    let (base, rest): (String, &[String]) = match head.as_str() {
        "crate" => (m.krate.clone(), &parts[1..]),
        "self" => (m.id.clone(), &parts[1..]),
        "super" => {
            let mut base = m.parent.clone();
            let mut i = 1;
            while parts.get(i).map(String::as_str) == Some("super") {
                base = base.rsplit_once('.').map(|x| x.0.to_string()).unwrap_or(base);
                i += 1;
            }
            (base, &parts[i..])
        }
        h if crate_ids.contains_key(h) => (crate_ids[h].clone(), &parts[1..]),
        h if is_module(&format!("{}.{h}", m.id)) => (format!("{}.{h}", m.id), &parts[1..]),
        _ => return format!("ext:{}", parts.join(".")),
    };
    let mut id = base;
    for p in rest {
        id.push('.');
        id.push_str(p);
    }
    // a library's path is its own, found where Atlas reads it
    if m.library {
        id.insert_str(0, "ext:");
    }
    id
}

fn collect_uses(m: &mut Module, crate_ids: &HashMap<String, String>, module_ids: &HashSet<String>, store: &mut GraphBuilder) {
    let mut out = vec![];
    let mut wild = vec![];
    // for each name and glob: whether test code imports it, which the module around it does not
    let (mut tested, mut wild_tested) = (vec![], vec![]);
    for u in walk(m.tree.root_node(), &["use_declaration"]) {
        collect_use(u, &[], &m.src, &mut out, &mut wild);
        let test = under_cfg_test(u, &m.src);
        tested.resize(out.len(), test);
        wild_tested.resize(wild.len(), test);
    }
    let mut bindings = vec![];
    let mut edges = vec![];
    let is_module = |id: &str| module_ids.contains(id);
    for ((alias, parts), test) in out.into_iter().zip(tested) {
        let id = path_to_id(m, &parts, crate_ids, &is_module);
        if !id.starts_with("ext:") && !test {
            edges.push(id.clone());
        }
        bindings.push((alias, id));
    }
    for (w, test) in wild.into_iter().zip(wild_tested) {
        let id = path_to_id(m, &w, crate_ids, &is_module);
        // a glob over a name the module binds (`use Kind::*` of its own enum) is no library's
        let bound = w.first().map_or(false, |h| m.symbols.contains_key(h) || bindings.iter().any(|(a, _)| a == h));
        if id.starts_with("ext:") {
            if !bound && !m.globs.contains(&id) {
                m.globs.push(id);
            }
        } else {
            if !test {
                edges.push(id.clone());
            }
            m.wildcards.push(id);
        }
    }
    for (k, v) in bindings {
        m.bindings.insert(k, v);
    }
    for target in edges {
        if module_ids.contains(&target) || store.nodes.contains_key(&target) {
            store.add_edge(&m.id, &target, EdgeKind::Imports, None);
        } else if let Some((modid, _)) = target.rsplit_once('.') {
            if module_ids.contains(modid) {
                store.add_edge(&m.id, modid, EdgeKind::Imports, None);
            }
        }
    }
}

// ---------- resolution ----------

/// The names a file's items declare as type parameters: a function's, an impl's, a trait's, a
/// type's.
fn type_parameters(tree: &Tree, src: &[u8]) -> HashSet<String> {
    walk(tree.root_node(), &["type_parameters"])
        .into_iter()
        .flat_map(named_children)
        .filter_map(|t| match t.kind() {
            "type_identifier" => Some(t),
            "type_parameter" | "constrained_type_parameter" | "optional_type_parameter" => {
                t.child_by_field_name("name").or_else(|| t.child_by_field_name("left"))
            }
            _ => None,
        })
        .map(|n| text(n, src).to_string())
        .collect()
}

/// A path as written -> what it names: one of the program's items, or `ext:..` a library's. Rust
/// keeps types and modules apart from values, and the head of a longer path (`download::f`) is a
/// module or a type, never a function bound by the same name (`pub use download::download`).
/// A name the module neither declares nor imports by name is the prelude's, or one a library's
/// glob import brings in (`DrawingArea` after `use plotters::prelude::*`), when the module has
/// one such glob.
fn resolve(
    m: &Module,
    parts: &[String],
    modules: &IndexMap<String, Module>,
    nodes: &IndexMap<String, Node>,
    crate_ids: &HashMap<String, String>,
) -> Option<String> {
    let head = parts.first()?;
    let is_module = |id: &str| nodes.get(id).map_or(false, |n| n.kind == NodeKind::Module);
    let value = |id: &&String| parts.len() > 1 && nodes.get(*id).map_or(false, |n| matches!(n.kind, NodeKind::Function | NodeKind::Method));
    let (mut target, rest): (String, &[String]) = if matches!(head.as_str(), "crate" | "self" | "super") || crate_ids.contains_key(head) {
        (path_to_id(m, parts, crate_ids, &is_module), &[])
    } else if let Some(s) = m.symbols.get(head).filter(|s| !value(s)) {
        (s.clone(), &parts[1..])
    } else if let Some(b) = m.bindings.get(head).filter(|b| !value(b)) {
        (b.clone(), &parts[1..])
    } else if let Some(w) = m.wildcards.iter().find(|w| nodes.contains_key(&format!("{w}.{head}"))) {
        (format!("{w}.{head}"), &parts[1..])
    } else if nodes.get(&format!("{}.{head}", m.id)).map_or(false, |n| n.kind == NodeKind::Module) {
        // a child module declared with `mod x;`, addressed as `x::f`
        (format!("{}.{head}", m.id), &parts[1..])
    } else if let Some(glob) = glob_of(m, parts) {
        (format!("{glob}.{head}"), &[])
    } else {
        (format!("ext:{}", parts.join(".")), &[])
    };
    for p in rest {
        target.push('.');
        target.push_str(p);
    }
    // a library's item where Atlas read its code
    if let Some(lib) = target.strip_prefix("ext:") {
        return Some(library_item(lib, modules, nodes, 0).unwrap_or(target));
    }
    if nodes.contains_key(&target) {
        return Some(target);
    }
    // a re-export through another module's `pub use`
    let (modid, nm) = target.rsplit_once('.')?;
    let re = modules.get(modid)?.bindings.get(nm)?;
    nodes.contains_key(re).then(|| re.clone())
}

/// The item of a library's code Atlas read that a library's path names (`serde_json.from_str`):
/// followed from the crate's root through the modules the path names and the names they re-export
/// or bring in by a glob. None where Atlas did not read it.
fn library_item(path: &str, modules: &IndexMap<String, Module>, nodes: &IndexMap<String, Node>, depth: u32) -> Option<String> {
    let parts: Vec<&str> = path.split('.').collect();
    let mut cur = modules.get(parts[0]).filter(|m| m.library)?;
    for (i, seg) in parts.iter().enumerate().skip(1) {
        let below = format!("{}.{seg}", cur.id);
        if let Some(child) = modules.get(&below) {
            cur = child;
            continue;
        }
        let rest: String = parts[i + 1..].iter().map(|p| format!(".{p}")).collect();
        if nodes.contains_key(&below) {
            let full = format!("{below}{rest}");
            return nodes.contains_key(&full).then_some(full);
        }
        if depth >= 8 {
            return None;
        }
        if let Some(b) = cur.bindings.get(*seg) {
            return library_item(&format!("{}{rest}", b.strip_prefix("ext:").unwrap_or(b)), modules, nodes, depth + 1);
        }
        // a glob brings in the names of a module read
        return cur
            .globs
            .iter()
            .filter_map(|g| library_module(g.strip_prefix("ext:").unwrap_or(g), modules))
            .find_map(|g| library_item(&format!("{}.{seg}{rest}", g.id), modules, nodes, depth + 1));
    }
    Some(cur.id.clone())
}

/// The module of a library's code Atlas read that a path names through the modules alone.
fn library_module<'m>(path: &str, modules: &'m IndexMap<String, Module>) -> Option<&'m Module> {
    let mut parts = path.split('.');
    let mut cur = modules.get(parts.next()?).filter(|m| m.library)?;
    for seg in parts {
        cur = modules.get(&format!("{}.{seg}", cur.id))?;
    }
    Some(cur)
}

/// The library glob a name is brought in by: a name of one segment the module neither declares
/// nor imports by name, which is not the prelude's nor a type parameter, when the module has one
/// library glob; with two the code does not say which.
fn glob_of<'m>(m: &'m Module, parts: &[String]) -> Option<&'m String> {
    let ([name], [glob]) = (parts, m.globs.as_slice()) else { return None };
    (name != "Self" && !knowledge::RUST_PRELUDE.contains(&name.as_str()) && !m.generics.contains(name)).then_some(glob)
}

/// What a path names where a value is wanted, as where it is called: where it names a module,
/// the function of that name the module's parent declares or re-exports
/// (`mod download; pub use download::download;`), since Rust keeps modules apart from values.
fn resolve_value(
    m: &Module,
    parts: &[String],
    modules: &IndexMap<String, Module>,
    nodes: &IndexMap<String, Node>,
    crate_ids: &HashMap<String, String>,
) -> Option<String> {
    let r = resolve(m, parts, modules, nodes, crate_ids)?;
    if nodes.get(&r).map_or(true, |n| n.kind != NodeKind::Module) {
        return Some(r);
    }
    let value = r.rsplit_once('.').and_then(|(parent, name)| {
        let home = modules.get(parent)?;
        resolve(home, &[name.to_string()], modules, nodes, crate_ids).filter(|v| nodes.get(v).map_or(false, |n| n.kind != NodeKind::Module))
    });
    Some(value.unwrap_or(r))
}

// ---------- pass 1c: impl blocks ----------

/// The `impl` blocks of the modules `ids`: methods on their types, and the traits they implement.
fn collect_impls(modules: &IndexMap<String, Module>, store: &mut GraphBuilder, crate_ids: &HashMap<String, String>, ids: &[String]) {
    let mut impls: Vec<Impl> = vec![];
    for m in ids.iter().filter_map(|id| modules.get(id)) {
        for item in items(m.tree.root_node()).into_iter().filter(|i| i.kind() == "impl_item") {
            impls.push(Impl {
                module: m.id.clone(),
                type_parts: path_parts(item.child_by_field_name("type"), &m.src),
                trait_parts: path_parts(item.child_by_field_name("trait"), &m.src),
                body: item.child_by_field_name("body"),
            });
        }
    }
    for im in impls {
        let m = &modules[&im.module];
        let src = &m.src;
        let Some(cls) = resolve(m, &im.type_parts, modules, &store.nodes, crate_ids).filter(|c| !c.starts_with("ext:")) else { continue };
        if !im.trait_parts.is_empty() {
            let tr = resolve(m, &im.trait_parts, modules, &store.nodes, crate_ids)
                .unwrap_or_else(|| format!("ext:{}", im.trait_parts.join(".")));
            if tr.starts_with("ext:") {
                store.ext_node(&tr);
            }
            store.add_edge(&cls, &tr, EdgeKind::Inherits, None);
            store.classes.entry(cls.clone()).or_default().resolved.push(tr);
        }
        let Some(body) = im.body else { continue };
        let file = crate::tree::path_str(&m.path);
        for f in named_children(body).into_iter().filter(|f| f.kind() == "function_item") {
            let mname = fn_name(f, src);
            let mnid = format!("{cls}.{mname}");
            if store.nodes.contains_key(&mnid) {
                continue;
            }
            let (params, returns, sig) = signature(f, src);
            store.sigs.insert(mnid.clone(), sig);
            let attrs = attrs_of(f, src);
            store.add_node(Node {
                id: mnid.clone(),
                kind: NodeKind::Method,
                name: mname.clone(),
                file: file.clone(),
                span: span(f),
                module: m.id.clone(),
                doc: doc_of(f, src),
                cls: Some(cls.clone()),
                entry: attrs.iter().any(|a| a == "test").then(|| Entry::new(EntryKind::Test, mname.clone())),
                decorators: attrs,
                on_class: !takes_self(&params),
                ctor: constructs(&params, &returns, cls.rsplit('.').next().unwrap_or(&cls)),
                private: im.trait_parts.is_empty() && !visible(f),
                params,
                returns,
                ..Default::default()
            });
            if let Some(c) = store.nodes.get_mut(&cls) {
                c.methods.push(mname);
            }
            store.add_edge(&cls, &mnid, EdgeKind::Contains, None);
        }
    }
}

// ---------- pass 2: fields, signatures, calls ----------

/// The kinds of item a node of the graph is defined by.
const DEFINES: &[&str] = &["function_item", "function_signature_item", "struct_item", "enum_item", "trait_item"];

/// A file's items by their first line and name, at their place in the file; the first of two
/// alike.
fn definitions(tree: &Tree, src: &[u8]) -> HashMap<(u32, String), (usize, usize)> {
    let mut out = HashMap::new();
    for d in walk(tree.root_node(), DEFINES) {
        if let Some(name) = d.child_by_field_name("name") {
            out.entry((d.start_position().row as u32 + 1, text(name, src).to_string())).or_insert((d.start_byte(), d.end_byte()));
        }
    }
    out
}

/// The item a node is defined by: the module's file, or the function, struct, enum or trait.
fn def_node<'t>(m: &'t Module, n: &Node) -> Option<TsNode<'t>> {
    if n.kind == NodeKind::Module {
        return Some(m.tree.root_node());
    }
    let (start, _) = n.span?;
    let &(from, to) = m.defs.get(&(start, n.name.clone()))?;
    let mut d = m.tree.root_node().descendant_for_byte_range(from, to)?;
    while !DEFINES.contains(&d.kind()) || d.byte_range() != (from..to) {
        d = d.parent()?;
    }
    Some(d)
}

/// Fields, signatures and bodies into edges: the program's, and those of the library's code read.
fn link(modules: &IndexMap<String, Module>, store: &mut GraphBuilder, crate_ids: &HashMap<String, String>) {
    let ids: Vec<String> = store.nodes.keys().cloned().collect();
    for id in &ids {
        let (kind, module) = (store.nodes[id].kind, store.nodes[id].module.clone());
        if kind != NodeKind::Class {
            continue;
        }
        let m = &modules[&module];
        let (fields, arms) = store.classes.get(id).map(|c| (c.fields.clone(), c.arms.clone())).unwrap_or_default();
        for (fname, types) in fields {
            for t in types {
                let Some(r) = resolve(m, &t, modules, &store.nodes, crate_ids) else { continue };
                if r.starts_with("ext:") {
                    store.classes.entry(id.clone()).or_default().foreign.entry(fname.clone()).or_insert(r);
                } else if store.nodes.get(&r).map_or(false, |x| x.kind == NodeKind::Class) {
                    store.classes.entry(id.clone()).or_default().attr_types.insert(fname.clone(), r.clone());
                    store.add_edge(id, &r, EdgeKind::Has, Some(fname.clone()));
                }
            }
        }
        // a variant is the types it carries that the graph holds; one that carries none of them
        // (`Fit { score: f64 }`, `Fit(f64)`) is a type of its own, named after its enum, which a
        // path does not name: `Outcome::Fit { .. }` makes an `Outcome`
        let mut seen: Vec<String> = vec![];
        for (vname, carried, at) in arms {
            let mut known: Vec<String> = vec![];
            for r in carried.iter().filter_map(|t| resolve(m, t, modules, &store.nodes, crate_ids)) {
                let holds = if r.starts_with("ext:") {
                    foreign_type(&r, &OWN)
                } else {
                    store.nodes.get(&r).map_or(false, |x| x.kind == NodeKind::Class)
                };
                if holds && r != *id && !known.contains(&r) {
                    known.push(r);
                }
            }
            if known.is_empty() {
                let enum_node = &store.nodes[id];
                let own = Node {
                    id: format!("{id}::{vname}"),
                    kind: NodeKind::Class,
                    name: format!("{}::{vname}", enum_node.name),
                    file: enum_node.file.clone(),
                    span: at,
                    module: module.clone(),
                    ..Default::default()
                };
                known.push(own.id.clone());
                store.add_node(own);
                store.add_edge(&module, &known[0], EdgeKind::Contains, None);
            }
            for r in known {
                if seen.contains(&r) {
                    continue;
                }
                if r.starts_with("ext:") {
                    store.ext_node(&r);
                }
                store.add_edge(id, &r, EdgeKind::Variant, None);
                seen.push(r);
            }
        }
    }
    for id in &ids {
        let (kind, module) = (store.nodes[id].kind, store.nodes[id].module.clone());
        if !matches!(kind, NodeKind::Function | NodeKind::Method) {
            continue;
        }
        let m = &modules[&module];
        let sig = store.sigs.get(id).cloned().unwrap_or_default();
        let own = store.nodes[id].cls.clone();
        for (types, kind) in [(&sig.ptypes, EdgeKind::Takes), (&sig.rtypes, EdgeKind::Returns)] {
            for t in types {
                // `Self` is the type the impl is for
                let resolved = match (t.first().map(String::as_str), &own) {
                    (Some("Self"), Some(c)) if t.len() == 1 => Some(c.clone()),
                    _ => resolve(m, t, modules, &store.nodes, crate_ids),
                };
                let Some(r) = resolved else { continue };
                if r.starts_with("ext:") {
                    if !foreign_type(&r, &OWN) {
                        continue;
                    }
                    store.ext_node(&r);
                }
                store.add_edge(id, &r, kind, None);
            }
        }
        take_sums(modules, store, crate_ids, id);
    }
    // every signature before any body: a body's locals take their types from what the functions
    // it calls give back, wherever those are defined
    let eval = Eval::new(modules, &store.nodes, crate_ids);
    let returned = returned_classes(store);
    let (mut waiting, mut args) = (vec![], vec![]);
    // the program's bodies, then those of the library's code read
    let (own, library): (Vec<&String>, Vec<&String>) = ids
        .iter()
        .filter(|id| matches!(store.nodes[*id].kind, NodeKind::Function | NodeKind::Method))
        .partition(|id| !store.nodes[*id].library);
    for id in own.into_iter().chain(library) {
        let l = link_calls(modules, store, crate_ids, &eval, &returned, id);
        waiting.extend(l.waiting.into_iter().map(|(t, b)| (id.clone(), t, b)));
        args.extend(l.args);
    }
    // a path or a statement built from a function's parameters is whole where its callers hand them
    let handing: Vec<Evaluated> = args.iter().map(|arg| Evaluated { arg, eval: &eval }).collect();
    let entry = |f: &str| store.nodes.get(f).map_or(false, |n| n.entry.is_some());
    let family = store.family();
    let completed = crate::paths::complete(waiting, &handing, &family, &|_, _| vec![], &entry);
    crate::paths::place_calls(store, completed, false);
    globals::read(modules, store, crate_ids);
}

/// The class each function gives back, by the first of its signature's types that is one.
fn returned_classes(store: &GraphBuilder) -> HashMap<String, String> {
    let mut out: HashMap<String, String> = HashMap::new();
    for e in
        store.edges.iter().filter(|e| e.kind == EdgeKind::Returns && store.nodes.get(&e.dst).map_or(false, |n| n.kind == NodeKind::Class))
    {
        out.entry(e.src.clone()).or_insert_with(|| e.dst.clone());
    }
    out
}

/// The library's sums a function takes, each a type of its own: `fetched: Result<Value,
/// reqwest::Error>` takes a `Result<Value, reqwest::Error>`, a sum whose variants are the types
/// it is written with, as an enum of the program is a sum of the types its variants hold (a type
/// of the program's or a library's that holds data). Its id is the type with each name resolved,
/// so one sum written two ways is one; a sum of none of those is none the graph holds.
fn take_sums(modules: &IndexMap<String, Module>, store: &mut GraphBuilder, crate_ids: &HashMap<String, String>, id: &str) {
    let n = store.nodes[id].clone();
    let m = &modules[&n.module];
    let src = &m.src;
    let Some(params) = def_node(m, &n).and_then(|d| d.child_by_field_name("parameters")) else { return };
    let resolved = |parts: &[String], nodes: &IndexMap<String, Node>| match (parts, &n.cls) {
        ([s], Some(c)) if s == "Self" => Some(c.clone()),
        _ => resolve(m, parts, modules, nodes, crate_ids),
    };
    for p in named_children(params).into_iter().filter(|p| p.kind() == "parameter") {
        let Some(mut t) = p.child_by_field_name("type") else { continue };
        while let Some(inner) = (t.kind() == "reference_type").then(|| t.child_by_field_name("type")).flatten() {
            t = inner;
        }
        let generic = path_parts(t.child_by_field_name("type").filter(|_| t.kind() == "generic_type"), src);
        if !generic.last().map_or(false, |g| knowledge::RUST_SUMS.contains(&g.as_str())) {
            continue;
        }
        // the program's own enum of that name is read as the program's
        let of = resolved(&generic, &store.nodes);
        if of.as_ref().map_or(false, |r| store.nodes.get(r).map_or(false, |x| x.kind == NodeKind::Class)) {
            continue;
        }
        let mut variants: Vec<String> = vec![];
        for a in t.child_by_field_name("type_arguments").map(named_children).unwrap_or_default() {
            for r in type_names(Some(a), src).iter().filter_map(|x| resolved(x, &store.nodes)) {
                let known = if r.starts_with("ext:") {
                    foreign_type(&r, &OWN)
                } else {
                    store.nodes.get(&r).map_or(false, |x| x.kind == NodeKind::Class)
                };
                if known && !variants.contains(&r) {
                    variants.push(r);
                }
            }
        }
        if variants.is_empty() {
            continue;
        }
        let mut meant = String::new();
        let mut at = t.start_byte();
        for x in walk(t, &["type_identifier", "scoped_type_identifier"]) {
            if x.start_byte() < at || (x.kind() == "type_identifier" && x.parent().map_or(false, |p| p.kind() == "scoped_type_identifier"))
            {
                continue;
            }
            let parts = path_parts(Some(x), src);
            meant.push_str(&String::from_utf8_lossy(&src[at..x.start_byte()]));
            let r = resolved(&parts, &store.nodes).unwrap_or_else(|| parts.join("."));
            meant.push_str(r.strip_prefix("ext:").unwrap_or(&r));
            at = x.end_byte();
        }
        meant.push_str(&String::from_utf8_lossy(&src[at..t.end_byte()]));
        let sum = format!("ext:{}", meant.split_whitespace().collect::<String>());
        if !store.nodes.contains_key(&sum) {
            let home = of.as_deref().map(|r| r.trim_start_matches("ext:")).and_then(|r| r.rsplit_once('.')).map_or("", |(h, _)| h);
            store.add_node(Node {
                id: sum.clone(),
                kind: NodeKind::External,
                name: text(t, src).split_whitespace().collect::<Vec<_>>().join(" "),
                module: home.to_string(),
                ..Default::default()
            });
            for v in variants {
                if v.starts_with("ext:") {
                    store.ext_node(&v);
                }
                store.add_edge(&sum, &v, EdgeKind::Variant, None);
            }
        }
        store.add_edge(id, &sum, EdgeKind::Takes, None);
    }
}

/// A function on a type, by the casing Rust's names follow (C-CASE): `File::open`, not
/// `fs::read_to_string`. What it returns is a value of that type, or one wrapping it.
fn on_type(parts: &[String]) -> bool {
    parts.len() > 1 && parts[parts.len() - 2].starts_with(|c: char| c.is_ascii_uppercase())
}

/// A library value a method is called on, traced to the call that made it.
struct Made<'t> {
    /// the type's path: `reqwest.Client`
    ty: Vec<String>,
    /// the making call's path: `reqwest.Client.new`
    ctor: Vec<String>,
    /// the method calls on it before a local held it: `append`, `open` for `f` in
    /// `let f = OpenOptions::new().append(true).open(p)?`
    before: Vec<(String, TsNode<'t>)>,
    /// the method calls on it in this expression, before this one
    here: Vec<(String, TsNode<'t>)>,
    /// where a local was made, when one holds the value
    origin: Option<TsNode<'t>>,
}

/// What an operation on a value of a library's type `ty` moves: a read of the path the value
/// holds (`read_to_string`), or what the effect tables say of `ty::op`; nothing for opening what
/// a call gave (`unwrap`).
fn moved_by(ty: &[String], op: &str) -> Option<Crossing> {
    match op {
        _ if knowledge::hands_on(op) => None,
        _ if knowledge::RUST_PATH_READS.contains(&op) => Some(Crossing::read(World::Disk)),
        _ => effects::boundary_rust(&[ty, &[op.to_string()]].concat()),
    }
}

/// The string literals under a node that can name a place: a closure's are its own code's.
fn place_literals(n: TsNode, src: &[u8]) -> Vec<String> {
    match n.kind() {
        "closure_expression" => vec![],
        "string_literal" | "raw_string_literal" => vec![unquote(text(n, src))],
        _ => named_children(n).into_iter().flat_map(|k| place_literals(k, src)).collect(),
    }
}

/// The name a call's value is kept by: `let x = f(..)?`, `let x = f(..).unwrap()`, `x = f(..)`,
/// through what an arm of a choice gives (`let x = if c { Some(f(..)) } else { None }`): the last
/// expression of a block, a `match` arm, `Some`, `Ok` and `Err`.
fn kept_as(c: TsNode, src: &[u8]) -> Option<String> {
    let mut n = c;
    loop {
        let p = n.parent()?;
        match p.kind() {
            k if WRAPPERS.contains(&k) || k == "else_clause" => n = p,
            "match_expression" if p.child_by_field_name("body").map(|b| b.id()) == Some(n.id()) => n = p,
            "block"
                if p.named_child(p.named_child_count().saturating_sub(1)).map(|x| x.id()) == Some(n.id())
                    && n.kind() != "expression_statement" =>
            {
                n = p
            }
            "arguments"
                if p.named_child_count() == 1
                    && p.parent()
                        .and_then(|c| c.child_by_field_name("function"))
                        .map_or(false, |f| matches!(text(f, src), "Some" | "Ok" | "Err")) =>
            {
                n = p.parent()?
            }
            _ if (CHOICES.chooses)(p, src) && !matches!((CHOICES.part)(p, n, src), Part::Decides) => n = p,
            // `.unwrap()` and its kind hand on the value they are called on
            "field_expression" => {
                let call = p.parent().filter(|x| x.kind() == "call_expression")?;
                if !p.child_by_field_name("field").map_or(false, |f| knowledge::hands_on(text(f, src))) {
                    return None;
                }
                n = call;
            }
            "let_declaration" => {
                let pat = p.child_by_field_name("pattern").filter(|x| matches!(x.kind(), "identifier" | "mut_pattern"))?;
                return Some(text(pat, src).trim_start_matches("mut ").to_string());
            }
            "assignment_expression" if p.child_by_field_name("right").map(|r| r.id()) == Some(n.id()) => {
                return p.child_by_field_name("left").filter(|l| l.kind() == "identifier").map(|l| text(l, src).to_string());
            }
            _ => return None,
        }
    }
}

/// The library value a chain of method calls starts from: a call of a function on a library's
/// type (`OpenOptions::new()`), reached through the locals that hold what it made
/// (`c` from `let c = reqwest::Client::new()`), or a value `declared` of a library's type (a
/// parameter `conn: &Connection`, a field `self.conn`), made wherever it was.
fn library_chain<'t>(
    mut v: TsNode<'t>,
    m: &Module,
    src: &[u8],
    made_by: &HashMap<String, TsNode<'t>>,
    declared: &dyn Fn(TsNode<'t>) -> Option<Vec<String>>,
) -> Option<Made<'t>> {
    let (mut before, mut here, mut origin) = (vec![], vec![], None);
    let mut locals = 0;
    loop {
        match v.kind() {
            k if WRAPPERS.contains(&k) => v = wrapped(v)?,
            "identifier" | "field_expression" if !made_by.contains_key(text(v, src)) => {
                let ty = declared(v)?;
                before.reverse();
                here.reverse();
                return Some(Made { ty, ctor: vec![], before, here, origin });
            }
            "identifier" if locals < 8 => {
                v = *made_by.get(text(v, src))?;
                origin.get_or_insert(v);
                locals += 1;
            }
            "call_expression" => {
                let f = v.child_by_field_name("function")?;
                if f.kind() == "field_expression" {
                    let call = (text(f.child_by_field_name("field")?, src).to_string(), v);
                    if locals == 0 {
                        here.push(call)
                    } else {
                        before.push(call)
                    }
                    v = f.child_by_field_name("value")?;
                    continue;
                }
                let parts = path_parts(Some(f), src);
                if !on_type(&parts) {
                    return None;
                }
                let ctor = expand(m, &parts);
                before.reverse();
                here.reverse();
                return Some(Made { ty: ctor[..ctor.len() - 1].to_vec(), ctor, before, here, origin });
            }
            _ => return None,
        }
    }
}

// ---------- calls among a macro's tokens ----------

/// What a parenthesised group among a macro's tokens calls.
enum Called<'t> {
    /// A path: `round2`, `Self::label`, `crate::report::round2`, `Vec::<u8>::new`.
    Path(Vec<String>),
    /// A method, with the names its receiver is reached by (`store` in `store.rows()`, `self`
    /// and `conn` in `self.conn.f()`); none when the receiver is no name (`a.f().g()`).
    Method(Vec<TsNode<'t>>, String),
}

/// The keywords after which a name and a group declare (`fn f(..)`, `struct S(..)`), name a
/// type (`dyn Fn(..)`) or start a pattern (`let Some(x) = ..`): no call. Among a macro's tokens
/// some are names to tree-sitter (`dyn`), so they are known by how they are written.
const NOT_CALLED_AFTER: &[&str] = &["fn", "struct", "enum", "union", "trait", "type", "dyn", "impl", "let"];

/// What a group among a macro's tokens calls: the path or the method written right before it. A
/// macro's tokens are no expression to tree-sitter, so they are read as Rust writes a call: a path
/// followed by a parenthesised group. A group in a pattern calls nothing: one an arm, a binding
/// or a guard follows (`Some(x) =>`, `Some(x) =`), or one where a macro takes a pattern
/// (`matches!(v, Some(_))`).
fn token_callee<'t>(g: TsNode<'t>, src: &[u8]) -> Option<Called<'t>> {
    if g.child(0).map(|x| x.kind()) != Some("(") || !in_macro(g) || in_pattern(g, src) {
        return None;
    }
    if g.next_sibling().map_or(false, |n| matches!(n.kind(), "=>" | "=" | "if" | "@")) {
        return None;
    }
    let mut name = g.prev_sibling()?;
    if name.kind() == ">" {
        name = turbofish(name)?;
    }
    if name.kind() != "identifier" {
        return None;
    }
    let mut parts = vec![text(name, src).to_string()];
    let mut at = name.prev_sibling();
    while let Some(sep) = at.filter(|t| t.kind() == "::") {
        let mut before = sep.prev_sibling()?;
        if before.kind() == ">" {
            before = turbofish(before)?;
        }
        if !matches!(before.kind(), "identifier" | "self" | "super" | "crate") {
            return None;
        }
        parts.insert(0, text(before, src).to_string());
        at = before.prev_sibling();
    }
    match at {
        Some(dot) if dot.kind() == "." => {
            let [method] = <[String; 1]>::try_from(parts).ok()?;
            Some(Called::Method(receiver(dot), method))
        }
        Some(k) if NOT_CALLED_AFTER.contains(&text(k, src)) => None,
        _ => Some(Called::Path(parts)),
    }
}

/// The names a method's receiver is reached by, from the `.` before the method: `self.conn` ->
/// `self`, `conn`; none when the chain passes through something that is not a name (`a.f().g()`,
/// `Type::X.g()`).
fn receiver(dot: TsNode) -> Vec<TsNode> {
    let mut names = vec![];
    let mut t = dot.prev_sibling();
    while let Some(n) = t {
        if !matches!(n.kind(), "identifier" | "self") {
            return vec![];
        }
        names.insert(0, n);
        match n.prev_sibling() {
            Some(d) if d.kind() == "." => t = d.prev_sibling(),
            Some(d) if d.kind() == "::" => return vec![],
            _ => return names,
        }
    }
    vec![]
}

/// The token before the turbofish that `close` ends (`f` of `f::<T>`); none when `close` ends no
/// turbofish (`a > (b)`).
fn turbofish(close: TsNode) -> Option<TsNode> {
    let mut depth = 0;
    let mut t = Some(close);
    while let Some(x) = t {
        match x.kind() {
            ">" => depth += 1,
            "<" => {
                depth -= 1;
                if depth == 0 {
                    return x.prev_sibling().filter(|s| s.kind() == "::")?.prev_sibling();
                }
            }
            _ => {}
        }
        t = x.prev_sibling();
    }
    None
}

/// Whether a group sits among the tokens of a macro's invocation, below its own arguments.
fn in_macro(g: TsNode) -> bool {
    let mut t = g.parent().filter(|p| p.kind() == "token_tree");
    while let Some(x) = t {
        match x.parent() {
            Some(p) if p.kind() == "token_tree" => t = Some(p),
            Some(p) => return p.kind() == "macro_invocation",
            None => return false,
        }
    }
    false
}

/// Whether a group stands in the pattern of a macro that takes one: after its first argument,
/// before a guard's `if`.
fn in_pattern(g: TsNode, src: &[u8]) -> bool {
    let mut child = g;
    while let Some(t) = child.parent().filter(|t| t.kind() == "token_tree") {
        let name = match t.parent() {
            Some(m) if m.kind() == "macro_invocation" => m.child_by_field_name("macro").and_then(|x| path_parts(Some(x), src).pop()),
            _ => t.prev_sibling().filter(|b| b.kind() == "!").and_then(|b| b.prev_sibling()).map(|x| text(x, src).to_string()),
        };
        if name.map_or(false, |n| knowledge::RUST_PATTERN_MACROS.contains(&n.as_str())) {
            let kids: Vec<TsNode> = (0..t.child_count()).filter_map(|i| t.child(i)).collect();
            let at = kids.iter().position(|k| k.id() == child.id());
            let comma = kids.iter().position(|k| k.kind() == ",");
            let guard = comma.and_then(|c| kids.iter().skip(c).position(|k| text(*k, src) == "if").map(|i| c + i));
            if let (Some(at), Some(comma)) = (at, comma) {
                if at > comma && guard.map_or(true, |g| at < g) {
                    return true;
                }
            }
        }
        child = t;
    }
    false
}

/// The names among a call's tokens that stand for values: not a field after a `.`, not a part of
/// a path, not a macro.
fn token_values(g: TsNode) -> Vec<TsNode> {
    walk(g, &["identifier"])
        .into_iter()
        .filter(|i| {
            !matches!(i.prev_sibling().map(|p| p.kind()), Some("." | "::"))
                && !matches!(i.next_sibling().map(|n| n.kind()), Some("::" | "!"))
        })
        .collect()
}

/// What a call is handed, by place: its arguments; among a macro's tokens, each argument that is
/// one name or literal (`&x` is `x`), and none for one written another way.
fn positional(c: TsNode) -> Vec<Option<TsNode>> {
    if c.kind() != "token_tree" {
        return c
            .child_by_field_name("arguments")
            .map(named_children)
            .unwrap_or_default()
            .into_iter()
            .filter(|a| !a.kind().ends_with("comment"))
            .map(Some)
            .collect();
    }
    paths::token_groups(c)
        .into_iter()
        .map(|g| {
            let rest: Vec<TsNode> = g.into_iter().skip_while(|t| matches!(t.kind(), "&" | "mut" | "mutable_specifier")).collect();
            match rest.as_slice() {
                [one] if one.is_named() && one.kind() != "token_tree" => Some(*one),
                _ => None,
            }
        })
        .collect()
}

/// Expand the head of a call path through its binding, so `fs::read` becomes `std.fs.read`.
fn expand(m: &Module, parts: &[String]) -> Vec<String> {
    match parts.first().and_then(|h| m.bindings.get(h)) {
        Some(b) if b.starts_with("ext:") => {
            let mut out: Vec<String> = b[4..].split('.').map(String::from).collect();
            out.extend(parts[1..].iter().cloned());
            out
        }
        _ => parts.to_vec(),
    }
}

/// What reading a body gives besides its edges and touches: its file and database calls with
/// their paths and statements as built (placed once the parameters' values are known), and the
/// arguments of its calls of the program's functions.
#[derive(Default)]
struct Linked<'t> {
    waiting: Vec<(Touch, Vec<crate::paths::Built>)>,
    args: Vec<Arg<'t>>,
}

fn link_calls<'t>(
    modules: &'t IndexMap<String, Module>,
    store: &mut GraphBuilder,
    crate_ids: &HashMap<String, String>,
    eval: &Eval<'t>,
    returned: &HashMap<String, String>,
    id: &str,
) -> Linked<'t> {
    let n = store.nodes[id].clone();
    let m = &modules[&n.module];
    let Some(d) = def_node(m, &n) else { return Linked::default() };
    let Some(body) = d.child_by_field_name("body") else { return Linked::default() };
    let mut steps = Steps::new(body, &m.src, &CHOICES, &USES);
    let read = {
        let mut b = Body::new(modules, store, crate_ids, eval, returned, m, &n, d, body);
        // a loop binds its element once what it goes over is made, and before its body runs
        let calls =
            evaluation_order(walk(body, &["call_expression", "struct_expression", "identifier", "scoped_identifier", "token_tree"]));
        let mut ran: Vec<((usize, u8), TsNode)> = calls.into_iter().map(|c| ((c.end_byte(), 0), c)).collect();
        let iterations = b.iterations(body);
        let at: HashMap<usize, (TsNode, TsNode)> = iterations.iter().map(|(p, v)| (p.id(), (*p, *v))).collect();
        ran.extend(iterations.iter().map(|(p, v)| ((v.end_byte(), 1), *p)));
        ran.sort_by_key(|(k, _)| *k);
        for ((_, rank), c) in ran {
            if rank == 1 {
                let (pat, value) = at[&c.id()];
                b.each(&mut steps, pat, value);
                continue;
            }
            let (t, w) = (b.read.touches.len(), b.read.waiting.len());
            b.call(&mut steps, c);
            steps.mark(c, b.read.touches[t..].iter_mut().chain(b.read.waiting[w..].iter_mut().map(|(t, _)| t)));
        }
        b.read
    };
    let receiver = n.params.iter().find(|p| p.receiver).map(|p| p.name.as_str());
    let is_type = |(a, b): (usize, usize)| names_type(&String::from_utf8_lossy(&m.src[a..b]));
    let compares =
        crate::tree::compares(body, &m.src, steps.choices(), &COMPARES, &CHOICES, &crate::tree::Scope { receiver, is_type: &is_type });
    steps.finish(store, id);
    if let Some(node) = store.nodes.get_mut(id) {
        node.compares = compares;
        node.raises = read.exits || crate::tree::holds(body, &m.src, panics, &["function_item", "impl_item", "trait_item", "mod_item"]);
    }
    for (t, input) in read.mutates {
        store.add_edge(id, &t, EdgeKind::Mutates, Some(input));
    }
    if let Some(node) = store.nodes.get_mut(id) {
        node.calls_handed = read.unresolved.iter().any(|(u, _)| n.params.iter().any(|p| &p.name == u));
    }
    for (name, bucket) in read.unresolved {
        store.not_followed(name, bucket);
    }
    for t in read.touches {
        store.touch(id, t);
    }
    Linked { waiting: read.waiting, args: read.args }
}

/// The names a function binds: its parameters, its type parameters and those of the impl or
/// trait it is in, and in its body each `let`, closure parameter, loop or match binding, and
/// function declared there.
fn bound_in(def: TsNode, src: &[u8]) -> HashSet<String> {
    fn pattern(p: TsNode, src: &[u8], out: &mut HashSet<String>) {
        match p.kind() {
            "identifier" | "shorthand_field_identifier" => {
                out.insert(text(p, src).to_string());
            }
            // the type a pattern takes apart (`Some(x)`) and a guard bind nothing
            _ => {
                let skipped = [p.child_by_field_name("type"), p.child_by_field_name("condition")];
                for c in named_children(p).into_iter().filter(|c| !skipped.contains(&Some(*c))) {
                    pattern(c, src, out);
                }
            }
        }
    }
    let mut out = HashSet::new();
    let mut scope = Some(def);
    while let Some(s) = scope {
        if s.id() == def.id() || matches!(s.kind(), "impl_item" | "trait_item") {
            for t in s.child_by_field_name("type_parameters").map(named_children).unwrap_or_default() {
                let name = if t.kind() == "type_identifier" {
                    Some(t)
                } else {
                    t.child_by_field_name("left").or_else(|| t.child_by_field_name("name"))
                };
                out.extend(name.map(|n| text(n, src).to_string()));
            }
        }
        scope = s.parent();
    }
    for n in
        walk(def, &["parameter", "closure_parameters", "let_declaration", "for_expression", "match_arm", "let_condition", "function_item"])
    {
        match n.kind() {
            "closure_parameters" => named_children(n).into_iter().for_each(|p| pattern(p, src, &mut out)),
            "function_item" if n.id() != def.id() => out.extend(n.child_by_field_name("name").map(|x| text(x, src).to_string())),
            "function_item" => {}
            _ => n.child_by_field_name("pattern").into_iter().for_each(|p| pattern(p, src, &mut out)),
        }
    }
    out
}

/// What reading a body finds besides its steps: what it touches, the file and database calls
/// whose paths and statements wait for what callers hand, the calls of the program's functions
/// with their arguments, the calls Atlas could not follow, the inputs it changes in place, and
/// whether it calls what ends the program.
#[derive(Default)]
struct Read<'t> {
    touches: Vec<Touch>,
    waiting: Vec<(Touch, Vec<crate::paths::Built>)>,
    args: Vec<Arg<'t>>,
    unresolved: Vec<(String, Bucket)>,
    mutates: Vec<(String, String)>,
    exits: bool,
}

/// A function's body as Rust reads it: the program around it, and the classes its names hold.
struct Body<'a, 't> {
    modules: &'a IndexMap<String, Module>,
    store: &'a GraphBuilder,
    crate_ids: &'a HashMap<String, String>,
    eval: &'a Eval<'t>,
    m: &'t Module,
    src: &'t [u8],
    /// The function the body is of, its id, and its parameters by name.
    id: String,
    def: TsNode<'t>,
    params: Vec<String>,
    cls: Option<String>,
    /// The classes the body's names hold: its typed parameters, and locals assigned from a
    /// constructor, a struct literal or a function with a typed return, or handed to a parameter
    /// that takes one.
    locals: HashMap<String, String>,
    /// The locals a call made, by the call: `let c = reqwest::Client::new()`.
    made_by: HashMap<String, TsNode<'t>>,
    /// The classes the fields of the body's own class hold.
    attr_types: std::collections::BTreeMap<String, String>,
    /// The library types its parameters are declared with: `conn` is `ext:rusqlite.Connection`.
    foreign: HashMap<String, String>,
    /// The library types its `let`s hold by the parameter they are handed to: `area` is a
    /// `DrawingArea` where `draw(&area, ..)` takes one.
    taken: HashMap<String, String>,
    /// The class each function gives back, by its signature.
    returned: &'a HashMap<String, String>,
    /// The names the function binds, found when a call not followed first asks.
    bound: std::cell::OnceCell<HashSet<String>>,
    /// It gives no value: what the last expression of its block gives goes nowhere.
    unit: bool,
    read: Read<'t>,
}

const STRINGS: &[&str] = &["string_literal", "raw_string_literal"];

impl<'a, 't> Body<'a, 't> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        modules: &'a IndexMap<String, Module>,
        store: &'a GraphBuilder,
        crate_ids: &'a HashMap<String, String>,
        eval: &'a Eval<'t>,
        returned: &'a HashMap<String, String>,
        m: &'t Module,
        n: &Node,
        d: TsNode<'t>,
        body: TsNode<'t>,
    ) -> Self {
        let cls = n.cls.clone();
        let attr_types = cls.as_ref().and_then(|c| store.classes.get(c)).map(|c| c.attr_types.clone()).unwrap_or_default();
        let params = paths::param_names(d, &m.src);
        let mut b = Body {
            modules,
            store,
            crate_ids,
            eval,
            m,
            src: &m.src,
            id: n.id.clone(),
            def: d,
            params,
            cls,
            locals: HashMap::new(),
            made_by: HashMap::new(),
            attr_types,
            foreign: HashMap::new(),
            taken: HashMap::new(),
            returned,
            bound: Default::default(),
            unit: n.returns.as_ref().map_or(true, |r| r.value.is_empty() && !r.own),
            read: Read::default(),
        };
        b.parameters(d);
        let mut untyped = b.lets(body);
        b.loops(d, body);
        // a loop variable over a sequence whose type the body does not write is typed by how it is used, as a `let` is
        untyped.extend(
            b.iterations(body)
                .into_iter()
                .map(|(p, _)| p)
                .filter(|p| matches!(p.kind(), "identifier" | "mut_pattern" | "reference_pattern"))
                .map(|p| text(p, b.src).trim_start_matches('&').trim_start_matches("mut ").trim().to_string()),
        );
        b.patterns(body);
        b.handed_to(body, untyped);
        // a macro that prints shows the human something
        for mac in walk(body, &["macro_invocation"]) {
            let name = mac.child_by_field_name("macro").map(|x| text(x, b.src)).unwrap_or("");
            if effects::screen_macro(name) {
                b.read.touches.push(Touch {
                    op: name.into(),
                    line: mac.start_position().row as u32 + 1,
                    ..Touch::new(World::Screen, None)
                });
            }
        }
        b
    }

    fn resolve(&self, parts: &[String]) -> Option<String> {
        resolve(self.m, parts, self.modules, &self.store.nodes, self.crate_ids)
    }

    fn resolve_value(&self, parts: &[String]) -> Option<String> {
        resolve_value(self.m, parts, self.modules, &self.store.nodes, self.crate_ids)
    }

    fn class_of(&self, id: &str) -> Option<String> {
        self.store.nodes.get(id).filter(|x| x.kind == NodeKind::Class).map(|_| id.to_string())
    }

    /// The class a function gives back, by its signature.
    fn returned_class(&self, id: &str) -> Option<String> {
        self.returned.get(id).cloned()
    }

    /// Typed parameters give receivers a type (`store.add_edge(..)` with `store: &mut
    /// GraphBuilder`), and `&mut T`, `&RefCell<T>` and `&mut self` hand the value back changed:
    /// `T -> T`.
    fn parameters(&mut self, d: TsNode<'t>) {
        let src = self.src;
        let Some(pn) = d.child_by_field_name("parameters") else { return };
        for p in named_children(pn) {
            if p.kind() == "self_parameter" && text(p, src).contains("mut") {
                self.read.mutates.extend(self.cls.clone().map(|c| (c, "self".to_string())));
                continue;
            }
            if p.kind() != "parameter" {
                continue;
            }
            let Some(pat) = p.child_by_field_name("pattern") else { continue };
            let pname = text(pat, src).trim_start_matches("mut ").to_string();
            let by_mut_ref = p.child_by_field_name("type").map_or(false, |t| text(t, src).starts_with("&mut") || changes_inside(t, src));
            let types = type_names(p.child_by_field_name("type"), src);
            if let Some(lib) = types.iter().find_map(|t| self.resolve(t).filter(|r| r.starts_with("ext:"))) {
                self.foreign.insert(pname.clone(), lib);
            }
            let class = types.iter().find_map(|t| self.resolve(t).filter(|r| !r.starts_with("ext:")).and_then(|r| self.class_of(&r)));
            if let Some(r) = class {
                if by_mut_ref {
                    self.read.mutates.push((r.clone(), pname.clone()));
                }
                self.locals.insert(pname, r);
            }
        }
    }

    /// The names a pattern binds hold the types its struct or enum variant declares for them:
    /// `Command::Report { format, .. }` binds `format` to the `Format` the variant's field is.
    fn patterns(&mut self, body: TsNode<'t>) {
        let src = self.src;
        for p in walk(body, &["struct_pattern"]) {
            let Some(fields) = self.declared_fields(&path_parts(p.child_by_field_name("type"), src)) else { continue };
            for f in named_children(p).into_iter().filter(|f| f.kind() == "field_pattern") {
                // `format` binds the field, `format: f` binds `f`
                let Some(field) = f.child_by_field_name("name").map(|x| text(x, src)) else { continue };
                let bound = match f.child_by_field_name("pattern") {
                    Some(x) if x.kind() == "identifier" => text(x, src),
                    Some(_) => continue,
                    None => field,
                };
                if let Some((_, class)) = fields.iter().find(|(n, _)| n == field) {
                    self.locals.insert(bound.to_string(), class.clone());
                }
            }
        }
    }

    /// The program's classes a struct pattern's fields hold, by field: a struct's, or an enum
    /// variant's (`Command::Report`), read where the enum declares it.
    fn declared_fields(&self, parts: &[String]) -> Option<Vec<(String, String)>> {
        if let Some(r) = self.resolve(parts).filter(|r| !r.starts_with("ext:")).and_then(|r| self.class_of(&r)) {
            return self.store.classes.get(&r).map(|c| c.attr_types.clone().into_iter().collect());
        }
        let (variant, owner) = parts.split_last()?;
        let e = self.resolve(owner).filter(|r| !r.starts_with("ext:")).and_then(|r| self.class_of(&r))?;
        let n = self.store.nodes.get(&e)?;
        let home = self.modules.get(&n.module)?;
        let item = def_node(home, n).filter(|d| d.kind() == "enum_item")?;
        let v = walk(item, &["enum_variant"])
            .into_iter()
            .find(|v| v.child_by_field_name("name").map(|x| text(x, &home.src)) == Some(variant.as_str()))?;
        let list = v.child_by_field_name("body").filter(|b| b.kind() == "field_declaration_list")?;
        let class = |t: Vec<String>| {
            resolve(home, &t, self.modules, &self.store.nodes, self.crate_ids)
                .filter(|r| !r.starts_with("ext:"))
                .and_then(|r| self.class_of(&r))
        };
        Some(
            named_children(list)
                .into_iter()
                .filter(|f| f.kind() == "field_declaration")
                .filter_map(|f| {
                    Some((
                        text(f.child_by_field_name("name")?, &home.src).to_string(),
                        type_names(f.child_by_field_name("type"), &home.src).into_iter().find_map(class)?,
                    ))
                })
                .collect(),
        )
    }

    /// Locals assigned from a constructor, a struct literal, or a function with a typed return;
    /// `.unwrap()`, `.expect(..)`, `.context(..)` hand on the type of the value they are called on.
    /// Gives the `let`s that write no type and whose value is of none the body knows.
    fn lets(&mut self, body: TsNode<'t>) -> Vec<String> {
        let src = self.src;
        let mut untyped = vec![];
        for l in walk(body, &["let_declaration"]) {
            let Some(pat) = l.child_by_field_name("pattern").filter(|p| matches!(p.kind(), "identifier" | "mut_pattern")) else { continue };
            let name = text(pat, src).trim_start_matches("mut ").to_string();
            let declared = type_names(l.child_by_field_name("type"), src);
            if let Some(t) = declared.iter().find_map(|t| self.resolve(t).filter(|r| r.starts_with("ext:"))) {
                self.foreign.insert(name.clone(), t);
            }
            let annotated = declared.iter().find_map(|t| self.resolve(t).and_then(|t| self.class_of(&t)));
            let mut value = l.child_by_field_name("value");
            while let Some(v) = value {
                value = match v.kind() {
                    k if WRAPPERS.contains(&k) => wrapped(v),
                    "unary_expression" => named_children(v).into_iter().next(),
                    "call_expression" => match v.child_by_field_name("function") {
                        Some(f)
                            if f.kind() == "field_expression"
                                && f.child_by_field_name("field").map_or(false, |x| knowledge::hands_on(text(x, src))) =>
                        {
                            f.child_by_field_name("value")
                        }
                        _ => break,
                    },
                    _ => break,
                };
            }
            if let Some(made) = l.child_by_field_name("value") {
                self.made_by.insert(name.clone(), made);
            }
            match annotated.or_else(|| value.and_then(|v| self.value_class(v))) {
                Some(t) => {
                    self.locals.insert(name, t);
                }
                None if l.child_by_field_name("type").is_none() => untyped.push(name),
                None => {}
            }
        }
        untyped
    }

    /// A loop variable over a sequence the body names (`for r in readings`, `&readings`,
    /// `readings.iter()`) is an element of it, of the class the sequence's written type holds
    /// (`readings: &[Reading]`, `let rows: Vec<Row>`).
    fn loops(&mut self, d: TsNode<'t>, body: TsNode<'t>) {
        let src = self.src;
        let typed = |n: TsNode<'t>, name: &str| n.child_by_field_name("type").map(|t| (name.to_string(), text(t, src).to_string()));
        let mut written: HashMap<String, String> = HashMap::new();
        for p in d.child_by_field_name("parameters").map(named_children).unwrap_or_default() {
            if let Some(pat) = p.child_by_field_name("pattern") {
                written.extend(typed(p, text(pat, src).trim_start_matches("mut ")));
            }
        }
        for l in walk(body, &["let_declaration"]) {
            if let Some(pat) = l.child_by_field_name("pattern").filter(|p| matches!(p.kind(), "identifier" | "mut_pattern")) {
                written.extend(typed(l, text(pat, src).trim_start_matches("mut ")));
            }
        }
        let sequence = |t: &str| {
            let head = t.trim_start_matches('&').trim_start_matches("mut ").trim();
            head.starts_with('[')
                || knowledge::RUST_SEQUENCES.contains(&head.split('<').next().unwrap_or("").rsplit("::").next().unwrap_or(""))
        };
        for (pat, value) in self.iterations(body) {
            if !matches!(pat.kind(), "identifier" | "mut_pattern" | "reference_pattern") {
                continue;
            }
            let name = text(pat, src).trim_start_matches('&').trim_start_matches("mut ").trim().to_string();
            let element = match self.sequence(value) {
                // a sequence the body names, as its type is written (`readings: &[Reading]`)
                Some(s) if s.kind() == "identifier" => {
                    written.get(text(s, src)).filter(|t| sequence(t)).and_then(|_| self.locals.get(text(s, src)).cloned())
                }
                // a field of a value of the program's struct (`g.nodes`): the class its type names
                Some(s) if s.kind() == "field_expression" => {
                    let owner = s.child_by_field_name("value").and_then(|o| self.locals.get(text(o, src)).cloned());
                    let field = s.child_by_field_name("field").map(|f| text(f, src)).unwrap_or("");
                    let named = owner
                        .and_then(|c| self.store.classes.get(&c))
                        .and_then(|k| k.fields.iter().find(|(f, _)| f == field))
                        .map(|(_, t)| t.clone());
                    named.unwrap_or_default().iter().find_map(|t| self.resolve(t).and_then(|r| self.class_of(&r)))
                }
                _ => None,
            };
            if let Some(class) = element {
                self.locals.insert(name, class);
            }
        }
    }

    /// What the body goes over element by element: each `for` loop's pattern and value.
    fn iterations(&self, body: TsNode<'t>) -> Vec<(TsNode<'t>, TsNode<'t>)> {
        walk(body, &["for_expression"])
            .into_iter()
            .filter_map(|l| Some((l.child_by_field_name("pattern")?, l.child_by_field_name("value")?)))
            .collect()
    }

    /// What a loop goes over, through `&xs`, `(xs)`, the methods that go over a sequence's
    /// elements and the adaptors that keep them (`xs.iter().filter(..)`): the sequence itself.
    fn sequence(&self, value: TsNode<'t>) -> Option<TsNode<'t>> {
        let src = self.src;
        let mut over = Some(value);
        while let Some(v) = over {
            over = match v.kind() {
                k if WRAPPERS.contains(&k) => wrapped(v),
                "call_expression" => v.child_by_field_name("function").filter(|f| {
                    f.kind() == "field_expression"
                        && f.child_by_field_name("field").map_or(false, |x| {
                            knowledge::RUST_ELEMENTS.contains(&text(x, src)) || knowledge::RUST_KEEPS_ELEMENTS.contains(&text(x, src))
                        })
                }),
                "field_expression" if v.parent().map_or(false, |p| p.kind() == "call_expression") => v.child_by_field_name("value"),
                _ => return Some(v),
            };
        }
        None
    }

    /// Rust fixes the type of a `let` that writes none by how the value is used: a `let` whose
    /// value is of no type the body knows, handed whole to a parameter of one of the program's
    /// functions (`draw(&area, ..)`), holds that parameter's type, read in the module that
    /// declares it: a class of the program's, or a library's (`area: &DrawingArea<DB, Shift>`
    /// through `use plotters::prelude::*`).
    fn handed_to(&mut self, body: TsNode<'t>, untyped: Vec<String>) {
        let src = self.src;
        let mut open: HashSet<String> = untyped.into_iter().filter(|n| !self.locals.contains_key(n)).collect();
        for c in walk(body, &["call_expression"]) {
            if open.is_empty() {
                return;
            }
            let Some((t, on_value)) = self.callee(c) else { continue };
            let Some(n) = self.store.nodes.get(&t) else { continue };
            let Some(home) = self.modules.get(&n.module) else { continue };
            let Some(def) = def_node(home, n) else { continue };
            let declared: Vec<TsNode> = def
                .child_by_field_name("parameters")
                .map(named_children)
                .unwrap_or_default()
                .into_iter()
                .filter(|p| p.kind() == "parameter")
                .collect();
            let skip = usize::from(!on_value && paths::takes_self(def));
            for (p, a) in declared.into_iter().zip(positional(c).into_iter().skip(skip)) {
                let a = a.map(|a| beneath(a, WRAPPERS));
                let Some(name) = a.filter(|x| x.kind() == "identifier").map(|x| text(x, src).to_string()).filter(|x| open.contains(x))
                else {
                    continue;
                };
                let ty = p.child_by_field_name("type");
                let named = |parts: &Vec<String>| match parts.as_slice() {
                    [s] if s == "Self" => n.cls.clone(),
                    _ => resolve(home, parts, self.modules, &self.store.nodes, self.crate_ids),
                };
                // a class of the program's as its parameter is typed, the records it holds
                // (`&[Month]`); a library's type only where it is the parameter's own, never one
                // it holds (`&[(NaiveDate, f64)]` names no what)
                let class =
                    type_names(ty, &home.src).iter().filter_map(named).find(|r| !r.starts_with("ext:") && self.class_of(r).is_some());
                let lib = Some(path_parts(ty, &home.src))
                    .filter(|parts| !parts.is_empty())
                    .as_ref()
                    .and_then(named)
                    .filter(|r| foreign_type(r, &OWN));
                match (class, lib) {
                    (Some(class), _) => self.locals.insert(name.clone(), class),
                    (None, Some(lib)) => self.taken.insert(name.clone(), lib),
                    (None, None) => continue,
                };
                open.remove(&name);
            }
        }
    }

    /// The program's function a call runs, and whether it is a method called on a value: one on
    /// the class its receiver holds, or one its path names.
    fn callee(&self, c: TsNode) -> Option<(String, bool)> {
        let src = self.src;
        let f = c.child_by_field_name("function")?;
        let own = |t: String| self.store.nodes.get(&t).filter(|n| matches!(n.kind, NodeKind::Function | NodeKind::Method)).map(|_| t);
        if f.kind() == "field_expression" {
            let class = self.value_class(f.child_by_field_name("value")?)?;
            return own(self.store.method_of(&class, text(f.child_by_field_name("field")?, src))?).map(|t| (t, true));
        }
        let parts = path_parts(Some(f), src);
        let t = match (parts.first().map(String::as_str), &self.cls) {
            (Some("Self"), Some(cls)) => self.store.method_of(cls, parts.get(1)?)?,
            _ => self.resolve_value(&parts)?,
        };
        own(t).map(|t| (t, false))
    }

    /// The class a value is of, where the body says: a name it holds, a struct literal, a method
    /// by what its signature gives back on the class of its receiver (a name, `self`, or the value
    /// another call gives), a function by its return.
    fn value_class(&self, v: TsNode) -> Option<String> {
        let src = self.src;
        match v.kind() {
            k if WRAPPERS.contains(&k) => self.value_class(wrapped(v)?),
            "self" => self.cls.clone(),
            "identifier" => self.locals.get(text(v, src)).cloned(),
            "struct_expression" => self.resolve(&path_parts(v.child_by_field_name("name"), src)).and_then(|r| self.class_of(&r)),
            "call_expression" if v.child_by_field_name("function").map_or(false, |f| f.kind() == "field_expression") => {
                let f = v.child_by_field_name("function")?;
                let on = self.value_class(f.child_by_field_name("value")?);
                let name = f.child_by_field_name("field").map(|x| text(x, src)).unwrap_or("");
                on.and_then(|t| self.store.method_of(&t, name)).and_then(|t| self.returned_class(&t))
            }
            "call_expression" => {
                let parts = path_parts(v.child_by_field_name("function"), src);
                if parts.is_empty() {
                    return None;
                }
                self.resolve_value(&parts)
                    .as_deref()
                    .and_then(|f| self.returned_class(f))
                    .or_else(|| self.resolve(&parts[..parts.len() - 1]).and_then(|r| self.class_of(&r)))
            }
            _ => None,
        }
    }

    /// The type a name of the body holds: a class of the program's, else the library's type a
    /// `let` takes from the parameter it is handed to.
    fn held_type(&self, name: &str) -> Option<String> {
        self.locals.get(name).or_else(|| self.taken.get(name)).cloned()
    }

    /// What a call is handed: the value it is called on, its arguments, a literal's fields.
    fn hands(&self, parts: &[Option<TsNode>], kept: &HashSet<String>) -> Vec<Held> {
        let ty_of = |name: &str| self.held_type(name);
        let mut out: Vec<Held> = vec![];
        for p in parts.iter().flatten() {
            for h in crate::tree::held_names(*p, self.src, &["self"], &ty_of, kept) {
                if !out.contains(&h) {
                    out.push(h);
                }
            }
        }
        out
    }

    /// A loop as the step it is: it takes what it goes over, and gives each element under the
    /// name it binds (`for n in &g.nodes` takes `g` and gives `n`), of the class `loops` found.
    fn each(&mut self, steps: &mut Steps, pat: TsNode<'t>, value: TsNode<'t>) {
        let names: Vec<String> = walk(pat, &["identifier"]).into_iter().map(|i| text(i, self.src).to_string()).collect();
        let hands = self.hands(&[Some(value)], &steps.kept);
        let gives: Vec<Held> = names
            .iter()
            .filter_map(|n| match self.held_type(n) {
                Some(t) => Some(Held::new(t, n.clone())),
                None => (!hands.is_empty()).then(|| Held::new(String::new(), n.clone())),
            })
            .collect();
        // the step stands where what it goes over is written: a call there hands it what it gives, the loop's body does not
        steps.op(value, &format!("for {} in {}", text(pat, self.src), text(value, self.src)), hands, gives);
    }

    /// What a call gives back, by the name the body keeps it by: the type the local is declared
    /// with, else the class the call builds or returns.
    fn gives(&self, c: TsNode, to: Option<&str>) -> Vec<Held> {
        let made = |t: &str| self.class_of(t).or_else(|| self.returned_class(t));
        kept_as(c, self.src)
            .map(|name| Held::new(self.held_type(&name).or_else(|| to.and_then(made)).unwrap_or_default(), name))
            .into_iter()
            .collect()
    }

    /// A boundary call's touches, refined by the literals that name its place.
    fn touch(&self, c: TsNode, kind: Crossing, op: &str, lits: &[String]) -> Vec<Touch> {
        Crosses::read(c, self.src, kind, None, op, lits, STRINGS, &self.locals).touches(None)
    }

    /// One call of the body, as the step it is: a struct literal, a method on a value, or a path,
    /// written as an expression or among a macro's tokens.
    fn call(&mut self, steps: &mut Steps, c: TsNode<'t>) {
        if c.kind() == "struct_expression" {
            return self.literal(steps, c);
        }
        if matches!(c.kind(), "identifier" | "scoped_identifier") {
            return self.unit(steps, c);
        }
        if c.kind() == "token_tree" {
            return self.token_call(steps, c);
        }
        let Some(f) = c.child_by_field_name("function") else { return };
        self.handed_on(steps, c);
        if f.kind() == "field_expression" {
            self.method_call(steps, c, f);
        } else {
            self.path_call(steps, c, path_parts(Some(f), self.src));
        }
    }

    /// A call among a macro's tokens, `c` the group it is handed (`(e.amount)` of `round2` in
    /// `params![.., round2(e.amount)]`): it runs like any call, so it is followed like one. A
    /// method is followed on a receiver the body knows the class of (`store.rows()`, `self.f()`,
    /// `self.conn.f()`).
    fn token_call(&mut self, steps: &mut Steps, c: TsNode<'t>) {
        let src = self.src;
        let Some(called) = token_callee(c, src) else { return };
        self.handed_on(steps, c);
        let (recv, method) = match called {
            Called::Path(parts) => return self.path_call(steps, c, parts),
            Called::Method(recv, method) => (recv, method),
        };
        let owner = |t: TsNode| if t.kind() == "self" { self.cls.clone() } else { self.locals.get(text(t, src)).cloned() };
        let class = match recv.as_slice() {
            [r] => owner(*r),
            [o, f] => owner(*o).and_then(|c| self.store.classes.get(&c)).and_then(|k| k.attr_types.get(text(*f, src)).cloned()),
            _ => None,
        };
        match class {
            // a method not found on the type (`to_string`, `len`) is not known; it builds nothing
            Some(r) => {
                if let Some(t) = self.store.method_of(&r, &method).filter(|t| !t.starts_with("ext:")) {
                    self.own_step(steps, c, &t, recv.last().copied(), Some(&r), None);
                }
            }
            None => self.library_step(steps, c, &method, recv.last().copied()),
        }
    }

    /// `Type { .. }`: a value of one of the program's types, built from its fields.
    fn literal(&self, steps: &mut Steps, c: TsNode<'t>) {
        let Some(r) = self
            .resolve(&path_parts(c.child_by_field_name("name"), self.src))
            .filter(|r| !r.starts_with("ext:"))
            .and_then(|r| self.class_of(&r))
        else {
            return;
        };
        let step =
            Step { hands: self.hands(&[c.child_by_field_name("body")], &steps.kept), gives: self.gives(c, Some(&r)), ..Default::default() };
        steps.call(c, &r, EdgeKind::Instantiates, None, step);
    }

    /// `Markdown`, named where a value goes: a unit struct named as a value is a value of it,
    /// built.
    fn unit(&self, steps: &mut Steps, c: TsNode<'t>) {
        let src = self.src;
        let is = |p: TsNode, field: &str| p.child_by_field_name(field).map(|v| v.id()) == Some(c.id());
        let value = c.parent().map_or(false, |p| match p.kind() {
            "arguments" | "return_expression" | "array_expression" | "tuple_expression" | "reference_expression" => true,
            "let_declaration" | "field_initializer" | "match_arm" => is(p, "value"),
            "assignment_expression" => is(p, "right"),
            "block" => p.named_child(p.named_child_count().saturating_sub(1)).map(|x| x.id()) == Some(c.id()),
            _ => false,
        });
        // a unit struct's name cannot be bound by a pattern: it is the struct wherever it is in scope
        if !value {
            return;
        }
        let Some(r) = self.resolve(&path_parts(Some(c), src)).filter(|r| !r.starts_with("ext:")).and_then(|r| self.class_of(&r)) else {
            return;
        };
        // a struct with fields, or an enum, is no value by its name alone
        if self.store.classes.get(&r).map_or(false, |k| !k.fields.is_empty() || !k.variants.is_empty()) {
            return;
        }
        let step = Step { gives: self.gives(c, Some(&r)), ..Default::default() };
        steps.call(c, &r, EdgeKind::Instantiates, None, step);
    }

    /// An own function handed on as a value (`xs.iter().map(Self::label)`, `thread::spawn(run)`):
    /// called by whoever receives it.
    fn handed_on(&self, steps: &mut Steps, c: TsNode<'t>) {
        for a in positional(c).into_iter().flatten() {
            if !matches!(a.kind(), "identifier" | "scoped_identifier") || self.locals.contains_key(text(a, self.src)) {
                continue;
            }
            let ps = path_parts(Some(a), self.src);
            let handed = match (ps.first().map(String::as_str), &self.cls) {
                (Some("Self"), Some(c)) => ps.get(1).and_then(|m| self.store.method_of(c, m)),
                _ => self.resolve_value(&ps),
            };
            if let Some(t) =
                handed.filter(|t| self.store.nodes.get(t).map_or(false, |n| matches!(n.kind, NodeKind::Function | NodeKind::Method)))
            {
                steps.handed(c, a, &t, EdgeKind::Calls);
            }
        }
    }

    /// The library path a call goes into, where it goes into a library's code Atlas has not read:
    /// a function or a type's by its path (`serde_json.from_str`, `regex.Regex.new`), or a method
    /// of the library type of the value it is called on (`regex.Regex.is_match`).
    fn library_target(&self, c: TsNode<'t>) -> Option<Vec<String>> {
        let f = c.child_by_field_name("function")?;
        if f.kind() == "field_expression" {
            if self.callee(c).is_some() {
                return None;
            }
            let method = text(f.child_by_field_name("field")?, self.src).to_string();
            let made = library_chain(f.child_by_field_name("value")?, self.m, self.src, &self.made_by, &|v| self.declared(v))?;
            return Some(made.ty.into_iter().chain([method]).collect());
        }
        let parts = path_parts(Some(f), self.src);
        let t = self.resolve_value(&parts)?;
        Some(t.strip_prefix("ext:")?.split('.').map(String::from).collect())
    }

    /// A call Atlas reads as computing what it gives, `written` as the code writes it: where what
    /// it gives goes nowhere, or is the last value of a block that gives none, the call is made for
    /// an effect Atlas does not see.
    fn dropped(&self, steps: &mut Steps, c: TsNode<'t>, written: &str) {
        let body = self.def.child_by_field_name("body");
        let last = body.and_then(|b| b.named_child(b.named_child_count().saturating_sub(1)));
        if self.unit && last.map(|l| l.id()) == Some(c.id()) {
            steps.unseen(c, &format!("{written}(..)"));
        } else {
            steps.dropped(c, written);
        }
    }

    /// A library's operation not read: a step where the body keeps what it gives.
    fn library_step(&self, steps: &mut Steps, c: TsNode<'t>, op: &str, recv: Option<TsNode<'t>>) {
        let hands = self.call_hands(c, recv, &steps.kept);
        steps.op(c, op, hands, self.gives(c, None));
    }

    /// What a call is handed: the value it is called on, and the names that stand for values in
    /// its arguments.
    fn call_hands(&self, c: TsNode<'t>, recv: Option<TsNode<'t>>, kept: &HashSet<String>) -> Vec<Held> {
        let parts: Vec<Option<TsNode>> = match c.kind() {
            "token_tree" => std::iter::once(recv).chain(token_values(c).into_iter().map(Some)).collect(),
            _ => vec![recv, c.child_by_field_name("arguments")],
        };
        self.hands(&parts, kept)
    }

    /// A step to one of the program's own functions or types; a method called on a value of a
    /// kind of its own type (a trait's method on a type that implements it) runs on that kind.
    fn own_step(&mut self, steps: &mut Steps, c: TsNode<'t>, t: &str, recv: Option<TsNode<'t>>, class: Option<&str>, via: Option<String>) {
        let kind = if self.store.nodes[t].kind == NodeKind::Class { EdgeKind::Instantiates } else { EdgeKind::Calls };
        let on_receiver = recv.map_or(false, |r| r.kind() == "self");
        let on = class.filter(|k| self.store.nodes[t].cls.as_deref().map_or(false, |b| b != *k)).unwrap_or_default().to_string();
        // a type built by parsing the command line the program is run with (`Cli::parse()`)
        let command_line =
            (kind == EdgeKind::Instantiates && c.kind() == "call_expression").then(|| self.eval.command_line(self.m, c)).flatten();
        let step = Step {
            on,
            on_receiver,
            hands: self.call_hands(c, recv, &steps.kept),
            gives: self.gives(c, Some(t)),
            command_line,
            ..Default::default()
        };
        steps.call(c, t, kind, via.filter(|_| kind == EdgeKind::Instantiates), step);
        if let Some(arg) = self.argument(c, t, recv.is_some()) {
            self.read.args.push(arg);
        }
    }

    /// What a call of the program's function `t` hands each of its parameters: a method called
    /// on a value is handed the rest, one called by its path its receiver first.
    fn argument(&self, c: TsNode<'t>, t: &str, on_value: bool) -> Option<Arg<'t>> {
        let n = self.store.nodes.get(t).filter(|n| matches!(n.kind, NodeKind::Function | NodeKind::Method))?;
        let home = self.modules.get(&n.module)?;
        let def = def_node(home, n)?;
        let skip = usize::from(!on_value && paths::takes_self(def));
        let passed = paths::param_names(def, &home.src)
            .into_iter()
            .zip(positional(c).into_iter().skip(skip))
            .filter_map(|(p, a)| Some((p, a?)))
            .filter(|(p, _)| !p.is_empty())
            .collect();
        Some(Arg {
            caller: self.id.clone(),
            callee: t.to_string(),
            module: self.m.id.clone(),
            scope: Some(self.def),
            params: self.params.clone(),
            passed,
        })
    }

    /// Where an expression of the body is evaluated.
    fn at(&self) -> At<'t, '_> {
        At { m: self.m, scope: Some(self.def), params: &self.params }
    }

    /// A call's touches, each with the path or statement it waits for when it has one.
    fn keep(&mut self, touches: Vec<(Touch, Vec<crate::paths::Built>)>) {
        for (t, built) in touches {
            if built.is_empty() {
                self.read.touches.push(t);
            } else {
                self.read.waiting.push((t, built));
            }
        }
    }

    /// `receiver.method(..)`: the program's method on the class the receiver holds, or a
    /// library's operation, which may touch the world through the value its chain made.
    fn method_call(&mut self, steps: &mut Steps, c: TsNode<'t>, f: TsNode<'t>) {
        let src = self.src;
        let (Some(value), Some(field)) = (f.child_by_field_name("value"), f.child_by_field_name("field")) else { return };
        let method = text(field, src).to_string();
        let receiver = match value.kind() {
            "self" => self.cls.clone(),
            "identifier" => self.locals.get(text(value, src)).cloned(),
            "field_expression" => {
                let v = value.child_by_field_name("value");
                let fld = value.child_by_field_name("field").map(|x| text(x, src)).unwrap_or("");
                match v.map(|v| v.kind()) {
                    Some("self") => self.attr_types.get(fld).cloned(),
                    Some("identifier") => v
                        .and_then(|v| self.locals.get(text(v, src)))
                        .and_then(|t| self.store.classes.get(t))
                        .and_then(|c| c.attr_types.get(fld).cloned()),
                    _ => None,
                }
            }
            // the value a call gives (`Trend::new(settings).save(..)`), or one written another way:
            // the class the body says it is
            _ => self.value_class(value),
        };
        let own = receiver.as_deref().and_then(|r| self.store.method_of(r, &method)).filter(|t| !t.starts_with("ext:"));
        if own.is_none() && self.changes_library_value(value, &method) {
            let hands = self.call_hands(c, Some(value), &steps.kept);
            let changed = self.hands(&[Some(value)], &steps.kept);
            steps.changes(c, &method, hands, self.gives(c, None), changed);
            return;
        }
        match (own, receiver) {
            (Some(t), r) => self.own_step(steps, c, &t, Some(value), r.as_deref(), None),
            // a method not found on a name's type (`to_string`, `clone`) is not known; it builds
            // nothing, and what it does where its value goes nowhere Atlas does not see
            (None, Some(_)) if matches!(value.kind(), "self" | "identifier" | "field_expression") => {
                self.dropped(steps, c, &format!("{}.{method}", text(value, src)));
            }
            // a library's operation; on what a call gives, also one its class does not have, which
            // is the wrapper's the class is given in (`map_or` of the `Option<&Node>` of `ix.node(..)`)
            _ => {
                self.library_step(steps, c, &method, Some(value));
                let touches = self.chain_touches(c, value, &method);
                if touches.is_empty() {
                    self.dropped(steps, c, text(f, src));
                }
                self.keep(touches);
            }
        }
    }

    /// Whether `method` changes the library value it is called on: the value's type, as the body
    /// declares it or else as it makes it, is of a crate whose row of `knowledge::MUTATORS` has
    /// the method. A prelude type (`Vec`, `String`) is std's.
    fn changes_library_value(&self, value: TsNode<'t>, method: &str) -> bool {
        let ty = match self.declared(value) {
            Some(ty) => ty,
            None => match library_chain(value, self.m, self.src, &self.made_by, &|v| self.declared(v)) {
                Some(made) => made.ty,
                None => return false,
            },
        };
        let Some(t) = self.resolve(&ty) else { return false };
        let Some(path) = t.strip_prefix("ext:") else { return false };
        let krate = match path.split_once('.') {
            Some((k, _)) => k,
            None if knowledge::RUST_PRELUDE.contains(&path) => knowledge::RUST_PRELUDE_CRATE,
            None => return false,
        };
        knowledge::mutates(knowledge::Registry::Crates, krate, method)
    }

    /// The library type a value is declared to be: a parameter's (`conn: &Connection`), or a
    /// field's of the body's own class or of a local's class (`self.conn`, `store.conn`).
    fn declared(&self, v: TsNode) -> Option<Vec<String>> {
        let src = self.src;
        let ext = match v.kind() {
            "identifier" => self.foreign.get(text(v, src)).cloned(),
            "field_expression" => {
                let field = text(v.child_by_field_name("field")?, src);
                let owner = v.child_by_field_name("value")?;
                let cls = match owner.kind() {
                    "self" => self.cls.clone(),
                    "identifier" => self.locals.get(text(owner, src)).cloned(),
                    _ => None,
                }?;
                self.store.classes.get(&cls)?.foreign.get(field).cloned()
            }
            _ => None,
        }?;
        Some(ext.strip_prefix("ext:")?.split('.').map(String::from).collect())
    }

    /// What a library's method call touches, through the value its chain made: a value made by a
    /// touch is at that touch's place (`File::open(p)`, `Command::new("git")`), and a call on it
    /// adds a place only when it names its own (`conn.execute("INSERT INTO log ..")`); a value
    /// built without touching anything touches where it is used, at the place its building
    /// named (`OpenOptions::new().append(true).open("log")`).
    /// The path a file is at is the one the call that names it is handed, as built, and a statement
    /// is what the call that runs it is handed.
    fn chain_touches(&self, c: TsNode<'t>, value: TsNode<'t>, method: &str) -> Vec<(Touch, Vec<crate::paths::Built>)> {
        let src = self.src;
        let made = library_chain(value, self.m, src, &self.made_by, &|v| self.declared(v));
        // the outer call of a chain stands for the chain: `client.get(url).send()`
        if made.is_some() && c.parent().map_or(false, |p| p.kind() == "field_expression") {
            return vec![];
        }
        let subtree = || {
            let mut l = place_literals(c, src);
            l.extend(made.as_ref().and_then(|x| x.origin).map(|o| place_literals(o, src)).unwrap_or_default());
            l
        };
        let moved = made.as_ref().and_then(|x| {
            let here: Vec<(String, TsNode)> = x.here.iter().cloned().chain([(method.to_string(), c)]).collect();
            // a builder's calls up to its `build` build, and move nothing (C-BUILDER)
            let (start, end) = knowledge::BUILDER;
            let built = if x.ctor.last().map(String::as_str) == Some(start) {
                x.before.iter().chain(&here).position(|(op, _)| op == end).map_or(usize::MAX, |i| i + 1)
            } else {
                0
            };
            let moves = |i: usize, op: &str| if i < built { None } else { moved_by(&x.ty, op) };
            let n = x.before.len();
            // the move is named by the first call a library knows by name, else the first that moves
            let (kind, op, call) = here
                .iter()
                .enumerate()
                .filter_map(|(i, (op, node))| moves(n + i, op).map(|k| (k, op.clone(), *node)))
                .min_by_key(|(k, _, _)| *k == Crossing::to(World::Disk))?;
            let touched =
                effects::boundary_rust(&x.ctor).is_some() || x.before.iter().enumerate().any(|(i, (op, _))| moves(i, op).is_some());
            let writes = x.before.iter().chain(&here).any(|(o, _)| knowledge::OPENS_TO_WRITE.contains(&o.as_str()));
            let own: Vec<String> = here
                .iter()
                .flat_map(|(_, n)| n.child_by_field_name("arguments").map(|a| place_literals(a, src)).unwrap_or_default())
                .collect();
            Some((if writes && kind.world == World::Disk { Crossing::write(World::Disk) } else { kind }, op, call, touched, own))
        });
        let first = |call: TsNode<'t>| {
            call.child_by_field_name("arguments").and_then(|a| named_children(a).into_iter().find(|x| !x.kind().ends_with("comment")))
        };
        let now = |ts: Vec<Touch>| ts.into_iter().map(|t| (t, vec![])).collect::<Vec<_>>();
        match moved {
            // a statement run on a connection
            Some((kind, op, call, _, _)) if matches!(kind.world, World::Db(_)) && self.statement(kind, first(call)).is_some() => {
                let built = self.statement(kind, first(call)).unwrap_or_default();
                self.touch(c, kind, &op, &[])
                    .into_iter()
                    .take(1)
                    .map(|t| (Touch { artifact: String::new(), dir: None, ..t }, built.clone()))
                    .collect()
            }
            Some((kind, op, _, true, own)) => now(self.touch(c, kind, &op, &own).into_iter().filter(|t| !t.artifact.is_empty()).collect()),
            // a file opened where a builder's value is used, at the path its `open` is handed
            Some((kind, op, call, false, _)) if kind.world == World::Disk && first(call).is_some() => {
                let built = first(call).map(|a| self.eval.of(self.at(), a, 0)).unwrap_or_default();
                self.touch(c, kind, &op, &subtree()).into_iter().map(|t| (t, built.clone())).collect()
            }
            // a request on a client, at the URL its method is handed
            Some((kind, op, call, false, _)) if kind.world == World::Net && first(call).is_some() => {
                let built = crate::paths::request_of(&op, first(call).map(|a| self.eval.of(self.at(), a, 0)).unwrap_or_default());
                self.touch(c, kind, &op, &subtree()).into_iter().map(|t| (t, built.clone())).collect()
            }
            // a method of a path, at the path it is called on: `cli.raw.exists()`
            Some((kind, op, _, false, _)) if kind.world == World::Disk && knowledge::RUST_PATH_READS.contains(&op.as_str()) => {
                let built = self.eval.of(self.at(), value, 0);
                self.touch(c, kind, &op, &subtree()).into_iter().map(|t| (t, built.clone())).collect()
            }
            Some((kind, op, _, false, _)) => now(self.touch(c, kind, &op, &subtree())),
            // a method of a path reads the file the path is
            None if knowledge::RUST_PATH_READS.contains(&method) => {
                let built = self.eval.of(self.at(), value, 0);
                self.touch(c, Crossing::read(World::Disk), method, &subtree()).into_iter().map(|t| (t, built.clone())).collect()
            }
            None => vec![],
        }
    }

    /// The touches of a call that names what it reaches by what it is handed first (the program
    /// `Command::new` runs): the value the function holds there, a constant's as much as a
    /// literal's, one touch for each value it can be; as written when the function does not hold
    /// it whole.
    fn touch_handed(&self, c: TsNode<'t>, tag: Crossing, op: &str, first: Option<TsNode<'t>>) -> Vec<Touch> {
        let lits = literals(c, self.src, STRINGS);
        let values: Vec<String> = match first.filter(|a| literals(*a, self.src, STRINGS).is_empty()) {
            Some(a) => {
                let built = self.eval.of(self.at(), a, 0);
                let whole = built.iter().all(crate::paths::known);
                if whole {
                    built.iter().map(crate::paths::render).collect()
                } else {
                    vec![]
                }
            }
            None => vec![],
        };
        if values.is_empty() {
            return self.touch(c, tag, op, &lits);
        }
        let mut out = vec![];
        for v in values {
            let mut cr = Crosses::read(c, self.src, tag, None, op, &[vec![v.clone()], lits.clone()].concat(), STRINGS, &self.locals);
            cr.what = v;
            out.extend(cr.touches(None));
        }
        out
    }

    /// The statement a call to a database whose statements have a grammar is handed, as built.
    fn statement(&self, kind: Crossing, arg: Option<TsNode<'t>>) -> Option<Vec<crate::paths::Built>> {
        let World::Db(db) = kind.world else { return None };
        arg.map(|a| self.eval.of(self.at(), a, 0)).filter(|b| crate::paths::is_statement(db, b))
    }

    /// `path::to::f(..)`, `Self::f(..)`, `Type::new(..)`: the program's function or type, or a
    /// library's, which may cross into the world.
    fn path_call(&mut self, steps: &mut Steps, c: TsNode<'t>, mut parts: Vec<String>) {
        if parts.is_empty() {
            // a call through a value no path names (`(self.on_done)()`) is one Atlas cannot follow
            if let Some(f) = c.child_by_field_name("function").filter(|_| c.kind() == "call_expression") {
                self.dropped(steps, c, text(f, self.src));
            }
            return;
        }
        let target = match (parts[0].as_str(), &self.cls) {
            ("Self", Some(cls)) => {
                let t = self.store.method_of(cls, parts.get(1).map(String::as_str).unwrap_or("")).unwrap_or_else(|| cls.clone());
                parts[0] = cls.clone();
                Some(t)
            }
            ("Self", None) => None,
            // `Type::new(..)`: a function on a type
            _ => self.resolve_value(&parts).or_else(|| {
                let r = self
                    .resolve(&parts[..parts.len().saturating_sub(1)])
                    .filter(|r| !r.starts_with("ext:") && parts.len() > 1)
                    .and_then(|r| self.class_of(&r))?;
                Some(self.store.method_of(&r, &parts[parts.len() - 1]).unwrap_or(r))
            }),
        };
        match target {
            // `Type::method(..)` with no such method known: keep the name, it is what was called
            Some(t) if !t.starts_with("ext:") => {
                let via = (parts.len() > 1 && parts[0] != "Self").then(|| parts.last().cloned()).flatten();
                self.own_step(steps, c, &t, None, None, via);
            }
            _ => {
                let op = parts.last().cloned().unwrap_or_default();
                self.library_step(steps, c, &op, None);
                let full = expand(self.m, &parts);
                self.read.unresolved.push((full.iter().take(2).cloned().collect::<Vec<_>>().join("."), self.bucket(&parts)));
                let exits = matches!(full.as_slice(), [.., module, f] if knowledge::ends_the_run(module, f));
                self.read.exits |= exits;
                match effects::boundary_rust(&full) {
                    Some(tag) => {
                        let touches = self.crossed(c, tag, &op);
                        self.keep(touches);
                    }
                    // what the tables say nothing of beside what it gives, Atlas does not see
                    None if !exits => self.dropped(steps, c, &parts.join("::")),
                    None => {}
                }
            }
        }
    }

    /// What the path a call not followed starts from is bound to: a name the function binds, a
    /// `use` of a library, one of the program's own items, or nothing the program binds: the
    /// prelude, or a crate the program depends on, where a longer path starts (`std::fs::read`,
    /// `serde_json::to_string`).
    fn bucket(&self, parts: &[String]) -> Bucket {
        let head = parts[0].as_str();
        if self.bound.get_or_init(|| bound_in(self.def, self.src)).contains(head) {
            return Bucket::Local;
        }
        if self.m.bindings.get(head).map_or(false, |b| b.starts_with("ext:")) {
            return Bucket::Library;
        }
        let free = head != "Self" && self.resolve(parts).map_or(false, |r| r.starts_with("ext:"));
        if !free {
            Bucket::Unknown
        } else if knowledge::RUST_PRELUDE.contains(&head) {
            Bucket::Builtin
        } else if parts.len() > 1 {
            Bucket::Library
        } else {
            Bucket::Unknown
        }
    }

    /// What a library's call by path touches: a file call the file at the path it is handed, as
    /// built (one that moves or copies reads the path it is handed first and writes the one it
    /// is handed second: `fs::copy(a, b)`); a database call the tables its statement names; any
    /// other what its literals name.
    fn crossed(&self, c: TsNode<'t>, tag: Crossing, op: &str) -> Vec<(Touch, Vec<crate::paths::Built>)> {
        let args: Vec<TsNode<'t>> = positional(c).into_iter().map_while(|a| a).collect();
        let now = |ts: Vec<Touch>| ts.into_iter().map(|t| (t, vec![])).collect::<Vec<_>>();
        if let Some(built) = self.statement(tag, args.first().copied()) {
            return self
                .touch(c, tag, op, &[])
                .into_iter()
                .take(1)
                .map(|t| (Touch { artifact: String::new(), dir: None, ..t }, built.clone()))
                .collect();
        }
        // a file database is opened on the file it is handed
        if let (World::Db(db), Some(a)) = (tag.world, args.first()) {
            if knowledge::opens_a_file(db, op) {
                let built = self.eval.of(self.at(), *a, 0);
                return self
                    .touch(c, tag, op, &[])
                    .into_iter()
                    .take(1)
                    .map(|t| (Touch { artifact: String::new(), dir: None, ..t }, built.clone()))
                    .collect();
            }
        }
        let files: Vec<(Crossing, TsNode<'t>)> = match crate::body::sides(op, args.len()) {
            Some(sides) if tag.world == World::Disk => sides.into_iter().zip(args).collect(),
            _ if tag.world == World::Disk && !args.is_empty() => vec![(tag, args[0])],
            // a request is its method and its URL, whole where the callers make it, as a path is
            _ if tag.world == World::Net && !args.is_empty() => {
                let built = crate::paths::request_of(op, self.eval.of(self.at(), args[0], 0));
                return self.touch(c, tag, op, &literals(c, self.src, STRINGS)).into_iter().map(|t| (t, built.clone())).collect();
            }
            _ => return now(self.touch_handed(c, tag, op, args.first().copied())),
        };
        let (mut out, two) = (vec![], files.len() > 1);
        for (kind, a) in files {
            let lits = literals(a, self.src, STRINGS);
            let mut cr = Crosses::read(c, self.src, kind, None, op, &lits, STRINGS, &self.locals);
            // a file is named by the argument that is its path, not by what is written into it; a
            // call handed two paths names each by its own
            cr.what = if two {
                lits.first().map(|l| l.trim().to_string()).unwrap_or_else(|| text(a, self.src).to_string())
            } else {
                crate::tree::what_in(a, self.src, STRINGS)
            };
            cr.fixed = crate::tree::head_literal(a, self.src, STRINGS);
            let built = self.eval.of(self.at(), a, 0);
            out.extend(cr.touches(None).into_iter().map(|t| (t, built.clone())));
        }
        out
    }
}

/// The variants of `Option` and `Result` that hold the value they give: where a value is followed,
/// `Some(x)` and `Ok(x)` are `x`.
const HOLDING: &[&str] = &["Some", "Ok"];

// ---------- choices ----------

/// `Option` and `Result` methods whose argument is the other arm: `find(g).unwrap_or_else(|| infer(g))`.
const OR_ELSE: &[&str] = &["unwrap_or_else", "unwrap_or", "or_else", "or", "map_or_else", "map_or"];

/// How many `else if` steps an `if` sits down its chain.
fn else_if_depth(n: TsNode) -> u32 {
    let mut d = 0;
    let mut cur = n;
    while let Some(up) = cur.parent().filter(|p| p.kind() == "else_clause").and_then(|e| e.parent()).filter(|p| p.kind() == "if_expression")
    {
        d += 1;
        cur = up;
    }
    d
}

/// Rust's choices: `if`/`else if`/`else` (with `if let`), `match`, and the `Option`/`Result`
/// methods that run their argument only when the receiver has nothing.
const CHOICES: ChoiceSyntax = ChoiceSyntax {
    chooses: |n, src| match n.kind() {
        "if_expression" | "match_block" | "match_arm" => true,
        "call_expression" => n
            .child_by_field_name("function")
            .filter(|f| f.kind() == "field_expression")
            .and_then(|f| f.child_by_field_name("field"))
            .map_or(false, |m| OR_ELSE.contains(&text(m, src))),
        _ => false,
    },
    continues: |n| {
        if n.kind() == "match_arm" {
            return n.parent();
        }
        let up = n.parent().filter(|p| p.kind() == "else_clause")?.parent()?;
        (n.kind() == "if_expression" && up.kind() == "if_expression").then_some(up)
    },
    part: |c, child, _| {
        let is = |f: &str| c.child_by_field_name(f).map(|x| x.id()) == Some(child.id());
        match c.kind() {
            "if_expression" if is("condition") => Part::Decides,
            "if_expression" if is("consequence") => Part::Arm(else_if_depth(c)),
            "if_expression" if is("alternative") => {
                if named_children(child).iter().any(|x| x.kind() == "if_expression") {
                    Part::Through
                } else {
                    Part::Arm(else_if_depth(c) + 1)
                }
            }
            "match_block" if child.kind() == "match_arm" => Part::Arm(nth_of_kind(c, child)),
            // a pattern and its guard decide which arm runs
            "match_arm" if is("pattern") => Part::Decides,
            "call_expression" if is("function") => Part::Arm(0),
            "call_expression" if is("arguments") => Part::Arm(1),
            _ => Part::Through,
        }
    },
};

/// Where Rust's values go: `let x = v`, `if let x = v`, `x = v`, `x += v`, `for x in v`; `return` and the last
/// expression of the function's block leave; a panic fails.
const USES: UseSyntax = UseSyntax {
    binds: |n| match n.kind() {
        "let_declaration" | "let_condition" => n.child_by_field_name("pattern").zip(n.child_by_field_name("value")),
        "assignment_expression" | "compound_assignment_expr" => n.child_by_field_name("left").zip(n.child_by_field_name("right")),
        "for_expression" => n.child_by_field_name("pattern").zip(n.child_by_field_name("value")),
        _ => None,
    },
    leaves: |n, body| {
        n.kind() == "return_expression"
            || (n.parent().map(|p| p.id()) == Some(body.id())
                && !matches!(n.kind(), "expression_statement" | "let_declaration" | "empty_statement")
                && body.named_child(body.named_child_count().saturating_sub(1)).map(|l| l.id()) == Some(n.id()))
    },
    fails: panics,
    hands: |n, child| match n.kind() {
        "call_expression" => n.child_by_field_name("arguments").map(|a| a.id()) == Some(child.id()),
        "macro_invocation" => child.kind() == "token_tree",
        _ => false,
    },
    splits: |n, child| n.kind() == "match_expression" && n.child_by_field_name("value").map(|v| v.id()) == Some(child.id()),
    names: &["identifier"],
    keeps: &["field_expression", "index_expression"],
};

/// Rust's comparisons: `==` and `!=` against a string or a member of a type (`Mode::Fast`), a
/// `match` on either, and `matches!` on strings.
const COMPARES: CompareSyntax = CompareSyntax {
    kinds: &["binary_expression", "match_expression", "macro_invocation"],
    operands: tested,
    each: |n| vec![n],
    literal: |n, src| matches!(n.kind(), "string_literal" | "raw_string_literal").then(|| unquote(text(n, src))),
    member: |n| (n.kind() == "scoped_identifier").then(|| n.child_by_field_name("path")).flatten().map(|p| (p.start_byte(), p.end_byte())),
    subject: |_| true,
    over: matched,
    copies: |n| (n.kind() == "let_declaration").then(|| n.child_by_field_name("pattern").zip(n.child_by_field_name("value"))).flatten(),
    place,
    binds: USES.binds,
};

/// The value an expression reads in place: a name, a field or an item, through `&`.
fn place(n: TsNode) -> Option<TsNode> {
    match n.kind() {
        "identifier" | "self" | "field_expression" | "index_expression" => Some(n),
        "reference_expression" => n.child_by_field_name("value").and_then(place),
        _ => None,
    }
}

/// Whether the bytes of a qualifier write a type: its last segment is written as Rust writes a
/// type's name, in upper camel case (`Mode`, `Self`, `crate::Mode`), which the compiler's naming
/// lints hold every type to.
fn names_type(q: &str) -> bool {
    q.rsplit("::").next().and_then(|s| s.chars().next()).is_some_and(|c| c.is_uppercase())
}

/// The two operands of a test of equality.
fn tested<'t>(n: TsNode<'t>, src: &[u8]) -> Option<(TsNode<'t>, TsNode<'t>)> {
    let op = n.child_by_field_name("operator").map(|o| text(o, src)).unwrap_or("");
    matches!(op, "==" | "!=").then_some((n.child_by_field_name("left")?, n.child_by_field_name("right")?))
}

/// A `match`, with the block that holds its arms, or a `matches!(kind, "a" | "b")`, decided where
/// it stands: the value and the cases of the patterns (each string, and each path pattern alone or
/// among alternatives: `Mode::Fast | Mode::Slow =>`; a variant taken apart, `Shape::Circle(r)`, is
/// a sum unwrapped, no case).
fn matched<'t>(n: TsNode<'t>, src: &[u8]) -> Option<(String, Vec<TsNode<'t>>, Option<TsNode<'t>>)> {
    fn strings(p: TsNode) -> Vec<TsNode> {
        walk(p, &["string_literal", "raw_string_literal"])
    }
    fn paths(p: TsNode) -> Vec<TsNode> {
        named_children(p)
            .into_iter()
            .flat_map(|c| match c.kind() {
                "scoped_identifier" => vec![c],
                "or_pattern" => paths(c),
                _ => vec![],
            })
            .collect()
    }
    if n.kind() == "match_expression" {
        let (Some(value), Some(block)) = (n.child_by_field_name("value"), n.child_by_field_name("body")) else { return None };
        // a pattern's guard (`x if x.kind() == "a"`) tests another value
        let unguarded = |p: TsNode<'t>| {
            let guard = p.child_by_field_name("condition").map(|g| g.id());
            named_children(p).into_iter().filter(move |c| Some(c.id()) != guard)
        };
        let cases = named_children(block)
            .into_iter()
            .filter_map(|a| a.child_by_field_name("pattern"))
            .flat_map(|p| unguarded(p).flat_map(strings).chain(paths(p)))
            .collect();
        return Some((text(value, src).to_string(), cases, Some(block)));
    }
    if n.kind() != "macro_invocation" || n.child_by_field_name("macro").map(|x| text(x, src)) != Some("matches") {
        return None;
    }
    let tt = named_children(n).into_iter().find(|c| c.kind() == "token_tree")?;
    // a nested `( .. )` is a token tree of its own, so the first comma among the tokens splits
    let kids: Vec<TsNode> = (0..tt.child_count()).filter_map(|i| tt.child(i)).collect();
    let comma = kids.iter().position(|k| k.kind() == ",").filter(|c| *c > 1)?;
    let guard = kids.iter().skip(comma).position(|k| text(*k, src) == "if").map_or(kids.len() - 1, |i| comma + i);
    let (from, to) = (kids[comma].end_byte(), kids[guard].start_byte());
    let cases = strings(tt).into_iter().filter(|s| s.start_byte() >= from && s.end_byte() <= to).collect();
    let subject = String::from_utf8_lossy(&src[kids[1].start_byte()..kids[comma - 1].end_byte()]).to_string();
    Some((subject, cases, None))
}

/// Rust's own types: std, core and alloc, the containers and wrappers, and the associated types
/// a trait names (`Self::Target`, `S::Ok`).
const OWN: OwnTypes = knowledge::RUST_OWN_TYPES;
