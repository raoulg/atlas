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
use walkdir::WalkDir;

use crate::effects;
use crate::model::{Compare, EdgeKind, Entry, EntryKind, Node, NodeKind, Touch};
use crate::builder::{ClassInfo, GraphBuilder, SigInfo};
use crate::tree::{
    arms_of, decides, evaluation_order, literals, named_children, nth_of_kind, number_choices, payload_of, span, starts_at_literal, text, unquote, walk, what_of,
    ChoiceSyntax, Part,
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

fn crates(repo: &Path) -> Result<Vec<Crate>> {
    let manifest = repo.join("Cargo.toml");
    let doc: toml::Table = std::fs::read_to_string(&manifest)?.parse().with_context(|| format!("parsing {}", manifest.display()))?;
    let mut out = vec![];
    let mut dirs: Vec<PathBuf> = vec![];
    if let Some(members) = doc.get("workspace").and_then(|w| w.get("members")).and_then(|m| m.as_array()) {
        for m in members.iter().filter_map(|m| m.as_str()) {
            if let Some(prefix) = m.strip_suffix("/*") {
                if let Ok(rd) = std::fs::read_dir(repo.join(prefix)) {
                    let mut ds: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.join("Cargo.toml").exists()).collect();
                    ds.sort();
                    dirs.extend(ds);
                }
            } else {
                dirs.push(repo.join(m));
            }
        }
    }
    if doc.get("package").is_some() {
        dirs.push(repo.to_path_buf());
    }
    for dir in dirs {
        let Ok(t) = std::fs::read_to_string(dir.join("Cargo.toml")) else { continue };
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
fn module_id(krate: &str, dir: &Path, path: &Path) -> Option<(String, Option<String>)> {
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
                if !dir.join("src/lib.rs").exists() {
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

fn signature(f: TsNode, src: &[u8]) -> (Vec<String>, String, SigInfo) {
    let mut params = vec![];
    let mut info = SigInfo::default();
    if let Some(pn) = f.child_by_field_name("parameters") {
        for p in named_children(pn) {
            if p.kind() == "attribute_item" {
                continue;
            }
            params.push(text(p, src).to_string());
            if p.kind() == "parameter" {
                info.ptypes.extend(type_names(p.child_by_field_name("type"), src));
            }
        }
    }
    let rt = f.child_by_field_name("return_type");
    info.rtypes = type_names(rt, src);
    (params, rt.map(|r| text(r, src).to_string()).unwrap_or_default(), info)
}

/// A method whose first parameter is the value it is called on: `&self`, `&'a mut self`,
/// `self: Box<Self>`.
fn takes_self(params: &[String]) -> bool {
    params.first().map_or(false, |p| p.split(':').next().unwrap_or("").trim_start_matches('&').split_whitespace().last() == Some("self"))
}

fn fn_name(f: TsNode, src: &[u8]) -> String {
    f.child_by_field_name("name").map(|n| text(n, src)).unwrap_or("").to_string()
}

// ---------- loading ----------

/// Parse a Cargo workspace or crate into a `GraphBuilder`; returns the store and the workspace name.
pub fn parse(repo: &Path) -> Result<(GraphBuilder, String)> {
    let mut parser = new_parser()?;
    let mut modules: IndexMap<String, Module> = IndexMap::new();
    let mut store = GraphBuilder::default();
    let crate_list = crates(repo)?;
    // a single crate is named after itself; a workspace after its directory
    let root_name = if crate_list.len() == 1 {
        crate_list[0].name.clone()
    } else {
        std::fs::canonicalize(repo)
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_else(|| "workspace".into())
    };
    let crate_ids: HashMap<String, String> = crate_list.iter().map(|c| (c.name.replace('-', "_"), c.name.clone())).collect();
    for c in &crate_list {
        let mut files: Vec<PathBuf> = WalkDir::new(&c.dir)
            .into_iter()
            // the walk's own root is read whatever it is called, `.` included; a directory with a
            // `Cargo.toml` of its own is another package
            .filter_entry(|e| {
                let n = e.file_name().to_str().unwrap_or("");
                e.depth() == 0 || !(n.starts_with('.') || n == "target" || n == "node_modules" || e.path().join("Cargo.toml").is_file())
            })
            .flatten()
            .filter(|e| e.file_type().is_file() && e.path().extension().and_then(|x| x.to_str()) == Some("rs"))
            .map(|e| e.path().to_path_buf())
            .filter(|p| {
                let rel = p.strip_prefix(&c.dir).unwrap_or(p);
                matches!(rel.components().next().map(|x| x.as_os_str().to_string_lossy().to_string()).as_deref(), Some("src") | Some("tests") | Some("benches") | Some("examples"))
            })
            .collect();
        files.sort();
        for p in files {
            let Some((mid, bin)) = module_id(&c.name, &c.dir, &p) else { continue };
            if modules.contains_key(&mid) {
                continue;
            }
            let src = std::fs::read(&p).with_context(|| format!("reading {}", p.display()))?;
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
            if let Some(n) = store.nodes.get_mut(&id).filter(|n| matches!(n.kind, NodeKind::Function | NodeKind::Class) && n.entry.is_none()) {
                n.entry = Some(Entry { kind: EntryKind::Export, label: label(&name) });
            }
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

fn method_of(store: &GraphBuilder, cls_id: &str, name: &str) -> Option<String> {
    let cand = format!("{cls_id}.{name}");
    store.nodes.contains_key(&cand).then_some(cand)
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
            let known = if r.starts_with("ext:") { foreign_type(&r) } else { store.nodes.get(&r).map_or(false, |x| x.kind == NodeKind::Class) };
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
                    if !foreign_type(&r) {
                        continue;
                    }
                    store.ext_node(&r);
                }
                store.add_edge(id, &r, kind, None);
            }
        }
        link_calls(modules, store, crate_ids, id);
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

/// The string literals under a node that can name a place: a closure's are its own code's.
fn place_literals(n: TsNode, src: &[u8]) -> Vec<String> {
    match n.kind() {
        "closure_expression" => vec![],
        "string_literal" | "raw_string_literal" => vec![unquote(text(n, src))],
        _ => named_children(n).into_iter().flat_map(|k| place_literals(k, src)).collect(),
    }
}

/// Methods of std's `Result` and `Option`: they open what a call gave, and move nothing.
const UNWRAPS: &[&str] = &["unwrap", "expect", "unwrap_or", "unwrap_or_else", "unwrap_or_default", "ok", "err", "map", "map_err", "and_then", "or_else", "context", "with_context"];

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
    let src = &m.src;
    let Some(d) = def_node(m, &n) else { return };
    let Some(body) = d.child_by_field_name("body") else { return };
    // typed parameters give receivers a type: `store.add_edge(..)` with `store: &mut GraphBuilder`
    let mut locals: HashMap<String, String> = HashMap::new();
    // locals a call made, by the call: `let c = reqwest::Client::new()`
    let mut made_by: HashMap<String, TsNode> = HashMap::new();
    // `&mut T` and `&mut self` hand the value back changed: `T -> T`
    let mut mutates: Vec<(String, String)> = vec![];
    if let Some(pn) = d.child_by_field_name("parameters") {
        for p in named_children(pn) {
            if p.kind() == "self_parameter" && text(p, src).contains("mut") {
                mutates.extend(n.cls.clone().map(|c| (c, "self".to_string())));
                continue;
            }
            if p.kind() != "parameter" {
                continue;
            }
            let Some(pat) = p.child_by_field_name("pattern") else { continue };
            let pname = text(pat, src).trim_start_matches("mut ").to_string();
            let by_mut_ref = p.child_by_field_name("type").map_or(false, |t| text(t, src).starts_with("&mut"));
            for t in type_names(p.child_by_field_name("type"), src) {
                if let Some(r) = resolve(m, &t, modules, &store.nodes, crate_ids).filter(|r| !r.starts_with("ext:")) {
                    if store.nodes.get(&r).map_or(false, |x| x.kind == NodeKind::Class) {
                        if by_mut_ref {
                            mutates.push((r.clone(), pname.clone()));
                        }
                        locals.insert(pname.clone(), r);
                        break;
                    }
                }
            }
        }
    }
    let cls = n.cls.clone();
    // locals assigned from a constructor, a struct literal, or a function with a typed return
    let class_of = |id: &str| store.nodes.get(id).filter(|x| x.kind == NodeKind::Class).map(|_| id.to_string());
    let returned_class = |id: &str| store.edges.iter().find(|e| e.src == id && e.kind == EdgeKind::Returns && store.nodes.get(&e.dst).map_or(false, |x| x.kind == NodeKind::Class)).map(|e| e.dst.clone());
    for l in walk(body, &["let_declaration"]) {
        let Some(pat) = l.child_by_field_name("pattern") else { continue };
        if pat.kind() != "identifier" && pat.kind() != "mut_pattern" {
            continue;
        }
        let name = text(pat, src).trim_start_matches("mut ").to_string();
        let mut value = l.child_by_field_name("value");
        loop {
            let Some(v) = value else { break };
            match v.kind() {
                "try_expression" | "await_expression" | "unary_expression" | "reference_expression" | "parenthesized_expression" => value = named_children(v).into_iter().next(),
                // `.unwrap()`, `.expect(..)`, `.ok_or_else(..)`, `.context(..)`: the receiver decides the type
                "call_expression" => match v.child_by_field_name("function") {
                    Some(f) if f.kind() == "field_expression" => value = f.child_by_field_name("value"),
                    _ => break,
                },
                _ => break,
            }
        }
        if let Some(made) = l.child_by_field_name("value") {
            made_by.insert(name.clone(), made);
        }
        let Some(v) = value else { continue };
        let ty = match v.kind() {
            "identifier" => locals.get(text(v, src)).cloned(),
            "struct_expression" => resolve(m, &path_parts(v.child_by_field_name("name"), src), modules, &store.nodes, crate_ids).and_then(|r| class_of(&r)),
            "call_expression" => {
                let parts = path_parts(v.child_by_field_name("function"), src);
                if parts.is_empty() {
                    None
                } else {
                    let full = resolve(m, &parts, modules, &store.nodes, crate_ids);
                    full.as_deref().and_then(returned_class).or_else(|| {
                        resolve(m, &parts[..parts.len().saturating_sub(1)], modules, &store.nodes, crate_ids).and_then(|r| class_of(&r))
                    })
                }
            }
            _ => None,
        };
        if let Some(t) = ty {
            locals.insert(name, t);
        }
    }
    let attr_types = cls.as_ref().and_then(|c| store.classes.get(c)).map(|c| c.attr_types.clone()).unwrap_or_default();
    let mut new_edges: Vec<(String, EdgeKind, Option<String>, Vec<(u32, u32)>)> = vec![];
    let choices = number_choices(body, src, &CHOICES);
    let mut touches: Vec<Touch> = vec![];
    let lit_kinds = &["string_literal", "raw_string_literal"];
    // a boundary call, refined by the literals that name its place
    let touch = |c: tree_sitter::Node, kind: &'static str, op: &str, lits: &[String], locals: &HashMap<String, String>| {
        let f = effects::facts(kind, None, op, lits);
        let what = what_of(c, src, lit_kinds, "arguments");
        let payload = payload_of(c, src, &what, "arguments", locals);
        let fixed = effects::locates(f.kind) && starts_at_literal(c, src, lit_kinds, "arguments", effects::on_receiver(&op));
        f.places()
            .into_iter()
            .map(|(artifact, dir)| Touch { kind: f.kind.into(), what: what.clone(), payload: payload.clone(), op: op.into(), artifact, dir: dir.into(), fixed, line: c.start_position().row as u32 + 1, n: 1 })
            .collect::<Vec<_>>()
    };
    for mac in walk(body, &["macro_invocation"]) {
        let name = mac.child_by_field_name("macro").map(|x| text(x, src)).unwrap_or("");
        if effects::screen_macro(name) {
            touches.push(Touch { kind: "screen".into(), op: name.into(), line: mac.start_position().row as u32 + 1, n: 1, ..Default::default() });
        }
    }
    let mut unresolved: Vec<String> = vec![];
    for c in evaluation_order(walk(body, &["call_expression", "struct_expression"])) {
        if c.kind() == "struct_expression" {
            let parts = path_parts(c.child_by_field_name("name"), src);
            if let Some(r) = resolve(m, &parts, modules, &store.nodes, crate_ids).filter(|r| !r.starts_with("ext:")) {
                if store.nodes.get(&r).map_or(false, |x| x.kind == NodeKind::Class) {
                    new_edges.push((r, EdgeKind::Instantiates, None, arms_of(c, body, src, &choices, &CHOICES)));
                }
            }
            continue;
        }
        let Some(f) = c.child_by_field_name("function") else { continue };
        // an own function handed on as a value (`xs.iter().map(Self::label)`, `thread::spawn(run)`)
        // is called by whoever receives it
        for a in c.child_by_field_name("arguments").map(named_children).unwrap_or_default() {
            if !matches!(a.kind(), "identifier" | "scoped_identifier") || locals.contains_key(text(a, src)) {
                continue;
            }
            let ps = path_parts(Some(a), src);
            let handed = match (ps.first().map(String::as_str), &cls) {
                (Some("Self"), Some(c)) => ps.get(1).and_then(|m| method_of(store, c, m)),
                _ => resolve(m, &ps, modules, &store.nodes, crate_ids),
            };
            if let Some(t) = handed.filter(|t| store.nodes.get(t).map_or(false, |n| matches!(n.kind, NodeKind::Function | NodeKind::Method))) {
                new_edges.push((t, EdgeKind::Calls, None, arms_of(a, body, src, &choices, &CHOICES)));
            }
        }
        let mut target: Option<String> = None;
        let mut parts: Vec<String> = vec![];
        if f.kind() == "field_expression" {
            // a method call: receiver.method(..)
            let (Some(value), Some(field)) = (f.child_by_field_name("value"), f.child_by_field_name("field")) else { continue };
            let method = text(field, src).to_string();
            let receiver: Option<String> = match value.kind() {
                "self" => cls.clone(),
                "identifier" => locals.get(text(value, src)).cloned(),
                "field_expression" => {
                    let v = value.child_by_field_name("value");
                    let fld = value.child_by_field_name("field").map(|x| text(x, src)).unwrap_or("");
                    match v.map(|v| v.kind()) {
                        Some("self") => attr_types.get(fld).cloned(),
                        Some("identifier") => locals
                            .get(text(v.unwrap(), src))
                            .and_then(|t| store.classes.get(t))
                            .and_then(|c| c.attr_types.get(fld).cloned()),
                        _ => None,
                    }
                }
                _ => None,
            };
            match receiver {
                // a method not found on the type (`to_string`, `clone`, a trait's default) is not
                // known; it builds nothing
                Some(r) => target = method_of(store, &r, &method),
                None => {
                    let made = library_chain(value, m, src, &made_by);
                    // the outer call of a chain stands for the chain: `client.get(url).send()`
                    if made.is_some() && c.parent().map_or(false, |p| p.kind() == "field_expression") {
                        continue;
                    }
                    let subtree = || {
                        let mut l = place_literals(c, src);
                        l.extend(made.as_ref().and_then(|x| x.origin).map(|o| place_literals(o, src)).unwrap_or_default());
                        l
                    };
                    let moved = made.as_ref().and_then(|x| {
                        let here: Vec<(String, TsNode)> = x.here.iter().cloned().chain([(method.clone(), c)]).collect();
                        // a builder's calls up to its `build` build, and move nothing (C-BUILDER)
                        let built = if x.ctor.last().map(String::as_str) == Some("builder") {
                            x.before.iter().chain(&here).position(|(op, _)| op == "build").map_or(usize::MAX, |i| i + 1)
                        } else {
                            0
                        };
                        let moves = |i: usize, op: &str| match op {
                            _ if i < built || UNWRAPS.contains(&op) => None,
                            _ if effects::RUST_PATH_READS.contains(&op) => Some("disk_read"),
                            _ => effects::boundary_rust(&[&x.ty[..], &[op.to_string()]].concat()),
                        };
                        let n = x.before.len();
                        // the move is named by the first call a library knows by name, else the first that moves
                        let (kind, op) = here.iter().enumerate().filter_map(|(i, (op, _))| moves(n + i, op).map(|k| (k, op.clone()))).min_by_key(|(k, _)| *k == "disk")?;
                        let touched = effects::boundary_rust(&x.ctor).is_some() || x.before.iter().enumerate().any(|(i, (op, _))| moves(i, op).is_some());
                        let writes = x.before.iter().chain(&here).any(|(o, _)| matches!(o.as_str(), "write" | "append" | "create" | "create_new" | "truncate"));
                        let own: Vec<String> = here.iter().flat_map(|(_, n)| n.child_by_field_name("arguments").map(|a| place_literals(a, src)).unwrap_or_default()).collect();
                        Some((if writes && kind.starts_with("disk") { "disk_write" } else { kind }, op, touched, own))
                    });
                    match moved {
                        // a value made by a touch is at that touch's place (`File::open(p)`,
                        // `Command::new("git")`): a call on it adds a place only when it names its
                        // own, `conn.execute("INSERT INTO log ..")`
                        Some((kind, op, true, own)) => touches.extend(touch(c, kind, &op, &own, &locals).into_iter().filter(|t| !t.artifact.is_empty())),
                        // a value built without touching anything touches where it is used, at the
                        // place its building named: `OpenOptions::new().append(true).open("log")`
                        Some((kind, op, false, _)) => touches.extend(touch(c, kind, &op, &subtree(), &locals)),
                        None if effects::RUST_PATH_READS.contains(&method.as_str()) => touches.extend(touch(c, "disk_read", &method, &subtree(), &locals)),
                        None => {}
                    }
                    continue;
                }
            }
            if target.is_none() {
                continue;
            }
        } else {
            parts = path_parts(Some(f), src);
            if parts.is_empty() {
                continue;
            }
            if parts[0] == "Self" {
                if let Some(c) = &cls {
                    let mut p = vec![c.clone()];
                    p.extend(parts[1..].iter().cloned());
                    target = method_of(store, c, parts.get(1).map(String::as_str).unwrap_or("")).or_else(|| Some(c.clone()));
                    parts = p;
                }
            } else {
                target = resolve(m, &parts, modules, &store.nodes, crate_ids);
                if target.as_deref().map_or(true, |t| t.starts_with("ext:") && false) {}
                if target.is_none() && parts.len() > 1 {
                    // `Type::new(..)`: a function on a type
                    if let Some(r) = resolve(m, &parts[..parts.len() - 1], modules, &store.nodes, crate_ids).filter(|r| !r.starts_with("ext:")) {
                        if store.nodes.get(&r).map_or(false, |x| x.kind == NodeKind::Class) {
                            target = method_of(store, &r, &parts[parts.len() - 1]).or(Some(r));
                        }
                    }
                }
            }
        }
        match target {
            Some(t) if !t.starts_with("ext:") => {
                let kind = if store.nodes[&t].kind == NodeKind::Class { EdgeKind::Instantiates } else { EdgeKind::Calls };
                // `Type::method(..)` with no such method known: keep the name, it is what was called
                let via = if kind == EdgeKind::Instantiates && parts.len() > 1 && parts[0] != "Self" { parts.last().cloned() } else { None };
                new_edges.push((t, kind, via, arms_of(c, body, src, &choices, &CHOICES)));
            }
            _ => {
                if parts.is_empty() {
                    continue;
                }
                let full = expand(m, &parts);
                unresolved.push(full.iter().take(2).cloned().collect::<Vec<_>>().join("."));
                if let Some(tag) = effects::boundary_rust(&full) {
                    touches.extend(touch(c, tag, parts.last().map(String::as_str).unwrap_or(""), &literals(c, src, lit_kinds), &locals));
                }
            }
        }
    }
    for (t, kind, via, arms) in new_edges {
        store.add_call(id, &t, kind, via, arms);
    }
    let compares = compares(body, src, &choices);
    if let Some(node) = store.nodes.get_mut(id) {
        node.compares = compares;
    }
    for (t, input) in mutates {
        store.add_edge(id, &t, EdgeKind::Mutates, Some(input));
    }
    for u in unresolved {
        *store.unresolved.entry(u).or_insert(0) += 1;
    }
    for t in touches {
        store.touch(id, t);
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

/// Where a body compares one value with string literals (`kind == "net"`, a `match` on
/// `&str`), and the choice each comparison decides.
fn compares(body: TsNode, src: &[u8], choices: &HashMap<usize, u32>) -> Vec<Compare> {
    let lit = |n: TsNode| matches!(n.kind(), "string_literal" | "raw_string_literal");
    let mut out = vec![];
    for n in walk(body, &["binary_expression", "match_expression", "macro_invocation"]) {
        let line = n.start_position().row as u32 + 1;
        if n.kind() == "macro_invocation" {
            // `matches!(kind, "a" | "b")`: the value before the comma, the literals of the pattern after it
            if n.child_by_field_name("macro").map(|x| text(x, src)) != Some("matches") {
                continue;
            }
            let Some(tt) = named_children(n).into_iter().find(|c| c.kind() == "token_tree") else { continue };
            // a nested `( .. )` is a token tree of its own, so the first comma among the tokens splits
            let kids: Vec<TsNode> = (0..tt.child_count()).filter_map(|i| tt.child(i)).collect();
            let Some(comma) = kids.iter().position(|k| k.kind() == ",").filter(|c| *c > 1) else { continue };
            let guard = kids.iter().skip(comma).position(|k| text(*k, src) == "if").map_or(kids.len() - 1, |i| comma + i);
            let (from, to) = (kids[comma].end_byte(), kids[guard].start_byte());
            let cases: Vec<String> = walk(tt, &["string_literal", "raw_string_literal"])
                .into_iter()
                .filter(|s| s.start_byte() >= from && s.end_byte() <= to)
                .map(|s| unquote(text(s, src)))
                .collect();
            if !cases.is_empty() {
                let subject = String::from_utf8_lossy(&src[kids[1].start_byte()..kids[comma - 1].end_byte()]).to_string();
                out.push(Compare { subject, cases, line, choice: decides(n, body, src, choices, &CHOICES) });
            }
            continue;
        }
        if n.kind() == "match_expression" {
            let (Some(value), Some(block)) = (n.child_by_field_name("value"), n.child_by_field_name("body")) else { continue };
            let cases: Vec<String> = named_children(block)
                .into_iter()
                .filter_map(|a| a.child_by_field_name("pattern"))
                .flat_map(|p| walk(p, &["string_literal", "raw_string_literal"]))
                .map(|s| unquote(text(s, src)))
                .collect();
            if !cases.is_empty() {
                out.push(Compare { subject: text(value, src).to_string(), cases, line, choice: choices.get(&block.id()).copied() });
            }
            continue;
        }
        let op = n.child_by_field_name("operator").map(|o| text(o, src)).unwrap_or("");
        let (Some(l), Some(r)) = (n.child_by_field_name("left"), n.child_by_field_name("right")) else { continue };
        if !matches!(op, "==" | "!=") {
            continue;
        }
        let (subject, case) = match (lit(l), lit(r)) {
            (false, true) => (l, r),
            (true, false) => (r, l),
            _ => continue,
        };
        out.push(Compare { subject: text(subject, src).to_string(), cases: vec![unquote(text(case, src))], line, choice: decides(n, body, src, choices, &CHOICES) });
    }
    out
}

/// A foreign type worth keeping in a signature: from a third-party crate, not std or a container.
fn foreign_type(id: &str) -> bool {
    let Some(full) = id.strip_prefix("ext:") else { return false };
    let head = full.split('.').next().unwrap_or("");
    let name = full.rsplit('.').next().unwrap_or("");
    full.contains('.')
        && !["std", "core", "alloc"].contains(&head)
        && !["Result", "Option", "Vec", "Box", "HashMap", "HashSet", "BTreeMap", "String", "Path", "PathBuf"].contains(&name)
        // `D::Error`, `S::Ok`, `Self::Target`: associated types on generic parameters, not places
        && !(head.len() == 1 && head.chars().all(|c| c.is_ascii_uppercase()))
        && !["Ok", "Err", "Error", "Target", "Output", "Item", "Self"].contains(&name)
}
