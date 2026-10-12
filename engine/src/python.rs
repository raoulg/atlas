//! `parse: Repo -> Graph` for Python packages.
//!
//! Two passes over tree-sitter trees: symbols and imports first, then resolution of
//! bases, signatures and calls into edges. The engine knows the language; it never
//! knows a project.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use indexmap::IndexMap;
use tree_sitter::{Node as TsNode, Parser, Tree};
use walkdir::WalkDir;

mod paths;

use crate::body::{Crosses, Step, Steps};
use crate::effects::{self, Crossing};
use crate::pytypes::{By, Facts, FileArg, Target, Ty, Types, Value};
use paths::{At, Built, Eval, Passed, Seg};
use crate::entries::entry_of;
use crate::model::{Dir, EdgeKind, Entry, EntryKind, Held, Node, NodeKind, Param, Returns, Touch, World};
pub use crate::builder::{foreign_type, ClassInfo, GraphBuilder, OwnTypes, SigInfo};
use crate::tree::{
    argument, evaluation_order, field_children, literals, named_children, nth_of_kind, payload_of, span, text, unquote, walk,
    what_of, ChoiceSyntax, CompareSyntax, Part,
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
    /// Where each definition is, by its first line and its name: its byte range.
    defs: std::cell::OnceCell<HashMap<(u32, String), (usize, usize)>>,
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

/// A signature as data: each parameter by name, type and kind, and what it gives back. A method's
/// first parameter is the value or the class it is called on, unless it is a `staticmethod`.
fn signature(f: TsNode, src: &[u8], receiver: bool) -> (Vec<Param>, Option<Returns>, SigInfo) {
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
            params.push(Param {
                name: pname.trim_start_matches('*').to_string(),
                ty: t.map(|t| text(t, src).to_string()).unwrap_or_default(),
                receiver: receiver && params.is_empty(),
                rest: pname.starts_with('*'),
                changes: false,
            });
            info.ptypes.extend(type_names(t, src));
        }
    }
    let rt = f.child_by_field_name("return_type");
    info.rtypes = type_names(rt, src);
    (params, rt.map(|r| returns(r, src)), info)
}

/// What a return annotation gives: `None` no value, `Self` a value of the class it runs on.
fn returns(rt: TsNode, src: &[u8]) -> Returns {
    let written = text(rt, src).to_string();
    let inner = if rt.kind() == "type" { named_children(rt).into_iter().next().unwrap_or(rt) } else { rt };
    let (value, own) = match inner.kind() {
        "none" => (String::new(), false),
        _ if written.rsplit('.').next() == Some("Self") => (String::new(), true),
        _ => (written.clone(), false),
    };
    Returns { written, value, own }
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
        defs: std::cell::OnceCell::new(),
    })
}

/// A notebook as the script it runs: its code cells in order, joined as ruff and ty join them,
/// so that a position in it is the same for both trees (a cell that starts with a cell magic
/// is left out, as ty leaves it out). A line for IPython (`%matplotlib`, `!pip install`) becomes
/// a comment of the same length, so the rest still parses as Python.
fn notebook_source(path: &Path) -> Result<String> {
    let nb = ruff_notebook::Notebook::from_path(path).map_err(|e| anyhow::anyhow!("parsing {}: {e}", path.display()))?;
    let lines: Vec<String> = nb
        .source_code()
        .split_inclusive('\n')
        .map(|l| {
            let indent = l.len() - l.trim_start().len();
            match l[indent..].starts_with(['%', '!']) {
                true => format!("{}#{}", &l[..indent], &l[indent + 1..]),
                false => l.to_string(),
            }
        })
        .collect();
    Ok(lines.concat())
}

/// The files of `named` (canonical) under `dir`, written from `dir` as the walk writes what it
/// finds.
pub fn named_under(dir: &Path, named: &[PathBuf]) -> Vec<PathBuf> {
    let Ok(d) = std::fs::canonicalize(dir) else { return vec![] };
    named.iter().filter_map(|f| f.strip_prefix(&d).ok().map(|r| dir.join(r))).collect()
}

/// The scripts and notebooks of a project: every `.py` and `.ipynb` outside its package, at any
/// depth, and the files named on the command line, even where git ignores them. Not its tests,
/// what git ignores, hidden directories or environments, and not another package or project
/// inside it (a directory with an `__init__.py` or a `pyproject.toml`).
fn scripts(root_dir: &Path, pkg_dir: Option<&Path>, named: &[PathBuf]) -> Vec<PathBuf> {
    const NOT: &[&str] = &["tests", "test", "node_modules", "venv", "env", "build", "dist", "site-packages", "__pycache__"];
    // without a package or a project file, the project is the files named
    if pkg_dir.is_none() && !root_dir.join("pyproject.toml").exists() {
        return named_under(root_dir, named);
    }
    let pkg = pkg_dir.map(|p| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())).unwrap_or_default();
    let mut out: Vec<PathBuf> = ignore::WalkBuilder::new(root_dir)
        .require_git(false)
        .git_global(false)
        .filter_entry(move |e| {
            let (p, name) = (e.path(), e.file_name().to_str().unwrap_or(""));
            let dir = e.file_type().map_or(false, |t| t.is_dir());
            e.depth() == 0
                || !dir
                || !(NOT.contains(&name) || std::fs::canonicalize(p).map_or(false, |c| c == pkg) || p.join("__init__.py").exists() || p.join("pyproject.toml").exists())
        })
        .build()
        .flatten()
        .map(|e| e.into_path())
        .filter(|p| p.is_file() && matches!(p.extension().and_then(|x| x.to_str()), Some("py" | "ipynb")))
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            !(name.starts_with("test_") || name.ends_with("_test.py") || name == "conftest.py")
        })
        .collect();
    let found: HashSet<PathBuf> = out.iter().filter_map(|p| std::fs::canonicalize(p).ok()).collect();
    out.extend(named_under(root_dir, named).into_iter().filter(|p| std::fs::canonicalize(p).map_or(false, |c| !found.contains(&c))));
    out.sort();
    out
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
pub fn parse(pkg_dir: Option<&Path>, project_root: Option<&Path>, opts: &crate::Options) -> Result<(GraphBuilder, String)> {
    // a project without a package is named after its folder
    let named = pkg_dir.or(project_root).context("no package and no project")?;
    let named = std::fs::canonicalize(named).unwrap_or_else(|_| named.to_path_buf());
    let root = named.file_name().and_then(|n| n.to_str()).context("package dir has no name")?.to_string();
    let mut parser = new_parser()?;
    let mut modules: IndexMap<String, Module> = IndexMap::new();
    let mut store = GraphBuilder::default();

    let mut files: Vec<PathBuf> = pkg_dir
        .into_iter()
        .flat_map(|d| {
            WalkDir::new(d).into_iter().filter_entry(|e| {
                let n = e.file_name().to_str().unwrap_or("");
                !(n.starts_with('.') || n == "__pycache__")
            })
        })
        .flatten()
        .filter(|e| e.file_type().is_file() && e.path().extension().and_then(|x| x.to_str()) == Some("py"))
        .map(|e| e.path().to_path_buf())
        .collect();
    files.sort();
    let base = pkg_dir.and_then(Path::parent).unwrap_or(Path::new(""));
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
    // the project's scripts and notebooks are routes into the package
    if let Some(root_dir) = project_root {
        for p in scripts(root_dir, pkg_dir, &opts.named) {
            let rel = p.strip_prefix(root_dir).unwrap_or(&p).to_path_buf();
            let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("script").to_string();
            // a dot in a file's name is no module boundary
            let dirs: Vec<String> = rel.parent().map(|d| d.components().map(|c| c.as_os_str().to_string_lossy().replace('.', "_")).collect()).unwrap_or_default();
            let mid = match dirs.is_empty() {
                false => format!("{}.{}", dirs.join("."), stem.replace('.', "_")),
                true => format!("_root.{}", stem.replace('.', "_")),
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

    let types = read_libraries(&mut parser, &mut modules, &mut store, project_root.unwrap_or(base), opts.libraries)?;
    for m in modules.values_mut() {
        collect_symbols(m, &mut store);
    }
    let module_ids: HashSet<String> = modules.keys().cloned().collect();
    for m in modules.values_mut() {
        collect_imports(m, &module_ids, &mut store, &root);
        store.bindings.insert(m.id.clone(), m.bindings.clone());
    }
    link(&modules, &mut store, &root, &types, project_root.unwrap_or(base));
    mark_exports(&modules, &mut store, &root);
    route_prefixes(&modules, &mut store);
    keep_reached(&mut store);
    Ok((store, root))
}

/// The libraries the project's code calls, read as code as far as those calls reach into them,
/// `depth` rounds deep: the library modules the code's calls go into, then the modules
/// of the same library their own calls go into. What a library calls in a module not read, or
/// in another library, stays a library's call, as the project's own calls into an unread
/// library do. A library is read
/// only where its source can be followed: not the standard library (ty knows it by its stubs),
/// not a package the effect tables describe, and not one with compiled code. Returns ty's
/// answers for every module read.
fn read_libraries(parser: &mut Parser, modules: &mut IndexMap<String, Module>, store: &mut GraphBuilder, root_dir: &Path, depth: usize) -> Result<Types> {
    let entry = |m: &Module| (crate::tree::path_str(&m.path), m.path.clone());
    let mut files: Vec<(String, PathBuf)> = modules.values().map(entry).collect();
    let Some(ty) = Ty::new(root_dir) else { return Ok(Types::default()) };
    let mut types = ty.read(&files, &files);
    let mut reached = types.libs.clone();
    let used: HashSet<String> = reached.iter().map(|(m, _)| package_of(m)).collect();
    let mut readable = Readable::default();
    let here = std::fs::canonicalize(root_dir).unwrap_or_else(|_| root_dir.to_path_buf());
    let project = files.len();
    for round in 1..=depth {
        let mut fresh = vec![];
        for (mid, path) in std::mem::take(&mut reached) {
            if modules.contains_key(&mid) || !used.contains(&package_of(&mid)) || !readable.reads(&mid, &path) {
                continue;
            }
            // a library in the project's environment is named from the project, like its own files
            let shown = path.strip_prefix(&here).map(|r| root_dir.join(r)).unwrap_or_else(|_| path.clone());
            let is_pkg = path.file_stem().and_then(|s| s.to_str()) == Some("__init__");
            let Ok(m) = load_module(parser, &mid, &shown, is_pkg) else { continue };
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
            reached = ty.read(&fresh, &fresh).libs;
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
        if crate::effects::described(&package_of(mid)) {
            return false;
        }
        let is_pkg = path.file_stem().and_then(|s| s.to_str()) == Some("__init__");
        let Some(top) = path.ancestors().nth(parts - 1 + usize::from(is_pkg)).map(Path::to_path_buf) else { return false };
        let compiled = self.compiled.entry(top.clone()).or_insert_with(|| {
            WalkDir::new(&top).into_iter().flatten().any(|e| matches!(e.path().extension().and_then(|x| x.to_str()), Some("so" | "pyd" | "dylib")))
        });
        !*compiled
    }
}

/// A library read as code keeps what the project reaches in it: the functions and classes its
/// calls, constructions, signatures and fields lead to, with the bases and constructors of the
/// classes, and the modules that hold them. The rest of a module read for one call is dropped.
fn keep_reached(store: &mut GraphBuilder) {
    let library: HashSet<String> = store.nodes.values().filter(|n| n.library || store.nodes.get(&n.module).map_or(false, |m| m.library)).map(|n| n.id.clone()).collect();
    if library.is_empty() {
        return;
    }
    for id in &library {
        if let Some(n) = store.nodes.get_mut(id) {
            n.library = true;
            n.entry = None;
        }
    }
    let follows = |k: EdgeKind| matches!(k, EdgeKind::Calls | EdgeKind::Instantiates | EdgeKind::Takes | EdgeKind::Returns | EdgeKind::Has | EdgeKind::Mutates | EdgeKind::Inherits | EdgeKind::Variant);
    let mut out: HashMap<&str, Vec<&str>> = HashMap::new();
    for e in store.edges.iter().filter(|e| follows(e.kind)) {
        out.entry(e.src.as_str()).or_default().push(e.dst.as_str());
    }
    let bases: HashMap<&str, Vec<&str>> = store.edges.iter().filter(|e| e.kind == EdgeKind::Inherits).fold(HashMap::new(), |mut m, e| {
        m.entry(e.src.as_str()).or_default().push(e.dst.as_str());
        m
    });
    let ancestors = |c: &str| -> Vec<String> {
        let (mut out, mut todo) = (vec![], vec![c.to_string()]);
        while let Some(x) = todo.pop() {
            for b in bases.get(x.as_str()).into_iter().flatten() {
                if !out.iter().any(|o: &String| o == b) && out.len() < 32 {
                    out.push(b.to_string());
                    todo.push(b.to_string());
                }
            }
        }
        out
    };
    let mut kept: HashSet<String> = HashSet::new();
    let mut stack: Vec<String> = store.nodes.keys().filter(|id| !library.contains(*id)).cloned().collect();
    loop {
        while let Some(id) = stack.pop() {
            let mut next: Vec<String> = out.get(id.as_str()).map(|v| v.iter().map(|s| s.to_string()).collect()).unwrap_or_default();
            // a class built is built by its constructor
            if store.nodes.get(&id).map_or(false, |n| n.kind == NodeKind::Class) {
                next.push(format!("{id}.__init__"));
            }
            for d in next {
                if library.contains(&d) && store.nodes.contains_key(&d) && kept.insert(d.clone()) {
                    stack.push(d);
                }
            }
        }
        // a class kept answers the calls into its bases with its own methods
        for c in kept.iter().filter(|id| store.nodes.get(*id).map_or(false, |n| n.kind == NodeKind::Class)) {
            for b in ancestors(c) {
                for m in store.nodes.values().filter(|n| n.cls.as_deref() == Some(b.as_str()) && (kept.contains(&n.id) || !library.contains(&n.id))) {
                    let own = format!("{c}.{}", m.name);
                    if library.contains(&own) && store.nodes.contains_key(&own) && !kept.contains(&own) {
                        stack.push(own);
                    }
                }
            }
        }
        if stack.is_empty() {
            break;
        }
        kept.extend(stack.iter().cloned());
    }
    // what is kept stands in its class and its module
    let holders: Vec<String> = kept.iter().filter_map(|id| store.nodes.get(id)).flat_map(|n| [n.cls.clone(), Some(n.module.clone())]).flatten().collect();
    kept.extend(holders);
    let drop: HashSet<String> = library.into_iter().filter(|id| !kept.contains(id)).collect();
    store.nodes.retain(|id, _| !drop.contains(id));
    store.edges.retain(|e| !drop.contains(&e.src) && !drop.contains(&e.dst));
    store.edge_set.retain(|(a, b, _)| !drop.contains(a) && !drop.contains(b));
    let files: HashSet<String> = store.nodes.values().map(|n| n.file.clone()).collect();
    store.files.retain(|f, _| files.contains(f));
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
    // each route by the app or router it is declared on: `@router.get(..)`
    const VERBS: &[&str] = &["get(", "post(", "put(", "delete(", "patch(", "websocket("];
    let routes: Vec<(String, String)> = store
        .nodes
        .values()
        .filter(|n| n.entry.as_ref().map_or(false, |e| e.kind == EntryKind::Route))
        .filter_map(|n| {
            let m = modules.get(&n.module)?;
            let recv = n.decorators.iter().find_map(|d| d.split_once('.').filter(|(_, rest)| VERBS.iter().any(|v| rest.starts_with(v))).map(|(r, _)| r.to_string()))?;
            Some((n.id.clone(), m.bindings.get(&recv).cloned().unwrap_or_else(|| format!("{}.{recv}", m.id))))
        })
        .collect();
    crate::entries::mount_routes(store, &routes, &own, &mounted, str::to_string);
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
                    let on_class = mdecs.iter().any(|d| matches!(d.as_str(), "classmethod" | "staticmethod"));
                    let (params, returns, sig) = signature(md, src, !mdecs.iter().any(|d| d == "staticmethod"));
                    let gives_class = returns.as_ref().map_or(false, |r| r.own || r.value.trim_matches(['"', '\'']) == name);
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
                        on_class,
                        ctor: mname == "__init__" || on_class && gives_class,
                        private: mname.starts_with('_'),
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
            store.classes.insert(nid.clone(), ClassInfo { bases: bases.clone(), fields, ..Default::default() });
            let private = name.starts_with('_');
            store.add_node(Node {
                id: nid.clone(),
                kind: NodeKind::Class,
                name,
                file: file.clone(),
                span,
                module: m.id.clone(),
                doc: first_docline(body, src),
                decorators: decs,
                private,
                bases: bases.iter().map(|b| b.join(".")).collect(),
                methods,
                ..Default::default()
            });
            store.add_edge(&m.id, &nid, EdgeKind::Contains, None);
        } else {
            let (params, returns, sig) = signature(d, src, false);
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
                private: name.starts_with('_'),
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


/// The definition node of a symbol, found again by its line and name.
fn def_node<'t>(m: &'t Module, n: &Node) -> Option<TsNode<'t>> {
    if n.kind == NodeKind::Module {
        return Some(m.tree.root_node());
    }
    let (start, _) = n.span?;
    let defs = m.defs.get_or_init(|| {
        walk(m.tree.root_node(), &["function_definition", "class_definition"])
            .into_iter()
            .filter_map(|d| Some(((d.start_position().row as u32 + 1, text(d.child_by_field_name("name")?, &m.src).to_string()), (d.start_byte(), d.end_byte()))))
            .collect()
    });
    let &(a, b) = defs.get(&(start, n.name.clone()))?;
    let mut d = m.tree.root_node().descendant_for_byte_range(a, b)?;
    while !(matches!(d.kind(), "function_definition" | "class_definition") && d.start_byte() == a && d.end_byte() == b) {
        d = d.parent()?;
    }
    Some(d)
}

fn link(modules: &IndexMap<String, Module>, store: &mut GraphBuilder, root: &str, types: &Types, root_dir: &Path) {
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
                    if !foreign_type(&r, &OWN) {
                        continue;
                    }
                    store.ext_node(&r);
                }
                store.add_edge(id, &r, kind, None);
            }
        }
    }
    // what the attributes hold, as ty types what the methods assign them
    let sites = sites(modules, store);
    attributes(modules, store, types, &sites, root);
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
                obj == Some("self") && store.method_of(&cls, name).map_or(false, |t| store.mutating.contains(&t))
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
    let eval = Eval::new(modules, types, root_dir);
    // a library's class by its package and its name, as the signatures name it
    let ext: HashMap<(String, String), String> = store
        .nodes
        .keys()
        .filter(|k| k.starts_with("ext:") && !k.contains('['))
        .filter_map(|k| Some(((package_of(&k[4..]), k.rsplit('.').next()?.to_string()), k.clone())))
        .collect();
    let mut handed = vec![];
    let mut files: Vec<(String, Touch, Vec<Built>)> = vec![];
    let mut args: Vec<Arg> = vec![];
    for id in &ids {
        if matches!(store.nodes[id].kind, NodeKind::Function | NodeKind::Method | NodeKind::Module) {
            let linked = link_calls(modules, store, types, &sites, &ext, &eval, root, id);
            handed.extend(linked.handed);
            files.extend(linked.files.into_iter().map(|(t, b)| (id.clone(), t, b)));
            args.extend(linked.args);
        }
    }
    place_files(modules, store, &eval, files, &args);
    // the work handed to a managed transaction (`session.execute_read(work)`) goes its way
    for (work, kind, dir) in handed {
        if let Some(n) = store.nodes.get_mut(&work) {
            for t in n.touches.iter_mut().filter(|t| t.kind == kind && t.dir.is_none()) {
                t.dir = Some(dir);
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
fn changed_roots<'a>(body: TsNode, src: &'a [u8], mutating: &dyn Fn(TsNode) -> bool) -> std::collections::BTreeSet<&'a str> {
    let mut out = std::collections::BTreeSet::new();
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

// ---------- the types of expressions, from ty ----------

/// What a call reaches: a function, method or class of the package, or a library, by its
/// package.
enum Callee {
    Def(String),
    Lib(String),
}

/// The library an external name belongs to: `ext:requests.Session` -> `requests`.
fn lib_of(resolved: &str) -> Option<String> {
    resolved.strip_prefix("ext:").map(|r| r.split(['.', '[']).next().unwrap_or(r).to_string())
}

/// The package a library's module belongs to: `neo4j._sync.work.session` -> `neo4j`. A module
/// named with one underscore is the implementation behind the module without it: `_csv` is
/// `csv`, and `_duckdb` (from its stubs `_duckdb-stubs`) is `duckdb`.
fn package_of(module: &str) -> String {
    let head = module.split('.').next().unwrap_or(module);
    head.strip_prefix('_').filter(|h| !h.is_empty() && !h.starts_with('_')).unwrap_or(head).to_string()
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

/// The nodes of the package's definitions, by where their names are written: the file and the
/// byte offset. This is where ty's answers meet the graph.
fn sites(modules: &IndexMap<String, Module>, store: &GraphBuilder) -> HashMap<(String, usize), String> {
    let mut out = HashMap::new();
    for n in store.nodes.values().filter(|n| matches!(n.kind, NodeKind::Function | NodeKind::Method | NodeKind::Class)) {
        let Some(m) = modules.get(&n.module) else { continue };
        if let Some(name) = def_node(m, n).and_then(|d| d.child_by_field_name("name")) {
            out.insert((crate::tree::path_str(&m.path), name.start_byte()), n.id.clone());
        }
    }
    out
}

/// What ty says about one module's expressions, in the graph's terms: the node or library a
/// call reaches, and the type a value has.
struct Seen<'a> {
    m: &'a Module,
    facts: Option<&'a Facts>,
    sites: &'a HashMap<(String, usize), String>,
    modules: &'a IndexMap<String, Module>,
    store: &'a GraphBuilder,
    root: &'a str,
}

impl<'a> Seen<'a> {
    fn new(m: &'a Module, types: &'a Types, sites: &'a HashMap<(String, usize), String>, modules: &'a IndexMap<String, Module>, store: &'a GraphBuilder, root: &'a str) -> Self {
        Seen { m, facts: types.of(&crate::tree::path_str(&m.path)), sites, modules, store, root }
    }

    fn range(n: TsNode) -> (usize, usize) {
        (n.start_byte(), n.end_byte())
    }

    fn is_class(&self, id: &str) -> bool {
        self.store.nodes.get(id).map_or(false, |n| n.kind == NodeKind::Class)
    }

    /// The node a definition is, by where its name is written.
    fn node(&self, t: &Target) -> Option<&'a String> {
        match t {
            Target::Here { file, at } => self.sites.get(&(file.clone(), *at)),
            Target::Lib(_) => None,
        }
    }

    /// What calling `f` reaches. ty answers first. A library's call is named by the package the
    /// code names it from, when it does (`sys.stdout.write` is `sys`'s), else by the package
    /// that defines it. Where ty knows nothing, a dotted name through what the module imports or
    /// defines is what that binds: without an environment, `requests.get` is still `get` of the
    /// module `requests`.
    fn callee(&self, f: TsNode) -> Option<Callee> {
        let reached = self.facts.map_or(&[][..], |x| x.callees(Self::range(f)));
        let mut lib = None;
        for c in reached {
            let def = match (&c.target, c.by) {
                (Target::Lib(m), _) => {
                    lib.get_or_insert_with(|| package_of(m));
                    None
                }
                (t, By::Instance) => self.node(t).and_then(|cls| self.store.method_of(cls, "__call__")),
                (t, By::Call | By::Construct) => self.node(t).cloned(),
                (t, By::Name) => self.node(t).filter(|id| self.store.nodes.get(*id).map_or(false, |n| matches!(n.kind, NodeKind::Function | NodeKind::Method | NodeKind::Class))).cloned(),
            };
            if let Some(d) = def {
                return Some(Callee::Def(d));
            }
        }
        if let Some(lib) = lib {
            return Some(Callee::Lib(match self.bound(f) {
                Some(Callee::Lib(named)) => named,
                _ => lib,
            }));
        }
        if !reached.is_empty() {
            return None;
        }
        self.bound(f)
    }

    /// The function of the package a value handed on is called as: a function or method, or an
    /// instance's `__call__`. Reading a property hands nothing on.
    fn handed_function(&self, a: TsNode) -> Option<String> {
        let reached = self.facts.map_or(&[][..], |x| x.callees(Self::range(a)));
        let function = |id: &&String| self.store.nodes.get(*id).map_or(false, |n| n.kind == NodeKind::Function);
        for c in reached {
            let def = match c.by {
                By::Call => self.node(&c.target).cloned(),
                By::Instance => self.node(&c.target).and_then(|cls| self.store.method_of(cls, "__call__")),
                By::Name => self.node(&c.target).filter(function).cloned(),
                By::Construct => None,
            };
            if def.is_some() {
                return def;
            }
        }
        match self.bound(a) {
            Some(Callee::Def(t)) if reached.is_empty() && function(&&t) => Some(t),
            _ => None,
        }
    }

    /// The types of a value that the picture draws: a class of the code Atlas reads, or a
    /// library's class by its package and name, as a signature names it (`ext:pandas.DataFrame`).
    fn drawn(&self, v: &Value, ext: &HashMap<(String, String), String>) -> Vec<String> {
        let mut out: Vec<String> = vec![];
        for (t, name) in v.classes.iter().zip(&v.names) {
            let id = match t {
                Target::Here { .. } => self.node(t).filter(|id| self.is_class(id)).cloned(),
                Target::Lib(module) => {
                    let pkg = package_of(module);
                    let id = ext.get(&(pkg.clone(), name.clone())).cloned().unwrap_or_else(|| format!("ext:{pkg}.{name}"));
                    foreign_type(&id, &OWN).then_some(id)
                }
            };
            if let Some(id) = id.filter(|id| !out.contains(id)) {
                out.push(id);
            }
        }
        out
    }

    /// What an expression hands a call: the value a call in it gives, and the names in it with
    /// the values they hold (`data[before]` hands `data`). A class or a function named is no
    /// value, and `self` is the call's receiver, not something handed.
    fn held_in(&self, e: TsNode, ext: &HashMap<(String, String), String>, made: &HashSet<String>) -> Vec<Held> {
        let mut out: Vec<Held> = vec![];
        let mut add = |ty: String, name: String, at: usize| {
            if !out.iter().any(|h| h.ty == ty && h.name == name) {
                out.push(Held { ty, name, at: at as u32 });
            }
        };
        let data = |v: &&Value| !v.is_class && !v.is_code;
        if e.kind() == "call" {
            for ty in self.value(e).filter(data).map(|v| self.drawn(v, ext)).unwrap_or_default() {
                add(ty, String::new(), e.start_byte());
            }
        }
        for i in walk(e, &["identifier"]) {
            let name = text(i, &self.m.src);
            if matches!(name, "self" | "cls") {
                continue;
            }
            let v = self.value(i);
            if v.map_or(false, |v| v.is_class || v.is_code) {
                continue;
            }
            let tys = v.map(|v| self.drawn(v, ext)).unwrap_or_default();
            // a name whose type is not known is still a value the body made, when an earlier step made it
            if tys.is_empty() && made.contains(name) {
                add(String::new(), name.to_string(), i.start_byte());
            }
            for ty in tys {
                add(ty, name.to_string(), i.start_byte());
            }
        }
        // a value known by name is not also a nameless one
        let named: HashSet<String> = out.iter().filter(|h| !h.name.is_empty()).map(|h| h.ty.clone()).collect();
        out.retain(|h| !h.name.is_empty() || !named.contains(&h.ty));
        out
    }

    /// Whether class `c` is `b` or descends from it.
    fn descends(&self, c: &str, b: &str) -> bool {
        let mut todo = vec![c.to_string()];
        let mut seen = HashSet::new();
        while let Some(x) = todo.pop() {
            if x == b {
                return true;
            }
            if seen.insert(x.clone()) {
                todo.extend(self.store.classes.get(&x).map(|i| i.resolved.clone()).unwrap_or_default());
            }
        }
        false
    }

    /// A class of the code Atlas reads, handed on as a value.
    fn handed_class(&self, a: TsNode) -> Option<String> {
        let reached = self.facts.map_or(&[][..], |x| x.callees(Self::range(a)));
        reached.iter().filter(|c| c.by == By::Construct).find_map(|c| self.node(&c.target).filter(|t| self.is_class(t)).cloned())
    }

    /// Whether a library call that its package's table marks as reaching a place does move data,
    /// as its types say: building a client (a web or LLM library's class) moves nothing, and a
    /// network library's call moves data only when it is handed a URL or sends on a socket. Where
    /// the types are not known, the table stands.
    fn moves(&self, f: TsNode, kind: &Crossing, lib: Option<&str>, op: &str) -> bool {
        let reached = self.facts.map_or(&[][..], |x| x.callees(Self::range(f)));
        if lib.map_or(false, effects::client) && reached.iter().any(|c| c.by == By::Construct && matches!(c.target, Target::Lib(_))) {
            return false;
        }
        match self.facts.and_then(|x| x.signature(Self::range(f))) {
            Some(sig) if kind.world == World::Net && lib.map_or(false, effects::network) => effects::requests(&sig.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), op),
            _ => true,
        }
    }

    /// What a dotted name reaches through what the module imports or defines.
    fn bound(&self, f: TsNode) -> Option<Callee> {
        let parts = call_path(f, &self.m.src);
        let head = parts.first()?;
        if !self.m.bindings.contains_key(head) && !self.m.symbols.contains_key(head) {
            return None;
        }
        let r = resolve(self.m, &parts, self.modules, &self.store.nodes, self.root)?;
        Some(match lib_of(&r) {
            Some(lib) => Callee::Lib(lib),
            None => Callee::Def(r),
        })
    }

    /// The type ty gives the expression `e`.
    fn value(&self, e: TsNode) -> Option<&'a Value> {
        self.facts?.value(Self::range(e))
    }

    /// The classes of the package a type names: `list[Node]` -> `Node`.
    fn classes(&self, v: &Value) -> Vec<String> {
        v.classes.iter().filter_map(|t| self.node(t)).filter(|id| self.is_class(id)).cloned().collect()
    }

    /// The class of the package a library's classmethod makes when called on it:
    /// `Node.model_validate(row)` gives back a `Node`, so it makes one.
    fn made_on(&self, call: TsNode, f: TsNode) -> Option<String> {
        let on = self.value(f.child_by_field_name("object")?).filter(|v| v.is_class)?;
        let made = self.value(call)?;
        let cls = self.classes(on).into_iter().next()?;
        (made.classes.len() == 1 && self.classes(made) == [cls.clone()]).then_some(cls)
    }

    /// The names a call is handed that hold the package's types, with the type ty gives them.
    fn handed(&self, call: TsNode) -> HashMap<String, String> {
        let Some(args) = call.child_by_field_name("arguments") else { return HashMap::new() };
        walk(args, &["identifier"])
            .into_iter()
            .filter_map(|id| {
                let v = self.value(id)?;
                (!self.classes(v).is_empty()).then(|| (text(id, &self.m.src).to_string(), v.shown.clone()))
            })
            .collect()
    }
}

/// What a class's attributes hold, as its methods assign them (`self.store = Store(..)`). An
/// attribute whose type names a class of the package is an edge `has`.
fn attributes(modules: &IndexMap<String, Module>, store: &mut GraphBuilder, types: &Types, sites: &HashMap<(String, usize), String>, root: &str) {
    let mut found: Vec<(String, String, String)> = vec![];
    for n in store.nodes.values().filter(|n| n.kind == NodeKind::Method) {
        let (Some(cls), Some(m)) = (n.cls.clone(), modules.get(&n.module)) else { continue };
        let Some(body) = def_node(m, n).and_then(|d| d.child_by_field_name("body")) else { continue };
        let seen = Seen::new(m, types, sites, modules, store, root);
        for a in walk(body, &["assignment"]) {
            let Some(left) = a.child_by_field_name("left").filter(|l| l.kind() == "attribute") else { continue };
            let lp = dotted(Some(left), &m.src);
            if lp.len() != 2 || lp[0] != "self" {
                continue;
            }
            let Some(v) = a.child_by_field_name("right").and_then(|r| seen.value(r)) else { continue };
            for c in seen.classes(v) {
                found.push((cls.clone(), lp[1].clone(), c));
            }
        }
    }
    for (cls, attr, c) in found {
        store.classes.entry(cls.clone()).or_default().attr_types.entry(attr.clone()).or_insert(c.clone());
        store.add_edge(&cls, &c, EdgeKind::Has, Some(attr));
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
            store.export(&t, if path.is_empty() { name.clone() } else { format!("{path}.{name}") });
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
            if (!r.starts_with("ext:") || foreign_type(&r, &OWN)) && !variants.contains(&r) {
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
        // `openai.api_key = ..` assigns the attribute; it names no function
        let assigned = a.parent().map_or(false, |p| p.kind() == "assignment" && p.child_by_field_name("left").map(|l| l.id()) == Some(a.id()));
        if called || assigned || a.parent().map_or(false, |p| p.kind() == "attribute") {
            continue;
        }
        let parts = dotted(Some(a), &m.src);
        // a function is named in lower case; `RoutingControl.READ` is a constant, not a call to come
        if parts.last().map_or(true, |l| l.starts_with(char::is_uppercase)) {
            continue;
        }
        let Some(pkg) = parts.first().and_then(|h| m.bindings.get(h)).filter(|b| !b.starts_with(root)).and_then(|b| b.split('.').next()) else { continue };
        let Some(tag) = effects::boundary(Some(pkg), &parts) else { continue };
        let op = parts.last().cloned().unwrap_or_default();
        let f = effects::facts(tag, Some(pkg), &op, &[]);
        out.push(Touch { what: what.into(), op, artifact: f.artifact, ..Touch::new(f.kind, f.dir) });
    }
    out
}

/// A body's calls, as edges and touches. Gives back the functions it hands to a managed
/// transaction, with the database and the direction the transaction says.
/// One call's arguments to a function of the package: which parameter each is handed to, where
/// the call is, and which of the callee's parameters it leaves out.
struct Arg<'t> {
    caller: String,
    callee: String,
    module: String,
    scope: Option<TsNode<'t>>,
    passed: Vec<(String, TsNode<'t>)>,
}

/// What reading a body gives besides its edges and touches: the work it hands a managed
/// transaction, its file calls with their paths as built (placed once the parameters' values
/// are known), and the arguments of its calls.
#[derive(Default)]
struct Linked<'t> {
    handed: Vec<(String, World, Dir)>,
    files: Vec<(Touch, Vec<Built>)>,
    args: Vec<Arg<'t>>,
}

fn link_calls<'t>(modules: &'t IndexMap<String, Module>, store: &mut GraphBuilder, types: &Types, sites: &HashMap<(String, usize), String>, ext: &HashMap<(String, String), String>, eval: &Eval, root: &str, id: &str) -> Linked<'t> {
    let n = store.nodes[id].clone();
    let m = &modules[&n.module];
    let Some(d) = def_node(m, &n) else { return Linked::default() };
    let src = &m.src;
    let in_module = n.kind == NodeKind::Module;
    let body = if in_module { Some(d) } else { d.child_by_field_name("body") };
    let Some(body) = body else { return Linked::default() };
    let mut handed: Vec<(String, World, Dir)> = vec![];
    let mut files: Vec<(Touch, Vec<Built>)> = vec![];
    let mut args: Vec<Arg> = vec![];
    let own_params: Vec<String> = if in_module { vec![] } else { paths::parameters(d, src, false).into_iter().map(|(p, _)| p).collect() };
    let at = At { m, scope: (!in_module).then_some(d), params: &own_params };
    // the direction a body says to a database's driver, per database
    let mut declared: Vec<(World, Dir)> = vec![];
    let params = if in_module { HashMap::new() } else { param_types(m, d, modules, &store.nodes, root) };
    let mut touches: Vec<Touch> = vec![];
    let mut unresolved: Vec<String> = vec![];
    let mut used: Vec<String> = vec![];
    let mut changes: Vec<(String, String)> = vec![];
    let mut steps = Steps::new(body, src, &CHOICES);
    let mut new_ext: Vec<String> = vec![];
    {
        let seen = Seen::new(m, types, sites, modules, store, root);
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
            let callee = seen.callee(fnode);
            // what the call is handed: the value it is called on, and its arguments
            let mut hands: Vec<Held> = vec![];
            let receiver = (fnode.kind() == "attribute").then(|| fnode.child_by_field_name("object")).flatten();
            if let Some(o) = receiver {
                hands.extend(seen.held_in(o, ext, &steps.kept));
            }
            if let Some(a) = c.child_by_field_name("arguments") {
                for x in named_children(a).into_iter().filter(|x| x.kind() != "comment") {
                    let v = if x.kind() == "keyword_argument" { x.child_by_field_name("value").unwrap_or(x) } else { x };
                    hands.extend(seen.held_in(v, ext, &steps.kept));
                }
            }
            let mut seen_held = HashSet::new();
            hands.retain(|h| seen_held.insert(h.clone()));
            // what the call gives back, by the name it is assigned to
            let assigned = kept_as(c, src).unwrap_or_default();
            let known = seen.value(c).filter(|v| !v.is_class && !v.is_code);
            let mut gives: Vec<Held> = known.map(|v| seen.drawn(v, ext)).unwrap_or_default().into_iter().map(|ty| Held::new(ty, assigned.clone())).collect();
            // a value whose type ty does not know is still made, and held by the name it is assigned to
            if gives.is_empty() && known.map_or(true, |v| v.shown == "Unknown") && !assigned.is_empty() && !hands.is_empty() {
                gives.push(Held::new(String::new(), assigned.clone()));
            }
            new_ext.extend(hands.iter().chain(&gives).map(|h| h.ty.clone()).filter(|t| t.starts_with("ext:")));
            steps.keep(&gives);
            // a method called on a subclass's value is that subclass's step
            let on = match (&callee, receiver) {
                (Some(Callee::Def(t)), Some(o)) => store.nodes.get(t).and_then(|n| n.cls.clone()).and_then(|b| {
                    let mine = seen.value(o).map(|v| seen.drawn(v, ext)).unwrap_or_default();
                    mine.into_iter().find(|c| *c != b && seen.is_class(c) && seen.descends(c, &b))
                }),
                _ => None,
            };
            // a function of the package handed on as a value (`executor.submit(self.run, x)`,
            // `Thread(target=work)`) is called by whoever receives it; a class handed to code
            // Atlas reads (`pipeline.add(DiffValues, ..)`) is built there
            for a in handed_on(c) {
                if let Some(t) = seen.handed_function(a) {
                    steps.handed(c, a, &t, EdgeKind::Calls);
                } else if let (Some(Callee::Def(_)), Some(t)) = (&callee, seen.handed_class(a)) {
                    steps.handed(c, a, &t, EdgeKind::Instantiates);
                }
            }
            if let Some(Callee::Def(t)) = &callee {
                let kind = if seen.is_class(t) { EdgeKind::Instantiates } else { EdgeKind::Calls };
                let on_receiver = receiver.map_or(false, |o| o.kind() == "identifier" && text(o, src) == "self");
                steps.call(c, t, kind, None, Step { on: on.unwrap_or_default(), on_receiver, hands, gives });
                if kind == EdgeKind::Calls && value_used(c) {
                    used.push(t.clone());
                }
                args.extend(arguments(modules, store, id, t, c, at));
                continue;
            }
            // a library's call not read that makes a value the body keeps under a name, from
            // values it was handed, is a step of its own; what a call gives without a name goes
            // into the call around it
            if !assigned.is_empty() {
                let op = match fnode.kind() {
                    "attribute" => fnode.child_by_field_name("attribute").map(|a| text(a, src).to_string()),
                    _ => Some(text(fnode, src).rsplit('.').next().unwrap_or("").to_string()),
                };
                steps.op(c, &op.unwrap_or_default(), hands, gives);
            }
            // a method on a value written out (`(root / name).write_text`) is its name alone
            let parts = match call_path(fnode, src) {
                p if p.is_empty() => fnode.child_by_field_name("attribute").map(|a| vec![text(a, src).to_string()]).unwrap_or_default(),
                p => p,
            };
            if parts.is_empty() {
                continue;
            }
            let lib = match callee {
                Some(Callee::Lib(lib)) => Some(lib),
                _ => None,
            };
            if let Some(cls) = lib.as_ref().and(seen.made_on(c, fnode)) {
                steps.edge(c, &cls, EdgeKind::Instantiates);
            }
            if lib.is_none() {
                unresolved.push(parts.iter().take(2).cloned().collect::<Vec<_>>().join("."));
            }
            let tag = effects::boundary(lib.as_deref(), &parts).filter(|t| seen.moves(fnode, t, lib.as_deref(), parts.last().map_or("", String::as_str)));
            if let Some(tag) = tag {
                let crossed = crossed(&seen, eval, at, c, fnode, tag, lib.as_deref(), parts.last().map_or("", String::as_str));
                touches.extend(crossed.touches);
                files.extend(crossed.files);
                declared.extend(crossed.said);
                handed.extend(crossed.handed);
            }
        }
        // what a body runs inside the session or transaction it opened goes the way it said
        for t in touches.iter_mut().filter(|t| t.dir.is_none()) {
            let said: HashSet<Dir> = declared.iter().filter(|(k, _)| *k == t.kind).map(|(_, d)| *d).collect();
            if let [dir] = said.into_iter().collect::<Vec<_>>()[..] {
                t.dir = Some(dir);
            }
        }
        // what the body changes of its inputs: `T -> T` for the receiver or a parameter it changes;
        // a value the body constructs itself is built, not changed
        if !in_module {
            let receiver_mutates = |c: TsNode| match c.child_by_field_name("function").filter(|f| f.kind() == "attribute" && f.child_by_field_name("object").map_or(false, |o| o.kind() == "identifier")) {
                Some(f) => matches!(seen.callee(f), Some(Callee::Def(t)) if store.mutating.contains(&t)),
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
    let compares = (n.kind != NodeKind::Module).then(|| crate::tree::compares(body, src, steps.choices(), &COMPARES, &CHOICES));
    steps.finish(store, id);
    for t in new_ext {
        store.ext_node(&t);
    }
    for t in used {
        store.used.insert((id.to_string(), t));
    }
    if let (Some(compares), Some(node)) = (compares, store.nodes.get_mut(id)) {
        node.compares = compares;
    }
    for (t, input) in changes {
        store.add_edge(id, &t, EdgeKind::Mutates, Some(input));
    }
    for t in touches {
        store.touch(id, t);
    }
    Linked { handed, files, args }
}

/// What a call that crosses into the world leaves: the touches it makes, the file touches whose
/// paths wait for what the callers hand, and the direction it says to a database's driver, with
/// the work a managed transaction it opens is handed.
#[derive(Default)]
struct Crossed {
    touches: Vec<Touch>,
    files: Vec<(Touch, Vec<Built>)>,
    said: Option<(World, Dir)>,
    handed: Option<(String, World, Dir)>,
}

/// A call that crosses `tag`: a file call by the files it is handed, any other by what its facts
/// and literals name.
#[allow(clippy::too_many_arguments)]
fn crossed<'t>(seen: &Seen, eval: &Eval, at: At<'_, 't>, c: TsNode<'t>, fnode: TsNode<'t>, tag: Crossing, lib: Option<&str>, op: &str) -> Crossed {
    let src = &at.m.src;
    match tag.world {
        World::Disk => {
            let (touches, files): (Vec<(Touch, Vec<Built>)>, _) = file_calls(seen, eval, at, c, fnode, tag, lib, op).into_iter().partition(|(_, built)| built.is_empty());
            Crossed { touches: touches.into_iter().map(|(t, _)| t).collect(), files, ..Default::default() }
        }
        _ => {
            let mut cr = Crosses::read(c, src, tag, lib, op, &literals(c, src, &["string"]), &["string"], &seen.handed(c));
            // on a hub, what moves is the repository the call names
            if let Some(repo) = argument(c, src, "repo_id").filter(|_| cr.facts.kind == World::Net && !cr.facts.artifact.is_empty()) {
                cr.payload = payload_of(c, src, &repo, "arguments", &seen.handed(c));
                cr.what = repo;
            }
            let said = match cr.facts.kind {
                World::Db(db) => effects::access(db, op, keyword(c, src, &["routing_", "default_access_mode"]).as_deref()),
                _ => None,
            };
            // the work a managed transaction is handed runs in its direction
            let work = said.filter(|_| effects::transaction(op)).and_then(|_| positional(c)).and_then(|a| seen.callee(a));
            let handed = match (work, said) {
                (Some(Callee::Def(w)), Some(d)) => Some((w, cr.facts.kind, d)),
                _ => None,
            };
            Crossed { touches: cr.touches(said), said: said.map(|d| (cr.facts.kind, d)), handed, ..Default::default() }
        }
    }
}

/// The files a file call touches, each a touch with its direction, what the code calls it (the
/// literal it is built from, else the expression that holds it: `datafile` in
/// `datafile.open(..)`), its name when the code writes it out and whether the code alone decides
/// it, and with its path as built (placed once the parameters' values are known). The file is what the call's library types as one: the path a method of a path is
/// called on, an argument typed as a path, or a file opened to read from or write to; a library
/// whose types are not known hands its first argument. The direction is what the type says
/// (`SupportsRead`, `SupportsWrite`), what the call's `mode` says, or for a call handed two paths
/// the API's own names for them (`src`, `dst`); else the operation's.
fn file_calls<'t>(seen: &Seen, eval: &Eval, at: At<'_, 't>, c: TsNode<'t>, fnode: TsNode<'t>, tag: Crossing, lib: Option<&str>, op: &str) -> Vec<(Touch, Vec<Built>)> {
    let src = &at.m.src;
    let touch = |kind: Crossing, what: String, artifact: String, fixed: bool| {
        let payload = payload_of(c, src, &what, "arguments", &seen.handed(c));
        Touch { what, payload, op: op.to_string(), artifact, fixed, line: c.start_position().row as u32 + 1, ..Touch::new(kind.world, kind.dir) }
    };
    let sig = seen.facts.and_then(|x| x.signature(Seen::range(fnode)));
    let typed = sig.map(|s| paths::file_args(s, c, src)).unwrap_or_default();
    let mode = typed.iter().find(|(p, _)| p.name == "mode").map(|(_, a)| unquote(text(*a, src)));
    let files: Vec<(Option<FileArg>, &str, TsNode)> = match (fnode.kind() == "attribute" && lib == Some("pathlib"), sig) {
        (true, _) => fnode.child_by_field_name("object").map(|o| vec![(Some(FileArg::Path), "self", o)]).unwrap_or_default(),
        (false, Some(_)) => typed.iter().filter(|(p, _)| p.file.is_some()).map(|(p, a)| (p.file, p.name.as_str(), *a)).collect(),
        (false, None) => positional(c).map(|a| vec![(None, "", a)]).unwrap_or_default(),
    };
    let paths = files.iter().filter(|(k, _, _)| *k == Some(FileArg::Path)).count();
    if files.is_empty() {
        let f = effects::facts(tag, lib, op, &mode.into_iter().collect::<Vec<_>>());
        let what = what_of(c, src, &["string"], "arguments");
        return f.places().into_iter().map(|(artifact, _)| (touch(Crossing { world: f.kind, dir: f.dir }, what.clone(), artifact, false), vec![])).collect();
    }
    files
        .into_iter()
        .map(|(file, name, a)| {
            // a method of a path names its file by the path and by its own arguments (a pattern)
            let mut lits = literals(a, src, &["string"]);
            if name == "self" {
                lits.extend(c.child_by_field_name("arguments").map(|x| literals(x, src, &["string"])).unwrap_or_default());
            }
            lits.extend(mode.clone());
            let f = effects::facts(tag, lib, op, &lits);
            let kind = file_direction(file, name, paths, &f);
            let artifact = f.places().into_iter().next().map(|(a, _)| a).unwrap_or_default();
            let what = match literals(a, src, &["string"]).into_iter().next() {
                Some(l) => crate::tree::shown(&l),
                None => text(a, src).chars().take(40).collect(),
            };
            (touch(kind, what, artifact, crate::tree::head_literal(a, src, &["string"])), eval.of(at, a, 0))
        })
        .collect()
}

/// The name the body keeps a call's value by: the target of the assignment (or `:=`) it is the
/// value of, through parentheses and `await`. A value assigned to `_` is not kept.
fn kept_as(c: TsNode, src: &[u8]) -> Option<String> {
    let mut n = c;
    loop {
        let p = n.parent()?;
        let value = |field: &str| p.child_by_field_name(field).map(|v| v.id()) == Some(n.id());
        match p.kind() {
            "parenthesized_expression" | "await" => n = p,
            "assignment" if value("right") => return p.child_by_field_name("left").filter(|l| l.kind() == "identifier" && text(*l, src) != "_").map(|l| text(l, src).to_string()),
            "named_expression" if value("value") => return p.child_by_field_name("name").map(|l| text(l, src).to_string()),
            _ => return None,
        }
    }
}

/// Which way a file argument goes: what its parameter's type says (a file to read from or to write
/// to), for a call handed two paths the API's own names for them, else what the operation does.
fn file_direction(file: Option<FileArg>, name: &str, paths: usize, f: &effects::Facts) -> Crossing {
    match file {
        Some(FileArg::Reads) => Crossing::read(World::Disk),
        Some(FileArg::Writes) => Crossing::write(World::Disk),
        _ if paths > 1 && matches!(name, "src" | "source" | "from_") => Crossing::read(World::Disk),
        _ if paths > 1 && matches!(name, "dst" | "destination" | "target" | "to") => Crossing::write(World::Disk),
        _ => Crossing { world: f.kind, dir: f.dir },
    }
}

/// Which parameter of the function a call reaches each argument is handed to: positional
/// arguments in order (after `self`), keyword arguments by name. A class's call reaches its
/// `__init__`.
fn arguments<'t>(modules: &'t IndexMap<String, Module>, store: &GraphBuilder, caller: &str, target: &str, c: TsNode<'t>, at: At<'_, 't>) -> Option<Arg<'t>> {
    let callee = match store.nodes.get(target)?.kind {
        NodeKind::Class => format!("{target}.__init__"),
        _ => target.to_string(),
    };
    let cn = store.nodes.get(&callee)?;
    let cm = modules.get(&cn.module)?;
    let def = def_node(cm, cn)?;
    let params = paths::parameters(def, &cm.src, cn.kind == NodeKind::Method);
    let mut passed = vec![];
    let mut i = 0;
    for a in named_children(c.child_by_field_name("arguments")?).into_iter().filter(|a| a.kind() != "comment") {
        match a.kind() {
            "keyword_argument" => {
                if let (Some(k), Some(v)) = (a.child_by_field_name("name"), a.child_by_field_name("value")) {
                    passed.push((text(k, &at.m.src).to_string(), v));
                }
            }
            "list_splat" | "dictionary_splat" => break,
            _ => {
                if let Some((p, _)) = params.get(i) {
                    passed.push((p.clone(), a));
                }
                i += 1;
            }
        }
    }
    Some(Arg { caller: caller.to_string(), callee, module: at.m.id.clone(), scope: at.scope, passed })
}

/// Place the file calls: each path, with its function's parameters bound to what its callers
/// hand them. Only the parameters a path depends on are followed, back through the callers'
/// own parameters; a default counts where a call leaves its parameter out, and an entry's
/// declared defaults count as its call.
fn place_files(modules: &IndexMap<String, Module>, store: &mut GraphBuilder, eval: &Eval, files: Vec<(String, Touch, Vec<Built>)>, args: &[Arg]) {
    let needs = |b: &Built, f: &str| b.iter().filter_map(|s| if let Seg::Param(p) = s { Some((f.to_string(), p.clone())) } else { None }).collect::<Vec<_>>();
    let mut queue: Vec<(String, String)> = files.iter().flat_map(|(f, _, bs)| bs.iter().flat_map(|b| needs(b, f)).collect::<Vec<_>>()).collect();
    let mut seen: HashSet<(String, String)> = HashSet::new();
    let mut passed: Vec<Passed> = vec![];
    let mut base: HashMap<(String, String), Vec<Built>> = HashMap::new();
    while let Some((f, p)) = queue.pop() {
        if !seen.insert((f.clone(), p.clone())) {
            continue;
        }
        let calls: Vec<&Arg> = args.iter().filter(|a| a.callee == f).collect();
        for a in &calls {
            let Some(&(_, node)) = a.passed.iter().find(|(q, _)| *q == p) else { continue };
            let Some(m) = modules.get(&a.module) else { continue };
            let caller_params: Vec<String> = a.scope.map(|s| paths::parameters(s, &m.src, false).into_iter().map(|(q, _)| q).collect()).unwrap_or_default();
            let values = eval.of(At { m, scope: a.scope, params: &caller_params }, node, 0);
            queue.extend(values.iter().flat_map(|b| needs(b, &a.caller)));
            passed.push(Passed { caller: a.caller.clone(), callee: f.clone(), param: p.clone(), values });
        }
        // a default: where a call leaves the parameter out, or as an entry is run
        let Some(n) = store.nodes.get(&f) else { continue };
        let Some(m) = modules.get(&n.module) else { continue };
        let Some(def) = def_node(m, n) else { continue };
        let omitted = calls.is_empty() || calls.iter().any(|a| !a.passed.iter().any(|(q, _)| *q == p));
        let declared = paths::parameters(def, &m.src, n.kind == NodeKind::Method).into_iter().find(|(q, _)| *q == p).and_then(|(_, d)| d);
        let entry = n.entry.is_some().then(|| paths::entry_defaults(def, &m.src).into_iter().find(|(q, _)| *q == p).map(|(_, d)| d)).flatten();
        for d in [entry, omitted.then_some(declared).flatten()].into_iter().flatten() {
            base.entry((f.clone(), p.clone())).or_default().extend(eval.of(At { m, scope: None, params: &[] }, d, 0));
        }
    }
    let values = paths::handed(&passed, base);
    for (f, t, built) in files {
        let mut placed: Vec<String> = built.iter().flat_map(|b| paths::bind(b, &f, &values)).filter_map(|b| paths::place(&b, &t.op)).collect();
        placed.dedup();
        if placed.is_empty() {
            store.touch(&f, t);
            continue;
        }
        for artifact in placed {
            store.touch(&f, Touch { artifact, ..t.clone() });
        }
    }
}

/// The arguments of a call that name a value without computing one: a name or an attribute,
/// handed positionally or by keyword.
fn handed_on(call: TsNode) -> Vec<TsNode> {
    let Some(args) = call.child_by_field_name("arguments") else { return vec![] };
    named_children(args)
        .into_iter()
        .filter_map(|a| match a.kind() {
            "keyword_argument" => a.child_by_field_name("value"),
            _ => Some(a),
        })
        .filter(|a| matches!(a.kind(), "identifier" | "attribute"))
        .collect()
}

/// Whether what a call gives back is used: anything but a call standing as a statement.
fn value_used(call: TsNode) -> bool {
    let mut n = call;
    while let Some(p) = n.parent().filter(|p| matches!(p.kind(), "await" | "parenthesized_expression")) {
        n = p;
    }
    n.parent().map_or(false, |p| p.kind() != "expression_statement")
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

/// Python's comparisons: `==`, `!=`, `in` and `not in` against strings, and a `match` over them.
const COMPARES: CompareSyntax = CompareSyntax {
    kinds: &["comparison_operator", "match_statement"],
    operands: tested,
    cases: string_cases,
    subject: |n| matches!(n.kind(), "identifier" | "attribute" | "subscript"),
    over: matched,
};

/// The two operands of a test of equality or membership.
fn tested<'t>(n: TsNode<'t>, src: &[u8]) -> Option<(TsNode<'t>, TsNode<'t>)> {
    let op = field_children(n, "operators").iter().map(|o| text(*o, src)).collect::<Vec<_>>().join(" ");
    match (op.as_str(), named_children(n).as_slice()) {
        ("==" | "!=" | "in" | "not in", [l, r]) => Some((*l, *r)),
        _ => None,
    }
}

/// A `match`: its subject, the strings of its case patterns, and the block that holds the cases.
fn matched<'t>(n: TsNode<'t>, src: &[u8]) -> Option<(String, Vec<String>, Option<TsNode<'t>>)> {
    let (Some(subject), Some(block)) = (n.child_by_field_name("subject"), n.child_by_field_name("body")) else { return None };
    let cases = walk(block, &["case_pattern"]).into_iter().flat_map(|p| walk(p, &["string"])).map(|s| unquote(text(s, src))).collect();
    Some((text(subject, src).to_string(), cases, Some(block)))
}

/// Python's own types: the standard library's modules that describe code, not data.
const OWN: OwnTypes = OwnTypes {
    packages: &["typing", "collections", "pathlib", "builtins", "datetime", "abc", "os", "enum", "dataclasses", "functools", "itertools", "types"],
    names: &[],
};
