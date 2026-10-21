//! Where a TypeScript file call's file is, and where a request goes: the path or URL evaluated
//! from the code, through the constants it names (across imports), and through the values a
//! function's parameters are handed by its callers.

use std::collections::HashMap;
use std::path::Path;

use indexmap::IndexMap;
use tree_sitter::Node as TsNode;

use super::module::{
    class_named, declarations, function_in, is_property, member_parts, package_from_ext, top_variables, Bound, Module, WRAPPERS,
};
use crate::knowledge::{self, PathFn};
use crate::paths::{cross, environment, environment_named, hole, lit, or_else, parent, substitute, union, Built, Seg};
use crate::tree::{beneath, named_children, path_str, text, unquote, walk};

const DEPTH: u32 = 12;

/// Evaluates paths and URLs in the modules of one package.
pub(super) struct Eval<'a> {
    modules: &'a IndexMap<String, Module>,
    root: &'a Path,
    /// The program's classes each class extends, by id, in the order it declares them.
    bases: HashMap<String, Vec<String>>,
}

/// Where an expression is evaluated: its module, and the function it is in with that function's
/// parameters.
#[derive(Clone, Copy)]
pub(super) struct At<'a, 't> {
    pub m: &'a Module,
    pub scope: Option<TsNode<'t>>,
    pub params: &'a [String],
}

/// The function a body belongs to: the body's own node, its parent, or the function a
/// declaration's call chain is handed (`createServerFn(..).handler(async () => ..)`).
pub(super) fn function_node(body: TsNode) -> Option<TsNode> {
    let fun = |n: TsNode| {
        matches!(
            n.kind(),
            "arrow_function"
                | "function_expression"
                | "function"
                | "function_declaration"
                | "generator_function_declaration"
                | "method_definition"
        )
    };
    if fun(body) {
        return Some(body);
    }
    match body.parent() {
        Some(p) if fun(p) => Some(p),
        _ => function_in(body),
    }
}

/// A function's parameters by name, in order; a destructured one has none.
pub(super) fn param_names(f: TsNode, src: &[u8]) -> Vec<String> {
    let Some(ps) = f.child_by_field_name("parameters").or_else(|| f.child_by_field_name("parameter")) else { return vec![] };
    if ps.kind() == "identifier" {
        return vec![text(ps, src).to_string()];
    }
    named_children(ps)
        .into_iter()
        .filter(|p| p.kind() != "comment")
        .map(|p| p.child_by_field_name("pattern").unwrap_or(p))
        .map(|p| if p.kind() == "identifier" { text(p, src).to_string() } else { String::new() })
        .collect()
}

impl<'a> Eval<'a> {
    pub fn new(modules: &'a IndexMap<String, Module>, root: &'a Path, bases: HashMap<String, Vec<String>>) -> Self {
        Eval { modules, root, bases }
    }

    /// The module's file, relative to the package root.
    fn file(&self, m: &Module) -> String {
        path_str(m.path.strip_prefix(self.root).unwrap_or(&m.path))
    }

    /// Where the module's own code lies: its file, as the package ships it.
    fn here(&self, m: &Module) -> Built {
        vec![Seg::Package(self.file(m))]
    }

    fn package(&self, m: &Module, name: &str) -> Option<String> {
        match m.bindings.get(name)? {
            Bound::Ext(e) => Some(package_from_ext(e, &m.ext_pkgs)),
            Bound::Own(_) => None,
        }
    }

    /// The values `e` can have.
    pub fn of<'t>(&self, at: At<'_, 't>, e: TsNode<'t>, depth: u32) -> Vec<Built> {
        if depth > DEPTH {
            return vec![hole("…")];
        }
        let e = beneath(e, WRAPPERS);
        let src = &at.m.src;
        let inner =
            |e: TsNode<'t>| named_children(e).into_iter().next().map(|x| self.of(at, x, depth + 1)).unwrap_or_else(|| vec![hole("value")]);
        match e.kind() {
            "string" => vec![lit(&unquote(text(e, src)))],
            "template_string" => {
                let mut out = vec![vec![]];
                for p in named_children(e) {
                    let piece = match p.kind() {
                        "template_substitution" => inner(p),
                        _ => vec![lit(text(p, src))],
                    };
                    out = cross(&out, &piece, "");
                }
                out
            }
            "binary_expression" => {
                let (Some(l), Some(r)) = (e.child_by_field_name("left"), e.child_by_field_name("right")) else {
                    return vec![hole("value")];
                };
                match e.child_by_field_name("operator").map(|o| text(o, src)) {
                    Some("+") => cross(&self.of(at, l, depth + 1), &self.of(at, r, depth + 1), ""),
                    // `x ?? d`: `x`, or `d` where `x` says nothing
                    Some("??" | "||") => or_else(self.of(at, l, depth + 1), self.of(at, r, depth + 1)),
                    _ => vec![hole("value")],
                }
            }
            "ternary_expression" => {
                let (Some(a), Some(b)) = (e.child_by_field_name("consequence"), e.child_by_field_name("alternative")) else {
                    return vec![hole("value")];
                };
                union(self.of(at, a, depth + 1), self.of(at, b, depth + 1))
            }
            "identifier" | "shorthand_property_identifier" => self.name(at, e, depth),
            "member_expression" => self.member(at, e, depth),
            "call_expression" => self.call(at, e, depth),
            "new_expression" => {
                // `new URL("../data", import.meta.url)`: relative to the module's own file
                let args = e.child_by_field_name("arguments").map(named_children).unwrap_or_default();
                match (e.child_by_field_name("constructor").map(|c| text(c, src)), args.as_slice()) {
                    (Some("URL"), [rel, base]) => {
                        cross(&self.of(at, *base, depth + 1).iter().map(parent).collect::<Vec<_>>(), &self.of(at, *rel, depth + 1), "/")
                    }
                    (Some("URL"), [x]) => self.of(at, *x, depth + 1),
                    _ => vec![hole("value")],
                }
            }
            _ => vec![hole("value")],
        }
    }

    fn member<'t>(&self, at: At<'_, 't>, e: TsNode<'t>, depth: u32) -> Vec<Built> {
        let src = &at.m.src;
        let parts = member_parts(e, src);
        let last = parts.last().cloned().unwrap_or_else(|| "value".into());
        match text(e, src) {
            "import.meta.url" | "import.meta.filename" => return vec![self.here(at.m)],
            "import.meta.dirname" => return vec![parent(&self.here(at.m))],
            _ => {}
        }
        // an environment variable is set where the program runs
        if matches!(parts.as_slice(), [p, env, _] if p == "process" && env == "env") {
            return vec![environment(&last)];
        }
        // what an object holds: a field its constructor sets from what it is handed
        if let [this, name] = parts.as_slice() {
            if this == "this" {
                if let Some(v) = self.held(at, e, name, depth) {
                    return v;
                }
            }
        }
        // a constant of an own module imported whole: `paths.DATA_DIR`
        if let [head, name] = parts.as_slice() {
            if let Some(Bound::Own(target)) = at.m.bindings.get(head) {
                if let Some(home) = self.modules.get(target) {
                    if let Some(v) = top_value(home, name) {
                        return self.of(At { m: home, scope: None, params: &[] }, v, depth + 1);
                    }
                }
            }
        }
        if let Some(v) = self.option(at, e, depth) {
            return v;
        }
        vec![hole(&last)]
    }

    /// An option of the command line as a parser gives it (`values.raw`, `parsed.values.raw`): the
    /// entry's input, as a parameter of the command is. It holds what the program runs with when
    /// its command line leaves the option out: the option's default, reached through the option,
    /// else no value, so that `values.raw ?? RAW` is `RAW`.
    fn option<'t>(&self, at: At<'_, 't>, e: TsNode<'t>, depth: u32) -> Option<Vec<Built>> {
        let src = &at.m.src;
        let parts = member_parts(e, src);
        let (option, path) = parts.split_last()?;
        let head = path.first()?;
        for (v, scope) in self.declared(at, head, e.start_byte()) {
            let (Some(pattern), Some(value)) = (v.child_by_field_name("name"), v.child_by_field_name("value")) else { continue };
            let Some((call, parser)) = self.parser_call(at.m, value) else { continue };
            // the values stand under their field: `{ values }`, `{ values: opts }`, or the whole result
            let whole = pattern.kind() == "identifier" && matches!(path, [_, field] if field == parser.values);
            let picked = pattern.kind() == "object_pattern" && path.len() == 1 && binds(pattern, parser.values, head, src);
            if !(whole || picked) {
                continue;
            }
            let here = At { m: at.m, scope, params: if scope.is_some() { at.params } else { &[] } };
            let handed = call.child_by_field_name("arguments").map(named_children).unwrap_or_default().into_iter().next()?;
            let options = self.object(here, handed, 0).and_then(|o| field(o, parser.options, src)).and_then(|o| self.object(here, o, 0))?;
            let declared = field(options, option, src).and_then(|o| self.object(here, o, 0))?;
            return Some(
                field(declared, parser.default, src).map_or_else(Vec::new, |d| crate::paths::option(option, self.of(here, d, depth + 1))),
            );
        }
        None
    }

    /// The declarations that may bind `name` where the code at `before` reads it: in the function
    /// it is in, written before it, then at the module's top level; each with the function it is in.
    fn declared<'s>(&self, at: At<'s, 's>, name: &str, before: usize) -> Vec<(TsNode<'s>, Option<TsNode<'s>>)> {
        let src = &at.m.src;
        let binds_name = |v: &TsNode| {
            v.child_by_field_name("name").map_or(false, |p| match p.kind() {
                "identifier" => text(p, src) == name,
                _ => walk(p, &["shorthand_property_identifier_pattern", "identifier"]).iter().any(|x| text(*x, src) == name),
            })
        };
        let local = at
            .scope
            .map(|s| walk(s, &["variable_declarator"]))
            .unwrap_or_default()
            .into_iter()
            .filter(|v| v.start_byte() < before && binds_name(v))
            .map(|v| (v, at.scope));
        let top = top_variables(at.m.tree.root_node()).into_iter().filter(binds_name).map(|v| (v, None));
        local.rev().chain(top).collect()
    }

    /// The options of the command line a call of a command-line parser parses, each as the human
    /// types it: the keys of the options it is handed (`parseArgs({ options: { raw: .. } })` is
    /// `--raw`), in the order they are written.
    pub fn command_line<'t>(&self, at: At<'_, 't>, call: TsNode<'t>) -> Option<Vec<String>> {
        let src = &at.m.src;
        let (call, parser) = self.parser_call(at.m, call)?;
        let handed = call.child_by_field_name("arguments").map(named_children).unwrap_or_default().into_iter().next();
        let options =
            handed.and_then(|h| self.object(at, h, 0)).and_then(|o| field(o, parser.options, src)).and_then(|o| self.object(at, o, 0));
        let keys = options.map(named_children).unwrap_or_default().into_iter().filter_map(|p| match p.kind() {
            "pair" => p.child_by_field_name("key").map(|k| key_text(k, src)),
            "shorthand_property_identifier" => Some(text(p, src).to_string()),
            _ => None,
        });
        Some(keys.map(|k| crate::entries::flag(&[format!("--{k}")], &k)).collect())
    }

    /// The call of a command-line parser a value is, with the parser: `parseArgs({..})` as the
    /// package that exports it names it, however the module imports it.
    fn parser_call<'t>(&self, m: &Module, value: TsNode<'t>) -> Option<(TsNode<'t>, &'static knowledge::OptionParser)> {
        let v = beneath(value, WRAPPERS);
        let parts = member_parts(v.child_by_field_name("function").filter(|_| v.kind() == "call_expression")?, &m.src);
        let Bound::Ext(ext) = m.bindings.get(parts.first()?)? else { return None };
        let package = package_from_ext(ext, &m.ext_pkgs);
        let full = std::iter::once(ext.clone()).chain(parts[1..].iter().cloned()).collect::<Vec<_>>().join(".");
        let function = full.rsplit('.').next()?;
        knowledge::OPTION_PARSERS.iter().find(|p| p.package == package && p.function == function).map(|p| (v, p))
    }

    /// The object literal an expression is: written where it stands, or held by a constant.
    fn object<'s>(&self, at: At<'s, 's>, e: TsNode<'s>, depth: u32) -> Option<TsNode<'s>> {
        if depth > DEPTH {
            return None;
        }
        let src = &at.m.src;
        let e = beneath(e, WRAPPERS);
        match e.kind() {
            "object" => Some(e),
            "identifier" | "shorthand_property_identifier" => {
                let (v, scope) = self
                    .declared(at, text(e, src), e.start_byte())
                    .into_iter()
                    .find(|(v, _)| v.child_by_field_name("name").map_or(false, |p| p.kind() == "identifier"))?;
                self.object(At { scope, ..at }, v.child_by_field_name("value")?, depth + 1)
            }
            _ => None,
        }
    }

    /// The field `name` of the value `e` is read on, the body's own receiver: the field the first
    /// class that sets it sets, along the order of the class `e` is written in and its bases.
    fn held<'t>(&self, at: At<'_, 't>, e: TsNode<'t>, name: &str, depth: u32) -> Option<Vec<Built>> {
        let src = &at.m.src;
        let mut class = e.parent();
        while let Some(c) = class.filter(|c| !matches!(c.kind(), "class_declaration" | "class" | "abstract_class_declaration")) {
            class = c.parent();
        }
        let class = class?;
        let first = format!("{}.{}", at.m.id, text(class.child_by_field_name("name")?, src));
        if let Some(held) = self.set_by(at.m, class, &first, name, depth) {
            return Some(held);
        }
        let mut todo: Vec<String> = self.bases.get(&first).into_iter().flatten().rev().cloned().collect();
        let mut seen = std::collections::HashSet::from([first]);
        while let Some(id) = todo.pop() {
            if !seen.insert(id.clone()) {
                continue;
            }
            let Some((m, class)) = id.rsplit_once('.').and_then(|(mid, local)| {
                let m = self.modules.get(mid)?;
                Some((m, class_named(m, local)?))
            }) else {
                continue;
            };
            if let Some(held) = self.set_by(m, class, &id, name, depth) {
                return Some(held);
            }
            todo.extend(self.bases.get(&id).into_iter().flatten().rev().cloned());
        }
        None
    }

    /// The field `name` as class `class` (`id`, in module `m`) sets it: a parameter property of its
    /// constructor (`constructor(private dir: string)`) or `this.dir = ..` in the constructor's body,
    /// from what the constructor's callers hand it; else the field's own initializer.
    fn set_by<'t>(&self, m: &'t Module, class: TsNode<'t>, id: &str, name: &str, depth: u32) -> Option<Vec<Built>> {
        let src = &m.src;
        let members = named_children(class.child_by_field_name("body")?);
        let ctor = members
            .iter()
            .find(|x| x.kind() == "method_definition" && x.child_by_field_name("name").map(|n| text(n, src)) == Some("constructor"));
        let held = |p: String| vec![vec![Seg::Held(format!("{id}.constructor"), p)]];
        if let Some(ctor) = ctor {
            let params = ctor.child_by_field_name("parameters").map(named_children).unwrap_or_default();
            if params.iter().any(|p| is_property(*p) && p.child_by_field_name("pattern").map(|x| text(x, src)) == Some(name)) {
                return Some(held(name.to_string()));
            }
            let set = ctor.child_by_field_name("body").and_then(|b| {
                walk(b, &["assignment_expression"]).into_iter().find(|a| {
                    a.child_by_field_name("left").map(|l| member_parts(l, src)) == Some(vec!["this".to_string(), name.to_string()])
                })
            });
            if let Some(right) = set.and_then(|a| a.child_by_field_name("right")) {
                let names = param_names(*ctor, src);
                let vals = self.of(At { m, scope: Some(*ctor), params: &names }, right, depth + 1);
                return Some(
                    vals.into_iter()
                        .map(|b| {
                            b.into_iter()
                                .map(|s| if let Seg::Param(x) = s { Seg::Held(format!("{id}.constructor"), x) } else { s })
                                .collect()
                        })
                        .collect(),
                );
            }
        }
        let field = members
            .iter()
            .find(|x| x.kind() == "public_field_definition" && x.child_by_field_name("name").map(|n| text(n, src)) == Some(name))?;
        field.child_by_field_name("value").map(|v| self.of(At { m, scope: None, params: &[] }, v, depth + 1))
    }

    fn name<'t>(&self, at: At<'_, 't>, e: TsNode<'t>, depth: u32) -> Vec<Built> {
        let src = &at.m.src;
        let name = text(e, src).to_string();
        match name.as_str() {
            "__filename" => return vec![self.here(at.m)],
            "__dirname" => return vec![parent(&self.here(at.m))],
            _ => {}
        }
        // a local of the function, then its parameters
        if let Some(scope) = at.scope {
            let local = walk(scope, &["variable_declarator"])
                .into_iter()
                .find(|v| v.child_by_field_name("name").map(|n| text(n, src)) == Some(name.as_str()) && v.start_byte() < e.start_byte());
            if let Some(v) = local.and_then(|v| v.child_by_field_name("value")) {
                return self.of(at, v, depth + 1);
            }
            if at.params.contains(&name) {
                return vec![vec![Seg::Param(name)]];
            }
        }
        if let Some(v) = top_value(at.m, &name) {
            return self.of(At { m: at.m, scope: None, params: &[] }, v, depth + 1);
        }
        // a constant another module of the package exports
        let mut bound = at.m.bindings.get(&name).cloned();
        for _ in 0..8 {
            let Some(Bound::Own(target)) = bound else { break };
            let Some((mid, n)) = target.rsplit_once('.') else { break };
            let Some(home) = self.modules.get(mid) else { break };
            if let Some(v) = top_value(home, n) {
                return self.of(At { m: home, scope: None, params: &[] }, v, depth + 1);
            }
            bound = home.bindings.get(n).cloned();
        }
        vec![hole(&name)]
    }

    fn call<'t>(&self, at: At<'_, 't>, c: TsNode<'t>, depth: u32) -> Vec<Built> {
        let src = &at.m.src;
        let Some(f) = c.child_by_field_name("function") else { return vec![hole("call")] };
        let parts = member_parts(f, src);
        let args = c.child_by_field_name("arguments").map(named_children).unwrap_or_default();
        let arg = |i: usize| args.get(i).map(|a| self.of(at, *a, depth + 1)).unwrap_or_else(|| vec![hole("value")]);
        let last = parts.last().map(String::as_str).unwrap_or("call");
        let pkg = parts.first().and_then(|h| self.package(at.m, h)).unwrap_or_default();
        // a path function by the package it is imported from, or by the global object it is on
        let on = if pkg.is_empty() && parts.len() == 2 { parts[0].as_str() } else { pkg.as_str() };
        match knowledge::PATH_FUNCTIONS.iter().find(|(p, f, _)| *p == on && *f == last).map(|(_, _, k)| *k) {
            Some(PathFn::Join) => args.iter().fold(vec![vec![]], |acc: Vec<Built>, a| {
                let next = self.of(at, *a, depth + 1);
                if acc == vec![Vec::<Seg>::new()] {
                    next
                } else {
                    cross(&acc, &next, "/")
                }
            }),
            Some(PathFn::Parent) => arg(0).iter().map(parent).collect(),
            Some(PathFn::Same) => arg(0),
            Some(PathFn::Home) => vec![lit("~")],
            Some(PathFn::Here) => vec![lit(".")],
            Some(PathFn::Temp) => vec![lit(knowledge::TEMP_DIR)],
            Some(PathFn::Environment) => vec![environment_named(&arg(0)).unwrap_or_else(|| hole(last))],
            // the language's own conversions to a string
            None if parts.as_slice() == ["String"] => arg(0),
            None if last == "toString" => {
                f.child_by_field_name("object").map(|o| self.of(at, o, depth + 1)).unwrap_or_else(|| vec![hole("value")])
            }
            None => self.given(at, &parts, &args, depth).unwrap_or_else(|| vec![hole(last)]),
        }
    }

    /// What one of the program's functions gives back, its parameters bound to what the call
    /// hands them: a function the module declares, or one it imports from another of its modules.
    fn given<'t>(&self, at: At<'_, 't>, parts: &[String], args: &[TsNode<'t>], depth: u32) -> Option<Vec<Built>> {
        let [name] = parts else { return None };
        let mut found = top_function(at.m, name).map(|d| (at.m, d));
        let mut bound = at.m.bindings.get(name).cloned();
        for _ in 0..8 {
            if found.is_some() {
                break;
            }
            let Some(Bound::Own(target)) = bound else { break };
            let Some((mid, n)) = target.rsplit_once('.') else { break };
            let Some(home) = self.modules.get(mid) else { break };
            found = top_function(home, n).map(|d| (home, d));
            bound = home.bindings.get(n).cloned();
        }
        let (home, def) = found?;
        let names = param_names(def, &home.src);
        let inner = At { m: home, scope: Some(def), params: &names };
        let gives = returns(def).into_iter().fold(vec![], |acc, x| union(acc, self.of(inner, x, depth + 1)));
        if gives.is_empty() {
            return None;
        }
        let handed: HashMap<&str, Vec<Built>> = names.iter().zip(args).map(|(p, a)| (p.as_str(), self.of(at, *a, depth + 1))).collect();
        Some(gives.iter().flat_map(|b| substitute(b, |p| handed.get(p).cloned().unwrap_or_else(|| vec![hole(p)]))).collect())
    }
}

/// The function a module declares at its top level under `name`: `function api(..)`, or a
/// constant that is one (`const api = (..) => ..`).
fn top_function<'t>(m: &'t Module, name: &str) -> Option<TsNode<'t>> {
    let declared = declarations(m.tree.root_node()).into_iter().map(|(d, _, _)| d).find(|d| {
        matches!(d.kind(), "function_declaration" | "function") && d.child_by_field_name("name").map(|n| text(n, &m.src)) == Some(name)
    });
    declared.or_else(|| top_value(m, name).filter(|v| matches!(v.kind(), "arrow_function" | "function_expression" | "function")))
}

/// What a function gives back: the expression an arrow is, and what each `return` in its body
/// (not in a function inside it) gives.
fn returns(f: TsNode) -> Vec<TsNode> {
    let Some(body) = f.child_by_field_name("body") else { return vec![] };
    if body.kind() != "statement_block" {
        return vec![body];
    }
    let own = |r: &TsNode| {
        let mut p = r.parent();
        while let Some(x) = p.filter(|x| x.id() != f.id()) {
            if function_node(x) == Some(x) {
                return false;
            }
            p = x.parent();
        }
        true
    };
    walk(body, &["return_statement"]).into_iter().filter(own).filter_map(|r| named_children(r).into_iter().next()).collect()
}

/// Whether a destructuring pattern binds the field `key` to the name `local`: `{ values }`,
/// `{ values: opts }`.
fn binds(pattern: TsNode, key: &str, local: &str, src: &[u8]) -> bool {
    named_children(pattern).into_iter().any(|p| match p.kind() {
        "shorthand_property_identifier_pattern" => text(p, src) == key && key == local,
        "pair_pattern" => {
            p.child_by_field_name("key").map(|k| key_text(k, src)) == Some(key.to_string())
                && p.child_by_field_name("value").map_or(false, |v| v.kind() == "identifier" && text(v, src) == local)
        }
        _ => false,
    })
}

/// A key of an object or of a pattern as it names a field: `raw`, `"dry-run"`.
fn key_text(k: TsNode, src: &[u8]) -> String {
    if k.kind() == "string" {
        unquote(text(k, src))
    } else {
        text(k, src).to_string()
    }
}

/// What an object literal holds under `key`: `{ key: value }`, or the name `{ key }` stands for.
fn field<'t>(object: TsNode<'t>, key: &str, src: &[u8]) -> Option<TsNode<'t>> {
    named_children(object).into_iter().find_map(|p| match p.kind() {
        "pair" if p.child_by_field_name("key").map(|k| key_text(k, src)) == Some(key.to_string()) => p.child_by_field_name("value"),
        "shorthand_property_identifier" if text(p, src) == key => Some(p),
        _ => None,
    })
}

/// The value a module-level `const` is declared with.
fn top_value<'t>(m: &'t Module, name: &str) -> Option<TsNode<'t>> {
    top_variables(m.tree.root_node())
        .into_iter()
        .find(|v| v.child_by_field_name("name").map(|n| text(n, &m.src)) == Some(name))?
        .child_by_field_name("value")
}

/// One call of a function of the package, with the argument each parameter is handed.
pub(super) struct Arg<'t> {
    pub caller: String,
    pub callee: String,
    pub module: String,
    pub scope: Option<TsNode<'t>>,
    pub params: Vec<String>,
    pub passed: Vec<(String, TsNode<'t>)>,
    /// The caller's parameter the call is made on: `exporter` in `exporter.write(..)`.
    pub on: Option<String>,
}

/// A call with what it hands each parameter, evaluated in the caller.
pub(super) struct Evaluated<'a, 't> {
    pub arg: &'a Arg<'t>,
    pub eval: &'a Eval<'a>,
}

impl crate::paths::Handing for Evaluated<'_, '_> {
    fn caller(&self) -> &str {
        &self.arg.caller
    }
    fn callee(&self) -> &str {
        &self.arg.callee
    }
    fn hands(&self, p: &str) -> Option<Vec<Built>> {
        let (_, node) = self.arg.passed.iter().find(|(q, _)| q == p)?;
        let m = self.eval.modules.get(&self.arg.module)?;
        Some(self.eval.of(At { m, scope: self.arg.scope, params: &self.arg.params }, *node, 0))
    }
    fn on(&self) -> Option<&str> {
        self.arg.on.as_deref()
    }
    fn class_handed(&self, p: &str) -> Option<String> {
        let (_, node) = self.arg.passed.iter().find(|(q, _)| q == p)?;
        let m = self.eval.modules.get(&self.arg.module)?;
        let name = text(node.child_by_field_name("constructor").filter(|_| node.kind() == "new_expression")?, &m.src);
        match m.bindings.get(name) {
            Some(Bound::Own(target)) => Some(target.clone()),
            Some(Bound::Ext(_)) => None,
            None => m.symbols.get(name).cloned(),
        }
    }
}
