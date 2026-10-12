//! Where a TypeScript file call's file is, and where a request goes: the path or URL evaluated
//! from the code, through the constants it names (across imports), and through the values a
//! function's parameters are handed by its callers.

use std::path::Path;

use indexmap::IndexMap;
use tree_sitter::Node as TsNode;

use super::{declarations, function_in, member_parts, Bound, Module};
use crate::knowledge::{self, PathFn};
use crate::paths::{cross, hole, lit, parent, union, Built, Seg};
use crate::tree::{named_children, path_str, text, unquote, walk};

const DEPTH: u32 = 12;

/// Evaluates paths and URLs in the modules of one package.
pub(super) struct Eval<'a> {
    modules: &'a IndexMap<String, Module>,
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

/// The function a body belongs to: the body's own node, its parent, or the function a
/// declaration's call chain is handed (`createServerFn(..).handler(async () => ..)`).
pub(super) fn function_node(body: TsNode) -> Option<TsNode> {
    let fun = |n: TsNode| matches!(n.kind(), "arrow_function" | "function_expression" | "function" | "function_declaration" | "generator_function_declaration" | "method_definition");
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
    pub fn new(modules: &'a IndexMap<String, Module>, root: &'a Path) -> Self {
        Eval { modules, root }
    }

    /// The module's file, relative to the package root.
    fn file(&self, m: &Module) -> String {
        path_str(m.path.strip_prefix(self.root).unwrap_or(&m.path))
    }

    fn package(&self, m: &Module, name: &str) -> Option<String> {
        match m.bindings.get(name)? {
            Bound::Ext(e) => Some(super::package_from_ext(e, &m.ext_pkgs)),
            Bound::Own(_) => None,
        }
    }

    /// The values `e` can have.
    pub fn of<'t>(&self, at: At<'_, 't>, e: TsNode<'t>, depth: u32) -> Vec<Built> {
        if depth > DEPTH {
            return vec![hole("…")];
        }
        let src = &at.m.src;
        let inner = |e: TsNode<'t>| named_children(e).into_iter().next().map(|x| self.of(at, x, depth + 1)).unwrap_or_else(|| vec![hole("value")]);
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
            "parenthesized_expression" | "as_expression" | "satisfies_expression" | "non_null_expression" | "await_expression" => inner(e),
            "binary_expression" => {
                let (Some(l), Some(r)) = (e.child_by_field_name("left"), e.child_by_field_name("right")) else { return vec![hole("value")] };
                match e.child_by_field_name("operator").map(|o| text(o, src)) {
                    Some("+") => cross(&self.of(at, l, depth + 1), &self.of(at, r, depth + 1), ""),
                    Some("??" | "||") => union(self.of(at, l, depth + 1), self.of(at, r, depth + 1)),
                    _ => vec![hole("value")],
                }
            }
            "ternary_expression" => {
                let (Some(a), Some(b)) = (e.child_by_field_name("consequence"), e.child_by_field_name("alternative")) else { return vec![hole("value")] };
                union(self.of(at, a, depth + 1), self.of(at, b, depth + 1))
            }
            "identifier" | "shorthand_property_identifier" => self.name(at, e, depth),
            "member_expression" => self.member(at, e, depth),
            "call_expression" => self.call(at, e, depth),
            "new_expression" => {
                // `new URL("../data", import.meta.url)`: relative to the module's own file
                let args = e.child_by_field_name("arguments").map(named_children).unwrap_or_default();
                match (e.child_by_field_name("constructor").map(|c| text(c, src)), args.as_slice()) {
                    (Some("URL"), [rel, base]) => cross(&self.of(at, *base, depth + 1).iter().map(parent).collect::<Vec<_>>(), &self.of(at, *rel, depth + 1), "/"),
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
            "import.meta.url" | "import.meta.filename" => return vec![lit(&self.file(at.m))],
            "import.meta.dirname" => return vec![parent(&lit(&self.file(at.m)))],
            _ => {}
        }
        // an environment variable is set where the program runs
        if matches!(parts.as_slice(), [p, env, _] if p == "process" && env == "env") {
            return vec![hole(&format!("${last}"))];
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
        vec![hole(&last)]
    }

    /// The field `name` of the class `e` is written in, as its constructor sets it: a parameter
    /// property (`constructor(private dir: string)`) or `this.dir = ..` in the constructor's body,
    /// from what the constructor's callers hand it; else the field's own initializer.
    fn held<'t>(&self, at: At<'_, 't>, e: TsNode<'t>, name: &str, depth: u32) -> Option<Vec<Built>> {
        let src = &at.m.src;
        let mut class = e.parent();
        while let Some(c) = class.filter(|c| !matches!(c.kind(), "class_declaration" | "class" | "abstract_class_declaration")) {
            class = c.parent();
        }
        let class = class?;
        let id = format!("{}.{}", at.m.id, text(class.child_by_field_name("name")?, src));
        let members = named_children(class.child_by_field_name("body")?);
        let ctor = members.iter().find(|m| m.kind() == "method_definition" && m.child_by_field_name("name").map(|n| text(n, src)) == Some("constructor"));
        let held = |p: String| vec![vec![Seg::Held(format!("{id}.constructor"), p)]];
        if let Some(ctor) = ctor {
            let params = ctor.child_by_field_name("parameters").map(named_children).unwrap_or_default();
            if params.iter().any(|p| named_children(*p).iter().any(|c| c.kind() == "accessibility_modifier") && p.child_by_field_name("pattern").map(|x| text(x, src)) == Some(name)) {
                return Some(held(name.to_string()));
            }
            let set = walk(ctor.child_by_field_name("body")?, &["assignment_expression"]).into_iter().find(|a| a.child_by_field_name("left").map(|l| member_parts(l, src)) == Some(vec!["this".to_string(), name.to_string()]));
            if let Some(right) = set.and_then(|a| a.child_by_field_name("right")) {
                let names = param_names(*ctor, src);
                let vals = self.of(At { m: at.m, scope: Some(*ctor), params: &names }, right, depth + 1);
                return Some(vals.into_iter().map(|b| b.into_iter().map(|s| if let Seg::Param(x) = s { Seg::Held(format!("{id}.constructor"), x) } else { s }).collect()).collect());
            }
        }
        let field = members.iter().find(|m| m.kind() == "public_field_definition" && m.child_by_field_name("name").map(|n| text(n, src)) == Some(name))?;
        field.child_by_field_name("value").map(|v| self.of(At { m: at.m, scope: None, params: &[] }, v, depth + 1))
    }

    fn name<'t>(&self, at: At<'_, 't>, e: TsNode<'t>, depth: u32) -> Vec<Built> {
        let src = &at.m.src;
        let name = text(e, src).to_string();
        match name.as_str() {
            "__filename" => return vec![lit(&self.file(at.m))],
            "__dirname" => return vec![parent(&lit(&self.file(at.m)))],
            _ => {}
        }
        // a local of the function, then its parameters
        if let Some(scope) = at.scope {
            let local = walk(scope, &["variable_declarator"]).into_iter().find(|v| v.child_by_field_name("name").map(|n| text(n, src)) == Some(name.as_str()) && v.start_byte() < e.start_byte());
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
            // the language's own conversions to a string
            None if parts.as_slice() == ["String"] => arg(0),
            None if last == "toString" => f.child_by_field_name("object").map(|o| self.of(at, o, depth + 1)).unwrap_or_else(|| vec![hole("value")]),
            None => vec![hole(last)],
        }
    }
}

/// The value a module-level `const` is declared with.
fn top_value<'t>(m: &'t Module, name: &str) -> Option<TsNode<'t>> {
    declarations(m.tree.root_node()).into_iter().filter(|(d, _, _)| matches!(d.kind(), "lexical_declaration" | "variable_declaration")).find_map(|(d, _, _)| {
        named_children(d).into_iter().find(|v| v.kind() == "variable_declarator" && v.child_by_field_name("name").map(|n| text(n, &m.src)) == Some(name))?.child_by_field_name("value")
    })
}

/// One call of a function of the package, with the argument each parameter is handed.
pub(super) struct Arg<'t> {
    pub caller: String,
    pub callee: String,
    pub module: String,
    pub scope: Option<TsNode<'t>>,
    pub params: Vec<String>,
    pub passed: Vec<(String, TsNode<'t>)>,
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
}
