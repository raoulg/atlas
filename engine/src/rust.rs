//! `parse: Repo -> Graph` for Rust workspaces.
//!
//! Crates become root modules, files become modules, structs/enums/traits become
//! classes, `impl` blocks attach methods to their type and `impl Trait for` edges,
//! `use` declarations become bindings. Calls resolve through paths, `self`, typed
//! struct fields and typed parameters.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use indexmap::IndexMap;
use tree_sitter::{Node as TsNode, Parser, Tree};
use walkdir::WalkDir;

use crate::effects;
use crate::model::{EdgeKind, Entry, EntryKind, Node, NodeKind};
use crate::store::{ClassInfo, SigInfo, Store};
use crate::tree::{named_children, payload_of, span, text, walk, what_of};

struct Crate {
    name: String,
    dir: PathBuf,
    is_bin: bool,
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
        let is_bin = dir.join("src/main.rs").exists() || dir.join("src/bin").is_dir() || d.get("bin").is_some();
        out.push(Crate { name: name.to_string(), dir, is_bin });
    }
    if out.is_empty() {
        anyhow::bail!("no crates found in {}", manifest.display());
    }
    Ok(out)
}

/// `src/lib.rs` -> crate, `src/a/mod.rs` -> crate.a, `src/a/b.rs` -> crate.a.b, `tests/x.rs` -> crate.tests.x
fn module_id(krate: &str, dir: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(dir).ok()?.with_extension("");
    let mut parts: Vec<String> = rel.components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect();
    if parts.first().map(String::as_str) == Some("src") {
        parts.remove(0);
        if parts.last().map(String::as_str) == Some("mod") {
            parts.pop();
        }
        if parts.len() == 1 && (parts[0] == "lib" || parts[0] == "main") {
            parts.clear();
        }
    }
    let mut id = krate.to_string();
    for p in parts {
        id.push('.');
        id.push_str(&p);
    }
    Some(id)
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

fn fn_name(f: TsNode, src: &[u8]) -> String {
    f.child_by_field_name("name").map(|n| text(n, src)).unwrap_or("").to_string()
}

// ---------- loading ----------

/// Parse a Cargo workspace or crate into a `Store`; returns the store and the workspace name.
pub fn parse(repo: &Path) -> Result<(Store, String)> {
    let mut parser = new_parser()?;
    let mut modules: IndexMap<String, Module> = IndexMap::new();
    let mut store = Store::default();
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
            .filter_entry(|e| {
                let n = e.file_name().to_str().unwrap_or("");
                !(n.starts_with('.') || n == "target" || n == "node_modules")
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
            let Some(mid) = module_id(&c.name, &c.dir, &p) else { continue };
            if modules.contains_key(&mid) {
                continue;
            }
            let src = std::fs::read(&p).with_context(|| format!("reading {}", p.display()))?;
            let tree = parser.parse(&src, None).with_context(|| format!("parsing {}", p.display()))?;
            let is_root = mid == c.name;
            let parent = if is_root { mid.clone() } else { mid.rsplit_once('.').map(|x| x.0.to_string()).unwrap_or(mid.clone()) };
            let name = mid.rsplit('.').next().unwrap_or(&mid).to_string();
            store.files.insert(p.to_string_lossy().to_string(), String::from_utf8_lossy(&src).to_string());
            store.add_node(Node {
                id: mid.clone(),
                kind: NodeKind::Module,
                name,
                file: p.to_string_lossy().to_string(),
                span: span(tree.root_node()),
                module: mid.clone(),
                doc: module_doc(tree.root_node(), &src),
                is_pkg: is_root,
                ..Default::default()
            });
            modules.insert(
                mid.clone(),
                Module { id: mid, krate: c.name.clone(), parent, path: p, src, tree, bindings: HashMap::new(), wildcards: vec![], symbols: HashMap::new(), is_root },
            );
        }
    }
    let bins: HashSet<String> = crate_list.iter().filter(|c| c.is_bin).map(|c| c.name.clone()).collect();
    for m in modules.values_mut() {
        collect_symbols(m, &mut store, &bins);
    }
    let module_ids: HashSet<String> = modules.keys().cloned().collect();
    for m in modules.values_mut() {
        collect_uses(m, &crate_ids, &module_ids, &mut store);
        store.bindings.insert(m.id.clone(), m.bindings.clone());
    }
    collect_impls(&modules, &mut store, &crate_ids);
    link(&modules, &mut store, &crate_ids);
    Ok((store, root_name))
}

// ---------- pass 1: symbols ----------

fn collect_symbols(m: &mut Module, store: &mut Store, bins: &HashSet<String>) {
    let src = &m.src;
    let file = m.path.to_string_lossy().to_string();
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
                } else if name == "main" && m.is_root && bins.contains(&m.krate) {
                    Some(Entry { kind: EntryKind::Cli, label: m.krate.clone() })
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
                        for v in walk(body, &["enum_variant"]) {
                            let vname = v.child_by_field_name("name").map(|n| text(n, src)).unwrap_or("").to_string();
                            let types = type_names(v.child_by_field_name("body"), src);
                            if !types.is_empty() {
                                fields.push((vname, types));
                            }
                        }
                    }
                }
                store.classes.insert(nid.clone(), ClassInfo { bases: vec![], resolved: vec![], attr_types: BTreeMap::new(), fields });
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

fn collect_uses(m: &mut Module, crate_ids: &HashMap<String, String>, module_ids: &HashSet<String>, store: &mut Store) {
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

fn method_of(store: &Store, cls_id: &str, name: &str) -> Option<String> {
    let cand = format!("{cls_id}.{name}");
    store.nodes.contains_key(&cand).then_some(cand)
}

// ---------- pass 1c: impl blocks ----------

fn collect_impls(modules: &IndexMap<String, Module>, store: &mut Store, crate_ids: &HashMap<String, String>) {
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
        let file = m.path.to_string_lossy().to_string();
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

fn link(modules: &IndexMap<String, Module>, store: &mut Store, crate_ids: &HashMap<String, String>) {
    let ids: Vec<String> = store.nodes.keys().cloned().collect();
    for id in &ids {
        let (kind, module) = (store.nodes[id].kind, store.nodes[id].module.clone());
        if kind != NodeKind::Class {
            continue;
        }
        let m = &modules[&module];
        let fields = store.classes.get(id).map(|c| c.fields.clone()).unwrap_or_default();
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
    }
    for id in &ids {
        let (kind, module) = (store.nodes[id].kind, store.nodes[id].module.clone());
        if !matches!(kind, NodeKind::Function | NodeKind::Method) {
            continue;
        }
        let m = &modules[&module];
        let sig = store.sigs.get(id).cloned().unwrap_or_default();
        for (types, kind) in [(&sig.ptypes, EdgeKind::Takes), (&sig.rtypes, EdgeKind::Returns)] {
            for t in types {
                let Some(r) = resolve(m, t, modules, &store.nodes, crate_ids) else { continue };
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

fn link_calls(modules: &IndexMap<String, Module>, store: &mut Store, crate_ids: &HashMap<String, String>, id: &str) {
    let n = store.nodes[id].clone();
    let m = &modules[&n.module];
    let src = &m.src;
    let Some(d) = def_node(m, &n) else { return };
    let Some(body) = d.child_by_field_name("body") else { return };
    // typed parameters give receivers a type: `store.add_edge(..)` with `store: &mut Store`
    let mut locals: HashMap<String, String> = HashMap::new();
    if let Some(pn) = d.child_by_field_name("parameters") {
        for p in named_children(pn).into_iter().filter(|p| p.kind() == "parameter") {
            let Some(pat) = p.child_by_field_name("pattern") else { continue };
            let pname = text(pat, src).trim_start_matches("mut ").to_string();
            for t in type_names(p.child_by_field_name("type"), src) {
                if let Some(r) = resolve(m, &t, modules, &store.nodes, crate_ids).filter(|r| !r.starts_with("ext:")) {
                    if store.nodes.get(&r).map_or(false, |x| x.kind == NodeKind::Class) {
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
    let mut new_edges: Vec<(String, EdgeKind)> = vec![];
    let mut touches: Vec<(String, String, String)> = vec![];
    let mut unresolved: Vec<String> = vec![];
    for c in walk(body, &["call_expression", "struct_expression"]) {
        if c.kind() == "struct_expression" {
            let parts = path_parts(c.child_by_field_name("name"), src);
            if let Some(r) = resolve(m, &parts, modules, &store.nodes, crate_ids).filter(|r| !r.starts_with("ext:")) {
                if store.nodes.get(&r).map_or(false, |x| x.kind == NodeKind::Class) {
                    new_edges.push((r, EdgeKind::Instantiates));
                }
            }
            continue;
        }
        let Some(f) = c.child_by_field_name("function") else { continue };
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
                Some(r) => target = method_of(store, &r, &method).or_else(|| store.nodes.contains_key(&r).then(|| r.clone())),
                None => continue,
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
                new_edges.push((t, kind));
            }
            _ => {
                if parts.is_empty() {
                    continue;
                }
                let full = expand(m, &parts);
                unresolved.push(full.iter().take(2).cloned().collect::<Vec<_>>().join("."));
                if let Some(tag) = effects::boundary_rust(&full) {
                    let what = what_of(c, src, &["string_literal", "raw_string_literal"], "arguments");
                    let payload = payload_of(c, src, &what, "arguments", &locals);
                    touches.push((tag.to_string(), what, payload));
                }
            }
        }
    }
    for (t, kind) in new_edges {
        store.add_edge(id, &t, kind, None);
    }
    for u in unresolved {
        *store.unresolved.entry(u).or_insert(0) += 1;
    }
    for (kind, what, payload) in touches {
        store.touch(id, &kind, &what, &payload);
    }
}

/// A foreign type worth keeping in a signature: from a third-party crate, not std or a container.
fn foreign_type(id: &str) -> bool {
    let Some(full) = id.strip_prefix("ext:") else { return false };
    let head = full.split('.').next().unwrap_or("");
    let name = full.rsplit('.').next().unwrap_or("");
    full.contains('.')
        && !["std", "core", "alloc", "anyhow", "serde", "serde_json", "indexmap", "regex", "toml", "clap"].contains(&head)
        && !["Result", "Option", "Vec", "Box", "HashMap", "HashSet", "BTreeMap", "IndexMap", "String", "Path", "PathBuf"].contains(&name)
}
