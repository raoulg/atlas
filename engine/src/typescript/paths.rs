//! Where a TypeScript file call's file is, and where a request goes: the path or URL evaluated
//! from the code, through the constants it names (across imports), and through the values a
//! function's parameters are handed by its callers.

use std::path::Path;

use indexmap::IndexMap;
use tree_sitter::Node as TsNode;

use super::{declarations, function_in, member_parts, Bound, Module};
use crate::paths::{cross, hole, lit, parent, union, Built, Seg};
use crate::tree::{named_children, path_str, text, unquote, walk};

const DEPTH: u32 = 12;
/// The most URLs a request is followed to, over all the call sites that build one.
const URLS: usize = 64;

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
        match (pkg.as_str(), last) {
            ("path", "join" | "resolve") => args.iter().fold(vec![vec![]], |acc: Vec<Built>, a| {
                let next = self.of(at, *a, depth + 1);
                if acc == vec![Vec::<Seg>::new()] {
                    next
                } else {
                    cross(&acc, &next, "/")
                }
            }),
            ("path", "dirname") => arg(0).iter().map(parent).collect(),
            ("url", "fileURLToPath") => arg(0),
            ("os", "homedir") => vec![lit("~")],
            _ if parts.as_slice() == ["process", "cwd"] => vec![lit(".")],
            _ if parts.as_slice() == ["String"] => arg(0),
            _ if last == "toString" => f.child_by_field_name("object").map(|o| self.of(at, o, depth + 1)).unwrap_or_else(|| vec![hole("value")]),
            _ => vec![hole(last)],
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

/// What a value built from the parameters of `f` is at each call that hands them, followed
/// out through the callers' parameters in turn, with the function where it is complete: the
/// URL `request(method, path)` fetches is `/api/customers` where `api.get("/customers")` is
/// called.
pub(super) fn at_callers(eval: &Eval, args: &[Arg], f: &str, b: &Built, depth: u32) -> Vec<(String, Built)> {
    if !b.iter().any(|s| matches!(s, Seg::Param(_))) {
        return vec![(f.to_string(), b.clone())];
    }
    let sites: Vec<&Arg> = args.iter().filter(|a| a.callee == f).collect();
    // nobody in the package calls `f`: its parameters are whatever its users hand it
    if sites.is_empty() || depth > DEPTH {
        return vec![(f.to_string(), b.iter().map(|s| if let Seg::Param(p) = s { Seg::Hole(p.clone()) } else { s.clone() }).collect())];
    }
    let mut out = vec![];
    for a in sites {
        let Some(m) = eval.modules.get(&a.module) else { continue };
        let at = At { m, scope: a.scope, params: &a.params };
        let mut built: Vec<Built> = vec![vec![]];
        for s in b {
            let piece = match s {
                Seg::Param(p) => match a.passed.iter().find(|(q, _)| q == p) {
                    Some((_, node)) => eval.of(at, *node, 0),
                    None => vec![hole(p)],
                },
                s => vec![vec![s.clone()]],
            };
            built = cross(&built, &piece, "");
        }
        for x in built {
            out.extend(at_callers(eval, args, &a.caller, &x, depth + 1));
            if out.len() >= URLS {
                return out;
            }
        }
    }
    out
}
