//! A TypeScript file as the reader holds it, and the vocabulary every pass reads it with: what its
//! names are bound to, its declarations and their docs, the types and signatures they write, the
//! members a path names, and where a name resolves to.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use indexmap::IndexMap;
use tree_sitter::{Node as TsNode, Parser, Tree};

use crate::builder::SigInfo;
use crate::knowledge;
use crate::model::{Node, Param, Returns};
use crate::tree::{named_children, text, walk, Wrappers};

pub(crate) const EXTENSIONS: &[&str] = &["ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs"];
/// What hands on the value inside it: parentheses, `await`, the non-null assertion `!`, and the
/// casts `as` and `satisfies`.
pub(super) const WRAPPERS: Wrappers =
    &["parenthesized_expression", "await_expression", "non_null_expression", "as_expression", "satisfies_expression"];
#[derive(Clone, Debug)]
pub(super) enum Bound {
    /// A node id, a member of an own module (`module.name`), or an own module.
    Own(String),
    /// A name from a package: `ext:pkg.name`.
    Ext(String),
}

pub(super) struct Module {
    pub(super) id: String,
    pub(super) path: PathBuf,
    pub(super) src: Vec<u8>,
    pub(super) tree: Tree,
    pub(super) bindings: HashMap<String, Bound>,
    pub(super) symbols: HashMap<String, String>,
    pub(super) default: Option<Bound>,
    pub(super) wildcards: Vec<String>,
    /// Type aliases: name -> (the names they stand for, whether they are a union).
    pub(super) aliases: HashMap<String, (Vec<Vec<String>>, bool)>,
    /// Function node id -> the byte range of its body (a block, an arrow, an initializer).
    pub(super) bodies: HashMap<String, (usize, usize)>,
    /// The package names the module imports from.
    pub(super) ext_pkgs: HashSet<String>,
    /// Module-level values by the type they hold: `const provider: ProviderLike = ..`,
    /// `const api = new Api()`.
    pub(super) values: HashMap<String, Vec<String>>,
    /// Names a body imports when it runs: `const { ApiProvider } = await import("./api-provider")`.
    pub(super) dynamic: HashMap<String, Bound>,
    /// The names the module exports: `export function`, `export { a as b }`, `export * from`.
    pub(super) exported: Vec<String>,
    /// Module-level values that are a web framework's router: `const app = new Hono()`.
    pub(super) routers: HashSet<String>,
    /// Route handlers by the router they are registered on: (handler id, router name).
    pub(super) routes: Vec<(String, String)>,
    /// Routers mounted on another under a prefix: (router, prefix, mounted router).
    pub(super) mounts: Vec<(String, String, String)>,
    /// The names imported from a file not the package's own (a package's, or one of a library's
    /// files): each by its local name, the specifier and its name there (`default` for a default
    /// import, none for the module itself).
    pub(super) imported: Vec<Imported>,
    /// The names bound to a library's code Atlas reads, by local name: the file each comes from
    /// and its name there.
    pub(super) links: HashMap<String, (PathBuf, Option<String>)>,
    /// What those names stand for where the library's code is read as code: the definition, by
    /// the name as written (`readConfig`, or a namespace's member `fk.readConfig`).
    pub(super) read: HashMap<String, String>,
    /// The values a package made that every body of the module sees, by name: the module's own,
    /// and each class's fields by where the class starts. Found once, when a body first asks.
    pub(super) made: std::cell::RefCell<HashMap<Option<usize>, HashMap<String, String>>>,
    /// The fields of each of the module's classes and shapes that hold one of the language's own
    /// containers, by the class: `rows: Row[]`, `seen = new Set<string>()`.
    pub(super) containers: HashMap<String, HashSet<String>>,
}

/// A name imported from a file not the package's own, as written.
#[derive(Clone, Debug)]
pub(super) struct Imported {
    pub(super) local: String,
    pub(super) spec: String,
    pub(super) name: Option<String>,
}

impl Module {
    /// A module of a file read from `src`, before its imports and symbols are collected.
    pub(super) fn new(id: String, path: PathBuf, src: Vec<u8>, tree: Tree) -> Module {
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

pub(super) fn ts_parser(tsx: bool) -> Result<Parser> {
    let mut p = Parser::new();
    let lang: tree_sitter::Language =
        if tsx { tree_sitter_typescript::LANGUAGE_TSX.into() } else { tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into() };
    p.set_language(&lang).context("loading the TypeScript grammar")?;
    Ok(p)
}

/// A path segment as part of a dotted id: dots inside a file name (`punt.functions`) are not
/// module boundaries.
pub(super) fn segment(s: &str) -> String {
    s.replace('.', "_")
}

pub(super) fn is_code(p: &Path) -> bool {
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
    EXTENSIONS.contains(&ext)
        && !name.ends_with(".d.ts")
        && !name.contains(".gen.")
        && !name.contains(".min.")
        && !name.contains(".config.")
}

/// `src/lib/map/layers.ts` -> `app.lib.map.layers`; `src/lib/index.ts` -> `app.lib`.
pub(super) fn module_id(root: &str, base: &Path, path: &Path) -> String {
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

/// The package an import comes from: `@tanstack/react-router/x` -> `@tanstack/react-router`,
/// `node:fs/promises` -> `fs`.
pub(super) fn package_of(spec: &str) -> String {
    let spec = spec.strip_prefix("node:").unwrap_or(spec);
    let mut parts = spec.split('/');
    match (spec.starts_with('@'), parts.next(), parts.next()) {
        (true, Some(scope), Some(name)) => format!("{scope}/{name}"),
        (_, Some(first), _) => first.to_string(),
        _ => spec.to_string(),
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
pub(super) fn doc_of(n: TsNode, src: &[u8]) -> String {
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
pub(super) fn prophecy_of(n: TsNode, src: &[u8]) -> Option<crate::model::Prophecy> {
    leading_comment(n, src).and_then(crate::model::Prophecy::of_jsdoc)
}

/// The type names an annotation mentions as values: `Promise<Regel[]>` -> Regel,
/// `Treffer | Mis` -> both, `Laag<Bron>` -> Laag (a generic class's arguments are types).
pub(super) fn type_names(t: Option<TsNode>, src: &[u8]) -> Vec<Vec<String>> {
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
pub(super) fn signature(params: Option<TsNode>, ret: Option<TsNode>, src: &[u8]) -> (Vec<Param>, Option<Returns>, SigInfo) {
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
pub(super) fn returns(rt: TsNode, src: &[u8]) -> Returns {
    let t = promised(rt, src);
    let (value, own) = match (t.kind(), text(t, src)) {
        ("predefined_type", "void" | "undefined") => (String::new(), false),
        ("this_type", _) => (String::new(), true),
        (_, v) => (v.to_string(), false),
    };
    Returns { written: text(rt, src).to_string(), value, own, fails: false, unchecked: unchecked(t, src) }
}

/// The type a return type gives once it arrives: `T` of `Promise<T>`.
pub(super) fn promised<'t>(rt: TsNode<'t>, src: &[u8]) -> TsNode<'t> {
    let mut t = rt;
    while t.kind() == "generic_type" && t.child_by_field_name("name").map_or(false, |n| text(n, src) == "Promise") {
        match t.child_by_field_name("type_arguments").and_then(|a| named_children(a).into_iter().next()) {
            Some(i) => t = i,
            None => break,
        }
    }
    t
}

/// The top-level statements of a module with the declaration each holds, unwrapping `export`.
pub(super) fn declarations<'t>(root: TsNode<'t>) -> Vec<(TsNode<'t>, bool, bool)> {
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
pub(super) fn top_variables<'t>(root: TsNode<'t>) -> Vec<TsNode<'t>> {
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
pub(super) fn function_in<'t>(value: TsNode<'t>) -> Option<TsNode<'t>> {
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

/// The name a type annotation starts with: `ProviderLike`, `Map<string, Laag>` -> `Map`.
pub(super) fn type_head(t: TsNode, src: &[u8]) -> Vec<String> {
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

/// A name in a module's scope -> a node id or `ext:` id, following imports and re-exports.
pub(super) fn resolve(m: &Module, parts: &[String], modules: &IndexMap<String, Module>, nodes: &IndexMap<String, Node>) -> Option<String> {
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
pub(super) fn defined(target: &str, modules: &IndexMap<String, Module>, nodes: &IndexMap<String, Node>, depth: u32) -> Option<String> {
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

pub(super) fn member_parts(n: TsNode, src: &[u8]) -> Vec<String> {
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

/// The function a call calls. The grammar reads an awaited generic call, `await f<T>(..)`, as a
/// call of `await f`: the function is `f`.
pub(super) fn function_of(c: TsNode) -> Option<TsNode> {
    let f = c.child_by_field_name("function")?;
    match f.kind() {
        "await_expression" => named_children(f).into_iter().next(),
        _ => Some(f),
    }
}

/// The package an `ext:` binding came from, as the module imported it.
pub(super) fn package_from_ext(id: &str, pkgs: &HashSet<String>) -> String {
    let path = id.trim_start_matches("ext:").replace('.', "/");
    pkgs.iter().filter(|p| path.starts_with(p.as_str())).max_by_key(|p| p.len()).cloned().unwrap_or_else(|| package_of(&path))
}
