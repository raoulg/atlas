//! Where a file call's file is: its path evaluated from the code, through the definitions ty
//! finds, and through the values a function's parameters are handed by its callers.
//!
//! A path is built from literal text, the parameters of the function it is built in, and what
//! cannot be known (a hole, named). A parameter holds what its callers pass, over all its call
//! sites, and its default where a call leaves it out; an entry's parameters hold their declared
//! defaults (`@click.option(.., default=..)`). The place of a path with an unknown part is the
//! directory before that part, so what one route unpacks into `D/{kind}/{id}` and another lists
//! in `D` meet at `D/`.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use indexmap::IndexMap;
use tree_sitter::Node as TsNode;

use super::{call_path, dotted, Module};
use crate::pytypes::{FileArg, Param, Target, Types};
use crate::tree::{named_children, path_str, text, unquote, walk};

/// A part of a path as the code builds it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) enum Seg {
    Lit(String),
    /// A parameter of the function the path is built in.
    Param(String),
    /// What cannot be known, by the name the code gives it.
    Hole(String),
}

pub(super) type Built = Vec<Seg>;

/// The most values a path or a parameter keeps.
const MAX: usize = 4;
const DEPTH: u32 = 12;

/// Constructors of a path from its parts.
const PATH_CTORS: &[&str] = &["Path", "PurePath", "PosixPath", "WindowsPath", "PurePosixPath", "PureWindowsPath"];
/// Methods that give back the path they are called on, written another way.
const SAME_PATH: &[&str] = &["resolve", "absolute", "expanduser", "as_posix", "__fspath__"];
/// Operations whose path is a directory: what they list, unpack into or make.
const DIR_OPS: &[&str] = &["iterdir", "glob", "rglob", "listdir", "scandir", "walk", "extractall", "mkdir", "makedirs", "rmtree", "copytree"];

fn lit(s: &str) -> Built {
    vec![Seg::Lit(s.to_string())]
}

fn hole(s: &str) -> Built {
    vec![Seg::Hole(s.to_string())]
}

/// `a` then `b`, with `sep` between them; adjacent literal text merges.
fn join(a: &Built, b: &Built, sep: &str) -> Built {
    let mut out = a.clone();
    let rest: Vec<Seg> = if sep.is_empty() { b.clone() } else { std::iter::once(Seg::Lit(sep.into())).chain(b.iter().cloned()).collect() };
    for s in rest {
        match (out.last_mut(), &s) {
            (Some(Seg::Lit(x)), Seg::Lit(y)) => x.push_str(y),
            _ => out.push(s),
        }
    }
    out
}

fn cross(xs: &[Built], ys: &[Built], sep: &str) -> Vec<Built> {
    let mut out = vec![];
    for x in xs {
        for y in ys {
            let b = join(x, y, sep);
            if !out.contains(&b) {
                out.push(b);
            }
        }
    }
    out.truncate(MAX);
    out
}

fn union(mut xs: Vec<Built>, ys: Vec<Built>) -> Vec<Built> {
    for y in ys {
        if !xs.contains(&y) {
            xs.push(y);
        }
    }
    xs.truncate(MAX);
    xs
}

/// The directory a path is in: up to its last `/`.
fn parent(b: &Built) -> Built {
    for i in (0..b.len()).rev() {
        if let Seg::Lit(s) = &b[i] {
            let t = s.trim_end_matches('/');
            if let Some(k) = t.rfind('/') {
                let mut out = b[..i].to_vec();
                out.push(Seg::Lit(t[..k.max(1)].to_string()));
                return out;
            }
        }
    }
    match b.as_slice() {
        [Seg::Lit(_)] => lit("."),
        _ => hole("parent"),
    }
}

/// A path with its unknown parts written as `{name}`.
fn render(b: &Built) -> String {
    b.iter()
        .map(|s| match s {
            Seg::Lit(x) => x.clone(),
            Seg::Param(x) | Seg::Hole(x) => format!("{{{x}}}"),
        })
        .collect()
}

/// The place a path names: the file, or for an operation on a directory that directory. A file
/// whose name is known under a directory that is not (`artefacts/{variant}/report.json`) is
/// that name under any such directory (`artefacts/*/report.json`); a path whose last part is
/// not known rests in the directory before its first unknown part, so what is unpacked into
/// `D/{kind}/{id}` and what lists `D` meet at `D/`. A path that names nothing in the repository
/// (only `.`, or nothing before an unknown) is no place.
pub(super) fn place(b: &Built, op: &str) -> Option<String> {
    let mut p = render(b).replace('\\', "/");
    while p.contains("//") {
        p = p.replace("//", "/");
    }
    p = p.replace("/./", "/");
    while let Some(r) = p.strip_prefix("./") {
        p = r.to_string();
    }
    let dir = DIR_OPS.contains(&op);
    let unknown = |s: &str| s.contains(['{', '*', '?', '[']);
    let p = p.trim_end_matches('/').to_string();
    let parts: Vec<&str> = p.split('/').collect();
    let p = match parts.iter().position(|x| unknown(x)) {
        None if dir => format!("{p}/"),
        None => p.clone(),
        Some(i) if !dir && i + 1 < parts.len() && !unknown(parts[parts.len() - 1]) && i > 0 => {
            parts.iter().map(|x| if unknown(x) { "*" } else { x }).collect::<Vec<_>>().join("/")
        }
        Some(0) => return None,
        Some(i) => format!("{}/", parts[..i].join("/")),
    };
    (!matches!(p.as_str(), "" | "." | "./" | "/" | "*")).then_some(p)
}

/// Evaluates paths in the modules of one project.
pub(super) struct Eval<'a> {
    by_file: HashMap<String, &'a Module>,
    types: &'a Types,
    root: &'a Path,
}

/// Where an expression is evaluated: its module, and the function it is in with that function's
/// parameters.
#[derive(Clone, Copy)]
pub(super) struct At<'a, 't> {
    pub m: &'a Module,
    pub scope: Option<TsNode<'t>>,
    pub params: &'a [String],
}

impl<'a> Eval<'a> {
    pub fn new(modules: &'a IndexMap<String, Module>, types: &'a Types, root: &'a Path) -> Self {
        Eval { by_file: modules.values().map(|m| (path_str(&m.path), m)).collect(), types, root }
    }

    fn def(&self, m: &Module, e: TsNode) -> Option<Target> {
        self.types.of(&path_str(&m.path))?.def((e.start_byte(), e.end_byte())).cloned()
    }

    fn signature<'s>(&'s self, m: &Module, f: TsNode) -> Option<&'s [Param]> {
        self.types.of(&path_str(&m.path))?.signature((f.start_byte(), f.end_byte()))
    }

    /// Whether calling `f` reaches a method of `pathlib`.
    fn of_pathlib(&self, m: &Module, f: TsNode) -> bool {
        self.types.of(&path_str(&m.path)).map_or(false, |x| x.callees((f.start_byte(), f.end_byte())).iter().any(|c| matches!(&c.target, Target::Lib(m) if m == "pathlib")))
    }

    fn callee_def(&self, m: &Module, f: TsNode) -> Option<(String, usize)> {
        let facts = self.types.of(&path_str(&m.path))?;
        facts.callees((f.start_byte(), f.end_byte())).iter().find_map(|c| match &c.target {
            Target::Here { file, at } => Some((file.clone(), *at)),
            Target::Lib(_) => None,
        })
    }

    /// The values `e` can have.
    pub fn of(&self, at: At, e: TsNode, depth: u32) -> Vec<Built> {
        if depth > DEPTH {
            return vec![hole("…")];
        }
        let src = &at.m.src;
        let named = || hole(dotted(Some(e), src).last().map(String::as_str).unwrap_or("value"));
        match e.kind() {
            "string" => vec![self.string(at, e, depth)],
            "concatenated_string" => named_children(e).into_iter().fold(vec![vec![]], |acc, s| cross(&acc, &[self.string(at, s, depth)], "")),
            "parenthesized_expression" => named_children(e).first().map(|x| self.of(at, *x, depth + 1)).unwrap_or_default(),
            "conditional_expression" => {
                let k = named_children(e);
                union(k.first().map(|x| self.of(at, *x, depth + 1)).unwrap_or_default(), k.get(2).map(|x| self.of(at, *x, depth + 1)).unwrap_or_default())
            }
            "boolean_operator" => named_children(e).into_iter().fold(vec![], |acc, x| union(acc, self.of(at, x, depth + 1))),
            "binary_operator" => {
                let (Some(l), Some(r)) = (e.child_by_field_name("left"), e.child_by_field_name("right")) else { return vec![named()] };
                let sep = match e.child_by_field_name("operator").map(|o| text(o, src)) {
                    Some("/") => "/",
                    Some("+") => "",
                    _ => return vec![named()],
                };
                cross(&self.of(at, l, depth + 1), &self.of(at, r, depth + 1), sep)
            }
            "identifier" => self.name(at, e, depth),
            "attribute" => {
                let (Some(obj), Some(attr)) = (e.child_by_field_name("object"), e.child_by_field_name("attribute")) else { return vec![named()] };
                match text(attr, src) {
                    "parent" => self.of(at, obj, depth + 1).iter().map(parent).collect(),
                    "stem" | "name" | "suffix" | "suffixes" | "parts" => vec![named()],
                    _ => self.defined(at, e, depth).unwrap_or_else(|| vec![named()]),
                }
            }
            "subscript" => {
                // `x.parents[n]`: the directory n + 1 levels up
                let v = e.child_by_field_name("value").filter(|v| v.kind() == "attribute");
                let parents = v.filter(|v| v.child_by_field_name("attribute").map(|a| text(a, src)) == Some("parents"));
                let n = e.child_by_field_name("subscript").and_then(|s| text(s, src).parse::<usize>().ok());
                match (parents.and_then(|v| v.child_by_field_name("object")), n) {
                    (Some(obj), Some(n)) => self.of(at, obj, depth + 1).iter().map(|b| (0..=n).fold(b.clone(), |b, _| parent(&b))).collect(),
                    _ => vec![named()],
                }
            }
            "call" => self.call(at, e, depth),
            "none" => vec![],
            _ => vec![hole("value")],
        }
    }

    /// A string literal, an f-string's holes evaluated.
    fn string(&self, at: At, s: TsNode, depth: u32) -> Built {
        let parts: Vec<TsNode> = named_children(s);
        if !parts.iter().any(|p| p.kind() == "interpolation") {
            return lit(&unquote(text(s, &at.m.src)));
        }
        let mut out: Vec<Built> = vec![vec![]];
        for p in parts {
            let piece = match p.kind() {
                "string_content" | "escape_sequence" => vec![lit(text(p, &at.m.src))],
                "interpolation" => named_children(p).first().map(|x| self.of(at, *x, depth + 1)).unwrap_or_else(|| vec![hole("value")]),
                _ => continue,
            };
            out = cross(&out, &piece, "");
        }
        out.into_iter().next().unwrap_or_default()
    }

    fn name(&self, at: At, e: TsNode, depth: u32) -> Vec<Built> {
        let name = text(e, &at.m.src);
        if name == "__file__" {
            let rel = at.m.path.strip_prefix(self.root).map(path_str).unwrap_or_else(|_| path_str(&at.m.path));
            return vec![lit(&rel)];
        }
        self.defined(at, e, depth).unwrap_or_else(|| vec![hole(name)])
    }

    /// The values of what a name or attribute is defined as.
    fn defined(&self, at: At, e: TsNode, depth: u32) -> Option<Vec<Built>> {
        let Target::Here { file, at: offset } = self.def(at.m, e)? else { return None };
        self.follow(at, &file, offset, depth)
    }

    /// What the definition whose name is written at `offset` in `file` holds: an assignment's
    /// value, a property's or function's `return`, a parameter of the function being evaluated.
    fn follow(&self, at: At, file: &str, offset: usize, depth: u32) -> Option<Vec<Built>> {
        let m = *self.by_file.get(file)?;
        let mut n = m.tree.root_node().descendant_for_byte_range(offset, offset)?;
        if n.kind() != "identifier" {
            n = n.parent().filter(|p| p.kind() == "identifier").unwrap_or(n);
        }
        let name = text(n, &m.src).to_string();
        let within = |s: Option<TsNode>| s.map_or(false, |s| std::ptr::eq(m, at.m) && s.start_byte() <= offset && offset < s.end_byte());
        let mut p = n.parent()?;
        if p.kind() == "attribute" {
            p = p.parent()?;
        }
        // `with open(p) as f`: `f` is what the context gives, the file at `p`
        if p.kind() == "as_pattern_target" {
            let value = p.parent().filter(|a| a.kind() == "as_pattern").and_then(|a| named_children(a).into_iter().next())?;
            let here = if within(at.scope) { At { m, ..at } } else { At { m, scope: None, params: &[] } };
            return Some(self.of(here, value, depth + 1));
        }
        match p.kind() {
            "assignment" => {
                let right = p.child_by_field_name("right")?;
                let here = if within(at.scope) { At { m, ..at } } else { At { m, scope: None, params: &[] } };
                Some(self.of(here, right, depth + 1))
            }
            "function_definition" => {
                let body = p.child_by_field_name("body")?;
                let returns: Vec<TsNode> = walk(body, &["return_statement"]).into_iter().filter(|r| enclosing_def(*r).map(|d| d.id()) == Some(p.id())).collect();
                let here = At { m, scope: Some(p), params: &[] };
                Some(returns.into_iter().filter_map(|r| named_children(r).into_iter().next()).fold(vec![], |acc, x| union(acc, self.of(here, x, depth + 1))))
            }
            "parameters" | "default_parameter" | "typed_parameter" | "typed_default_parameter" | "list_splat_pattern" | "dictionary_splat_pattern" => {
                Some(vec![if within(at.scope) && at.params.contains(&name) { vec![Seg::Param(name)] } else { hole(&name) }])
            }
            _ => None,
        }
    }

    fn call(&self, at: At, c: TsNode, depth: u32) -> Vec<Built> {
        let src = &at.m.src;
        let Some(f) = c.child_by_field_name("function") else { return vec![hole("call")] };
        let args: Vec<TsNode> = c.child_by_field_name("arguments").map(named_children).unwrap_or_default().into_iter().filter(|a| a.kind() != "comment").collect();
        let positional: Vec<TsNode> = args.iter().copied().filter(|a| !matches!(a.kind(), "keyword_argument" | "list_splat" | "dictionary_splat")).collect();
        let parts = call_path(f, src);
        let last = parts.last().map(String::as_str).or_else(|| f.child_by_field_name("attribute").map(|a| text(a, src))).unwrap_or("");
        let receiver = || f.child_by_field_name("object").map(|o| self.of(at, o, depth + 1)).unwrap_or_else(|| vec![hole("value")]);
        let joined = |xs: &[TsNode]| xs.iter().fold(vec![vec![]], |acc: Vec<Built>, a| if acc == vec![Vec::<Seg>::new()] { self.of(at, *a, depth + 1) } else { cross(&acc, &self.of(at, *a, depth + 1), "/") });
        if PATH_CTORS.contains(&last) {
            return if positional.is_empty() { vec![lit(".")] } else { joined(&positional) };
        }
        match last {
            "join" if parts.iter().any(|p| p == "path") => return joined(&positional),
            "joinpath" => return cross(&receiver(), &joined(&positional), "/"),
            "cwd" => return vec![lit(".")],
            "home" => return vec![lit("~")],
            "str" | "fspath" if positional.len() == 1 => return self.of(at, positional[0], depth + 1),
            "getenv" | "get" if parts.iter().any(|p| p == "getenv" || p == "environ") => {
                let var = positional.first().map(|a| unquote(text(*a, src))).unwrap_or_default();
                let default = positional.get(1).map(|a| self.of(at, *a, depth + 1)).unwrap_or_default();
                return union(vec![hole(&format!("${var}"))], default);
            }
            "with_suffix" => {
                let suffix = positional.first().map(|a| unquote(text(*a, src))).unwrap_or_default();
                return receiver().into_iter().map(|mut b| {
                    if let Some(Seg::Lit(s)) = b.last_mut() {
                        let stem_end = s.rfind('.').filter(|&i| i > s.rfind('/').map_or(0, |j| j + 1)).unwrap_or(s.len());
                        s.truncate(stem_end);
                        s.push_str(&suffix);
                    }
                    b
                }).collect();
            }
            "with_name" => return cross(&receiver().iter().map(parent).collect::<Vec<_>>(), &joined(&positional), "/"),
            m if SAME_PATH.contains(&m) => return receiver(),
            _ => {}
        }
        // a method of a path gives back something at that path: `p.open("w")`
        if f.kind() == "attribute" && self.of_pathlib(at.m, f) {
            return receiver();
        }
        // a library call handed a path gives back something at that path: `open(p)`, `ZipFile(p)`
        if let Some(sig) = self.signature(at.m, f) {
            if let Some((_, a)) = file_args(sig, c, src).into_iter().find(|(p, _)| p.file == Some(FileArg::Path)) {
                return self.of(at, a, depth + 1);
            }
        }
        // a function of the package: what it gives back
        if let Some((file, offset)) = self.callee_def(at.m, f) {
            if let Some(v) = self.follow(At { scope: None, params: &[], ..at }, &file, offset, depth).filter(|v| !v.is_empty()) {
                return v;
            }
        }
        vec![hole(last)]
    }
}

/// The arguments of a call matched to the parameters of its signature: positional ones in
/// order, keyword ones by name.
pub(super) fn file_args<'s, 't>(sig: &'s [Param], call: TsNode<'t>, src: &[u8]) -> Vec<(&'s Param, TsNode<'t>)> {
    let Some(args) = call.child_by_field_name("arguments") else { return vec![] };
    let mut out = vec![];
    let mut i = 0;
    for a in named_children(args).into_iter().filter(|a| a.kind() != "comment") {
        match a.kind() {
            "keyword_argument" => {
                let name = a.child_by_field_name("name").map(|n| text(n, src)).unwrap_or("");
                if let (Some(p), Some(v)) = (sig.iter().find(|p| p.name == name), a.child_by_field_name("value")) {
                    out.push((p, v));
                }
            }
            "list_splat" | "dictionary_splat" => break,
            _ => {
                if let Some(p) = sig.get(i) {
                    out.push((p, a));
                }
                i += 1;
            }
        }
    }
    out
}

/// The function a node sits in.
pub(super) fn enclosing_def(n: TsNode) -> Option<TsNode> {
    let mut p = n.parent();
    while let Some(x) = p {
        if x.kind() == "function_definition" {
            return Some(x);
        }
        p = x.parent();
    }
    None
}

/// A function's parameters in order, with their defaults: `self` and `cls` are left out of a
/// method's, so the first is the first a caller passes.
pub(super) fn parameters<'t>(def: TsNode<'t>, src: &[u8], method: bool) -> Vec<(String, Option<TsNode<'t>>)> {
    let Some(ps) = def.child_by_field_name("parameters") else { return vec![] };
    let mut out = vec![];
    for p in named_children(ps) {
        let (name, default) = match p.kind() {
            "identifier" => (Some(p), None),
            "typed_parameter" => (named_children(p).into_iter().find(|c| c.kind() == "identifier"), None),
            "default_parameter" | "typed_default_parameter" => (p.child_by_field_name("name"), p.child_by_field_name("value")),
            _ => continue,
        };
        if let Some(n) = name {
            out.push((text(n, src).to_string(), default));
        }
    }
    if method && out.first().map_or(false, |(n, _)| n == "self" || n == "cls") {
        out.remove(0);
    }
    out
}

/// The defaults an entry's parameters take when it is run as written: a plain default, a click
/// option's or argument's `default=`, typer's `Option(default)`.
pub(super) fn entry_defaults<'t>(def: TsNode<'t>, src: &[u8]) -> Vec<(String, TsNode<'t>)> {
    let mut out = vec![];
    for (name, default) in parameters(def, src, false) {
        if let Some(d) = default {
            let given = match d.kind() {
                "call" if call_path(d.child_by_field_name("function").unwrap_or(d), src).last().map_or(false, |l| l == "Option" || l == "Argument") => {
                    d.child_by_field_name("arguments").and_then(|a| named_children(a).into_iter().find(|x| x.kind() != "keyword_argument" && x.kind() != "comment").or_else(|| keyword(a, src, "default")))
                }
                _ => Some(d),
            };
            out.extend(given.map(|g| (name.clone(), g)));
        }
    }
    let decorated = def.parent().filter(|p| p.kind() == "decorated_definition");
    for dec in decorated.map(|p| named_children(p).into_iter().filter(|c| c.kind() == "decorator").collect::<Vec<_>>()).unwrap_or_default() {
        let Some(call) = named_children(dec).into_iter().find(|c| c.kind() == "call") else { continue };
        let head = call_path(call.child_by_field_name("function").unwrap_or(call), src);
        if !matches!(head.last().map(String::as_str), Some("option" | "argument")) {
            continue;
        }
        let Some(args) = call.child_by_field_name("arguments") else { continue };
        let Some(default) = keyword(args, src, "default") else { continue };
        let names: Vec<String> = named_children(args).into_iter().filter(|a| a.kind() == "string").map(|a| unquote(text(a, src))).collect();
        let name = names.iter().find(|n| !n.starts_with('-')).cloned().or_else(|| names.iter().filter(|n| n.starts_with("--")).max_by_key(|n| n.len()).cloned()).or_else(|| names.first().cloned());
        if let Some(n) = name {
            out.push((n.trim_start_matches('-').replace('-', "_"), default));
        }
    }
    out
}

fn keyword<'t>(args: TsNode<'t>, src: &[u8], name: &str) -> Option<TsNode<'t>> {
    named_children(args).into_iter().find_map(|a| (a.kind() == "keyword_argument" && a.child_by_field_name("name").map(|n| text(n, src)) == Some(name)).then(|| a.child_by_field_name("value")).flatten())
}

/// One argument a call hands a parameter, as the caller builds it.
pub(super) struct Passed {
    pub caller: String,
    pub callee: String,
    pub param: String,
    pub values: Vec<Built>,
}

/// What each function's parameters hold: what their callers pass, carried through the
/// parameters of the callers in turn, until nothing changes.
pub(super) fn handed(passed: &[Passed], base: HashMap<(String, String), Vec<Built>>) -> HashMap<(String, String), Vec<Built>> {
    let mut values = base;
    for _ in 0..10 {
        let mut changed = false;
        for p in passed {
            let got: Vec<Built> = p.values.iter().flat_map(|b| bind(b, &p.caller, &values)).collect();
            let slot = values.entry((p.callee.clone(), p.param.clone())).or_default();
            for g in got {
                if slot.len() < MAX && !slot.contains(&g) {
                    slot.push(g);
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    values
}

/// A path with the parameters of `f` replaced by what they hold; one that holds nothing known
/// stays a hole.
pub(super) fn bind(b: &Built, f: &str, values: &HashMap<(String, String), Vec<Built>>) -> Vec<Built> {
    let mut out: Vec<Built> = vec![vec![]];
    for s in b {
        let piece: Vec<Built> = match s {
            Seg::Param(p) => match values.get(&(f.to_string(), p.clone())).filter(|v| !v.is_empty()) {
                Some(v) => v.iter().map(|x| x.iter().map(|s| if let Seg::Param(q) = s { Seg::Hole(q.clone()) } else { s.clone() }).collect()).collect(),
                None => vec![hole(p)],
            },
            s => vec![vec![s.clone()]],
        };
        out = cross(&out, &piece, "");
    }
    let mut seen = HashSet::new();
    out.retain(|b| seen.insert(b.clone()));
    out
}
