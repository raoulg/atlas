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

mod paths;

use paths::{Arg, At, Eval};

use crate::builder::{foreign_type, ClassInfo, GraphBuilder, OwnTypes, SigInfo};
use crate::effects::{self, Crossing};
use crate::model::{EdgeKind, Entry, EntryKind, Held, Node, NodeKind, Param, Returns, Site, Touch, World};
use crate::tree::{
    arms_of, evaluation_order, literals, named_children, nth_of_kind, number_choices, payload_of, span, starts_at_literal, text, unquote,
    walk, what_of, ChoiceSyntax, CompareSyntax, Part,
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
/// Packages whose types describe the code rather than data it moves: a UI framework's
/// components and props, a schema library's schemas.
const OWN: OwnTypes = OwnTypes { packages: &["react", "react-dom", "preact", "vue", "svelte", "solid-js", "zod", "yup", "valibot", "superstruct", "io-ts"], names: &[] };
/// Web frameworks whose router registers a route by a call: `app.get("/x", handler)`.
const ROUTERS: &[&str] = &["hono", "express", "fastify", "koa-router", "@koa/router", "elysia", "itty-router", "h3"];
/// A router's registration calls, by HTTP method.
const VERBS: &[&str] = &["get", "post", "put", "delete", "patch", "all"];

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
    /// The names the module exports: `export function`, `export { a as b }`, `export * from`.
    exported: Vec<String>,
    /// Module-level values that are a web framework's router: `const app = new Hono()`.
    routers: HashSet<String>,
    /// Route handlers by the router they are registered on: (handler id, router name).
    routes: Vec<(String, String)>,
    /// Routers mounted on another under a prefix: (router, prefix, mounted router).
    mounts: Vec<(String, String, String)>,
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

/// A signature as data: each parameter by name and type (`(id: string, opt?: Opties)`), the types
/// they name, and what it gives back. `...rest` takes whatever arguments are left; a `this`
/// parameter is the value the function is called on.
fn signature(params: Option<TsNode>, ret: Option<TsNode>, src: &[u8]) -> (Vec<Param>, Option<Returns>, SigInfo) {
    let mut out = vec![];
    let mut info = SigInfo::default();
    if let Some(ps) = params {
        // an arrow with one bare parameter: `x => ..`
        if ps.kind() == "identifier" {
            out.push(Param { name: text(ps, src).to_string(), ..Default::default() });
        } else {
            for p in named_children(ps).into_iter().filter(|p| p.kind() != "comment") {
                let pattern = p.child_by_field_name("pattern");
                let written = pattern.map(|x| text(x, src)).unwrap_or_else(|| text(p, src));
                let name = written.split_whitespace().collect::<Vec<_>>().join(" ");
                let ty = p.child_by_field_name("type");
                out.push(Param {
                    name: name.trim_start_matches("...").to_string(),
                    ty: ty.and_then(|t| named_children(t).into_iter().next()).map(|t| text(t, src).to_string()).unwrap_or_default(),
                    receiver: pattern.map_or(false, |x| x.kind() == "this"),
                    rest: pattern.map_or(false, |x| x.kind() == "rest_pattern"),
                    changes: false,
                });
                info.ptypes.extend(type_names(ty, src));
            }
        }
    }
    let returns = ret.and_then(|r| named_children(r).into_iter().next()).map(|r| returns(r, src));
    info.rtypes = type_names(ret, src);
    (out, returns, info)
}

/// What a return type gives: a `Promise<T>` gives `T` once it arrives, `void` and `undefined` no
/// value, `this` a value of the class it runs on.
fn returns(rt: TsNode, src: &[u8]) -> Returns {
    let mut t = rt;
    while t.kind() == "generic_type" && t.child_by_field_name("name").map_or(false, |n| text(n, src) == "Promise") {
        match t.child_by_field_name("type_arguments").and_then(|a| named_children(a).into_iter().next()) {
            Some(i) => t = i,
            None => break,
        }
    }
    let (value, own) = match (t.kind(), text(t, src)) {
        ("predefined_type", "void" | "undefined") => (String::new(), false),
        ("this_type", _) => (String::new(), true),
        (_, v) => (v.to_string(), false),
    };
    Returns { written: text(rt, src).to_string(), value, own }
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
                exported: vec![],
                routers: HashSet::new(),
                routes: vec![],
                mounts: vec![],
            },
        );
    }
    for m in modules.values_mut() {
        collect_imports(m, &aliases, &files);
        collect_wrapped(m);
        collect_symbols(m, &mut store);
        collect_routes(m, &mut store);
    }
    route_prefixes(&modules, &mut store);
    mark_bins(repo, &files, &mut store);
    mark_exports(repo, &files, &modules, &mut store);
    link(&modules, &mut store, repo);
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
        Some(c) => format!("{c}.{name}"),
        None => format!("{}.{name}", m.id),
    };
    let params = f.child_by_field_name("parameters").or_else(|| f.child_by_field_name("parameter"));
    let (params, returns, sig) = signature(params, f.child_by_field_name("return_type"), src);
    store.sigs.insert(id.clone(), sig);
    let on_class = cls.is_some() && (0..def.child_count()).filter_map(|i| def.child(i)).any(|c| c.kind() == "static");
    // the constructor initializes what `new` makes; a static method that gives its class constructs
    let class_name = cls.map(|c| c.rsplit('.').next().unwrap_or(c));
    let gives_class = returns.as_ref().map_or(false, |r| r.own || Some(r.value.as_str()) == class_name);
    store.add_node(Node {
        id: id.clone(),
        kind: if cls.is_some() { NodeKind::Method } else { NodeKind::Function },
        on_class,
        ctor: cls.is_some() && (name == "constructor" || on_class && gives_class),
        // a member is private when it says so; a module's own name is, unless the module exports it
        private: cls.is_some() && (name.starts_with('#') || named_children(def).iter().any(|c| c.kind() == "accessibility_modifier" && text(*c, src) != "public")),
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
    for (d, exported, default) in declarations(tree.root_node()) {
        let name = d.child_by_field_name("name").map(|n| text(n, &src).to_string()).unwrap_or_default();
        if exported && !name.is_empty() {
            m.exported.push(name.clone());
        }
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
                    if exported {
                        m.exported.push(vname.clone());
                    }
                    // an object of functions is a namespace: `api.get(..)` calls its `get`; one with a
                    // declared type is a value of that type
                    if value.kind() == "object" && v.child_by_field_name("type").is_none() && namespace(m, store, &vname, value) {
                        continue;
                    }
                    // a router is a value its registrations hand routes to, not a function
                    if router_made(value, m, &src) {
                        m.routers.insert(vname);
                        continue;
                    }
                    let Some(f) = function_in(value) else {
                        let declared = v.child_by_field_name("type").and_then(|t| named_children(t).into_iter().next()).map(|t| type_head(t, &src));
                        let built = (value.kind() == "new_expression").then(|| value.child_by_field_name("constructor").map(|c| member_parts(c, &src))).flatten();
                        if let Some(t) = declared.or(built).filter(|t| !t.is_empty()) {
                            m.values.insert(vname, t);
                        }
                        continue;
                    };
                    let id = add_function(m, store, &vname, d, f, value, None);
                    // an arrow handed to a call takes its parameters' types from the call
                    if f.id() != value.id() {
                        if let (Some(n), Some(call)) = (store.nodes.get_mut(&id), f.parent().and_then(|a| a.parent()).filter(|c| c.kind() == "call_expression")) {
                            n.typed_by = call.child_by_field_name("function").map(|x| text(x, &src).to_string()).unwrap_or_default();
                        }
                    }
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
    for n in store.nodes.values_mut().filter(|n| n.module == m.id && n.cls.is_none() && matches!(n.kind, NodeKind::Function | NodeKind::Class)) {
        let local = n.name.split('.').next().unwrap_or(&n.name);
        n.private = !m.exported.iter().any(|e| e == local);
    }
}

/// The functions of an object literal, each a function of the module named by its key
/// (`api.get`); false when the object holds none.
fn namespace(m: &mut Module, store: &mut GraphBuilder, name: &str, object: TsNode) -> bool {
    let src = m.src.clone();
    let mut any = false;
    for member in named_children(object) {
        let (key, f, body) = match member.kind() {
            "pair" => match (member.child_by_field_name("key"), member.child_by_field_name("value").filter(|v| matches!(v.kind(), "arrow_function" | "function_expression" | "function"))) {
                (Some(k), Some(v)) => (k, v, v),
                _ => continue,
            },
            "method_definition" => match (member.child_by_field_name("name"), member.child_by_field_name("body")) {
                (Some(k), Some(b)) => (k, member, b),
                _ => continue,
            },
            _ => continue,
        };
        let key = if key.kind() == "string" { unquote(text(key, &src)) } else { text(key, &src).to_string() };
        let id = add_function(m, store, &format!("{name}.{key}"), member, f, body, None);
        store.add_edge(&m.id.clone(), &id, EdgeKind::Contains, None);
        any = true;
    }
    if any {
        m.symbols.insert(name.to_string(), format!("{}.{name}", m.id));
    }
    any
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
                    m.exported.push(text(alias.unwrap_or(n), &src).to_string());
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
                    m.exported.push(text(local, &src).to_string());
                    m.bindings.insert(text(local, &src).to_string(), member(Some(text(n, &src))));
                }
            }
            _ => {}
        }
    }
}

/// The module a path in `package.json` names, with or without its extension.
fn module_at(repo: &Path, path: &str, files: &HashMap<PathBuf, String>) -> Option<String> {
    let p = normalize(&repo.join(path));
    files.get(&p).cloned().or_else(|| files.iter().find(|(f, _)| f.with_extension("") == p.with_extension("")).map(|(_, id)| id.clone()))
}

/// `package.json` `bin`, and each script that runs a file of the package (`tsx server/index.ts`):
/// a command, a CLI route into the module it runs. Scripts that run one module name it together.
fn mark_bins(repo: &Path, files: &HashMap<PathBuf, String>, store: &mut GraphBuilder) {
    let Ok(text) = std::fs::read_to_string(repo.join("package.json")) else { return };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else { return };
    let bins: Vec<(String, String)> = match &json["bin"] {
        serde_json::Value::String(p) => vec![(json["name"].as_str().unwrap_or("bin").to_string(), p.clone())],
        serde_json::Value::Object(o) => o.iter().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string()))).collect(),
        _ => vec![],
    };
    for (name, path) in bins {
        if let Some(n) = module_at(repo, &path, files).and_then(|id| store.nodes.get_mut(&id)) {
            n.entry = Some(Entry { kind: EntryKind::Cli, label: name });
        }
    }
    let mut scripts: IndexMap<String, Vec<String>> = IndexMap::new();
    for (name, cmd) in json["scripts"].as_object().into_iter().flatten() {
        let Some(cmd) = cmd.as_str() else { continue };
        if let Some(id) = cmd.split_whitespace().find_map(|w| module_at(repo, w.trim_matches(['"', '\'']), files)) {
            scripts.entry(id).or_default().push(name.clone());
        }
    }
    for (id, mut names) in scripts {
        names.sort_by_key(|n| (n.len(), n.clone()));
        if let Some(n) = store.nodes.get_mut(&id).filter(|n| n.entry.is_none()) {
            n.entry = Some(Entry { kind: EntryKind::Cli, label: names.join(" · ") });
        }
    }
}

/// What a package exports is a route into it: the names its entry module exports, the module
/// `package.json` points to (`exports`, `module`, `main`) or else the index at its root. A
/// name re-exported is followed to where it is defined.
fn mark_exports(repo: &Path, files: &HashMap<PathBuf, String>, modules: &IndexMap<String, Module>, store: &mut GraphBuilder) {
    let json = std::fs::read_to_string(repo.join("package.json")).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()).unwrap_or_default();
    let dot = |v: &serde_json::Value| -> Option<String> {
        match v {
            serde_json::Value::String(s) => Some(s.clone()),
            serde_json::Value::Object(o) => ["import", "default", "types", "require"].iter().find_map(|k| o.get(*k).and_then(|x| x.as_str()).map(str::to_string)),
            _ => None,
        }
    };
    let declared = [json["exports"].get(".").and_then(dot), dot(&json["exports"]), json["module"].as_str().map(str::to_string), json["main"].as_str().map(str::to_string)];
    let root = files.values().find(|id| modules.get(*id).map_or(false, |m| m.path.file_stem().and_then(|s| s.to_str()) == Some("index") && !id.contains('.'))).cloned();
    let Some(entry) = declared.into_iter().flatten().find_map(|p| module_at(repo, &p, files)).or(root) else { return };
    let mut names: Vec<(String, String)> = vec![];
    let mut stack = vec![(entry.clone(), 0)];
    while let Some((mid, depth)) = stack.pop() {
        let Some(m) = modules.get(&mid) else { continue };
        names.extend(m.exported.iter().map(|n| (n.clone(), format!("{mid}.{n}"))));
        if depth < 8 {
            stack.extend(m.wildcards.iter().map(|w| (w.clone(), depth + 1)));
        }
    }
    for (name, target) in names {
        let Some(id) = defined(&target, modules, &store.nodes, 0) else { continue };
        if let Some(n) = store.nodes.get_mut(&id).filter(|n| matches!(n.kind, NodeKind::Function | NodeKind::Class) && n.entry.is_none()) {
            n.entry = Some(Entry { kind: EntryKind::Export, label: name });
        }
    }
}

/// Whether a value is made by a web framework's router: `new Hono()`, `express()`,
/// `express.Router()`, and the chain of registrations on it (`new Hono().get(..).post(..)`).
fn router_made(mut v: TsNode, m: &Module, src: &[u8]) -> bool {
    loop {
        match v.kind() {
            "call_expression" => match v.child_by_field_name("function") {
                Some(f) if f.kind() == "member_expression" && f.child_by_field_name("object").map_or(false, |o| matches!(o.kind(), "call_expression" | "new_expression")) => {
                    v = f.child_by_field_name("object").unwrap();
                }
                Some(f) => return router_pkg(&member_parts(f, src), m),
                None => return false,
            },
            "new_expression" => return v.child_by_field_name("constructor").map_or(false, |c| router_pkg(&member_parts(c, src), m)),
            "parenthesized_expression" | "await_expression" => match named_children(v).into_iter().next() {
                Some(x) => v = x,
                None => return false,
            },
            _ => return false,
        }
    }
}

/// Module-level names for an imported function wrapped to answer with a promise:
/// `const run = promisify(execFile)` is `execFile`.
fn collect_wrapped(m: &mut Module) {
    let mut wrapped = vec![];
    for (d, _, _) in declarations(m.tree.root_node()) {
        for v in named_children(d).into_iter().filter(|v| v.kind() == "variable_declarator") {
            let (Some(n), Some(value)) = (v.child_by_field_name("name"), v.child_by_field_name("value")) else { continue };
            if let Some(b) = promisified(value, m, &m.src) {
                wrapped.push((text(n, &m.src).to_string(), b));
            }
        }
    }
    m.bindings.extend(wrapped);
}

/// What `util.promisify(f)` wraps, when `f` is bound.
fn promisified(value: TsNode, m: &Module, src: &[u8]) -> Option<Bound> {
    let f = value.child_by_field_name("function").filter(|_| value.kind() == "call_expression")?;
    let parts = member_parts(f, src);
    let pkg = match m.bindings.get(parts.first()?)? {
        Bound::Ext(e) => package_from_ext(e, &m.ext_pkgs),
        Bound::Own(_) => return None,
    };
    (pkg == "util" && parts.last().map(String::as_str) == Some("promisify")).then_some(())?;
    let arg = named_children(value.child_by_field_name("arguments")?).into_iter().next().filter(|a| a.kind() == "identifier")?;
    m.bindings.get(text(arg, src)).cloned()
}

/// The package a client was made by: `new Anthropic()`, `new Pool()`, `axios.create()`,
/// `createClient()`. What a package's other calls give back is a result, not a client.
fn made_by_package(value: TsNode, m: &Module, src: &[u8]) -> Option<String> {
    let mut v = value;
    while matches!(v.kind(), "await_expression" | "parenthesized_expression" | "as_expression" | "non_null_expression") {
        v = named_children(v).into_iter().next()?;
    }
    let maker = match v.kind() {
        "new_expression" => v.child_by_field_name("constructor")?,
        "call_expression" => v.child_by_field_name("function").filter(|f| {
            let last = member_parts(*f, src).pop().unwrap_or_default();
            last.starts_with("create") || matches!(last.as_str(), "connect" | "open")
        })?,
        _ => return None,
    };
    match m.bindings.get(member_parts(maker, src).first()?)? {
        Bound::Ext(e) => Some(package_from_ext(e, &m.ext_pkgs)),
        Bound::Own(_) => None,
    }
}

fn router_pkg(parts: &[String], m: &Module) -> bool {
    match parts.first().and_then(|h| m.bindings.get(h)) {
        Some(Bound::Ext(e)) => ROUTERS.contains(&package_from_ext(e, &m.ext_pkgs).as_str()),
        _ => false,
    }
}

/// The routes a module registers on its routers: `app.get("/x", (c) => ..)` is a handler at
/// `GET /x`, and `app.route("/api", sub)` (`app.use` in Express) mounts one router on another.
/// An inline handler is a function of its own, named by its route.
fn collect_routes(m: &mut Module, store: &mut GraphBuilder) {
    let tree = m.tree.clone();
    let src = m.src.clone();
    let lit = |n: TsNode| matches!(n.kind(), "string" | "template_string") && !walk(n, &["template_substitution"]).iter().any(|_| true);
    for c in walk(tree.root_node(), &["call_expression"]) {
        let Some(f) = c.child_by_field_name("function").filter(|f| f.kind() == "member_expression") else { continue };
        let (Some(obj), Some(prop)) = (f.child_by_field_name("object"), f.child_by_field_name("property")) else { continue };
        let verb = text(prop, &src).to_string();
        // the router: a module-level router by name, or the chain a declaration makes
        let router = match obj.kind() {
            "identifier" if m.routers.contains(text(obj, &src)) => text(obj, &src).to_string(),
            _ => {
                let mut up = c.parent();
                while let Some(p) = up.filter(|p| matches!(p.kind(), "member_expression" | "call_expression" | "parenthesized_expression")) {
                    up = p.parent();
                }
                let declared = up.filter(|p| p.kind() == "variable_declarator").and_then(|d| d.child_by_field_name("name")).map(|n| text(n, &src).to_string());
                // or a chain of registrations on a named router: `app.get(..).post(..)`
                let chained = || {
                    let mut base = obj;
                    while base.kind() == "call_expression" {
                        base = base.child_by_field_name("function").and_then(|f| f.child_by_field_name("object"))?;
                    }
                    (base.kind() == "identifier").then(|| text(base, &src).to_string())
                };
                match declared.or_else(chained) {
                    Some(name) if m.routers.contains(&name) => name,
                    _ => continue,
                }
            }
        };
        let args = c.child_by_field_name("arguments").map(named_children).unwrap_or_default();
        let Some(path) = args.first().filter(|a| lit(**a)).map(|a| unquote(text(*a, &src))) else { continue };
        if matches!(verb.as_str(), "route" | "use") {
            if let Some(sub) = args.get(1).filter(|a| a.kind() == "identifier") {
                m.mounts.push((router.clone(), path, text(*sub, &src).to_string()));
            }
            continue;
        }
        if !VERBS.contains(&verb.as_str()) {
            continue;
        }
        let method = if verb == "all" { "GET".to_string() } else { verb.to_uppercase() };
        let label = format!("{method} {path}");
        let Some(handler) = args.last().filter(|a| a.id() != args[0].id()) else { continue };
        let id = match handler.kind() {
            "arrow_function" | "function_expression" | "function" => {
                let body = handler.child_by_field_name("body").unwrap_or(*handler);
                let id = add_function(m, store, &format!("{router}.{}", label.replace('.', "_")), c, *handler, body, None);
                if let Some(n) = store.nodes.get_mut(&id) {
                    n.name = label.clone();
                }
                store.add_edge(&m.id.clone(), &id, EdgeKind::Contains, None);
                id
            }
            "identifier" => match m.symbols.get(text(*handler, &src)) {
                Some(id) => id.clone(),
                None => continue,
            },
            _ => continue,
        };
        if let Some(n) = store.nodes.get_mut(&id).filter(|n| n.entry.is_none()) {
            n.entry = Some(Entry { kind: EntryKind::Route, label });
            m.routes.push((id, router.clone()));
        }
    }
}

/// A route's full path: the prefixes of the routers it is mounted under, out to the one nobody
/// mounts. `:id` is a segment, written `{id}` as in every other language.
fn route_prefixes(modules: &IndexMap<String, Module>, store: &mut GraphBuilder) {
    // a router by where it is made: the module that makes it, followed through imports
    fn key_of(m: &Module, local: &str, modules: &IndexMap<String, Module>, depth: u32) -> Option<String> {
        if m.routers.contains(local) {
            return Some(format!("{}.{local}", m.id));
        }
        let bound = if local == "default" { m.default.clone() } else { m.bindings.get(local).cloned() };
        let Some(Bound::Own(target)) = bound else { return None };
        let (mid, name) = target.rsplit_once('.').filter(|_| depth < 8)?;
        key_of(modules.get(mid)?, name, modules, depth + 1)
    }
    let mut mounted: HashMap<String, (String, String)> = HashMap::new();
    for m in modules.values() {
        for (parent, prefix, sub) in &m.mounts {
            if let (Some(p), Some(s)) = (key_of(m, parent, modules, 0), key_of(m, sub, modules, 0)) {
                mounted.insert(s, (p, prefix.clone()));
            }
        }
    }
    let full = |key: &str| {
        let (mut key, mut out) = (key.to_string(), String::new());
        for _ in 0..10 {
            let Some((parent, prefix)) = mounted.get(&key) else { break };
            out = format!("{}{out}", prefix.trim_end_matches('/'));
            key = parent.clone();
        }
        out
    };
    for m in modules.values() {
        for (id, router) in &m.routes {
            let prefix = key_of(m, router, modules, 0).map(|k| full(&k)).unwrap_or_default();
            let Some(e) = store.nodes.get_mut(id).and_then(|n| n.entry.as_mut()) else { continue };
            let Some((method, path)) = e.label.split_once(' ') else { continue };
            let path = if path == "/" && !prefix.is_empty() { String::new() } else { path.to_string() };
            let segs: Vec<String> = format!("{prefix}{path}").split('/').map(|s| if let Some(p) = s.strip_prefix(':') { format!("{{{}}}", p.trim_end_matches('?')) } else if s == "*" { "{rest:path}".into() } else { s.to_string() }).collect();
            e.label = format!("{method} {}", segs.join("/"));
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


/// What the bodies hand the places pass: file and network calls with what their path or URL
/// is built from, and the calls of the package's functions with the arguments they pass.
#[derive(Default)]
struct Linked<'t> {
    calls: Vec<(String, Touch, Vec<crate::paths::Built>)>,
    args: Vec<Arg<'t>>,
}

fn link(modules: &IndexMap<String, Module>, store: &mut GraphBuilder, repo: &Path) {
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
                if (!r.starts_with("ext:") || foreign_type(&r, &OWN)) && !variants.contains(&r) {
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
                for r in type_of(m, t, &store.nodes).into_iter().filter(|r| !r.starts_with("ext:") || foreign_type(r, &OWN)) {
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
                            if !foreign_type(&r, &OWN) {
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
    let eval = Eval::new(modules, repo);
    let mut linked = Linked::default();
    for id in &ids {
        let n = &store.nodes[id];
        let Some(m) = modules.get(&n.module) else { continue };
        if matches!(n.kind, NodeKind::Function | NodeKind::Method) || (n.kind == NodeKind::Module && modules.contains_key(id)) {
            link_calls(m, modules, store, id, &returned, &eval, &mut linked);
        }
    }
    place_calls(store, &eval, linked);
}

/// Place the file and network calls: each path or URL, with its function's parameters bound to
/// what each call of it hands them. A file is touched by the function that opens it, at every
/// place it is opened at; a URL without a host that one of the package's routes serves is a
/// request from the function that names it to the route's handler.
fn place_calls(store: &mut GraphBuilder, eval: &Eval, linked: Linked) {
    let routes: Vec<(String, String, String)> = store
        .nodes
        .values()
        .filter_map(|n| {
            let e = n.entry.as_ref().filter(|e| e.kind == EntryKind::Route)?;
            let (method, path) = e.label.split_once(' ')?;
            Some((n.id.clone(), method.to_string(), path.to_string()))
        })
        .collect();
    for (f, t, built) in linked.calls {
        let found: Vec<(String, crate::paths::Built)> = built.iter().flat_map(|b| paths::at_callers(eval, &linked.args, &f, b, 0)).collect();
        let mut placed: Vec<String> = vec![];
        let mut requested = false;
        for (holder, b) in &found {
            let rendered = crate::paths::render(b);
            // a request's method, when the code says it, and its URL
            let (method, url) = match rendered.split_once(' ') {
                Some((m, u)) if t.kind == World::Net => (m.to_ascii_uppercase(), u.to_string()),
                _ => (String::new(), rendered.clone()),
            };
            if t.kind == World::Net {
                if url.starts_with('/') {
                    let known = !method.contains('{');
                    let served: Vec<&(String, String, String)> = routes.iter().filter(|(_, m, p)| (!known || *m == method) && crate::parts::route_matches(p, &url)).collect();
                    // a literal segment serves before a parameter: `/years` before `/{year}`
                    let fewest = served.iter().map(|(_, _, p)| p.matches('{').count()).min().unwrap_or(0);
                    for (handler, _, path) in served.into_iter().filter(|(_, _, p)| p.matches('{').count() == fewest) {
                        store.add_edge(holder, handler, EdgeKind::Requests, Some(path.clone()));
                        requested = true;
                    }
                } else if let Some(host) = effects::facts(Crossing::to(World::Net), None, &t.op, &[url.clone()]).places().into_iter().next().map(|(a, _)| a).filter(|a| !a.is_empty()) {
                    placed.push(host);
                }
            } else if let Some(p) = crate::paths::place(b, &t.op) {
                placed.push(p);
            }
        }
        placed.sort();
        placed.dedup();
        if placed.is_empty() {
            if !requested {
                // a URL without a host is the page's own origin, decided by whoever serves it
                let fixed = t.fixed && (t.kind != World::Net || !t.artifact.is_empty());
                store.touch(&f, Touch { fixed, ..t });
            }
            continue;
        }
        for artifact in placed {
            store.touch(&f, Touch { artifact, ..t.clone() });
        }
    }
}

/// The class a module-level value holds, in its own module or the one it is imported from.
/// The name a call's value is kept by: `const x = await f(..)`, `x = f(..)`.
fn kept_as(c: TsNode, src: &[u8]) -> Option<String> {
    let mut n = c;
    loop {
        let p = n.parent()?;
        match p.kind() {
            "await_expression" | "parenthesized_expression" | "as_expression" | "non_null_expression" | "satisfies_expression" => n = p,
            "variable_declarator" if p.child_by_field_name("value").map(|v| v.id()) == Some(n.id()) => {
                return p.child_by_field_name("name").filter(|x| x.kind() == "identifier").map(|x| text(x, src).to_string());
            }
            "assignment_expression" if p.child_by_field_name("right").map(|r| r.id()) == Some(n.id()) => {
                return p.child_by_field_name("left").filter(|l| l.kind() == "identifier").map(|l| text(l, src).to_string());
            }
            _ => return None,
        }
    }
}

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

fn link_calls<'t>(m: &'t Module, modules: &'t IndexMap<String, Module>, store: &mut GraphBuilder, id: &str, returned: &HashMap<String, String>, eval: &Eval, linked: &mut Linked<'t>) {
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
    let scope = if is_module { None } else { paths::function_node(body) };
    let params: Vec<String> = scope.map(|f| paths::param_names(f, src)).unwrap_or_default();
    let mine = |x: TsNode| !is_module || module_level(x, root, &others);
    // parameters and locals with a class type give receivers a type
    let mut locals: HashMap<String, String> = HashMap::new();
    let sig_params = n.params.clone();
    for p in sig_params.iter().filter(|p| !p.ty.is_empty()) {
        let name = p.name.as_str();
        let ty = p.ty.split(['<', '[', ' ', '|']).next().unwrap_or("").to_string();
        if let Some(r) = resolve(m, &[ty], modules, &store.nodes).filter(|r| store.nodes.get(r).map_or(false, |x| x.kind == NodeKind::Class)) {
            locals.insert(name.to_string(), r);
        }
    }
    let param_types = locals.clone();
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
    // values a package made, by the package: a call on one is the package's
    let mut made: HashMap<String, String> = HashMap::new();
    for v in walk(if is_module { root } else { body }, &["variable_declarator"]) {
        let (Some(name), Some(value)) = (v.child_by_field_name("name"), v.child_by_field_name("value")) else { continue };
        if name.kind() == "identifier" && (mine(v) || is_module || module_level(v, root, &m.bodies)) {
            if let Some(p) = made_by_package(value, m, src) {
                made.insert(text(name, src).to_string(), p);
            }
        }
    }
    if !is_module {
        for v in walk(root, &["variable_declarator"]).into_iter().filter(|v| module_level(*v, root, &m.bodies)) {
            let (Some(name), Some(value)) = (v.child_by_field_name("name"), v.child_by_field_name("value")) else { continue };
            if name.kind() == "identifier" {
                if let Some(p) = made_by_package(value, m, src) {
                    made.entry(text(name, src).to_string()).or_insert(p);
                }
            }
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
    // the calls as steps, in the order they run, and the names the steps so far have kept
    let mut steps: Vec<Site> = vec![];
    let mut kept: HashSet<String> = HashSet::new();
    let ty_of = |name: &str| locals.get(name).cloned().or_else(|| value_type(m, name, modules, &store.nodes));
    let hands_of = |parts: &[Option<TsNode>], kept: &HashSet<String>| -> Vec<Held> {
        let mut out: Vec<Held> = vec![];
        for p in parts.iter().flatten() {
            for h in crate::tree::held_names(*p, src, &[], &ty_of, kept) {
                if !out.contains(&h) {
                    out.push(h);
                }
            }
        }
        out
    };
    let gives_of = |c: TsNode, to: Option<&str>| -> Vec<Held> {
        kept_as(c, src).map(|name| Held::new(locals.get(&name).cloned().or_else(|| to.and_then(|t| returned.get(t).cloned())).unwrap_or_default(), name)).into_iter().collect()
    };
    for &c in &sites {
        let arms = arms_of(c, body, src, &choices, &CHOICES);
        let line = c.start_position().row as u32 + 1;
        match c.kind() {
            "new_expression" => {
                let parts = c.child_by_field_name("constructor").map(|x| member_parts(x, src)).unwrap_or_default();
                if let Some(r) = resolve(m, &parts, modules, &store.nodes).filter(|r| own(r, store)) {
                    if store.nodes[&r].kind == NodeKind::Class {
                        let gives = gives_of(c, Some(&r));
                        steps.push(Site { line, to: r.clone(), hands: hands_of(&[c.child_by_field_name("arguments")], &kept), gives: gives.clone(), arms: arms.clone(), span: (c.start_byte() as u32, c.end_byte() as u32), ..Default::default() });
                        kept.extend(gives.into_iter().map(|g| g.name));
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
                    // a component is handed its attributes
                    let attrs: Vec<Option<TsNode>> = walk(c, &["jsx_attribute"]).into_iter().map(Some).collect();
                    steps.push(Site { line, to: r.clone(), hands: hands_of(&attrs, &kept), arms: arms.clone(), span: (c.start_byte() as u32, c.end_byte() as u32), ..Default::default() });
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
                        steps.push(Site { line, to: r.clone(), arms: arms.clone(), ..Default::default() });
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
                let object = on.as_ref().map(|(o, _)| *o);
                let class = on.as_ref().and_then(|(o, _)| receiver(*o));
                let target = match on.and_then(|(o, name)| receiver(o).and_then(|k| method_of(store, &k, &name, 0))) {
                    Some(t) => Some(t),
                    None if parts.is_empty() => continue,
                    None => resolve(m, &parts, modules, &store.nodes),
                };
                let hands = hands_of(&[object, c.child_by_field_name("arguments")], &kept);
                match &target {
                    Some(t) if own(t, store) => {
                        // a method found on a base runs on the subclass the value is
                        let sub = class.filter(|k| store.nodes[t].cls.as_deref().map_or(false, |b| b != k.as_str())).unwrap_or_default();
                        let this = object.map_or(false, |o| o.kind() == "this");
                        let gives = gives_of(c, Some(t));
                        steps.push(Site { line, to: t.clone(), on: sub, on_receiver: this, hands, gives: gives.clone(), arms: arms.clone(), span: (c.start_byte() as u32, c.end_byte() as u32), ..Default::default() });
                        kept.extend(gives.into_iter().map(|g| g.name));
                    }
                    // a library's operation not read is a step where the body keeps what it gives
                    _ => {
                        let gives = gives_of(c, None);
                        if !hands.is_empty() && !gives.is_empty() {
                            let op = parts.last().cloned().unwrap_or_default();
                            steps.push(Site { line, op, hands, gives: gives.clone(), arms: arms.clone(), span: (c.start_byte() as u32, c.end_byte() as u32), ..Default::default() });
                            kept.extend(gives.into_iter().map(|g| g.name));
                        }
                    }
                }
                match target {
                    Some(t) if own(&t, store) => {
                        let kind = if store.nodes[&t].kind == NodeKind::Class { EdgeKind::Instantiates } else { EdgeKind::Calls };
                        let callee = if kind == EdgeKind::Instantiates { format!("{t}.constructor") } else { t.clone() };
                        if let Some(a) = argument(modules, store, id, &callee, c, m, scope, &params) {
                            linked.args.push(a);
                        }
                        new_edges.push((t, kind, arms));
                    }
                    _ => {
                        let key = parts.iter().take(2).cloned().collect::<Vec<_>>().join(".");
                        *store.unresolved.entry(key).or_insert(0) += 1;
                        let pkg = match m.bindings.get(&parts[0]) {
                            Some(Bound::Ext(e)) => Some(package_from_ext(e, &m.ext_pkgs)),
                            _ => made.get(&parts[0]).cloned(),
                        };
                        // a language model is asked through a client the package made, not its helpers
                        let kind = effects::boundary_ts(pkg.as_deref(), &parts).filter(|k| k.world != World::Llm || made.contains_key(&parts[0]));
                        if let Some(kind) = kind {
                            let op = parts.last().cloned().unwrap_or_default();
                            let kinds = &["string", "template_string"];
                            let f = effects::facts(kind, pkg.as_deref(), &op, &literals(c, src, kinds));
                            let what = what_of(c, src, kinds, "arguments");
                            let payload = payload_of(c, src, &what, "arguments", &HashMap::new());
                            let fixed = f.kind.located() && starts_at_literal(c, src, kinds, "arguments", effects::on_receiver(&op));
                            let args = c.child_by_field_name("arguments").map(named_children).unwrap_or_default();
                            // the path a file call is handed, the URL a request is: placed once every call is known
                            if f.kind.located() && !args.is_empty() {
                                let at = At { m, scope, params: &params };
                                for (kind, a) in sides(&op, &args, &f) {
                                    let t = Touch { what: text(a, src).to_string(), payload: payload.clone(), op: op.clone(), artifact: f.artifact.clone(), fixed, line: c.start_position().row as u32 + 1, ..Touch::new(kind.world, kind.dir) };
                                    let built = eval.of(at, a, 0);
                                    // a request is its method and its URL: `fetch(url, { method })`, `axios.post(url)`
                                    let built = if kind.world == World::Net { crate::paths::cross(&method_of_request(&op, &args, at, eval), &built, " ") } else { built };
                                    linked.calls.push((id.to_string(), t, built));
                                }
                                continue;
                            }
                            for (artifact, dir) in f.places() {
                                // a URL without a host is the page's own origin, decided by whoever serves it
                                let fixed = fixed && (f.kind != World::Net || !artifact.is_empty());
                                touches.push(Touch { what: what.clone(), payload: payload.clone(), op: op.clone(), artifact, fixed, line: c.start_position().row as u32 + 1, ..Touch::new(f.kind, dir) });
                            }
                        }
                    }
                }
            }
        }
    }
    // a constructor of the platform's own that connects opens a connection: `new WebSocket(url)`
    let opened = sites.iter().filter(|c| c.kind() == "new_expression").filter(|c| {
        let parts = c.child_by_field_name("constructor").map(|x| member_parts(x, src)).unwrap_or_default();
        resolve(m, &parts, modules, &store.nodes).filter(|r| own(r, store)).is_none()
    });
    touches.extend(opened.filter_map(|c| connects(*c, src, &m.bindings)));
    // a component renders what it returns to the screen: a page's route ends where the human sees it
    if !is_module {
        if let Some(jsx) = walk(body, &["jsx_element", "jsx_self_closing_element", "jsx_fragment"]).into_iter().next() {
            touches.push(Touch { op: "render".into(), line: jsx.start_position().row as u32 + 1, ..Touch::new(World::Screen, None) });
        }
    }
    for (t, kind, arms) in new_edges {
        store.add_call(id, &t, kind, None, arms);
    }
    if let Some(node) = store.nodes.get_mut(id) {
        node.sites = steps;
    }
    for t in touches {
        store.touch(id, t);
    }
    // what the body changes of its inputs: `T -> T` for the receiver or a parameter it changes;
    // a constructor builds its object, it does not change it
    if !is_module {
        for r in changed_roots(body, src) {
            let ty = if r == "this" { cls.clone().filter(|_| !id.ends_with(".constructor")) } else { param_types.get(r).cloned() };
            if let Some(t) = ty {
                store.add_edge(id, &t, EdgeKind::Mutates, Some(r.to_string()));
            }
        }
    }
    if !is_module {
        let cmp = crate::tree::compares(body, src, &choices, &COMPARES, &CHOICES);
        if let Some(node) = store.nodes.get_mut(id) {
            node.compares = cmp;
        }
    }
}

/// The HTTP method of a request: `fetch`'s `init.method` (GET when it has none), or the verb
/// of the call.
fn method_of_request<'t>(op: &str, args: &[TsNode<'t>], at: At<'_, 't>, eval: &Eval) -> Vec<crate::paths::Built> {
    const HTTP: &[&str] = &["get", "post", "put", "patch", "delete", "head", "options"];
    if HTTP.contains(&op) {
        return vec![crate::paths::lit(&op.to_uppercase())];
    }
    let init = args.get(1).filter(|a| a.kind() == "object").and_then(|o| {
        named_children(*o).into_iter().find_map(|p| match p.kind() {
            "pair" if p.child_by_field_name("key").map(|k| text(k, &at.m.src)) == Some("method") => p.child_by_field_name("value"),
            "shorthand_property_identifier" if text(p, &at.m.src) == "method" => Some(p),
            _ => None,
        })
    });
    match init {
        Some(v) => eval.of(at, v, 0),
        None => vec![crate::paths::lit("GET")],
    }
}

/// Methods that change their receiver: an array's, a map's or a set's.
const MUTATORS: &[&str] = &["push", "pop", "shift", "unshift", "splice", "sort", "reverse", "fill", "copyWithin", "set", "delete", "clear", "add"];

/// The name a chain like `this.items[0].x` starts from.
fn chain_root<'a>(n: TsNode, src: &'a [u8]) -> Option<&'a str> {
    match n.kind() {
        "identifier" | "this" => Some(text(n, src)),
        "member_expression" | "subscript_expression" => chain_root(n.child_by_field_name("object")?, src),
        "non_null_expression" | "parenthesized_expression" => chain_root(named_children(n).into_iter().next()?, src),
        _ => None,
    }
}

/// The names whose objects a body changes in place: the root of every assignment to a member
/// or an item (`this.x = ..`, `p.items[k] += ..`, `p.n++`, `delete p.k`), and of every call to a
/// method that changes its receiver (`p.rows.push(..)`). Changing a part changes the whole.
fn changed_roots<'a>(body: TsNode, src: &'a [u8]) -> std::collections::BTreeSet<&'a str> {
    let member = |n: TsNode| matches!(n.kind(), "member_expression" | "subscript_expression");
    let mut out = std::collections::BTreeSet::new();
    for a in walk(body, &["assignment_expression", "augmented_assignment_expression"]) {
        out.extend(a.child_by_field_name("left").filter(|l| member(*l)).and_then(|l| chain_root(l, src)));
    }
    for u in walk(body, &["update_expression", "unary_expression"]) {
        let deletes = u.kind() == "update_expression" || (0..u.child_count()).filter_map(|i| u.child(i)).any(|c| c.kind() == "delete");
        if deletes {
            out.extend(u.child_by_field_name("argument").filter(|a| member(*a)).and_then(|a| chain_root(a, src)));
        }
    }
    for c in walk(body, &["call_expression"]) {
        let Some(f) = c.child_by_field_name("function").filter(|f| f.kind() == "member_expression") else { continue };
        if f.child_by_field_name("property").map_or(false, |p| MUTATORS.contains(&text(p, src))) {
            out.extend(f.child_by_field_name("object").and_then(|o| chain_root(o, src)));
        }
    }
    out
}

/// A constructor of the platform's own that connects, as the touch it makes: `new WebSocket(url)`.
/// A name the module imports is that import's, not the platform's.
fn connects(c: TsNode, src: &[u8], bindings: &HashMap<String, Bound>) -> Option<Touch> {
    let parts = c.child_by_field_name("constructor").map(|x| member_parts(x, src)).unwrap_or_default();
    let [name] = parts.as_slice() else { return None };
    if bindings.contains_key(name) {
        return None;
    }
    let kind = effects::boundary_ts(None, &parts)?;
    let kinds = &["string", "template_string"];
    let f = effects::facts(kind, None, name, &literals(c, src, kinds));
    Some(Touch { what: what_of(c, src, kinds, "arguments"), op: name.clone(), artifact: f.artifact, line: c.start_position().row as u32 + 1, ..Touch::new(f.kind, f.dir) })
}

/// The paths a file call is handed, each with the way it goes: a copy or a move reads its first and
/// writes its second; any other call goes its own way with its first.
fn sides<'t>(op: &str, args: &[TsNode<'t>], f: &effects::Facts) -> Vec<(Crossing, TsNode<'t>)> {
    match matches!(op, "copyFile" | "copyFileSync" | "rename" | "renameSync" | "cp" | "cpSync") && args.len() > 1 {
        true => vec![(Crossing::read(World::Disk), args[0]), (Crossing::write(World::Disk), args[1])],
        false => vec![(Crossing { world: f.kind, dir: if f.kind == World::Disk { f.dir } else { None } }, args[0])],
    }
}

/// Which parameter of the function a call reaches each argument is handed to, in order.
#[allow(clippy::too_many_arguments)]
fn argument<'t>(modules: &'t IndexMap<String, Module>, store: &GraphBuilder, caller: &str, callee: &str, c: TsNode<'t>, m: &'t Module, scope: Option<TsNode<'t>>, params: &[String]) -> Option<Arg<'t>> {
    let cn = store.nodes.get(callee)?;
    let cm = modules.get(&cn.module)?;
    let (s, e) = *cm.bodies.get(callee)?;
    let def = paths::function_node(cm.tree.root_node().descendant_for_byte_range(s, e)?)?;
    let names = paths::param_names(def, &cm.src);
    let passed = named_children(c.child_by_field_name("arguments")?)
        .into_iter()
        .take_while(|a| a.kind() != "spread_element")
        .zip(names)
        .filter(|(_, n)| !n.is_empty())
        .map(|(a, n)| (n, a))
        .collect();
    Some(Arg { caller: caller.to_string(), callee: callee.to_string(), module: m.id.clone(), scope, params: params.to_vec(), passed })
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

/// TypeScript's comparisons: `===`, `==`, `!==` and `!=` against a string, and a `switch` over strings.
const COMPARES: CompareSyntax = CompareSyntax {
    kinds: &["binary_expression", "switch_statement"],
    operands: tested,
    cases: |n, src| (n.kind() == "string").then(|| vec![unquote(text(n, src))]),
    subject: |n| matches!(n.kind(), "identifier" | "member_expression"),
    over: switched,
};

/// The two operands of a test of equality.
fn tested<'t>(n: TsNode<'t>, src: &[u8]) -> Option<(TsNode<'t>, TsNode<'t>)> {
    let op = n.child_by_field_name("operator").map(|o| text(o, src)).unwrap_or("");
    matches!(op, "===" | "==" | "!==" | "!=").then_some((n.child_by_field_name("left")?, n.child_by_field_name("right")?))
}

/// A `switch`: its value, the strings of its cases, and the body that holds them.
fn switched<'t>(n: TsNode<'t>, src: &[u8]) -> Option<(String, Vec<String>, Option<TsNode<'t>>)> {
    let (Some(value), Some(body)) = (n.child_by_field_name("value"), n.child_by_field_name("body")) else { return None };
    let cases = named_children(body).into_iter().filter_map(|c| c.child_by_field_name("value")).filter(|v| v.kind() == "string").map(|v| unquote(text(v, src))).collect();
    Some((text(value, src).trim_start_matches('(').trim_end_matches(')').to_string(), cases, Some(body)))
}
