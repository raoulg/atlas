//! Where a Rust file call's file is, and what a statement says: the path or text evaluated from
//! the code, through the constants it names (across `use`), the functions of the program that
//! give it, and the values a function's parameters are handed by its callers.

use std::cell::OnceCell;
use std::collections::HashMap;

use indexmap::IndexMap;
use tree_sitter::Node as TsNode;

use super::module::{def_node, expand, path_parts, resolve, resolve_value, shadowed, Module};
use crate::knowledge::{self, PathFn, PathMethod};
use crate::model::{Node, NodeKind};
use crate::paths::{cross, environment_named, hole, lit, or_else, parent, substitute, union, Built, Seg};
use crate::tree::{beneath, named_children, text, unquote, walk};

const DEPTH: u32 = 12;

/// Evaluates paths and statements in the crates of one workspace.
pub(super) struct Eval<'a> {
    modules: &'a IndexMap<String, Module>,
    /// The program's items, to find the function a value is given by.
    nodes: IndexMap<String, Node>,
    crate_ids: &'a HashMap<String, String>,
    /// The struct literals of every module, by the type each builds: found once, when a field is
    /// first asked for.
    literals: OnceCell<HashMap<String, Vec<(&'a Module, TsNode<'a>)>>>,
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
        Eval { modules, nodes: nodes.clone(), crate_ids, literals: OnceCell::new() }
    }

    /// The values `e` can have.
    pub fn of(&self, at: At<'a, '_>, e: TsNode<'a>, depth: u32) -> Vec<Built> {
        if depth > DEPTH {
            return vec![hole("…")];
        }
        let e = beneath(e, super::module::WRAPPERS);
        let src = &at.m.src;
        let first =
            |e: TsNode<'a>| named_children(e).into_iter().next().map(|x| self.of(at, x, depth + 1)).unwrap_or_else(|| vec![hole("value")]);
        let field = |f: &str| e.child_by_field_name(f).map(|x| self.of(at, x, depth + 1)).unwrap_or_else(|| vec![hole("value")]);
        match e.kind() {
            "string_literal" | "raw_string_literal" => vec![lit(&unquote(text(e, src)))],
            "else_clause" => first(e),
            "identifier" => self.name(at, text(e, src), e.start_byte(), depth),
            "scoped_identifier" => {
                let parts = path_parts(Some(e), src);
                self.constant(at.m, &parts, depth).unwrap_or_else(|| vec![hole(parts.last().map(String::as_str).unwrap_or("value"))])
            }
            "field_expression" => {
                let field = e.child_by_field_name("field").map(|f| text(f, src)).unwrap_or("value");
                let held = match e.child_by_field_name("value") {
                    Some(v) if v.kind() == "self" => self.held(at, field, depth),
                    Some(v) => self.parsed(at, v, depth).and_then(|ty| self.option(&ty, None, field, depth)),
                    None => None,
                };
                held.unwrap_or_else(|| vec![hole(field)])
            }
            "binary_expression" if e.child_by_field_name("operator").map(|o| text(o, src)) == Some("+") => {
                cross(&field("left"), &field("right"), "")
            }
            "call_expression" => self.call(at, e, depth),
            "macro_invocation" => self.macro_value(at, e, depth),
            "closure_expression" => field("body"),
            "block" => tail(e).map(|t| self.of(at, t, depth + 1)).unwrap_or_default(),
            "if_expression" => {
                union(field("consequence"), e.child_by_field_name("alternative").map(|a| self.of(at, a, depth + 1)).unwrap_or_default())
            }
            "match_expression" => {
                let arms = e.child_by_field_name("body").map(|b| walk(b, &["match_arm"])).unwrap_or_default();
                arms.into_iter().filter_map(|a| a.child_by_field_name("value")).fold(vec![], |acc, v| union(acc, self.of(at, v, depth + 1)))
            }
            _ => vec![hole("value")],
        }
    }

    /// A name where it is used at `pos`: what the binding of it in scope there holds (a `let`, a
    /// pattern that takes a value out of `Some` or `Ok`, or a field out of a struct or a variant),
    /// a parameter of the function, a `const` in the function or the module, or one a `use`
    /// brings in.
    fn name(&self, at: At<'a, '_>, name: &str, pos: usize, depth: u32) -> Vec<Built> {
        let src = &at.m.src;
        if let Some(scope) = at.scope {
            match bound(scope, name, pos, src) {
                Some(Bound::Value(v)) => return self.of(at, v, depth + 1),
                Some(Bound::Field(pattern, field, value)) => {
                    return self.taken_apart(at, pattern, &field, value, depth).unwrap_or_else(|| vec![hole(name)])
                }
                None => {}
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
        // a library's code builds none of the program's types
        let own = self.nodes.get(&ty).map_or(true, |n| !n.library);
        for &(m, lit) in self.literals().get(&ty).into_iter().flatten().filter(|(m, _)| !(own && m.library)) {
            let value =
                lit.child_by_field_name("body").map(named_children).unwrap_or_default().into_iter().find_map(|fi| match fi.kind() {
                    "field_initializer" if fi.child_by_field_name("field").map(|f| text(f, &m.src)) == Some(field) => {
                        fi.child_by_field_name("value")
                    }
                    "shorthand_field_initializer" if named_children(fi).first().map(|x| text(*x, &m.src)) == Some(field) => {
                        named_children(fi).into_iter().next()
                    }
                    _ => None,
                });
            let (Some(value), Some(f)) = (value, enclosing_fn(lit)) else { continue };
            let Some(id) = self.fn_id(m, f) else { continue };
            let names = param_names(f, &m.src);
            let vals = self.of(At { m, scope: Some(f), params: &names }, value, depth + 1);
            out = union(
                out,
                vals.into_iter()
                    .map(|b| b.into_iter().map(|s| if let Seg::Param(x) = s { Seg::Held(id.clone(), x) } else { s }).collect())
                    .collect(),
            );
        }
        (!out.is_empty()).then_some(out)
    }

    /// The struct literals of every module, by the type each builds.
    fn literals(&self) -> &HashMap<String, Vec<(&'a Module, TsNode<'a>)>> {
        self.literals.get_or_init(|| {
            let mut out: HashMap<String, Vec<(&'a Module, TsNode<'a>)>> = HashMap::new();
            for m in self.modules.values() {
                for lit in walk(m.tree.root_node(), &["struct_expression"]) {
                    let name = path_parts(lit.child_by_field_name("name"), &m.src);
                    let built = match name.as_slice() {
                        [s] if s == "Self" => self.impl_type(m, lit),
                        _ => resolve(m, &name, self.modules, &self.nodes, self.crate_ids),
                    };
                    if let Some(ty) = built {
                        out.entry(ty).or_default().push((m, lit));
                    }
                }
            }
            out
        })
    }

    /// The type a command-line parser derives (clap's `#[derive(Parser)] struct Cli`) that a value
    /// is, where the function parses the command line into it itself: a call of a function on that
    /// type that the derive defines and the program does not (`Cli::parse()`), reached through the
    /// `let`s that hold it, `?` and `.unwrap()`, and the fields of what was parsed (`cli.cmd`). A
    /// value handed in as a parameter holds what its callers hand.
    fn parsed(&self, at: At<'a, '_>, e: TsNode<'a>, depth: u32) -> Option<String> {
        if depth > DEPTH {
            return None;
        }
        let e = beneath(e, super::module::WRAPPERS);
        let src = &at.m.src;
        match e.kind() {
            "identifier" => {
                let name = text(e, src);
                let named = |p: Option<TsNode>| {
                    p.map_or(false, |p| matches!(p.kind(), "identifier" | "mut_pattern") && text(p, src).trim_start_matches("mut ") == name)
                };
                let local = walk(at.scope?, &["let_declaration"])
                    .into_iter()
                    .rfind(|l| l.end_byte() <= e.start_byte() && named(l.child_by_field_name("pattern")))?;
                self.parsed(at, local.child_by_field_name("value")?, depth + 1)
            }
            "field_expression" => {
                let ty = self.parsed(at, e.child_by_field_name("value")?, depth + 1)?;
                let (n, field) = (self.nodes.get(&ty)?, text(e.child_by_field_name("field")?, src));
                let home = self.modules.get(&n.module)?;
                let fields = def_node(home, n)?.child_by_field_name("body").map(named_children).unwrap_or_default();
                let declared = fields.into_iter().find(|d| {
                    d.kind() == "field_declaration" && d.child_by_field_name("name").map(|x| text(x, &home.src)) == Some(field)
                })?;
                self.deriving(home, &path_parts(declared.child_by_field_name("type"), &home.src))
            }
            "call_expression" => {
                let f = e.child_by_field_name("function")?;
                if f.kind() == "field_expression" {
                    let method = text(f.child_by_field_name("field")?, src);
                    return knowledge::hands_on(method).then(|| self.parsed(at, f.child_by_field_name("value")?, depth + 1)).flatten();
                }
                let parts = path_parts(Some(f), src);
                let own = resolve_value(at.m, &parts, self.modules, &self.nodes, self.crate_ids)
                    .and_then(|id| self.nodes.get(&id))
                    .map_or(false, |n| matches!(n.kind.tag(), NodeKind::Function | NodeKind::Method));
                if own {
                    return None;
                }
                self.deriving(at.m, parts.split_last()?.1)
            }
            _ => None,
        }
    }

    /// The program's type a path names, when a command-line parser derives it.
    fn deriving(&self, m: &Module, parts: &[String]) -> Option<String> {
        let ty = resolve(m, parts, self.modules, &self.nodes, self.crate_ids)?;
        let n = self.nodes.get(&ty).filter(|n| n.kind == NodeKind::Class)?;
        let home = self.modules.get(&n.module)?;
        derived_parser(home, def_node(home, n)?).map(|_| ty)
    }

    /// A field of what a command-line parser derived from the command line (clap's
    /// `#[derive(Parser)]`), of a struct or of a variant of an enum (`#[derive(Subcommand)] enum
    /// Cmd`): the option, holding what the program runs with when the command line leaves it out,
    /// the default its attribute declares (`#[arg(long, default_value = RAW)]`), else no value the
    /// code says.
    fn option(&self, ty: &str, variant: Option<&str>, field: &str, depth: u32) -> Option<Vec<Built>> {
        let n = self.nodes.get(ty)?;
        let home = self.modules.get(&n.module)?;
        let item = def_node(home, n)?;
        let parser = derived_parser(home, item)?;
        let src = &home.src;
        let body = match variant {
            None => item.child_by_field_name("body")?,
            Some(v) => walk(item, &["enum_variant"])
                .into_iter()
                .find(|x| x.child_by_field_name("name").map(|n| text(n, src)) == Some(v))?
                .child_by_field_name("body")?,
        };
        let declared = named_children(body)
            .into_iter()
            .find(|d| d.kind() == "field_declaration" && d.child_by_field_name("name").map(|x| text(x, src)) == Some(field))?;
        Some(crate::paths::option(field, self.default(home, declared, parser, depth)))
    }

    /// The options of the command line a call parses, where it parses the one the program is run
    /// with: a function the derive of a command-line parser defines that reads that command line
    /// (`Cli::parse()`), on a type of the program that derives it.
    pub fn command_line(&self, m: &Module, call: TsNode) -> Option<Vec<String>> {
        let parts = path_parts(call.child_by_field_name("function"), &m.src);
        let (function, ty) = parts.split_last()?;
        let ty = self.deriving(m, ty)?;
        let n = self.nodes.get(&ty)?;
        let home = self.modules.get(&n.module)?;
        let parser = derived_parser(home, def_node(home, n)?)?;
        parser.reads.contains(&function.as_str()).then(|| self.options(&ty, 0))
    }

    /// The options a type a command-line parser derives declares, in the order of its fields, each
    /// as the human types it: a field whose attribute names it long or short is an option
    /// (`#[arg(short, long)] out` is `--out`), any other a positional argument (`<out>`). A
    /// subcommand is no option of its own, and a flattened field adds the options of its type.
    fn options(&self, ty: &str, depth: u32) -> Vec<String> {
        let Some(n) = self.nodes.get(ty) else { return vec![] };
        let Some(home) = self.modules.get(&n.module) else { return vec![] };
        let Some(item) = def_node(home, n) else { return vec![] };
        let Some(parser) = derived_parser(home, item).filter(|_| depth <= DEPTH) else { return vec![] };
        let src = &home.src;
        let fields = item.child_by_field_name("body").map(named_children).unwrap_or_default();
        let mut out = vec![];
        for d in fields.into_iter().filter(|d| d.kind() == "field_declaration") {
            let Some(name) = d.child_by_field_name("name").map(|x| text(x, src).to_string()) else { continue };
            let groups: Vec<Vec<TsNode>> = attributes(d)
                .into_iter()
                .filter(|a| attribute_name(*a, src).map_or(false, |n| parser.attributes.contains(&n)))
                .flat_map(|a| a.child_by_field_name("arguments").map(token_groups).unwrap_or_default())
                .collect();
            let key = |g: &Vec<TsNode>| g.first().map(|k| text(*k, src)).unwrap_or("");
            if groups.iter().any(|g| parser.not_options.contains(&key(g))) {
                continue;
            }
            if groups.iter().any(|g| key(g) == parser.flatten) {
                let field_type = path_parts(d.child_by_field_name("type"), src);
                out.extend(self.deriving(home, &field_type).map(|t| self.options(&t, depth + 1)).unwrap_or_default());
                continue;
            }
            // the flag a key names: its value when it is handed one, else the field's name
            let named = |which: &str, dashes: &str, own: String| {
                groups.iter().find(|g| key(g) == which).map(|g| match g.as_slice() {
                    [_, eq, v, ..] if eq.kind() == "=" => format!("{dashes}{}", unquote(text(*v, src)).trim_matches('\'')),
                    _ => format!("{dashes}{own}"),
                })
            };
            let flags: Vec<String> =
                [named(parser.short, "-", name.chars().take(1).collect()), named(parser.long, "--", name.replace('_', "-"))]
                    .into_iter()
                    .flatten()
                    .collect();
            out.push(if flags.is_empty() { crate::entries::argument(&name) } else { crate::entries::flag(&flags, &name) });
        }
        out
    }

    /// The field a struct pattern takes out of `value`, what it matches (`Cmd::Analyze { out, .. }`
    /// in a `match` on `cli.cmd`), where the code says what that field holds: an option, of what
    /// the function parsed from the command line.
    fn taken_apart(&self, at: At<'a, '_>, pattern: TsNode<'a>, field: &str, value: TsNode<'a>, depth: u32) -> Option<Vec<Built>> {
        self.parsed(at, value, depth)?;
        let parts = path_parts(pattern.child_by_field_name("type"), &at.m.src);
        if let Some(ty) = self.deriving(at.m, &parts) {
            return self.option(&ty, None, field, depth);
        }
        let (variant, owner) = parts.split_last()?;
        self.option(&self.deriving(at.m, owner)?, Some(variant), field, depth)
    }

    /// The default an option's attribute declares, in the module that declares it.
    fn default(&self, home: &'a Module, declared: TsNode<'a>, parser: &knowledge::DerivedParser, depth: u32) -> Vec<Built> {
        let src = &home.src;
        for a in attributes(declared).into_iter().filter(|a| attribute_name(*a, src).map_or(false, |n| parser.attributes.contains(&n))) {
            for g in a.child_by_field_name("arguments").map(token_groups).unwrap_or_default() {
                if let [key, eq, value @ ..] = g.as_slice() {
                    if eq.kind() == "=" && parser.defaults.contains(&text(*key, src)) && !value.is_empty() {
                        return self.tokens(At { m: home, scope: None, params: &[] }, value, depth);
                    }
                }
            }
        }
        vec![]
    }

    /// The type of the `impl` a node is written in.
    fn impl_type(&self, m: &'a Module, n: TsNode<'a>) -> Option<String> {
        let mut p = n.parent();
        while let Some(x) = p.filter(|x| x.kind() != "impl_item") {
            p = x.parent();
        }
        resolve(m, &path_parts(p?.child_by_field_name("type"), &m.src), self.modules, &self.nodes, self.crate_ids)
            .filter(|r| !r.starts_with("ext:"))
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
        let args: Vec<TsNode<'a>> = c
            .child_by_field_name("arguments")
            .map(named_children)
            .unwrap_or_default()
            .into_iter()
            .filter(|a| !a.kind().ends_with("comment"))
            .collect();
        let arg = |i: usize| args.get(i).map(|a| self.of(at, *a, depth + 1)).unwrap_or_else(|| vec![hole("value")]);
        if f.kind() == "field_expression" {
            let (Some(recv), Some(field)) = (f.child_by_field_name("value"), f.child_by_field_name("field")) else {
                return vec![hole("call")];
            };
            let method = text(field, src);
            let on = || self.of(at, recv, depth + 1);
            return match knowledge::RUST_PATH_METHODS.iter().find(|(m, _)| *m == method).map(|(_, k)| *k) {
                Some(PathMethod::Join) => cross(&on(), &arg(0), "/"),
                Some(PathMethod::Parent) => on().iter().map(parent).collect(),
                Some(PathMethod::Same) => on(),
                Some(PathMethod::Extension) => {
                    on().into_iter().flat_map(|b| arg(0).into_iter().map(move |x| with_extension(&b, &x))).collect()
                }
                Some(PathMethod::Sibling) => cross(&on().iter().map(parent).collect::<Vec<_>>(), &arg(0), "/"),
                // what an `Option` or a `Result` holds, or what it falls back to where it holds nothing
                None if super::module::OR_ELSE.contains(&method) && !method.starts_with("map") => or_else(on(), arg(0)),
                None if knowledge::hands_on(method) && !matches!(method, "map" | "and_then") => on(),
                None => vec![hole(method)],
            };
        }
        let parts = expand(at.m, &path_parts(Some(f), src));
        // `Some(x)` and `Ok(x)` hold `x`
        if matches!(parts.as_slice(), [v] if super::module::HOLDING.contains(&v.as_str())) {
            return arg(0);
        }
        let n = parts.len();
        let known = knowledge::RUST_PATH_FUNCTIONS
            .iter()
            .find(|(on, name, _)| n >= 2 && parts[n - 2] == *on && parts[n - 1] == *name)
            .map(|(_, _, k)| *k);
        match known {
            Some(PathFn::Same) => arg(0),
            Some(PathFn::Join) => args.iter().fold(vec![vec![]], |acc: Vec<Built>, a| {
                if acc == vec![Vec::<Seg>::new()] {
                    self.of(at, *a, depth + 1)
                } else {
                    cross(&acc, &self.of(at, *a, depth + 1), "/")
                }
            }),
            Some(PathFn::Parent) => arg(0).iter().map(parent).collect(),
            Some(PathFn::Home) => vec![lit("~")],
            Some(PathFn::Here) => vec![lit(".")],
            Some(PathFn::Temp) => vec![lit(knowledge::TEMP_DIR)],
            Some(PathFn::Environment) => {
                vec![environment_named(&arg(0)).unwrap_or_else(|| hole(parts.last().map(String::as_str).unwrap_or("call")))]
            }
            None => self.given(at, f, &args, depth).unwrap_or_else(|| vec![hole(parts.last().map(String::as_str).unwrap_or("call"))]),
        }
    }

    /// What one of the program's functions gives back, its parameters bound to what the call
    /// hands them; none where the path `f` names a local of the body.
    fn given(&self, at: At<'a, '_>, f: TsNode<'a>, args: &[TsNode<'a>], depth: u32) -> Option<Vec<Built>> {
        let parts = &path_parts(Some(f), &at.m.src);
        if shadowed(at.scope, f, parts, &at.m.src) {
            return None;
        }
        let id = resolve_value(at.m, parts, self.modules, &self.nodes, self.crate_ids).filter(|id| !id.starts_with("ext:"))?;
        let n = self.nodes.get(&id).filter(|n| matches!(n.kind.tag(), NodeKind::Function | NodeKind::Method))?;
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
        let groups = token_groups(tt);
        // the directory the crate's manifest is in, fixed where the program is built
        let built_in = |var: &str| matches!(groups.first().map(Vec::as_slice), Some([x]) if unquote(text(*x, src)) == var);
        match name.as_str() {
            "format" | "format_args" => self.format(at, mac.start_byte(), &groups, depth),
            "concat" => groups.iter().fold(vec![vec![]], |acc, g| cross(&acc, &self.tokens(at, g, depth), "")),
            m if knowledge::RUST_BUILD_DIRS.iter().any(|(mac, var)| *mac == m && built_in(var)) => vec![lit(&at.m.manifest_dir)],
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
            [s, dot, field] if text(*s, src) == "self" && dot.kind() == "." && field.kind() == "identifier" => {
                self.held(at, text(*field, src), depth).unwrap_or_else(|| vec![hole(text(*field, src))])
            }
            [x, dot, m, call]
                if x.kind() == "identifier"
                    && dot.kind() == "."
                    && call.kind() == "token_tree"
                    && knowledge::RUST_PATH_METHODS.iter().any(|(n, k)| *n == text(*m, src) && *k == PathMethod::Same) =>
            {
                self.of(at, *x, depth + 1)
            }
            _ => vec![hole(g.iter().find(|t| t.kind() == "identifier").map(|t| text(*t, src)).unwrap_or("value"))],
        }
    }
}

/// The arguments among a token tree's tokens: the tokens between its commas, inside its
/// delimiters.
pub(super) fn token_groups(tt: TsNode) -> Vec<Vec<TsNode>> {
    let mut cur = tt.walk();
    let tokens: Vec<TsNode> = tt.children(&mut cur).collect();
    let mut groups: Vec<Vec<TsNode>> = vec![vec![]];
    for t in tokens.iter().skip(1).take(tokens.len().saturating_sub(2)).filter(|t| !t.kind().ends_with("comment")) {
        if t.kind() == "," {
            groups.push(vec![]);
        } else if let Some(g) = groups.last_mut() {
            g.push(*t);
        }
    }
    groups.retain(|g| !g.is_empty());
    groups
}

/// The attributes written before an item or a field (`#[derive(..)]`, `#[arg(..)]`), past its
/// comments.
fn attributes(item: TsNode) -> Vec<TsNode> {
    let mut out = vec![];
    let mut p = item.prev_named_sibling();
    while let Some(x) = p.filter(|x| matches!(x.kind(), "attribute_item" | "line_comment" | "block_comment")) {
        out.extend(named_children(x).into_iter().filter(|a| a.kind() == "attribute"));
        p = x.prev_named_sibling();
    }
    out
}

/// The name an attribute is written with: `derive` of `#[derive(Parser)]`.
fn attribute_name<'s>(a: TsNode, src: &'s [u8]) -> Option<&'s str> {
    named_children(a).into_iter().next().filter(|n| n.kind() == "identifier").map(|n| text(n, src))
}

/// The command-line parser a type derives (`#[derive(Parser)]` with clap's `Parser` in scope, or
/// `#[derive(clap::Parser)]`).
fn derived_parser(m: &Module, item: TsNode) -> Option<&'static knowledge::DerivedParser> {
    let src = &m.src;
    let derives = attributes(item)
        .into_iter()
        .filter(|a| attribute_name(*a, src) == Some("derive"))
        .filter_map(|a| a.child_by_field_name("arguments"));
    let paths: Vec<Vec<String>> = derives
        .flat_map(token_groups)
        .map(|g| g.into_iter().filter(|t| t.kind() == "identifier").map(|t| text(t, src).to_string()).collect::<Vec<_>>())
        .map(|parts| expand(m, &parts))
        .collect();
    knowledge::DERIVED_PARSERS.iter().find(|p| {
        paths
            .iter()
            .any(|path| matches!(path.as_slice(), [package, .., derive] if package == p.package && p.derives.contains(&derive.as_str())))
    })
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

/// What a binding of a name holds: a value, or the field a struct pattern takes out of the value
/// it matches.
enum Bound<'a> {
    Value(TsNode<'a>),
    Field(TsNode<'a>, String, TsNode<'a>),
}

/// What a name holds where the code at `pos` reads it, by the binding of it in scope there that
/// is written last: a `let` complete before `pos`, or a pattern (`if let` in the block it opens,
/// `let .. else` after it, a `match` arm in its arm) that binds the whole value, takes the value
/// `Some` or `Ok` holds out of it (`if let Some(dir) = to.parent()`), which holds what that one
/// holds, or takes a field out of a struct or a variant (`Cmd::Analyze { out, .. }`).
fn bound<'a>(scope: TsNode<'a>, name: &str, pos: usize, src: &[u8]) -> Option<Bound<'a>> {
    let named = |p: TsNode| matches!(p.kind(), "identifier" | "mut_pattern") && text(p, src).trim_start_matches("mut ") == name;
    let binds = |p: TsNode<'a>, v: TsNode<'a>| -> Option<Bound<'a>> {
        if named(p) {
            return Some(Bound::Value(v));
        }
        let ty = p.child_by_field_name("type");
        let inner: Vec<TsNode> = named_children(p).into_iter().filter(|c| Some(c.id()) != ty.map(|t| t.id())).collect();
        match p.kind() {
            "tuple_struct_pattern"
                if ty.map_or(false, |t| super::module::HOLDING.contains(&text(t, src))) && matches!(inner.as_slice(), [x] if named(*x)) =>
            {
                Some(Bound::Value(v))
            }
            "struct_pattern" => inner.into_iter().filter(|f| f.kind() == "field_pattern").find_map(|f| {
                let field = text(f.child_by_field_name("name")?, src).to_string();
                let held = f.child_by_field_name("pattern").map_or(field == name, named);
                held.then_some(Bound::Field(p, field, v))
            }),
            _ => None,
        }
    };
    let around = |n: Option<TsNode>| n.map_or(false, |n| n.start_byte() <= pos && pos < n.end_byte());
    let mut best: Option<(usize, Bound<'a>)> = None;
    for b in walk(scope, &["let_declaration", "let_condition", "match_arm"]) {
        let found = match b.kind() {
            "let_declaration" => {
                let (Some(p), Some(v)) = (b.child_by_field_name("pattern"), b.child_by_field_name("value")) else { continue };
                (b.end_byte() <= pos).then(|| binds(p, v)).flatten().map(|x| (b.end_byte(), x))
            }
            "let_condition" => {
                let (Some(p), Some(v)) = (b.child_by_field_name("pattern"), b.child_by_field_name("value")) else { continue };
                let mut opens = b.parent();
                while let Some(x) = opens.filter(|x| !matches!(x.kind(), "if_expression" | "while_expression")) {
                    opens = x.parent();
                }
                let block = opens.and_then(|x| x.child_by_field_name("consequence").or_else(|| x.child_by_field_name("body")));
                around(block).then(|| binds(p, v)).flatten().map(|x| (p.end_byte(), x))
            }
            _ => {
                let Some(p) = b.child_by_field_name("pattern").and_then(|m| named_children(m).into_iter().next()) else { continue };
                let Some(v) = b.parent().and_then(|arms| arms.parent()).and_then(|m| m.child_by_field_name("value")) else { continue };
                around(b.child_by_field_name("value")).then(|| binds(p, v)).flatten().map(|x| (p.end_byte(), x))
            }
        };
        if let Some((end, x)) = found.filter(|(end, _)| best.as_ref().map_or(true, |(e, _)| end >= e)) {
            best = Some((end, x));
        }
    }
    best.map(|(_, x)| x)
}

/// The value a `const` or `static` of this name is declared with, directly under `n`.
fn const_in<'a>(n: TsNode<'a>, name: &str, src: &[u8]) -> Option<TsNode<'a>> {
    walk(n, &["const_item", "static_item"])
        .into_iter()
        .find(|i| i.child_by_field_name("name").map(|x| text(x, src)) == Some(name))?
        .child_by_field_name("value")
}

/// The expression a block ends in, which is what it gives.
fn tail(block: TsNode) -> Option<TsNode> {
    let last = named_children(block).into_iter().rev().find(|x| !x.kind().ends_with("comment"))?;
    match last.kind() {
        "expression_statement" if last.child(last.child_count().saturating_sub(1)).map_or(false, |x| x.kind() != ";") => {
            named_children(last).into_iter().next()
        }
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
    let mut out: Vec<TsNode> =
        walk(body, &["return_expression"]).into_iter().filter(own).filter_map(|r| named_children(r).into_iter().next()).collect();
    out.extend(tail(body));
    out
}

/// A path with another extension: `page.json` with `html` is `page.html`.
fn with_extension(b: &Built, ext: &Built) -> Built {
    let [Seg::Lit(ext)] = ext.as_slice() else { return b.clone() };
    let mut out = b.clone();
    if let Some(s) = crate::paths::last_lit(&mut out) {
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
