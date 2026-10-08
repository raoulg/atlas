//! `parse: Repo -> Graph` for Python packages.
//!
//! Two passes over tree-sitter trees: symbols and imports first, then resolution of
//! bases, signatures and calls into edges. The engine knows the language; it never
//! knows a project.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use indexmap::IndexMap;
use tree_sitter::{Node as TsNode, Parser, Tree};
use walkdir::WalkDir;

use crate::effects;
use crate::entries::entry_of;
use crate::model::{EdgeKind, Entry, EntryKind, Node, NodeKind, Touch};
pub use crate::builder::{ClassInfo, GraphBuilder, SigInfo};
use crate::tree::{field_children, literals, named_children, payload_of, text, walk, what_of};

const BUILTIN_TYPES: &[&str] = &[
    "str", "int", "float", "bool", "bytes", "list", "dict", "set", "tuple", "None", "Any", "Optional",
    "Union", "Iterable", "Iterator", "Sequence", "Mapping", "Callable", "Path", "Literal", "TypeVar",
    "Self", "ClassVar", "type", "object", "frozenset", "Generator", "AsyncIterator", "Awaitable",
    "Annotated",
];

struct Module {
    id: String,
    path: PathBuf,
    src: Vec<u8>,
    tree: Tree,
    package: String,
    bindings: HashMap<String, String>,
    symbols: HashMap<String, String>,
    ext_pkgs: HashSet<String>,
}

// ---------- tree helpers ----------

/// identifier / attribute / dotted_name -> parts.
fn dotted(n: Option<TsNode>, src: &[u8]) -> Vec<String> {
    let Some(n) = n else { return vec![] };
    match n.kind() {
        "identifier" => vec![text(n, src).to_string()],
        "dotted_name" => text(n, src).split('.').map(String::from).collect(),
        "attribute" => {
            let mut base = dotted(n.child_by_field_name("object"), src);
            match (base.is_empty(), n.child_by_field_name("attribute")) {
                (false, Some(a)) => {
                    base.push(text(a, src).to_string());
                    base
                }
                _ => vec![],
            }
        }
        "subscript" => dotted(n.child_by_field_name("value"), src),
        _ => vec![],
    }
}

/// All dotted names mentioned in an annotation, e.g. `list[Graph]` -> `[[Graph]]`.
fn type_names(t: Option<TsNode>, src: &[u8]) -> Vec<Vec<String>> {
    let Some(t) = t else { return vec![] };
    let mut out = vec![];
    for n in walk(t, &["identifier", "attribute"]) {
        if n.kind() == "identifier" && n.parent().map_or(false, |p| p.kind() == "attribute") {
            continue;
        }
        let parts = dotted(Some(n), src);
        if let Some(last) = parts.last() {
            if !BUILTIN_TYPES.contains(&last.as_str()) {
                out.push(parts);
            }
        }
    }
    out
}

fn first_docline(body: Option<TsNode>, src: &[u8]) -> String {
    let Some(body) = body else { return String::new() };
    let Some(first) = named_children(body).into_iter().next() else { return String::new() };
    if first.kind() != "expression_statement" {
        return String::new();
    }
    match named_children(first).into_iter().next() {
        Some(s) if s.kind() == "string" => {
            let raw = text(s, src).trim_matches(|c| c == '"' || c == '\'' || c == ' ' || c == '\n');
            raw.lines().next().unwrap_or("").trim().to_string()
        }
        _ => String::new(),
    }
}

/// Direct child definitions of a block, with their decorators.
fn iter_defs<'t>(block: TsNode<'t>, src: &[u8]) -> Vec<(Vec<String>, TsNode<'t>)> {
    let mut out = vec![];
    for ch in named_children(block) {
        match ch.kind() {
            "function_definition" | "class_definition" => out.push((vec![], ch)),
            "decorated_definition" => {
                let decs = named_children(ch)
                    .into_iter()
                    .filter(|d| d.kind() == "decorator")
                    .map(|d| text(d, src).trim_start_matches('@').to_string())
                    .collect();
                if let Some(d) = ch.child_by_field_name("definition") {
                    out.push((decs, d));
                }
            }
            _ => {}
        }
    }
    out
}

fn signature(f: TsNode, src: &[u8]) -> (Vec<String>, String, SigInfo) {
    let mut params = vec![];
    let mut info = SigInfo::default();
    if let Some(pn) = f.child_by_field_name("parameters") {
        for p in named_children(pn) {
            let name_node = if p.kind() == "identifier" {
                Some(p)
            } else {
                p.child_by_field_name("name").or_else(|| named_children(p).into_iter().next())
            };
            let pname = name_node.map(|n| text(n, src)).unwrap_or_else(|| text(p, src));
            let t = p.child_by_field_name("type");
            params.push(match t {
                Some(t) => format!("{pname}: {}", text(t, src)),
                None => pname.to_string(),
            });
            info.ptypes.extend(type_names(t, src));
        }
    }
    let rt = f.child_by_field_name("return_type");
    let returns = rt.map(|r| text(r, src).to_string()).unwrap_or_default();
    info.rtypes = type_names(rt, src);
    (params, returns, info)
}

// ---------- loading ----------

fn new_parser() -> Result<Parser> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter::Language::new(tree_sitter_python::LANGUAGE))
        .context("loading the Python grammar")?;
    Ok(parser)
}

fn load_module(parser: &mut Parser, id: &str, path: &Path, is_pkg: bool) -> Result<Module> {
    let src = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let tree = parser.parse(&src, None).with_context(|| format!("parsing {}", path.display()))?;
    let package = if is_pkg { id.to_string() } else { id.rsplit_once('.').map(|x| x.0).unwrap_or(id).to_string() };
    Ok(Module {
        id: id.into(),
        path: path.into(),
        src,
        tree,
        package,
        bindings: HashMap::new(),
        symbols: HashMap::new(),
        ext_pkgs: HashSet::new(),
    })
}

/// The nearest ancestor of the package that holds a `pyproject.toml`.
pub fn project_root(pkg_dir: &Path) -> Option<PathBuf> {
    let mut cur = Some(pkg_dir);
    for _ in 0..4 {
        let p = cur?;
        if p.join("pyproject.toml").exists() {
            return Some(p.to_path_buf());
        }
        cur = p.parent();
    }
    None
}

/// The importable package inside a repo: `src/<pkg>` or `<pkg>/`, the first with an `__init__.py`.
pub fn discover_package(repo: &Path) -> Result<PathBuf> {
    if repo.join("__init__.py").exists() {
        return Ok(repo.to_path_buf());
    }
    for base in [repo.join("src"), repo.to_path_buf()] {
        let Ok(rd) = std::fs::read_dir(&base) else { continue };
        let mut dirs: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
        dirs.sort();
        for d in dirs {
            let name = d.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.starts_with('.') || ["tests", "test", "node_modules", "docs"].contains(&name) {
                continue;
            }
            if d.join("__init__.py").exists() {
                return Ok(d);
            }
        }
    }
    anyhow::bail!("no Python package found under {} (looked for src/<pkg>/__init__.py and <pkg>/__init__.py)", repo.display())
}

/// Parse a package (and the scripts next to it) into a `GraphBuilder`. Returns the store and the root package name.
pub fn parse(pkg_dir: &Path, project_root: Option<&Path>) -> Result<(GraphBuilder, String)> {
    let root = pkg_dir.file_name().and_then(|n| n.to_str()).context("package dir has no name")?.to_string();
    let mut parser = new_parser()?;
    let mut modules: IndexMap<String, Module> = IndexMap::new();
    let mut store = GraphBuilder::default();

    let mut files: Vec<PathBuf> = WalkDir::new(pkg_dir)
        .into_iter()
        .filter_entry(|e| {
            let n = e.file_name().to_str().unwrap_or("");
            !(n.starts_with('.') || n == "__pycache__")
        })
        .flatten()
        .filter(|e| e.file_type().is_file() && e.path().extension().and_then(|x| x.to_str()) == Some("py"))
        .map(|e| e.path().to_path_buf())
        .collect();
    files.sort();
    let base = pkg_dir.parent().unwrap_or(pkg_dir);
    for p in files {
        let rel = p.strip_prefix(base).unwrap_or(&p).with_extension("");
        let mut parts: Vec<String> = rel.components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect();
        let is_pkg = parts.last().map_or(false, |l| l == "__init__");
        if is_pkg {
            parts.pop();
        }
        let mid = parts.join(".");
        let m = load_module(&mut parser, &mid, &p, is_pkg)?;
        store.files.insert(p.to_string_lossy().to_string(), String::from_utf8_lossy(&m.src).to_string());
        store.add_node(Node {
            id: mid.clone(),
            kind: NodeKind::Module,
            name: parts.last().cloned().unwrap_or_default(),
            file: p.to_string_lossy().to_string(),
            span: Some((1, m.tree.root_node().end_position().row as u32 + 1)),
            module: mid.clone(),
            doc: first_docline(Some(m.tree.root_node()), &m.src),
            is_pkg,
            ..Default::default()
        });
        modules.insert(mid, m);
    }
    // scripts next to the package (repo root and scripts/) are routes into it
    if let Some(root_dir) = project_root {
        let mut extras: Vec<PathBuf> = vec![];
        for dir in [root_dir.to_path_buf(), root_dir.join("scripts")] {
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            let mut ps: Vec<PathBuf> = rd
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_file() && p.extension().and_then(|x| x.to_str()) == Some("py"))
                .collect();
            ps.sort();
            extras.extend(ps);
        }
        for p in extras {
            let rel = p.strip_prefix(root_dir).unwrap_or(&p).to_path_buf();
            let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("script").to_string();
            let mid = if rel.starts_with("scripts") { format!("scripts.{stem}") } else { format!("_root.{stem}") };
            let m = load_module(&mut parser, &mid, &p, false)?;
            store.files.insert(p.to_string_lossy().to_string(), String::from_utf8_lossy(&m.src).to_string());
            store.add_node(Node {
                id: mid.clone(),
                kind: NodeKind::Module,
                name: stem,
                file: p.to_string_lossy().to_string(),
                span: Some((1, m.tree.root_node().end_position().row as u32 + 1)),
                module: mid.clone(),
                doc: first_docline(Some(m.tree.root_node()), &m.src),
                entry: Some(Entry { kind: EntryKind::Script, label: rel.to_string_lossy().to_string() }),
                ..Default::default()
            });
            modules.insert(mid, m);
        }
    }

    for m in modules.values_mut() {
        collect_symbols(m, &mut store);
    }
    let module_ids: HashSet<String> = modules.keys().cloned().collect();
    for m in modules.values_mut() {
        collect_imports(m, &module_ids, &mut store, &root);
        m.ext_pkgs = m.bindings.values().filter(|v| !v.starts_with(&root)).map(|v| v.split('.').next().unwrap_or("").to_string()).collect();
        store.bindings.insert(m.id.clone(), m.bindings.clone());
    }
    link(&modules, &mut store, &root);
    Ok((store, root))
}

// ---------- pass 1: symbols and imports ----------

fn collect_symbols(m: &mut Module, store: &mut GraphBuilder) {
    let root = m.tree.root_node();
    let src = &m.src;
    let file = m.path.to_string_lossy().to_string();
    for (decs, d) in iter_defs(root, src) {
        let name = d.child_by_field_name("name").map(|n| text(n, src)).unwrap_or("").to_string();
        let nid = format!("{}.{}", m.id, name);
        m.symbols.insert(name.clone(), nid.clone());
        let body = d.child_by_field_name("body");
        let span = Some((d.start_position().row as u32 + 1, d.end_position().row as u32 + 1));
        if d.kind() == "class_definition" {
            let bases: Vec<Vec<String>> = d
                .child_by_field_name("superclasses")
                .map(|s| named_children(s).into_iter().map(|b| dotted(Some(b), src)).filter(|b| !b.is_empty()).collect())
                .unwrap_or_default();
            let mut methods = vec![];
            let mut fields = vec![];
            if let Some(body) = body {
                for (mdecs, md) in iter_defs(body, src) {
                    let mname = md.child_by_field_name("name").map(|n| text(n, src)).unwrap_or("").to_string();
                    let mnid = format!("{nid}.{mname}");
                    let (params, returns, sig) = signature(md, src);
                    store.sigs.insert(mnid.clone(), sig);
                    store.add_node(Node {
                        id: mnid.clone(),
                        kind: NodeKind::Method,
                        name: mname.clone(),
                        file: file.clone(),
                        span: Some((md.start_position().row as u32 + 1, md.end_position().row as u32 + 1)),
                        module: m.id.clone(),
                        doc: first_docline(md.child_by_field_name("body"), src),
                        cls: Some(nid.clone()),
                        decorators: mdecs.clone(),
                        params,
                        returns,
                        entry: entry_of(&mdecs, &mname),
                        ..Default::default()
                    });
                    methods.push(mname);
                    store.add_edge(&nid, &mnid, EdgeKind::Contains, None);
                }
                for st in named_children(body) {
                    if st.kind() != "expression_statement" {
                        continue;
                    }
                    let Some(a) = named_children(st).into_iter().next() else { continue };
                    if a.kind() != "assignment" {
                        continue;
                    }
                    if let (Some(left), Some(t)) = (a.child_by_field_name("left"), a.child_by_field_name("type")) {
                        if left.kind() == "identifier" {
                            fields.push((text(left, src).to_string(), type_names(Some(t), src)));
                        }
                    }
                }
            }
            store.classes.insert(nid.clone(), ClassInfo { bases: bases.clone(), resolved: vec![], attr_types: BTreeMap::new(), fields });
            store.add_node(Node {
                id: nid.clone(),
                kind: NodeKind::Class,
                name,
                file: file.clone(),
                span,
                module: m.id.clone(),
                doc: first_docline(body, src),
                decorators: decs,
                bases: bases.iter().map(|b| b.join(".")).collect(),
                methods,
                ..Default::default()
            });
            store.add_edge(&m.id, &nid, EdgeKind::Contains, None);
        } else {
            let (params, returns, sig) = signature(d, src);
            store.sigs.insert(nid.clone(), sig);
            store.add_node(Node {
                id: nid.clone(),
                kind: NodeKind::Function,
                name: name.clone(),
                file: file.clone(),
                span,
                module: m.id.clone(),
                doc: first_docline(body, src),
                decorators: decs.clone(),
                params,
                returns,
                entry: entry_of(&decs, &name),
                ..Default::default()
            });
            store.add_edge(&m.id, &nid, EdgeKind::Contains, None);
        }
    }
}

fn resolve_relative(m: &Module, dots: usize, name: Option<&str>) -> String {
    let mut base: Vec<&str> = m.package.split('.').collect();
    if dots > 1 {
        let keep = base.len().saturating_sub(dots - 1);
        base.truncate(keep);
    }
    if let Some(n) = name {
        base.push(n);
    }
    base.join(".")
}

fn import_edge(m: &Module, target: &str, module_ids: &HashSet<String>, store: &mut GraphBuilder, root: &str) {
    if !target.starts_with(root) {
        return;
    }
    if module_ids.contains(target) || store.nodes.contains_key(target) {
        store.add_edge(&m.id, target, EdgeKind::Imports, None);
    } else if let Some((modid, _)) = target.rsplit_once('.') {
        if module_ids.contains(modid) {
            store.add_edge(&m.id, modid, EdgeKind::Imports, None);
        }
    }
}

fn collect_imports(m: &mut Module, module_ids: &HashSet<String>, store: &mut GraphBuilder, root: &str) {
    let tree_root = m.tree.root_node();
    let mut bindings: Vec<(String, String)> = vec![];
    let mut edges: Vec<String> = vec![];
    for st in walk(tree_root, &["import_statement", "import_from_statement"]) {
        let src = &m.src;
        if st.kind() == "import_statement" {
            for n in field_children(st, "name") {
                if n.kind() == "aliased_import" {
                    let target = n.child_by_field_name("name").map(|x| text(x, src)).unwrap_or("").to_string();
                    let alias = n.child_by_field_name("alias").map(|x| text(x, src)).unwrap_or("").to_string();
                    bindings.push((alias, target.clone()));
                    edges.push(target);
                } else {
                    let target = text(n, src).to_string();
                    let head = target.split('.').next().unwrap_or("").to_string();
                    bindings.push((head.clone(), head));
                    bindings.push((target.clone(), target.clone()));
                    edges.push(target);
                }
            }
        } else {
            let Some(mn) = st.child_by_field_name("module_name") else { continue };
            let modname = if mn.kind() == "relative_import" {
                let raw = text(mn, src);
                let dots = raw.len() - raw.trim_start_matches('.').len();
                let rest = raw.trim_start_matches('.');
                resolve_relative(m, dots, if rest.is_empty() { None } else { Some(rest) })
            } else {
                text(mn, src).to_string()
            };
            let names = field_children(st, "name");
            if names.is_empty() {
                edges.push(modname.clone());
                continue;
            }
            for n in names {
                let (nm, alias) = if n.kind() == "aliased_import" {
                    (
                        n.child_by_field_name("name").map(|x| text(x, src)).unwrap_or("").to_string(),
                        n.child_by_field_name("alias").map(|x| text(x, src)).unwrap_or("").to_string(),
                    )
                } else {
                    let t = text(n, src).to_string();
                    (t.clone(), t)
                };
                let target = format!("{modname}.{nm}");
                bindings.push((alias, target.clone()));
                edges.push(target);
            }
        }
    }
    for (k, v) in bindings {
        m.bindings.insert(k, v);
    }
    for target in edges {
        import_edge(m, &target, module_ids, store, root);
    }
}

// ---------- pass 2: resolution ----------

/// Dotted name in module scope -> node id, `ext:<name>`, or None.
fn resolve(m: &Module, parts: &[String], modules: &IndexMap<String, Module>, nodes: &IndexMap<String, Node>, root: &str) -> Option<String> {
    let head = parts.first()?;
    let mut target = m.symbols.get(head).or_else(|| m.bindings.get(head))?.clone();
    for p in &parts[1..] {
        target = format!("{target}.{p}");
    }
    if nodes.contains_key(&target) {
        return Some(target);
    }
    if target.starts_with(&format!("{root}.")) {
        let (modid, nm) = target.rsplit_once('.')?;
        let t2 = modules.get(modid)?.bindings.get(nm)?;
        return if nodes.contains_key(t2) { Some(t2.clone()) } else { None };
    }
    Some(format!("ext:{target}"))
}

fn method_of(store: &GraphBuilder, cls_id: &str, name: &str, depth: u32) -> Option<String> {
    if depth > 8 {
        return None;
    }
    let cand = format!("{cls_id}.{name}");
    if store.nodes.contains_key(&cand) {
        return Some(cand);
    }
    for b in store.classes.get(cls_id).map(|c| c.resolved.clone()).unwrap_or_default() {
        if !b.starts_with("ext:") {
            if let Some(r) = method_of(store, &b, name, depth + 1) {
                return Some(r);
            }
        }
    }
    None
}

/// The definition node of a symbol, found again by its line and name.
fn def_node<'t>(m: &'t Module, n: &Node) -> Option<TsNode<'t>> {
    if n.kind == NodeKind::Module {
        return Some(m.tree.root_node());
    }
    let (start, _) = n.span?;
    walk(m.tree.root_node(), &["function_definition", "class_definition"]).into_iter().find(|d| {
        d.start_position().row as u32 + 1 == start && d.child_by_field_name("name").map_or(false, |x| text(x, &m.src) == n.name)
    })
}

fn link(modules: &IndexMap<String, Module>, store: &mut GraphBuilder, root: &str) {
    let aliases = type_aliases(modules);
    let ids: Vec<String> = store.nodes.keys().cloned().collect();
    // bases and annotated fields
    for id in &ids {
        let (kind, module) = (store.nodes[id].kind, store.nodes[id].module.clone());
        if kind != NodeKind::Class {
            continue;
        }
        let m = &modules[&module];
        let info = store.classes.get(id).cloned().unwrap_or_default();
        let mut resolved = vec![];
        for b in &info.bases {
            let r = resolve(m, b, modules, &store.nodes, root).unwrap_or_else(|| format!("ext:{}", b.join(".")));
            if r.starts_with("ext:") {
                store.ext_node(&r);
            }
            store.add_edge(id, &r, EdgeKind::Inherits, None);
            resolved.push(r);
        }
        store.classes.entry(id.clone()).or_default().resolved = resolved;
        for (fname, types) in &info.fields {
            for t in types {
                if let Some(r) = resolve(m, t, modules, &store.nodes, root).filter(|r| !r.starts_with("ext:")) {
                    store.classes.entry(id.clone()).or_default().attr_types.insert(fname.clone(), r.clone());
                    store.add_edge(id, &r, EdgeKind::Has, Some(fname.clone()));
                }
            }
        }
    }
    // self.x = param in __init__
    for id in &ids {
        let n = &store.nodes[id];
        if n.kind == NodeKind::Method && n.name == "__init__" {
            infer_self_attrs(modules, store, root, id);
        }
    }
    // methods that change their receiver, directly or through another such method on self
    for _ in 0..5 {
        let before = store.mutating.len();
        for id in &ids {
            let n = &store.nodes[id];
            let (Some(cls), true) = (n.cls.clone(), n.kind == NodeKind::Method && n.name != "__init__") else { continue };
            let m = &modules[&n.module];
            let Some(body) = def_node(m, n).and_then(|d| d.child_by_field_name("body")) else { continue };
            let on_self = |c: TsNode| {
                let f = c.child_by_field_name("function").filter(|f| f.kind() == "attribute");
                let obj = f.and_then(|f| f.child_by_field_name("object")).map(|o| text(o, &m.src));
                let name = f.and_then(|f| f.child_by_field_name("attribute")).map(|a| text(a, &m.src)).unwrap_or("");
                obj == Some("self") && method_of(store, &cls, name, 0).map_or(false, |t| store.mutating.contains(&t))
            };
            if changed_roots(body, &m.src, &on_self).contains("self") {
                let id = id.clone();
                store.mutating.insert(id);
            }
        }
        if store.mutating.len() == before {
            break;
        }
    }
    // signatures -> takes / returns; bodies -> calls / instantiates / boundaries
    for id in &ids {
        let (kind, module) = (store.nodes[id].kind, store.nodes[id].module.clone());
        match kind {
            NodeKind::Function | NodeKind::Method => {
                let m = &modules[&module];
                let sig = store.sigs.get(id).cloned().unwrap_or_default();
                for (types, kind) in [(&sig.ptypes, EdgeKind::Takes), (&sig.rtypes, EdgeKind::Returns)] {
                    let targets: Vec<String> = types
                        .iter()
                        .flat_map(|t| match resolve(m, t, modules, &store.nodes, root) {
                            Some(r) => vec![r],
                            None => alias_targets(m, t, modules, &aliases, &store.nodes, root),
                        })
                        .collect();
                    for r in targets {
                        if r.starts_with("ext:") {
                            if !foreign_type(&r) {
                                continue;
                            }
                            store.ext_node(&r);
                        }
                        store.add_edge(id, &r, kind, None);
                    }
                }
                link_calls(modules, store, root, id);
            }
            NodeKind::Module => link_calls(modules, store, root, id),
            _ => {}
        }
    }
}

/// Typed parameters that are classes of the package, by name: `graph: Graph` -> Graph.
fn param_types(m: &Module, d: TsNode, modules: &IndexMap<String, Module>, nodes: &IndexMap<String, Node>, root: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let Some(pn) = d.child_by_field_name("parameters") else { return out };
    for p in named_children(pn) {
        // `typed_parameter` carries its identifier as a plain child, `typed_default_parameter` as a field
        let name = p.child_by_field_name("name").or_else(|| named_children(p).into_iter().find(|c| c.kind() == "identifier"));
        if let (Some(name), Some(t)) = (name, p.child_by_field_name("type")) {
            for tn in type_names(Some(t), &m.src) {
                if let Some(r) = resolve(m, &tn, modules, nodes, root).filter(|r| !r.starts_with("ext:")) {
                    if nodes.get(&r).map_or(false, |x| x.kind == NodeKind::Class) {
                        out.insert(text(name, &m.src).to_string(), r);
                    }
                }
            }
        }
    }
    out
}

/// Methods that change their library receiver: `xs.append(..)`, `g.add_node(..)`.
const MUTATORS: &[&str] = &[
    "append", "extend", "insert", "remove", "pop", "clear", "update", "add", "discard", "setdefault", "sort", "reverse",
    "popitem", "add_node", "add_edge", "add_nodes_from", "add_edges_from", "remove_node", "remove_edge", "push",
];

/// The name a chain like `self.items[0].x` starts from.
fn root_name<'a>(n: TsNode, src: &'a [u8]) -> Option<&'a str> {
    match n.kind() {
        "identifier" => Some(text(n, src)),
        "attribute" => root_name(n.child_by_field_name("object")?, src),
        "subscript" => root_name(n.child_by_field_name("value")?, src),
        _ => None,
    }
}

/// The names whose objects a body changes in place: the root of every assignment to an
/// attribute or item (`self.x = ..`, `p.items[k] = ..`), and of every call to a mutator
/// (`self.nodes.append(..)`) or to a method known to change its receiver. Changing a part
/// changes the whole.
fn changed_roots<'a>(body: TsNode, src: &'a [u8], mutating: &dyn Fn(TsNode) -> bool) -> HashSet<&'a str> {
    let mut out = HashSet::new();
    for a in walk(body, &["assignment", "augmented_assignment"]) {
        if let Some(left) = a.child_by_field_name("left").filter(|l| matches!(l.kind(), "attribute" | "subscript")) {
            out.extend(root_name(left, src));
        }
    }
    for c in walk(body, &["call"]) {
        let Some(f) = c.child_by_field_name("function").filter(|f| f.kind() == "attribute") else { continue };
        let Some(obj) = f.child_by_field_name("object") else { continue };
        let name = f.child_by_field_name("attribute").map(|a| text(a, src)).unwrap_or("");
        if MUTATORS.contains(&name) || mutating(c) {
            out.extend(root_name(obj, src));
        }
    }
    out
}

fn infer_self_attrs(modules: &IndexMap<String, Module>, store: &mut GraphBuilder, root: &str, init_id: &str) {
    let n = store.nodes[init_id].clone();
    let Some(cls) = n.cls.clone() else { return };
    let m = &modules[&n.module];
    let Some(d) = def_node(m, &n) else { return };
    let src = &m.src;
    let ptypes = param_types(m, d, modules, &store.nodes, root);
    let Some(body) = d.child_by_field_name("body") else { return };
    let mut found: Vec<(String, String)> = vec![];
    for a in walk(body, &["assignment"]) {
        let Some(left) = a.child_by_field_name("left") else { continue };
        if left.kind() != "attribute" {
            continue;
        }
        let lp = dotted(Some(left), src);
        if lp.len() != 2 || lp[0] != "self" {
            continue;
        }
        let mut target: Option<String> = None;
        if let Some(t) = a.child_by_field_name("type") {
            for tn in type_names(Some(t), src) {
                if let Some(r) = resolve(m, &tn, modules, &store.nodes, root).filter(|r| !r.starts_with("ext:")) {
                    target = Some(r);
                }
            }
        }
        if target.is_none() {
            if let Some(right) = a.child_by_field_name("right") {
                if right.kind() == "identifier" {
                    target = ptypes.get(text(right, src)).cloned();
                } else if right.kind() == "call" {
                    let parts = dotted(right.child_by_field_name("function"), src);
                    if let Some(r) = resolve(m, &parts, modules, &store.nodes, root) {
                        if store.nodes.get(&r).map_or(false, |x| x.kind == NodeKind::Class) {
                            target = Some(r);
                        }
                    }
                }
            }
        }
        if let Some(t) = target {
            found.push((lp[1].clone(), t));
        }
    }
    for (attr, t) in found {
        store.classes.entry(cls.clone()).or_default().attr_types.insert(attr.clone(), t.clone());
        store.add_edge(&cls, &t, EdgeKind::Has, Some(attr));
    }
}

/// Module-level type aliases, `Result = Union[FitResult, FailedFit]` or `Id = int | str`, by
/// full name: the module they live in and the names they stand for.
fn type_aliases(modules: &IndexMap<String, Module>) -> HashMap<String, (String, Vec<Vec<String>>)> {
    let mut out = HashMap::new();
    for (mid, m) in modules {
        for a in top_assignments(m) {
            let (Some(left), Some(right)) = (a.child_by_field_name("left"), a.child_by_field_name("right")) else { continue };
            let is_type = match right.kind() {
                "subscript" => matches!(dotted(right.child_by_field_name("value"), &m.src).last().map(String::as_str), Some("Union" | "Optional")),
                "binary_operator" => text(right, &m.src).contains('|'),
                _ => false,
            };
            if left.kind() == "identifier" && is_type {
                out.insert(format!("{mid}.{}", text(left, &m.src)), (mid.clone(), type_names(Some(right), &m.src)));
            }
        }
    }
    out
}

/// The assignments at the top level of a module.
fn top_assignments<'t>(m: &'t Module) -> Vec<TsNode<'t>> {
    named_children(m.tree.root_node())
        .into_iter()
        .filter(|s| s.kind() == "expression_statement")
        .flat_map(named_children)
        .filter(|a| a.kind() == "assignment")
        .collect()
}

/// The classes an alias stands for, resolved in the module that defines it.
fn alias_targets(m: &Module, t: &[String], modules: &IndexMap<String, Module>, aliases: &HashMap<String, (String, Vec<Vec<String>>)>, nodes: &IndexMap<String, Node>, root: &str) -> Vec<String> {
    let [name] = t else { return vec![] };
    let full = m.bindings.get(name).cloned().unwrap_or_else(|| format!("{}.{name}", m.id));
    let Some((mid, names)) = aliases.get(&full) else { return vec![] };
    names.iter().filter_map(|n| resolve(&modules[mid], n, modules, nodes, root)).collect()
}

/// I/O functions named without being called: `READERS = {".csv": pd.read_csv}` reads csv
/// wherever `READERS` is used. Keyed by the module-level name that holds them.
fn io_tables(m: &Module, root: &str) -> HashMap<String, Vec<Touch>> {
    let mut out: HashMap<String, Vec<Touch>> = HashMap::new();
    for a in top_assignments(m) {
        let (Some(left), Some(right)) = (a.child_by_field_name("left"), a.child_by_field_name("right")) else { continue };
        if left.kind() != "identifier" {
            continue;
        }
        let ts = io_refs(m, right, root, text(left, &m.src));
        if !ts.is_empty() {
            out.insert(text(left, &m.src).to_string(), ts);
        }
    }
    out
}

/// Attributes under `n` that name an I/O function of a library without calling it.
fn io_refs(m: &Module, n: TsNode, root: &str, what: &str) -> Vec<Touch> {
    let mut out = vec![];
    for a in walk(n, &["attribute"]) {
        let called = a.parent().map_or(false, |p| p.kind() == "call" && p.child_by_field_name("function").map(|f| f.id()) == Some(a.id()));
        if called || a.parent().map_or(false, |p| p.kind() == "attribute") {
            continue;
        }
        let parts = dotted(Some(a), &m.src);
        let Some(pkg) = parts.first().and_then(|h| m.bindings.get(h)).filter(|b| !b.starts_with(root)).and_then(|b| b.split('.').next()) else { continue };
        let Some(tag) = effects::boundary(Some(pkg), &parts, &m.ext_pkgs) else { continue };
        let op = parts.last().cloned().unwrap_or_default();
        let f = effects::facts(tag, Some(pkg), &op, &[]);
        out.push(Touch { kind: f.kind.into(), what: what.into(), op, artifact: f.artifact, dir: f.dir.into(), n: 1, ..Default::default() });
    }
    out
}

fn link_calls(modules: &IndexMap<String, Module>, store: &mut GraphBuilder, root: &str, id: &str) {
    let n = store.nodes[id].clone();
    let m = &modules[&n.module];
    let Some(d) = def_node(m, &n) else { return };
    let src = &m.src;
    let body = if n.kind == NodeKind::Module { Some(d) } else { d.child_by_field_name("body") };
    let Some(body) = body else { return };
    let cls_attrs = n.cls.as_ref().and_then(|c| store.classes.get(c)).map(|c| c.attr_types.clone());
    // typed parameters, and locals assigned from a constructor or from a function whose return type is a class
    let params = if n.kind == NodeKind::Module { HashMap::new() } else { param_types(m, d, modules, &store.nodes, root) };
    let mut locals: HashMap<String, String> = params.clone();
    if n.kind != NodeKind::Module {
        for a in walk(body, &["assignment"]) {
            let (Some(left), Some(right)) = (a.child_by_field_name("left"), a.child_by_field_name("right")) else { continue };
            if left.kind() != "identifier" || right.kind() != "call" {
                continue;
            }
            let parts = dotted(right.child_by_field_name("function"), src);
            let Some(r) = resolve(m, &parts, modules, &store.nodes, root).filter(|r| !r.starts_with("ext:")) else { continue };
            let ty = match store.nodes.get(&r).map(|x| x.kind) {
                Some(NodeKind::Class) => Some(r),
                Some(NodeKind::Function) | Some(NodeKind::Method) => store
                    .edges
                    .iter()
                    .find(|e| e.src == r && e.kind == EdgeKind::Returns && store.nodes.get(&e.dst).map_or(false, |x| x.kind == NodeKind::Class))
                    .map(|e| e.dst.clone()),
                _ => None,
            };
            if let Some(t) = ty {
                locals.insert(text(left, src).to_string(), t);
            }
        }
    }
    let mut touches: Vec<Touch> = vec![];
    let mut new_edges: Vec<(String, EdgeKind)> = vec![];
    for c in walk(body, &["call"]) {
        if n.kind == NodeKind::Module {
            // calls inside a definition belong to that definition
            let mut p = c.parent();
            let mut inner = false;
            while let Some(pp) = p {
                if pp.id() == body.id() {
                    break;
                }
                if matches!(pp.kind(), "function_definition" | "class_definition") {
                    inner = true;
                    break;
                }
                p = pp.parent();
            }
            if inner {
                continue;
            }
        }
        let fnode = c.child_by_field_name("function");
        let mut parts = dotted(fnode, src);
        let mut target: Option<String> = None;
        if parts.is_empty() {
            // `X(...).method(...)`: a method on something just constructed
            let Some(f) = fnode.filter(|f| f.kind() == "attribute") else { continue };
            let (Some(obj), Some(attr)) = (f.child_by_field_name("object"), f.child_by_field_name("attribute")) else { continue };
            if obj.kind() != "call" {
                continue;
            }
            let inner = dotted(obj.child_by_field_name("function"), src);
            if inner.is_empty() {
                continue;
            }
            let attr = text(attr, src).to_string();
            if let Some(r) = resolve(m, &inner, modules, &store.nodes, root).filter(|r| !r.starts_with("ext:")) {
                if store.nodes.get(&r).map_or(false, |x| x.kind == NodeKind::Class) {
                    target = method_of(store, &r, &attr, 0).or(Some(r));
                }
            }
            parts = inner;
            parts.push(attr);
        }
        if target.is_some() {
            // resolved through the constructor chain above
        } else if (parts[0] == "self" || parts[0] == "cls") && n.cls.is_some() {
            let cls = n.cls.as_deref().unwrap();
            if parts.len() == 2 {
                target = method_of(store, cls, &parts[1], 0);
            } else if parts.len() == 3 {
                if let Some(t) = cls_attrs.as_ref().and_then(|a| a.get(&parts[1])) {
                    target = method_of(store, t, &parts[2], 0).or_else(|| Some(t.clone()));
                }
            }
        } else if parts.len() == 2 && locals.contains_key(&parts[0]) {
            let t = &locals[&parts[0]];
            target = method_of(store, t, &parts[1], 0).or_else(|| Some(t.clone()));
        } else {
            target = resolve(m, &parts, modules, &store.nodes, root);
            if target.is_none() && parts.len() > 1 {
                if let Some(r) = resolve(m, &parts[..parts.len() - 1], modules, &store.nodes, root) {
                    if store.nodes.get(&r).map_or(false, |x| x.kind == NodeKind::Class) {
                        target = method_of(store, &r, &parts[parts.len() - 1], 0).or(Some(r));
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
                let key = parts.iter().take(2).cloned().collect::<Vec<_>>().join(".");
                *store.unresolved.entry(key).or_insert(0) += 1;
                let pkg = m
                    .bindings
                    .get(&parts[0])
                    .filter(|b| !b.starts_with(root))
                    .and_then(|b| b.split('.').next())
                    .map(str::to_string);
                if let Some(tag) = effects::boundary(pkg.as_deref(), &parts, &m.ext_pkgs) {
                    let op = parts.last().cloned().unwrap_or_default();
                    let f = effects::facts(tag, pkg.as_deref(), &op, &literals(c, src, &["string"]));
                    let what = what_of(c, src, &["string"], "arguments");
                    let payload = payload_of(c, src, &what, "arguments", &locals);
                    touches.push(Touch { kind: f.kind.into(), what, payload, op, artifact: f.artifact, dir: f.dir.into(), n: 1 });
                }
            }
        }
    }
    // what the body changes of its inputs: `T -> T` for the receiver or a parameter it changes;
    // a value the body constructs itself is built, not changed
    if n.kind != NodeKind::Module {
        let receiver_mutates = |c: TsNode| {
            let f = c.child_by_field_name("function").filter(|f| f.kind() == "attribute");
            let obj = f.and_then(|f| f.child_by_field_name("object")).filter(|o| o.kind() == "identifier").map(|o| text(o, src));
            let name = f.and_then(|f| f.child_by_field_name("attribute")).map(|a| text(a, src)).unwrap_or("");
            let ty = obj.and_then(|o| if o == "self" { n.cls.clone() } else { locals.get(o).cloned() });
            ty.and_then(|t| method_of(store, &t, name, 0)).map_or(false, |t| store.mutating.contains(&t))
        };
        for r in changed_roots(body, src, &receiver_mutates) {
            let ty = if r == "self" { n.cls.clone().filter(|_| n.name != "__init__") } else { params.get(r).cloned() };
            if let Some(t) = ty {
                new_edges.push((t, EdgeKind::Mutates));
            }
        }
    }
    // I/O functions referenced, directly or through a module-level table, may be called later
    if n.kind != NodeKind::Module {
        touches.extend(io_refs(m, body, root, ""));
        let tables = io_tables(m, root);
        for ident in walk(body, &["identifier"]) {
            if let Some(ts) = tables.get(text(ident, src)) {
                touches.extend(ts.iter().cloned());
            }
        }
    }
    for (t, kind) in new_edges {
        store.add_edge(id, &t, kind, None);
    }
    for t in touches {
        store.touch(id, t);
    }
}

/// A foreign type worth keeping in a signature: bound to a third-party package, not a builtin.
fn foreign_type(id: &str) -> bool {
    let Some(full) = id.strip_prefix("ext:") else { return false };
    let head = full.split('.').next().unwrap_or("");
    full.contains('.') && !["typing", "collections", "pathlib", "builtins", "datetime", "abc", "os", "enum", "dataclasses", "functools", "itertools"].contains(&head)
}
