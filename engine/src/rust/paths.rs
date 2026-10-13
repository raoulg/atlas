//! Where a Rust file call's file is, and what a statement says: the path or text evaluated from
//! the code, through the constants it names (across `use`), the functions of the program that
//! give it, and the values a function's parameters are handed by its callers.

use std::collections::HashMap;

use indexmap::IndexMap;
use tree_sitter::Node as TsNode;

use super::{def_node, expand, path_parts, resolve, Module};
use crate::knowledge::{self, PathFn, PathMethod};
use crate::model::{Node, NodeKind};
use crate::paths::{cross, hole, lit, parent, substitute, union, Built, Seg};
use crate::tree::{named_children, text, unquote, walk};

const DEPTH: u32 = 12;

/// Evaluates paths and statements in the crates of one workspace.
pub(super) struct Eval<'a> {
    modules: &'a IndexMap<String, Module>,
    /// The program's items, to find the function a value is given by.
    nodes: IndexMap<String, Node>,
    crate_ids: &'a HashMap<String, String>,
}

/// Where an expression is evaluated: its module, and the function it is in with that function's
/// parameters.
#[derive(Clone, Copy)]
pub(super) struct At<'a, 'p> {
    pub m: &'a Module,
    pub scope: Option<TsNode<'a>>,
    pub params: &'p [String],
}

/// A function's parameters by name, in order, without `self`; one bound by a pattern has none.
pub(super) fn param_names(f: TsNode, src: &[u8]) -> Vec<String> {
    let Some(ps) = f.child_by_field_name("parameters") else { return vec![] };
    named_children(ps)
        .into_iter()
        .filter(|p| p.kind() == "parameter")
        .map(|p| match p.child_by_field_name("pattern") {
            Some(x) if matches!(x.kind(), "identifier" | "mut_pattern") => text(x, src).trim_start_matches("mut ").to_string(),
            _ => String::new(),
        })
        .collect()
}

/// Whether a function takes `self`.
pub(super) fn takes_self(f: TsNode) -> bool {
    f.child_by_field_name("parameters").map_or(false, |ps| named_children(ps).iter().any(|p| p.kind() == "self_parameter"))
}

impl<'a> Eval<'a> {
    pub fn new(modules: &'a IndexMap<String, Module>, nodes: &IndexMap<String, Node>, crate_ids: &'a HashMap<String, String>) -> Self {
        Eval { modules, nodes: nodes.clone(), crate_ids }
    }

    /// The values `e` can have.
    pub fn of(&self, at: At<'a, '_>, e: TsNode<'a>, depth: u32) -> Vec<Built> {
        if depth > DEPTH {
            return vec![hole("…")];
        }
        let src = &at.m.src;
        let first = |e: TsNode<'a>| named_children(e).into_iter().next().map(|x| self.of(at, x, depth + 1)).unwrap_or_else(|| vec![hole("value")]);
        let field = |f: &str| e.child_by_field_name(f).map(|x| self.of(at, x, depth + 1)).unwrap_or_else(|| vec![hole("value")]);
        match e.kind() {
            "string_literal" | "raw_string_literal" => vec![lit(&unquote(text(e, src)))],
            "reference_expression" => field("value"),
            "parenthesized_expression" | "try_expression" | "await_expression" | "else_clause" => first(e),
            "identifier" => self.name(at, text(e, src), e.start_byte(), depth),
            "scoped_identifier" => {
                let parts = path_parts(Some(e), src);
                self.constant(at.m, &parts, depth).unwrap_or_else(|| vec![hole(parts.last().map(String::as_str).unwrap_or("value"))])
            }
            "field_expression" => {
                let field = e.child_by_field_name("field").map(|f| text(f, src)).unwrap_or("value");
                let on_self = e.child_by_field_name("value").map_or(false, |v| v.kind() == "self");
                on_self.then(|| self.held(at, field, depth)).flatten().unwrap_or_else(|| vec![hole(field)])
            }
            "binary_expression" if e.child_by_field_name("operator").map(|o| text(o, src)) == Some("+") => cross(&field("left"), &field("right"), ""),
            "call_expression" => self.call(at, e, depth),
            "macro_invocation" => self.macro_value(at, e, depth),
            "closure_expression" => field("body"),
            "block" => tail(e).map(|t| self.of(at, t, depth + 1)).unwrap_or_default(),
            "if_expression" => union(field("consequence"), e.child_by_field_name("alternative").map(|a| self.of(at, a, depth + 1)).unwrap_or_default()),
            "match_expression" => {
                let arms = e.child_by_field_name("body").map(|b| walk(b, &["match_arm"])).unwrap_or_default();
                arms.into_iter().filter_map(|a| a.child_by_field_name("value")).fold(vec![], |acc, v| union(acc, self.of(at, v, depth + 1)))
            }
            _ => vec![hole("value")],
        }
    }

    /// A name where it is used at `pos`: the last `let` of it complete before that, a parameter
    /// of the function, a `const` in the function or the module, or one a `use` brings in.
    fn name(&self, at: At<'a, '_>, name: &str, pos: usize, depth: u32) -> Vec<Built> {
        let src = &at.m.src;
        if let Some(scope) = at.scope {
            let local = walk(scope, &["let_declaration"])
                .into_iter()
                .rfind(|l| l.end_byte() <= pos && l.child_by_field_name("pattern").map_or(false, |p| matches!(p.kind(), "identifier" | "mut_pattern") && text(p, src).trim_start_matches("mut ") == name));
            if let Some(v) = local.and_then(|l| l.child_by_field_name("value")) {
                return self.of(at, v, depth + 1);
            }
            if at.params.iter().any(|p| p == name) {
                return vec![vec![Seg::Param(name.to_string())]];
            }
            if let Some(v) = const_in(scope, name, src) {
                return self.of(at, v, depth + 1);
            }
        }
        self.constant(at.m, &[name.to_string()], depth).unwrap_or_else(|| vec![hole(name)])
    }

    /// The value of the `const` or `static` a path names: in the module, where its `use` leads,
    /// or in the module the path names.
    fn constant(&self, m: &'a Module, parts: &[String], depth: u32) -> Option<Vec<Built>> {
        let (home, name): (&'a Module, &str) = match parts {
            [name] => match m.bindings.get(name).filter(|b| !b.starts_with("ext:")).and_then(|b| b.rsplit_once('.')) {
                Some((mid, n)) => (self.modules.get(mid)?, n),
                None => (m, name.as_str()),
            },
            [path @ .., name] => {
                let mid = resolve(m, path, self.modules, &self.nodes, self.crate_ids).filter(|r| !r.starts_with("ext:"))?;
                (self.modules.get(&mid)?, name.as_str())
            }
            [] => return None,
        };
        let v = const_in(home.tree.root_node(), name, &home.src)?;
        Some(self.of(At { m: home, scope: None, params: &[] }, v, depth + 1))
    }

    /// A field of the value a method runs on, as the struct literals that build that type set it
    /// (`Self { root }` in `new(root)`), from what the functions they are in are handed.
    fn held(&self, at: At<'a, '_>, field: &str, depth: u32) -> Option<Vec<Built>> {
        let ty = self.impl_type(at.m, at.scope?)?;
        let mut out = vec![];
        for m in self.modules.values() {
            for lit in walk(m.tree.root_node(), &["struct_expression"]) {
                let name = path_parts(lit.child_by_field_name("name"), &m.src);
                let built = match name.as_slice() {
                    [s] if s == "Self" => self.impl_type(m, lit),
                    _ => resolve(m, &name, self.modules, &self.nodes, self.crate_ids),
                };
                if built.as_deref() != Some(ty.as_str()) {
                    continue;
                }
                let value = lit.child_by_field_name("body").map(named_children).unwrap_or_default().into_iter().find_map(|fi| match fi.kind() {
                    "field_initializer" if fi.child_by_field_name("field").map(|f| text(f, &m.src)) == Some(field) => fi.child_by_field_name("value"),
                    "shorthand_field_initializer" if named_children(fi).first().map(|x| text(*x, &m.src)) == Some(field) => named_children(fi).into_iter().next(),
                    _ => None,
                });
                let (Some(value), Some(f)) = (value, enclosing_fn(lit)) else { continue };
                let Some(id) = self.fn_id(m, f) else { continue };
                let names = param_names(f, &m.src);
                let vals = self.of(At { m, scope: Some(f), params: &names }, value, depth + 1);
                out = union(out, vals.into_iter().map(|b| b.into_iter().map(|s| if let Seg::Param(x) = s { Seg::Held(id.clone(), x) } else { s }).collect()).collect());
            }
        }
        (!out.is_empty()).then_some(out)
    }

    /// The type of the `impl` a node is written in.
    fn impl_type(&self, m: &'a Module, n: TsNode<'a>) -> Option<String> {
        let mut p = n.parent();
        while let Some(x) = p.filter(|x| x.kind() != "impl_item") {
            p = x.parent();
        }
        resolve(m, &path_parts(p?.child_by_field_name("type"), &m.src), self.modules, &self.nodes, self.crate_ids).filter(|r| !r.starts_with("ext:"))
    }

    /// The id the graph knows a function by: its type's and its name for a method, its module's
    /// and its name for a free function; none for one inside another function.
    fn fn_id(&self, m: &'a Module, f: TsNode<'a>) -> Option<String> {
        let name = text(f.child_by_field_name("name")?, &m.src);
        if enclosing_fn(f).is_some() {
            return None;
        }
        Some(match self.impl_type(m, f) {
            Some(ty) => format!("{ty}.{name}"),
            None => format!("{}.{name}", m.id),
        })
    }

    fn call(&self, at: At<'a, '_>, c: TsNode<'a>, depth: u32) -> Vec<Built> {
        let src = &at.m.src;
        let Some(f) = c.child_by_field_name("function") else { return vec![hole("call")] };
        let args: Vec<TsNode<'a>> = c.child_by_field_name("arguments").map(named_children).unwrap_or_default().into_iter().filter(|a| !a.kind().ends_with("comment")).collect();
        let arg = |i: usize| args.get(i).map(|a| self.of(at, *a, depth + 1)).unwrap_or_else(|| vec![hole("value")]);
        if f.kind() == "field_expression" {
            let (Some(recv), Some(field)) = (f.child_by_field_name("value"), f.child_by_field_name("field")) else { return vec![hole("call")] };
            let method = text(field, src);
            let on = || self.of(at, recv, depth + 1);
            return match knowledge::RUST_PATH_METHODS.iter().find(|(m, _)| *m == method).map(|(_, k)| *k) {
                Some(PathMethod::Join) => cross(&on(), &arg(0), "/"),
                Some(PathMethod::Parent) => on().iter().map(parent).collect(),
                Some(PathMethod::Same) => on(),
                Some(PathMethod::Extension) => on().into_iter().flat_map(|b| arg(0).into_iter().map(move |x| with_extension(&b, &x))).collect(),
                Some(PathMethod::Sibling) => cross(&on().iter().map(parent).collect::<Vec<_>>(), &arg(0), "/"),
                // what an `Option` or a `Result` holds, or what it falls back to
                None if super::OR_ELSE.contains(&method) && !method.starts_with("map") => union(on(), arg(0)),
                None if knowledge::hands_on(method) && !matches!(method, "map" | "and_then") => on(),
                None => vec![hole(method)],
            };
        }
        let parts = expand(at.m, &path_parts(Some(f), src));
        let n = parts.len();
        let known = knowledge::RUST_PATH_FUNCTIONS.iter().find(|(on, name, _)| n >= 2 && parts[n - 2] == *on && parts[n - 1] == *name).map(|(_, _, k)| *k);
        match known {
            Some(PathFn::Same) => arg(0),
            Some(PathFn::Join) => args.iter().fold(vec![vec![]], |acc: Vec<Built>, a| if acc == vec![Vec::<Seg>::new()] { self.of(at, *a, depth + 1) } else { cross(&acc, &self.of(at, *a, depth + 1), "/") }),
            Some(PathFn::Parent) => arg(0).iter().map(parent).collect(),
            Some(PathFn::Home) => vec![lit("~")],
            Some(PathFn::Here) => vec![lit(".")],
            Some(PathFn::Temp) => vec![lit(knowledge::TEMP_DIR)],
            None => self.given(at, &path_parts(Some(f), src), &args, depth).unwrap_or_else(|| vec![hole(parts.last().map(String::as_str).unwrap_or("call"))]),
        }
    }

    /// What one of the program's functions gives back, its parameters bound to what the call
    /// hands them.
    fn given(&self, at: At<'a, '_>, parts: &[String], args: &[TsNode<'a>], depth: u32) -> Option<Vec<Built>> {
        let id = resolve(at.m, parts, self.modules, &self.nodes, self.crate_ids).filter(|id| !id.starts_with("ext:"))?;
        let n = self.nodes.get(&id).filter(|n| matches!(n.kind, NodeKind::Function | NodeKind::Method))?;
        let home = self.modules.get(&n.module)?;
        let def = def_node(home, n)?;
        let names = param_names(def, &home.src);
        let inner = At { m: home, scope: Some(def), params: &names };
        let gives = returns(def.child_by_field_name("body")?).into_iter().fold(vec![], |acc, x| union(acc, self.of(inner, x, depth + 1)));
        if gives.is_empty() {
            return None;
        }
        // a method called by its path is handed its receiver first
        let args = if takes_self(def) { args.get(1..).unwrap_or_default() } else { args };
        let handed: HashMap<&str, Vec<Built>> = names.iter().zip(args).map(|(p, a)| (p.as_str(), self.of(at, *a, depth + 1))).collect();
        Some(gives.iter().flat_map(|b| substitute(b, |p| handed.get(p).cloned().unwrap_or_else(|| vec![hole(p)]))).collect())
    }

    /// `format!` and `concat!`: their text with each argument evaluated in its place.
    fn macro_value(&self, at: At<'a, '_>, mac: TsNode<'a>, depth: u32) -> Vec<Built> {
        let src = &at.m.src;
        let name = mac.child_by_field_name("macro").map(|x| path_parts(Some(x), src)).and_then(|p| p.last().cloned()).unwrap_or_default();
        let Some(tt) = named_children(mac).into_iter().find(|x| x.kind() == "token_tree") else { return vec![hole(&name)] };
        // the arguments: the tokens between the commas, inside the delimiters
        let mut cur = tt.walk();
        let tokens: Vec<TsNode<'a>> = tt.children(&mut cur).collect();
        let mut groups: Vec<Vec<TsNode<'a>>> = vec![vec![]];
        for t in tokens.iter().skip(1).take(tokens.len().saturating_sub(2)) {
            if t.kind() == "," {
                groups.push(vec![]);
            } else if let Some(g) = groups.last_mut() {
                g.push(*t);
            }
        }
        groups.retain(|g| !g.is_empty());
        match name.as_str() {
            "format" | "format_args" => self.format(at, mac.start_byte(), &groups, depth),
            "concat" => groups.iter().fold(vec![vec![]], |acc, g| cross(&acc, &self.tokens(at, g, depth), "")),
            _ => vec![hole(&name)],
        }
    }

    /// A format string with its holes filled: by position, by index, by a named argument, or by
    /// a name the string captures.
    fn format(&self, at: At<'a, '_>, pos: usize, groups: &[Vec<TsNode<'a>>], depth: u32) -> Vec<Built> {
        let src = &at.m.src;
        let Some([fmt]) = groups.first().map(Vec::as_slice) else { return vec![hole("format")] };
        if !matches!(fmt.kind(), "string_literal" | "raw_string_literal") {
            return vec![hole("format")];
        }
        let (mut positional, mut named): (Vec<&[TsNode<'a>]>, HashMap<&str, &[TsNode<'a>]>) = (vec![], HashMap::new());
        for g in &groups[1..] {
            match g.as_slice() {
                [n, eq, rest @ ..] if n.kind() == "identifier" && eq.kind() == "=" && !rest.is_empty() => {
                    named.insert(text(*n, src), rest);
                }
                g => positional.push(g),
            }
        }
        let fmt = unquote(text(*fmt, src));
        let (mut out, mut plain, mut next): (Vec<Built>, String, usize) = (vec![vec![]], String::new(), 0);
        let mut chars = fmt.chars().peekable();
        while let Some(ch) = chars.next() {
            match ch {
                '{' if chars.peek() == Some(&'{') => {
                    chars.next();
                    plain.push('{');
                }
                '}' if chars.peek() == Some(&'}') => {
                    chars.next();
                    plain.push('}');
                }
                '{' => {
                    let spec: String = chars.by_ref().take_while(|c| *c != '}').collect();
                    let key = spec.split(':').next().unwrap_or("").trim();
                    let value = if key.is_empty() {
                        next += 1;
                        positional.get(next - 1).map(|g| self.tokens(at, g, depth))
                    } else if let Ok(i) = key.parse::<usize>() {
                        positional.get(i).map(|g| self.tokens(at, g, depth))
                    } else {
                        Some(named.get(key).map(|g| self.tokens(at, g, depth)).unwrap_or_else(|| self.name(at, key, pos, depth + 1)))
                    };
                    out = cross(&out, &[lit(&std::mem::take(&mut plain))], "");
                    out = cross(&out, &value.unwrap_or_else(|| vec![hole("value")]), "");
                }
                c => plain.push(c),
            }
        }
        cross(&out, &[lit(&plain)], "")
    }

    /// A macro's argument, as tokens: a literal, a name, or a name written another way
    /// (`dir.display()`).
    fn tokens(&self, at: At<'a, '_>, g: &[TsNode<'a>], depth: u32) -> Vec<Built> {
        let src = &at.m.src;
        match g {
            [x] if matches!(x.kind(), "string_literal" | "raw_string_literal" | "identifier") => self.of(at, *x, depth + 1),
            [x] if x.kind() == "integer_literal" => vec![lit(text(*x, src))],
            // a field of the value the method is called on: what its type's struct literals set it to
            [s, dot, field] if text(*s, src) == "self" && dot.kind() == "." && field.kind() == "identifier" => self.held(at, text(*field, src), depth).unwrap_or_else(|| vec![hole(text(*field, src))]),
            [x, dot, m, call] if x.kind() == "identifier" && dot.kind() == "." && call.kind() == "token_tree" && knowledge::RUST_PATH_METHODS.iter().any(|(n, k)| *n == text(*m, src) && *k == PathMethod::Same) => self.of(at, *x, depth + 1),
            _ => vec![hole(g.iter().find(|t| t.kind() == "identifier").map(|t| text(*t, src)).unwrap_or("value"))],
        }
    }
}

/// The function a node is written in.
fn enclosing_fn(n: TsNode) -> Option<TsNode> {
    let mut p = n.parent();
    while let Some(x) = p {
        if x.kind() == "function_item" {
            return Some(x);
        }
        p = x.parent();
    }
    None
}

/// The value a `const` or `static` of this name is declared with, directly under `n`.
fn const_in<'a>(n: TsNode<'a>, name: &str, src: &[u8]) -> Option<TsNode<'a>> {
    walk(n, &["const_item", "static_item"]).into_iter().find(|i| i.child_by_field_name("name").map(|x| text(x, src)) == Some(name))?.child_by_field_name("value")
}

/// The expression a block ends in, which is what it gives.
fn tail(block: TsNode) -> Option<TsNode> {
    let last = named_children(block).into_iter().rev().find(|x| !x.kind().ends_with("comment"))?;
    match last.kind() {
        "expression_statement" if last.child(last.child_count().saturating_sub(1)).map_or(false, |x| x.kind() != ";") => named_children(last).into_iter().next(),
        k if k.ends_with("_statement") || k.ends_with("_item") || k == "let_declaration" => None,
        _ => Some(last),
    }
}

/// What a function's body gives back: what it ends in, and what each `return` in it (not in a
/// closure or a function inside it) gives.
fn returns(body: TsNode) -> Vec<TsNode> {
    let own = |r: &TsNode| {
        let mut p = r.parent();
        while let Some(x) = p.filter(|x| x.id() != body.id()) {
            if matches!(x.kind(), "closure_expression" | "function_item") {
                return false;
            }
            p = x.parent();
        }
        true
    };
    let mut out: Vec<TsNode> = walk(body, &["return_expression"]).into_iter().filter(own).filter_map(|r| named_children(r).into_iter().next()).collect();
    out.extend(tail(body));
    out
}

/// A path with another extension: `page.json` with `html` is `page.html`.
fn with_extension(b: &Built, ext: &Built) -> Built {
    let [Seg::Lit(ext)] = ext.as_slice() else { return b.clone() };
    let mut out = b.clone();
    if let Some(Seg::Lit(s)) = out.last_mut() {
        let name_at = s.rfind('/').map_or(0, |i| i + 1);
        if let Some(dot) = s[name_at..].rfind('.').filter(|d| *d > 0) {
            s.truncate(name_at + dot);
        }
        if !ext.is_empty() {
            s.push('.');
            s.push_str(ext);
        }
    }
    out
}

/// One call of a function of the program, with the argument each parameter is handed.
pub(super) struct Arg<'a> {
    pub caller: String,
    pub callee: String,
    pub module: String,
    pub scope: Option<TsNode<'a>>,
    pub params: Vec<String>,
    pub passed: Vec<(String, TsNode<'a>)>,
}

/// A call with what it hands each parameter, evaluated in the caller.
pub(super) struct Evaluated<'e, 'a> {
    pub arg: &'e Arg<'a>,
    pub eval: &'e Eval<'a>,
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
