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
use crate::model::{Compare, EdgeKind, Entry, EntryKind, Node, NodeKind, Touch};
pub use crate::builder::{ClassInfo, GraphBuilder, SigInfo};
use crate::tree::{
    argument, arms_of, decides, evaluation_order, field_children, literals, named_children, nth_of_kind, number_choices, payload_of, span, starts_at_literal, text, unquote, walk,
    what_of, ChoiceSyntax, Part,
};

const BUILTIN_TYPES: &[&str] = &[
    "str", "int", "float", "bool", "bytes", "list", "dict", "set", "tuple", "None", "Any", "Optional",
    "Union", "Iterable", "Iterator", "Sequence", "Mapping", "Callable", "Path", "Literal", "TypeVar",
    "Self", "ClassVar", "type", "object", "frozenset", "Generator", "AsyncIterator", "Awaitable",
    "Annotated",
];

/// Generic types whose parameters are the values they hold or stand for: `list[Graph]` holds
/// Graphs, `Optional[Node]` is a Node or nothing. A generic class of a package or a library
/// (`VectorCache[TVectorizer]`, `Dataset[Row]`) is a value of that class; its parameters are
/// types, not values that come along.
const CONTAINERS: &[&str] = &[
    "list", "dict", "set", "tuple", "frozenset", "type", "Optional", "Union", "Iterable", "Iterator", "Sequence",
    "Mapping", "Callable", "Generator", "AsyncIterator", "Awaitable", "Annotated", "ClassVar", "List", "Dict",
    "Tuple", "Set", "FrozenSet", "Type", "Deque", "deque", "DefaultDict", "defaultdict", "OrderedDict", "Counter",
    "ChainMap", "Collection", "Container", "AbstractSet", "MutableSet", "MutableSequence", "MutableMapping",
    "AsyncIterable", "AsyncGenerator", "Coroutine", "Reversible", "Final", "Required", "NotRequired", "ReadOnly",
    "TypeGuard", "TypeIs", "Unpack",
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

/// An array annotated with its dtype and shape, jaxtyping style: `Float[Tensor, "batch dim"]`
/// is one type, `Float[Tensor, batch dim]`, its shape the contract a step keeps.
fn shaped<'t>(n: TsNode<'t>, src: &[u8]) -> Option<String> {
    // an expression subscripts (`Float[..]`); an annotation is a generic type of type parameters
    let unwrap = |x: TsNode<'t>| if x.kind() == "type" { named_children(x).into_iter().next().unwrap_or(x) } else { x };
    let (head, args) = match n.kind() {
        "subscript" => (dotted(n.child_by_field_name("value"), src), field_children(n, "subscript")),
        "generic_type" => {
            let kids = named_children(n);
            let params = kids.iter().find(|k| k.kind() == "type_parameter").map(|tp| named_children(*tp).into_iter().map(unwrap).collect()).unwrap_or_default();
            (dotted(kids.first().copied(), src), params)
        }
        _ => return None,
    };
    let ([array, shape], Some(dtype)) = (args.as_slice(), head.last()) else { return None };
    if shape.kind() != "string" || named_children(*shape).iter().any(|c| c.kind() == "interpolation") || matches!(dtype.as_str(), "Literal" | "Annotated") {
        return None;
    }
    let array = text(*array, src).rsplit('.').next().unwrap_or("");
    let dims = unquote(text(*shape, src)).split_whitespace().collect::<Vec<_>>().join(" ");
    Some(format!("{}[{array}, {dims}]", head.join(".")))
}

/// All dotted names mentioned in an annotation, e.g. `list[Graph]` -> `[[Graph]]`. A shaped
/// array is one name, `[["Float[Tensor, batch dim]"]]`.
fn type_names(t: Option<TsNode>, src: &[u8]) -> Vec<Vec<String>> {
    let Some(t) = t else { return vec![] };
    let mut out = vec![];
    let mut keep = |parts: Vec<String>| {
        if parts.last().map_or(false, |l| !BUILTIN_TYPES.contains(&l.as_str())) {
            out.push(parts);
        }
    };
    let arrays: Vec<TsNode> = walk(t, &["subscript", "generic_type"]).into_iter().filter(|n| shaped(*n, src).is_some()).collect();
    // the parameters of a generic class that is not a container: `VectorCache[TVectorizer]`
    let mut params: Vec<TsNode> = vec![];
    for g in walk(t, &["subscript", "generic_type"]) {
        let (head, args) = match g.kind() {
            "subscript" => (dotted(g.child_by_field_name("value"), src), field_children(g, "subscript")),
            _ => (dotted(named_children(g).first().copied(), src), named_children(g).into_iter().filter(|k| k.kind() == "type_parameter").collect()),
        };
        if head.last().map_or(false, |h| !CONTAINERS.contains(&h.as_str())) {
            params.extend(args);
        }
    }
    let inside = |n: TsNode| arrays.iter().chain(params.iter()).any(|a| a.start_byte() <= n.start_byte() && n.end_byte() <= a.end_byte());
    for a in arrays.iter().filter(|a| !arrays.iter().any(|b| b.id() != a.id() && b.start_byte() <= a.start_byte() && a.end_byte() <= b.end_byte())) {
        keep(vec![shaped(*a, src).unwrap_or_default()]);
    }
    for n in walk(t, &["identifier", "attribute", "string"]) {
        if inside(n) {
            continue;
        }
        if n.kind() == "identifier" && n.parent().map_or(false, |p| p.kind() == "attribute") {
            continue;
        }
        if n.kind() == "string" {
            // a quoted annotation is a forward reference: `"pd.DataFrame"`, `"List[Node]"`
            let lit = text(n, src).trim_matches(|c| c == '"' || c == '\'');
            for name in quoted_values(lit) {
                keep(name.split('.').map(str::to_string).collect());
            }
            continue;
        }
        if n.parent().map_or(false, |p| p.kind() == "string" || p.kind() == "interpolation") {
            continue;
        }
        keep(dotted(Some(n), src));
    }
    out
}

/// The names in a quoted annotation that stand for values: `"List[Node]"` -> Node,
/// `"VectorCache[TVectorizer]"` -> VectorCache, its parameter being a type.
fn quoted_values(lit: &str) -> Vec<&str> {
    let mut out = vec![];
    // per open bracket: whether what it holds are parameters of a generic class
    let mut opaque: Vec<bool> = vec![];
    let mut last = "";
    let mut start = None;
    let is_name = |c: char| c.is_alphanumeric() || c == '_' || c == '.';
    for (i, c) in lit.char_indices().chain(std::iter::once((lit.len(), ' '))) {
        if is_name(c) {
            start.get_or_insert(i);
            continue;
        }
        if let Some(s) = start.take() {
            let w = &lit[s..i];
            if w.chars().next().map_or(false, |c| c.is_alphabetic() || c == '_') && !opaque.iter().any(|o| *o) {
                out.push(w);
            }
            last = w;
        }
        match c {
            '[' => opaque.push(!CONTAINERS.contains(&last.rsplit('.').next().unwrap_or(last))),
            ']' => {
                opaque.pop();
            }
            _ => {}
        }
        if !c.is_whitespace() {
            last = if c == '[' { "" } else { last };
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
            if matches!(p.kind(), "comment" | "keyword_separator" | "positional_separator") {
                continue;
            }
            let name_node = match p.kind() {
                "identifier" | "list_splat_pattern" | "dictionary_splat_pattern" => Some(p),
                _ => p.child_by_field_name("name").or_else(|| named_children(p).into_iter().next()),
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
    let src = match path.extension().and_then(|x| x.to_str()) {
        Some("ipynb") => notebook_source(path)?.into_bytes(),
        _ => std::fs::read(path).with_context(|| format!("reading {}", path.display()))?,
    };
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

/// A notebook as the script it runs: its code cells in order, separated as jupytext does
/// (`# %%`). A line for IPython (`%matplotlib`, `!pip install`) is kept as a comment, so the
/// rest still parses as Python.
fn notebook_source(path: &Path) -> Result<String> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let nb: serde_json::Value = serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    let cells = nb.get("cells").and_then(|c| c.as_array()).cloned().unwrap_or_default();
    let mut out = vec![];
    for cell in cells.iter().filter(|c| c.get("cell_type").and_then(|t| t.as_str()) == Some("code")) {
        let src = match cell.get("source") {
            Some(serde_json::Value::Array(lines)) => lines.iter().filter_map(|l| l.as_str()).collect::<String>(),
            Some(serde_json::Value::String(s)) => s.clone(),
            _ => continue,
        };
        let code: Vec<String> = src.lines().map(|l| if l.trim_start().starts_with(['%', '!']) { format!("# {l}") } else { l.to_string() }).collect();
        out.push(format!("# %%\n{}", code.join("\n")));
    }
    Ok(out.join("\n\n") + "\n")
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
            ..Default::default()
        });
        modules.insert(mid, m);
    }
    // scripts next to the package (repo root and scripts/) are routes into it
    if let Some(root_dir) = project_root {
        let mut extras: Vec<PathBuf> = vec![];
        // scripts and notebooks next to the package, and the examples that wire it together
        let dirs = ["", "scripts", "examples", "demo", "demos", "notebooks"].map(|d| root_dir.join(d));
        for dir in dirs {
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            let mut ps: Vec<PathBuf> = rd
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_file() && matches!(p.extension().and_then(|x| x.to_str()), Some("py" | "ipynb")))
                .collect();
            ps.sort();
            extras.extend(ps);
        }
        for p in extras {
            let rel = p.strip_prefix(root_dir).unwrap_or(&p).to_path_buf();
            let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("script").to_string();
            let mid = match rel.parent().and_then(|d| d.to_str()).filter(|d| !d.is_empty()) {
                Some(d) => format!("{d}.{stem}"),
                None => format!("_root.{stem}"),
            };
            let m = load_module(&mut parser, &mid, &p, false)?;
            store.files.insert(crate::tree::path_str(&p), String::from_utf8_lossy(&m.src).to_string());
            store.add_node(Node {
                id: mid.clone(),
                kind: NodeKind::Module,
                name: stem,
                file: crate::tree::path_str(&p),
                span: Some((1, m.tree.root_node().end_position().row as u32 + 1)),
                module: mid.clone(),
                doc: first_docline(Some(m.tree.root_node()), &m.src),
                entry: Some(Entry {
                    kind: if p.extension().and_then(|x| x.to_str()) == Some("ipynb") { EntryKind::Notebook } else { EntryKind::Script },
                    label: crate::tree::path_str(&rel),
                }),
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
    mark_exports(&modules, &mut store, &root);
    route_prefixes(&modules, &mut store);
    Ok((store, root))
}

/// A web app's routes at the paths a client asks for: under the prefix an app is mounted at
/// (`app.mount("/api", sub)`), a router is included with (`app.include_router(r, prefix="/v1")`)
/// and a router declares (`APIRouter(prefix="/items")`).
fn route_prefixes(modules: &IndexMap<String, Module>, store: &mut GraphBuilder) {
    let mut own: HashMap<String, String> = HashMap::new();
    let mut mounted: HashMap<String, (String, String)> = HashMap::new();
    for m in modules.values() {
        let src = &m.src;
        let key = |name: &str| m.bindings.get(name).cloned().unwrap_or_else(|| format!("{}.{name}", m.id));
        for call in walk(m.tree.root_node(), &["call"]) {
            let Some(f) = call.child_by_field_name("function") else { continue };
            let positional: Vec<TsNode> = call
                .child_by_field_name("arguments")
                .map(named_children)
                .unwrap_or_default()
                .into_iter()
                .filter(|a| !matches!(a.kind(), "keyword_argument" | "comment"))
                .collect();
            let name_at = |i: usize| positional.get(i).filter(|a| a.kind() == "identifier").map(|a| text(*a, src).to_string());
            match (f.kind(), f.child_by_field_name("attribute").map(|a| text(a, src))) {
                ("attribute", Some("mount")) => {
                    let parent = f.child_by_field_name("object").map(|o| text(o, src).to_string());
                    let path = positional.first().filter(|a| a.kind() == "string").map(|a| unquote(text(*a, src)));
                    if let (Some(parent), Some(path), Some(child)) = (parent, path, name_at(1)) {
                        mounted.insert(key(&child), (key(&parent), path));
                    }
                }
                ("attribute", Some("include_router")) => {
                    let parent = f.child_by_field_name("object").map(|o| text(o, src).to_string());
                    if let (Some(parent), Some(child)) = (parent, name_at(0)) {
                        let prefix = lit_kw(call, src, "prefix").unwrap_or_default();
                        mounted.insert(key(&child), (key(&parent), prefix));
                    }
                }
                _ if text(f, src).ends_with("APIRouter") => {
                    let target = call.parent().filter(|p| p.kind() == "assignment").and_then(|a| a.child_by_field_name("left")).filter(|l| l.kind() == "identifier");
                    if let (Some(t), Some(prefix)) = (target, lit_kw(call, src, "prefix")) {
                        own.insert(key(text(t, src)), prefix);
                    }
                }
                _ => {}
            }
        }
    }
    // from the app or router a route is declared on, out to the app nobody mounts
    let full = |start: &str| {
        let trim = |p: &str| p.trim_end_matches('/').to_string();
        let (mut key, mut out) = (start.to_string(), trim(own.get(start).map(String::as_str).unwrap_or("")));
        for _ in 0..10 {
            let Some((parent, prefix)) = mounted.get(&key) else { break };
            out = format!("{}{}{out}", trim(own.get(parent).map(String::as_str).unwrap_or("")), trim(prefix));
            key = parent.clone();
        }
        out
    };
    for n in store.nodes.values_mut() {
        let Some(e) = n.entry.as_mut().filter(|e| e.kind == EntryKind::Route) else { continue };
        let Some(m) = modules.get(&n.module) else { continue };
        const VERBS: &[&str] = &["get(", "post(", "put(", "delete(", "patch(", "websocket("];
        let Some(recv) = n.decorators.iter().find_map(|d| d.split_once('.').filter(|(_, rest)| VERBS.iter().any(|v| rest.starts_with(v))).map(|(r, _)| r.to_string())) else { continue };
        let prefix = full(&m.bindings.get(&recv).cloned().unwrap_or_else(|| format!("{}.{recv}", m.id)));
        if prefix.is_empty() {
            continue;
        }
        if let Some((method, path)) = e.label.split_once(' ') {
            let path = if path == "/" { String::new() } else { path.to_string() };
            e.label = format!("{method} {}{path}", prefix.trim_end_matches('/'));
        }
    }
}

/// A keyword argument that is a string literal.
fn lit_kw(call: TsNode, src: &[u8], kw: &str) -> Option<String> {
    let args = named_children(call.child_by_field_name("arguments")?);
    let k = args.iter().find(|a| a.kind() == "keyword_argument" && a.child_by_field_name("name").map(|n| text(n, src)) == Some(kw))?;
    let v = k.child_by_field_name("value").filter(|v| v.kind() == "string")?;
    Some(unquote(text(v, src)))
}

// ---------- pass 1: symbols and imports ----------

fn collect_symbols(m: &mut Module, store: &mut GraphBuilder) {
    let root = m.tree.root_node();
    let src = &m.src;
    let file = crate::tree::path_str(&m.path);
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
    // a function or class under another name is that function or class: `graaf = triple`
    let renamed: Vec<(String, String)> = top_assignments(m)
        .into_iter()
        .filter_map(|a| {
            let (left, right) = (a.child_by_field_name("left")?, a.child_by_field_name("right")?);
            if left.kind() != "identifier" || !matches!(right.kind(), "identifier" | "attribute") || a.child_by_field_name("type").is_some() {
                return None;
            }
            let parts = dotted(Some(right), &m.src);
            let head = m.symbols.get(parts.first()?).or_else(|| m.bindings.get(&parts[0]))?;
            let name = text(left, &m.src).to_string();
            (!m.symbols.contains_key(&name)).then(|| (name, std::iter::once(head.clone()).chain(parts[1..].iter().cloned()).collect::<Vec<_>>().join(".")))
        })
        .collect();
    m.bindings.extend(renamed);
    for target in edges {
        import_edge(m, &target, module_ids, store, root);
    }
}

// ---------- pass 2: resolution ----------

/// Dotted name in module scope -> node id, `ext:<name>`, or None.
fn resolve(m: &Module, parts: &[String], modules: &IndexMap<String, Module>, nodes: &IndexMap<String, Node>, root: &str) -> Option<String> {
    // a shaped array is its dtype's type with the shape kept: `ext:jaxtyping.Float[Tensor, batch dim]`
    if let [one] = parts {
        if let Some((head, shape)) = one.split_once('[') {
            let base = resolve(m, &head.split('.').map(str::to_string).collect::<Vec<_>>(), modules, nodes, root)?;
            return Some(if base.starts_with("ext:jaxtyping.") { format!("{base}[{shape}") } else { base });
        }
    }
    let head = parts.first()?;
    let mut target = m.symbols.get(head).or_else(|| m.bindings.get(head))?.clone();
    for p in &parts[1..] {
        target = format!("{target}.{p}");
    }
    if target.starts_with(&format!("{root}.")) {
        return defined(&target, modules, nodes);
    }
    if nodes.contains_key(&target) {
        return Some(target);
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
    add_sums(modules, store, &aliases, root);
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
    // signatures -> takes / returns, every one before any body is read
    for id in &ids {
        let (kind, module) = (store.nodes[id].kind, store.nodes[id].module.clone());
        if !matches!(kind, NodeKind::Function | NodeKind::Method) {
            continue;
        }
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
    }
    // what functions give back, globals and attributes hold
    let known = type_values(modules, store, root);
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
    // bodies -> calls / instantiates / boundaries
    let mut handed = vec![];
    for id in &ids {
        if matches!(store.nodes[id].kind, NodeKind::Function | NodeKind::Method | NodeKind::Module) {
            handed.extend(link_calls(modules, store, &known, root, id));
        }
    }
    // the work handed to a managed transaction (`session.execute_read(work)`) goes its way
    for (work, kind, dir) in handed {
        if let Some(n) = store.nodes.get_mut(&work) {
            for t in n.touches.iter_mut().filter(|t| t.kind == kind && t.dir.is_empty()) {
                t.dir = dir.into();
            }
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

// ---------- the types of expressions ----------

/// What a value is, as far as the code says: an instance of a class of the package, a
/// collection of such instances (`list[Graph]`), a mapping to them (`dict[str, Graph]`), or a
/// value a library made (`requests.Session()`, `duckdb.connect(..)`), named by the library.
#[derive(Clone, Debug, PartialEq)]
enum Ty {
    Class(String),
    Many(String),
    Map(String),
    Lib(String),
}

/// What a call reaches: a function, method or class of the package, or a library.
enum Callee {
    Def(String),
    Lib(String),
}

/// Generic types whose parameter is what iterating them gives: `list[Graph]` is many Graphs.
const COLLECTIONS: &[&str] = &[
    "list", "List", "set", "Set", "frozenset", "FrozenSet", "Sequence", "MutableSequence", "Iterable", "Iterator", "Collection",
    "AbstractSet", "MutableSet", "Generator", "AsyncIterable", "AsyncIterator", "AsyncGenerator", "deque", "Deque",
];

/// Generic types that map keys to their last parameter: `dict[str, Graph]`.
const MAPPINGS: &[&str] = &["dict", "Dict", "Mapping", "MutableMapping", "defaultdict", "DefaultDict", "OrderedDict"];

/// Generic types that are one of their parameters: `Optional[Store]`, `Final[Store]`.
const ONE_OF: &[&str] = &["Optional", "Union", "Annotated", "Final", "ClassVar", "Required", "NotRequired", "ReadOnly"];

/// The library an external name belongs to: `ext:requests.Session` -> `requests`.
fn lib_of(resolved: &str) -> Option<String> {
    resolved.strip_prefix("ext:").map(|r| r.split(['.', '[']).next().unwrap_or(r).to_string())
}

/// An annotation's generic head and parameters: `list[Graph]` -> (`list`, [Graph]).
fn generic<'t>(n: TsNode<'t>, src: &[u8]) -> Option<(String, Vec<TsNode<'t>>)> {
    let unwrap = |x: TsNode<'t>| if x.kind() == "type" { named_children(x).into_iter().next().unwrap_or(x) } else { x };
    let (head, args) = match n.kind() {
        "generic_type" => {
            let kids = named_children(n);
            let args = kids.iter().find(|k| k.kind() == "type_parameter").map(|tp| named_children(*tp).into_iter().map(unwrap).collect()).unwrap_or_default();
            (dotted(kids.first().copied(), src), args)
        }
        "subscript" => (dotted(n.child_by_field_name("value"), src), field_children(n, "subscript").into_iter().map(unwrap).collect()),
        _ => return None,
    };
    Some((head.last()?.clone(), args))
}

/// The type an annotation gives a value: a class of the package or a library's value, many of
/// them (`list[Graph]`), or one of them (`Store | None`, `Optional[Store]`). A generic class
/// (`Holder[Node]`) is a value of that class; a mapping or a tuple has no one type.
fn annotated(m: &Module, t: TsNode, modules: &IndexMap<String, Module>, nodes: &IndexMap<String, Node>, root: &str) -> Option<Ty> {
    let src = &m.src;
    let t = if t.kind() == "type" { named_children(t).into_iter().next()? } else { t };
    match t.kind() {
        // `Store | None`: the side that names a type
        "binary_operator" => return named_children(t).into_iter().find_map(|x| annotated(m, x, modules, nodes, root)),
        "string" => {
            let lit = text(t, src).trim_matches(|c| c == '"' || c == '\'');
            let head = lit.split('[').next().unwrap_or("").rsplit('.').next().unwrap_or("");
            let many = lit.contains('[') && COLLECTIONS.contains(&head);
            let ty = quoted_values(lit).into_iter().find_map(|name| one(m, &name.split('.').map(str::to_string).collect::<Vec<_>>(), modules, nodes, root));
            return if many { ty.and_then(|t| if let Ty::Class(c) = t { Some(Ty::Many(c)) } else { None }) } else { ty };
        }
        _ => {}
    }
    if let Some((head, args)) = generic(t, src) {
        if COLLECTIONS.contains(&head.as_str()) {
            return match args.first().and_then(|a| annotated(m, *a, modules, nodes, root))? {
                Ty::Class(c) | Ty::Many(c) => Some(Ty::Many(c)),
                Ty::Map(_) | Ty::Lib(_) => None,
            };
        }
        if MAPPINGS.contains(&head.as_str()) {
            return match args.last().and_then(|a| annotated(m, *a, modules, nodes, root))? {
                Ty::Class(c) => Some(Ty::Map(c)),
                _ => None,
            };
        }
        if ONE_OF.contains(&head.as_str()) {
            return args.into_iter().find_map(|a| annotated(m, a, modules, nodes, root));
        }
        if CONTAINERS.contains(&head.as_str()) {
            return None;
        }
    }
    let name = match generic(t, src) {
        Some(_) => dotted(named_children(t).first().copied().or(t.child_by_field_name("value")), src),
        None => dotted(Some(t), src),
    };
    one(m, &name, modules, nodes, root)
}

/// A name in an annotation as a type: a class of the package, or a library's value.
fn one(m: &Module, name: &[String], modules: &IndexMap<String, Module>, nodes: &IndexMap<String, Node>, root: &str) -> Option<Ty> {
    if name.last().map_or(true, |l| BUILTIN_TYPES.contains(&l.as_str())) {
        return None;
    }
    let r = resolve(m, name, modules, nodes, root)?;
    match lib_of(&r) {
        Some(lib) => Some(Ty::Lib(lib)),
        None => nodes.get(&r).filter(|n| n.kind == NodeKind::Class).map(|_| Ty::Class(r)),
    }
}

/// What is typed before any body is read: what each function gives back (and, for a tuple, at
/// each place), what each module's globals and each class's attributes hold.
#[derive(Default)]
struct Known {
    returns: HashMap<String, Ty>,
    tuples: HashMap<String, Vec<Option<Ty>>>,
    globals: HashMap<String, HashMap<String, Ty>>,
    attrs: HashMap<String, HashMap<String, Ty>>,
}

/// Fill `Known`: return types from the annotations; then, twice so that a type found through
/// another settles, globals from their assignments and attributes from the class's annotated
/// fields and from `self.x = ..` in its methods, `__init__` first. An attribute that holds a
/// class of the package is an edge `has`.
fn type_values(modules: &IndexMap<String, Module>, store: &mut GraphBuilder, root: &str) -> Known {
    let mut known = Known::default();
    let defs: Vec<String> = store.nodes.values().filter(|n| matches!(n.kind, NodeKind::Function | NodeKind::Method)).map(|n| n.id.clone()).collect();
    for id in &defs {
        let n = &store.nodes[id];
        let m = &modules[&n.module];
        let Some(rt) = def_node(m, n).and_then(|d| d.child_by_field_name("return_type")) else { continue };
        if let Some(t) = annotated(m, rt, modules, &store.nodes, root) {
            known.returns.insert(id.clone(), t);
        }
        let inner = named_children(rt).into_iter().next().unwrap_or(rt);
        if let Some((head, args)) = generic(inner, &m.src).filter(|(h, _)| h == "tuple" || h == "Tuple") {
            let _ = head;
            known.tuples.insert(id.clone(), args.into_iter().map(|a| annotated(m, a, modules, &store.nodes, root)).collect());
        }
    }
    let mut methods: Vec<&String> = defs.iter().filter(|id| store.nodes[*id].cls.is_some()).collect();
    methods.sort_by_key(|id| store.nodes[*id].name != "__init__");
    let classes: Vec<String> = store.nodes.values().filter(|n| n.kind == NodeKind::Class).map(|n| n.id.clone()).collect();
    for _ in 0..2 {
        let mut globals = HashMap::new();
        let mut found: Vec<(String, String, Ty)> = vec![];
        {
            for (mid, m) in modules {
                let scope = Scope::new(m, modules, store, &known, root, None);
                let mut g = HashMap::new();
                for a in top_assignments(m) {
                    let Some(left) = a.child_by_field_name("left").filter(|l| l.kind() == "identifier") else { continue };
                    if let Some(t) = scope.assigned(a) {
                        g.insert(text(left, &m.src).to_string(), t);
                    }
                }
                globals.insert(mid.clone(), g);
            }
            // a class's annotated fields: `nodes: list[Node]`
            for cls in &classes {
                let n = &store.nodes[cls];
                let m = &modules[&n.module];
                let Some(body) = def_node(m, n).and_then(|d| d.child_by_field_name("body")) else { continue };
                for st in named_children(body).into_iter().filter(|s| s.kind() == "expression_statement") {
                    let Some(a) = named_children(st).into_iter().next().filter(|a| a.kind() == "assignment") else { continue };
                    let (Some(left), Some(t)) = (a.child_by_field_name("left").filter(|l| l.kind() == "identifier"), a.child_by_field_name("type")) else { continue };
                    if let Some(t) = annotated(m, t, modules, &store.nodes, root) {
                        found.push((cls.clone(), text(left, &m.src).to_string(), t));
                    }
                }
            }
            for id in &methods {
                let n = &store.nodes[*id];
                let m = &modules[&n.module];
                let Some(d) = def_node(m, n) else { continue };
                let scope = Scope::of_def(m, modules, store, &known, root, n, d);
                let Some(body) = d.child_by_field_name("body") else { continue };
                for a in walk(body, &["assignment"]) {
                    let Some(left) = a.child_by_field_name("left").filter(|l| l.kind() == "attribute") else { continue };
                    let lp = dotted(Some(left), &m.src);
                    if lp.len() != 2 || lp[0] != "self" {
                        continue;
                    }
                    if let Some(t) = scope.assigned(a) {
                        found.push((n.cls.clone().unwrap_or_default(), lp[1].clone(), t));
                    }
                }
            }
        }
        known.globals = globals;
        for (cls, attr, t) in found {
            let attrs = known.attrs.entry(cls.clone()).or_default();
            if attrs.contains_key(&attr) {
                continue;
            }
            attrs.insert(attr.clone(), t.clone());
            if let Ty::Class(c) | Ty::Many(c) | Ty::Map(c) = t {
                store.classes.entry(cls.clone()).or_default().attr_types.entry(attr.clone()).or_insert(c.clone());
                store.add_edge(&cls, &c, EdgeKind::Has, Some(attr));
            }
        }
    }
    known
}

/// The dotted path a call is written with; through a call it is made on, `X(..).method` is
/// `X.method`.
fn call_path(f: TsNode, src: &[u8]) -> Vec<String> {
    let parts = dotted(Some(f), src);
    if !parts.is_empty() || f.kind() != "attribute" {
        return parts;
    }
    let (Some(obj), Some(attr)) = (f.child_by_field_name("object"), f.child_by_field_name("attribute")) else { return vec![] };
    let mut inner = match obj.kind() {
        "call" => obj.child_by_field_name("function").map(|g| call_path(g, src)).unwrap_or_default(),
        _ => vec![],
    };
    if !inner.is_empty() {
        inner.push(text(attr, src).to_string());
    }
    inner
}

/// Where expressions are typed: one body, or a module's top level; the names bound there and
/// the class `self` belongs to. The type of an expression is composed from its parts: a name
/// has the type of its binding, `a.b` the type of `b` in the type of `a`, `f(..)` what `f`
/// makes or gives back, `a or b` the type of either.
struct Scope<'a> {
    m: &'a Module,
    modules: &'a IndexMap<String, Module>,
    store: &'a GraphBuilder,
    known: &'a Known,
    root: &'a str,
    cls: Option<String>,
    names: HashMap<String, Ty>,
}

impl<'a> Scope<'a> {
    fn new(m: &'a Module, modules: &'a IndexMap<String, Module>, store: &'a GraphBuilder, known: &'a Known, root: &'a str, cls: Option<String>) -> Self {
        Scope { m, modules, store, known, root, cls, names: HashMap::new() }
    }

    /// A function's scope: its annotated parameters, then what its body binds.
    fn of_def(m: &'a Module, modules: &'a IndexMap<String, Module>, store: &'a GraphBuilder, known: &'a Known, root: &'a str, n: &Node, d: TsNode) -> Self {
        let mut s = Scope::new(m, modules, store, known, root, n.cls.clone());
        if let Some(pn) = d.child_by_field_name("parameters") {
            for p in named_children(pn) {
                let name = p.child_by_field_name("name").or_else(|| named_children(p).into_iter().find(|c| c.kind() == "identifier"));
                if let (Some(name), Some(t)) = (name, p.child_by_field_name("type")) {
                    if let Some(t) = annotated(m, t, modules, &store.nodes, root) {
                        s.names.insert(text(name, &m.src).to_string(), t);
                    }
                }
            }
        }
        if let Some(body) = d.child_by_field_name("body") {
            s.bind(body);
        }
        s
    }

    fn is_class(&self, id: &str) -> bool {
        self.store.nodes.get(id).map_or(false, |n| n.kind == NodeKind::Class)
    }

    /// What an assignment gives its single target: its annotation, else its value.
    fn assigned(&self, a: TsNode) -> Option<Ty> {
        match a.child_by_field_name("type") {
            Some(t) => annotated(self.m, t, self.modules, &self.store.nodes, self.root),
            None => self.of(a.child_by_field_name("right")?),
        }
    }

    /// Bind what a body assigns, loops over and opens with `with`, twice so that a name typed
    /// from another settles.
    fn bind(&mut self, body: TsNode) {
        for _ in 0..2 {
            for a in walk(body, &["assignment", "named_expression", "for_statement", "for_in_clause", "as_pattern"]) {
                let bound = self.binds(a);
                self.names.extend(bound);
            }
        }
    }

    /// The names one binding gives a type: an assignment its targets, a loop its variable, a
    /// `with` its alias.
    fn binds(&self, a: TsNode) -> Vec<(String, Ty)> {
        let pairs: Vec<(Option<TsNode>, Option<Ty>)> = match a.kind() {
            "assignment" => match a.child_by_field_name("left") {
                Some(left) if matches!(left.kind(), "pattern_list" | "tuple_pattern") => {
                    let targets = named_children(left);
                    let types = a.child_by_field_name("right").map(|r| self.unpacked(r, targets.len())).unwrap_or_default();
                    targets.into_iter().map(Some).zip(types).collect()
                }
                left => vec![(left, self.assigned(a))],
            },
            "named_expression" => vec![(a.child_by_field_name("name"), a.child_by_field_name("value").and_then(|v| self.of(v)))],
            "for_statement" | "for_in_clause" => vec![(a.child_by_field_name("left"), a.child_by_field_name("right").and_then(|r| self.item(r)))],
            _ => vec![(a.child_by_field_name("alias").and_then(|t| named_children(t).into_iter().next()), named_children(a).into_iter().next().and_then(|v| self.entered(v)))],
        };
        pairs.into_iter().filter_map(|(n, t)| Some((text(n.filter(|n| n.kind() == "identifier")?, &self.m.src).to_string(), t?))).collect()
    }

    /// What `with v as x` gives `x`: what the class's `__enter__` gives back, else `v` itself.
    fn entered(&self, v: TsNode) -> Option<Ty> {
        let t = self.of(v)?;
        let enter = match &t {
            Ty::Class(c) => method_of(self.store, c, "__enter__", 0).and_then(|e| self.known.returns.get(&e).cloned()),
            _ => None,
        };
        enter.or(Some(t))
    }

    /// The types of `a, b = ..`, place by place: the elements of a tuple written out, or the
    /// places of a function's `-> tuple[A, B]`.
    fn unpacked(&self, right: TsNode, n: usize) -> Vec<Option<Ty>> {
        match right.kind() {
            "expression_list" | "tuple" => named_children(right).into_iter().map(|e| self.of(e)).collect(),
            "call" => match right.child_by_field_name("function").and_then(|f| self.callee(f)) {
                Some(Callee::Def(d)) => self.known.tuples.get(&d).cloned().unwrap_or_default(),
                _ => vec![None; n],
            },
            _ => vec![None; n],
        }
    }

    /// A name: bound in this scope, else a global of this module or of the one it is imported from.
    fn name(&self, n: &str) -> Option<Ty> {
        if matches!(n, "self" | "cls") {
            if let Some(c) = &self.cls {
                return Some(Ty::Class(c.clone()));
            }
        }
        if let Some(t) = self.names.get(n) {
            return Some(t.clone());
        }
        let (mut module, mut name) = (self.m.id.clone(), n.to_string());
        for _ in 0..8 {
            if let Some(t) = self.known.globals.get(&module).and_then(|g| g.get(&name)) {
                return Some(t.clone());
            }
            let target = self.modules.get(&module)?.bindings.get(&name)?;
            let (mid, nm) = target.rsplit_once('.')?;
            (module, name) = (mid.to_string(), nm.to_string());
        }
        None
    }

    /// The type of attribute `a` of a value of type `t`: declared on its class or on a base,
    /// or given by a property.
    fn attr(&self, t: &Ty, a: &str) -> Option<Ty> {
        let Ty::Class(c) = t else { return None };
        if let Some(t) = self.attr_of(c, a, 0) {
            return Some(t);
        }
        let p = method_of(self.store, c, a, 0).filter(|p| self.store.nodes[p].decorators.iter().any(|d| d.ends_with("property")))?;
        self.known.returns.get(&p).cloned()
    }

    fn attr_of(&self, cls: &str, a: &str, depth: u32) -> Option<Ty> {
        if depth > 8 {
            return None;
        }
        if let Some(t) = self.known.attrs.get(cls).and_then(|x| x.get(a)) {
            return Some(t.clone());
        }
        let bases = self.store.classes.get(cls).map(|c| c.resolved.clone()).unwrap_or_default();
        bases.iter().filter(|b| !b.starts_with("ext:")).find_map(|b| self.attr_of(b, a, depth + 1))
    }

    /// One item of a value: an element of a collection, or what a class's `__iter__` yields.
    fn item(&self, e: TsNode) -> Option<Ty> {
        match self.of(e)? {
            Ty::Many(c) => Some(Ty::Class(c)),
            Ty::Class(c) => match self.known.returns.get(&method_of(self.store, &c, "__iter__", 0)?)? {
                Ty::Many(x) => Some(Ty::Class(x.clone())),
                _ => None,
            },
            Ty::Map(_) | Ty::Lib(_) => None,
        }
    }

    /// The type of an expression.
    fn of(&self, e: TsNode) -> Option<Ty> {
        let src = &self.m.src;
        match e.kind() {
            "identifier" => self.name(text(e, src)),
            "attribute" => {
                let t = self.of(e.child_by_field_name("object")?)?;
                self.attr(&t, text(e.child_by_field_name("attribute")?, src))
            }
            "call" => self.from_mapping(e).or_else(|| self.made(e)),
            "subscript" => match self.of(e.child_by_field_name("value")?)? {
                Ty::Many(c) | Ty::Map(c) => Some(Ty::Class(c)),
                Ty::Class(c) => self.known.returns.get(&method_of(self.store, &c, "__getitem__", 0)?).cloned(),
                Ty::Lib(_) => None,
            },
            "boolean_operator" => named_children(e).into_iter().find_map(|x| self.of(x)),
            "conditional_expression" => {
                let k = named_children(e);
                k.first().and_then(|x| self.of(*x)).or_else(|| k.get(2).and_then(|x| self.of(*x)))
            }
            "parenthesized_expression" | "await" => named_children(e).into_iter().next().and_then(|x| self.of(x)),
            _ => None,
        }
    }

    /// What a mapping's own methods give: `d.get(k)` is one value, `d.values()` many.
    fn from_mapping(&self, call: TsNode) -> Option<Ty> {
        let f = call.child_by_field_name("function").filter(|f| f.kind() == "attribute")?;
        let Ty::Map(c) = self.of(f.child_by_field_name("object")?)? else { return None };
        match text(f.child_by_field_name("attribute")?, &self.m.src) {
            "get" | "pop" | "setdefault" => Some(Ty::Class(c)),
            "values" => Some(Ty::Many(c)),
            _ => None,
        }
    }

    /// What a call makes: an instance of the class it constructs, what a function gives back,
    /// or a value of the library it is named from (`requests.Session()`). What a method of a
    /// library's value gives back is not known.
    fn made(&self, call: TsNode) -> Option<Ty> {
        let f = call.child_by_field_name("function")?;
        match self.callee(f)? {
            Callee::Def(d) if self.is_class(&d) => Some(Ty::Class(d)),
            Callee::Def(d) => self.known.returns.get(&d).cloned(),
            Callee::Lib(lib) => self.named_from_library(f).then_some(Ty::Lib(lib)),
        }
    }

    /// Whether a call is written from a library's import (`requests.Session`, `HfApi`), not on
    /// a value.
    fn named_from_library(&self, f: TsNode) -> bool {
        let Some(head) = dotted(Some(f), &self.m.src).into_iter().next() else { return false };
        !self.names.contains_key(&head) && self.m.bindings.get(&head).map_or(false, |b| !b.starts_with(self.root))
    }

    /// The library a value belongs to when it hangs under a library's value:
    /// `client.chat.completions`. What a call on that value gives back is not the library's.
    fn lib_under(&self, mut e: TsNode) -> Option<String> {
        loop {
            e = match e.kind() {
                "attribute" => e.child_by_field_name("object")?,
                "subscript" => e.child_by_field_name("value")?,
                _ => return None,
            };
            if let Some(Ty::Lib(lib)) = self.of(e) {
                return Some(lib);
            }
        }
    }

    /// What a call reaches: a method of its receiver's class, the library its receiver belongs
    /// to, or what its dotted name resolves to.
    fn callee(&self, f: TsNode) -> Option<Callee> {
        let src = &self.m.src;
        if f.kind() == "attribute" {
            let (obj, name) = (f.child_by_field_name("object")?, text(f.child_by_field_name("attribute")?, src));
            match self.of(obj) {
                Some(Ty::Class(c)) => return method_of(self.store, &c, name, 0).map(Callee::Def),
                Some(Ty::Many(_) | Ty::Map(_)) => return None,
                Some(Ty::Lib(lib)) => return Some(Callee::Lib(lib)),
                None => {
                    if let Some(lib) = self.lib_under(obj) {
                        return Some(Callee::Lib(lib));
                    }
                }
            }
        }
        let parts = call_path(f, src);
        let head = parts.first()?;
        // a name this scope binds is a value, not a path into a module
        if self.names.contains_key(head) || (matches!(head.as_str(), "self" | "cls") && self.cls.is_some()) {
            return None;
        }
        match resolve(self.m, &parts, self.modules, &self.store.nodes, self.root) {
            Some(r) => Some(match lib_of(&r) {
                Some(lib) => Callee::Lib(lib),
                None => Callee::Def(r),
            }),
            // `Class.method` with the method on a base, or a constructor the class inherits
            None if parts.len() > 1 => {
                let owner = resolve(self.m, &parts[..parts.len() - 1], self.modules, &self.store.nodes, self.root).filter(|r| self.is_class(r))?;
                Some(Callee::Def(method_of(self.store, &owner, parts.last()?, 0).unwrap_or(owner)))
            }
            None => None,
        }
    }
}

/// What a package exports is a route into it: the names in `__all__` of its `__init__.py`,
/// else (at the root) the public names that module imports from the package or defines. A
/// public subpackage that declares `__all__` is a door too (`from vectormesh.components import
/// Serial`), labelled by its path from the root; a name the root exports keeps the root's label.
fn mark_exports(modules: &IndexMap<String, Module>, store: &mut GraphBuilder, root: &str) {
    let public = |id: &str| id.split('.').all(|p| !p.starts_with('_'));
    let mut packages: Vec<&Module> = modules.values().filter(|m| m.package == m.id && (m.id == root || (m.id.starts_with(&format!("{root}.")) && public(&m.id)))).collect();
    packages.sort_by_key(|m| (m.id != root, m.id.clone()));
    for m in packages {
        let declared = top_assignments(m).into_iter().find_map(|a| {
            let left = a.child_by_field_name("left").filter(|l| text(*l, &m.src) == "__all__")?;
            Some(literals(a.child_by_field_name("right").unwrap_or(left), &m.src, &["string"]))
        });
        let names: Vec<String> = match declared {
            Some(d) => d,
            None if m.id == root => m.symbols.keys().chain(m.bindings.keys()).filter(|n| !n.starts_with('_')).cloned().collect(),
            None => continue,
        };
        let path = m.id.strip_prefix(root).unwrap_or("").trim_start_matches('.');
        for name in names {
            let Some(t) = m.symbols.get(&name).or_else(|| m.bindings.get(&name)).filter(|t| t.starts_with(&format!("{root}."))) else { continue };
            let Some(t) = defined(t, modules, &store.nodes) else { continue };
            if let Some(n) = store.nodes.get_mut(&t) {
                if matches!(n.kind, NodeKind::Function | NodeKind::Class) && n.entry.is_none() {
                    let label = if path.is_empty() { name.clone() } else { format!("{path}.{name}") };
                    n.entry = Some(Entry { kind: EntryKind::Export, label });
                }
            }
        }
    }
}

/// A name followed through the packages that re-export it to where it is defined:
/// `vectormesh.VectorCache` -> `vectormesh.data.VectorCache` -> `vectormesh.data.cache.VectorCache`.
fn defined(target: &str, modules: &IndexMap<String, Module>, nodes: &IndexMap<String, Node>) -> Option<String> {
    let mut t = target.to_string();
    for _ in 0..8 {
        if nodes.contains_key(&t) {
            return Some(t);
        }
        let (modid, name) = t.rsplit_once('.')?;
        t = modules.get(modid)?.bindings.get(name)?.clone();
    }
    None
}

/// A module-level type alias: `Result = Union[FitResult, FailedFit]`, `Id = int | str`,
/// `T = TypeVar("T", bound="Base")`.
struct Alias {
    module: String,
    names: Vec<Vec<String>>,
    /// A union: the value is one of its names. A `TypeVar` or an `Optional` stands for them.
    sum: bool,
    span: Option<(u32, u32)>,
    text: String,
}

/// Module-level type aliases by full name.
fn type_aliases(modules: &IndexMap<String, Module>) -> HashMap<String, Alias> {
    let mut out = HashMap::new();
    for (mid, m) in modules {
        for a in top_assignments(m) {
            let (Some(left), Some(right)) = (a.child_by_field_name("left"), a.child_by_field_name("right")) else { continue };
            if left.kind() != "identifier" {
                continue;
            }
            let head = dotted(right.child_by_field_name("value").or_else(|| right.child_by_field_name("function")), &m.src).last().cloned().unwrap_or_default();
            let (names, sum) = match right.kind() {
                "subscript" if matches!(head.as_str(), "Union" | "Optional") => (type_names(Some(right), &m.src), head == "Union"),
                "binary_operator" if text(right, &m.src).contains('|') => (type_names(Some(right), &m.src), true),
                // `T = TypeVar("T", bound="TransformBase")` stands for its bound, `TypeVar("T", A, B)` for its constraints
                "call" if head == "TypeVar" => (typevar_names(right, &m.src), false),
                _ => continue,
            };
            let alias = Alias { module: mid.clone(), names, sum, span: span(a), text: text(right, &m.src).to_string() };
            out.insert(format!("{mid}.{}", text(left, &m.src)), alias);
        }
    }
    out
}

/// A union of two or more known types is a type of its own, a sum: a node with a `variant`
/// edge to each type it may be. Signatures that name the alias take or return the sum.
fn add_sums(modules: &IndexMap<String, Module>, store: &mut GraphBuilder, aliases: &HashMap<String, Alias>, root: &str) {
    let mut sums: Vec<(&String, &Alias)> = aliases.iter().filter(|(_, a)| a.sum).collect();
    sums.sort_by_key(|(id, _)| id.as_str());
    for (id, a) in sums {
        let m = &modules[&a.module];
        let mut variants: Vec<String> = vec![];
        for r in a.names.iter().filter_map(|n| resolve(m, n, modules, &store.nodes, root)) {
            if (!r.starts_with("ext:") || foreign_type(&r)) && !variants.contains(&r) {
                variants.push(r);
            }
        }
        if variants.len() < 2 {
            continue;
        }
        store.add_node(Node {
            id: id.clone(),
            kind: NodeKind::Class,
            name: id.rsplit('.').next().unwrap_or(id).to_string(),
            file: crate::tree::path_str(&m.path),
            span: a.span,
            module: a.module.clone(),
            doc: a.text.clone(),
            ..Default::default()
        });
        store.add_edge(&a.module, id, EdgeKind::Contains, None);
        for v in variants {
            if v.starts_with("ext:") {
                store.ext_node(&v);
            }
            store.add_edge(id, &v, EdgeKind::Variant, None);
        }
    }
}

/// What a `TypeVar` ranges over: its bound, else its constraints; a bound may be a string.
fn typevar_names(call: TsNode, src: &[u8]) -> Vec<Vec<String>> {
    let Some(args) = call.child_by_field_name("arguments") else { return vec![] };
    let mut out = vec![];
    for (i, a) in named_children(args).into_iter().enumerate() {
        let value = match a.kind() {
            "keyword_argument" if a.child_by_field_name("name").map(|n| text(n, src)) == Some("bound") => a.child_by_field_name("value"),
            "keyword_argument" => None,
            _ if i > 0 => Some(a),
            _ => None,
        };
        let Some(v) = value else { continue };
        if v.kind() == "string" {
            let lit = text(v, src).trim_matches(|c| c == '"' || c == '\'');
            out.push(lit.split('.').map(str::to_string).collect());
        } else {
            out.extend(type_names(Some(v), src));
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

/// What an alias stands for, resolved in the module that defines it: the sum it names, else
/// the classes it ranges over.
fn alias_targets(m: &Module, t: &[String], modules: &IndexMap<String, Module>, aliases: &HashMap<String, Alias>, nodes: &IndexMap<String, Node>, root: &str) -> Vec<String> {
    let [name] = t else { return vec![] };
    let full = m.bindings.get(name).cloned().unwrap_or_else(|| format!("{}.{name}", m.id));
    if nodes.contains_key(&full) {
        return vec![full];
    }
    let Some(a) = aliases.get(&full) else { return vec![] };
    a.names.iter().filter_map(|n| resolve(&modules[&a.module], n, modules, nodes, root)).collect()
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
        // a function is named in lower case; `RoutingControl.READ` is a constant, not a call to come
        if parts.last().map_or(true, |l| l.starts_with(char::is_uppercase)) {
            continue;
        }
        let Some(pkg) = parts.first().and_then(|h| m.bindings.get(h)).filter(|b| !b.starts_with(root)).and_then(|b| b.split('.').next()) else { continue };
        let Some(tag) = effects::boundary(Some(pkg), &parts, &m.ext_pkgs) else { continue };
        let op = parts.last().cloned().unwrap_or_default();
        let f = effects::facts(tag, Some(pkg), &op, &[]);
        out.push(Touch { kind: f.kind.into(), what: what.into(), op, artifact: f.artifact, dir: f.dir.into(), n: 1, ..Default::default() });
    }
    out
}

/// A body's calls, as edges and touches. Gives back the functions it hands to a managed
/// transaction, with the database and the direction the transaction says.
fn link_calls(modules: &IndexMap<String, Module>, store: &mut GraphBuilder, known: &Known, root: &str, id: &str) -> Vec<(String, String, &'static str)> {
    let n = store.nodes[id].clone();
    let m = &modules[&n.module];
    let Some(d) = def_node(m, &n) else { return vec![] };
    let src = &m.src;
    let in_module = n.kind == NodeKind::Module;
    let body = if in_module { Some(d) } else { d.child_by_field_name("body") };
    let Some(body) = body else { return vec![] };
    let mut handed: Vec<(String, String, &'static str)> = vec![];
    // the direction a body says to a database's driver, per database
    let mut declared: Vec<(String, &'static str)> = vec![];
    let params = if in_module { HashMap::new() } else { param_types(m, d, modules, &store.nodes, root) };
    let mut touches: Vec<Touch> = vec![];
    let mut new_edges: Vec<(String, EdgeKind, Vec<(u32, u32)>)> = vec![];
    let mut unresolved: Vec<String> = vec![];
    let choices = number_choices(body, src, &CHOICES);
    let mut changes: Vec<(String, String)> = vec![];
    {
        let scope = if in_module { Scope::new(m, modules, store, known, root, None) } else { Scope::of_def(m, modules, store, known, root, &n, d) };
        // the names that hold values of the package's classes, for what a call is handed
        let locals: HashMap<String, String> = scope
            .names
            .iter()
            .filter_map(|(k, t)| match t {
                Ty::Class(c) | Ty::Many(c) | Ty::Map(c) => Some((k.clone(), c.clone())),
                Ty::Lib(_) => None,
            })
            .collect();
        for c in evaluation_order(walk(body, &["call"])) {
            if in_module {
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
            let Some(fnode) = c.child_by_field_name("function") else { continue };
            let callee = scope.callee(fnode);
            if let Some(Callee::Def(t)) = &callee {
                let kind = if scope.is_class(t) { EdgeKind::Instantiates } else { EdgeKind::Calls };
                new_edges.push((t.clone(), kind, arms_of(c, body, src, &choices, &CHOICES)));
                continue;
            }
            let parts = call_path(fnode, src);
            if parts.is_empty() {
                continue;
            }
            unresolved.push(parts.iter().take(2).cloned().collect::<Vec<_>>().join("."));
            let lib = match callee {
                Some(Callee::Lib(lib)) => Some(lib),
                _ => None,
            };
            // a library's own call is classified by the library; a call on a value it made falls
            // back on the names every library shares (`encode`, `read_text`)
            let on_value = lib.is_some() && !scope.named_from_library(fnode);
            let tag = effects::boundary(lib.as_deref(), &parts, &m.ext_pkgs).or_else(|| on_value.then(|| effects::boundary(None, &parts, &m.ext_pkgs)).flatten());
            if let Some(tag) = tag {
                let op = parts.last().cloned().unwrap_or_default();
                let f = effects::facts(tag, lib.as_deref(), &op, &literals(c, src, &["string"]));
                // on a hub, what moves is the repository the call names
                let what = match f.kind == "net" && !f.artifact.is_empty() {
                    true => argument(c, src, "repo_id").unwrap_or_else(|| what_of(c, src, &["string"], "arguments")),
                    false => what_of(c, src, &["string"], "arguments"),
                };
                let payload = payload_of(c, src, &what, "arguments", &locals);
                let fixed = effects::locates(f.kind) && starts_at_literal(c, src, &["string"], "arguments", effects::on_receiver(&op));
                let said = effects::access(f.kind, &op, keyword(c, src, &["routing_", "default_access_mode"]).as_deref());
                if !said.is_empty() {
                    declared.push((f.kind.to_string(), said));
                    // the work a managed transaction is handed runs in its direction
                    if effects::transaction(&op) {
                        if let Some(Callee::Def(work)) = positional(c).and_then(|a| scope.callee(a)) {
                            handed.push((work, f.kind.to_string(), said));
                        }
                    }
                }
                for (artifact, dir) in f.places() {
                    let dir = if dir.is_empty() { said } else { dir };
                    touches.push(Touch { kind: f.kind.into(), what: what.clone(), payload: payload.clone(), op: op.clone(), artifact, dir: dir.into(), fixed, line: c.start_position().row as u32 + 1, n: 1 });
                }
            }
        }
        // what a body runs inside the session or transaction it opened goes the way it said
        for t in touches.iter_mut().filter(|t| t.dir.is_empty()) {
            let said: HashSet<&str> = declared.iter().filter(|(k, _)| *k == t.kind).map(|(_, d)| *d).collect();
            if let [dir] = said.into_iter().collect::<Vec<_>>()[..] {
                t.dir = dir.into();
            }
        }
        // what the body changes of its inputs: `T -> T` for the receiver or a parameter it changes;
        // a value the body constructs itself is built, not changed
        if !in_module {
            let receiver_mutates = |c: TsNode| match c.child_by_field_name("function").filter(|f| f.kind() == "attribute" && f.child_by_field_name("object").map_or(false, |o| o.kind() == "identifier")) {
                Some(f) => matches!(scope.callee(f), Some(Callee::Def(t)) if store.mutating.contains(&t)),
                None => false,
            };
            for r in changed_roots(body, src, &receiver_mutates) {
                let ty = if r == "self" { n.cls.clone().filter(|_| n.name != "__init__") } else { params.get(r).cloned() };
                if let Some(t) = ty {
                    changes.push((t, r.to_string()));
                }
            }
        }
    }
    for key in unresolved {
        *store.unresolved.entry(key).or_insert(0) += 1;
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
    for (t, kind, arms) in new_edges {
        store.add_call(id, &t, kind, None, arms);
    }
    if n.kind != NodeKind::Module {
        let compares = compares(body, src, &choices);
        if let Some(node) = store.nodes.get_mut(id) {
            node.compares = compares;
        }
    }
    for (t, input) in changes {
        store.add_edge(id, &t, EdgeKind::Mutates, Some(input));
    }
    for t in touches {
        store.touch(id, t);
    }
    handed
}

/// A call's first positional argument.
fn positional(call: TsNode) -> Option<TsNode> {
    named_children(call.child_by_field_name("arguments")?).into_iter().find(|a| !matches!(a.kind(), "keyword_argument" | "comment" | "list_splat" | "dictionary_splat"))
}

/// The text of the first of these keyword arguments a call is given.
fn keyword(call: TsNode, src: &[u8], names: &[&str]) -> Option<String> {
    let args = named_children(call.child_by_field_name("arguments")?);
    let k = args.iter().find(|a| a.kind() == "keyword_argument" && a.child_by_field_name("name").map_or(false, |n| names.contains(&text(n, src))))?;
    Some(text(k.child_by_field_name("value")?, src).to_string())
}

// ---------- choices ----------

/// Python's choices: `if`/`elif`/`else`, `match`, `try`/`except`, `a if c else b`, `x or y`.
const CHOICES: ChoiceSyntax = ChoiceSyntax {
    chooses: |n, src| match n.kind() {
        "if_statement" | "elif_clause" | "conditional_expression" | "try_statement" | "case_clause" => true,
        "boolean_operator" => n.child_by_field_name("operator").map(|o| text(o, src)) == Some("or"),
        // the cases of a `match` sit in its body block
        "block" => n.parent().map_or(false, |p| p.kind() == "match_statement"),
        _ => false,
    },
    continues: |n| if matches!(n.kind(), "elif_clause" | "case_clause") { n.parent() } else { None },
    part: |c, child, _| {
        let is = |f: &str| c.child_by_field_name(f).map(|x| x.id()) == Some(child.id());
        match c.kind() {
            "if_statement" if is("condition") => Part::Decides,
            "if_statement" if is("consequence") => Part::Arm(0),
            "if_statement" if child.kind() == "else_clause" => {
                Part::Arm(1 + named_children(c).iter().filter(|x| x.kind() == "elif_clause").count() as u32)
            }
            "elif_clause" if is("condition") => Part::Decides,
            "elif_clause" if is("consequence") => Part::Arm(1 + c.parent().map_or(0, |p| nth_of_kind(p, c))),
            "conditional_expression" => match named_children(c).iter().position(|x| x.id() == child.id()) {
                Some(0) => Part::Arm(0),
                Some(1) => Part::Decides,
                Some(2) => Part::Arm(1),
                _ => Part::Through,
            },
            "boolean_operator" if is("left") => Part::Arm(0),
            "boolean_operator" if is("right") => Part::Arm(1),
            "block" if child.kind() == "case_clause" => Part::Arm(nth_of_kind(c, child)),
            // a pattern and its guard decide which case runs
            "case_clause" if child.kind() == "case_pattern" || is("guard") => Part::Decides,
            "try_statement" if is("body") || child.kind() == "else_clause" => Part::Arm(0),
            "try_statement" if child.kind() == "except_clause" => Part::Arm(1 + nth_of_kind(c, child)),
            _ => Part::Through,
        }
    },
};

/// The string literals an operand names: `"a"`, or every element of `{"a", "b"}`.
fn string_cases(n: TsNode, src: &[u8]) -> Option<Vec<String>> {
    match n.kind() {
        "string" if n.child_by_field_name("interpolation").is_none() && !named_children(n).iter().any(|c| c.kind() == "interpolation") => Some(vec![unquote(text(n, src))]),
        "set" | "list" | "tuple" => {
            let items = named_children(n);
            (!items.is_empty() && items.iter().all(|i| i.kind() == "string")).then(|| items.iter().map(|i| unquote(text(*i, src))).collect())
        }
        _ => None,
    }
}

/// Where a body compares one value with string literals, and the choice each comparison decides.
fn compares(body: TsNode, src: &[u8], choices: &HashMap<usize, u32>) -> Vec<Compare> {
    let mut out = vec![];
    for n in walk(body, &["comparison_operator", "match_statement"]) {
        let line = n.start_position().row as u32 + 1;
        if n.kind() == "match_statement" {
            let (Some(subject), Some(block)) = (n.child_by_field_name("subject"), n.child_by_field_name("body")) else { continue };
            let cases: Vec<String> = walk(block, &["case_pattern"]).into_iter().flat_map(|p| walk(p, &["string"])).map(|s| unquote(text(s, src))).collect();
            if !cases.is_empty() {
                out.push(Compare { subject: text(subject, src).to_string(), cases, line, choice: choices.get(&block.id()).copied() });
            }
            continue;
        }
        let op = field_children(n, "operators").iter().map(|o| text(*o, src)).collect::<Vec<_>>().join(" ");
        let operands = named_children(n);
        if !matches!(op.as_str(), "==" | "!=" | "in" | "not in") || operands.len() != 2 {
            continue;
        }
        let (subject, cases) = match (string_cases(operands[0], src), string_cases(operands[1], src)) {
            (None, Some(c)) => (operands[0], c),
            (Some(c), None) => (operands[1], c),
            _ => continue,
        };
        if matches!(subject.kind(), "identifier" | "attribute" | "subscript") {
            out.push(Compare { subject: text(subject, src).to_string(), cases, line, choice: decides(n, body, src, choices, &CHOICES) });
        }
    }
    out
}

/// A foreign type worth keeping in a signature: bound to a third-party package, not a builtin.
fn foreign_type(id: &str) -> bool {
    let Some(full) = id.strip_prefix("ext:") else { return false };
    let head = full.split('.').next().unwrap_or("");
    full.contains('.') && !["typing", "collections", "pathlib", "builtins", "datetime", "abc", "os", "enum", "dataclasses", "functools", "itertools"].contains(&head)
}
