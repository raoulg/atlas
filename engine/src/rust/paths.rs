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
use crate::paths::{bound as named_by, cross, each, environment_named, held_by, hole, lit, or_else, parent, substitute, union, Built, Seg};
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
                    Some(v) => self
                        .parsed(at, v, depth)
                        .and_then(|ty| self.option(&ty, None, field, depth))
                        .or_else(|| self.type_of(at, v, depth + 1).and_then(|ty| self.held_in(&ty, field, depth))),
                    None => None,
                };
                held.unwrap_or_else(|| vec![hole(field)])
            }
            "binary_expression" if e.child_by_field_name("operator").map(|o| text(o, src)) == Some("+") => {
                cross(&field("left"), &field("right"), "")
            }
            "call_expression" => self.call(at, e, depth),
            // what leaves (`return`, `break`, `continue`) gives no value where it stands: an arm
            // that leaves adds none to its `match`
            "return_expression" | "break_expression" | "continue_expression" => vec![],
            "index_expression" => {
                let (v, k) = match named_children(e).as_slice() {
                    [v, k] => (*v, *k),
                    _ => return vec![hole("value")],
                };
                self.looked_up(at, v, k, depth).unwrap_or_else(|| vec![hole("value")])
            }
            // an array is the values it holds; iterating it yields one of them
            "array_expression" => elements(e).into_iter().fold(vec![], |acc, x| union(acc, self.of(at, x, depth + 1))),
            "macro_invocation" if self.list_macro(at.m, e) => {
                let groups = named_children(e).into_iter().find(|x| x.kind() == "token_tree").map(token_groups).unwrap_or_default();
                groups.iter().fold(vec![], |acc, g| union(acc, self.tokens(at, g, depth)))
            }
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
                Some(Bound::Value(v)) => {
                    let values = self.changed(at, scope, name, v, pos, self.of(at, v, depth + 1), depth);
                    return named_by(&self.local(at.m, scope, name), values);
                }
                Some(Bound::Handed(args)) => {
                    let values = args.into_iter().fold(vec![], |acc, a| union(acc, self.of(at, a, depth + 1)));
                    return named_by(&self.local(at.m, scope, name), values);
                }
                Some(Bound::Field(pattern, field, value)) => {
                    return self
                        .taken_apart(at, pattern, &field, value, depth)
                        .unwrap_or_else(|| vec![hole(&self.local(at.m, scope, name))])
                }
                Some(Bound::Element(value, i)) => {
                    if let Some(values) = self.element(at, value, i, depth) {
                        return named_by(&self.local(at.m, scope, name), values);
                    }
                }
                // an element of what a loop or a method goes over: one of the values it holds; an
                // element of nothing known leaves the name the code gives it
                Some(Bound::Each(over)) => {
                    let values = self.of(at, over, depth + 1);
                    if !values.iter().all(|b| matches!(b.as_slice(), [Seg::Hole(_)])) {
                        return named_by(&self.local(at.m, scope, name), each(values));
                    }
                }
                None => {}
            }
            if at.params.iter().any(|p| p == name) {
                return vec![vec![Seg::Param(name.to_string())]];
            }
            if let Some(v) = const_in(scope, name, src) {
                return named_by(name, self.of(at, v, depth + 1));
            }
        }
        self.constant(at.m, &[name.to_string()], depth).unwrap_or_else(|| vec![hole(name)])
    }

    /// What the function makes of `values`, which the name holds where `v` binds it, by the changes it makes
    /// to that value before `pos`, in the order they run: a method of a `PathBuf` that changes the path it
    /// holds (knowledge::RUST_PATH_CHANGES), on the value or called by its type's path (`p.push(x)` and
    /// `PathBuf::push(&mut p, x)` are each `p = p.join(x)`), and an assignment of another value
    /// (`p = p.join(x)`). A change that may not run where the binding does (in an arm the binding is not
    /// in) leaves the value one of the two; one the code repeats (in a loop or a closure) runs a number of
    /// times the code does not say, so a join leaves the value below what it was, a change of its
    /// extension or name one of the two, and any other change a value not said.
    fn changed(
        &self,
        at: At<'a, '_>,
        scope: TsNode<'a>,
        name: &str,
        v: TsNode<'a>,
        pos: usize,
        values: Vec<Built>,
        depth: u32,
    ) -> Vec<Built> {
        let src = &at.m.src;
        let this = |n: TsNode<'a>| {
            n.kind() == "identifier"
                && text(n, src) == name
                && matches!(bound(scope, name, n.start_byte(), src), Some(Bound::Value(x)) if x.id() == v.id())
        };
        // a method called on the value, as a method or by its type's path: its name and its other arguments
        let on_this = |c: TsNode<'a>| -> Option<(String, Vec<TsNode<'a>>)> {
            let c = beneath(c, super::module::WRAPPERS);
            let f = c.child_by_field_name("function").filter(|_| c.kind() == "call_expression")?;
            let args: Vec<TsNode<'a>> = c
                .child_by_field_name("arguments")
                .map(named_children)
                .unwrap_or_default()
                .into_iter()
                .filter(|a| !a.kind().ends_with("comment"))
                .collect();
            let (target, method, rest) = match f.kind() {
                "field_expression" => (f.child_by_field_name("value")?, text(f.child_by_field_name("field")?, src).to_string(), args),
                _ => match (expand(at.m, &path_parts(Some(f), src)).as_slice(), args.split_first()) {
                    ([.., ty, m], Some((on, rest))) if knowledge::RUST_PATH_TYPES.contains(&ty.as_str()) => (*on, m.clone(), rest.to_vec()),
                    _ => return None,
                },
            };
            this(beneath(target, super::module::WRAPPERS)).then_some((method, rest))
        };
        let mut out = values;
        for c in walk(scope, &["call_expression", "assignment_expression"]) {
            if c.start_byte() < v.end_byte() || c.end_byte() > pos {
                continue;
            }
            let (made, kind) = if c.kind() == "assignment_expression" {
                match (c.child_by_field_name("left"), c.child_by_field_name("right")) {
                    // `p = p.join(x)` is the change `p.push(x)` is
                    (Some(l), Some(r)) if this(l) => {
                        let kind =
                            on_this(r).and_then(|(m, _)| knowledge::RUST_PATH_METHODS.iter().find(|(n, _)| *n == m).map(|(_, k)| *k));
                        (self.of(at, r, depth + 1), kind)
                    }
                    _ => continue,
                }
            } else {
                let Some((method, rest)) = on_this(c) else { continue };
                let Some(k) = knowledge::RUST_PATH_CHANGES.iter().find(|(m, _)| *m == method).map(|(_, k)| *k) else { continue };
                if !holds_path_buf(at.m, v) {
                    continue;
                }
                let first = rest.first().map(|a| self.of(at, *a, depth + 1)).unwrap_or_else(|| vec![hole("value")]);
                (path_made(k, out.clone(), first), Some(k))
            };
            out = match (runs(c, v), kind) {
                (Runs::Once, _) => made,
                (Runs::Maybe, _) | (Runs::Repeated, Some(PathMethod::Same | PathMethod::Extension | PathMethod::Sibling)) => {
                    union(out, made)
                }
                (Runs::Repeated, Some(PathMethod::Join)) => {
                    // below a value not said is again a value not said
                    let below = out
                        .iter()
                        .filter(|b| !matches!(b.as_slice(), [Seg::Hole(_)]))
                        .map(|b| crate::paths::join(b, &hole("value"), "/"))
                        .collect();
                    union(out, below)
                }
                (Runs::Repeated, _) => vec![hole("value")],
            };
        }
        out
    }

    /// The element `i` of what `value` gives, where the code says it: of a tuple it writes, or of the
    /// pair a socket's `accept` gives, whose first is a door to the listener's place.
    fn element(&self, at: At<'a, '_>, value: TsNode<'a>, i: usize, depth: u32) -> Option<Vec<Built>> {
        let value = beneath(value, super::module::WRAPPERS);
        match value.kind() {
            "tuple_expression" => named_children(value).get(i).map(|x| self.of(at, *x, depth + 1)),
            "call_expression" => {
                let f = value.child_by_field_name("function").filter(|f| f.kind() == "field_expression")?;
                let method = text(f.child_by_field_name("field")?, &at.m.src);
                let on = f.child_by_field_name("value")?;
                knowledge::RUST_DOOR_KEEPERS.contains(&(method, Some(i))).then(|| self.of(at, on, depth + 1))
            }
            _ => None,
        }
    }

    /// The name a value bound in a function goes by beyond it: the binding's with the function's
    /// (`begin.file`, `Settings.of.dir`), as a parameter's is (`analyze.dir`), so that two functions'
    /// `file`s are two values.
    fn local(&self, m: &'a Module, scope: TsNode<'a>, name: &str) -> String {
        let Some(f) = scope.child_by_field_name("name").map(|n| text(n, &m.src)) else { return name.to_string() };
        match self.impl_type(m, scope) {
            Some(ty) => format!("{}.{f}.{name}", ty.rsplit('.').next().unwrap_or(&ty)),
            None => format!("{f}.{name}"),
        }
    }

    /// The value of the `const` or `static` a path names: in the module, where its `use` leads,
    /// or in the module the path names.
    fn constant(&self, m: &'a Module, parts: &[String], depth: u32) -> Option<Vec<Built>> {
        let (home, name, v) = self.constant_value(m, parts)?;
        Some(named_by(&name, self.of(At { m: home, scope: None, params: &[] }, v, depth + 1)))
    }

    /// The `const` or `static` a path names: its module, its name there and the value it is
    /// declared with.
    fn constant_value(&self, m: &'a Module, parts: &[String]) -> Option<(&'a Module, String, TsNode<'a>)> {
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
        Some((home, name.to_string(), const_in(home.tree.root_node(), name, &home.src)?))
    }

    /// `d[k]`, `d.get(k)`: `d` a collection the code writes out where it stands or where `d` is
    /// bound (a map from its pairs, an array, a `vec!`), the value a key the code decides picks,
    /// else one of the values `d` holds, as Python's `d[k]` is. A key the collection does not
    /// write picks none of them, so it is one of them all.
    fn looked_up(&self, at: At<'a, '_>, v: TsNode<'a>, k: TsNode<'a>, depth: u32) -> Option<Vec<Built>> {
        let k = beneath(k, super::module::WRAPPERS);
        let key = match k.kind() {
            "integer_literal" => Some(text(k, &at.m.src).to_string()),
            _ => match self.of(at, k, depth + 1).as_slice() {
                [b] if crate::paths::known(b) => Some(crate::paths::render(b)),
                _ => None,
            },
        };
        let (here, held, scope) = self.written_out(at, v, depth)?;
        let values =
            self.picked(At { m: here, scope, params: if scope == at.scope { at.params } else { &[] } }, held, key.as_deref(), depth);
        (!values.iter().all(|b| matches!(b.as_slice(), [Seg::Hole(_)]))).then_some(values)
    }

    /// The collection a value is, where the code writes it out: where it stands, or where the name
    /// it is read by is bound (a `let`, a `const` or a `static`), with the module and the function
    /// it is written in.
    fn written_out(&self, at: At<'a, '_>, v: TsNode<'a>, depth: u32) -> Option<(&'a Module, TsNode<'a>, Option<TsNode<'a>>)> {
        if depth > DEPTH {
            return None;
        }
        let v = beneath(v, super::module::WRAPPERS);
        let src = &at.m.src;
        match v.kind() {
            "array_expression" => Some((at.m, v, at.scope)),
            "macro_invocation" if self.list_macro(at.m, v) => Some((at.m, v, at.scope)),
            "call_expression" if self.map_from_pairs(at.m, v).is_some() => Some((at.m, v, at.scope)),
            "identifier" => {
                let name = text(v, src);
                if let Some(scope) = at.scope {
                    match bound(scope, name, v.start_byte(), src) {
                        Some(Bound::Value(x)) => return self.written_out(at, x, depth + 1),
                        Some(_) => return None,
                        None if at.params.iter().any(|p| p == name) => return None,
                        None => {}
                    }
                    if let Some(x) = const_in(scope, name, src) {
                        return self.written_out(at, x, depth + 1);
                    }
                }
                let (home, _, x) = self.constant_value(at.m, &[name.to_string()])?;
                self.written_out(At { m: home, scope: None, params: &[] }, x, depth + 1)
            }
            "scoped_identifier" => {
                let (home, _, x) = self.constant_value(at.m, &path_parts(Some(v), src))?;
                self.written_out(At { m: home, scope: None, params: &[] }, x, depth + 1)
            }
            _ => None,
        }
    }

    /// What a key or an index picks of a collection the code writes out (a map by a key its pairs
    /// write, an array or a `vec!` by its place), else every value it holds.
    fn picked(&self, at: At<'a, '_>, held: TsNode<'a>, key: Option<&str>, depth: u32) -> Vec<Built> {
        let index = key.and_then(|k| k.parse::<usize>().ok());
        let all = |values: Vec<Vec<Built>>| values.into_iter().fold(vec![], union);
        if let Some(pairs) = self.map_from_pairs(at.m, held) {
            let pairs: Vec<(TsNode<'a>, TsNode<'a>)> = elements(pairs)
                .into_iter()
                .filter(|p| p.kind() == "tuple_expression")
                .filter_map(|p| match named_children(p).as_slice() {
                    [k, v] => Some((*k, *v)),
                    _ => None,
                })
                .collect();
            let written = |k: TsNode<'a>| match self.of(at, k, depth + 1).as_slice() {
                [b] => crate::paths::known(b).then(|| crate::paths::render(b)),
                _ => None,
            };
            return match key.and_then(|key| pairs.iter().find(|(k, _)| written(*k).as_deref() == Some(key))) {
                Some((_, v)) => self.of(at, *v, depth + 1),
                None => all(pairs.iter().map(|(_, v)| self.of(at, *v, depth + 1)).collect()),
            };
        }
        if held.kind() == "macro_invocation" {
            let groups = named_children(held).into_iter().find(|x| x.kind() == "token_tree").map(token_groups).unwrap_or_default();
            return match index.and_then(|i| groups.get(i)) {
                Some(g) => self.tokens(at, g, depth),
                None => all(groups.iter().map(|g| self.tokens(at, g, depth)).collect()),
            };
        }
        let items = elements(held);
        match index.and_then(|i| items.get(i)) {
            Some(x) => self.of(at, *x, depth + 1),
            None => all(items.iter().map(|x| self.of(at, *x, depth + 1)).collect()),
        }
    }

    /// The pairs a map is built from where the code writes them out (knowledge::RUST_MAPS_FROM_PAIRS):
    /// the array `HashMap::from([("item", "/api/items"), ..])` is handed.
    fn map_from_pairs(&self, m: &'a Module, call: TsNode<'a>) -> Option<TsNode<'a>> {
        if call.kind() != "call_expression" {
            return None;
        }
        let parts = expand(m, &path_parts(call.child_by_field_name("function"), &m.src));
        let n = parts.len();
        let maps = knowledge::RUST_MAPS_FROM_PAIRS.iter().any(|(ty, f)| n >= 2 && parts[n - 2] == *ty && parts[n - 1] == *f);
        let first = named_children(call.child_by_field_name("arguments")?).into_iter().find(|a| !a.kind().ends_with("comment"))?;
        let first = beneath(first, super::module::WRAPPERS);
        (maps && first.kind() == "array_expression").then_some(first)
    }

    /// Whether a macro writes a list out by its elements (knowledge::RUST_LIST_MACROS): `vec![a, b]`.
    fn list_macro(&self, m: &Module, mac: TsNode) -> bool {
        mac.child_by_field_name("macro")
            .map(|x| path_parts(Some(x), &m.src))
            .and_then(|p| p.last().cloned())
            .is_some_and(|name| knowledge::RUST_LIST_MACROS.contains(&name.as_str()))
    }

    /// The listing of a directory (knowledge::LISTINGS_BELOW) a value comes from, where it is one
    /// of its elements or the listing itself: through the bindings that hold it, the loops and the
    /// methods that go over it or take one of its elements (`entries.next_entry()`), and the
    /// methods that hand it on (`entry?`, `.unwrap()`, `.flatten()`). A path's method that is
    /// another name for a listing (`p.read_dir()`) is that listing.
    fn listing_of(&self, at: At<'a, '_>, e: TsNode<'a>, depth: u32) -> Option<&'static knowledge::Listing> {
        if depth > DEPTH {
            return None;
        }
        let e = beneath(e, super::module::WRAPPERS);
        let src = &at.m.src;
        match e.kind() {
            "identifier" => match bound(at.scope?, text(e, src), e.start_byte(), src)? {
                Bound::Value(v) | Bound::Each(v) => self.listing_of(at, v, depth + 1),
                Bound::Handed(args) => args.into_iter().find_map(|a| self.listing_of(at, a, depth + 1)),
                Bound::Field(..) | Bound::Element(..) => None,
            },
            "call_expression" => {
                let f = e.child_by_field_name("function")?;
                if f.kind() == "field_expression" {
                    let method = text(f.child_by_field_name("field")?, src);
                    let on = f.child_by_field_name("value")?;
                    if let Some(l) = path_listing(method).filter(|_| self.type_of(at, on, depth + 1).is_none()) {
                        return Some(l);
                    }
                    let keeps = knowledge::hands_on(method) && !matches!(method, "map" | "and_then");
                    let one = knowledge::RUST_ONE_ELEMENT.contains(&method);
                    return (keeps || one || knowledge::RUST_SAME_ELEMENTS.contains(&method))
                        .then(|| self.listing_of(at, on, depth + 1))
                        .flatten();
                }
                listing(&expand(at.m, &path_parts(Some(f), src)))
            }
            _ => None,
        }
    }

    /// A field of the value a method runs on, as the struct literals that build that type set it
    /// (`Self { root }` in `new(root)`), from what the functions they are in are handed.
    fn held(&self, at: At<'a, '_>, field: &str, depth: u32) -> Option<Vec<Built>> {
        self.held_in(&self.impl_type(at.m, at.scope?)?, field, depth)
    }

    /// A field of a value of the program's type `ty`, as the struct literals that build that type set
    /// it, from what the functions they are in are handed: of the value a method runs on, or of any
    /// value whose type the code says (`tmp.root` of a `Tmp`).
    fn held_in(&self, ty: &str, field: &str, depth: u32) -> Option<Vec<Built>> {
        let ty = ty.to_string();
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
            out = union(out, vals.into_iter().map(|b| held_by(&id, b)).collect());
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
                    .map_or(false, |n| matches!(n.kind, NodeKind::Function | NodeKind::Method));
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
            if let Some(k) = knowledge::RUST_PATH_METHODS.iter().find(|(m, _)| *m == method).map(|(_, k)| *k) {
                return path_made(k, on(), arg(0));
            }
            // a listing of the directory a path names, as the function it is another name for: `p.read_dir()` is
            // `fs::read_dir(p)`; a method of the program's own type of that name is the program's
            let listed = path_listing(method).filter(|_| args.is_empty() && self.type_of(at, recv, depth + 1).is_none());
            return match listed {
                Some(l) if l.names => vec![lit("*")],
                Some(_) => cross(&on(), &[lit("*")], "/"),
                // the value a key picks of a collection the code writes out: `paths.get(key)`
                None if knowledge::RUST_LOOKUPS.contains(&method) && args.len() == 1 && self.written_out(at, recv, depth).is_some() => {
                    self.looked_up(at, recv, args[0], depth).unwrap_or_else(|| vec![hole("call")])
                }
                // the path a listing's element is at: `entry.path()` of `fs::read_dir(d)`'s `entry`
                None if knowledge::ELEMENT_PATHS.iter().any(|(lib, _, m)| {
                    *m == method && args.is_empty() && self.listing_of(at, recv, depth).is_some_and(|l| l.library == *lib)
                }) =>
                {
                    on()
                }
                // the elements an iterator keeps, one of them, and what the closure it runs on each gives
                None if knowledge::RUST_SAME_ELEMENTS.contains(&method) => on(),
                // one element of what it is called on, as a loop's is: of nothing known, a value the name it is bound to names
                None if knowledge::RUST_ONE_ELEMENT.contains(&method) => match on() {
                    v if v.iter().all(|b| matches!(b.as_slice(), [Seg::Hole(_)])) => vec![hole("value")],
                    v => v,
                },
                None if knowledge::RUST_GIVES_EACH.contains(&method)
                    && matches!(args.as_slice(), [c] if beneath(*c, super::module::WRAPPERS).kind() == "closure_expression") =>
                {
                    match self.of(at, args[0], depth + 1) {
                        v if v.is_empty() => vec![hole("call")],
                        v => v,
                    }
                }
                // what an `Option` or a `Result` holds, or what it falls back to where it holds nothing
                None if super::module::OR_ELSE.contains(&method) && !method.starts_with("map") => or_else(on(), arg(0)),
                None if knowledge::hands_on(method) && !matches!(method, "map" | "and_then") => on(),
                // a door to the place of the socket it is called on: a clone, the connections a
                // listener accepts
                None if knowledge::RUST_DOOR_KEEPERS.contains(&(method, None)) => on(),
                None => self.method_given(at, recv, method, &args, depth).unwrap_or_else(|| vec![hole("call")]),
            };
        }
        let parts = expand(at.m, &path_parts(Some(f), src));
        // `Some(x)` and `Ok(x)` hold `x`
        if matches!(parts.as_slice(), [v] if super::module::HOLDING.contains(&v.as_str())) {
            return arg(0);
        }
        // a listing of a directory: each element is below it, `*`, as the path or as its name alone
        if let Some(l) = listing(&parts) {
            return match args.first().filter(|_| !l.names) {
                Some(d) => cross(&self.of(at, *d, depth + 1), &[lit("*")], "/"),
                None => vec![lit("*")],
            };
        }
        let n = parts.len();
        // a path's method called by its type's path is handed the path first: `Path::join(p, x)` is `p.join(x)`
        if let ([.., ty, method], [on, rest @ ..]) = (parts.as_slice(), args.as_slice()) {
            let k = knowledge::RUST_PATH_METHODS.iter().find(|(m, _)| m == method).map(|(_, k)| *k);
            if let Some(k) = k.filter(|_| knowledge::RUST_PATH_TYPES.contains(&ty.as_str())) {
                let first = rest.first().map(|a| self.of(at, *a, depth + 1)).unwrap_or_else(|| vec![hole("value")]);
                return path_made(k, self.of(at, *on, depth + 1), first);
            }
        }
        // a socket opened at the address it is handed is a door to that place
        if let Some((_, _, i)) = knowledge::RUST_DOORS.iter().find(|(on, name, _)| n >= 2 && parts[n - 2] == *on && parts[n - 1] == *name) {
            return arg(*i);
        }
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
            Some(PathFn::Environment) => vec![environment_named(&arg(0)).unwrap_or_else(|| hole("call"))],
            None => self.given(at, f, &args, depth).unwrap_or_else(|| vec![hole("call")]),
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

    /// What one of the program's methods gives back, called on a value whose type the code says
    /// (`Workbench::of(dir).file(name)`): its parameters bound to what the call hands them, and
    /// what it reads of `self` as the values of its type hold it.
    fn method_given(&self, at: At<'a, '_>, recv: TsNode<'a>, method: &str, args: &[TsNode<'a>], depth: u32) -> Option<Vec<Built>> {
        let ty = self.type_of(at, recv, depth + 1)?;
        let n = self.nodes.get(&format!("{ty}.{method}")).filter(|n| n.kind == NodeKind::Method)?;
        let home = self.modules.get(&n.module)?;
        let def = def_node(home, n)?;
        let names = param_names(def, &home.src);
        let inner = At { m: home, scope: Some(def), params: &names };
        let gives = returns(def.child_by_field_name("body")?).into_iter().fold(vec![], |acc, x| union(acc, self.of(inner, x, depth + 1)));
        if gives.is_empty() {
            return None;
        }
        let handed: HashMap<&str, Vec<Built>> = names.iter().zip(args).map(|(p, a)| (p.as_str(), self.of(at, *a, depth + 1))).collect();
        Some(gives.iter().flat_map(|b| substitute(b, |p| handed.get(p).cloned().unwrap_or_else(|| vec![hole(p)]))).collect())
    }

    /// The program's type of the value `e` is, where the code says it: the value a method runs on,
    /// what one of the program's functions or methods gives back by its signature, a binding of
    /// such a value, or a parameter by its declared type.
    fn type_of(&self, at: At<'a, '_>, e: TsNode<'a>, depth: u32) -> Option<String> {
        if depth > DEPTH {
            return None;
        }
        let e = beneath(e, super::module::WRAPPERS);
        let src = &at.m.src;
        let own = |id: &str| self.nodes.get(id).filter(|n| n.kind == NodeKind::Class && !n.library).map(|_| id.to_string());
        let gives = |n: &Node| -> Option<String> {
            let r = n.returns.as_ref()?;
            if r.own {
                return n.cls.clone();
            }
            let home = self.modules.get(&n.module)?;
            let parts: Vec<String> = r.value.split("::").map(str::to_string).collect();
            resolve(home, &parts, self.modules, &self.nodes, self.crate_ids).and_then(|id| own(&id))
        };
        match e.kind() {
            // the value a method runs on
            "self" => self.impl_type(at.m, at.scope?),
            "call_expression" => {
                let f = e.child_by_field_name("function")?;
                let n = if f.kind() == "field_expression" {
                    let ty = self.type_of(at, f.child_by_field_name("value")?, depth + 1)?;
                    self.nodes.get(&format!("{ty}.{}", text(f.child_by_field_name("field")?, src)))?
                } else {
                    let id = resolve_value(at.m, &path_parts(Some(f), src), self.modules, &self.nodes, self.crate_ids)?;
                    self.nodes.get(&id)?
                };
                gives(n)
            }
            "identifier" => {
                let name = text(e, src);
                let scope = at.scope?;
                if let Some(Bound::Value(v)) = bound(scope, name, e.start_byte(), src) {
                    return self.type_of(at, v, depth + 1);
                }
                let ps = scope.child_by_field_name("parameters")?;
                let declared = named_children(ps).into_iter().find(|p| {
                    p.kind() == "parameter"
                        && p.child_by_field_name("pattern").map(|x| text(x, src).trim_start_matches("mut ")) == Some(name)
                })?;
                let mut t = declared.child_by_field_name("type")?;
                while t.kind() == "reference_type" {
                    t = t.child_by_field_name("type")?;
                }
                resolve(at.m, &path_parts(Some(t), src), self.modules, &self.nodes, self.crate_ids).and_then(|id| own(&id))
            }
            _ => None,
        }
    }

    /// `format!` and `concat!`: their text with each argument evaluated in its place.
    fn macro_value(&self, at: At<'a, '_>, mac: TsNode<'a>, depth: u32) -> Vec<Built> {
        let src = &at.m.src;
        let name = mac.child_by_field_name("macro").map(|x| path_parts(Some(x), src)).and_then(|p| p.last().cloned()).unwrap_or_default();
        let Some(tt) = named_children(mac).into_iter().find(|x| x.kind() == "token_tree") else { return vec![hole("call")] };
        let groups = token_groups(tt);
        // the directory the crate's manifest is in, fixed where the program is built
        let built_in = |var: &str| matches!(groups.first().map(Vec::as_slice), Some([x]) if unquote(text(*x, src)) == var);
        match name.as_str() {
            "format" | "format_args" => self.format(at, mac.start_byte(), &groups, depth),
            "concat" => groups.iter().fold(vec![vec![]], |acc, g| cross(&acc, &self.tokens(at, g, depth), "")),
            m if knowledge::RUST_BUILD_DIRS.iter().any(|(mac, var)| *mac == m && built_in(var)) => vec![lit(&at.m.manifest_dir)],
            _ => vec![hole("call")],
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
            // a call of one of the program's functions, each argument one name or literal:
            // `endpoint("item")`
            [f, call] if f.kind() == "identifier" && call.kind() == "token_tree" && call.child(0).is_some_and(|c| c.kind() == "(") => {
                let groups = token_groups(*call);
                let args: Vec<TsNode<'a>> =
                    groups.iter().filter_map(|g| if let [one] = g.as_slice() { Some(*one) } else { None }).collect();
                (args.len() == groups.len()).then(|| self.given(at, *f, &args, depth)).flatten().unwrap_or_else(|| vec![hole("value")])
            }
            // an expression written as tokens that none of these is: no value the code names
            _ => vec![hole("value")],
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

/// The elements an array the code writes out holds, or the one it repeats (`[x; n]`).
fn elements(array: TsNode) -> Vec<TsNode> {
    let mut cur = array.walk();
    let repeats = array.children(&mut cur).any(|c| c.kind() == ";");
    let items: Vec<TsNode> = named_children(array).into_iter().filter(|x| !x.kind().ends_with("comment")).collect();
    if repeats {
        items.into_iter().take(1).collect()
    } else {
        items
    }
}

/// The library's function that lists a directory (knowledge::LISTINGS_BELOW) a path names, as its
/// `use` expands it: `fs::read_dir` after `use std::fs`.
fn listing(parts: &[String]) -> Option<&'static knowledge::Listing> {
    let (function, path) = parts.split_last()?;
    let library = path.join("::");
    knowledge::LISTINGS_BELOW.iter().find(|l| l.function == function && l.library == library)
}

/// The listing a path's method is another name for (knowledge::RUST_PATH_ALIASES): `read_dir` is
/// `fs::read_dir`.
fn path_listing(method: &str) -> Option<&'static knowledge::Listing> {
    let (_, module) = knowledge::RUST_PATH_ALIASES.iter().find(|(m, _)| *m == method)?;
    knowledge::LISTINGS_BELOW.iter().find(|l| l.function == method && l.library == *module)
}

/// Whether the value `v` a binding is bound to is a `PathBuf`, where the code says so: by the type the
/// binding declares (`let mut p: PathBuf`), or by what makes the value, through what hands it on (`?`,
/// `.unwrap()`): a path type's function (`PathBuf::from(d)`, `PathBuf::new()`), one of the directories
/// the environment gives (`env::current_dir()`), or a path's method that gives a `PathBuf`
/// (knowledge::RUST_PATH_BUFS: `d.join(x)`, `d.to_path_buf()`). A list's or a string's `push` and
/// `pop` change no path.
fn holds_path_buf(m: &Module, v: TsNode) -> bool {
    let src = &m.src;
    let path_type = |parts: &[String]| parts.last().is_some_and(|t| knowledge::RUST_PATH_TYPES.contains(&t.as_str()));
    if let Some(ty) = v.parent().filter(|l| l.kind() == "let_declaration").and_then(|l| l.child_by_field_name("type")) {
        return path_type(&path_parts(Some(ty), src));
    }
    let mut e = beneath(v, super::module::WRAPPERS);
    while let Some(f) = e.child_by_field_name("function").filter(|_| e.kind() == "call_expression") {
        if f.kind() == "field_expression" {
            let (Some(on), Some(method)) = (f.child_by_field_name("value"), f.child_by_field_name("field").map(|x| text(x, src))) else {
                return false;
            };
            if !knowledge::hands_on(method) {
                return knowledge::RUST_PATH_BUFS.contains(&method);
            }
            e = beneath(on, super::module::WRAPPERS);
            continue;
        }
        let parts = expand(m, &path_parts(Some(f), src));
        let n = parts.len();
        let made = knowledge::RUST_PATH_FUNCTIONS.iter().find(|(on, name, _)| n >= 2 && parts[n - 2] == *on && parts[n - 1] == *name);
        return match made.map(|(_, _, k)| *k) {
            Some(PathFn::Home | PathFn::Here | PathFn::Temp) => true,
            Some(_) => path_type(&parts[..n - 1]),
            None => false,
        };
    }
    false
}

/// How often a change runs where the binding it changes runs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Runs {
    /// Once, whenever the binding does.
    Once,
    /// Once or not at all: it stands in an arm of a choice the binding does not stand in.
    Maybe,
    /// As often as the code around it says, none or more: it stands in the body of a loop or a closure
    /// the binding does not stand in.
    Repeated,
}

/// How often `change` runs where the binding `v` runs.
fn runs(change: TsNode, v: TsNode) -> Runs {
    let mut around = std::collections::HashSet::new();
    let mut p = Some(v);
    while let Some(x) = p {
        around.insert(x.id());
        p = x.parent();
    }
    let (mut child, mut p, mut out) = (change, change.parent(), Runs::Once);
    while let Some(x) = p.filter(|x| !around.contains(&x.id())) {
        let at = |field: &str| x.child_by_field_name(field).is_some_and(|f| f.id() == child.id());
        match x.kind() {
            "for_expression" | "while_expression" | "loop_expression" | "closure_expression" if at("body") => return Runs::Repeated,
            "if_expression" if at("consequence") || at("alternative") => out = Runs::Maybe,
            "match_arm" => out = Runs::Maybe,
            _ => {}
        }
        (child, p) = (x, x.parent());
    }
    out
}

/// The path a method of a path makes of the path it is called on, `on`, and its first argument.
fn path_made(k: PathMethod, on: Vec<Built>, first: Vec<Built>) -> Vec<Built> {
    match k {
        PathMethod::Join => cross(&on, &first, "/"),
        PathMethod::Parent => on.iter().map(parent).collect(),
        PathMethod::Same => on,
        PathMethod::Extension => on.iter().flat_map(|b| first.iter().map(move |x| with_extension(b, x))).collect(),
        PathMethod::Sibling => cross(&on.iter().map(parent).collect::<Vec<_>>(), &first, "/"),
    }
}

/// What a binding of a name holds: a value, the field a struct pattern takes out of the value it
/// matches, an element of the tuple it matches, for a parameter of a closure a `let` keeps what
/// each call of the closure hands it, or one element of what a loop or a method that runs a
/// closure on each element goes over.
enum Bound<'a> {
    Value(TsNode<'a>),
    Field(TsNode<'a>, String, TsNode<'a>),
    Element(TsNode<'a>, usize),
    Each(TsNode<'a>),
    Handed(Vec<TsNode<'a>>),
}

/// What a name holds where the code at `pos` reads it, by the binding of it in scope there that
/// is written last: a `let` complete before `pos`, or a pattern (`if let` in the block it opens,
/// `let .. else` after it, a `match` arm in its arm) that binds the whole value, takes the value
/// `Some` or `Ok` holds out of it (`if let Some(dir) = to.parent()`), which holds what that one
/// holds, takes an element out of a tuple (`let (conn, _) = listener.accept()?`), or takes a field
/// out of a struct or a variant (`Cmd::Analyze { out, .. }`), a `for` loop's name in its body, or
/// a parameter of the closure `pos` is in (`handed`).
fn bound<'a>(scope: TsNode<'a>, name: &str, pos: usize, src: &[u8]) -> Option<Bound<'a>> {
    let named = |p: TsNode| matches!(p.kind(), "identifier" | "mut_pattern") && text(p, src).trim_start_matches("mut ") == name;
    let element = |t: TsNode| (t.kind() == "tuple_pattern").then(|| named_children(t).into_iter().position(|x| named(x))).flatten();
    let binds = |p: TsNode<'a>, v: TsNode<'a>| -> Option<Bound<'a>> {
        if named(p) {
            return Some(Bound::Value(v));
        }
        let ty = p.child_by_field_name("type");
        let inner: Vec<TsNode> = named_children(p).into_iter().filter(|c| Some(c.id()) != ty.map(|t| t.id())).collect();
        match p.kind() {
            "tuple_struct_pattern" if ty.map_or(false, |t| super::module::HOLDING.contains(&text(t, src))) => match inner.as_slice() {
                [x] if named(*x) => Some(Bound::Value(v)),
                [x] => element(*x).map(|i| Bound::Element(v, i)),
                _ => None,
            },
            "tuple_pattern" => element(p).map(|i| Bound::Element(v, i)),
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
    for b in walk(scope, &["let_declaration", "let_condition", "match_arm", "for_expression"]) {
        let found = match b.kind() {
            // `for entry in fs::read_dir(d)?`: one element of what the loop goes over
            "for_expression" => {
                let (Some(p), Some(v)) = (b.child_by_field_name("pattern"), b.child_by_field_name("value")) else { continue };
                (named(p) && around(b.child_by_field_name("body"))).then(|| (p.end_byte(), Bound::Each(v)))
            }
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
    if let Some((end, x)) = handed(scope, name, pos, src).filter(|(end, _)| best.as_ref().map_or(true, |(e, _)| end >= e)) {
        best = Some((end, x));
    }
    best.map(|(_, x)| x)
}

/// The parameter `name` of the closure `pos` is in, where a `let` keeps the closure by a name the
/// body calls it by (`let read = |name: &str| repo.text(root.join(name));`, `read("ty.toml")`):
/// what each call hands it, as a function's parameter is what its callers hand it. With the end of
/// the closure's parameters, where the binding starts.
fn handed<'a>(scope: TsNode<'a>, name: &str, pos: usize, src: &[u8]) -> Option<(usize, Bound<'a>)> {
    let inside = |n: TsNode| n.start_byte() <= pos && pos < n.end_byte();
    // the innermost closure around `pos` that takes `name`
    let (closure, slot) = walk(scope, &["closure_expression"])
        .into_iter()
        .filter(|c| c.child_by_field_name("body").map_or(false, inside))
        .filter_map(|c| {
            let params = named_children(c.child_by_field_name("parameters")?);
            let slot = params.iter().position(|p| {
                let pattern = if p.kind() == "parameter" { p.child_by_field_name("pattern") } else { Some(*p) };
                pattern.map_or(false, |x| x.kind() == "identifier" && text(x, src) == name)
            })?;
            Some((c, slot))
        })
        .max_by_key(|(c, _)| c.start_byte())?;
    let end = closure.child_by_field_name("parameters")?.end_byte();
    // a closure handed to a method that runs it on each element of what the method is called on
    // (knowledge::RUST_EACH): its parameters from the element's place on are elements of that
    let mut handed_to = closure;
    while let Some(p) = handed_to.parent().filter(|p| crate::tree::wraps(super::module::WRAPPERS, *p)) {
        handed_to = p;
    }
    if let Some(method) = handed_to
        .parent()
        .filter(|a| a.kind() == "arguments")
        .and_then(|a| a.parent())
        .filter(|c| c.kind() == "call_expression")
        .and_then(|c| c.child_by_field_name("function"))
        .filter(|f| f.kind() == "field_expression")
    {
        let element = method.child_by_field_name("field").and_then(|m| knowledge::rust_element(text(m, src)));
        return match (element, method.child_by_field_name("value")) {
            (Some(at), Some(over)) if slot >= at as usize => Some((end, Bound::Each(over))),
            _ => None,
        };
    }
    let kept = closure.parent().filter(|l| l.kind() == "let_declaration")?;
    let by = kept.child_by_field_name("pattern").filter(|p| p.kind() == "identifier").map(|p| text(p, src))?;
    let args: Vec<TsNode<'a>> = walk(kept.parent()?, &["call_expression"])
        .into_iter()
        .filter(|c| c.start_byte() >= kept.end_byte())
        .filter(|c| c.child_by_field_name("function").map_or(false, |f| f.kind() == "identifier" && text(f, src) == by))
        .filter_map(|c| {
            named_children(c.child_by_field_name("arguments")?).into_iter().filter(|a| !a.kind().ends_with("comment")).nth(slot)
        })
        .collect();
    (!args.is_empty()).then_some((end, Bound::Handed(args)))
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

/// A path with another extension: `page.json` with `html` is `page.html`, with an extension the
/// code builds (`format!("{}.part", id)`) `page.{}.part`, and with none `page`.
fn with_extension(b: &Built, ext: &Built) -> Built {
    let none = matches!(ext.as_slice(), [Seg::Lit(e)] if e.is_empty());
    crate::paths::with_suffix(b, &if none { vec![] } else { [lit("."), ext.clone()].concat() })
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
