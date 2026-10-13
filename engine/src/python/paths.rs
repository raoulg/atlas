//! Where a file call's file is: its path evaluated from the code, through the definitions ty
//! finds, and through the values a function's parameters are handed by its callers. An entry's
//! parameters hold their declared defaults (`@click.option(.., default=..)`).

use std::collections::HashMap;
use std::path::Path;

use indexmap::IndexMap;
use tree_sitter::Node as TsNode;

use super::{call_path, dotted, Module};
use crate::pytypes::{FileArg, Param, Target, Types};
use crate::tree::{named_children, path_str, text, unquote, walk};

pub(super) use crate::paths::{Built, Seg};
use crate::knowledge;
use crate::paths::{cross, hole, lit, parent, union};

const DEPTH: u32 = 12;

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

impl<'a, 't> At<'a, 't> {
    /// Where a definition written at `offset` in `m` is evaluated: in this scope when it is written
    /// inside it, else at the top of `m`.
    fn of_definition(self, m: &'a Module, offset: usize) -> At<'a, 't> {
        let inside = std::ptr::eq(m, self.m) && self.scope.map_or(false, |s| s.start_byte() <= offset && offset < s.end_byte());
        if inside {
            At { m, ..self }
        } else {
            At { m, scope: None, params: &[] }
        }
    }
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
        self.types.of(&path_str(&m.path)).map_or(false, |x| x.callees((f.start_byte(), f.end_byte())).iter().any(|c| matches!(&c.target, Target::Lib(m) if knowledge::PATH_LIBRARIES.contains(&m.as_str()))))
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
                    a => self.parsed(at, obj, a, depth).or_else(|| self.defined(at, e, depth)).unwrap_or_else(|| vec![named()]),
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

    /// An attribute of the object a command-line parser gives (`args.out`): the default of the
    /// argument that declares it, what the program runs with when its command line leaves it out.
    /// The attribute is named by the argument's `dest`, else its longest `--option` (`-` as
    /// `_`), else its positional name.
    fn parsed(&self, at: At, obj: TsNode, name: &str, depth: u32) -> Option<Vec<Built>> {
        let Target::Here { file, at: offset } = self.def(at.m, obj)? else { return None };
        let m = *self.by_file.get(&file)?;
        let facts = self.types.of(&file)?;
        let assigned = m.tree.root_node().descendant_for_byte_range(offset, offset)?.parent().filter(|p| p.kind() == "assignment")?;
        let right = assigned.child_by_field_name("right")?;
        let value = facts.value((right.start_byte(), right.end_byte()))?;
        let of_lib = |t: &Target, lib: &str| matches!(t, Target::Lib(l) if l == lib || l.starts_with(&format!("{lib}.")));
        let parser = knowledge::ARG_PARSERS.iter().find(|p| value.names.iter().any(|n| n == p.namespace) && value.classes.iter().any(|c| of_lib(c, p.package)))?;
        for call in walk(m.tree.root_node(), &["call"]) {
            let Some(f) = call.child_by_field_name("function").filter(|f| f.kind() == "attribute") else { continue };
            if f.child_by_field_name("attribute").map(|a| text(a, &m.src)) != Some(parser.declares) || !facts.callees((f.start_byte(), f.end_byte())).iter().any(|c| of_lib(&c.target, parser.package)) {
                continue;
            }
            let (mut flags, mut dest, mut default) = (vec![], None, None);
            for a in call.child_by_field_name("arguments").map(named_children).unwrap_or_default() {
                match a.kind() {
                    "string" => flags.push(unquote(text(a, &m.src))),
                    "keyword_argument" => match a.child_by_field_name("name").map(|k| text(k, &m.src)) {
                        Some(k) if k == parser.dest => dest = a.child_by_field_name("value").map(|v| unquote(text(v, &m.src))),
                        Some(k) if k == parser.default => default = a.child_by_field_name("value"),
                        _ => {}
                    },
                    _ => {}
                }
            }
            let option = |f: &String| f.trim_start_matches('-').replace('-', "_");
            let declared = dest
                .or_else(|| flags.iter().filter(|f| f.starts_with("--")).max_by_key(|f| f.len()).map(option))
                .or_else(|| flags.iter().find(|f| !f.starts_with('-')).cloned())
                .or_else(|| flags.first().map(option));
            if declared.as_deref() == Some(name) {
                return default.map(|d| self.of(At { m, scope: enclosing_def(call), params: &[] }, d, depth + 1));
            }
        }
        None
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
            return Some(self.of(at.of_definition(m, offset), value, depth + 1));
        }
        match p.kind() {
            // a field a value is built from (a dataclass's, a model's): what its builders hand it
            "assignment" if p.child_by_field_name("type").is_some() && field_of(p, &m.src).is_some() => {
                let class = field_of(p, &m.src)?;
                let id = function_id(m, class)?;
                Some(vec![vec![Seg::Held(format!("{id}.__init__"), name)]])
            }
            "assignment" => {
                let right = p.child_by_field_name("right")?;
                let here = at.of_definition(m, offset);
                // set in another function from what it was handed (`self.cache_dir = Path(cache_dir)`
                // in `__init__`): what that function's callers hand it
                match enclosing_def(p).filter(|_| here.scope.is_none()).and_then(|d| function_id(m, d).map(|id| (d, id))) {
                    Some((d, id)) => {
                        let names: Vec<String> = parameters(d, &m.src, in_class(d)).into_iter().map(|(n, _)| n).collect();
                        let vals = self.of(At { m, scope: Some(d), params: &names }, right, depth + 1);
                        Some(vals.into_iter().map(|b| b.into_iter().map(|s| if let Seg::Param(x) = s { Seg::Held(id.clone(), x) } else { s }).collect()).collect())
                    }
                    None => Some(self.of(here, right, depth + 1)),
                }
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
        if knowledge::PATH_MAKERS.contains(&last) {
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
            m if knowledge::SAME_PATH.contains(&m) => return receiver(),
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

/// The id the graph knows a function by: its module, the classes around it, its name. A
/// function inside a function has none of its own.
fn function_id(m: &Module, d: TsNode) -> Option<String> {
    let mut names = vec![text(d.child_by_field_name("name")?, &m.src).to_string()];
    let mut p = d.parent();
    while let Some(x) = p {
        match x.kind() {
            "class_definition" => names.push(text(x.child_by_field_name("name")?, &m.src).to_string()),
            "function_definition" => return None,
            _ => {}
        }
        p = x.parent();
    }
    names.reverse();
    Some(format!("{}.{}", m.id, names.join(".")))
}

/// The class an assignment declares a field of: one written in the body of a class that has no
/// `__init__` of its own, so that it is built from its fields.
fn field_of<'t>(a: TsNode<'t>, src: &[u8]) -> Option<TsNode<'t>> {
    let st = a.parent().filter(|x| x.kind() == "expression_statement")?;
    let class = st.parent().filter(|x| x.kind() == "block")?.parent().filter(|x| x.kind() == "class_definition")?;
    let body = class.child_by_field_name("body")?;
    let own_init = named_children(body).into_iter().any(|d| {
        let d = if d.kind() == "decorated_definition" { d.child_by_field_name("definition").unwrap_or(d) } else { d };
        d.kind() == "function_definition" && d.child_by_field_name("name").map(|n| text(n, src)) == Some("__init__")
    });
    (!own_init).then_some(class)
}

/// Whether a function is a method: defined in a class's body.
fn in_class(d: TsNode) -> bool {
    let mut p = d.parent();
    while let Some(x) = p {
        match x.kind() {
            "class_definition" => return true,
            "function_definition" | "module" => return false,
            _ => p = x.parent(),
        }
    }
    false
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

