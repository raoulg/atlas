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

mod libraries;
mod paths;

use paths::{Arg, At, Eval};

use crate::body::{changed_through, BindSyntax, Crosses, Step, Steps, UseSyntax};
use crate::builder::{foreign_type, ClassInfo, Declared, GraphBuilder, SigInfo};
use crate::effects::{self, Crossing};
use crate::knowledge;
use crate::knowledge::OwnTypes;
use crate::model::{Bucket, Dir, EdgeKind, Entry, EntryKind, Held, Node, NodeKind, Param, Returns, Touch, World};
use crate::repo::{normal, Repo};
use crate::tree::{
    beneath, evaluation_order, literals, named_children, nth_of_kind, outermost, span, text, unquote, walk, what_of, wrapped, ChoiceSyntax,
    CompareSyntax, Part, Wrappers,
};

pub(crate) const EXTENSIONS: &[&str] = &["ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs"];
/// What hands on the value inside it: parentheses, `await`, the non-null assertion `!`, and the
/// casts `as` and `satisfies`.
const WRAPPERS: Wrappers =
    &["parenthesized_expression", "await_expression", "non_null_expression", "as_expression", "satisfies_expression"];
const OWN: OwnTypes = knowledge::TYPESCRIPT_OWN_TYPES;

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
    /// The names imported from a file not the package's own (a package's, or one of a library's
    /// files): each by its local name, the specifier and its name there (`default` for a default
    /// import, none for the module itself).
    imported: Vec<Imported>,
    /// The names bound to a library's code Atlas reads, by local name: the file each comes from
    /// and its name there.
    links: HashMap<String, (PathBuf, Option<String>)>,
    /// What those names stand for where the library's code is read as code: the definition, by
    /// the name as written (`readConfig`, or a namespace's member `fk.readConfig`).
    read: HashMap<String, String>,
    /// The values a package made that every body of the module sees, by name: the module's own,
    /// and each class's fields by where the class starts. Found once, when a body first asks.
    made: std::cell::RefCell<HashMap<Option<usize>, HashMap<String, String>>>,
    /// The fields of each of the module's classes and shapes that hold one of the language's own
    /// containers, by the class: `rows: Row[]`, `seen = new Set<string>()`.
    containers: HashMap<String, HashSet<String>>,
}

/// A name imported from a file not the package's own, as written.
#[derive(Clone, Debug)]
struct Imported {
    local: String,
    spec: String,
    name: Option<String>,
}

impl Module {
    /// A module of a file read from `src`, before its imports and symbols are collected.
    fn new(id: String, path: PathBuf, src: Vec<u8>, tree: Tree) -> Module {
        Module {
            id,
            path,
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
            imported: vec![],
            links: HashMap::new(),
            read: HashMap::new(),
            made: Default::default(),
            containers: HashMap::new(),
        }
    }
}

/// Does this look like a JavaScript or TypeScript package: a `package.json` with a
/// `tsconfig.json`, a `jsconfig.json` or a `src/` beside it.
pub fn detect(repo: &Repo) -> bool {
    repo.is_file("package.json") && (repo.is_file("tsconfig.json") || repo.is_file("jsconfig.json") || repo.is_dir("src"))
}

fn ts_parser(tsx: bool) -> Result<Parser> {
    let mut p = Parser::new();
    let lang: tree_sitter::Language =
        if tsx { tree_sitter_typescript::LANGUAGE_TSX.into() } else { tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into() };
    p.set_language(&lang).context("loading the TypeScript grammar")?;
    Ok(p)
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

/// A path segment as part of a dotted id: dots inside a file name (`punt.functions`) are not
/// module boundaries.
fn segment(s: &str) -> String {
    s.replace('.', "_")
}

/// Where module paths start: `src/` when there is one.
pub(crate) fn code_base(repo: &Repo) -> PathBuf {
    if repo.is_dir("src") {
        repo.root().join("src")
    } else {
        repo.root().to_path_buf()
    }
}

fn is_code(p: &Path) -> bool {
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
    EXTENSIONS.contains(&ext)
        && !name.ends_with(".d.ts")
        && !name.contains(".gen.")
        && !name.contains(".min.")
        && !name.contains(".config.")
}

/// `src/lib/map/layers.ts` -> `app.lib.map.layers`; `src/lib/index.ts` -> `app.lib`.
fn module_id(root: &str, base: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(base).unwrap_or(path);
    let mut parts: Vec<String> =
        rel.parent().map(|d| d.components().map(|c| segment(&c.as_os_str().to_string_lossy())).collect()).unwrap_or_default();
    let stem = rel.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let stem = stem.rsplit_once('.').map(|(s, _)| s).unwrap_or(stem);
    if stem != "index" {
        parts.push(segment(stem));
    }
    std::iter::once(root.to_string()).chain(parts).collect::<Vec<_>>().join(".")
}

/// tsconfig `paths`: alias prefix -> directory, `"@/*": ["./src/*"]` -> (`@/`, `<repo>/src/`).
fn path_aliases(repo: &Repo) -> Vec<(String, PathBuf)> {
    let mut out = vec![];
    for cfg in ["tsconfig.json", "jsconfig.json"] {
        let Some(text) = repo.text(cfg) else { continue };
        let Ok(json) = serde_json::from_str::<serde_json::Value>(&jsonc(&text)) else { continue };
        let opts = &json["compilerOptions"];
        let base = repo.root().join(opts["baseUrl"].as_str().unwrap_or("."));
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
            ('/', Some('/')) => while chars.next_if(|&n| n != '\n').is_some() {},
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
    let base = normal(&base);
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

/// The comment right before a declaration (before its `export` when it has one).
fn leading_comment<'t>(n: TsNode<'t>, src: &'t [u8]) -> Option<&'t str> {
    let mut target = n;
    while let Some(p) = target.parent().filter(|p| matches!(p.kind(), "export_statement" | "ambient_declaration")) {
        target = p;
    }
    let prev = target.prev_named_sibling().filter(|p| p.kind() == "comment")?;
    (prev.end_position().row + 1 >= target.start_position().row).then(|| text(prev, src))
}

/// A leading JSDoc or line comment, first line.
fn doc_of(n: TsNode, src: &[u8]) -> String {
    let Some(comment) = leading_comment(n, src) else { return String::new() };
    comment
        .lines()
        .map(|l| l.trim().trim_start_matches("/**").trim_start_matches("/*").trim_start_matches("//").trim_start_matches('*').trim())
        .find(|l| !l.is_empty() && !l.starts_with("@prophecy"))
        .unwrap_or("")
        .trim_end_matches("*/")
        .trim()
        .to_string()
}

/// What the leading comment's `@prophecy` tag says, if it has one.
fn prophecy_of(n: TsNode, src: &[u8]) -> Option<crate::model::Prophecy> {
    leading_comment(n, src).and_then(crate::model::Prophecy::of_jsdoc)
}

// ---------- types ----------

/// The type names an annotation mentions as values: `Promise<Regel[]>` -> Regel,
/// `Treffer | Mis` -> both, `Laag<Bron>` -> Laag (a generic class's arguments are types).
fn type_names(t: Option<TsNode>, src: &[u8]) -> Vec<Vec<String>> {
    let Some(t) = t else { return vec![] };
    let mut out = vec![];
    collect_types(t, src, &[], &mut out);
    out
}

/// An annotation's names apart from the unions it writes in place, and those unions, outermost
/// first: `(r: Fit | Failed)` takes one union, which may be a sum, not a `Fit` and a `Failed`.
fn split_unions(t: Option<TsNode>, src: &[u8]) -> (Vec<Vec<String>>, Vec<crate::builder::Union>) {
    let Some(t) = t else { return (vec![], vec![]) };
    let all = walk(t, &["union_type"]);
    let written: Vec<TsNode> = all
        .iter()
        .copied()
        .filter(|u| !all.iter().any(|o| o.id() != u.id() && o.start_byte() <= u.start_byte() && u.end_byte() <= o.end_byte()))
        .collect();
    let mut names = vec![];
    collect_types(t, src, &written, &mut names);
    let unions =
        written.iter().map(|u| crate::builder::Union { names: type_names(Some(*u), src), written: text(*u, src).to_string() }).collect();
    (names, unions)
}

/// The type names below `n`, apart from those inside the nodes `skip`.
fn collect_types(n: TsNode, src: &[u8], skip: &[TsNode], out: &mut Vec<Vec<String>>) {
    if skip.iter().any(|s| s.id() == n.id()) {
        return;
    }
    match n.kind() {
        "type_identifier" => out.push(vec![text(n, src).to_string()]),
        "nested_type_identifier" => out.push(text(n, src).split('.').map(str::to_string).collect()),
        "generic_type" => {
            let Some(name) = n.child_by_field_name("name") else { return };
            let parts: Vec<String> = text(name, src).split('.').map(str::to_string).collect();
            if knowledge::TYPESCRIPT_CONTAINERS.contains(&parts.last().map(String::as_str).unwrap_or("")) {
                if let Some(args) = n.child_by_field_name("type_arguments") {
                    collect_types(args, src, skip, out);
                }
            } else {
                out.push(parts);
            }
        }
        // a function type is a callback, an object type an anonymous shape: neither names a place
        "function_type" | "object_type" | "predefined_type" | "literal_type" | "template_literal_type" | "type_query" => {}
        _ => {
            for c in named_children(n) {
                collect_types(c, src, skip, out);
            }
        }
    }
}

/// Whether a type is the unchecked one: `any`, which says no more than leaving the type out.
/// `unknown` is checked: it is the type of what is still to be checked.
fn unchecked(t: TsNode, src: &[u8]) -> bool {
    t.kind() == "predefined_type" && text(t, src) == "any"
}

/// A signature as data: each parameter by name and type (`(id: string, opt?: Opties)`), the types
/// they name, and what it gives back. `...rest` takes whatever arguments are left; a `this`
/// parameter is the value the function is called on; a parameter typed `any` is unchecked.
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
                let written = ty.and_then(|t| named_children(t).into_iter().next());
                out.push(Param {
                    name: name.trim_start_matches("...").to_string(),
                    ty: written.map(|t| text(t, src).to_string()).unwrap_or_default(),
                    receiver: pattern.map_or(false, |x| x.kind() == "this"),
                    rest: pattern.map_or(false, |x| x.kind() == "rest_pattern"),
                    changes: false,
                    unchecked: written.map_or(false, |t| unchecked(t, src)),
                });
                let (names, unions) = split_unions(ty, src);
                info.ptypes.extend(names);
                info.punions.extend(unions);
            }
        }
    }
    let returns = ret.and_then(|r| named_children(r).into_iter().next()).map(|r| returns(r, src));
    (info.rtypes, info.runions) = split_unions(ret, src);
    (out, returns, info)
}

/// What a return type gives: a `Promise<T>` gives `T` once it arrives, `void` and `undefined` no
/// value, `this` a value of the class it runs on; `any`, also once it arrives, is unchecked. A
/// `Promise` says that the value arrives later, not that it may fail: it rejects where what it
/// runs throws, as any call may.
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
    Returns { written: text(rt, src).to_string(), value, own, fails: false, unchecked: unchecked(t, src) }
}

// ---------- pass 1: files, symbols, imports ----------

/// What a reading of the package at `repo` takes from outside it: the packages installed in a
/// `node_modules` above it, which Node finds from its files as well.
pub fn environment(repo: &Repo) -> Vec<crate::cache::Check> {
    let root = std::fs::canonicalize(repo.root()).unwrap_or_else(|_| repo.root().to_path_buf());
    root.ancestors().skip(1).map(|a| crate::cache::Check::Tree(a.join(knowledge::NODE_PACKAGES))).collect()
}

pub fn parse(repo: &Repo, opts: &crate::Options) -> Result<(GraphBuilder, String)> {
    let root = root_name(repo);
    let base = code_base(repo);
    let aliases = path_aliases(repo);
    let mut store = GraphBuilder::default();
    // not under what a tool made
    let made = |p: &Path| {
        p.strip_prefix(&base).map_or(false, |r| r.components().any(|c| knowledge::made_dir(c.as_os_str().to_str().unwrap_or(""))))
    };
    let mut paths: Vec<PathBuf> = repo.files_under(&base).into_iter().filter(|p| is_code(p) && !made(p)).map(|p| normal(&p)).collect();
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
        store.add_node(Node {
            id: d.clone(),
            kind: NodeKind::Module,
            name: d.rsplit('.').next().unwrap_or(d).to_string(),
            module: d.clone(),
            is_pkg: true,
            ..Default::default()
        });
    }
    let mut modules: IndexMap<String, Module> = IndexMap::new();
    let (mut ts, mut tsx) = (ts_parser(false)?, ts_parser(true)?);
    for p in &paths {
        let id = files[p].clone();
        let src = repo.bytes(p).with_context(|| format!("reading {}", p.display()))?.to_vec();
        let jsx = matches!(p.extension().and_then(|e| e.to_str()), Some("tsx" | "jsx" | "js" | "mjs" | "cjs"));
        let tree = (if jsx { &mut tsx } else { &mut ts }).parse(&src, None).with_context(|| format!("parsing {}", p.display()))?;
        store.files.insert(crate::tree::path_str(p), String::from_utf8_lossy(&src).to_string());
        let is_index = p.file_stem().and_then(|s| s.to_str()) == Some("index");
        let rel = crate::tree::path_str(p.strip_prefix(repo.root()).unwrap_or(p));
        // a test file is test code, and a route of its own: its body runs the tests
        let test = test_file(p);
        let entry = test.then(|| Entry::new(EntryKind::Test, rel.clone()));
        let node = Node {
            id: id.clone(),
            kind: NodeKind::Module,
            name: p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string(),
            file: crate::tree::path_str(p),
            span: span(tree.root_node()),
            module: id.clone(),
            is_pkg: is_index,
            test,
            entry,
            ..Default::default()
        };
        store.add_node(node);
        modules.insert(id.clone(), Module::new(id, p.clone(), src, tree));
    }
    for m in modules.values_mut() {
        collect_imports(m, &aliases, &files);
        collect_wrapped(m);
        collect_symbols(m, &mut store);
        collect_routes(m, &mut store);
    }
    libraries::read(repo, &mut modules, &mut store, opts.libraries);
    for m in modules.values_mut() {
        m.containers = container_fields(m);
    }
    route_prefixes(&modules, &mut store);
    mark_bins(repo, &files, &mut store);
    mark_exports(repo, &files, &modules, &mut store);
    link(&modules, &mut store, repo.root());
    store.mark_library();
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
                out.push((unambient(d), true, default));
            }
        } else {
            out.push((unambient(st), false, false));
        }
    }
    out
}

/// What `declare` declares: a sketch or a typing without a body (`declare function f(x: A): B;`)
/// is the declaration itself.
fn unambient(d: TsNode) -> TsNode {
    if d.kind() == "ambient_declaration" {
        named_children(d).into_iter().find(|c| c.kind() != "comment").unwrap_or(d)
    } else {
        d
    }
}

/// The variables a module declares at its top level, unwrapping `export`: `const x = ..`, `let y`.
fn top_variables<'t>(root: TsNode<'t>) -> Vec<TsNode<'t>> {
    declarations(root)
        .into_iter()
        .filter(|(d, _, _)| matches!(d.kind(), "lexical_declaration" | "variable_declaration"))
        .flat_map(|(d, _, _)| named_children(d))
        .filter(|v| v.kind() == "variable_declarator")
        .collect()
}

/// The function a top-level `const` stands for: an arrow or function expression, or the last
/// one handed to a call chain (`createServerFn(..).handler(async ({ data }) => ..)`). An object
/// literal is a value, not a function.
fn function_in<'t>(value: TsNode<'t>) -> Option<TsNode<'t>> {
    match value.kind() {
        "arrow_function" | "function_expression" | "function" => Some(value),
        k if matches!(k, "call_expression" | "member_expression") || WRAPPERS.contains(&k) => {
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

/// The path of the file route a declaration makes: `createFileRoute("/map")({..})`.
fn route_path(value: TsNode, src: &[u8]) -> Option<String> {
    walk(value, &["call_expression"]).into_iter().find_map(|c| {
        let f = c.child_by_field_name("function")?;
        let maker = text(f, src);
        if knowledge::ROOT_ROUTES.contains(&maker) {
            return Some("__root".into());
        }
        knowledge::FILE_ROUTES.contains(&maker).then_some(())?;
        let lit = c.child_by_field_name("arguments").and_then(|a| named_children(a).into_iter().find(|x| x.kind() == "string"))?;
        Some(unquote(text(lit, src)))
    })
}

fn add_function(m: &mut Module, store: &mut GraphBuilder, name: &str, def: TsNode, f: TsNode, body: TsNode, cls: Option<&str>) -> String {
    add_declared(m, store, name, def, f, body, cls, Declared::Body)
}

/// A function, from one declaration of its name: what `GraphBuilder::declare` keeps of it, and,
/// when it is the declaration kept, the body it runs.
#[allow(clippy::too_many_arguments)]
fn add_declared(
    m: &mut Module,
    store: &mut GraphBuilder,
    name: &str,
    def: TsNode,
    f: TsNode,
    body: TsNode,
    cls: Option<&str>,
    declared: Declared,
) -> String {
    let src = &m.src;
    let id = match cls {
        Some(c) => format!("{c}.{name}"),
        None => format!("{}.{name}", m.id),
    };
    let params = f.child_by_field_name("parameters").or_else(|| f.child_by_field_name("parameter"));
    let (params, returns, sig) = signature(params, f.child_by_field_name("return_type"), src);
    let on_class = cls.is_some() && (0..def.child_count()).filter_map(|i| def.child(i)).any(|c| c.kind() == "static");
    // the constructor initializes what `new` makes; a static method that gives its class constructs
    let class_name = cls.map(|c| c.rsplit('.').next().unwrap_or(c));
    let gives_class = returns.as_ref().map_or(false, |r| r.own || Some(r.value.as_str()) == class_name);
    let kept = declared == Declared::Body || !store.nodes.contains_key(&id);
    store.declare(
        Node {
            id: id.clone(),
            kind: if cls.is_some() { NodeKind::Method } else { NodeKind::Function },
            on_class,
            ctor: cls.is_some() && (name == "constructor" || on_class && gives_class),
            // a member is private when it says so; a module's own name is, unless the module exports it
            private: cls.is_some()
                && (name.starts_with('#')
                    || named_children(def).iter().any(|c| c.kind() == "accessibility_modifier" && text(*c, src) != "public")),
            name: name.to_string(),
            file: crate::tree::path_str(&m.path),
            span: span(def),
            module: m.id.clone(),
            doc: doc_of(def, src),
            prophecy: prophecy_of(def, src),
            cls: cls.map(str::to_string),
            // a method an interface or an abstract class declares is its signature alone
            declared: matches!(f.kind(), "method_signature" | "abstract_method_signature"),
            params,
            returns,
            ..Default::default()
        },
        sig,
        declared,
    );
    if kept {
        m.bodies.insert(id.clone(), (body.start_byte(), body.end_byte()));
    }
    id
}

/// What a declaration of a function is among the declarations of its name in one scope: a
/// signature without a body, where the name is declared again, is an overload; a `set` accessor
/// writes the property its `get` reads.
fn declared_as(d: TsNode, overloaded: &HashSet<String>, src: &[u8]) -> Declared {
    let name = d.child_by_field_name("name").map_or("", |n| text(n, src));
    let signature = matches!(d.kind(), "function_signature" | "method_signature");
    if signature && overloaded.contains(name) {
        Declared::Overload
    } else if (0..d.child_count()).filter_map(|i| d.child(i)).any(|c| c.kind() == "set") {
        Declared::Writer
    } else {
        Declared::Body
    }
}

/// The names declared more than once among `decls`.
fn declared_again<'t>(decls: impl Iterator<Item = TsNode<'t>>, src: &[u8]) -> HashSet<String> {
    let mut seen = HashSet::new();
    decls.filter_map(|d| d.child_by_field_name("name")).map(|n| text(n, src).to_string()).filter(|n| !seen.insert(n.clone())).collect()
}

fn collect_symbols(m: &mut Module, store: &mut GraphBuilder) {
    let tree = m.tree.clone();
    let src = m.src.clone();
    let mut route: Option<String> = None;
    let functions = |d: &TsNode| matches!(d.kind(), "function_declaration" | "function_signature");
    let overloaded = declared_again(declarations(tree.root_node()).into_iter().map(|(d, _, _)| d).filter(functions), &src);
    for (d, exported, default) in declarations(tree.root_node()) {
        let name = d.child_by_field_name("name").map(|n| text(n, &src).to_string()).unwrap_or_default();
        if exported && !name.is_empty() {
            m.exported.push(name.clone());
        }
        match d.kind() {
            "function_declaration" | "function_signature" | "generator_function_declaration" | "function_expression" | "function"
                if !name.is_empty() =>
            {
                let body = d.child_by_field_name("body").unwrap_or(d);
                let id = add_declared(m, store, &name, d, d, body, None, declared_as(d, &overloaded, &src));
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
                            let parts: Vec<String> =
                                text(v, &src).split('<').next().unwrap_or("").split('.').map(|s| s.trim().to_string()).collect();
                            if !parts.iter().any(String::is_empty) {
                                bases.push(parts);
                            }
                        }
                    }
                }
                let mut methods: Vec<String> = vec![];
                if let Some(body) = d.child_by_field_name("body") {
                    let members = |x: &TsNode| matches!(x.kind(), "method_definition" | "abstract_method_signature" | "method_signature");
                    let overloaded = declared_again(named_children(body).into_iter().filter(members), &src);
                    for member in named_children(body) {
                        match member.kind() {
                            "method_definition" | "abstract_method_signature" | "method_signature" => {
                                let mname = member.child_by_field_name("name").map(|n| text(n, &src).to_string()).unwrap_or_default();
                                let mbody = member.child_by_field_name("body").unwrap_or(member);
                                let declared = declared_as(member, &overloaded, &src);
                                let mid = add_declared(m, store, &mname, member, member, mbody, Some(&id), declared);
                                store.add_edge(&id, &mid, EdgeKind::Contains, None);
                                let own = mid.rsplit('.').next().unwrap_or("").to_string();
                                if !methods.contains(&own) {
                                    methods.push(own);
                                }
                                // `constructor(private bron: Bron)` declares a field
                                if mname == "constructor" {
                                    for p in member.child_by_field_name("parameters").map(named_children).unwrap_or_default() {
                                        if is_property(p) {
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
                        let declared =
                            v.child_by_field_name("type").and_then(|t| named_children(t).into_iter().next()).map(|t| type_head(t, &src));
                        let built = (value.kind() == "new_expression")
                            .then(|| value.child_by_field_name("constructor").map(|c| member_parts(c, &src)))
                            .flatten();
                        if let Some(t) = declared.or(built).filter(|t| !t.is_empty()) {
                            m.values.insert(vname, t);
                        }
                        continue;
                    };
                    let id = add_function(m, store, &vname, d, f, value, None);
                    // an arrow handed to a call takes its parameters' types from the call
                    if f.id() != value.id() {
                        if let (Some(n), Some(call)) =
                            (store.nodes.get_mut(&id), f.parent().and_then(|a| a.parent()).filter(|c| c.kind() == "call_expression"))
                        {
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
            n.entry = Some(Entry::new(EntryKind::Route, if path == "__root" { "/ (root)".into() } else { path }));
        }
    }
    for n in
        store.nodes.values_mut().filter(|n| n.module == m.id && n.cls.is_none() && matches!(n.kind, NodeKind::Function | NodeKind::Class))
    {
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
            "pair" => match (
                member.child_by_field_name("key"),
                member.child_by_field_name("value").filter(|v| matches!(v.kind(), "arrow_function" | "function_expression" | "function")),
            ) {
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
    let mut out: Vec<String> = vec![];
    let sigs: Vec<TsNode> = named_children(body).into_iter().filter(|x| x.kind() == "method_signature").collect();
    let overloaded = declared_again(sigs.iter().copied(), &m.src);
    for sig in sigs {
        let name = sig.child_by_field_name("name").map(|n| text(n, &m.src).to_string()).unwrap_or_default();
        let declared = declared_as(sig, &overloaded, &m.src);
        let mid = add_declared(m, store, &name, sig, sig, sig, Some(&id), declared);
        store.add_edge(&id, &mid, EdgeKind::Contains, None);
        if !out.contains(&name) {
            out.push(name);
        }
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
/// Whether a constructor's parameter declares a field of its class as well: one written with an
/// access modifier, `readonly` or `override` (`constructor(readonly nodes: Node[])`).
pub(crate) fn is_property(p: TsNode) -> bool {
    (0..p.child_count()).filter_map(|i| p.child(i)).any(|c| matches!(c.kind(), "accessibility_modifier" | "readonly" | "override_modifier"))
}

fn fields_of(body: TsNode, src: &[u8]) -> Vec<(String, Vec<Vec<String>>)> {
    named_children(body)
        .into_iter()
        .filter(|p| p.kind() == "property_signature")
        .filter_map(|p| Some((text(p.child_by_field_name("name")?, src).to_string(), type_names(p.child_by_field_name("type"), src))))
        .collect()
}

fn add_type(
    m: &mut Module,
    store: &mut GraphBuilder,
    name: &str,
    d: TsNode,
    bases: Vec<Vec<String>>,
    fields: Vec<(String, Vec<Vec<String>>)>,
    methods: Vec<String>,
) {
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
        prophecy: prophecy_of(d, &m.src),
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
            if own.is_none() {
                m.imported.push(Imported { local: name.clone(), spec: spec.clone(), name: Some(name.clone()) });
            }
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
        let imported = |into: &mut Vec<Imported>, local: &str, name: Option<&str>| {
            if own.is_none() {
                into.push(Imported { local: local.to_string(), spec: spec.clone(), name: name.map(str::to_string) });
            }
        };
        match st.kind() {
            "import_statement" => {
                for clause in named_children(st).into_iter().filter(|c| c.kind() == "import_clause") {
                    for c in named_children(clause) {
                        match c.kind() {
                            "identifier" => {
                                let local = text(c, &src).to_string();
                                imported(&mut m.imported, &local, Some("default"));
                                m.bindings.insert(local, if own.is_some() { member(Some("default")) } else { member(None) });
                            }
                            "namespace_import" => {
                                if let Some(id) = named_children(c).into_iter().find(|x| x.kind() == "identifier") {
                                    imported(&mut m.imported, text(id, &src), None);
                                    m.bindings.insert(text(id, &src).to_string(), member(None));
                                }
                            }
                            "named_imports" => {
                                for s in named_children(c).into_iter().filter(|s| s.kind() == "import_specifier") {
                                    let Some(n) = s.child_by_field_name("name") else { continue };
                                    let local = s.child_by_field_name("alias").unwrap_or(n);
                                    imported(&mut m.imported, text(local, &src), Some(text(n, &src)));
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
                    imported(&mut m.imported, text(local, &src), Some(text(n, &src)));
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
    let p = normal(&repo.join(path));
    files.get(&p).cloned().or_else(|| files.iter().find(|(f, _)| f.with_extension("") == p.with_extension("")).map(|(_, id)| id.clone()))
}

/// `package.json` `bin`, and each script that runs a file of the package (`tsx server/index.ts`):
/// a command, a CLI route into the module it runs. Scripts that run one module name it together.
fn mark_bins(repo: &Repo, files: &HashMap<PathBuf, String>, store: &mut GraphBuilder) {
    let Some(text) = repo.text("package.json") else { return };
    let repo = repo.root();
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else { return };
    let bins: Vec<(String, String)> = match &json["bin"] {
        serde_json::Value::String(p) => vec![(json["name"].as_str().unwrap_or("bin").to_string(), p.clone())],
        serde_json::Value::Object(o) => o.iter().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string()))).collect(),
        _ => vec![],
    };
    for (name, path) in bins {
        if let Some(n) = module_at(repo, &path, files).and_then(|id| store.nodes.get_mut(&id)) {
            n.entry = Some(Entry::new(EntryKind::Cli, name));
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
            n.entry = Some(Entry::new(EntryKind::Cli, names.join(" · ")));
        }
    }
}

/// What a package exports is a route into it: the names its entry module exports, the module
/// `package.json` points to (`exports`, `module`, `main`) or else the index at its root. A
/// name re-exported is followed to where it is defined.
fn mark_exports(repo: &Repo, files: &HashMap<PathBuf, String>, modules: &IndexMap<String, Module>, store: &mut GraphBuilder) {
    let json = repo.text("package.json").and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()).unwrap_or_default();
    let repo = repo.root();
    let dot = |v: &serde_json::Value| -> Option<String> {
        match v {
            serde_json::Value::String(s) => Some(s.clone()),
            serde_json::Value::Object(o) => {
                ["import", "default", "types", "require"].iter().find_map(|k| o.get(*k).and_then(|x| x.as_str()).map(str::to_string))
            }
            _ => None,
        }
    };
    let declared = [
        json["exports"].get(".").and_then(dot),
        dot(&json["exports"]),
        json["module"].as_str().map(str::to_string),
        json["main"].as_str().map(str::to_string),
    ];
    let root = files
        .values()
        .find(|id| modules.get(*id).map_or(false, |m| m.path.file_stem().and_then(|s| s.to_str()) == Some("index") && !id.contains('.')))
        .cloned();
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
        if let Some(id) = defined(&target, modules, &store.nodes, 0) {
            store.export(&id, name);
        }
    }
}

/// Whether a value is made by a web framework's router: `new Hono()`, `express()`,
/// `express.Router()`, and the chain of registrations on it (`new Hono().get(..).post(..)`).
fn router_made(mut v: TsNode, m: &Module, src: &[u8]) -> bool {
    loop {
        v = beneath(v, WRAPPERS);
        match v.kind() {
            "call_expression" => match v.child_by_field_name("function") {
                Some(f)
                    if f.kind() == "member_expression"
                        && f.child_by_field_name("object").map_or(false, |o| matches!(o.kind(), "call_expression" | "new_expression")) =>
                {
                    v = f.child_by_field_name("object").unwrap();
                }
                Some(f) => return router_pkg(&member_parts(f, src), m),
                None => return false,
            },
            "new_expression" => return v.child_by_field_name("constructor").map_or(false, |c| router_pkg(&member_parts(c, src), m)),
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
    knowledge::PROMISIFIERS.contains(&(pkg.as_str(), parts.last()?.as_str())).then_some(())?;
    let arg = named_children(value.child_by_field_name("arguments")?).into_iter().next().filter(|a| a.kind() == "identifier")?;
    m.bindings.get(text(arg, src)).cloned()
}

/// The package a client was made by: `new Anthropic()`, `new Pool()`, `axios.create()`,
/// `createClient()`. What a package's other calls give back is a result, not a client.
fn made_by_package(value: TsNode, m: &Module, src: &[u8]) -> Option<String> {
    let v = beneath(value, WRAPPERS);
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

/// The package a type annotation names: `Database.Database`, `Pool`, `: Client`.
/// Whether a type as written is one of the language's own containers that change in place
/// (`Row[]`, `Array<Row>`, `Map<string, Row>`) where the module names nothing of its own so; a
/// read-only one is not.
fn container_type(t: &str, m: &Module) -> bool {
    let t = t.trim().trim_start_matches(':').trim();
    if t.starts_with("readonly ") || t.starts_with("Readonly") {
        return false;
    }
    let head = t.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$')).next().unwrap_or("");
    t.ends_with("[]")
        || (knowledge::TYPESCRIPT_CHANGEABLE.contains(&head) && !m.bindings.contains_key(head) && !m.symbols.contains_key(head))
}

/// Whether a value as written builds one: `[]`, `new Map()`, `new Set<string>()`.
fn builds_container(v: TsNode, m: &Module, src: &[u8]) -> bool {
    match v.kind() {
        "array" => true,
        "new_expression" => v.child_by_field_name("constructor").map_or(false, |c| container_type(text(c, src), m)),
        _ => false,
    }
}

/// The fields of a module's classes and shapes that hold one of the language's own containers, by
/// the class: declared as one (`rows: Row[]`, a constructor's `readonly seen: Set<string>`) or set
/// to a new one (`rows = []`).
fn container_fields(m: &Module) -> HashMap<String, HashSet<String>> {
    let src = &m.src[..];
    let holds = |ty: Option<TsNode>, value: Option<TsNode>| match ty {
        Some(t) => container_type(text(t, src), m),
        None => value.map_or(false, |v| builds_container(v, m, src)),
    };
    let mut out: HashMap<String, HashSet<String>> = HashMap::new();
    for (d, _, _) in declarations(m.tree.root_node()) {
        let Some(name) = d.child_by_field_name("name").map(|n| text(n, src)) else { continue };
        let body = match d.kind() {
            "class_declaration" | "abstract_class_declaration" | "class" | "interface_declaration" => d.child_by_field_name("body"),
            "type_alias_declaration" => d.child_by_field_name("value").filter(|v| v.kind() == "object_type"),
            _ => None,
        };
        let Some(body) = body else { continue };
        let mut fields = HashSet::new();
        for member in named_children(body) {
            match member.kind() {
                "public_field_definition" | "property_signature" => {
                    if holds(member.child_by_field_name("type"), member.child_by_field_name("value")) {
                        fields.extend(member.child_by_field_name("name").map(|n| text(n, src).to_string()));
                    }
                }
                // `constructor(readonly rows: Row[])` declares a field, and `this.rows = []` in it sets one
                "method_definition" if member.child_by_field_name("name").map(|n| text(n, src)) == Some("constructor") => {
                    for p in member.child_by_field_name("parameters").map(named_children).unwrap_or_default() {
                        if is_property(p) && holds(p.child_by_field_name("type"), None) {
                            fields.extend(p.child_by_field_name("pattern").map(|n| text(n, src).to_string()));
                        }
                    }
                    for a in member.child_by_field_name("body").map(|b| walk(b, &["assignment_expression"])).unwrap_or_default() {
                        let (Some(l), Some(r)) = (a.child_by_field_name("left"), a.child_by_field_name("right")) else { continue };
                        if let ([this, field], true) = (member_parts(l, src).as_slice(), builds_container(r, m, src)) {
                            if this == "this" {
                                fields.insert(field.clone());
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        if !fields.is_empty() {
            out.insert(format!("{}.{name}", m.id), fields);
        }
    }
    out
}

fn typed_package(ty: TsNode, m: &Module, src: &[u8]) -> Option<String> {
    let head = text(ty, src).trim_start_matches(':').trim().split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$')).next()?;
    match m.bindings.get(head)? {
        Bound::Ext(e) => Some(package_from_ext(e, &m.ext_pkgs)),
        Bound::Own(_) => None,
    }
}

fn router_pkg(parts: &[String], m: &Module) -> bool {
    match parts.first().and_then(|h| m.bindings.get(h)) {
        Some(Bound::Ext(e)) => knowledge::ROUTERS.contains(&package_from_ext(e, &m.ext_pkgs).as_str()),
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
                while let Some(p) =
                    up.filter(|p| matches!(p.kind(), "member_expression" | "call_expression") || WRAPPERS.contains(&p.kind()))
                {
                    up = p.parent();
                }
                let declared = up
                    .filter(|p| p.kind() == "variable_declarator")
                    .and_then(|d| d.child_by_field_name("name"))
                    .map(|n| text(n, &src).to_string());
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
        if let Some(mount) = knowledge::MOUNTS.iter().find(|mt| mt.verb == verb) {
            let sub = match mount.child {
                knowledge::Arg::At(i) => args.get(i).filter(|a| a.kind() == "identifier"),
                knowledge::Arg::Named(_) => None,
            };
            if let Some(sub) = sub.filter(|_| matches!(mount.prefix, knowledge::Arg::At(0))) {
                m.mounts.push((router.clone(), path, text(*sub, &src).to_string()));
            }
            continue;
        }
        if !knowledge::registers_route(&verb) {
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
            n.entry = Some(Entry::new(EntryKind::Route, label));
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
    let mut mounted: HashMap<String, (String, String, bool)> = HashMap::new();
    for m in modules.values() {
        for (parent, prefix, sub) in &m.mounts {
            if let (Some(p), Some(s)) = (key_of(m, parent, modules, 0), key_of(m, sub, modules, 0)) {
                mounted.insert(s, (p, prefix.clone(), false));
            }
        }
    }
    let routes: Vec<(String, String)> = modules
        .values()
        .flat_map(|m| m.routes.iter().map(move |(id, router)| (id.clone(), key_of(m, router, modules, 0).unwrap_or_default())))
        .collect();
    crate::entries::mount_routes(store, &routes, &HashMap::new(), &mounted, express_segment);
}

/// A part of a route's path as the graph writes it: Express's `:id` is `{id}`, its `*` the rest.
fn express_segment(s: &str) -> String {
    if let Some(p) = s.strip_prefix(':') {
        format!("{{{}}}", p.trim_end_matches('?'))
    } else if s == "*" {
        "{rest:path}".into()
    } else {
        s.to_string()
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
        // a name of a library's code read as code is its definition there
        Bound::Ext(_) if read_as_code(m, parts).is_some() => read_as_code(m, parts),
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
        Some(Bound::Ext(id)) => return Some(m.read.get(name).cloned().unwrap_or(id)),
        None => {}
    }
    m.wildcards.iter().find_map(|w| defined(&format!("{w}.{name}"), modules, nodes, depth + 1))
}

/// What a name path written in `m` stands for in a library's code read as code: the definition its
/// head (`readConfig`, `Client` of `Client.create`) or a namespace's member (`fk.readConfig`)
/// stands for, with the rest of the path below it.
fn read_as_code(m: &Module, parts: &[String]) -> Option<String> {
    let member = parts.get(1).and_then(|p| m.read.get(&format!("{}.{p}", parts[0])).map(|d| (d, 2)));
    let (def, used) = member.or_else(|| m.read.get(parts.first()?).map(|d| (d, 1)))?;
    Some(std::iter::once(def.as_str()).chain(parts[used..].iter().map(String::as_str)).collect::<Vec<_>>().join("."))
}

/// What the bodies hand the places pass: file and network calls with what their path or URL
/// is built from, and the calls of the package's functions with the arguments they pass.
#[derive(Default)]
struct Linked<'t> {
    calls: Vec<(String, Touch, Vec<crate::paths::Built>)>,
    args: Vec<Arg<'t>>,
}

/// Types, signatures and bodies into edges: the program's, and the functions of the library's code
/// read.
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
            for (types, unions, kind) in [(&sig.ptypes, &sig.punions, EdgeKind::Takes), (&sig.rtypes, &sig.runions, EdgeKind::Returns)] {
                let mut targets: Vec<String> = types.iter().flat_map(|t| type_of(m, t, &store.nodes)).collect();
                // a union of two or more known types is a sum, written in place as by a name
                for u in unions {
                    let mut variants: Vec<String> = vec![];
                    for r in u.names.iter().flat_map(|t| type_of(m, t, &store.nodes)) {
                        if (!r.starts_with("ext:") || foreign_type(&r, &OWN)) && !variants.contains(&r) {
                            variants.push(r);
                        }
                    }
                    if variants.len() > 1 {
                        targets.push(store.written_sum(&variants, &u.written, &n));
                    } else {
                        targets.extend(variants);
                    }
                }
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
    }
    // the class a function hands on, for typing what a body holds
    let mut returned: HashMap<String, String> = HashMap::new();
    for e in store.edges.iter().filter(|e| e.kind == EdgeKind::Returns) {
        if store.nodes.get(&e.dst).map_or(false, |x| x.kind == NodeKind::Class) {
            returned.entry(e.src.clone()).or_insert_with(|| e.dst.clone());
        }
    }
    let eval = Eval::new(modules, repo);
    // of a library's code, the methods of the classes the program names, and of their bases
    let named = named_classes(modules, store);
    // methods that change their receiver, directly or through another such method of the
    // program's: until nothing changes, which ends, as the set only grows and the methods are finite
    loop {
        let mut found = vec![];
        for id in &ids {
            let n = &store.nodes[id];
            if n.library && !n.cls.as_ref().map_or(false, |c| named.contains(c)) {
                continue;
            }
            let Some(m) = modules.get(&n.module).filter(|_| n.kind == NodeKind::Method && !store.mutating.contains(id)) else { continue };
            let Some(body) = m.bodies.get(id).and_then(|(s, e)| m.tree.root_node().descendant_for_byte_range(*s, *e)) else { continue };
            let b = Body::new(m, modules, store, n, body, &returned, &eval);
            if !b.initializer && b.changed().contains("this") {
                found.push(id.clone());
            }
        }
        if found.is_empty() {
            break;
        }
        store.mutating.extend(found);
    }
    // bodies -> calls / instantiates / boundaries: the program's, then the functions of the
    // library's code read; a library's module runs its own code on import, by no call of the
    // program's
    let mut linked = Linked::default();
    let (own, library): (Vec<&String>, Vec<&String>) = ids
        .iter()
        .filter(|id| {
            let n = &store.nodes[*id];
            matches!(n.kind, NodeKind::Function | NodeKind::Method)
                || (n.kind == NodeKind::Module && !n.library && modules.contains_key(*id))
        })
        .partition(|id| !store.nodes[*id].library);
    for id in own.into_iter().chain(library) {
        let Some(m) = store.nodes.get(id).and_then(|n| modules.get(&n.module)) else { continue };
        link_calls(m, modules, store, id, &returned, &eval, &mut linked);
    }
    let handing: Vec<paths::Evaluated> = linked.args.iter().map(|arg| paths::Evaluated { arg, eval: &eval }).collect();
    let entry = |f: &str| store.nodes.get(f).map_or(false, |n| n.entry.is_some());
    let family = store.family();
    let completed = crate::paths::complete(linked.calls, &handing, &family, &|_, _| vec![], &entry);
    // a page's request without a host goes to the page's own origin
    crate::paths::place_calls(store, completed, true);
    globals(modules, store, &returned, &eval);
}

/// The fields of a class that hold what a package made: declared with a library's type, set from
/// what a package made, or a constructor's parameter property (`this.db`).
fn class_made(class: TsNode, m: &Module) -> HashMap<String, String> {
    let src = &m.src[..];
    let field = |name: TsNode| format!("this.{}", text(name, src));
    let mut out = HashMap::new();
    for f in walk(class, &["public_field_definition"]) {
        if let (Some(name), Some(pkg)) =
            (f.child_by_field_name("name"), f.child_by_field_name("type").and_then(|t| typed_package(t, m, src)))
        {
            out.insert(field(name), pkg);
        }
    }
    for p in walk(class, &["required_parameter", "optional_parameter"]).into_iter().filter(|p| is_property(*p)) {
        if let (Some(name), Some(pkg)) =
            (p.child_by_field_name("pattern"), p.child_by_field_name("type").and_then(|t| typed_package(t, m, src)))
        {
            out.insert(field(name), pkg);
        }
    }
    for a in walk(class, &["assignment_expression"]) {
        let (Some(l), Some(r)) = (a.child_by_field_name("left"), a.child_by_field_name("right")) else { continue };
        if let ([this, name], Some(pkg)) = (member_parts(l, src).as_slice(), made_by_package(r, m, src)) {
            if this == "this" {
                out.entry(format!("this.{name}")).or_insert(pkg);
            }
        }
    }
    out
}

/// The library's classes the program names (imports, builds or calls a method of), with the
/// classes of the library they inherit from.
fn named_classes(modules: &IndexMap<String, Module>, store: &GraphBuilder) -> HashSet<String> {
    let class = |id: &str| {
        store.nodes.get(id).filter(|n| n.library).and_then(|n| if n.kind == NodeKind::Class { Some(n.id.clone()) } else { n.cls.clone() })
    };
    let mut todo: Vec<String> = modules
        .values()
        .filter(|m| !store.nodes.get(&m.id).map_or(false, |n| n.library))
        .flat_map(|m| m.read.values())
        .filter_map(|d| class(d))
        .collect();
    let mut out = HashSet::new();
    while let Some(c) = todo.pop() {
        if out.insert(c.clone()) {
            todo.extend(
                store
                    .classes
                    .get(&c)
                    .map(|i| i.resolved.clone())
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|b| store.nodes.get(b).map_or(false, |n| n.library)),
            );
        }
    }
    out
}

/// One use of a module's value by a function: where the value is kept (`module.name`), the name
/// the function uses it by, and the line.
type Use = (String, String, u32);

/// The values a module keeps between calls, which its functions change: a name a module declares
/// at its top level that a function assigns anew (`current = config`, `count++`) or changes in
/// place (`LATEST[k] = v`, `seen.n = ..`, `delete seen[k]`, `queue.push(..)` and the language's
/// other methods that change their receiver), in its own module or where another imports it.
/// Every function that changes one writes it and every one that uses it reads it: a place,
/// `Global(module.name)`. A name no function changes is a constant of the code, and no place; the
/// methods of a value a package made are the package's, and change no value of the language.
fn globals(modules: &IndexMap<String, Module>, store: &mut GraphBuilder, returned: &HashMap<String, String>, eval: &Eval) {
    let tops: HashMap<&str, HashSet<String>> = modules.values().map(|m| (m.id.as_str(), top_names(m))).collect();
    let mutators = knowledge::mutators(knowledge::Registry::Npm);
    let mut uses: Vec<(String, Vec<Use>, Vec<Use>)> = vec![];
    for m in modules.values() {
        let (src, root) = (&m.src[..], m.tree.root_node());
        let kept = |n: &str| {
            if tops[m.id.as_str()].contains(n) {
                Some(format!("{}.{n}", m.id))
            } else {
                kept_in(modules, &tops, m.bindings.get(n)?, 0)
            }
        };
        for n in store.nodes.values().filter(|n| n.module == m.id && matches!(n.kind, NodeKind::Function | NodeKind::Method)) {
            let Some(&(s, e)) = m.bodies.get(&n.id) else { continue };
            let Some(body) = root.descendant_for_byte_range(s, e) else { continue };
            // a function defined in this one is a function of its own
            let inner: Vec<(usize, usize)> = m.bodies.values().copied().filter(|&(a, b)| s <= a && b <= e && (a, b) != (s, e)).collect();
            let mine = |x: &TsNode| !inner.iter().any(|&(a, b)| a <= x.start_byte() && x.end_byte() <= b);
            let b = Body::new(m, modules, store, n, body, returned, eval);
            let bound = b.bound_in();
            let module_value = |x: &str| if bound.contains(x) { None } else { kept(x) };
            let line = |x: TsNode| x.start_position().row as u32 + 1;
            let mut writes: Vec<Use> = vec![];
            for a in walk(body, &["assignment_expression", "augmented_assignment_expression", "update_expression"]).into_iter().filter(mine)
            {
                if let Some(t) =
                    a.child_by_field_name("left").or_else(|| a.child_by_field_name("argument")).filter(|t| t.kind() == "identifier")
                {
                    note(&mut writes, module_value(text(t, src)), text(t, src), line(a));
                }
            }
            // a method of the language's own that changes its receiver, on a value no package made
            let language: &dyn Fn(TsNode) -> Option<TsNode> = &|c| {
                let f = function_of(c).filter(|f| f.kind() == "member_expression")?;
                let changing = f.child_by_field_name("property").map_or(false, |p| mutators.contains(&text(p, src)));
                if !changing || b.callee(c).map_or(false, |t| b.own(&t)) {
                    return None;
                }
                let o = f.child_by_field_name("object")?;
                let made = chain_root(o, src).and_then(module_value).map_or(false, |k| package_value(modules, &k));
                (!made).then_some(o)
            };
            for (at, name) in changes_in(body, src, language).into_iter().filter(|(x, _)| mine(x)) {
                note(&mut writes, module_value(name), name, line(at));
            }
            let mut reads: Vec<Use> = vec![];
            for x in walk(body, &["identifier", "shorthand_property_identifier"]).into_iter().filter(mine) {
                // what an assignment puts in a name, not the value the name held
                let assigned = x.parent().map_or(false, |p| {
                    p.kind() == "assignment_expression" && p.child_by_field_name("left").map(|l| l.id()) == Some(x.id())
                });
                if !assigned {
                    note(&mut reads, module_value(text(x, src)), text(x, src), line(x));
                }
            }
            uses.push((n.id.clone(), writes, reads));
        }
    }
    let state: HashSet<String> = uses.iter().flat_map(|(_, w, _)| w.iter().map(|(k, _, _)| k.clone())).collect();
    for (id, writes, reads) in uses {
        for (dir, names) in [(Dir::Write, writes), (Dir::Read, reads)] {
            for (kept, name, at) in names.into_iter().filter(|(k, _, _)| state.contains(k)) {
                store.touch(&id, Touch { what: name.clone(), artifact: kept, op: name, line: at, ..Touch::new(World::Global, Some(dir)) });
            }
        }
    }
}

/// A use of a module's value, once per value.
fn note(uses: &mut Vec<Use>, kept: Option<String>, name: &str, line: u32) {
    if let Some(k) = kept.filter(|k| !uses.iter().any(|(u, _, _)| u == k)) {
        uses.push((k, name.to_string(), line));
    }
}

/// The values a module declares at its top level: `let current = ..`, `const LATEST = new Map()`.
fn top_names(m: &Module) -> HashSet<String> {
    top_variables(m.tree.root_node())
        .into_iter()
        .filter_map(|v| v.child_by_field_name("name").filter(|n| n.kind() == "identifier"))
        .map(|n| text(n, &m.src).to_string())
        .collect()
}

/// Where a name a module imports is kept: the top-level value of the module it comes from,
/// followed through re-exports.
fn kept_in(modules: &IndexMap<String, Module>, tops: &HashMap<&str, HashSet<String>>, bound: &Bound, depth: u32) -> Option<String> {
    let Bound::Own(target) = bound else { return None };
    let (mid, name) = target.rsplit_once('.')?;
    let home = modules.get(mid)?;
    if tops.get(mid).map_or(false, |t| t.contains(name)) {
        return Some(target.clone());
    }
    if depth > 8 {
        return None;
    }
    match if name == "default" { home.default.as_ref() } else { home.bindings.get(name) } {
        Some(b) => kept_in(modules, tops, b, depth + 1),
        None => home.wildcards.iter().find_map(|w| kept_in(modules, tops, &Bound::Own(format!("{w}.{name}")), depth + 1)),
    }
}

/// Whether the top-level value kept at `kept` is one a package made: built by a package's
/// constructor or function (`new Pool()`, `express()`, `new Hono().basePath(..)`), or declared
/// with a package's type (`let pool: Pool`).
fn package_value(modules: &IndexMap<String, Module>, kept: &str) -> bool {
    let Some((home, name)) = kept.rsplit_once('.').and_then(|(mid, name)| Some((modules.get(mid)?, name))) else { return false };
    let src = &home.src[..];
    let Some(v) =
        top_variables(home.tree.root_node()).into_iter().find(|v| v.child_by_field_name("name").map(|n| text(n, src)) == Some(name))
    else {
        return false;
    };
    if v.child_by_field_name("type").and_then(|t| typed_package(t, home, src)).is_some() {
        return true;
    }
    let package = |n: TsNode| matches!(member_parts(n, src).first().and_then(|h| home.bindings.get(h)), Some(Bound::Ext(_)));
    let mut value = v.child_by_field_name("value");
    while let Some(x) = value.map(|v| beneath(v, WRAPPERS)) {
        match x.kind() {
            "new_expression" => return x.child_by_field_name("constructor").map_or(false, package),
            // a chain of calls on what a package made: `new Hono().basePath("/api")`
            "call_expression" => match function_of(x) {
                Some(f)
                    if f.kind() == "member_expression"
                        && f.child_by_field_name("object").map_or(false, |o| matches!(o.kind(), "call_expression" | "new_expression")) =>
                {
                    value = f.child_by_field_name("object");
                }
                Some(f) => return package(f),
                None => return false,
            },
            _ => return false,
        }
    }
    false
}

/// The class a module-level value holds, in its own module or the one it is imported from.
/// The name a call's value is kept by: `const x = await f(..)`, `x = f(..)`, through the arms of
/// a choice (`c ? a : b`, `x ?? y`).
fn kept_as(c: TsNode, src: &[u8]) -> Option<String> {
    let mut n = c;
    loop {
        let p = n.parent()?;
        match p.kind() {
            k if WRAPPERS.contains(&k) => n = p,
            _ if (CHOICES.chooses)(p, src) && matches!((CHOICES.part)(p, n, src), Part::Arm(_)) => n = p,
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

fn link_calls<'t>(
    m: &'t Module,
    modules: &'t IndexMap<String, Module>,
    store: &mut GraphBuilder,
    id: &str,
    returned: &HashMap<String, String>,
    eval: &Eval,
    linked: &mut Linked<'t>,
) {
    let n = store.nodes[id].clone();
    let root = m.tree.root_node();
    let body = match n.kind {
        NodeKind::Module => root,
        _ => match m.bodies.get(id).and_then(|(s, e)| root.descendant_for_byte_range(*s, *e)) {
            Some(b) => b,
            None => return,
        },
    };
    let mut steps = Steps::new(body, &m.src, &CHOICES, &USES);
    let read = {
        let mut b = Body::new(m, modules, store, &n, body, returned, eval);
        // a loop binds its element once what it goes over is made, and before its body runs
        let mut ran: Vec<((usize, u8), TsNode)> = b.sites.iter().map(|c| ((c.end_byte(), 0), *c)).collect();
        ran.extend(
            walk(body, &["for_in_statement"])
                .into_iter()
                .filter(|l| b.mine(*l) && l.child_by_field_name("operator").map_or(false, |o| text(o, &m.src) == "of"))
                .filter_map(|l| l.child_by_field_name("right").map(|r| ((r.end_byte(), 1), l))),
        );
        ran.sort_by_key(|(k, _)| *k);
        for (_, c) in ran {
            if c.kind() == "for_in_statement" {
                b.each(&mut steps, c);
                continue;
            }
            let (t, w) = (b.read.touches.len(), b.read.calls.len());
            b.call(&mut steps, c);
            steps.mark(c, b.read.touches[t..].iter_mut().chain(b.read.calls[w..].iter_mut().map(|(t, _)| t)));
        }
        b.class_state(&mut steps);
        b.ends();
        b.read
    };
    let compares = (n.kind != NodeKind::Module).then(|| {
        // a qualifier is a type when it names an `enum` or a class of the program
        let is_type = |(a, b): (usize, usize)| {
            let parts: Vec<String> = String::from_utf8_lossy(&m.src[a..b]).split('.').map(str::to_string).collect();
            resolve(m, &parts, modules, &store.nodes).and_then(|t| store.nodes.get(&t)).is_some_and(|t| t.kind == NodeKind::Class)
        };
        // the value a method runs on is `this`, unless it is called on its class
        let receiver =
            n.params.iter().find(|p| p.receiver).map(|p| p.name.as_str()).or((n.kind == NodeKind::Method && !n.on_class).then_some("this"));
        crate::tree::compares(body, &m.src, steps.choices(), &COMPARES, &CHOICES, &crate::tree::Scope { receiver, is_type: &is_type })
    });
    steps.finish(store, id);
    if let Some(node) = store.nodes.get_mut(id) {
        node.calls_handed = read.unresolved.iter().any(|(u, _)| n.params.iter().any(|p| &p.name == u));
    }
    for (name, bucket) in read.unresolved {
        store.not_followed(name, bucket);
    }
    for t in read.used {
        store.used.insert((id.to_string(), t));
    }
    for t in read.touches {
        store.touch(id, t);
    }
    for (t, input) in read.mutates {
        if t.starts_with("ext:") {
            store.ext_node(&t);
        }
        store.add_edge(id, &t, EdgeKind::Mutates, Some(input));
    }
    if let (Some(compares), Some(node)) = (compares, store.nodes.get_mut(id)) {
        node.compares = compares;
        node.raises = read.exits
            || crate::tree::holds(
                body,
                &m.src,
                |n, _| n.kind() == "throw_statement",
                &["function_declaration", "class_declaration", "method_definition"],
            );
    }
    linked.args.extend(read.args);
    linked.calls.extend(read.calls.into_iter().map(|(t, built)| (id.to_string(), t, built)));
}

/// What reading a body finds besides its steps: what it touches, the calls Atlas could not
/// follow, the functions whose value it uses, the inputs it changes in place, the calls of the
/// package's functions with the arguments they pass, the file and network calls whose path
/// or URL waits for them, and whether it calls what ends the program.
#[derive(Default)]
struct Read<'t> {
    touches: Vec<Touch>,
    unresolved: Vec<(String, Bucket)>,
    used: Vec<String>,
    mutates: Vec<(String, String)>,
    args: Vec<Arg<'t>>,
    calls: Vec<(Touch, Vec<crate::paths::Built>)>,
    exits: bool,
}

/// A function's body, or a module's own code, as TypeScript reads it: the program around it,
/// and the classes and packages its names hold.
struct Body<'a, 't> {
    m: &'t Module,
    modules: &'t IndexMap<String, Module>,
    store: &'a GraphBuilder,
    src: &'t [u8],
    root: TsNode<'t>,
    body: TsNode<'t>,
    id: String,
    is_module: bool,
    /// The function the body belongs to, and its parameters by name.
    scope: Option<TsNode<'t>>,
    params: Vec<String>,
    cls: Option<String>,
    initializer: bool,
    /// The classes the fields of the body's own class hold.
    attrs: std::collections::BTreeMap<String, String>,
    /// The class each of the program's functions hands on.
    returned: &'a HashMap<String, String>,
    eval: &'a Eval<'a>,
    /// The classes the body's names hold: its typed parameters, and locals declared with a type,
    /// built with `new`, or handed on by a function.
    locals: HashMap<String, String>,
    /// What a change to each typed parameter changes (`inputs_of`).
    inputs: HashMap<String, String>,
    /// The values a package made, by the package: a call on one is the package's.
    made: HashMap<String, String>,
    /// The names that hold one of the language's own containers, as the body declares or builds
    /// them: `rows: Row[]`, `const seen = new Set<string>()`, `const out = []`.
    containers: HashSet<String>,
    /// The calls, constructors, elements and names of the body, in the order they run.
    sites: Vec<TsNode<'t>>,
    read: Read<'t>,
}

const STRINGS: &[&str] = &["string", "template_string"];

impl<'a, 't> Body<'a, 't> {
    fn new(
        m: &'t Module,
        modules: &'t IndexMap<String, Module>,
        store: &'a GraphBuilder,
        n: &Node,
        body: TsNode<'t>,
        returned: &'a HashMap<String, String>,
        eval: &'a Eval<'a>,
    ) -> Self {
        let is_module = n.kind == NodeKind::Module;
        let scope = if is_module { None } else { paths::function_node(body) };
        let cls = n.cls.clone();
        let attrs = cls.as_ref().and_then(|c| store.classes.get(c)).map(|c| c.attr_types.clone()).unwrap_or_default();
        let mut b = Body {
            m,
            modules,
            store,
            src: &m.src,
            root: m.tree.root_node(),
            body,
            id: n.id.clone(),
            is_module,
            scope,
            params: scope.map(|f| paths::param_names(f, &m.src)).unwrap_or_default(),
            cls,
            initializer: n.ctor && !n.on_class,
            attrs,
            returned,
            eval,
            locals: HashMap::new(),
            inputs: HashMap::new(),
            made: HashMap::new(),
            containers: HashSet::new(),
            sites: vec![],
            read: Read::default(),
        };
        b.typed(n);
        b.made();
        b.sites = evaluation_order(walk(
            body,
            &["call_expression", "new_expression", "object", "jsx_opening_element", "jsx_self_closing_element", "identifier"],
        ))
        .into_iter()
        .filter(|x| b.mine(*x))
        .collect();
        b
    }

    /// Whether a node is the body's own: inside a module's own code, not in a function it defines.
    fn mine(&self, x: TsNode) -> bool {
        !self.is_module || module_level(x, self.root, &self.m.bodies)
    }

    fn resolve(&self, parts: &[String]) -> Option<String> {
        resolve(self.m, parts, self.modules, &self.store.nodes)
    }

    fn class(&self, r: Option<String>) -> Option<String> {
        r.filter(|r| self.store.nodes.get(r).map_or(false, |x| x.kind == NodeKind::Class))
    }

    /// The values a class keeps that the body changes by assignment: `Counter.total += 1`,
    /// `Registry.handlers[name] = fn`. A static of a class is no place Atlas draws, so what the
    /// change leaves behind it does not see.
    fn class_state(&self, steps: &mut Steps) {
        if self.is_module {
            return;
        }
        let src = self.src;
        for a in walk(self.body, &["assignment_expression", "augmented_assignment_expression", "update_expression"]) {
            let Some(mut root) = a.child_by_field_name(if a.kind() == "update_expression" { "argument" } else { "left" }) else { continue };
            let mut through = false;
            while let Some(inner) = match root.kind() {
                "member_expression" | "subscript_expression" => root.child_by_field_name("object"),
                _ => None,
            } {
                (root, through) = (inner, true);
            }
            if through && root.kind() == "identifier" && self.class(self.resolve(&[text(root, src).to_string()])).is_some() {
                steps.unseen(a, text(a, src));
            }
        }
    }

    /// Whether `r` is one of the program's own functions or classes.
    fn own(&self, r: &str) -> bool {
        !r.starts_with("ext:") && self.store.nodes.contains_key(r)
    }

    /// What a change to each typed parameter changes, by name: the type the picture draws it as,
    /// the program's class its type names, also inside a container (`e: Expense`, `rows: Row[]`,
    /// `Map<string, Row>`), or a library's class (`http: AxiosInstance`); else the language's own
    /// container it is (`lines: string[]` is an `Array`).
    fn inputs_of(&self) -> HashMap<String, String> {
        let src = self.src;
        let mut out = HashMap::new();
        for p in self.scope.and_then(|f| f.child_by_field_name("parameters")).map(named_children).unwrap_or_default() {
            let (Some(name), Some(ty)) =
                (p.child_by_field_name("pattern").filter(|x| x.kind() == "identifier"), p.child_by_field_name("type"))
            else {
                continue;
            };
            let drawn = type_names(Some(ty), src).into_iter().filter_map(|t| self.resolve(&t)).find(|r| {
                if r.starts_with("ext:") {
                    foreign_type(r, &OWN)
                } else {
                    self.class(Some(r.clone())).is_some()
                }
            });
            let written = text(ty, src).trim_start_matches(':').trim();
            let container = container_type(written, self.m).then(|| {
                let head = written.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$')).next().unwrap_or("");
                format!("ext:javascript.{}", if knowledge::TYPESCRIPT_CHANGEABLE.contains(&head) { head } else { "Array" })
            });
            if let Some(t) = drawn.or(container) {
                out.insert(text(name, src).to_string(), t);
            }
        }
        out
    }

    /// Whether a value holds one of the language's own containers that change in place: a name the
    /// body declares or builds as one, or a field declared as one of the class the value is
    /// (`this.rows`, `bag.items`), or of a class it inherits from.
    fn holds_container(&self, v: TsNode) -> bool {
        let src = self.src;
        let v = beneath(v, WRAPPERS);
        match v.kind() {
            "identifier" => self.containers.contains(text(v, src)),
            "member_expression" => {
                let (Some(o), Some(field)) = (v.child_by_field_name("object"), v.child_by_field_name("property")) else { return false };
                // the class of the value the field is read from: the body's own, a name's, a field's of the body's own
                let class = match o.kind() {
                    "this" => self.cls.clone(),
                    "identifier" => self.locals.get(text(o, src)).cloned(),
                    "member_expression" if o.child_by_field_name("object").map_or(false, |t| t.kind() == "this") => {
                        o.child_by_field_name("property").and_then(|p| self.attrs.get(text(p, src)).cloned())
                    }
                    _ => None,
                };
                let (mut todo, mut seen) = (class.into_iter().collect::<Vec<_>>(), HashSet::new());
                while let Some(c) = todo.pop() {
                    if !seen.insert(c.clone()) {
                        continue;
                    }
                    let home = self.store.nodes.get(&c).and_then(|n| self.modules.get(&n.module));
                    if home.and_then(|m| m.containers.get(&c)).map_or(false, |fs| fs.contains(text(field, src))) {
                        return true;
                    }
                    todo.extend(self.store.classes.get(&c).map(|i| i.resolved.clone()).unwrap_or_default());
                }
                false
            }
            _ => false,
        }
    }

    /// Parameters and locals with a class type give receivers a type.
    fn typed(&mut self, n: &Node) {
        let src = self.src;
        // the types the body's names are written with: its parameters' and its typed locals'
        let mut written: HashMap<String, String> = n.params.iter().map(|p| (p.name.clone(), p.ty.clone())).collect();
        for p in n.params.iter().filter(|p| !p.ty.is_empty()) {
            if container_type(&p.ty, self.m) {
                self.containers.insert(p.name.clone());
            }
            let ty = p.ty.split(['<', '[', ' ', '|']).next().unwrap_or("").to_string();
            if let Some(r) = self.class(self.resolve(&[ty])) {
                self.locals.insert(p.name.clone(), r);
            }
        }
        self.inputs = self.inputs_of();
        let declarators: Vec<TsNode> = walk(self.body, &["variable_declarator"]).into_iter().filter(|v| self.mine(*v)).collect();
        for v in declarators {
            let (Some(name), Some(value)) = (v.child_by_field_name("name"), v.child_by_field_name("value")) else { continue };
            if name.kind() != "identifier" {
                continue;
            }
            let ty = v.child_by_field_name("type").map(|t| text(t, src));
            if ty.map_or_else(|| builds_container(value, self.m, src), |t| container_type(t, self.m)) {
                self.containers.insert(text(name, src).to_string());
            }
            if let Some(t) = ty {
                written.insert(text(name, src).to_string(), t.to_string());
            }
            let declared = v.child_by_field_name("type").and_then(|t| named_children(t).into_iter().next()).map(|t| member_parts(t, src));
            let built = (value.kind() == "new_expression")
                .then(|| value.child_by_field_name("constructor").map(|c| member_parts(c, src)))
                .flatten();
            let typed = self.class(declared.or(built).and_then(|t| self.resolve(&t)));
            // `const p = await getProvider()`: what the function hands on
            let handed = || {
                let call = walk(value, &["call_expression"]).into_iter().next()?;
                let f = self.resolve(&member_parts(function_of(call)?, src))?;
                self.returned.get(&f).cloned()
            };
            if let Some(r) = typed.or_else(handed) {
                self.locals.insert(text(name, src).to_string(), r);
            }
        }
        // `for (const r of rows)`: an element of the container the loop goes over, as its type is
        // written (`rows: Row[]`)
        let loops: Vec<TsNode> = walk(self.body, &["for_in_statement"]).into_iter().filter(|l| self.mine(*l)).collect();
        for l in loops {
            let (Some(left), Some(right)) = (l.child_by_field_name("left"), l.child_by_field_name("right")) else { continue };
            let of = l.child_by_field_name("operator").map_or(false, |o| text(o, src) == "of");
            if !of || left.kind() != "identifier" {
                continue;
            }
            // over a field of a value of the program's class (`g.nodes`): the class the field's type holds
            if right.kind() == "member_expression" {
                let (Some(obj), Some(prop)) = (right.child_by_field_name("object"), right.child_by_field_name("property")) else {
                    continue;
                };
                let Some(owner) = self.locals.get(text(obj, src)).cloned() else { continue };
                let types = self
                    .store
                    .classes
                    .get(&owner)
                    .and_then(|k| k.fields.iter().find(|(f, _)| f == text(prop, src)).map(|(_, t)| t.clone()));
                if let Some(r) = types.unwrap_or_default().into_iter().find_map(|t| self.class(self.resolve(&t))) {
                    self.locals.insert(text(left, src).to_string(), r);
                }
                continue;
            }
            // over a name of the body, or one its module declares (`const STATIONS: Station[]`), as its type is written
            let declared = || {
                walk(self.root, &["variable_declarator"])
                    .into_iter()
                    .filter(|v| module_level(*v, self.root, &self.m.bodies))
                    .find(|v| v.child_by_field_name("name").map(|n| text(n, src)) == Some(text(right, src)))
                    .and_then(|v| v.child_by_field_name("type"))
                    .map(|t| text(t, src).trim_start_matches(':').trim().to_string())
            };
            let Some(ty) = written.get(text(right, src)).cloned().or_else(declared).filter(|t| container_type(t, self.m)) else {
                continue;
            };
            let element = ty
                .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
                .filter(|w| !w.is_empty() && !knowledge::TYPESCRIPT_CHANGEABLE.contains(w))
                .find_map(|w| self.class(self.resolve(&[w.to_string()])));
            if let Some(r) = element {
                self.locals.insert(text(left, src).to_string(), r);
            }
        }
    }

    /// The values a package made, by the package, in the body and at its module's top level.
    fn made(&mut self) {
        let (m, src, root) = (self.m, self.src, self.root);
        let named =
            |v: TsNode<'t>| Some((v.child_by_field_name("name").filter(|n| n.kind() == "identifier")?, v.child_by_field_name("value")?));
        for v in walk(if self.is_module { root } else { self.body }, &["variable_declarator"]) {
            let Some((name, value)) = named(v) else { continue };
            if self.mine(v) || self.is_module || module_level(v, root, &m.bodies) {
                if let Some(p) = made_by_package(value, m, src) {
                    self.made.insert(text(name, src).to_string(), p);
                }
            }
        }
        // a parameter declared with a library's type holds a value its package made
        for p in self.scope.and_then(|f| f.child_by_field_name("parameters")).map(named_children).unwrap_or_default() {
            if let (Some(name), Some(pkg)) = (
                p.child_by_field_name("pattern").filter(|x| x.kind() == "identifier"),
                p.child_by_field_name("type").and_then(|t| typed_package(t, m, src)),
            ) {
                self.made.insert(text(name, src).to_string(), pkg);
            }
        }
        // so does a field of the body's class: declared with a library's type, set from what a
        // package made, or a constructor's parameter property (`this.db`)
        let mut class = self.body.parent();
        while let Some(c) = class.filter(|c| !matches!(c.kind(), "class_declaration" | "class" | "abstract_class_declaration")) {
            class = c.parent();
        }
        if let Some(class) = class.filter(|_| !self.is_module) {
            let fields = m.made.borrow_mut().entry(Some(class.start_byte())).or_insert_with(|| class_made(class, m)).clone();
            self.made.extend(fields);
        }
        if !self.is_module {
            let tops = m
                .made
                .borrow_mut()
                .entry(None)
                .or_insert_with(|| {
                    let mut out = HashMap::new();
                    for v in walk(root, &["variable_declarator"]).into_iter().filter(|v| module_level(*v, root, &m.bodies)) {
                        let Some((name, value)) = named(v) else { continue };
                        if let Some(p) = made_by_package(value, m, src) {
                            out.entry(text(name, src).to_string()).or_insert(p);
                        }
                    }
                    out
                })
                .clone();
            for (k, p) in tops {
                self.made.entry(k).or_insert(p);
            }
        }
    }

    /// A loop as the step it is: it takes what it goes over, and gives each element under the
    /// name it binds (`for (const n of g.nodes)` takes `g` and gives `n`).
    fn each(&mut self, steps: &mut Steps, l: TsNode<'t>) {
        let (Some(left), Some(right)) = (l.child_by_field_name("left"), l.child_by_field_name("right")) else { return };
        let names: Vec<String> = walk(left, &["identifier"]).into_iter().map(|i| text(i, self.src).to_string()).collect();
        let hands = self.hands(&[Some(right)], &steps.kept);
        let gives: Vec<Held> = names
            .iter()
            .filter_map(|n| match self.locals.get(n) {
                Some(t) => Some(Held::new(t.clone(), n.clone())),
                None => (!hands.is_empty()).then(|| Held::new(String::new(), n.clone())),
            })
            .collect();
        // the step stands where what it goes over is written: a call there hands it what it gives, the loop's body does not
        steps.op(right, &format!("for {} of {}", text(left, self.src), text(right, self.src)), hands, gives);
    }

    /// What a call is handed: the value it is called on, its arguments, an element's attributes.
    fn hands(&self, parts: &[Option<TsNode>], kept: &HashSet<String>) -> Vec<Held> {
        let ty_of = |name: &str| self.locals.get(name).cloned().or_else(|| value_type(self.m, name, self.modules, &self.store.nodes));
        let mut out: Vec<Held> = vec![];
        for p in parts.iter().flatten() {
            for h in crate::tree::held_names(*p, self.src, &[], &ty_of, kept) {
                if !out.contains(&h) {
                    out.push(h);
                }
            }
        }
        out
    }

    /// What a call gives back, by the name the body keeps it by. Its type is one a step gives: what
    /// the program's function `to` hands on, as the name it is kept by is declared or as the
    /// function says. A call Atlas does not follow gives no type the program names: a cast or an
    /// annotation on its value (`JSON.parse(..) as Config`) is asserted, not given.
    fn gives(&self, c: TsNode, to: Option<&str>) -> Vec<Held> {
        let ty =
            |name: &String| to.and_then(|t| self.locals.get(name).cloned().or_else(|| self.returned.get(t).cloned())).unwrap_or_default();
        kept_as(c, self.src).map(|name| Held::new(ty(&name), name)).into_iter().collect()
    }

    /// The values the body keeps of what a parse of the command line gives: the name it is kept
    /// by, or the names a pattern takes out of it (`const { values } = parseArgs(..)`).
    fn parsed_into(&self, c: TsNode) -> Vec<Held> {
        let v = outermost(c, WRAPPERS);
        let pattern = v
            .parent()
            .filter(|d| d.kind() == "variable_declarator" && d.child_by_field_name("value").map(|x| x.id()) == Some(v.id()))
            .and_then(|d| d.child_by_field_name("name"));
        match pattern.filter(|p| p.kind() == "object_pattern") {
            Some(p) => named_children(p)
                .into_iter()
                .filter_map(|x| match x.kind() {
                    "shorthand_property_identifier_pattern" => Some(x),
                    "pair_pattern" => x.child_by_field_name("value").filter(|v| v.kind() == "identifier"),
                    _ => None,
                })
                .map(|x| Held::new("", text(x, self.src)))
                .collect(),
            None => self.gives(c, None),
        }
    }

    /// One site of the body, as the step it is.
    fn call(&mut self, steps: &mut Steps, c: TsNode<'t>) {
        match c.kind() {
            "new_expression" => self.constructed(steps, c),
            "object" => self.literal(steps, c),
            "jsx_opening_element" | "jsx_self_closing_element" => self.rendered(steps, c),
            "identifier" => self.handed_on(steps, c),
            _ => self.called(steps, c),
        }
    }

    /// `new T(..)` of one of the program's classes, handing its constructor its arguments.
    fn constructed(&mut self, steps: &mut Steps, c: TsNode<'t>) {
        let parts = c.child_by_field_name("constructor").map(|x| member_parts(x, self.src)).unwrap_or_default();
        if let Some(r) = self.class(self.resolve(&parts).filter(|r| self.own(r))) {
            let step = Step {
                hands: self.hands(&[c.child_by_field_name("arguments")], &steps.kept),
                gives: self.gives(c, Some(&r)),
                ..Default::default()
            };
            steps.call(c, &r, EdgeKind::Instantiates, None, step);
            let ctor = self.store.initializer_of(&r);
            if let Some(a) = ctor.and_then(|t| argument(self.modules, self.store, &self.id, &t, c, self.m, self.scope, &self.params)) {
                self.read.args.push(a);
            }
        }
    }

    /// `const settings: Settings = { .. }`: an object literal declared as a type of the program's,
    /// which the compiler checks it against, makes a value of that type, as `new` does: a step to
    /// the type, handed what the literal is built from.
    fn literal(&self, steps: &mut Steps, c: TsNode<'t>) {
        let v = outermost(c, WRAPPERS);
        let Some(d) =
            v.parent().filter(|d| d.kind() == "variable_declarator" && d.child_by_field_name("value").map(|x| x.id()) == Some(v.id()))
        else {
            return;
        };
        let Some(ty) = d.child_by_field_name("type").and_then(|t| named_children(t).into_iter().next()) else { return };
        if let Some(r) = self.class(self.resolve(&type_head(ty, self.src))).filter(|r| self.own(r)) {
            let step = Step { hands: self.hands(&[Some(c)], &steps.kept), gives: self.gives(c, Some(&r)), ..Default::default() };
            steps.call(c, &r, EdgeKind::Instantiates, None, step);
        }
    }

    /// `<Kaart/>` renders a component, handed its attributes; a lowercase name is an element of
    /// the page.
    fn rendered(&self, steps: &mut Steps, c: TsNode<'t>) {
        let parts = c.child_by_field_name("name").map(|x| member_parts(x, self.src)).unwrap_or_default();
        if parts.first().map_or(true, |p| !p.starts_with(|ch: char| ch.is_uppercase()) && parts.len() == 1) {
            return;
        }
        if let Some(r) = self.resolve(&parts).filter(|r| self.own(r)) {
            let kind = if self.store.nodes[&r].kind == NodeKind::Class { EdgeKind::Instantiates } else { EdgeKind::Calls };
            let attrs: Vec<Option<TsNode>> = walk(c, &["jsx_attribute"]).into_iter().map(Some).collect();
            steps.call(c, &r, kind, None, Step { hands: self.hands(&attrs, &steps.kept), ..Default::default() });
        }
    }

    /// An own function handed on as a value: called by whoever receives it.
    fn handed_on(&self, steps: &mut Steps, c: TsNode<'t>) {
        let handed = c.parent().map_or(false, |p| match p.kind() {
            "arguments" | "array" | "jsx_expression" | "return_statement" => true,
            "pair" | "variable_declarator" => p.child_by_field_name("value").map(|v| v.id()) == Some(c.id()),
            _ => false,
        });
        let fun = |r: &String| matches!(self.store.nodes[r].kind, NodeKind::Function | NodeKind::Method);
        if let Some(r) = self.resolve(&[text(c, self.src).to_string()]).filter(|r| handed && self.own(r) && fun(r)) {
            steps.handed(c, c, &r, EdgeKind::Calls);
        }
    }

    /// The class of the value a method is called on, typed however the body knows it: built with
    /// `new`, or `super`, the class the body's own class extends. A cast names the class itself:
    /// `(x as Store).f()` runs on a Store.
    fn receiver(&self, o: TsNode) -> Option<String> {
        let src = self.src;
        match o.kind() {
            "as_expression" => self.class(self.resolve(&type_head(named_children(o).into_iter().last()?, src))).filter(|r| self.own(r)),
            k if WRAPPERS.contains(&k) => self.receiver(wrapped(o)?),
            "this" => self.cls.clone(),
            "super" => self.class(self.resolve(self.extends()?)).filter(|r| self.own(r)),
            "identifier" => {
                self.locals.get(text(o, src)).cloned().or_else(|| value_type(self.m, text(o, src), self.modules, &self.store.nodes))
            }
            "call_expression" => self.returned.get(&self.resolve(&member_parts(function_of(o)?, src))?).cloned(),
            "new_expression" => self.class(self.resolve(&member_parts(o.child_by_field_name("constructor")?, src))).filter(|r| self.own(r)),
            "member_expression" if o.child_by_field_name("object").map(|x| x.kind()) == Some("this") => {
                self.attrs.get(text(o.child_by_field_name("property")?, src)).cloned()
            }
            _ => None,
        }
    }

    /// The class the body's own class extends, as written: the first of its bases.
    fn extends(&self) -> Option<&[String]> {
        self.store.classes.get(self.cls.as_deref()?)?.bases.first().map(Vec::as_slice)
    }

    /// What call `c` reaches: the method of the class of the value it is called on, else what
    /// the name it is called by is bound to; nothing when it is called through a value the body
    /// cannot name (`EXPORTERS[format](..)`, `make()(..)`). `super(..)` reaches the initializer
    /// the class the body's class extends runs.
    fn callee(&self, c: TsNode) -> Option<String> {
        let f = function_of(c)?;
        if f.kind() == "super" {
            return self.store.initializer_of(&self.receiver(f)?);
        }
        let method = (f.kind() == "member_expression")
            .then(|| Some((f.child_by_field_name("object")?, text(f.child_by_field_name("property")?, self.src))))
            .flatten();
        if let Some(t) = method.and_then(|(o, name)| self.store.method_of(&self.receiver(o)?, name)) {
            return Some(t);
        }
        let parts = member_parts(f, self.src);
        if parts.is_empty() {
            None
        } else {
            self.resolve(&parts)
        }
    }

    /// `Source.prototype.read.call(v, ..)` or `.apply(v, [..])`: the method the class's prototype
    /// finds, called through its class on the value handed first, and no override of it. Whether
    /// the call is one.
    fn through_prototype(&mut self, steps: &mut Steps, c: TsNode<'t>, parts: &[String]) -> bool {
        let [class @ .., proto, name, how] = parts else { return false };
        if proto != "prototype" || !matches!(how.as_str(), "call" | "apply") {
            return false;
        }
        let Some(t) = self.class(self.resolve(class)).filter(|k| self.own(k)).and_then(|k| self.store.method_of(&k, name)) else {
            return false;
        };
        // the value it runs on: the body's own receiver, or a value of a class below the method's
        let value = c.child_by_field_name("arguments").and_then(|a| named_children(a).into_iter().next());
        let on_receiver = value.map_or(false, |v| v.kind() == "this");
        let on = value
            .filter(|_| !on_receiver)
            .and_then(|v| self.receiver(v))
            .filter(|k| self.store.nodes[&t].cls.as_deref().map_or(false, |b| b != k.as_str()))
            .unwrap_or_default();
        if value_used(c) {
            self.read.used.push(t.clone());
        }
        let hands = self.hands(&[c.child_by_field_name("arguments")], &steps.kept);
        let step = Step { on, on_receiver, named: true, hands, gives: self.gives(c, Some(&t)), ..Default::default() };
        steps.call(c, &t, EdgeKind::Calls, None, step);
        true
    }

    /// `f(..)` or `x.m(..)`: the program's function or method, found on the class of the value it
    /// is called on, or a call Atlas does not follow: a library's operation, which may cross into
    /// the world, or a call through a value the body cannot name.
    fn called(&mut self, steps: &mut Steps, c: TsNode<'t>) {
        let src = self.src;
        let Some(f) = function_of(c) else { return };
        if read_in_place(f) {
            return;
        }
        let parts = member_parts(f, src);
        if self.through_prototype(steps, c, &parts) {
            return;
        }
        let object = (f.kind() == "member_expression").then(|| f.child_by_field_name("object")).flatten();
        let class = object.and_then(|o| self.receiver(o));
        let target = self.callee(c);
        let hands = self.hands(&[object, c.child_by_field_name("arguments")], &steps.kept);
        match target {
            Some(t) if self.own(&t) => {
                // `super.f()` runs on the body's own receiver the next `f` after the class it is
                // written in; a static method called through a class (`Base.make()`) is that
                // class's, and no override of it, and so is the initializer `super(..)` runs
                let sup = object.map_or(false, |o| o.kind() == "super");
                let named = f.kind() == "super"
                    || !sup
                        && self.store.nodes[&t].kind == NodeKind::Method
                        && object
                            .filter(|o| o.kind() == "identifier")
                            .and_then(|o| self.resolve(&[text(o, src).to_string()]))
                            .map_or(false, |r| self.store.nodes.get(&r).map_or(false, |n| n.kind == NodeKind::Class));
                let after = if sup { self.cls.clone().unwrap_or_default() } else { String::new() };
                // a method found on a base runs on the subclass the value is
                let on =
                    class.filter(|k| !sup && self.store.nodes[&t].cls.as_deref().map_or(false, |b| b != k.as_str())).unwrap_or_default();
                let on_receiver = object.map_or(false, |o| o.kind() == "this") || !after.is_empty();
                let kind = if self.store.nodes[&t].kind == NodeKind::Class { EdgeKind::Instantiates } else { EdgeKind::Calls };
                if kind == EdgeKind::Calls && value_used(c) {
                    self.read.used.push(t.clone());
                }
                let step = Step { on, on_receiver, named, after, hands, gives: self.gives(c, Some(&t)), ..Default::default() };
                steps.call(c, &t, kind, None, step);
                let callee = if kind == EdgeKind::Instantiates { self.store.initializer_of(&t) } else { Some(t.clone()) };
                if let Some(mut a) =
                    callee.and_then(|t| argument(self.modules, self.store, &self.id, &t, c, self.m, self.scope, &self.params))
                {
                    a.on =
                        object.filter(|o| o.kind() == "identifier").map(|o| text(o, src).to_string()).filter(|o| self.params.contains(o));
                    self.read.args.push(a);
                }
            }
            // a call not followed is a step where the body keeps what it gives, and is counted
            // by the name the code calls it by
            _ if parts.is_empty() => {
                let (name, bucket) = (written_callee(f, src), self.bucket_of(f));
                self.unread(steps, c, &name, bucket, hands, vec![]);
                // what the language's own computes Atlas reads; what anything else does, it does not see
                if bucket != Bucket::Builtin {
                    steps.dropped(c, &name);
                }
                self.read.unresolved.push((name, bucket));
            }
            _ => {
                let changed = self.changes(c);
                let bucket = if changed.is_empty() { self.bucket_path(&parts) } else { Bucket::Builtin };
                // a library's operation by its name, a call Atlas cannot follow as written
                let name = if matches!(bucket, Bucket::Library | Bucket::Builtin) {
                    parts.last().cloned().unwrap_or_default()
                } else {
                    text(f, src).to_string()
                };
                let parsed = self.eval.command_line(At { m: self.m, scope: self.scope, params: &self.params }, c);
                // the language's own, a change and a parse of the command line are what the tables say
                let known = parsed.is_some() || bucket == Bucket::Builtin;
                match parsed {
                    Some(options) => steps.parses(c, &name, hands, self.parsed_into(c), options),
                    None => self.unread(steps, c, &name, bucket, hands, changed),
                }
                self.read.unresolved.push((parts.iter().take(2).cloned().collect::<Vec<_>>().join("."), bucket));
                let exits = matches!(parts.as_slice(), [.., module, f] if knowledge::ends_the_run(module, f));
                self.read.exits |= exits;
                let crossed = (self.read.touches.len(), self.read.calls.len());
                self.crossed(c, &parts);
                // a package's call the tables say nothing of beside what it gives, or one Atlas
                // cannot follow: what it does, Atlas does not see
                if !known && !exits && crossed == (self.read.touches.len(), self.read.calls.len()) {
                    steps.dropped(c, &text(f, src));
                }
            }
        }
    }

    /// What a callee no name path reaches starts from, as `bucket` reads a name: the first name
    /// path it is written on (`this.db.prepare(..).run` starts at `this.db`). A method on what a
    /// call Atlas follows gives is on a value the body made; one on a literal or on what an
    /// operator computes is the language's own (`"a,b".split`, `[..xs].sort`, `(a - b).toFixed`);
    /// `super` is the class the body's class extends.
    fn bucket_of(&self, n: TsNode) -> Bucket {
        let parts = member_parts(n, self.src);
        if !parts.is_empty() {
            return self.bucket_path(&parts);
        }
        let logical = || n.child_by_field_name("operator").map_or(false, |o| matches!(text(o, self.src), "??" | "||" | "&&"));
        let inner = match n.kind() {
            // what a step Atlas follows gives, or what the body builds of its own classes, is a
            // value the body made
            "call_expression" if self.callee(n).map_or(false, |t| self.own(&t)) => return Bucket::Local,
            "new_expression" if self.receiver(n).is_some() => return Bucket::Local,
            "member_expression" | "subscript_expression" => n.child_by_field_name("object"),
            "call_expression" => function_of(n),
            "new_expression" => n.child_by_field_name("constructor"),
            k if WRAPPERS.contains(&k) => wrapped(n),
            // `a ?? b` is one of its operands, any other operator's value the language's own
            "binary_expression" if logical() => n.child_by_field_name("left"),
            "ternary_expression" => n.child_by_field_name("consequence"),
            "binary_expression" | "unary_expression" | "string" | "template_string" | "number" | "array" | "object" | "regex" | "true"
            | "false" => return Bucket::Builtin,
            "super" => return self.extends().and_then(|b| b.first()).map_or(Bucket::Unknown, |b| self.bucket(b)),
            _ => None,
        };
        inner.map_or(Bucket::Unknown, |i| self.bucket_of(i))
    }

    /// What a call's name path starts from is bound to; a field that holds what a package made is
    /// that package's, as a local is.
    /// A call the body reads no further, as a step: a library's operation where the body keeps what
    /// it gives or what it `changed`, any other call Atlas cannot follow always, since it runs all
    /// the same.
    fn unread(&self, steps: &mut Steps, c: TsNode<'t>, name: &str, bucket: Bucket, hands: Vec<Held>, changed: Vec<Held>) {
        if !changed.is_empty() {
            steps.changes(c, name, hands, self.gives(c, None), changed);
        } else if matches!(bucket, Bucket::Library | Bucket::Builtin) {
            steps.op(c, name, hands, self.gives(c, None));
        } else {
            steps.not_followed(c, name, hands, self.gives(c, None));
        }
    }

    /// The container a method of the language's own changes, as the value the call gives: `rows`
    /// for `rows.push(row)`, where the body holds an array, a `Map` or a `Set` by that name.
    fn changes(&self, c: TsNode) -> Vec<Held> {
        let Some(f) = function_of(c).filter(|f| f.kind() == "member_expression") else { return vec![] };
        let (Some(o), Some(p)) = (f.child_by_field_name("object"), f.child_by_field_name("property")) else { return vec![] };
        let name = text(o, self.src);
        if o.kind() != "identifier"
            || !self.containers.contains(name)
            || !knowledge::mutates(knowledge::Registry::Npm, "javascript", text(p, self.src))
        {
            return vec![];
        }
        let ty = self.locals.get(name).cloned().unwrap_or_default();
        vec![Held { ty, name: name.to_string(), at: o.start_byte() as u32 }]
    }

    fn bucket_path(&self, parts: &[String]) -> Bucket {
        self.bucket(self.held_field(parts).as_deref().unwrap_or(&parts[0]))
    }

    /// The field of the class a call goes through, when it holds what a package made: `this.db` of
    /// `this.db.prepare(..)`. A call on it is a call on the value the field holds.
    fn held_field(&self, parts: &[String]) -> Option<String> {
        (parts.len() > 2 && parts[0] == "this").then(|| format!("this.{}", parts[1])).filter(|f| self.made.contains_key(f))
    }

    /// What the name a call not followed starts from is bound to: in the body (a parameter, a
    /// local, the receiver), imported from a package, the module's own, or nothing the program
    /// binds: the language's globals. A name that holds a value a package made is the package's.
    fn bucket(&self, head: &str) -> Bucket {
        let made = self.made.contains_key(head);
        if head == "this" || self.params.iter().any(|p| p == head) || self.bound_in().contains(head) {
            return if made { Bucket::Library } else { Bucket::Local };
        }
        match self.m.bindings.get(head) {
            Some(Bound::Ext(_)) => Bucket::Library,
            Some(Bound::Own(_)) => Bucket::Unknown,
            None if made => Bucket::Library,
            None if !self.m.symbols.contains_key(head) && knowledge::TYPESCRIPT_GLOBALS.contains(&head) => Bucket::Builtin,
            None => Bucket::Unknown,
        }
    }

    /// The names the body binds: its parameters, each variable it declares, the parameters of
    /// the functions in it, a caught error, a loop's variable, and the functions it declares.
    fn bound_in(&self) -> HashSet<String> {
        fn pattern(p: TsNode, src: &[u8], out: &mut HashSet<String>) {
            match p.kind() {
                "identifier" | "shorthand_property_identifier_pattern" => {
                    out.insert(text(p, src).to_string());
                }
                // a default's value binds nothing
                "assignment_pattern" | "object_assignment_pattern" => {
                    p.child_by_field_name("left").into_iter().for_each(|l| pattern(l, src, out))
                }
                "pair_pattern" => p.child_by_field_name("value").into_iter().for_each(|v| pattern(v, src, out)),
                "object_pattern" | "array_pattern" | "rest_pattern" => named_children(p).into_iter().for_each(|c| pattern(c, src, out)),
                _ => {}
            }
        }
        let fields: &[(&str, &str)] = &[
            ("variable_declarator", "name"),
            ("required_parameter", "pattern"),
            ("optional_parameter", "pattern"),
            ("arrow_function", "parameter"),
            ("catch_clause", "parameter"),
            ("for_in_statement", "left"),
            ("function_declaration", "name"),
            ("generator_function_declaration", "name"),
        ];
        let kinds: Vec<&str> = fields.iter().map(|(k, _)| *k).collect();
        let mut out = HashSet::new();
        for n in walk(self.scope.unwrap_or(self.body), &kinds).into_iter().filter(|n| self.mine(*n)) {
            let field = fields.iter().find(|(k, _)| *k == n.kind()).map_or("", |(_, f)| *f);
            if let Some(p) = n.child_by_field_name(field) {
                pattern(p, self.src, &mut out);
            }
        }
        out
    }

    /// What a library's call crosses into: a file call, a request or a database statement waits
    /// for the path, URL or statement it is handed, placed once every call is known; any other
    /// touches what its facts name.
    fn crossed(&mut self, c: TsNode<'t>, parts: &[String]) {
        let (m, src) = (self.m, self.src);
        let (head, parts) = match self.held_field(parts) {
            Some(f) => (f, &parts[1..]),
            None => (parts[0].clone(), parts),
        };
        let pkg = match m.bindings.get(&head) {
            Some(Bound::Ext(e)) => Some(package_from_ext(e, &m.ext_pkgs)),
            _ => self.made.get(&head).cloned(),
        };
        // some parts of the world are reached only through a client the package made, not its helpers
        let Some(kind) = effects::boundary_ts(pkg.as_deref(), parts)
            .filter(|k| !knowledge::THROUGH_A_CLIENT.contains(&k.world) || self.made.contains_key(&head))
        else {
            return;
        };
        let op = parts.last().cloned().unwrap_or_default();
        let cr = Crosses::read(c, src, kind, pkg.as_deref(), &op, &literals(c, src, STRINGS), STRINGS, &HashMap::new());
        let args = c.child_by_field_name("arguments").map(named_children).unwrap_or_default();
        let at = At { m, scope: self.scope, params: &self.params };
        // a statement is read where it is whole: from a constant, a template, or what the callers
        // hand the parameters it is built from
        let statement = match (cr.facts.kind, args.first()) {
            (World::Db(db), Some(a)) => Some(self.eval.of(at, *a, 0)).filter(|b| crate::paths::is_statement(db, b)),
            _ => None,
        };
        if let Some(built) = statement {
            let t = Touch {
                what: cr.what.clone(),
                payload: cr.payload.clone(),
                op: op.clone(),
                line: cr.line,
                ..Touch::new(cr.facts.kind, None)
            };
            self.read.calls.push((t, built));
            return;
        }
        if !cr.facts.kind.located() || args.is_empty() {
            self.read.touches.extend(cr.touches(None));
            return;
        }
        for (kind, a) in sides(&op, &args, &cr.facts) {
            let t = Touch {
                what: text(a, src).to_string(),
                payload: cr.payload.clone(),
                op: op.clone(),
                artifact: cr.facts.artifact.clone(),
                fixed: cr.fixed,
                line: cr.line,
                ..Touch::new(kind.world, kind.dir)
            };
            let built = self.eval.of(at, a, 0);
            // a request is its method and its URL: `fetch(url, { method })`, `axios.post(url)`
            let built = if kind.world == World::Net {
                crate::paths::cross(&method_of_request(&op, &args, at, self.eval), &built, " ")
            } else {
                built
            };
            // a value the code holds none of (an option without a default) is the call as read
            if built.is_empty() {
                self.read.touches.push(t);
            } else {
                self.read.calls.push((t, built));
            }
        }
    }

    /// The names whose values the body changes in place, followed through the names they are
    /// changed by (`changed_through`). A call changes the value it is made on by what that value
    /// is: a method of the program's own when its body changes its own receiver, a method of the
    /// language's table on a value that holds an array, a `Map` or a `Set`. A method's name alone
    /// changes nothing (`http.delete(..)` on a library's client).
    fn changed(&self) -> std::collections::BTreeSet<&'t str> {
        let changes = |c: TsNode<'t>| -> Option<TsNode<'t>> {
            let f = function_of(c).filter(|f| f.kind() == "member_expression")?;
            let on = f.child_by_field_name("object")?;
            let changing = match self.callee(c).filter(|t| self.own(t)) {
                Some(t) => self.store.mutating.contains(&t),
                None => {
                    f.child_by_field_name("property")
                        .map_or(false, |p| knowledge::mutates(knowledge::Registry::Npm, "javascript", text(p, self.src)))
                        && self.holds_container(on)
                }
            };
            changing.then_some(on)
        };
        changes_in(self.body, self.src, &changes)
            .into_iter()
            .filter_map(|(x, root)| changed_through(self.body, self.src, &BINDS, root, x.start_byte()))
            .collect()
    }

    /// What the body does besides its calls: the connections the platform's own constructors
    /// open (`new WebSocket(url)`), the page a component renders, and the inputs it changes in
    /// place (`T -> T` for the receiver or a parameter; an initializer builds its object).
    fn ends(&mut self) {
        let src = self.src;
        let opened: Vec<Touch> = self
            .sites
            .iter()
            .filter(|c| c.kind() == "new_expression")
            .filter(|c| {
                self.resolve(&c.child_by_field_name("constructor").map(|x| member_parts(x, src)).unwrap_or_default())
                    .filter(|r| self.own(r))
                    .is_none()
            })
            .filter_map(|c| connects(*c, src, &self.m.bindings))
            .collect();
        self.read.touches.extend(opened);
        if self.is_module {
            return;
        }
        if let Some(jsx) = walk(self.body, &["jsx_element", "jsx_self_closing_element", "jsx_fragment"]).into_iter().next() {
            self.read.touches.push(Touch {
                op: "render".into(),
                line: jsx.start_position().row as u32 + 1,
                ..Touch::new(World::Screen, None)
            });
        }
        for r in self.changed() {
            let ty = if r == "this" { self.cls.clone().filter(|_| !self.initializer) } else { self.inputs.get(r).cloned() };
            if let Some(t) = ty {
                self.read.mutates.push((t, r.to_string()));
            }
        }
    }
}

/// The function a call calls. The grammar reads an awaited generic call, `await f<T>(..)`, as a
/// call of `await f`: the function is `f`.
fn function_of(c: TsNode) -> Option<TsNode> {
    let f = c.child_by_field_name("function")?;
    match f.kind() {
        "await_expression" => named_children(f).into_iter().next(),
        _ => Some(f),
    }
}

/// Whether a callee is read where it stands rather than called: a function written where it is
/// called (`(() => ..)()`), whose steps are the body's own, and `import(..)`, which binds a module.
fn read_in_place(f: TsNode) -> bool {
    matches!(beneath(f, WRAPPERS).kind(), "arrow_function" | "function_expression" | "function" | "import")
}

/// The name of a call no name path reaches, as the code writes it: a method on a value written out
/// is its name alone (`(..).trim`), any other callee its text (`EXPORTERS[format]`).
fn written_callee(f: TsNode, src: &[u8]) -> String {
    match f.child_by_field_name("property").filter(|_| f.kind() == "member_expression") {
        Some(p) => text(p, src).to_string(),
        None => text(f, src).split_whitespace().collect::<Vec<_>>().join(" "),
    }
}

/// Whether what a call gives back is used: anything but a call standing as a statement.
fn value_used(call: TsNode) -> bool {
    outermost(call, WRAPPERS).parent().map_or(false, |p| p.kind() != "expression_statement")
}

/// The HTTP method of a request: `fetch`'s `init.method` (GET when it has none), or the verb
/// of the call.
fn method_of_request<'t>(op: &str, args: &[TsNode<'t>], at: At<'_, 't>, eval: &Eval) -> Vec<crate::paths::Built> {
    if knowledge::HTTP_METHODS.contains(&op) {
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

/// The name a chain like `this.items[0].x` starts from.
fn chain_root<'a>(n: TsNode, src: &'a [u8]) -> Option<&'a str> {
    match n.kind() {
        "identifier" | "this" => Some(text(n, src)),
        "member_expression" | "subscript_expression" => chain_root(n.child_by_field_name("object")?, src),
        k if WRAPPERS.contains(&k) => chain_root(wrapped(n)?, src),
        _ => None,
    }
}

/// How TypeScript binds a name, as `changes` follows a change through it: `const x = v` and `x = v`
/// bind `x` to `v`, `for (const x of v)` to each element of `v`.
const BINDS: BindSyntax = BindSyntax {
    binds: |n, src| {
        let (l, v) = match n.kind() {
            "variable_declarator" => (n.child_by_field_name("name")?, n.child_by_field_name("value")?),
            "assignment_expression" => (n.child_by_field_name("left")?, n.child_by_field_name("right")?),
            _ => return None,
        };
        (l.kind() == "identifier").then(|| (text(l, src), held_by(v, src)))
    },
    loops: |n, src| {
        let of = n.child_by_field_name("operator").map_or(false, |o| o.kind() == "of");
        let (l, v, b) = (n.child_by_field_name("left")?, n.child_by_field_name("right")?, n.child_by_field_name("body")?);
        (n.kind() == "for_in_statement" && of && l.kind() == "identifier").then(|| (text(l, src), chain_root(v, src), b))
    },
};

/// The names whose value a TypeScript value may be or be a part of: `graph`, `this.items[0]`,
/// either side of `a ?? b`, `a || b` and `c ? a : b`. A call, a spread (`{ ...e }`) and a literal
/// make a value anew.
fn held_by<'a>(v: TsNode, src: &'a [u8]) -> Vec<&'a str> {
    match v.kind() {
        "binary_expression" if v.child_by_field_name("operator").map_or(false, |o| matches!(o.kind(), "??" | "||" | "&&")) => {
            [v.child_by_field_name("left"), v.child_by_field_name("right")].into_iter().flatten().flat_map(|x| held_by(x, src)).collect()
        }
        "ternary_expression" => [v.child_by_field_name("consequence"), v.child_by_field_name("alternative")]
            .into_iter()
            .flatten()
            .flat_map(|x| held_by(x, src))
            .collect(),
        k if WRAPPERS.contains(&k) => wrapped(v).into_iter().flat_map(|x| held_by(x, src)).collect(),
        _ => chain_root(v, src).into_iter().collect(),
    }
}

/// Each change a body makes in place: the node that makes it, and the name whose object it
/// changes, the root of every assignment to a member or an item (`this.x = ..`, `p.items[k] +=
/// ..`, `p.n++`, `delete p.k`), and of the value every call that changes something is made on, as
/// `changes` says (`p.rows.push(..)`). Changing a part changes the whole.
fn changes_in<'a, 't>(body: TsNode<'t>, src: &'a [u8], changes: &dyn Fn(TsNode<'t>) -> Option<TsNode<'t>>) -> Vec<(TsNode<'t>, &'a str)> {
    let member = |n: TsNode| matches!(n.kind(), "member_expression" | "subscript_expression");
    let mut out = vec![];
    for a in walk(body, &["assignment_expression", "augmented_assignment_expression"]) {
        out.extend(a.child_by_field_name("left").filter(|l| member(*l)).and_then(|l| chain_root(l, src)).map(|r| (a, r)));
    }
    for u in walk(body, &["update_expression", "unary_expression"]) {
        let deletes = u.kind() == "update_expression" || (0..u.child_count()).filter_map(|i| u.child(i)).any(|c| c.kind() == "delete");
        if deletes {
            out.extend(u.child_by_field_name("argument").filter(|a| member(*a)).and_then(|a| chain_root(a, src)).map(|r| (u, r)));
        }
    }
    for c in walk(body, &["call_expression"]) {
        out.extend(changes(c).and_then(|o| chain_root(o, src)).map(|r| (c, r)));
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
    Some(Touch {
        what: what_of(c, src, kinds, "arguments"),
        op: name.clone(),
        artifact: f.artifact,
        line: c.start_position().row as u32 + 1,
        ..Touch::new(f.kind, f.dir)
    })
}

/// The paths a file call is handed, each with the way it goes: a copy or a move reads its first and
/// writes its second; any other call goes its own way with its first.
fn sides<'t>(op: &str, args: &[TsNode<'t>], f: &effects::Facts) -> Vec<(Crossing, TsNode<'t>)> {
    match crate::body::sides(op, args.len()) {
        Some([from, to]) => vec![(from, args[0]), (to, args[1])],
        None => vec![(Crossing { world: f.kind, dir: if f.kind == World::Disk { f.dir } else { None } }, args[0])],
    }
}

/// Which parameter of the function a call reaches each argument is handed to, in order.
#[allow(clippy::too_many_arguments)]
fn argument<'t>(
    modules: &'t IndexMap<String, Module>,
    store: &GraphBuilder,
    caller: &str,
    callee: &str,
    c: TsNode<'t>,
    m: &'t Module,
    scope: Option<TsNode<'t>>,
    params: &[String],
) -> Option<Arg<'t>> {
    let cn = store.nodes.get(callee)?;
    let cm = modules.get(&cn.module)?;
    let (s, e) = *cm.bodies.get(callee)?;
    let at = cm.tree.root_node().descendant_for_byte_range(s, e)?;
    // a method an interface or an abstract class only declares is its signature
    let def = if matches!(at.kind(), "method_signature" | "abstract_method_signature") { at } else { paths::function_node(at)? };
    let names = paths::param_names(def, &cm.src);
    let passed = named_children(c.child_by_field_name("arguments")?)
        .into_iter()
        .take_while(|a| a.kind() != "spread_element")
        .zip(names)
        .filter(|(_, n)| !n.is_empty())
        .map(|(a, n)| (n, a))
        .collect();
    Some(Arg {
        caller: caller.to_string(),
        callee: callee.to_string(),
        module: m.id.clone(),
        scope,
        params: params.to_vec(),
        passed,
        on: None,
    })
}

/// The package an `ext:` binding came from, as the module imported it.
fn package_from_ext(id: &str, pkgs: &HashSet<String>) -> String {
    let path = id.trim_start_matches("ext:").replace('.', "/");
    pkgs.iter().filter(|p| path.starts_with(p.as_str())).max_by_key(|p| p.len()).cloned().unwrap_or_else(|| package_of(&path))
}

// ---------- choices ----------

/// TypeScript's choices: `if`/`else if`/`else`, `a ? b : c`, `switch`, `try`/`catch`,
/// `x ?? y` and `x || y`.
/// Where TypeScript's values go: `const x = v`, `x = v`, `x += v`, `for (const x of v)`; `return`
/// and `yield` leave; `throw` fails.
const USES: UseSyntax = UseSyntax {
    binds: |n| match n.kind() {
        "variable_declarator" => n.child_by_field_name("name").zip(n.child_by_field_name("value")),
        "assignment_expression" | "augmented_assignment_expression" | "for_in_statement" => {
            n.child_by_field_name("left").zip(n.child_by_field_name("right"))
        }
        _ => None,
    },
    leaves: |n, _| matches!(n.kind(), "return_statement" | "yield_expression"),
    fails: |n, _| n.kind() == "throw_statement",
    hands: |n, child| {
        matches!(n.kind(), "call_expression" | "new_expression") && n.child_by_field_name("arguments").map(|a| a.id()) == Some(child.id())
    },
    splits: |n, child| n.kind() == "switch_statement" && n.child_by_field_name("value").map(|v| v.id()) == Some(child.id()),
    names: &["identifier", "shorthand_property_identifier"],
    keeps: &["member_expression", "subscript_expression"],
};

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
            while let Some(up) =
                cur.parent().filter(|p| p.kind() == "else_clause").and_then(|e| e.parent()).filter(|p| p.kind() == "if_statement")
            {
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
            "switch_body" if child.kind() == "switch_default" => {
                Part::Arm(named_children(c).iter().filter(|x| x.kind() == "switch_case").count() as u32)
            }
            "try_statement" if is("body") => Part::Arm(0),
            "try_statement" if is("handler") => Part::Arm(1),
            "binary_expression" if is("left") => Part::Arm(0),
            "binary_expression" if is("right") => Part::Arm(1),
            _ => Part::Through,
        }
    },
};

/// TypeScript's comparisons: `===`, `==`, `!==` and `!=` against a string or a member of an
/// enumeration (`Mode.Fast`), and a `switch` over them.
const COMPARES: CompareSyntax = CompareSyntax {
    kinds: &["binary_expression", "switch_statement"],
    operands: tested,
    each: |n| vec![n],
    literal: |n, src| (n.kind() == "string").then(|| unquote(text(n, src))),
    member: |n| {
        (n.kind() == "member_expression")
            .then(|| n.child_by_field_name("object"))
            .flatten()
            .filter(|o| matches!(o.kind(), "identifier" | "member_expression"))
            .map(|o| (o.start_byte(), o.end_byte()))
    },
    subject: |n| matches!(n.kind(), "identifier" | "member_expression"),
    over: switched,
    copies: |n| match n.kind() {
        "variable_declarator" => n.child_by_field_name("name").zip(n.child_by_field_name("value")),
        "assignment_expression" => n.child_by_field_name("left").zip(n.child_by_field_name("right")),
        _ => None,
    },
    place: |n| matches!(n.kind(), "identifier" | "this" | "member_expression" | "subscript_expression").then_some(n),
    binds: USES.binds,
};

/// The two operands of a test of equality.
fn tested<'t>(n: TsNode<'t>, src: &[u8]) -> Option<(TsNode<'t>, TsNode<'t>)> {
    let op = n.child_by_field_name("operator").map(|o| text(o, src)).unwrap_or("");
    matches!(op, "===" | "==" | "!==" | "!=").then_some((n.child_by_field_name("left")?, n.child_by_field_name("right")?))
}

/// A `switch`: its value, the values of its cases, and the body that holds them.
fn switched<'t>(n: TsNode<'t>, src: &[u8]) -> Option<(String, Vec<TsNode<'t>>, Option<TsNode<'t>>)> {
    let (Some(value), Some(body)) = (n.child_by_field_name("value"), n.child_by_field_name("body")) else { return None };
    let cases = named_children(body).into_iter().filter_map(|c| c.child_by_field_name("value")).collect();
    Some((text(value, src).trim_start_matches('(').trim_end_matches(')').to_string(), cases, Some(body)))
}
