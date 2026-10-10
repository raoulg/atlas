//! `parse: Repo -> Graph` for TypeScript and JavaScript apps and packages.
//!
//! Two passes over tree-sitter trees, as for Python: symbols and imports first, then
//! resolution of types, calls, JSX and boundaries into edges. A module is a file under the
//! package (`src/` is a layout folder, not a name), a directory is a package module, and an
//! `index` file stands for its directory. The engine knows the language and its common
//! libraries; it never knows a project.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use indexmap::IndexMap;
use tree_sitter::{Node as TsNode, Parser, Tree};
use walkdir::WalkDir;

use crate::builder::{ClassInfo, GraphBuilder, SigInfo};
use crate::effects;
use crate::model::{Compare, EdgeKind, Entry, EntryKind, Node, NodeKind, Touch};
use crate::tree::{
    arms_of, decides, evaluation_order, literals, named_children, nth_of_kind, number_choices, payload_of, span, starts_at_literal, text, unquote,
    walk, what_of, ChoiceSyntax, Part,
};

/// Directories that hold no code of the author's.
const SKIP_DIRS: &[&str] = &["node_modules", "dist", "build", "out", "coverage", "public", ".next", ".output", ".wrangler", ".vercel", ".turbo"];
const EXTENSIONS: &[&str] = &["ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs"];
/// Generic types whose arguments are the values they hold or stand for: `Promise<Regel[]>`
/// hands on Regels. A generic class of a package or a library is a value of that class.
const CONTAINERS: &[&str] = &[
    "Promise", "Array", "ReadonlyArray", "Set", "ReadonlySet", "Map", "ReadonlyMap", "Record", "Partial", "Required", "Readonly",
    "Pick", "Omit", "NonNullable", "Awaited", "Iterable", "AsyncIterable", "Generator", "AsyncGenerator",
];
/// Packages whose types are how code is written, not places data rests.
const PLAIN_PACKAGES: &[&str] = &["react", "react-dom", "zod"];

#[derive(Clone, Debug)]
enum Bound {
    /// A node id, a member of an own module (`module.name`), or an own module.
    Own(String),
    /// A name from a package: `ext:pkg.name`.
    Ext(String),
}

struct Module {
    id: String,
    path: PathBuf,
    src: Vec<u8>,
    tree: Tree,
    bindings: HashMap<String, Bound>,
    symbols: HashMap<String, String>,
    default: Option<Bound>,
    wildcards: Vec<String>,
    /// Type aliases: name -> (the names they stand for, whether they are a union).
    aliases: HashMap<String, (Vec<Vec<String>>, bool)>,
    /// Function node id -> the byte range of its body (a block, an arrow, an initializer).
    bodies: HashMap<String, (usize, usize)>,
    /// The package names the module imports from.
    ext_pkgs: HashSet<String>,
    /// Module-level values by the type they hold: `const provider: ProviderLike = ..`,
    /// `const api = new Api()`.
    values: HashMap<String, Vec<String>>,
    /// Names a body imports when it runs: `const { ApiProvider } = await import("./api-provider")`.
    dynamic: HashMap<String, Bound>,
}

/// Does this look like a JavaScript or TypeScript package: a `package.json` with a
/// `tsconfig.json`, a `jsconfig.json` or a `src/` beside it.
pub fn detect(repo: &Path) -> bool {
    repo.join("package.json").exists() && (repo.join("tsconfig.json").exists() || repo.join("jsconfig.json").exists() || repo.join("src").is_dir())
}

fn ts_parser(tsx: bool) -> Result<Parser> {
    let mut p = Parser::new();
    let lang: tree_sitter::Language = if tsx { tree_sitter_typescript::LANGUAGE_TSX.into() } else { tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into() };
    p.set_language(&lang).context("loading the TypeScript grammar")?;
    Ok(p)
}

/// The package's name for the graph: its directory, as Python's is its package directory.
fn root_name(repo: &Path) -> String {
    let dir = std::fs::canonicalize(repo).ok().and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string())).unwrap_or_else(|| "app".into());
    segment(&dir)
}

/// A path segment as part of a dotted id: dots inside a file name (`punt.functions`) are not
/// module boundaries.
fn segment(s: &str) -> String {
    s.replace('.', "_")
}

/// Where module paths start: `src/` when there is one.
fn code_base(repo: &Path) -> PathBuf {
    let src = repo.join("src");
    if src.is_dir() { src } else { repo.to_path_buf() }
}

fn is_code(p: &Path) -> bool {
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
    EXTENSIONS.contains(&ext) && !name.ends_with(".d.ts") && !name.contains(".gen.") && !name.contains(".min.") && !name.contains(".config.")
}

/// `src/lib/kaart/lagen.ts` -> `plusmin.lib.kaart.lagen`; `src/lib/index.ts` -> `plusmin.lib`.
fn module_id(root: &str, base: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(base).unwrap_or(path);
    let mut parts: Vec<String> = rel.parent().map(|d| d.components().map(|c| segment(&c.as_os_str().to_string_lossy())).collect()).unwrap_or_default();
    let stem = rel.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let stem = stem.rsplit_once('.').map(|(s, _)| s).unwrap_or(stem);
    if stem != "index" {
        parts.push(segment(stem));
    }
    std::iter::once(root.to_string()).chain(parts).collect::<Vec<_>>().join(".")
}

/// tsconfig `paths`: alias prefix -> directory, `"@/*": ["./src/*"]` -> (`@/`, `<repo>/src/`).
fn path_aliases(repo: &Path) -> Vec<(String, PathBuf)> {
    let mut out = vec![];
    for cfg in ["tsconfig.json", "jsconfig.json"] {
        let Ok(text) = std::fs::read_to_string(repo.join(cfg)) else { continue };
        let Ok(json) = serde_json::from_str::<serde_json::Value>(&jsonc(&text)) else { continue };
        let opts = &json["compilerOptions"];
        let base = repo.join(opts["baseUrl"].as_str().unwrap_or("."));
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
            ('/', Some('/')) => {
                while chars.next_if(|&n| n != '\n').is_some() {}
            }
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

/// The file an import specifier names: relative to the importing file, or through a path alias.
fn resolve_spec(spec: &str, from: &Path, aliases: &[(String, PathBuf)], files: &HashMap<PathBuf, String>) -> Option<String> {
    let base = if spec.starts_with('.') {
        from.parent()?.join(spec)
    } else {
        let (alias, dir) = aliases.iter().find(|(a, _)| !a.is_empty() && spec.starts_with(a.as_str()))?;
        dir.join(&spec[alias.len()..])
    };
    let base = normalize(&base);
    let stripped = base.with_extension("");
    let mut candidates = vec![base.clone()];
    for b in [&base, &stripped] {
        for ext in EXTENSIONS {
            candidates.push(PathBuf::from(format!("{}.{ext}", b.display())));
            candidates.push(b.join(format!("index.{ext}")));
        }
    }
    candidates.into_iter().find_map(|c| files.get(&c).cloned())
}

fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

/// The package an import comes from: `@tanstack/react-router/x` -> `@tanstack/react-router`,
/// `node:fs/promises` -> `fs`.
fn package_of(spec: &str) -> String {
    let spec = spec.strip_prefix("node:").unwrap_or(spec);
    let mut parts = spec.split('/');
    match (spec.starts_with('@'), parts.next(), parts.next()) {
        (true, Some(scope), Some(name)) => format!("{scope}/{name}"),
        (_, Some(first), _) => first.to_string(),
        _ => spec.to_string(),
    }
}

/// A name from a package as an id: `ol/Map.js` default -> `ext:ol.Map`, `react` useState ->
/// `ext:react.useState`.
fn ext_id(spec: &str, name: Option<&str>) -> String {
    let path = spec.strip_prefix("node:").unwrap_or(spec);
    let path = path.rsplit_once('.').filter(|(_, e)| EXTENSIONS.contains(e)).map(|(p, _)| p).unwrap_or(path);
    match name {
        Some(n) => format!("ext:{}.{n}", path.replace('/', ".")),
        None => {
            let (pkg, last) = path.rsplit_once('/').unwrap_or((path, path));
            format!("ext:{}.{last}", pkg.replace('/', "."))
        }
    }
}

/// A leading JSDoc or line comment, first line.
fn doc_of(n: TsNode, src: &[u8]) -> String {
    let target = if n.parent().map_or(false, |p| p.kind() == "export_statement") { n.parent().unwrap() } else { n };
    let Some(prev) = target.prev_named_sibling().filter(|p| p.kind() == "comment") else { return String::new() };
    if prev.end_position().row + 1 < target.start_position().row {
        return String::new();
    }
    text(prev, src)
        .lines()
        .map(|l| l.trim().trim_start_matches("/**").trim_start_matches("/*").trim_start_matches("//").trim_start_matches('*').trim())
        .find(|l| !l.is_empty())
        .unwrap_or("")
        .trim_end_matches("*/")
        .trim()
        .to_string()
}

// ---------- types ----------

/// The type names an annotation mentions as values: `Promise<Regel[]>` -> Regel,
/// `Treffer | Mis` -> both, `Laag<Bron>` -> Laag (a generic class's arguments are types).
fn type_names(t: Option<TsNode>, src: &[u8]) -> Vec<Vec<String>> {
    let Some(t) = t else { return vec![] };
    let mut out = vec![];
    collect_types(t, src, &mut out);
    out
}

fn collect_types(n: TsNode, src: &[u8], out: &mut Vec<Vec<String>>) {
    match n.kind() {
        "type_identifier" => out.push(vec![text(n, src).to_string()]),
        "nested_type_identifier" => out.push(text(n, src).split('.').map(str::to_string).collect()),
        "generic_type" => {
            let Some(name) = n.child_by_field_name("name") else { return };
            let parts: Vec<String> = text(name, src).split('.').map(str::to_string).collect();
            if CONTAINERS.contains(&parts.last().map(String::as_str).unwrap_or("")) {
                if let Some(args) = n.child_by_field_name("type_arguments") {
                    collect_types(args, src, out);
                }
            } else {
                out.push(parts);
            }
        }
        // a function type is a callback, an object type an anonymous shape: neither names a place
        "function_type" | "object_type" | "predefined_type" | "literal_type" | "template_literal_type" | "type_query" => {}
        _ => {
            for c in named_children(n) {
                collect_types(c, src, out);
            }
        }
    }
}

/// `(id: string, opt?: Opties)` as the author wrote each parameter, and the types they name.
fn signature(params: Option<TsNode>, ret: Option<TsNode>, src: &[u8]) -> (Vec<String>, String, SigInfo) {
    let mut out = vec![];
    let mut info = SigInfo::default();
    if let Some(ps) = params {
        // an arrow with one bare parameter: `x => ..`
        if ps.kind() == "identifier" {
            out.push(text(ps, src).to_string());
        } else {
            for p in named_children(ps).into_iter().filter(|p| p.kind() != "comment") {
                let pattern = p.child_by_field_name("pattern").map(|x| text(x, src)).unwrap_or_else(|| text(p, src));
                let pattern = pattern.split_whitespace().collect::<Vec<_>>().join(" ");
                let optional = if p.kind() == "optional_parameter" { "?" } else { "" };
                let ty = p.child_by_field_name("type");
                out.push(match ty.and_then(|t| named_children(t).into_iter().next()) {
                    Some(t) => format!("{pattern}{optional}: {}", text(t, src)),
                    None => pattern.to_string(),
                });
                info.ptypes.extend(type_names(ty, src));
            }
        }
    }
    let returns = ret.and_then(|r| named_children(r).into_iter().next()).map(|r| text(r, src).to_string()).unwrap_or_default();
    info.rtypes = type_names(ret, src);
    (out, returns, info)
}

// ---------- pass 1: files, symbols, imports ----------

pub fn parse(repo: &Path) -> Result<(GraphBuilder, String)> {
    let root = root_name(repo);
    let base = code_base(repo);
    let aliases = path_aliases(repo);
    let mut store = GraphBuilder::default();
    let mut paths: Vec<PathBuf> = WalkDir::new(&base)
        .into_iter()
        .filter_entry(|e| {
            let n = e.file_name().to_str().unwrap_or("");
            e.depth() == 0 || !(n.starts_with('.') || SKIP_DIRS.contains(&n))
        })
        .flatten()
        .filter(|e| e.file_type().is_file() && is_code(e.path()))
        .map(|e| normalize(e.path()))
        .collect();
    paths.sort();
    let files: HashMap<PathBuf, String> = paths.iter().map(|p| (p.clone(), module_id(&root, &base, p))).collect();
    // every directory is a package module, so roles follow the folders
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
        store.add_node(Node { id: d.clone(), kind: NodeKind::Module, name: d.rsplit('.').next().unwrap_or(d).to_string(), module: d.clone(), is_pkg: true, ..Default::default() });
    }
    let mut modules: IndexMap<String, Module> = IndexMap::new();
    let (mut ts, mut tsx) = (ts_parser(false)?, ts_parser(true)?);
    for p in &paths {
        let id = files[p].clone();
        let src = std::fs::read(p).with_context(|| format!("reading {}", p.display()))?;
        let jsx = matches!(p.extension().and_then(|e| e.to_str()), Some("tsx" | "jsx" | "js" | "mjs" | "cjs"));
        let tree = (if jsx { &mut tsx } else { &mut ts }).parse(&src, None).with_context(|| format!("parsing {}", p.display()))?;
        store.files.insert(crate::tree::path_str(p), String::from_utf8_lossy(&src).to_string());
        let is_index = p.file_stem().and_then(|s| s.to_str()) == Some("index");
        let rel = crate::tree::path_str(p.strip_prefix(repo).unwrap_or(p));
        let entry = test_file(p).then(|| Entry { kind: EntryKind::Test, label: rel.clone() });
        let node = Node {
            id: id.clone(),
            kind: NodeKind::Module,
            name: p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string(),
            file: crate::tree::path_str(p),
            span: span(tree.root_node()),
            module: id.clone(),
            is_pkg: is_index,
            entry,
            ..Default::default()
        };
        store.add_node(node);
        modules.insert(
            id.clone(),
            Module {
                id,
                path: p.clone(),
                src,
                tree,
                bindings: HashMap::new(),
                symbols: HashMap::new(),
                default: None,
                wildcards: vec![],
                aliases: HashMap::new(),
                bodies: HashMap::new(),
                ext_pkgs: HashSet::new(),
                values: HashMap::new(),
                dynamic: HashMap::new(),
            },
        );
    }
    for m in modules.values_mut() {
        collect_symbols(m, &mut store);
        collect_imports(m, &aliases, &files);
    }
    mark_bins(repo, &files, &mut store);
    link(&modules, &mut store);
    Ok((store, root))
}

fn test_file(p: &Path) -> bool {
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
    name.contains(".test.") || name.contains(".spec.") || p.components().any(|c| c.as_os_str() == "__tests__")
}

/// The top-level statements of a module with the declaration each holds, unwrapping `export`.
fn declarations<'t>(root: TsNode<'t>) -> Vec<(TsNode<'t>, bool, bool)> {
    let mut out = vec![];
    for st in named_children(root) {
        if st.kind() == "export_statement" {
            let default = (0..st.child_count()).filter_map(|i| st.child(i)).any(|c| c.kind() == "default");
            if let Some(d) = st.child_by_field_name("declaration").or_else(|| st.child_by_field_name("value")) {
                out.push((d, true, default));
            }
        } else {
            out.push((st, false, false));
        }
    }
    out
}

/// The function a top-level `const` stands for: an arrow or function expression, or the last
/// one handed to a call chain (`createServerFn(..).handler(async ({ data }) => ..)`). An object
/// literal is a value, not a function.
fn function_in<'t>(value: TsNode<'t>) -> Option<TsNode<'t>> {
    match value.kind() {
        "arrow_function" | "function_expression" | "function" => Some(value),
        "call_expression" | "await_expression" | "parenthesized_expression" | "as_expression" | "satisfies_expression" | "member_expression" => {
            let mut found = None;
            for c in named_children(value) {
                if c.kind() == "arguments" {
                    for a in named_children(c) {
                        if matches!(a.kind(), "arrow_function" | "function_expression" | "function") {
                            found = Some(a);
                        }
                    }
                } else if let Some(f) = function_in(c) {
                    found = Some(f);
                }
            }
            found
        }
        _ => None,
    }
}

/// The path of the file route a declaration makes: `createFileRoute("/kaart")({..})`.
fn route_path(value: TsNode, src: &[u8]) -> Option<String> {
    walk(value, &["call_expression"]).into_iter().find_map(|c| {
        let f = c.child_by_field_name("function")?;
        match text(f, src) {
            "createFileRoute" | "createLazyFileRoute" => {
                let lit = c.child_by_field_name("arguments").and_then(|a| named_children(a).into_iter().find(|x| x.kind() == "string"))?;
                Some(unquote(text(lit, src)))
            }
            "createRootRoute" | "createRootRouteWithContext" => Some("__root".into()),
            _ => None,
        }
    })
}

fn add_function(m: &mut Module, store: &mut GraphBuilder, name: &str, def: TsNode, f: TsNode, body: TsNode, cls: Option<&str>) -> String {
    let src = &m.src;
    let id = match cls {
        // a class's constructor is its __init__, the name the graph gives constructors
        Some(c) if name == "constructor" => format!("{c}.__init__"),
        Some(c) => format!("{c}.{name}"),
        None => format!("{}.{name}", m.id),
    };
    let params = f.child_by_field_name("parameters").or_else(|| f.child_by_field_name("parameter"));
    let (params, returns, sig) = signature(params, f.child_by_field_name("return_type"), src);
    store.sigs.insert(id.clone(), sig);
    store.add_node(Node {
        id: id.clone(),
        kind: if cls.is_some() { NodeKind::Method } else { NodeKind::Function },
        on_class: cls.is_some() && (0..def.child_count()).filter_map(|i| def.child(i)).any(|c| c.kind() == "static"),
        name: name.to_string(),
        file: crate::tree::path_str(&m.path),
        span: span(def),
        module: m.id.clone(),
        doc: doc_of(def, src),
        cls: cls.map(str::to_string),
        params,
        returns,
        ..Default::default()
    });
    m.bodies.insert(id.clone(), (body.start_byte(), body.end_byte()));
    id
}

fn collect_symbols(m: &mut Module, store: &mut GraphBuilder) {
    let tree = m.tree.clone();
    let src = m.src.clone();
    let mut route: Option<String> = None;
    for (d, _exported, default) in declarations(tree.root_node()) {
        let name = d.child_by_field_name("name").map(|n| text(n, &src).to_string()).unwrap_or_default();
        match d.kind() {
            "function_declaration" | "generator_function_declaration" | "function_expression" | "function" if !name.is_empty() => {
                let body = d.child_by_field_name("body").unwrap_or(d);
                let id = add_function(m, store, &name, d, d, body, None);
                store.add_edge(&m.id.clone(), &id, EdgeKind::Contains, None);
                m.symbols.insert(name.clone(), id.clone());
                if default {
                    m.default = Some(Bound::Own(id));
                }
            }
            "class_declaration" | "abstract_class_declaration" | "class" if !name.is_empty() => {
                let id = format!("{}.{name}", m.id);
                let mut bases = vec![];
                let mut fields: Vec<(String, Vec<Vec<String>>)> = vec![];
                for h in walk(d, &["class_heritage"]).into_iter().take(1) {
                    for c in walk(h, &["extends_clause", "implements_clause"]) {
                        for v in named_children(c).into_iter().filter(|v| v.kind() != "type_arguments") {
                            let parts: Vec<String> = text(v, &src).split('<').next().unwrap_or("").split('.').map(|s| s.trim().to_string()).collect();
                            if !parts.iter().any(String::is_empty) {
                                bases.push(parts);
                            }
                        }
                    }
                }
                let mut methods = vec![];
                if let Some(body) = d.child_by_field_name("body") {
                    for member in named_children(body) {
                        match member.kind() {
                            "method_definition" | "abstract_method_signature" | "method_signature" => {
                                let mname = member.child_by_field_name("name").map(|n| text(n, &src).to_string()).unwrap_or_default();
                                let mbody = member.child_by_field_name("body").unwrap_or(member);
                                let mid = add_function(m, store, &mname, member, member, mbody, Some(&id));
                                store.add_edge(&id, &mid, EdgeKind::Contains, None);
                                methods.push(mid.rsplit('.').next().unwrap_or("").to_string());
                                // `constructor(private bron: Bron)` declares a field
                                if mname == "constructor" {
                                    for p in member.child_by_field_name("parameters").map(named_children).unwrap_or_default() {
                                        if named_children(p).iter().any(|c| c.kind() == "accessibility_modifier") {
                                            if let Some(pat) = p.child_by_field_name("pattern") {
                                                fields.push((text(pat, &src).to_string(), type_names(p.child_by_field_name("type"), &src)));
                                            }
                                        }
                                    }
                                }
                            }
                            "public_field_definition" | "property_signature" => {
                                let fname = member.child_by_field_name("name").map(|n| text(n, &src).to_string()).unwrap_or_default();
                                // a field holding an arrow is a method in all but syntax
                                if let Some(f) = member.child_by_field_name("value").filter(|v| v.kind() == "arrow_function") {
                                    let mid = add_function(m, store, &fname, member, f, f, Some(&id));
                                    store.add_edge(&id, &mid, EdgeKind::Contains, None);
                                    methods.push(fname);
                                } else {
                                    fields.push((fname, type_names(member.child_by_field_name("type"), &src)));
                                }
                            }
                            _ => {}
                        }
                    }
                }
                add_type(m, store, &name, d, bases, fields, methods);
                if default {
                    m.default = Some(Bound::Own(id));
                }
            }
            "interface_declaration" => {
                let bases: Vec<Vec<String>> = walk(d, &["extends_type_clause"])
                    .into_iter()
                    .flat_map(|c| named_children(c))
                    .map(|v| text(v, &src).split('<').next().unwrap_or("").split('.').map(|s| s.trim().to_string()).collect())
                    .collect();
                let body = d.child_by_field_name("body");
                let fields = body.map(|b| fields_of(b, &src)).unwrap_or_default();
                let methods = body.map(|b| method_signatures(m, store, &name, b)).unwrap_or_default();
                add_type(m, store, &name, d, bases, fields, methods);
            }
            "type_alias_declaration" => {
                let Some(value) = d.child_by_field_name("value") else { continue };
                match value.kind() {
                    // a named shape is a type of its own
                    "object_type" => {
                        let fields = fields_of(value, &src);
                        let methods = method_signatures(m, store, &name, value);
                        add_type(m, store, &name, d, vec![], fields, methods);
                    }
                    kind => {
                        m.aliases.insert(name.clone(), (type_names(Some(value), &src), kind == "union_type"));
                    }
                }
            }
            "enum_declaration" => add_type(m, store, &name, d, vec![], vec![], vec![]),
            "lexical_declaration" | "variable_declaration" => {
                for v in named_children(d).into_iter().filter(|v| v.kind() == "variable_declarator") {
                    let (Some(n), Some(value)) = (v.child_by_field_name("name"), v.child_by_field_name("value")) else { continue };
                    if n.kind() != "identifier" {
                        continue;
                    }
                    if let Some(path) = route_path(value, &src) {
                        route = Some(path);
                        continue;
                    }
                    let vname = text(n, &src).to_string();
                    let Some(f) = function_in(value) else {
                        let declared = v.child_by_field_name("type").and_then(|t| named_children(t).into_iter().next()).map(|t| type_head(t, &src));
                        let built = (value.kind() == "new_expression").then(|| value.child_by_field_name("constructor").map(|c| member_parts(c, &src))).flatten();
                        if let Some(t) = declared.or(built).filter(|t| !t.is_empty()) {
                            m.values.insert(vname, t);
                        }
                        continue;
                    };
                    let id = add_function(m, store, &vname, d, f, value, None);
                    store.add_edge(&m.id.clone(), &id, EdgeKind::Contains, None);
                    m.symbols.insert(vname, id);
                }
            }
            "identifier" if default => {
                m.default = Some(Bound::Own(format!("{}.{}", m.id, text(d, &src))));
            }
            _ => {}
        }
    }
    if let Some(path) = route {
        if let Some(n) = store.nodes.get_mut(&m.id) {
            n.entry = Some(Entry { kind: EntryKind::Route, label: if path == "__root" { "/ (root)".into() } else { path } });
        }
    }
}

/// What an interface or a named object type promises its implementations do: each method
/// signature is a method of the type, with no body of its own.
fn method_signatures(m: &mut Module, store: &mut GraphBuilder, type_name: &str, body: TsNode) -> Vec<String> {
    let id = format!("{}.{type_name}", m.id);
    let mut out = vec![];
    for sig in named_children(body).into_iter().filter(|x| x.kind() == "method_signature") {
        let name = sig.child_by_field_name("name").map(|n| text(n, &m.src).to_string()).unwrap_or_default();
        let mid = add_function(m, store, &name, sig, sig, sig, Some(&id));
        store.add_edge(&id, &mid, EdgeKind::Contains, None);
        out.push(name);
    }
    out
}

/// The name a type annotation starts with: `ProviderLike`, `Map<string, Laag>` -> `Map`.
fn type_head(t: TsNode, src: &[u8]) -> Vec<String> {
    let node = if t.kind() == "generic_type" { t.child_by_field_name("name").unwrap_or(t) } else { t };
    match node.kind() {
        "type_identifier" | "nested_type_identifier" | "identifier" => text(node, src).split('.').map(str::to_string).collect(),
        _ => vec![],
    }
}

/// The typed members of an interface or object type: `{ id: string; laag: Laag }`.
fn fields_of(body: TsNode, src: &[u8]) -> Vec<(String, Vec<Vec<String>>)> {
    named_children(body)
        .into_iter()
        .filter(|p| p.kind() == "property_signature")
        .filter_map(|p| Some((text(p.child_by_field_name("name")?, src).to_string(), type_names(p.child_by_field_name("type"), src))))
        .collect()
}

fn add_type(m: &mut Module, store: &mut GraphBuilder, name: &str, d: TsNode, bases: Vec<Vec<String>>, fields: Vec<(String, Vec<Vec<String>>)>, methods: Vec<String>) {
    let id = format!("{}.{name}", m.id);
    store.classes.insert(id.clone(), ClassInfo { bases: bases.clone(), fields, ..Default::default() });
    store.add_node(Node {
        id: id.clone(),
        kind: NodeKind::Class,
        name: name.to_string(),
        file: crate::tree::path_str(&m.path),
        span: span(d),
        module: m.id.clone(),
        doc: doc_of(d, &m.src),
        bases: bases.iter().map(|b| b.join(".")).collect(),
        methods,
        ..Default::default()
    });
    store.add_edge(&m.id.clone(), &id, EdgeKind::Contains, None);
    m.symbols.insert(name.to_string(), id);
}

fn collect_imports(m: &mut Module, aliases: &[(String, PathBuf)], files: &HashMap<PathBuf, String>) {
    let src = m.src.clone();
    let tree = m.tree.clone();
    // `const { ApiProvider } = await import("./api-provider")`: names a body imports when it runs
    for v in walk(tree.root_node(), &["variable_declarator"]) {
        let (Some(pattern), Some(value)) = (v.child_by_field_name("name"), v.child_by_field_name("value")) else { continue };
        if pattern.kind() != "object_pattern" {
            continue;
        }
        let Some(spec) = walk(value, &["call_expression"]).into_iter().find_map(|c| {
            (c.child_by_field_name("function")?.kind() == "import").then_some(())?;
            let lit = named_children(c.child_by_field_name("arguments")?).into_iter().find(|a| a.kind() == "string")?;
            Some(unquote(text(lit, &src)))
        }) else {
            continue;
        };
        let own = resolve_spec(&spec, &m.path, aliases, files);
        for name in walk(pattern, &["shorthand_property_identifier_pattern"]) {
            let name = text(name, &src).to_string();
            let bound = match &own {
                Some(id) => Bound::Own(format!("{id}.{name}")),
                None => Bound::Ext(ext_id(&spec, Some(&name))),
            };
            m.dynamic.insert(name, bound);
        }
    }
    for st in named_children(tree.root_node()) {
        let Some(source) = st.child_by_field_name("source") else {
            // `export { a, b as c }` names what is already here
            if st.kind() == "export_statement" {
                for s in walk(st, &["export_specifier"]) {
                    let (Some(n), alias) = (s.child_by_field_name("name"), s.child_by_field_name("alias")) else { continue };
                    let target = format!("{}.{}", m.id, text(n, &src));
                    if let Some(a) = alias {
                        m.bindings.insert(text(a, &src).to_string(), Bound::Own(target));
                    }
                }
            }
            continue;
        };
        let spec = unquote(text(source, &src));
        let own = resolve_spec(&spec, &m.path, aliases, files);
        if own.is_none() {
            m.ext_pkgs.insert(package_of(&spec));
        }
        let member = |name: Option<&str>| match &own {
            Some(id) => Bound::Own(match name {
                Some(n) => format!("{id}.{n}"),
                None => id.clone(),
            }),
            None => Bound::Ext(ext_id(&spec, name)),
        };
        match st.kind() {
            "import_statement" => {
                for clause in named_children(st).into_iter().filter(|c| c.kind() == "import_clause") {
                    for c in named_children(clause) {
                        match c.kind() {
                            "identifier" => {
                                let local = text(c, &src).to_string();
                                m.bindings.insert(local, if own.is_some() { member(Some("default")) } else { member(None) });
                            }
                            "namespace_import" => {
                                if let Some(id) = named_children(c).into_iter().find(|x| x.kind() == "identifier") {
                                    m.bindings.insert(text(id, &src).to_string(), member(None));
                                }
                            }
                            "named_imports" => {
                                for s in named_children(c).into_iter().filter(|s| s.kind() == "import_specifier") {
                                    let Some(n) = s.child_by_field_name("name") else { continue };
                                    let local = s.child_by_field_name("alias").unwrap_or(n);
                                    m.bindings.insert(text(local, &src).to_string(), member(Some(text(n, &src))));
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            "export_statement" => {
                let specs = walk(st, &["export_specifier"]);
                if specs.is_empty() {
                    // `export * from "./x"`
                    if let Some(id) = &own {
                        m.wildcards.push(id.clone());
                    }
                }
                for s in specs {
                    let Some(n) = s.child_by_field_name("name") else { continue };
                    let local = s.child_by_field_name("alias").unwrap_or(n);
                    m.bindings.insert(text(local, &src).to_string(), member(Some(text(n, &src))));
                }
            }
            _ => {}
        }
    }
}

/// `package.json` `bin`: each command is a CLI route into the module it runs.
fn mark_bins(repo: &Path, files: &HashMap<PathBuf, String>, store: &mut GraphBuilder) {
    let Ok(text) = std::fs::read_to_string(repo.join("package.json")) else { return };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else { return };
    let bins: Vec<(String, String)> = match &json["bin"] {
        serde_json::Value::String(p) => vec![(json["name"].as_str().unwrap_or("bin").to_string(), p.clone())],
        serde_json::Value::Object(o) => o.iter().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string()))).collect(),
        _ => vec![],
    };
    for (name, path) in bins {
        let p = normalize(&repo.join(&path));
        let id = files.get(&p).cloned().or_else(|| files.iter().find(|(f, _)| f.with_extension("") == p.with_extension("")).map(|(_, id)| id.clone()));
        if let Some(n) = id.and_then(|id| store.nodes.get_mut(&id)) {
            n.entry = Some(Entry { kind: EntryKind::Cli, label: name });
        }
    }
}

// ---------- pass 2: resolution ----------

/// A name in a module's scope -> a node id or `ext:` id, following imports and re-exports.
fn resolve(m: &Module, parts: &[String], modules: &IndexMap<String, Module>, nodes: &IndexMap<String, Node>) -> Option<String> {
    let head = parts.first()?;
    let bound = match m.symbols.get(head) {
        Some(id) => Bound::Own(id.clone()),
        None => m.bindings.get(head).or_else(|| m.dynamic.get(head))?.clone(),
    };
    match bound {
        Bound::Ext(id) => Some(if parts.len() > 1 { format!("{id}.{}", parts[1..].join(".")) } else { id }),
        Bound::Own(target) => {
            let full = if parts.len() > 1 { format!("{target}.{}", parts[1..].join(".")) } else { target };
            defined(&full, modules, nodes, 0)
        }
    }
}

/// An own target followed to where it is defined: through re-exports, `export *` and defaults.
fn defined(target: &str, modules: &IndexMap<String, Module>, nodes: &IndexMap<String, Node>, depth: u32) -> Option<String> {
    if nodes.contains_key(target) && !modules.contains_key(target) {
        return Some(target.to_string());
    }
    if depth > 8 {
        return None;
    }
    let (modid, name) = target.rsplit_once('.')?;
    let m = modules.get(modid)?;
    if let Some(id) = m.symbols.get(name) {
        return Some(id.clone());
    }
    let next = if name == "default" { m.default.clone() } else { m.bindings.get(name).cloned() };
    match next {
        Some(Bound::Own(t)) => return defined(&t, modules, nodes, depth + 1),
        Some(Bound::Ext(id)) => return Some(id),
        None => {}
    }
    m.wildcards.iter().find_map(|w| defined(&format!("{w}.{name}"), modules, nodes, depth + 1))
}

/// A foreign type worth a place: a package's type, not React's own vocabulary.
fn foreign_type(id: &str) -> bool {
    let Some(full) = id.strip_prefix("ext:") else { return false };
    let head = full.split('.').next().unwrap_or("");
    full.contains('.') && !PLAIN_PACKAGES.contains(&head)
}

fn link(modules: &IndexMap<String, Module>, store: &mut GraphBuilder) {
    // a union of two or more known types is a sum; any other alias stands for what it names
    for m in modules.values() {
        let mut names: Vec<&String> = m.aliases.keys().collect();
        names.sort();
        for name in names {
            let (types, union) = &m.aliases[name];
            if !*union {
                continue;
            }
            let mut variants: Vec<String> = vec![];
            for r in types.iter().filter_map(|t| resolve(m, t, modules, &store.nodes)) {
                if (!r.starts_with("ext:") || foreign_type(&r)) && !variants.contains(&r) {
                    variants.push(r);
                }
            }
            if variants.len() < 2 {
                continue;
            }
            let id = format!("{}.{name}", m.id);
            store.add_node(Node {
                id: id.clone(),
                kind: NodeKind::Class,
                name: name.clone(),
                file: crate::tree::path_str(&m.path),
                module: m.id.clone(),
                ..Default::default()
            });
            store.add_edge(&m.id, &id, EdgeKind::Contains, None);
            for v in variants {
                if v.starts_with("ext:") {
                    store.ext_node(&v);
                }
                store.add_edge(&id, &v, EdgeKind::Variant, None);
            }
        }
    }
    let type_of = |m: &Module, t: &[String], nodes: &IndexMap<String, Node>| -> Vec<String> {
        if let Some(r) = resolve(m, t, modules, nodes) {
            return vec![r];
        }
        // an alias declared here or imported: what it stands for
        let [name] = t else { return vec![] };
        let (home, alias) = match m.aliases.get(name) {
            Some(_) => (m, name.clone()),
            None => match m.bindings.get(name) {
                Some(Bound::Own(target)) => {
                    let Some((mid, n)) = target.rsplit_once('.') else { return vec![] };
                    match modules.get(mid) {
                        Some(h) => (h, n.to_string()),
                        None => return vec![],
                    }
                }
                _ => return vec![],
            },
        };
        let sum = format!("{}.{alias}", home.id);
        if nodes.contains_key(&sum) {
            return vec![sum];
        }
        home.aliases.get(&alias).map(|(ts, _)| ts.iter().filter_map(|x| resolve(home, x, modules, nodes)).collect()).unwrap_or_default()
    };
    let ids: Vec<String> = store.nodes.keys().cloned().collect();
    // bases and typed fields
    for id in &ids {
        let n = &store.nodes[id];
        if n.kind != NodeKind::Class {
            continue;
        }
        let Some(m) = modules.get(&n.module) else { continue };
        let info = store.classes.get(id).cloned().unwrap_or_default();
        let mut resolved = vec![];
        for b in &info.bases {
            if let Some(r) = resolve(m, b, modules, &store.nodes) {
                if r.starts_with("ext:") {
                    store.ext_node(&r);
                }
                store.add_edge(id, &r, EdgeKind::Inherits, None);
                resolved.push(r);
            }
        }
        store.classes.entry(id.clone()).or_default().resolved = resolved;
        for (fname, types) in &info.fields {
            for t in types {
                for r in type_of(m, t, &store.nodes).into_iter().filter(|r| !r.starts_with("ext:") || foreign_type(r)) {
                    if r.starts_with("ext:") {
                        store.ext_node(&r);
                    } else {
                        store.classes.entry(id.clone()).or_default().attr_types.insert(fname.clone(), r.clone());
                    }
                    store.add_edge(id, &r, EdgeKind::Has, Some(fname.clone()));
                }
            }
        }
    }
    // signatures -> takes / returns
    for id in &ids {
        let n = store.nodes[id].clone();
        let Some(m) = modules.get(&n.module) else { continue };
        if matches!(n.kind, NodeKind::Function | NodeKind::Method) {
            let sig = store.sigs.get(id).cloned().unwrap_or_default();
            for (types, kind) in [(&sig.ptypes, EdgeKind::Takes), (&sig.rtypes, EdgeKind::Returns)] {
                for t in types {
                    for r in type_of(m, t, &store.nodes) {
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
        }
    }
    // the class a function hands on, for typing what a body holds
    let mut returned: HashMap<String, String> = HashMap::new();
    for e in store.edges.iter().filter(|e| e.kind == EdgeKind::Returns) {
        if store.nodes.get(&e.dst).map_or(false, |x| x.kind == NodeKind::Class) {
            returned.entry(e.src.clone()).or_insert_with(|| e.dst.clone());
        }
    }
    // bodies -> calls / instantiates / boundaries
    for id in &ids {
        let n = &store.nodes[id];
        let Some(m) = modules.get(&n.module) else { continue };
        if matches!(n.kind, NodeKind::Function | NodeKind::Method) || (n.kind == NodeKind::Module && modules.contains_key(id)) {
            link_calls(m, modules, store, id, &returned);
        }
    }
}

/// The class a module-level value holds, in its own module or the one it is imported from.
fn value_type(m: &Module, name: &str, modules: &IndexMap<String, Module>, nodes: &IndexMap<String, Node>) -> Option<String> {
    let (home, local) = match m.values.contains_key(name) {
        true => (m, name.to_string()),
        false => match m.bindings.get(name)? {
            Bound::Own(target) => {
                let (mid, n) = target.rsplit_once('.')?;
                (modules.get(mid)?, n.to_string())
            }
            Bound::Ext(_) => return None,
        },
    };
    resolve(home, home.values.get(&local)?, modules, nodes).filter(|r| nodes.get(r).map_or(false, |x| x.kind == NodeKind::Class))
}

/// The method `name` of a class or one of its bases.
fn method_of(store: &GraphBuilder, cls: &str, name: &str, depth: u32) -> Option<String> {
    let name = if name == "constructor" { "__init__" } else { name };
    let cand = format!("{cls}.{name}");
    if store.nodes.contains_key(&cand) {
        return Some(cand);
    }
    if depth > 8 {
        return None;
    }
    store.classes.get(cls)?.resolved.iter().filter(|b| !b.starts_with("ext:")).find_map(|b| method_of(store, b, name, depth + 1))
}

/// Inside a module's body, what belongs to the module and not to a function defined in it.
fn module_level(n: TsNode, root: TsNode, bodies: &HashMap<String, (usize, usize)>) -> bool {
    let inside = bodies.values().any(|(s, e)| *s <= n.start_byte() && n.end_byte() <= *e);
    !inside && n.start_byte() >= root.start_byte()
}

fn member_parts(n: TsNode, src: &[u8]) -> Vec<String> {
    match n.kind() {
        "identifier" | "this" | "property_identifier" | "type_identifier" => vec![text(n, src).to_string()],
        "member_expression" | "nested_identifier" | "jsx_namespace_name" => {
            let (Some(o), Some(p)) = (n.child_by_field_name("object"), n.child_by_field_name("property")) else {
                return text(n, src).split('.').map(str::to_string).collect();
            };
            let mut base = member_parts(o, src);
            if base.is_empty() {
                return vec![];
            }
            base.push(text(p, src).to_string());
            base
        }
        _ => vec![],
    }
}

fn link_calls(m: &Module, modules: &IndexMap<String, Module>, store: &mut GraphBuilder, id: &str, returned: &HashMap<String, String>) {
    let n = store.nodes[id].clone();
    let src = &m.src;
    let root = m.tree.root_node();
    let is_module = n.kind == NodeKind::Module;
    let body = if is_module {
        root
    } else {
        let Some((s, e)) = m.bodies.get(id).copied() else { return };
        let Some(b) = root.descendant_for_byte_range(s, e) else { return };
        b
    };
    // within a module's own body, a function's body is the function's
    let others: HashMap<String, (usize, usize)> = if is_module { m.bodies.clone() } else { HashMap::new() };
    let mine = |x: TsNode| !is_module || module_level(x, root, &others);
    // parameters and locals with a class type give receivers a type
    let mut locals: HashMap<String, String> = HashMap::new();
    let sig_params = n.params.clone();
    for p in &sig_params {
        let Some((name, ty)) = p.split_once(':') else { continue };
        let name = name.trim().trim_end_matches('?');
        let ty = ty.trim().split(['<', '[', ' ', '|']).next().unwrap_or("").to_string();
        if let Some(r) = resolve(m, &[ty], modules, &store.nodes).filter(|r| store.nodes.get(r).map_or(false, |x| x.kind == NodeKind::Class)) {
            locals.insert(name.to_string(), r);
        }
    }
    for v in walk(body, &["variable_declarator"]).into_iter().filter(|v| mine(*v)) {
        let (Some(name), Some(value)) = (v.child_by_field_name("name"), v.child_by_field_name("value")) else { continue };
        if name.kind() != "identifier" {
            continue;
        }
        let declared = v.child_by_field_name("type").and_then(|t| named_children(t).into_iter().next()).map(|t| member_parts(t, src));
        let built = (value.kind() == "new_expression").then(|| value.child_by_field_name("constructor").map(|c| member_parts(c, src))).flatten();
        let typed = declared.or(built).and_then(|t| resolve(m, &t, modules, &store.nodes)).filter(|r| store.nodes.get(r).map_or(false, |x| x.kind == NodeKind::Class));
        // `const p = await getProvider()`: what the function hands on
        let handed = || {
            let call = walk(value, &["call_expression"]).into_iter().next()?;
            let f = resolve(m, &member_parts(call.child_by_field_name("function")?, src), modules, &store.nodes)?;
            returned.get(&f).cloned()
        };
        if let Some(r) = typed.or_else(handed) {
            locals.insert(text(name, src).to_string(), r);
        }
    }
    let cls = n.cls.clone();
    let attrs = cls.as_ref().and_then(|c| store.classes.get(c)).map(|c| c.attr_types.clone()).unwrap_or_default();
    let choices = number_choices(body, src, &CHOICES);
    let mut new_edges: Vec<(String, EdgeKind, Vec<(u32, u32)>)> = vec![];
    let mut touches: Vec<Touch> = vec![];
    let own = |r: &str, store: &GraphBuilder| !r.starts_with("ext:") && store.nodes.contains_key(r);
    let sites: Vec<TsNode> = evaluation_order(walk(body, &["call_expression", "new_expression", "jsx_opening_element", "jsx_self_closing_element", "identifier"]))
        .into_iter()
        .filter(|x| mine(*x))
        .collect();
    for c in sites {
        let arms = arms_of(c, body, src, &choices, &CHOICES);
        match c.kind() {
            "new_expression" => {
                let parts = c.child_by_field_name("constructor").map(|x| member_parts(x, src)).unwrap_or_default();
                if let Some(r) = resolve(m, &parts, modules, &store.nodes).filter(|r| own(r, store)) {
                    if store.nodes[&r].kind == NodeKind::Class {
                        new_edges.push((r, EdgeKind::Instantiates, arms));
                    }
                }
            }
            "jsx_opening_element" | "jsx_self_closing_element" => {
                // `<Kaart/>` renders a component; a lowercase name is an element of the page
                let parts = c.child_by_field_name("name").map(|x| member_parts(x, src)).unwrap_or_default();
                if parts.first().map_or(true, |p| !p.starts_with(|ch: char| ch.is_uppercase()) && parts.len() == 1) {
                    continue;
                }
                if let Some(r) = resolve(m, &parts, modules, &store.nodes).filter(|r| own(r, store)) {
                    let kind = if store.nodes[&r].kind == NodeKind::Class { EdgeKind::Instantiates } else { EdgeKind::Calls };
                    new_edges.push((r, kind, arms));
                }
            }
            "identifier" => {
                // an own function handed on as a value is called by whoever receives it
                let handed = c.parent().map_or(false, |p| match p.kind() {
                    "arguments" | "array" | "jsx_expression" | "return_statement" => true,
                    "pair" => p.child_by_field_name("value").map(|v| v.id()) == Some(c.id()),
                    "variable_declarator" => p.child_by_field_name("value").map(|v| v.id()) == Some(c.id()),
                    _ => false,
                });
                if !handed {
                    continue;
                }
                if let Some(r) = resolve(m, &[text(c, src).to_string()], modules, &store.nodes).filter(|r| own(r, store)) {
                    if matches!(store.nodes[&r].kind, NodeKind::Function | NodeKind::Method) {
                        new_edges.push((r, EdgeKind::Calls, arms));
                    }
                }
            }
            _ => {
                let Some(f) = c.child_by_field_name("function") else { continue };
                let parts = member_parts(f, src);
                // the receiver of a method, typed however the body knows it
                let receiver = |o: TsNode| -> Option<String> {
                    let mut o = o;
                    while matches!(o.kind(), "parenthesized_expression" | "await_expression" | "non_null_expression") {
                        o = named_children(o).into_iter().next()?;
                    }
                    match o.kind() {
                        "this" => cls.clone(),
                        "identifier" => locals.get(text(o, src)).cloned().or_else(|| value_type(m, text(o, src), modules, &store.nodes)),
                        "call_expression" => {
                            let fid = resolve(m, &member_parts(o.child_by_field_name("function")?, src), modules, &store.nodes)?;
                            returned.get(&fid).cloned()
                        }
                        "member_expression" if o.child_by_field_name("object").map(|x| x.kind()) == Some("this") => {
                            attrs.get(text(o.child_by_field_name("property")?, src)).cloned()
                        }
                        _ => None,
                    }
                };
                let on = (f.kind() == "member_expression").then(|| Some((f.child_by_field_name("object")?, text(f.child_by_field_name("property")?, src).to_string()))).flatten();
                let target = match on.and_then(|(o, name)| receiver(o).and_then(|k| method_of(store, &k, &name, 0))) {
                    Some(t) => Some(t),
                    None if parts.is_empty() => continue,
                    None => resolve(m, &parts, modules, &store.nodes),
                };
                match target {
                    Some(t) if own(&t, store) => {
                        let kind = if store.nodes[&t].kind == NodeKind::Class { EdgeKind::Instantiates } else { EdgeKind::Calls };
                        new_edges.push((t, kind, arms));
                    }
                    _ => {
                        let key = parts.iter().take(2).cloned().collect::<Vec<_>>().join(".");
                        *store.unresolved.entry(key).or_insert(0) += 1;
                        let pkg = match m.bindings.get(&parts[0]) {
                            Some(Bound::Ext(e)) => Some(package_from_ext(e, &m.ext_pkgs)),
                            _ => None,
                        };
                        if let Some(kind) = effects::boundary_ts(pkg.as_deref(), &parts) {
                            let op = parts.last().cloned().unwrap_or_default();
                            let kinds = &["string", "template_string"];
                            let f = effects::facts(kind, pkg.as_deref(), &op, &literals(c, src, kinds));
                            let what = what_of(c, src, kinds, "arguments");
                            let payload = payload_of(c, src, &what, "arguments", &HashMap::new());
                            let fixed = effects::locates(f.kind) && starts_at_literal(c, src, kinds, "arguments", effects::on_receiver(&op));
                            for (artifact, dir) in f.places() {
                                touches.push(Touch {
                                    kind: f.kind.into(),
                                    what: what.clone(),
                                    payload: payload.clone(),
                                    op: op.clone(),
                                    artifact,
                                    dir: dir.into(),
                                    fixed,
                                    line: c.start_position().row as u32 + 1,
                                    n: 1,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    // a component renders what it returns to the screen: a page's route ends where the human sees it
    if !is_module {
        if let Some(jsx) = walk(body, &["jsx_element", "jsx_self_closing_element", "jsx_fragment"]).into_iter().next() {
            touches.push(Touch { kind: "screen".into(), op: "render".into(), line: jsx.start_position().row as u32 + 1, n: 1, ..Default::default() });
        }
    }
    for (t, kind, arms) in new_edges {
        store.add_call(id, &t, kind, None, arms);
    }
    for t in touches {
        store.touch(id, t);
    }
    if !is_module {
        let cmp = compares(body, src, &choices);
        if let Some(node) = store.nodes.get_mut(id) {
            node.compares = cmp;
        }
    }
}

/// The package an `ext:` binding came from, as the module imported it.
fn package_from_ext(id: &str, pkgs: &HashSet<String>) -> String {
    let path = id.trim_start_matches("ext:").replace('.', "/");
    pkgs.iter().filter(|p| path.starts_with(p.as_str())).max_by_key(|p| p.len()).cloned().unwrap_or_else(|| package_of(&path))
}

// ---------- choices ----------

/// TypeScript's choices: `if`/`else if`/`else`, `a ? b : c`, `switch`, `try`/`catch`,
/// `x ?? y` and `x || y`.
const CHOICES: ChoiceSyntax = ChoiceSyntax {
    chooses: |n, src| match n.kind() {
        "if_statement" | "ternary_expression" | "switch_body" | "try_statement" => true,
        "binary_expression" => n.child_by_field_name("operator").map_or(false, |o| matches!(text(o, src), "||" | "??")),
        _ => false,
    },
    continues: |n| {
        let up = n.parent().filter(|p| p.kind() == "else_clause")?.parent()?;
        (n.kind() == "if_statement" && up.kind() == "if_statement").then_some(up)
    },
    part: |c, child, _| {
        let is = |f: &str| c.child_by_field_name(f).map(|x| x.id()) == Some(child.id());
        let depth = || {
            let mut d = 0;
            let mut cur = c;
            while let Some(up) = cur.parent().filter(|p| p.kind() == "else_clause").and_then(|e| e.parent()).filter(|p| p.kind() == "if_statement") {
                d += 1;
                cur = up;
            }
            d
        };
        match c.kind() {
            "if_statement" if is("condition") => Part::Decides,
            "if_statement" if is("consequence") => Part::Arm(depth()),
            "if_statement" if is("alternative") => {
                if named_children(child).iter().any(|x| x.kind() == "if_statement") {
                    Part::Through
                } else {
                    Part::Arm(depth() + 1)
                }
            }
            "ternary_expression" if is("condition") => Part::Decides,
            "ternary_expression" if is("consequence") => Part::Arm(0),
            "ternary_expression" if is("alternative") => Part::Arm(1),
            "switch_body" if child.kind() == "switch_case" => Part::Arm(nth_of_kind(c, child)),
            // `default` comes after every case
            "switch_body" if child.kind() == "switch_default" => Part::Arm(named_children(c).iter().filter(|x| x.kind() == "switch_case").count() as u32),
            "try_statement" if is("body") => Part::Arm(0),
            "try_statement" if is("handler") => Part::Arm(1),
            "binary_expression" if is("left") => Part::Arm(0),
            "binary_expression" if is("right") => Part::Arm(1),
            _ => Part::Through,
        }
    },
};

/// Where a body compares one value with string literals (`mode === "kaart"`, a `switch` over
/// strings), and the choice each comparison decides.
fn compares(body: TsNode, src: &[u8], choices: &HashMap<usize, u32>) -> Vec<Compare> {
    let lit = |n: TsNode| n.kind() == "string";
    let mut out = vec![];
    for n in walk(body, &["binary_expression", "switch_statement"]) {
        let line = n.start_position().row as u32 + 1;
        if n.kind() == "switch_statement" {
            let (Some(value), Some(sbody)) = (n.child_by_field_name("value"), n.child_by_field_name("body")) else { continue };
            let cases: Vec<String> = named_children(sbody)
                .into_iter()
                .filter_map(|c| c.child_by_field_name("value"))
                .filter(|v| lit(*v))
                .map(|v| unquote(text(v, src)))
                .collect();
            if !cases.is_empty() {
                let subject = text(value, src).trim_start_matches('(').trim_end_matches(')').to_string();
                out.push(Compare { subject, cases, line, choice: choices.get(&sbody.id()).copied() });
            }
            continue;
        }
        let op = n.child_by_field_name("operator").map(|o| text(o, src)).unwrap_or("");
        if !matches!(op, "===" | "==" | "!==" | "!=") {
            continue;
        }
        let (Some(l), Some(r)) = (n.child_by_field_name("left"), n.child_by_field_name("right")) else { continue };
        let (subject, case) = match (lit(l), lit(r)) {
            (false, true) => (l, r),
            (true, false) => (r, l),
            _ => continue,
        };
        if matches!(subject.kind(), "identifier" | "member_expression") {
            out.push(Compare { subject: text(subject, src).to_string(), cases: vec![unquote(text(case, src))], line, choice: decides(n, body, src, choices, &CHOICES) });
        }
    }
    out
}
