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
use crate::paths::{
    bound as named_by, cross, each, environment, environment_named, held_by, hole, lit, or_else, parent, substitute, union, Built, Seg,
};
use crate::tree::{beneath, named_children, outermost, path_str, text, unquote, walk};

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
            // a number the code writes is what it says: a socket's port
            "number" => vec![lit(text(e, src))],
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
            // `process.env[name]`: what the environment holds for the variable, where the code decides
            // its name, as `process.env.NAME` is; one no code decides is a value the code gives no name
            "subscript_expression" if e.child_by_field_name("object").is_some_and(|o| environment_object(&member_parts(o, src))) => {
                let named = e.child_by_field_name("index").and_then(|k| environment_named(&self.of(at, k, depth + 1)));
                vec![named.unwrap_or_else(|| hole("value"))]
            }
            "subscript_expression" => self.looked_up(at, e, depth).unwrap_or_else(|| vec![hole("value")]),
            // an array is the values it holds; iterating it yields one of them
            "array" => {
                named_children(e).into_iter().filter(|x| x.kind() != "comment").fold(vec![], |acc, x| union(acc, self.of(at, x, depth + 1)))
            }
            "spread_element" => inner(e),
            "call_expression" => self.call(at, e, depth),
            // a path class made of its parts: `new URL("../data", import.meta.url)` is relative to
            // the module's own file
            "new_expression" => {
                let args = e.child_by_field_name("arguments").map(named_children).unwrap_or_default();
                // a socket made without an address is at the place a later call on it names
                if self.door(at.m, e.child_by_field_name("constructor")).is_some_and(|a| a.is_none()) {
                    return self.placed_later(at, e, depth);
                }
                match e.child_by_field_name("constructor").and_then(|c| self.path_function_of(at.m, c)) {
                    Some(made) => self.made(at, made, &args, depth),
                    None => vec![hole("value")],
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
        if parts.split_last().is_some_and(|(_, object)| environment_object(object)) {
            return vec![environment(&last)];
        }
        // the name of an entry a listing gives (`e.name` of `readdirSync(d, { withFileTypes: true })`):
        // below the directory listed, `*`, as each name `readdirSync(d)` gives is
        if let (Some(o), [_, attr]) = (e.child_by_field_name("object"), parts.as_slice()) {
            let library = (o.kind() == "identifier").then(|| self.lists_entries(at, o)).flatten();
            if library.is_some_and(|lib| knowledge::ELEMENT_NAMES.iter().any(|(l, _, n)| *l == lib && n == attr)) {
                return vec![lit("*")];
            }
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
                        return named_by(name, self.of(At { m: home, scope: None, params: &[] }, v, depth + 1));
                    }
                }
            }
        }
        if let Some(v) = self.option(at, e, depth) {
            return v;
        }
        // a property of an object the code writes out, as `x["key"]` is
        if let Some((here, held)) = e.child_by_field_name("object").and_then(|o| self.written_out(at, o, depth)) {
            if held.kind() == "object" {
                let values = self.picked(here, held, Some(&last), depth);
                if !values.iter().all(|b| matches!(b.as_slice(), [Seg::Hole(_)])) {
                    return values;
                }
            }
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
        let e = beneath(e, WRAPPERS);
        if e.kind() == "object" {
            return Some(e);
        }
        let (at, v) = self.named_value(at, e)?;
        self.object(at, v, depth + 1)
    }

    /// What a name stands for where `e` reads it: the value its declaration in scope there gives
    /// it, in that declaration's scope.
    fn named_value<'s>(&self, at: At<'s, 's>, e: TsNode<'s>) -> Option<(At<'s, 's>, TsNode<'s>)> {
        if !matches!(e.kind(), "identifier" | "shorthand_property_identifier") {
            return None;
        }
        let (v, scope) = self
            .declared(at, text(e, &at.m.src), e.start_byte())
            .into_iter()
            .find(|(v, _)| v.child_by_field_name("name").map_or(false, |p| p.kind() == "identifier"))?;
        Some((At { scope, ..at }, v.child_by_field_name("value")?))
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
                return Some(vals.into_iter().map(|b| held_by(&format!("{id}.constructor"), b)).collect());
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
        // the element a loop or a function handed to an array's method binds the name to
        if let Some(v) = self.element(at, e, &name, depth) {
            return named_by(&at.scope.map_or_else(|| name.clone(), |s| local_name(s, &name, src)), each(v));
        }
        // a local of the function, then its parameters
        if let Some(scope) = at.scope {
            let local = walk(scope, &["variable_declarator"])
                .into_iter()
                .find(|v| v.child_by_field_name("name").map(|n| text(n, src)) == Some(name.as_str()) && v.start_byte() < e.start_byte());
            // a `for` statement's own variable is changed by the loop as many times as the code does not say, so
            // where the loop reads it, its value is not said (`docs/reference/mathematics.md`, "Paths as terms")
            if local.and_then(|v| v.parent()).and_then(|d| d.parent()).is_some_and(|f| f.kind() == "for_statement") {
                return vec![hole(&name)];
            }
            if let Some(v) = local.and_then(|v| v.child_by_field_name("value")) {
                return named_by(&local_name(scope, &name, src), self.of(at, v, depth + 1));
            }
            if at.params.contains(&name) {
                return vec![vec![Seg::Param(name)]];
            }
        }
        if let Some(v) = top_value(at.m, &name) {
            return named_by(&name, self.of(At { m: at.m, scope: None, params: &[] }, v, depth + 1));
        }
        // a constant another module of the package exports
        if let Some((home, n, v)) = self.exported(at.m, &name) {
            return named_by(&n, self.of(At { m: home, scope: None, params: &[] }, v, depth + 1));
        }
        vec![hole(&name)]
    }

    /// The constant another module of the package exports that the module imports as `name`: its
    /// module, its own name there and its value, followed through the modules that pass it on.
    fn exported(&self, m: &Module, name: &str) -> Option<(&'a Module, String, TsNode<'a>)> {
        let mut bound = m.bindings.get(name).cloned();
        for _ in 0..8 {
            let Some(Bound::Own(target)) = bound else { break };
            let (mid, n) = target.rsplit_once('.')?;
            let home = self.modules.get(mid)?;
            if let Some(v) = top_value(home, n) {
                return Some((home, n.to_string(), v));
            }
            bound = home.bindings.get(n).cloned();
        }
        None
    }

    /// What the name `name`, read at `e`, is bound to an element of: by a loop over a collection
    /// (`for (const p of ps)`), or as the parameter of a function handed to a method of an array that
    /// runs it on each element (`ps.map((p) => ..)`, knowledge::TYPESCRIPT_EACH); or what it is bound
    /// to a key of, by a loop over an object's keys (`for (const k in obj)`), which says `true`.
    fn bound_over<'t>(&self, at: At<'_, 't>, e: TsNode<'t>, name: &str) -> Option<(TsNode<'t>, bool)> {
        let src = &at.m.src;
        let inside = |n: Option<TsNode>| n.is_some_and(|n| n.start_byte() <= e.start_byte() && e.end_byte() <= n.end_byte());
        let mut p = e.parent();
        loop {
            let x = p?;
            match x.kind() {
                "for_in_statement"
                    if x.child_by_field_name("left").is_some_and(|l| l.kind() == "identifier" && text(l, src) == name)
                        && inside(x.child_by_field_name("body")) =>
                {
                    let keys = x.child_by_field_name("operator").is_some_and(|o| text(o, src) == "in");
                    return Some((x.child_by_field_name("right")?, keys));
                }
                // a parameter of a function inside the body binds the name, as an element or not
                "arrow_function" | "function_expression" | "function" if inside(x.child_by_field_name("body")) => {
                    if let Some(slot) = param_names(x, src).iter().position(|n| n == name) {
                        return Some((each_over(x, slot, src).or_else(|| self.served(at.m, x, slot))?, false));
                    }
                }
                _ => {}
            }
            // the function the body is: no binding beyond it is the element's
            if at.scope.is_some_and(|s| s.id() == x.id()) {
                return None;
            }
            p = x.parent();
        }
    }

    /// The library of the listing the name `o` is bound to an element of, where the listing is handed
    /// the option that makes it give entries rather than names (knowledge::LISTING_ENTRIES), through the
    /// methods that keep an array's elements: `e` of `for (const e of readdirSync(d, { withFileTypes: true }))`.
    fn lists_entries<'t>(&self, at: At<'_, 't>, o: TsNode<'t>) -> Option<&'static str> {
        let src = &at.m.src;
        let Some((mut over, false)) = self.bound_over(at, o, text(o, src)) else { return None };
        let call = loop {
            let c = beneath(over, WRAPPERS);
            let f = c.child_by_field_name("function").filter(|_| c.kind() == "call_expression")?;
            match f.child_by_field_name("property").filter(|_| f.kind() == "member_expression").map(|p| text(p, src)) {
                Some(m) if knowledge::TYPESCRIPT_SAME_ELEMENTS.contains(&m) => over = f.child_by_field_name("object")?,
                _ => break c,
            }
        };
        let parts = member_parts(call.child_by_field_name("function")?, src);
        let pkg = parts.first().and_then(|h| self.package(at.m, h)).unwrap_or_default();
        let l = listing(&pkg, parts.last()?)?;
        let (_, option) = knowledge::LISTING_ENTRIES.iter().find(|(lib, _)| *lib == l.library)?;
        let args = call.child_by_field_name("arguments").map(named_children).unwrap_or_default();
        let options = args.into_iter().filter(|x| x.kind() != "comment").nth(1).filter(|x| x.kind() == "object")?;
        (field(options, option, src)?.kind() == "true").then_some(l.library)
    }

    /// The value the name `name`, read at `e`, is bound to as an element (`bound_over`): one of the
    /// values `ps` holds, as Python's `for p in ps` is, or one of the keys `obj` writes out, as
    /// Python's `for k in d` is. An element of nothing known leaves the name the code gives it.
    fn element<'t>(&self, at: At<'_, 't>, e: TsNode<'t>, name: &str, depth: u32) -> Option<Vec<Built>> {
        let src = &at.m.src;
        let (over, keys) = self.bound_over(at, e, name)?;
        let read = |at: At<'_, 't>| if keys { self.keys(at, over, depth + 1) } else { self.of(at, over, depth + 1) };
        // what the element is of is read where it is written: outside the function the body is, in
        // the function around that
        let values = if at.scope.is_some_and(|s| s.start_byte() <= over.start_byte() && over.end_byte() <= s.end_byte()) {
            read(at)
        } else {
            let mut outer = over.parent();
            while let Some(x) = outer.filter(|x| function_node(*x) != Some(*x)) {
                outer = x.parent();
            }
            let names = outer.map(|f| param_names(f, src)).unwrap_or_default();
            read(At { m: at.m, scope: outer, params: &names })
        };
        (!values.iter().all(|b| matches!(b.as_slice(), [Seg::Hole(_)]))).then_some(values)
    }

    /// `x[k]`, `x` an object or an array the code writes out where it stands or where `x` is
    /// declared: the value a key the code decides picks, else one of the values `x` holds, as
    /// Python's `d[k]` is. A key the object does not write, or an index past the array's end, picks
    /// none of them, so it is one of them all.
    fn looked_up<'s>(&self, at: At<'s, 's>, e: TsNode<'s>, depth: u32) -> Option<Vec<Built>> {
        let (v, k) = (e.child_by_field_name("object")?, beneath(e.child_by_field_name("index")?, WRAPPERS));
        let key = match k.kind() {
            "number" => Some(text(k, &at.m.src).to_string()),
            _ => match self.of(at, k, depth + 1).as_slice() {
                [b] if crate::paths::known(b) => Some(crate::paths::render(b)),
                _ => None,
            },
        };
        let (here, held) = self.written_out(at, v, depth)?;
        let values = self.picked(here, held, key.as_deref(), depth);
        (!values.iter().all(|b| matches!(b.as_slice(), [Seg::Hole(_)]))).then_some(values)
    }

    /// The object or array literal a value is, where the code writes it out: where it stands, or
    /// where the name it is read by is declared, in the function, the module or the module that
    /// exports it. A parameter is what its callers hand, which the code does not write out here.
    fn written_out<'s>(&'s self, at: At<'s, 's>, v: TsNode<'s>, depth: u32) -> Option<(At<'s, 's>, TsNode<'s>)> {
        if depth > DEPTH {
            return None;
        }
        let v = beneath(v, WRAPPERS);
        match v.kind() {
            "object" | "array" => Some((at, v)),
            "identifier" => {
                let name = text(v, &at.m.src);
                match self.named_value(at, v) {
                    Some((here, _)) if here.scope.is_none() && at.params.iter().any(|p| p == name) => None,
                    Some((here, value)) => self.written_out(here, value, depth + 1),
                    None => {
                        let (home, _, value) = self.exported(at.m, name)?;
                        self.written_out(At { m: home, scope: None, params: &[] }, value, depth + 1)
                    }
                }
            }
            _ => None,
        }
    }

    /// What a key or an index picks of an object or an array the code writes out (an object by a
    /// key its properties write, an array by its place), else every value it holds.
    fn picked<'s>(&self, at: At<'s, 's>, held: TsNode<'s>, key: Option<&str>, depth: u32) -> Vec<Built> {
        let src = &at.m.src;
        let entries: Vec<TsNode> = named_children(held).into_iter().filter(|x| x.kind() != "comment").collect();
        if held.kind() == "array" {
            let i = key.and_then(|k| k.parse::<usize>().ok());
            return self.of(at, i.and_then(|i| entries.get(i).copied()).unwrap_or(held), depth + 1);
        }
        let named = |p: TsNode<'s>| self.key(at, p, depth);
        let value = |p: TsNode<'s>| match p.kind() {
            "pair" => p.child_by_field_name("value"),
            "shorthand_property_identifier" | "spread_element" => Some(p),
            _ => None,
        };
        let picked = key.and_then(|key| {
            entries.iter().find_map(|p| match p.kind() {
                "pair" if named(*p).as_deref() == Some(key) => value(*p),
                "shorthand_property_identifier" if text(*p, src) == key => Some(*p),
                _ => None,
            })
        });
        match picked {
            Some(x) => self.of(at, x, depth + 1),
            None => entries.into_iter().filter_map(value).fold(vec![], |acc, x| union(acc, self.of(at, x, depth + 1))),
        }
    }

    /// A property's key as the code writes it: a name, a string, a number, or `[K]` computed where the
    /// code decides `K`; none for a spread or a key no code decides.
    fn key<'s>(&self, at: At<'s, 's>, p: TsNode<'s>, depth: u32) -> Option<String> {
        let src = &at.m.src;
        if p.kind() == "shorthand_property_identifier" {
            return Some(text(p, src).to_string());
        }
        let k = p.child_by_field_name("key")?;
        match k.kind() {
            "computed_property_name" => match self.of(at, named_children(k).into_iter().next()?, depth + 1).as_slice() {
                [b] if crate::paths::known(b) => Some(crate::paths::render(b)),
                _ => None,
            },
            _ => Some(key_text(k, src)),
        }
    }

    /// The keys of an object the code writes out, where `v` stands or where the name it is read by is
    /// declared, as `for (k in obj)` goes over them: each as the code writes it. Nothing is known of
    /// them where one is not decided (a spread, a key computed from a value).
    fn keys<'s>(&self, at: At<'s, 's>, v: TsNode<'s>, depth: u32) -> Vec<Built> {
        let Some((here, held)) = self.written_out(at, v, depth).filter(|(_, h)| h.kind() == "object") else { return vec![] };
        let mut out = vec![];
        for p in named_children(held).into_iter().filter(|x| x.kind() != "comment") {
            match self.key(here, p, depth) {
                Some(k) => out = union(out, vec![lit(&k)]),
                None => return vec![],
            }
        }
        out
    }

    /// The path function the function `f` of a call, or the class of a `new`, is: a function by
    /// the package it is imported from or by the global object it is on (`path.join`,
    /// `process.cwd`; knowledge::PATH_FUNCTIONS), a class by its name (`URL`;
    /// knowledge::TYPESCRIPT_PATH_CLASSES).
    pub fn path_function_of(&self, m: &Module, f: TsNode) -> Option<PathFn> {
        let src = &m.src;
        if f.parent().is_some_and(|p| p.kind() == "new_expression") {
            return knowledge::TYPESCRIPT_PATH_CLASSES.iter().find(|(c, _)| *c == text(f, src)).map(|(_, k)| *k);
        }
        let parts = member_parts(f, src);
        let last = parts.last().map(String::as_str).unwrap_or("call");
        let pkg = parts.first().and_then(|h| self.package(m, h)).unwrap_or_default();
        let on = if pkg.is_empty() && parts.len() == 2 { parts[0].as_str() } else { pkg.as_str() };
        knowledge::PATH_FUNCTIONS.iter().find(|(p, n, _)| *p == on && *n == last).map(|(_, _, k)| *k)
    }

    /// What the path function `made` makes of the `args` it is handed.
    fn made<'t>(&self, at: At<'_, 't>, made: PathFn, args: &[TsNode<'t>], depth: u32) -> Vec<Built> {
        let arg = |i: usize| args.get(i).map(|a| self.of(at, *a, depth + 1)).unwrap_or_else(|| vec![hole("value")]);
        match made {
            PathFn::Join => args.iter().fold(vec![vec![]], |acc: Vec<Built>, a| {
                let next = self.of(at, *a, depth + 1);
                if acc == vec![Vec::<Seg>::new()] {
                    next
                } else {
                    cross(&acc, &next, "/")
                }
            }),
            PathFn::Parent => arg(0).iter().map(parent).collect(),
            PathFn::Same => arg(0),
            PathFn::Home => vec![lit("~")],
            PathFn::Here => vec![lit(".")],
            PathFn::Temp => vec![lit(knowledge::TEMP_DIR)],
            PathFn::Environment => vec![environment_named(&arg(0)).unwrap_or_else(|| hole("call"))],
            PathFn::Relative if args.len() > 1 => cross(&arg(1).iter().map(parent).collect::<Vec<_>>(), &arg(0), "/"),
            PathFn::Relative => arg(0),
        }
    }

    fn call<'t>(&self, at: At<'_, 't>, c: TsNode<'t>, depth: u32) -> Vec<Built> {
        let src = &at.m.src;
        let Some(f) = c.child_by_field_name("function") else { return vec![hole("call")] };
        let parts = member_parts(f, src);
        let args = c.child_by_field_name("arguments").map(named_children).unwrap_or_default();
        let arg = |i: usize| args.get(i).map(|a| self.of(at, *a, depth + 1)).unwrap_or_else(|| vec![hole("value")]);
        let last = parts.last().map(String::as_str).unwrap_or("call");
        let pkg = parts.first().and_then(|h| self.package(at.m, h)).unwrap_or_default();
        // the method a call of a value's method calls, whatever the value is written as
        let method = match f.kind() {
            "member_expression" => f.child_by_field_name("property").map_or("", |p| text(p, src)),
            _ => "",
        };
        // a socket is at the address it is opened at (`docs/reference/mathematics.md`, "A door keeps
        // its place"): the one it is handed, or the one a later call on it names
        match self.door(at.m, Some(f)) {
            Some(Some(a)) => return self.address(at, &args, a, depth),
            Some(None) => return self.placed_later(at, c, depth),
            None => {}
        }
        match self.path_function_of(at.m, f) {
            Some(made) => self.made(at, made, &args, depth),
            // the language's own conversions to a string
            None if parts.as_slice() == ["String"] => arg(0),
            None if last == "toString" => {
                f.child_by_field_name("object").map(|o| self.of(at, o, depth + 1)).unwrap_or_else(|| vec![hole("value")])
            }
            // a listing of a directory: each element is below it, `*`, as the path or as its name
            // alone, which the code joins back (`path.join(d, name)`)
            None if listing(&pkg, last).is_some() => match args.first().filter(|_| listing(&pkg, last).is_some_and(|l| !l.names)) {
                Some(d) => cross(&self.of(at, *d, depth + 1), &[lit("*")], "/"),
                None => vec![lit("*")],
            },
            // the elements a method of an array keeps (`names.filter(..)`), and what the function it
            // runs on each element gives (`names.map((n) => path.join(d, n))`)
            None if f.kind() == "member_expression" && knowledge::TYPESCRIPT_SAME_ELEMENTS.contains(&method) => {
                f.child_by_field_name("object").map(|o| self.of(at, o, depth + 1)).unwrap_or_else(|| vec![hole("value")])
            }
            None if f.kind() == "member_expression"
                && knowledge::TYPESCRIPT_GIVES_EACH.contains(&method)
                && args.first().is_some_and(|a| function_node(beneath(*a, WRAPPERS)) == Some(beneath(*a, WRAPPERS))) =>
            {
                let gives = returns(beneath(args[0], WRAPPERS)).into_iter().fold(vec![], |acc, x| union(acc, self.of(at, x, depth + 1)));
                if gives.is_empty() {
                    vec![hole("call")]
                } else {
                    gives
                }
            }
            None => self
                .given(at, &parts, &args, depth)
                .or_else(|| self.method_given(at, f, &args, depth))
                .unwrap_or_else(|| vec![hole("call")]),
        }
    }

    /// What one of the program's methods gives back, its parameters bound to what the call hands
    /// them: a static method called on its class (`Workbench.of(dir)`), or a method called on a
    /// value whose class the code says (`Workbench.of(dir).file(name)`, `bench.file(name)`), what
    /// it reads of `this` as the instances of that class hold it.
    fn method_given<'s>(&'s self, at: At<'s, 's>, f: TsNode<'s>, args: &[TsNode<'s>], depth: u32) -> Option<Vec<Built>> {
        if f.kind() != "member_expression" {
            return None;
        }
        let name = text(f.child_by_field_name("property")?, &at.m.src);
        let object = f.child_by_field_name("object")?;
        let (home, class) = self.class_called(at, object).or_else(|| self.class_of(at, object, depth + 1))?;
        let def = named_children(class.child_by_field_name("body")?)
            .into_iter()
            .find(|x| x.kind() == "method_definition" && x.child_by_field_name("name").map(|n| text(n, &home.src)) == Some(name))?;
        let names = param_names(def, &home.src);
        let inner = At { m: home, scope: Some(def), params: &names };
        let gives = returns(def).into_iter().fold(vec![], |acc, x| union(acc, self.of(inner, x, depth + 1)));
        if gives.is_empty() {
            return None;
        }
        let handed: HashMap<&str, Vec<Built>> = names.iter().zip(args).map(|(p, a)| (p.as_str(), self.of(at, *a, depth + 1))).collect();
        Some(gives.iter().flat_map(|b| substitute(b, |p| handed.get(p).cloned().unwrap_or_else(|| vec![hole(p)]))).collect())
    }

    /// The program's class a name is, where `e` names one: declared in the module, or imported from
    /// another of its modules.
    fn class_called<'s>(&'s self, at: At<'s, '_>, e: TsNode<'_>) -> Option<(&'s Module, TsNode<'s>)> {
        if e.kind() != "identifier" {
            return None;
        }
        let name = text(e, &at.m.src);
        if let Some(c) = class_named(at.m, name) {
            return Some((at.m, c));
        }
        let mut bound = at.m.bindings.get(name).cloned();
        for _ in 0..8 {
            let Some(Bound::Own(target)) = bound else { break };
            let (mid, n) = target.rsplit_once('.')?;
            let home = self.modules.get(mid)?;
            if let Some(c) = class_named(home, n) {
                return Some((home, c));
            }
            bound = home.bindings.get(n).cloned();
        }
        None
    }

    /// The program's class of the value `e` is, where the code says it: a `new` of it, what one of
    /// the program's methods gives back by its declared type, or a local that holds such a value.
    fn class_of<'s>(&'s self, at: At<'s, 's>, e: TsNode<'s>, depth: u32) -> Option<(&'s Module, TsNode<'s>)> {
        if depth > DEPTH {
            return None;
        }
        let src = &at.m.src;
        let e = beneath(e, WRAPPERS);
        match e.kind() {
            "new_expression" => self.class_called(at, e.child_by_field_name("constructor")?),
            "call_expression" => {
                let f = e.child_by_field_name("function").filter(|f| f.kind() == "member_expression")?;
                let name = text(f.child_by_field_name("property")?, src);
                let object = f.child_by_field_name("object")?;
                let (home, class) = self.class_called(at, object).or_else(|| self.class_of(at, object, depth + 1))?;
                let def = named_children(class.child_by_field_name("body")?)
                    .into_iter()
                    .find(|x| x.kind() == "method_definition" && x.child_by_field_name("name").map(|n| text(n, &home.src)) == Some(name))?;
                let returned = def.child_by_field_name("return_type").and_then(|t| named_children(t).into_iter().next())?;
                self.class_called(At { m: home, scope: None, params: &[] }, returned).or_else(|| {
                    (returned.kind() == "type_identifier")
                        .then(|| class_named(home, text(returned, &home.src)).map(|c| (home, c)))
                        .flatten()
                })
            }
            _ => {
                let (at, v) = self.named_value(at, e)?;
                self.class_of(at, v, depth + 1)
            }
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

    /// The Node socket a function or a class `f` opens (`knowledge::NODE_DOORS`), by the package
    /// its head is imported from: where its address is, or none where a later call names it.
    fn door(&self, m: &Module, f: Option<TsNode>) -> Option<Option<&'static knowledge::NodeAddress>> {
        let parts = member_parts(f?, &m.src);
        let pkg = self.package(m, parts.first()?)?;
        let last = parts.last()?;
        knowledge::NODE_DOORS.iter().find(|(p, n, _)| *p == pkg && n == last).map(|(_, _, a)| a.as_ref())
    }

    /// A socket's address as a Node call is handed it (`knowledge::NodeAddress`): `host:port`, the
    /// one form every language's is (`127.0.0.1:7878`), of the port and the host it is handed or of
    /// the options object that names them. A function handed where the host would be is a listener,
    /// and the host is Node's default.
    pub fn address<'t>(&self, at: At<'_, 't>, args: &[TsNode<'t>], a: &knowledge::NodeAddress, depth: u32) -> Vec<Built> {
        let src = &at.m.src;
        let (port, host) = match args.first().map(|x| beneath(*x, WRAPPERS)) {
            Some(o) if o.kind() == "object" => (field(o, "port", src), field(o, "host", src)),
            _ => (args.get(a.port).copied(), args.get(a.host).copied()),
        };
        let given = |x: Option<TsNode<'t>>| {
            x.filter(|x| !matches!(beneath(*x, WRAPPERS).kind(), "arrow_function" | "function_expression" | "function"))
        };
        let port = given(port).map(|p| self.of(at, p, depth + 1)).unwrap_or_else(|| vec![hole("port")]);
        let host = given(host).map(|h| self.of(at, h, depth + 1)).unwrap_or_else(|| vec![lit(a.default)]);
        cross(&host, &port, ":")
    }

    /// The place of a socket made without an address (`made`): the address the first call on it
    /// that names one is handed (`knowledge::NODE_DOOR_PLACES`), on it as made
    /// (`net.createServer(..).listen(port)`) or on the name the code keeps it by, later in the
    /// function or the module.
    fn placed_later<'s>(&self, at: At<'s, 's>, made: TsNode<'s>, depth: u32) -> Vec<Built> {
        let src = &at.m.src;
        let made = outermost(made, WRAPPERS);
        // the address a call on the socket names, where `on` is the socket it is called on
        let place = |call: TsNode<'s>, on: &dyn Fn(TsNode<'s>) -> bool| -> Option<&'static knowledge::NodeAddress> {
            let f = call.child_by_field_name("function").filter(|f| f.kind() == "member_expression")?;
            let method = text(f.child_by_field_name("property")?, src);
            let object = outermost(beneath(f.child_by_field_name("object")?, WRAPPERS), WRAPPERS);
            on(object).then(|| knowledge::NODE_DOOR_PLACES.iter().find(|(m, _)| *m == method).map(|(_, a)| a)).flatten()
        };
        let args = |call: TsNode<'s>| call.child_by_field_name("arguments").map(named_children).unwrap_or_default();
        let chained = made.parent().filter(|p| p.kind() == "member_expression").and_then(|p| p.parent());
        if let Some((call, a)) =
            chained.filter(|c| c.kind() == "call_expression").and_then(|c| Some((c, place(c, &|o| o.id() == made.id())?)))
        {
            return self.address(at, &args(call), a, depth);
        }
        let kept = made.parent().filter(|v| v.kind() == "variable_declarator").and_then(|v| v.child_by_field_name("name"));
        if let Some(name) = kept.filter(|n| n.kind() == "identifier").map(|n| text(n, src)) {
            let within = at.scope.unwrap_or_else(|| at.m.tree.root_node());
            let later = walk(within, &["call_expression"])
                .into_iter()
                .filter(|c| c.start_byte() > made.end_byte())
                .find_map(|c| Some((c, place(c, &|o| o.kind() == "identifier" && text(o, src) == name)?)));
            if let Some((call, a)) = later {
                return self.address(at, &args(call), a, depth);
            }
        }
        vec![hole("address")]
    }

    /// The Node server whose connections a function `f` is run on, where its parameter `slot` is the
    /// one each connection fills (`knowledge::NODE_DOOR_KEEPERS`): the call that makes the server it
    /// is handed to (`net.createServer((conn) => ..)`), or the server a listener for its connections
    /// is registered on (`server.on("connection", (conn) => ..)`).
    pub(super) fn served<'t>(&self, m: &Module, f: TsNode<'t>, slot: usize) -> Option<TsNode<'t>> {
        let src = &m.src;
        let args = outermost(f, WRAPPERS).parent().filter(|a| a.kind() == "arguments" && slot == 0)?;
        let call = args.parent().filter(|c| c.kind() == "call_expression")?;
        let callee = call.child_by_field_name("function")?;
        let last = member_parts(callee, src).pop()?;
        if self.door(m, Some(callee)).is_some() && knowledge::NODE_DOOR_KEEPERS.iter().any(|(maker, _)| *maker == last) {
            return Some(call);
        }
        let event = named_children(args).into_iter().next().filter(|e| e.kind() == "string").map(|e| unquote(text(e, src)));
        let connection = event.is_some_and(|e| knowledge::NODE_DOOR_KEEPERS.iter().any(|(_, ev)| *ev == e));
        (callee.kind() == "member_expression" && knowledge::NODE_LISTENERS.contains(&last.as_str()) && connection)
            .then(|| callee.child_by_field_name("object"))
            .flatten()
    }
}

/// Whether a value, by the parts of its name, is the runtime's object that holds the environment's
/// variables (knowledge::TYPESCRIPT_ENVIRONMENT): `process.env`.
fn environment_object(parts: &[String]) -> bool {
    knowledge::TYPESCRIPT_ENVIRONMENT.iter().any(|o| o.len() == parts.len() && o.iter().zip(parts).all(|(a, b)| a == b))
}

/// The library's function that lists a directory (knowledge::LISTINGS_BELOW) a call of `function`
/// imported from the package `pkg` is: `fs.readdirSync`, `readdir` of `fs/promises`.
fn listing(pkg: &str, function: &str) -> Option<&'static knowledge::Listing> {
    knowledge::LISTINGS_BELOW.iter().find(|l| l.function == function && pkg.split('/').next() == Some(l.library))
}

/// The array a function is handed to be run on each element of, where its parameter `slot` is the
/// one the element fills (knowledge::TYPESCRIPT_EACH): `ps` of `ps.map((p) => ..)`.
fn each_over<'t>(f: TsNode<'t>, slot: usize, src: &[u8]) -> Option<TsNode<'t>> {
    let mut handed = f;
    while let Some(p) = handed.parent().filter(|p| WRAPPERS.contains(&p.kind())) {
        handed = p;
    }
    let args = handed.parent().filter(|a| a.kind() == "arguments")?;
    if named_children(args).into_iter().find(|x| x.kind() != "comment").map(|x| x.id()) != Some(handed.id()) {
        return None;
    }
    let method = args.parent().filter(|c| c.kind() == "call_expression")?.child_by_field_name("function")?;
    let method = (method.kind() == "member_expression").then_some(method)?;
    let name = text(method.child_by_field_name("property")?, src);
    knowledge::TYPESCRIPT_EACH
        .iter()
        .any(|(m, element)| *m == name && *element as usize == slot)
        .then(|| method.child_by_field_name("object"))?
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

/// The name a value bound in a function goes by beyond it: the binding's with the function's
/// (`main.folder`, `Store.open.root`), as a parameter's is (`analyze.dir`), so that two functions'
/// `file`s are two values. A function without a name of its own (an arrow held by nothing) leaves
/// the binding's.
fn local_name(scope: TsNode, name: &str, src: &[u8]) -> String {
    let own = scope
        .child_by_field_name("name")
        .or_else(|| scope.parent().filter(|p| p.kind() == "variable_declarator").and_then(|p| p.child_by_field_name("name")));
    let Some(own) = own.map(|n| text(n, src).to_string()) else { return name.to_string() };
    let mut p = scope.parent();
    while let Some(x) = p.filter(|x| !matches!(x.kind(), "class_declaration" | "class" | "abstract_class_declaration")) {
        p = x.parent();
    }
    match p.and_then(|c| c.child_by_field_name("name")) {
        Some(class) if scope.kind() == "method_definition" => format!("{}.{own}.{name}", text(class, src)),
        _ => format!("{own}.{name}"),
    }
}
