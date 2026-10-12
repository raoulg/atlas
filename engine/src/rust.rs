//! `parse: Repo -> Graph` for Rust workspaces.
//!
//! Crates become root modules, files become modules, structs/enums/traits become
//! classes, `impl` blocks attach methods to their type and `impl Trait for` edges,
//! `use` declarations become bindings. Calls resolve through paths, `self`, typed
//! struct fields and typed parameters.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use indexmap::IndexMap;
use tree_sitter::{Node as TsNode, Parser, Tree};

use crate::body::{Crosses, Step, Steps};
use crate::repo::Repo;
use crate::knowledge;
use crate::effects::{self, Crossing};
use crate::model::{EdgeKind, Entry, EntryKind, Held, Node, NodeKind, Param, Returns, Touch, World};
use crate::builder::{foreign_type, ClassInfo, GraphBuilder, SigInfo};
use crate::knowledge::OwnTypes;
use crate::tree::{
    evaluation_order, literals, named_children, nth_of_kind, span, text, unquote, walk,
    ChoiceSyntax, CompareSyntax, Part,
};

struct Crate {
    name: String,
    dir: PathBuf,
}

struct Module {
    id: String,
    krate: String,
    parent: String,
    path: PathBuf,
    src: Vec<u8>,
    tree: Tree,
    bindings: HashMap<String, String>,
    wildcards: Vec<String>,
    symbols: HashMap<String, String>,
    is_root: bool,
    /// For the root of a binary target (`src/main.rs`, `src/bin/x.rs`): the binary's name.
    bin: Option<String>,
}

/// A trait impl found in a module, resolved once bindings exist.
struct Impl<'t> {
    module: String,
    type_parts: Vec<String>,
    trait_parts: Vec<String>,
    body: Option<TsNode<'t>>,
}

// ---------- crates and modules ----------

fn crates(repo: &Repo) -> Result<Vec<Crate>> {
    let manifest = repo.root().join("Cargo.toml");
    let doc: toml::Table = repo.text(&manifest).with_context(|| format!("reading {}", manifest.display()))?.parse().with_context(|| format!("parsing {}", manifest.display()))?;
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
        "generic_type" | "reference_type" | "dynamic_type" | "abstract_type" => path_parts(n.child_by_field_name("type").or_else(|| named_children(n).into_iter().next()), src),
        "generic_function" => path_parts(n.child_by_field_name("function"), src),
        "bracketed_type" => path_parts(named_children(n).into_iter().next(), src),
        _ => vec![],
    }
}

/// All type names mentioned in a type, e.g. `Option<Vec<Node>>` -> `[[Option], [Vec], [Node]]`.
fn type_names(t: Option<TsNode>, src: &[u8]) -> Vec<Vec<String>> {
    let Some(t) = t else { return vec![] };
    let mut out = vec![];
    for n in walk(t, &["type_identifier", "scoped_type_identifier"]) {
        if n.kind() == "type_identifier" && n.parent().map_or(false, |p| p.kind() == "scoped_type_identifier") {
            continue;
        }
        let parts = path_parts(Some(n), src);
        if !parts.is_empty() {
            out.push(parts);
        }
    }
    out
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

/// A signature as data: each parameter by name, type and kind, and what it gives back. `self`,
/// `&self`, `&mut self` and `self: Box<Self>` are the value the method is called on; a `&mut`
/// says the body may change what it is handed.
fn signature(f: TsNode, src: &[u8]) -> (Vec<Param>, Option<Returns>, SigInfo) {
    let mut params = vec![];
    let mut info = SigInfo::default();
    if let Some(pn) = f.child_by_field_name("parameters") {
        for p in named_children(pn) {
            let mutable = |t: TsNode| t.kind() == "reference_type" && named_children(t).iter().any(|c| c.kind() == "mutable_specifier");
            match p.kind() {
                "self_parameter" => params.push(Param { name: "self".into(), receiver: true, changes: text(p, src).contains("mut"), ..Default::default() }),
                "parameter" => {
                    let pattern = p.child_by_field_name("pattern");
                    let ty = p.child_by_field_name("type");
                    params.push(Param {
                        name: pattern.map(|x| text(x, src).trim_start_matches("mut ").to_string()).unwrap_or_default(),
                        ty: ty.map(|t| text(t, src).to_string()).unwrap_or_default(),
                        receiver: pattern.map_or(false, |x| x.kind() == "self"),
                        changes: ty.map_or(false, mutable),
                        rest: false,
                    });
                    info.ptypes.extend(type_names(ty, src));
                }
                _ => {}
            }
        }
    }
    let rt = f.child_by_field_name("return_type");
    info.rtypes = type_names(rt, src);
    (params, rt.map(|r| returns(r, src)), info)
}

/// What a return type gives: a `Result<T, E>` gives `T` when it does not fail, a reference what
/// it refers to, `()` no value, `Self` a value of the type the `impl` is for.
fn returns(rt: TsNode, src: &[u8]) -> Returns {
    let mut t = rt;
    loop {
        let inner = match t.kind() {
            "reference_type" => t.child_by_field_name("type"),
            "generic_type" if t.child_by_field_name("type").map_or(false, |n| text(n, src).rsplit("::").next() == Some("Result")) => {
                t.child_by_field_name("type_arguments").and_then(|a| named_children(a).into_iter().next())
            }
            _ => None,
        };
        match inner {
            Some(i) => t = i,
            None => break,
        }
    }
    let (value, own) = match (t.kind(), text(t, src)) {
        ("unit_type", _) => (String::new(), false),
        (_, "Self") => (String::new(), true),
        (_, v) => (v.to_string(), false),
    };
    Returns { written: text(rt, src).to_string(), value, own }
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

/// Parse a Cargo workspace or crate into a `GraphBuilder`; returns the store and the workspace name.
pub fn parse(repo: &Repo) -> Result<(GraphBuilder, String)> {
    let mut parser = new_parser()?;
    let mut modules: IndexMap<String, Module> = IndexMap::new();
    let mut store = GraphBuilder::default();
    let crate_list = crates(repo)?;
    // a single crate is named after itself; a workspace after its directory
    let root_name = if crate_list.len() == 1 { crate_list[0].name.clone() } else { repo.name().unwrap_or_else(|| "workspace".into()) };
    let crate_ids: HashMap<String, String> = crate_list.iter().map(|c| (c.name.replace('-', "_"), c.name.clone())).collect();
    for c in &crate_list {
        // a directory under the crate with a `Cargo.toml` of its own is another package
        let other = |p: &Path| p.ancestors().skip(1).take_while(|a| *a != c.dir.as_path() && a.starts_with(&c.dir)).any(|a| repo.is_file(a.join("Cargo.toml")));
        let mut files: Vec<PathBuf> = repo
            .files_under(&c.dir)
            .into_iter()
            .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("rs") && !other(p))
            .filter(|p| {
                let rel = p.strip_prefix(&c.dir).unwrap_or(p);
                matches!(rel.components().next().map(|x| x.as_os_str().to_string_lossy().to_string()).as_deref(), Some("src") | Some("tests") | Some("benches") | Some("examples"))
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
                ..Default::default()
            });
            modules.insert(
                mid.clone(),
                Module { id: mid, krate: c.name.clone(), parent, path: p, src, tree, bindings: HashMap::new(), wildcards: vec![], symbols: HashMap::new(), is_root, bin },
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
    collect_impls(&modules, &mut store, &crate_ids);
    link(&modules, &mut store, &crate_ids);
    Ok((store, root_name))
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
                let entry = if attrs.iter().any(|a| a == "test") {
                    Some(Entry { kind: EntryKind::Test, label: name.clone() })
                } else if let Some(bin) = m.bin.as_ref().filter(|_| name == "main") {
                    Some(Entry { kind: EntryKind::Cli, label: bin.clone() })
                } else if exported(m, item, src) {
                    Some(Entry { kind: EntryKind::Export, label: name.clone() })
                } else {
                    None
                };
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
                let mut methods = vec![];
                if let Some(body) = item.child_by_field_name("body") {
                    if item.kind() == "trait_item" {
                        for f in named_children(body).into_iter().filter(|f| matches!(f.kind(), "function_signature_item" | "function_item")) {
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
                            variants.extend(type_names(v.child_by_field_name("body"), src));
                        }
                    }
                }
                store.classes.insert(nid.clone(), ClassInfo { fields, variants, ..Default::default() });
                let entry = exported(m, item, src).then(|| Entry { kind: EntryKind::Export, label: name.clone() });
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
    id
}

fn collect_uses(m: &mut Module, crate_ids: &HashMap<String, String>, module_ids: &HashSet<String>, store: &mut GraphBuilder) {
    let mut out = vec![];
    let mut wild = vec![];
    for u in walk(m.tree.root_node(), &["use_declaration"]) {
        collect_use(u, &[], &m.src, &mut out, &mut wild);
    }
    let mut bindings = vec![];
    let mut edges = vec![];
    let is_module = |id: &str| module_ids.contains(id);
    for (alias, parts) in out {
        let id = path_to_id(m, &parts, crate_ids, &is_module);
        if !id.starts_with("ext:") {
            edges.push(id.clone());
        }
        bindings.push((alias, id));
    }
    for w in wild {
        let id = path_to_id(m, &w, crate_ids, &is_module);
        if !id.starts_with("ext:") {
            edges.push(id.clone());
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

fn resolve(m: &Module, parts: &[String], modules: &IndexMap<String, Module>, nodes: &IndexMap<String, Node>, crate_ids: &HashMap<String, String>) -> Option<String> {
    let head = parts.first()?;
    let is_module = |id: &str| nodes.get(id).map_or(false, |n| n.kind == NodeKind::Module);
    let (mut target, rest): (String, &[String]) = if matches!(head.as_str(), "crate" | "self" | "super") || crate_ids.contains_key(head) {
        (path_to_id(m, parts, crate_ids, &is_module), &[])
    } else if let Some(s) = m.symbols.get(head) {
        (s.clone(), &parts[1..])
    } else if let Some(b) = m.bindings.get(head) {
        (b.clone(), &parts[1..])
    } else if let Some(w) = m.wildcards.iter().find(|w| nodes.contains_key(&format!("{w}.{head}"))) {
        (format!("{w}.{head}"), &parts[1..])
    } else if nodes.get(&format!("{}.{head}", m.id)).map_or(false, |n| n.kind == NodeKind::Module) {
        // a child module declared with `mod x;`, addressed as `x::f`
        (format!("{}.{head}", m.id), &parts[1..])
    } else {
        return Some(format!("ext:{}", parts.join(".")));
    };
    for p in rest {
        target.push('.');
        target.push_str(p);
    }
    if target.starts_with("ext:") || nodes.contains_key(&target) {
        return Some(target);
    }
    // a re-export through another module's `pub use`
    let (modid, nm) = target.rsplit_once('.')?;
    let re = modules.get(modid)?.bindings.get(nm)?;
    nodes.contains_key(re).then(|| re.clone())
}


// ---------- pass 1c: impl blocks ----------

fn collect_impls(modules: &IndexMap<String, Module>, store: &mut GraphBuilder, crate_ids: &HashMap<String, String>) {
    let mut impls: Vec<Impl> = vec![];
    for m in modules.values() {
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
            let tr = resolve(m, &im.trait_parts, modules, &store.nodes, crate_ids).unwrap_or_else(|| format!("ext:{}", im.trait_parts.join(".")));
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
                entry: attrs.iter().any(|a| a == "test").then(|| Entry { kind: EntryKind::Test, label: mname.clone() }),
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

fn def_node<'t>(m: &'t Module, n: &Node) -> Option<TsNode<'t>> {
    if n.kind == NodeKind::Module {
        return Some(m.tree.root_node());
    }
    let (start, _) = n.span?;
    walk(m.tree.root_node(), &["function_item", "function_signature_item", "struct_item", "enum_item", "trait_item"])
        .into_iter()
        .find(|d| d.start_position().row as u32 + 1 == start && d.child_by_field_name("name").map_or(false, |x| text(x, &m.src) == n.name))
}

fn link(modules: &IndexMap<String, Module>, store: &mut GraphBuilder, crate_ids: &HashMap<String, String>) {
    let ids: Vec<String> = store.nodes.keys().cloned().collect();
    for id in &ids {
        let (kind, module) = (store.nodes[id].kind, store.nodes[id].module.clone());
        if kind != NodeKind::Class {
            continue;
        }
        let m = &modules[&module];
        let (fields, variants) = store.classes.get(id).map(|c| (c.fields.clone(), c.variants.clone())).unwrap_or_default();
        for (fname, types) in fields {
            for t in types {
                if let Some(r) = resolve(m, &t, modules, &store.nodes, crate_ids).filter(|r| !r.starts_with("ext:")) {
                    if store.nodes.get(&r).map_or(false, |x| x.kind == NodeKind::Class) {
                        store.classes.entry(id.clone()).or_default().attr_types.insert(fname.clone(), r.clone());
                        store.add_edge(id, &r, EdgeKind::Has, Some(fname.clone()));
                    }
                }
            }
        }
        let mut seen: Vec<String> = vec![];
        for t in variants {
            let Some(r) = resolve(m, &t, modules, &store.nodes, crate_ids) else { continue };
            let known = if r.starts_with("ext:") { foreign_type(&r, &OWN) } else { store.nodes.get(&r).map_or(false, |x| x.kind == NodeKind::Class) };
            if known && r != *id && !seen.contains(&r) {
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
    }
    // every signature before any body: a body's locals take their types from what the functions
    // it calls give back, wherever those are defined
    for id in &ids {
        if matches!(store.nodes[id].kind, NodeKind::Function | NodeKind::Method) {
            link_calls(modules, store, crate_ids, id);
        }
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

/// The name a call's value is kept by: `let x = f(..)?`, `let x = f(..).unwrap()`, `x = f(..)`.
fn kept_as(c: TsNode, src: &[u8]) -> Option<String> {
    let mut n = c;
    loop {
        let p = n.parent()?;
        match p.kind() {
            "try_expression" | "await_expression" | "reference_expression" | "parenthesized_expression" => n = p,
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
/// (`c` from `let c = reqwest::Client::new()`).
fn library_chain<'t>(mut v: TsNode<'t>, m: &Module, src: &[u8], made_by: &HashMap<String, TsNode<'t>>) -> Option<Made<'t>> {
    let (mut before, mut here, mut origin) = (vec![], vec![], None);
    let mut locals = 0;
    loop {
        match v.kind() {
            "try_expression" | "await_expression" | "reference_expression" | "parenthesized_expression" => v = named_children(v).into_iter().next()?,
            "identifier" if locals < 8 => {
                v = *made_by.get(text(v, src))?;
                origin.get_or_insert(v);
                locals += 1;
            }
            "call_expression" => {
                let f = v.child_by_field_name("function")?;
                if f.kind() == "field_expression" {
                    let call = (text(f.child_by_field_name("field")?, src).to_string(), v);
                    if locals == 0 { here.push(call) } else { before.push(call) }
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

fn link_calls(modules: &IndexMap<String, Module>, store: &mut GraphBuilder, crate_ids: &HashMap<String, String>, id: &str) {
    let n = store.nodes[id].clone();
    let m = &modules[&n.module];
    let Some(d) = def_node(m, &n) else { return };
    let Some(body) = d.child_by_field_name("body") else { return };
    let mut steps = Steps::new(body, &m.src, &CHOICES);
    let read = {
        let mut b = Body::new(modules, store, crate_ids, m, &n, d, body);
        for c in evaluation_order(walk(body, &["call_expression", "struct_expression"])) {
            b.call(&mut steps, c);
        }
        b.read
    };
    let compares = crate::tree::compares(body, &m.src, steps.choices(), &COMPARES, &CHOICES);
    steps.finish(store, id);
    if let Some(node) = store.nodes.get_mut(id) {
        node.compares = compares;
    }
    for (t, input) in read.mutates {
        store.add_edge(id, &t, EdgeKind::Mutates, Some(input));
    }
    for u in read.unresolved {
        *store.unresolved.entry(u).or_insert(0) += 1;
    }
    for t in read.touches {
        store.touch(id, t);
    }
}

/// What reading a body finds besides its steps: what it touches, the calls Atlas could not
/// follow, and the inputs it changes in place.
#[derive(Default)]
struct Read {
    touches: Vec<Touch>,
    unresolved: Vec<String>,
    mutates: Vec<(String, String)>,
}

/// A function's body as Rust reads it: the program around it, and the classes its names hold.
struct Body<'a, 't> {
    modules: &'a IndexMap<String, Module>,
    store: &'a GraphBuilder,
    crate_ids: &'a HashMap<String, String>,
    m: &'t Module,
    src: &'t [u8],
    cls: Option<String>,
    /// The classes the body's names hold: its typed parameters, and locals assigned from a
    /// constructor, a struct literal or a function with a typed return.
    locals: HashMap<String, String>,
    /// The locals a call made, by the call: `let c = reqwest::Client::new()`.
    made_by: HashMap<String, TsNode<'t>>,
    /// The classes the fields of the body's own class hold.
    attr_types: std::collections::BTreeMap<String, String>,
    read: Read,
}

const STRINGS: &[&str] = &["string_literal", "raw_string_literal"];

impl<'a, 't> Body<'a, 't> {
    fn new(modules: &'a IndexMap<String, Module>, store: &'a GraphBuilder, crate_ids: &'a HashMap<String, String>, m: &'t Module, n: &Node, d: TsNode<'t>, body: TsNode<'t>) -> Self {
        let cls = n.cls.clone();
        let attr_types = cls.as_ref().and_then(|c| store.classes.get(c)).map(|c| c.attr_types.clone()).unwrap_or_default();
        let mut b = Body { modules, store, crate_ids, m, src: &m.src, cls, locals: HashMap::new(), made_by: HashMap::new(), attr_types, read: Read::default() };
        b.parameters(d);
        b.lets(body);
        // a macro that prints shows the human something
        for mac in walk(body, &["macro_invocation"]) {
            let name = mac.child_by_field_name("macro").map(|x| text(x, b.src)).unwrap_or("");
            if effects::screen_macro(name) {
                b.read.touches.push(Touch { op: name.into(), line: mac.start_position().row as u32 + 1, ..Touch::new(World::Screen, None) });
            }
        }
        b
    }

    fn resolve(&self, parts: &[String]) -> Option<String> {
        resolve(self.m, parts, self.modules, &self.store.nodes, self.crate_ids)
    }

    fn class_of(&self, id: &str) -> Option<String> {
        self.store.nodes.get(id).filter(|x| x.kind == NodeKind::Class).map(|_| id.to_string())
    }

    /// The class a function gives back, by its signature.
    fn returned_class(&self, id: &str) -> Option<String> {
        self.store.edges.iter().find(|e| e.src == id && e.kind == EdgeKind::Returns && self.class_of(&e.dst).is_some()).map(|e| e.dst.clone())
    }

    /// Typed parameters give receivers a type (`store.add_edge(..)` with `store: &mut
    /// GraphBuilder`), and `&mut T` and `&mut self` hand the value back changed: `T -> T`.
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
            let by_mut_ref = p.child_by_field_name("type").map_or(false, |t| text(t, src).starts_with("&mut"));
            let class = type_names(p.child_by_field_name("type"), src).iter().find_map(|t| self.resolve(t).filter(|r| !r.starts_with("ext:")).and_then(|r| self.class_of(&r)));
            if let Some(r) = class {
                if by_mut_ref {
                    self.read.mutates.push((r.clone(), pname.clone()));
                }
                self.locals.insert(pname, r);
            }
        }
    }

    /// Locals assigned from a constructor, a struct literal, or a function with a typed return;
    /// `.unwrap()`, `.expect(..)`, `.context(..)` hand on the type of the value they are called on.
    fn lets(&mut self, body: TsNode<'t>) {
        let src = self.src;
        for l in walk(body, &["let_declaration"]) {
            let Some(pat) = l.child_by_field_name("pattern").filter(|p| matches!(p.kind(), "identifier" | "mut_pattern")) else { continue };
            let name = text(pat, src).trim_start_matches("mut ").to_string();
            let mut value = l.child_by_field_name("value");
            while let Some(v) = value {
                value = match v.kind() {
                    "try_expression" | "await_expression" | "unary_expression" | "reference_expression" | "parenthesized_expression" => named_children(v).into_iter().next(),
                    "call_expression" => match v.child_by_field_name("function") {
                        Some(f) if f.kind() == "field_expression" && f.child_by_field_name("field").map_or(false, |x| knowledge::hands_on(text(x, src))) => f.child_by_field_name("value"),
                        _ => break,
                    },
                    _ => break,
                };
            }
            if let Some(made) = l.child_by_field_name("value") {
                self.made_by.insert(name.clone(), made);
            }
            if let Some(t) = value.and_then(|v| self.value_class(v)) {
                self.locals.insert(name, t);
            }
        }
    }

    /// The class a value is of, where the body says: a name it holds, a struct literal, a method
    /// by what its signature gives back on the class of its receiver, a function by its return.
    fn value_class(&self, v: TsNode) -> Option<String> {
        let src = self.src;
        match v.kind() {
            "identifier" => self.locals.get(text(v, src)).cloned(),
            "struct_expression" => self.resolve(&path_parts(v.child_by_field_name("name"), src)).and_then(|r| self.class_of(&r)),
            "call_expression" if v.child_by_field_name("function").map_or(false, |f| f.kind() == "field_expression") => {
                let f = v.child_by_field_name("function")?;
                let on = match f.child_by_field_name("value") {
                    Some(r) if r.kind() == "self" => self.cls.clone(),
                    Some(r) if r.kind() == "identifier" => self.locals.get(text(r, src)).cloned(),
                    _ => None,
                };
                let name = f.child_by_field_name("field").map(|x| text(x, src)).unwrap_or("");
                on.and_then(|t| self.store.method_of(&t, name)).and_then(|t| self.returned_class(&t))
            }
            "call_expression" => {
                let parts = path_parts(v.child_by_field_name("function"), src);
                if parts.is_empty() {
                    return None;
                }
                self.resolve(&parts).as_deref().and_then(|f| self.returned_class(f)).or_else(|| self.resolve(&parts[..parts.len() - 1]).and_then(|r| self.class_of(&r)))
            }
            _ => None,
        }
    }

    /// What a call is handed: the value it is called on, its arguments, a literal's fields.
    fn hands(&self, parts: &[Option<TsNode>], kept: &HashSet<String>) -> Vec<Held> {
        let ty_of = |name: &str| self.locals.get(name).cloned();
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

    /// What a call gives back, by the name the body keeps it by.
    fn gives(&self, c: TsNode, to: Option<&str>) -> Vec<Held> {
        kept_as(c, self.src).map(|name| Held::new(self.locals.get(&name).cloned().or_else(|| to.and_then(|t| self.returned_class(t))).unwrap_or_default(), name)).into_iter().collect()
    }

    /// A boundary call's touches, refined by the literals that name its place.
    fn touch(&self, c: TsNode, kind: Crossing, op: &str, lits: &[String]) -> Vec<Touch> {
        Crosses::read(c, self.src, kind, None, op, lits, STRINGS, &self.locals).touches(None)
    }

    /// One call of the body, as the step it is: a struct literal, a method on a value, or a path.
    fn call(&mut self, steps: &mut Steps, c: TsNode<'t>) {
        if c.kind() == "struct_expression" {
            return self.literal(steps, c);
        }
        let Some(f) = c.child_by_field_name("function") else { return };
        self.handed_on(steps, c);
        if f.kind() == "field_expression" {
            self.method_call(steps, c, f);
        } else {
            self.path_call(steps, c, f);
        }
    }

    /// `Type { .. }`: a value of one of the program's types, built from its fields.
    fn literal(&self, steps: &mut Steps, c: TsNode<'t>) {
        let Some(r) = self.resolve(&path_parts(c.child_by_field_name("name"), self.src)).filter(|r| !r.starts_with("ext:")).and_then(|r| self.class_of(&r)) else { return };
        let step = Step { hands: self.hands(&[c.child_by_field_name("body")], &steps.kept), gives: self.gives(c, Some(&r)), ..Default::default() };
        steps.call(c, &r, EdgeKind::Instantiates, None, step);
    }

    /// An own function handed on as a value (`xs.iter().map(Self::label)`, `thread::spawn(run)`):
    /// called by whoever receives it.
    fn handed_on(&self, steps: &mut Steps, c: TsNode<'t>) {
        for a in c.child_by_field_name("arguments").map(named_children).unwrap_or_default() {
            if !matches!(a.kind(), "identifier" | "scoped_identifier") || self.locals.contains_key(text(a, self.src)) {
                continue;
            }
            let ps = path_parts(Some(a), self.src);
            let handed = match (ps.first().map(String::as_str), &self.cls) {
                (Some("Self"), Some(c)) => ps.get(1).and_then(|m| self.store.method_of(c, m)),
                _ => self.resolve(&ps),
            };
            if let Some(t) = handed.filter(|t| self.store.nodes.get(t).map_or(false, |n| matches!(n.kind, NodeKind::Function | NodeKind::Method))) {
                steps.handed(c, a, &t, EdgeKind::Calls);
            }
        }
    }

    /// A library's operation not read: a step where the body keeps what it gives.
    fn library_step(&self, steps: &mut Steps, c: TsNode<'t>, op: &str, recv: Option<TsNode<'t>>) {
        let hands = self.hands(&[recv, c.child_by_field_name("arguments")], &steps.kept);
        steps.op(c, op, hands, self.gives(c, None));
    }

    /// A step to one of the program's own functions or types.
    fn own_step(&self, steps: &mut Steps, c: TsNode<'t>, t: &str, recv: Option<TsNode<'t>>, via: Option<String>) {
        let kind = if self.store.nodes[t].kind == NodeKind::Class { EdgeKind::Instantiates } else { EdgeKind::Calls };
        let on_receiver = recv.map_or(false, |r| r.kind() == "self");
        let step = Step { on_receiver, hands: self.hands(&[recv, c.child_by_field_name("arguments")], &steps.kept), gives: self.gives(c, Some(t)), ..Default::default() };
        steps.call(c, t, kind, via.filter(|_| kind == EdgeKind::Instantiates), step);
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
                    Some("identifier") => v.and_then(|v| self.locals.get(text(v, src))).and_then(|t| self.store.classes.get(t)).and_then(|c| c.attr_types.get(fld).cloned()),
                    _ => None,
                }
            }
            _ => None,
        };
        match receiver {
            // a method not found on the type (`to_string`, `clone`) is not known; it builds nothing
            Some(r) => {
                if let Some(t) = self.store.method_of(&r, &method).filter(|t| !t.starts_with("ext:")) {
                    self.own_step(steps, c, &t, Some(value), None);
                }
            }
            None => {
                self.library_step(steps, c, &method, Some(value));
                let touches = self.chain_touches(c, value, &method);
                self.read.touches.extend(touches);
            }
        }
    }

    /// What a library's method call touches, through the value its chain made: a value made by a
    /// touch is at that touch's place (`File::open(p)`, `Command::new("git")`), and a call on it
    /// adds a place only when it names its own (`conn.execute("INSERT INTO log ..")`); a value
    /// built without touching anything touches where it is used, at the place its building
    /// named (`OpenOptions::new().append(true).open("log")`).
    fn chain_touches(&self, c: TsNode<'t>, value: TsNode<'t>, method: &str) -> Vec<Touch> {
        let src = self.src;
        let made = library_chain(value, self.m, src, &self.made_by);
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
            let built = if x.ctor.last().map(String::as_str) == Some(start) { x.before.iter().chain(&here).position(|(op, _)| op == end).map_or(usize::MAX, |i| i + 1) } else { 0 };
            let moves = |i: usize, op: &str| if i < built { None } else { moved_by(&x.ty, op) };
            let n = x.before.len();
            // the move is named by the first call a library knows by name, else the first that moves
            let (kind, op) = here.iter().enumerate().filter_map(|(i, (op, _))| moves(n + i, op).map(|k| (k, op.clone()))).min_by_key(|(k, _)| *k == Crossing::to(World::Disk))?;
            let touched = effects::boundary_rust(&x.ctor).is_some() || x.before.iter().enumerate().any(|(i, (op, _))| moves(i, op).is_some());
            let writes = x.before.iter().chain(&here).any(|(o, _)| knowledge::OPENS_TO_WRITE.contains(&o.as_str()));
            let own: Vec<String> = here.iter().flat_map(|(_, n)| n.child_by_field_name("arguments").map(|a| place_literals(a, src)).unwrap_or_default()).collect();
            Some((if writes && kind.world == World::Disk { Crossing::write(World::Disk) } else { kind }, op, touched, own))
        });
        match moved {
            Some((kind, op, true, own)) => self.touch(c, kind, &op, &own).into_iter().filter(|t| !t.artifact.is_empty()).collect(),
            Some((kind, op, false, _)) => self.touch(c, kind, &op, &subtree()),
            None if knowledge::RUST_PATH_READS.contains(&method) => self.touch(c, Crossing::read(World::Disk), method, &subtree()),
            None => vec![],
        }
    }

    /// `path::to::f(..)`, `Self::f(..)`, `Type::new(..)`: the program's function or type, or a
    /// library's, which may cross into the world.
    fn path_call(&mut self, steps: &mut Steps, c: TsNode<'t>, f: TsNode<'t>) {
        let mut parts = path_parts(Some(f), self.src);
        if parts.is_empty() {
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
            _ => self.resolve(&parts).or_else(|| {
                let r = self.resolve(&parts[..parts.len().saturating_sub(1)]).filter(|r| !r.starts_with("ext:") && parts.len() > 1).and_then(|r| self.class_of(&r))?;
                Some(self.store.method_of(&r, &parts[parts.len() - 1]).unwrap_or(r))
            }),
        };
        match target {
            // `Type::method(..)` with no such method known: keep the name, it is what was called
            Some(t) if !t.starts_with("ext:") => {
                let via = (parts.len() > 1 && parts[0] != "Self").then(|| parts.last().cloned()).flatten();
                self.own_step(steps, c, &t, None, via);
            }
            _ => {
                let op = parts.last().cloned().unwrap_or_default();
                self.library_step(steps, c, &op, None);
                let full = expand(self.m, &parts);
                self.read.unresolved.push(full.iter().take(2).cloned().collect::<Vec<_>>().join("."));
                if let Some(tag) = effects::boundary_rust(&full) {
                    let touches = self.crossed(c, tag, &op);
                    self.read.touches.extend(touches);
                }
            }
        }
    }

    /// What a library's call by path touches: one that moves or copies a file reads the path it
    /// is handed first and writes the one it is handed second (`fs::copy(a, b)`); any other
    /// touches what its literals name.
    fn crossed(&self, c: TsNode<'t>, tag: Crossing, op: &str) -> Vec<Touch> {
        let args = c.child_by_field_name("arguments").map(named_children).unwrap_or_default();
        let Some(sides) = crate::body::sides(op, args.len()).filter(|_| tag.world == World::Disk) else {
            return self.touch(c, tag, op, &literals(c, self.src, STRINGS));
        };
        let mut out = vec![];
        for (kind, a) in sides.into_iter().zip(args) {
            let lits = literals(a, self.src, STRINGS);
            let mut cr = Crosses::read(c, self.src, kind, None, op, &lits, STRINGS, &self.locals);
            cr.what = lits.first().map(|l| l.trim().to_string()).unwrap_or_else(|| text(a, self.src).to_string());
            cr.fixed = crate::tree::head_literal(a, self.src, STRINGS);
            out.extend(cr.touches(None));
        }
        out
    }
}

// ---------- choices ----------

/// `Option` and `Result` methods whose argument is the other arm: `find(g).unwrap_or_else(|| infer(g))`.
const OR_ELSE: &[&str] = &["unwrap_or_else", "unwrap_or", "or_else", "or", "map_or_else", "map_or"];

/// How many `else if` steps an `if` sits down its chain.
fn else_if_depth(n: TsNode) -> u32 {
    let mut d = 0;
    let mut cur = n;
    while let Some(up) = cur.parent().filter(|p| p.kind() == "else_clause").and_then(|e| e.parent()).filter(|p| p.kind() == "if_expression") {
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

/// Rust's comparisons: `==` and `!=` against a string, a `match` on `&str`, and `matches!`.
const COMPARES: CompareSyntax = CompareSyntax {
    kinds: &["binary_expression", "match_expression", "macro_invocation"],
    operands: tested,
    cases: |n, src| matches!(n.kind(), "string_literal" | "raw_string_literal").then(|| vec![unquote(text(n, src))]),
    subject: |_| true,
    over: matched,
};

/// The two operands of a test of equality.
fn tested<'t>(n: TsNode<'t>, src: &[u8]) -> Option<(TsNode<'t>, TsNode<'t>)> {
    let op = n.child_by_field_name("operator").map(|o| text(o, src)).unwrap_or("");
    matches!(op, "==" | "!=").then_some((n.child_by_field_name("left")?, n.child_by_field_name("right")?))
}

/// A `match`, with the block that holds its arms, or a `matches!(kind, "a" | "b")`, decided where
/// it stands: the value and the strings of the patterns.
fn matched<'t>(n: TsNode<'t>, src: &[u8]) -> Option<(String, Vec<String>, Option<TsNode<'t>>)> {
    fn strings(p: TsNode) -> Vec<TsNode> {
        walk(p, &["string_literal", "raw_string_literal"])
    }
    if n.kind() == "match_expression" {
        let (Some(value), Some(block)) = (n.child_by_field_name("value"), n.child_by_field_name("body")) else { return None };
        let cases = named_children(block).into_iter().filter_map(|a| a.child_by_field_name("pattern")).flat_map(strings).map(|s| unquote(text(s, src))).collect();
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
    let cases = strings(tt).into_iter().filter(|s| s.start_byte() >= from && s.end_byte() <= to).map(|s| unquote(text(s, src))).collect();
    let subject = String::from_utf8_lossy(&src[kids[1].start_byte()..kids[comma - 1].end_byte()]).to_string();
    Some((subject, cases, None))
}

/// Rust's own types: std, core and alloc, the containers and wrappers, and the associated types
/// a trait names (`Self::Target`, `S::Ok`).
const OWN: OwnTypes = knowledge::RUST_OWN_TYPES;
