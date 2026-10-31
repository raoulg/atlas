//! A function's body as Python reads it: what ty says of its expressions, its steps in the order
//! they run, the arguments its calls hand, what its calls touch and change, and the syntax of its
//! choices and comparisons.

use std::collections::{HashMap, HashSet};

use indexmap::IndexMap;
use tree_sitter::Node as TsNode;

use super::module::{
    call_path, def_node, dotted, is_main_guard, lib_of, named_path, outer_type_names, package_of, resolve, top_assignments, type_names,
    written, Module, Own, WRAPPERS,
};
use super::paths::{self, At, Built, Eval};
use crate::body::{changed_through, BindSyntax, Crosses, Range, Step, Steps, UseSyntax};
use crate::builder::{foreign_type, GraphBuilder};
use crate::effects::{self, Crossing};
use crate::knowledge::{self, OwnTypes};
use crate::model::{Bucket, Dir, EdgeKind, Held, Node, NodeKind, Slot, Touch, World};
use crate::pytypes::{By, Facts, FileArg, Target, Types, Value};
use crate::tree::{
    beneath, evaluation_order, field_children, literals, named_children, nth_of_kind, outermost, payload_of, text, unquote, walk, what_of,
    ChoiceSyntax, CompareSyntax, Part,
};

/// What a change to each typed parameter changes, by name: the type the picture draws it as, the
/// class of the package its annotation names, also inside a container (`graph: Graph`, `rows:
/// list[Row]` -> Row), or a library's class (`buf: io.StringIO`); else the class its annotation
/// names outermost, as ty finds it (`xs: list[int]` -> `ext:builtins.list`). With them, the
/// parameters that hold one of the language's own containers that change in place.
fn changed_types(
    m: &Module,
    d: TsNode,
    modules: &IndexMap<String, Module>,
    nodes: &IndexMap<String, Node>,
    own: &Own,
    facts: Option<&Facts>,
) -> (HashMap<String, String>, HashSet<String>) {
    let (mut out, mut changeable) = (HashMap::new(), HashSet::new());
    let Some(pn) = d.child_by_field_name("parameters") else { return (out, changeable) };
    for p in named_children(pn) {
        // `typed_parameter` carries its identifier as a plain child, `typed_default_parameter` as a field
        let name = p.child_by_field_name("name").or_else(|| named_children(p).into_iter().find(|c| c.kind() == "identifier"));
        let (Some(name), Some(t)) = (name, p.child_by_field_name("type")) else { continue };
        let name = text(name, &m.src).to_string();
        let mut drawn = None;
        for tn in type_names(Some(t), &m.src) {
            match resolve(m, &tn, modules, nodes, own) {
                Some(r) if !r.starts_with("ext:") && nodes.get(&r).map_or(false, |x| x.kind == NodeKind::Class) => drawn = Some(r),
                Some(r) if drawn.is_none() && foreign_type(&r, &OWN) => drawn = Some(r),
                _ => {}
            }
        }
        // the class the annotation names outermost, where ty says a library defines it
        let outer = outer_type_names(t, &m.src).into_iter().find_map(|h| match facts?.def((h.start_byte(), h.end_byte()))? {
            Target::Lib(module) => Some((package_of(module), text(h, &m.src).rsplit('.').next()?.to_string())),
            Target::Here { .. } => None,
        });
        if let Some((package, class)) = &outer {
            if knowledge::PYTHON_CHANGEABLE.contains(&(package.as_str(), class.as_str())) {
                changeable.insert(name.clone());
            }
        }
        if let Some(r) = drawn.or_else(|| outer.map(|(package, class)| format!("ext:{package}.{class}"))) {
            out.insert(name, r);
        }
    }
    (out, changeable)
}

/// How Python binds a name, as `changes` follows a change through it: `x = v` binds `x` to `v`,
/// `for x in v` to each element of `v`.
const BINDS: BindSyntax = BindSyntax {
    binds: |n, src| {
        let (l, v) = (n.child_by_field_name("left")?, n.child_by_field_name("right")?);
        (n.kind() == "assignment" && l.kind() == "identifier").then(|| (text(l, src), held_by(v, src)))
    },
    loops: |n, src| {
        let (l, v, b) = (n.child_by_field_name("left")?, n.child_by_field_name("right")?, n.child_by_field_name("body")?);
        (n.kind() == "for_statement" && l.kind() == "identifier").then(|| (text(l, src), root_name(v, src), b))
    },
};

/// The names whose value a Python value may be or be a part of: `graph`, `self.items[0]`, either
/// side of `a or b` and of `a if c else b` (`projections`). A call and a slice (`xs[:]`) make a
/// value anew.
fn held_by<'a>(v: TsNode, src: &'a [u8]) -> Vec<&'a str> {
    projections(v).into_iter().flatten().map(|n| text(n, src)).collect()
}

/// Whether the name `i` is one a `lambda` between it and `e` binds: a parameter of the function
/// the lambda is, no value of the body around it.
fn lambda_binds(i: TsNode, e: TsNode, src: &[u8]) -> bool {
    let name = text(i, src);
    let mut x = i;
    while x.id() != e.id() {
        let Some(p) = x.parent() else { return false };
        if p.kind() == "lambda"
            && p.child_by_field_name("parameters").is_some_and(|ps| walk(ps, &["identifier"]).iter().any(|n| text(*n, src) == name))
        {
            return true;
        }
        x = p;
    }
    false
}

/// Whether the name `at` is one of the parameters of the function it stands in, bound by nothing nearer: no assignment,
/// loop or `case` pattern of the body binds it before `at`. (`self` and `cls` are the value a method runs on and its
/// class, read apart.)
fn own_parameter(at: TsNode, src: &[u8]) -> bool {
    let name = text(at, src);
    let mut def = at;
    while let Some(p) = def.parent() {
        def = p;
        if matches!(p.kind(), "function_definition" | "lambda") {
            break;
        }
    }
    let Some(params) = def.child_by_field_name("parameters").filter(|_| def.kind() == "function_definition") else { return false };
    let declared = named_children(params).into_iter().any(|p| {
        let id = if p.kind() == "identifier" { Some(p) } else { p.child_by_field_name("name").or_else(|| p.named_child(0)) };
        id.is_some_and(|id| id.kind() == "identifier" && text(id, src) == name)
    });
    declared && reaching(at, name, src).is_empty() && looped_by(at, name, src).is_none() && matched_by(at, name, src).is_none()
}

/// The bindings of `name` that reach `at`: the latest assignment to it before `at` in the function,
/// and where that one stands in an arm of a choice `at` is not in, the latest in each of its other
/// arms (`if c: x = a.b` / `else: x = d.e`).
fn reaching<'t>(at: TsNode<'t>, name: &str, src: &[u8]) -> Vec<TsNode<'t>> {
    let mut scope = at;
    while let Some(p) = scope.parent() {
        scope = p;
        if matches!(p.kind(), "function_definition" | "lambda") {
            break;
        }
    }
    let binds = |a: &TsNode| {
        a.child_by_field_name("left").is_some_and(|l| match l.kind() {
            "identifier" => text(l, src) == name,
            "pattern_list" | "tuple_pattern" => named_children(l).iter().any(|n| n.kind() == "identifier" && text(*n, src) == name),
            _ => false,
        })
    };
    let before: Vec<TsNode> = walk(scope, &["assignment"]).into_iter().filter(|a| a.end_byte() <= at.start_byte() && binds(a)).collect();
    let Some(last) = before.last().copied() else { return vec![] };
    let within = |n: TsNode, outer: TsNode| outer.start_byte() <= n.start_byte() && n.end_byte() <= outer.end_byte();
    // the arms of the choice the latest binding stands in, where `at` stands outside that choice
    let mut arm = last;
    while let Some(p) = arm.parent().filter(|p| p.id() != scope.id()) {
        if matches!((CHOICES.part)(p, arm, src), Part::Arm(_)) && (CHOICES.chooses)(p, src) && !within(at, p) {
            let mut choice = p;
            while let Some(up) = (CHOICES.continues)(choice) {
                choice = up;
            }
            // the choice and what continues it (an `elif`), each with its arms
            let parts = std::iter::once(choice).chain(named_children(choice).into_iter().filter(|c| (CHOICES.continues)(*c).is_some()));
            let arms: Vec<TsNode> = parts
                .flat_map(|q| named_children(q).into_iter().filter(move |c| matches!((CHOICES.part)(q, *c, src), Part::Arm(_))))
                .collect();
            let mut out: Vec<TsNode> = arms.iter().filter_map(|b| before.iter().filter(|a| within(**a, *b)).last().copied()).collect();
            out.dedup_by_key(|a| a.id());
            return out;
        }
        arm = p;
    }
    vec![last]
}

/// The names a value is made of without a step: what it is a projection of (`projections`), and the
/// names an operator, a literal of several values or an f-string computes it from (`bottom = height -
/// margin`, `pair = (a, b)`, `body::computed_of`). A literal of its own is made of none; `None` for a
/// part a call makes.
fn sources<'t>(v: TsNode<'t>, src: &[u8]) -> Vec<Option<TsNode<'t>>> {
    match crate::body::computed_of(v, &USES, src, &part) {
        Some(names) => names.into_iter().map(Some).collect(),
        None => vec![None],
    }
}

/// What a part of a value Python's own code computes is made of (`body::computed_of`): a literal, a piece of a string
/// and a comment of none, any other part what it is a projection of.
fn part(p: TsNode) -> Option<Vec<TsNode>> {
    match p.kind() {
        "integer" | "float" | "true" | "false" | "none" | "ellipsis" | "comment" | "string_start" | "string_content" | "string_end"
        | "escape_sequence" | "format_specifier" => Some(vec![]),
        _ => projections(p).into_iter().collect(),
    }
}

/// The names a value is a projection of, by their roots: `a.x[k]` of `a`, `y` of itself, either part
/// of `a.x if c else b.y` and of `a or b`. `None` for a part that is no projection (a call, a slice).
fn projections(v: TsNode) -> Vec<Option<TsNode>> {
    let v = beneath(v, WRAPPERS);
    match v.kind() {
        "conditional_expression" => {
            let parts = named_children(v);
            [parts.first(), parts.get(2)].into_iter().flatten().flat_map(|p| projections(*p)).collect()
        }
        "boolean_operator" => {
            [v.child_by_field_name("left"), v.child_by_field_name("right")].into_iter().flatten().flat_map(projections).collect()
        }
        "subscript" if field_children(v, "subscript").iter().any(|s| s.kind() == "slice") => vec![None],
        "attribute" | "subscript" | "identifier" => {
            let mut root = v;
            while let Some(inner) = match root.kind() {
                "attribute" => root.child_by_field_name("object"),
                "subscript" => root.child_by_field_name("value"),
                _ => None,
            } {
                root = inner;
            }
            vec![(root.kind() == "identifier").then_some(root)]
        }
        _ => vec![None],
    }
}

/// What the loop around `at` that binds `name` goes over (`for s, t in rows`): its element is a part of
/// it. A loop that is a step of its own has made the name, which then holds what the loop gave.
fn looped_by<'t>(at: TsNode<'t>, name: &str, src: &[u8]) -> Option<TsNode<'t>> {
    let names = |l: Option<TsNode>| l.is_some_and(|l| walk(l, &["identifier"]).iter().any(|n| text(*n, src) == name));
    let mut x = at;
    while let Some(p) = x.parent() {
        match p.kind() {
            "function_definition" | "lambda" => return None,
            "for_statement" if p.child_by_field_name("body").is_some_and(|b| b.id() == x.id()) && names(p.child_by_field_name("left")) => {
                return p.child_by_field_name("right");
            }
            // a comprehension's element and its later clauses stand in its loops
            "list_comprehension" | "set_comprehension" | "dictionary_comprehension" | "generator_expression" => {
                let clause = named_children(p)
                    .into_iter()
                    .find(|c| c.kind() == "for_in_clause" && c.id() != x.id() && names(c.child_by_field_name("left")));
                if let Some(c) = clause {
                    return c.child_by_field_name("right");
                }
            }
            _ => {}
        }
        x = p;
    }
    None
}

/// The subject of the `match` whose `case` around `at` binds `name` in its pattern: a capture, a
/// keyword's value or an `as` name, not a class or a keyword the pattern names.
fn matched_by<'t>(at: TsNode<'t>, name: &str, src: &[u8]) -> Option<TsNode<'t>> {
    let mut x = at;
    while let Some(p) = x.parent() {
        if p.kind() == "case_clause" {
            let binds = named_children(p).into_iter().filter(|c| c.kind() == "case_pattern").any(|pat| {
                walk(pat, &["identifier"]).into_iter().any(|i| {
                    let first = |q: TsNode, c: TsNode| named_children(q).first().is_some_and(|f| f.id() == c.id());
                    // a keyword, a class the pattern matches, or a value written by its dotted name
                    let label = i.parent().is_some_and(|q| match q.kind() {
                        "keyword_pattern" => first(q, i),
                        "dotted_name" => {
                            named_children(q).len() > 1 || q.parent().is_some_and(|c| c.kind() == "class_pattern" && first(c, q))
                        }
                        _ => false,
                    });
                    !label && text(i, src) == name
                })
            });
            if binds {
                return p.parent().and_then(|b| b.parent()).filter(|m| m.kind() == "match_statement")?.child_by_field_name("subject");
            }
        }
        if matches!(p.kind(), "function_definition" | "lambda") {
            return None;
        }
        x = p;
    }
    None
}

/// The name a chain like `self.items[0].x` starts from.
fn root_name<'a>(n: TsNode, src: &'a [u8]) -> Option<&'a str> {
    root_of(n).map(|r| text(r, src))
}

/// The name a chain like `self.items[i].x` starts from, where it is written: what a change to the
/// chain changes, never the key it is looked up by; a wrapper is the value inside (`(board).cells`).
fn root_of(n: TsNode) -> Option<TsNode> {
    let n = beneath(n, WRAPPERS);
    match n.kind() {
        "identifier" => Some(n),
        "attribute" => root_of(n.child_by_field_name("object")?),
        "subscript" => root_of(n.child_by_field_name("value")?),
        _ => None,
    }
}

/// The names whose objects a body changes in place: the root of everything it changes, followed
/// through the names it is changed by (`changed_through`). Changing a part changes the whole.
pub(super) fn changed_roots<'a, 't>(
    body: TsNode<'t>,
    src: &'a [u8],
    changes: &dyn Fn(TsNode<'t>) -> Vec<TsNode<'t>>,
) -> std::collections::BTreeSet<&'a str> {
    changed(body, changes).into_iter().filter_map(|x| changed_through(body, src, &BINDS, root_name(x, src)?, x.start_byte())).collect()
}

/// The parameters each function and method of the package changes in place, by name, each with
/// the type a change to it changes where its annotation names one (`changed_types`), else `""`:
/// what its body changes itself and what it hands a call that changes it. The value a method runs
/// on is `GraphBuilder::mutating`'s.
pub(super) type Changing = HashMap<String, Vec<(String, String)>>;

/// The parameters a function's body changes in place (`changing`): the roots of what it changes,
/// and a parameter that holds one of the language's own containers it changes with `xs += ys`.
pub(super) fn changed_params<'t>(
    seen: &Seen,
    changing: &Changing,
    d: TsNode<'t>,
    body: TsNode<'t>,
    method: bool,
    cls: Option<&str>,
) -> (bool, Vec<(String, String)>) {
    let src = &seen.m.src;
    let (types, changeable) = changed_types(seen.m, d, seen.modules, &seen.store.nodes, seen.own, seen.facts);
    let mut roots = changed_roots(body, src, &|c| changed_by(seen, changing, c, cls));
    for a in walk(body, &["augmented_assignment"]) {
        let Some(l) = a.child_by_field_name("left").filter(|l| l.kind() == "identifier") else { continue };
        if changeable.contains(text(l, src)) {
            roots.extend(changed_through(body, src, &BINDS, text(l, src), a.start_byte()));
        }
    }
    let params = paths::parameters(d, src, method).into_iter().map(|(p, _)| p).filter(|p| roots.contains(p.as_str()));
    let params = params.map(|p| {
        let t = types.get(&p).cloned().unwrap_or_default();
        (p, t)
    });
    (roots.contains("self"), params.collect())
}

/// What a call changes in place: the value a method of the package that changes its receiver runs
/// on (`board.reset()`, `Board.reset(board)`, `self.reset()` in a method of `cls`), what it hands
/// the parameters a function or method of the package changes (`swap(a, b)`, `changing`), or what
/// a library's call changes (`library_change`).
pub(super) fn changed_by<'t>(seen: &Seen, changing: &Changing, c: TsNode<'t>, cls: Option<&str>) -> Vec<TsNode<'t>> {
    let src = &seen.m.src;
    let Some(f) = c.child_by_field_name("function") else { return vec![] };
    let obj = f.child_by_field_name("object").filter(|_| f.kind() == "attribute");
    let name = f.child_by_field_name("attribute").filter(|_| f.kind() == "attribute").map_or("", |a| text(a, src));
    let on_self = cls.filter(|_| obj.map(|o| text(o, src)) == Some("self")).and_then(|k| seen.store.method_of(k, name));
    let callee = match seen.callee(f) {
        Some(Callee::Def(t)) => Some(t),
        _ => None,
    };
    let mut out = vec![];
    for t in on_self.iter().chain(callee.iter()) {
        // a method called through its class runs on the value it is handed first
        let named = obj.and_then(|o| seen.value(o)).map_or(false, |v| v.is_class);
        let first = named && seen.store.nodes.get(t).map_or(false, |n| n.kind == NodeKind::Method && !n.on_class);
        if seen.store.mutating.contains(t) {
            out.extend(if first { first_argument(c) } else { obj });
        }
        if let Some((to, passed)) = passed(seen.modules, seen.store, t, c, first, src) {
            let ps = changing.get(&to).map_or(&[][..], Vec::as_slice);
            out.extend(passed.into_iter().filter(|(p, _)| ps.iter().any(|(q, _)| q == p)).map(|(_, a)| a));
        }
        if !out.is_empty() {
            return out;
        }
    }
    library_change(seen.facts, c, src).into_iter().collect()
}

/// What a body changes in place: every attribute or item it assigns (`self.x = ..`,
/// `p.items[k] = ..`, each of `a.x, b.y = ..`) or deletes, and what a call changes (`changed_by`).
pub(super) fn changed<'t>(body: TsNode<'t>, changes: &dyn Fn(TsNode<'t>) -> Vec<TsNode<'t>>) -> Vec<TsNode<'t>> {
    let mut out = vec![];
    for a in walk(body, &["assignment", "augmented_assignment"]) {
        let Some(l) = a.child_by_field_name("left") else { continue };
        let targets = if matches!(l.kind(), "pattern_list" | "tuple_pattern" | "list_pattern") { named_children(l) } else { vec![l] };
        out.extend(targets.into_iter().filter(|l| matches!(l.kind(), "attribute" | "subscript")));
    }
    // `del d[k]`, `del obj.x`
    for d in walk(body, &["delete_statement"]) {
        out.extend(
            walk(d, &["attribute", "subscript"])
                .into_iter()
                .filter(|t| t.parent().map_or(false, |p| p.kind() == "delete_statement" || p.kind() == "expression_list")),
        );
    }
    out.extend(walk(body, &["call"]).into_iter().flat_map(changes));
    out
}

/// What a call to a library changes in place, as ty says where it goes: the value a mutating
/// method is called on (`xs.append(x)` on a list), or the argument a function changes
/// (`setattr(obj, ..)`, `random.shuffle(xs)`).
pub(super) fn library_change<'t>(facts: Option<&Facts>, c: TsNode<'t>, src: &[u8]) -> Option<TsNode<'t>> {
    let f = c.child_by_field_name("function")?;
    let package = facts?.callees((f.start_byte(), f.end_byte())).iter().find_map(|x| match &x.target {
        Target::Lib(m) => Some(package_of(m)),
        Target::Here { .. } => None,
    })?;
    // a method handed its package's in-place keyword as `True` changes its receiver
    let in_place =
        knowledge::IN_PLACE.iter().any(|(p, k)| *p == package && keyword_node(c, src, k).map_or(false, |v| text(v, src) == "True"));
    match f.kind() {
        "attribute"
            if in_place || knowledge::mutates(knowledge::Registry::PyPi, &package, text(f.child_by_field_name("attribute")?, src)) =>
        {
            f.child_by_field_name("object")
        }
        _ => {
            let name = dotted(Some(f), src).last().cloned()?;
            let (_, _, i) = knowledge::CHANGES_ARGUMENT.iter().find(|(p, n, _)| *p == package && *n == name)?;
            positional_at(c, *i)
        }
    }
}

/// The value a call hands by keyword `k`.
fn keyword_node<'t>(call: TsNode<'t>, src: &[u8], k: &str) -> Option<TsNode<'t>> {
    named_children(call.child_by_field_name("arguments")?)
        .into_iter()
        .find(|a| a.kind() == "keyword_argument" && a.child_by_field_name("name").map(|n| text(n, src)) == Some(k))?
        .child_by_field_name("value")
}

/// The argument a call hands at position `i`.
fn positional_at(call: TsNode, i: usize) -> Option<TsNode> {
    named_children(call.child_by_field_name("arguments")?)
        .into_iter()
        .filter(|a| !matches!(a.kind(), "keyword_argument" | "comment" | "list_splat" | "dictionary_splat"))
        .nth(i)
}

/// What a call reaches: a function, method or class of the package, or a library, by its
/// package.
pub(super) enum Callee {
    Def(String),
    Lib(String),
}

/// The nodes of the package's definitions, by where their names are written: the file and the
/// byte offset. This is where ty's answers meet the graph.
pub(super) fn sites(modules: &IndexMap<String, Module>, store: &GraphBuilder) -> HashMap<(String, usize), String> {
    let mut out = HashMap::new();
    for n in store.nodes.values().filter(|n| matches!(n.kind, NodeKind::Function | NodeKind::Method | NodeKind::Class)) {
        let Some(m) = modules.get(&n.module) else { continue };
        if let Some(name) = def_node(m, n).and_then(|d| d.child_by_field_name("name")) {
            out.insert((crate::tree::path_str(&m.path), name.start_byte()), n.id.clone());
        }
    }
    out
}

/// What ty says about one module's expressions, in the graph's terms: the node or library a
/// call reaches, and the type a value has.
pub(super) struct Seen<'a> {
    pub(super) m: &'a Module,
    pub(super) facts: Option<&'a Facts>,
    pub(super) sites: &'a HashMap<(String, usize), String>,
    pub(super) modules: &'a IndexMap<String, Module>,
    pub(super) store: &'a GraphBuilder,
    pub(super) own: &'a Own,
    /// The class whose instance `self` holds, in a method that runs on one.
    pub(super) receiver: Option<String>,
    /// The class `cls` holds, in a method that runs on its class (`@classmethod`).
    pub(super) class: Option<String>,
}

impl<'a> Seen<'a> {
    pub(super) fn new(
        m: &'a Module,
        types: &'a Types,
        sites: &'a HashMap<(String, usize), String>,
        modules: &'a IndexMap<String, Module>,
        store: &'a GraphBuilder,
        own: &'a Own,
    ) -> Self {
        Seen { m, facts: types.of(&crate::tree::path_str(&m.path)), sites, modules, store, own, receiver: None, class: None }
    }

    fn range(n: TsNode) -> (usize, usize) {
        (n.start_byte(), n.end_byte())
    }

    fn is_class(&self, id: &str) -> bool {
        self.store.nodes.get(id).map_or(false, |n| n.kind == NodeKind::Class)
    }

    /// The node a definition is, by where its name is written.
    fn node(&self, t: &Target) -> Option<&'a String> {
        match t {
            Target::Here { file, at } => self.sites.get(&(file.clone(), *at)),
            Target::Lib(_) => None,
        }
    }

    /// What calling `f` reaches. ty answers first. A library's call is named by the package the
    /// code names it from, when it does (`sys.stdout.write` is `sys`'s), else by the package
    /// that defines it. Where ty knows nothing, a dotted name through what the module imports or
    /// defines is what that binds: without an environment, `requests.get` is still `get` of the
    /// module `requests`.
    pub(super) fn callee(&self, f: TsNode) -> Option<Callee> {
        // `cls(..)` in a method on its class builds an instance of that class
        if let Some(class) = self.class.as_ref().filter(|_| f.kind() == "identifier" && text(f, &self.m.src) == "cls") {
            return Some(Callee::Def(class.clone()));
        }
        let reached = self.facts.map_or(&[][..], |x| x.callees(Self::range(f)));
        let mut lib = None;
        for c in reached {
            let def = match (&c.target, c.by) {
                (Target::Lib(m), _) => {
                    lib.get_or_insert_with(|| package_of(m));
                    None
                }
                (t, By::Instance) => self.node(t).and_then(|cls| self.store.method_of(cls, "__call__")),
                (t, By::Call | By::Construct) => self.node(t).cloned(),
                (t, By::Name) => self
                    .node(t)
                    .filter(|id| {
                        self.store
                            .nodes
                            .get(*id)
                            .map_or(false, |n| matches!(n.kind, NodeKind::Function | NodeKind::Method | NodeKind::Class))
                    })
                    .cloned(),
            };
            if let Some(d) = def {
                return Some(Callee::Def(d));
            }
        }
        if let Some(lib) = lib {
            return Some(Callee::Lib(match self.bound(f) {
                Some(Callee::Lib(named)) => named,
                _ => lib,
            }));
        }
        if !reached.is_empty() {
            return None;
        }
        self.bound(f).or_else(|| self.on_library_value(f).map(Callee::Lib))
    }

    /// The library a method is called into where ty does not know the method: the one whose class
    /// the value the chain starts from is (`table[cols].assign(..)` on a pandas `DataFrame` is
    /// pandas'), where that value is of a library's class alone.
    fn on_library_value(&self, f: TsNode) -> Option<String> {
        let mut root = called_on(f)?;
        while let Some(inner) = match root.kind() {
            "attribute" => root.child_by_field_name("object"),
            "subscript" => root.child_by_field_name("value"),
            "call" => root.child_by_field_name("function").and_then(called_on),
            _ => None,
        } {
            root = inner;
        }
        let v = self.value(root).filter(|v| !v.is_class && !v.is_code)?;
        let mut packages = v.classes.iter().map(|t| match t {
            Target::Lib(m) => Some(package_of(m)),
            Target::Here { .. } => None,
        });
        let first = packages.next()??;
        packages.all(|p| p.as_deref() == Some(first.as_str())).then_some(first)
    }

    /// The package a definition is in: a library's, by its module, or one Atlas reads as code, by
    /// the first part of its node's name.
    pub(super) fn package(&self, t: &Target) -> Option<String> {
        match t {
            Target::Lib(module) => Some(package_of(module)),
            Target::Here { .. } => self.node(t).map(|id| id.split('.').next().unwrap_or("").to_string()),
        }
    }

    /// The package a name or an attribute comes from: where calling it goes, else where it is
    /// defined (`@cli.command` written without a call), else what the module imports it as.
    pub(super) fn library(&self, f: TsNode) -> Option<String> {
        let reached = self.facts.map_or(&[][..], |x| x.callees(Self::range(f)));
        let defined = || self.facts.and_then(|x| x.def(Self::range(f))).and_then(|t| self.package(t));
        let imported = || match self.bound(f) {
            Some(Callee::Lib(lib)) => Some(lib),
            _ => None,
        };
        reached.iter().find_map(|c| self.package(&c.target)).or_else(defined).or_else(imported)
    }

    /// The function of the package a value handed on is called as: a function or method, or an
    /// instance's `__call__`. Reading a property hands nothing on.
    fn handed_function(&self, a: TsNode) -> Option<String> {
        let reached = self.facts.map_or(&[][..], |x| x.callees(Self::range(a)));
        let function = |id: &&String| self.store.nodes.get(*id).map_or(false, |n| n.kind == NodeKind::Function);
        for c in reached {
            let def = match c.by {
                By::Call => self.node(&c.target).cloned(),
                By::Instance => self.node(&c.target).and_then(|cls| self.store.method_of(cls, "__call__")),
                By::Name => self.node(&c.target).filter(function).cloned(),
                By::Construct => None,
            };
            if def.is_some() {
                return def;
            }
        }
        match self.bound(a) {
            Some(Callee::Def(t)) if reached.is_empty() && function(&&t) => Some(t),
            _ => None,
        }
    }

    /// The types of a value that the picture draws: a class of the code Atlas reads, or a
    /// library's class by its package and name, as a signature names it (`ext:pandas.DataFrame`).
    fn drawn(&self, v: &Value, ext: &HashMap<(String, String), String>) -> Vec<String> {
        let mut out: Vec<String> = vec![];
        for (t, name) in v.classes.iter().zip(&v.names) {
            let id = match t {
                Target::Here { .. } => self.node(t).filter(|id| self.is_class(id)).cloned(),
                Target::Lib(module) => {
                    let pkg = package_of(module);
                    let id = ext.get(&(pkg.clone(), name.clone())).cloned().unwrap_or_else(|| format!("ext:{pkg}.{name}"));
                    foreign_type(&id, &OWN).then_some(id)
                }
            };
            if let Some(id) = id.filter(|id| !out.contains(id)) {
                out.push(id);
            }
        }
        out
    }

    /// What a call gives back, by the name it is assigned to: the type ty says, or, for a value
    /// whose type ty does not know, the name alone, when the call made it from what it was handed.
    fn gives(&self, c: TsNode, assigned: &str, hands: &[Held], ext: &HashMap<(String, String), String>) -> Vec<Held> {
        let known = self.value(c).filter(|v| !v.is_class && !v.is_code);
        let mut gives: Vec<Held> =
            known.map(|v| self.drawn(v, ext)).unwrap_or_default().into_iter().map(|ty| Held::new(ty, assigned)).collect();
        if gives.is_empty() && known.map_or(true, |v| v.shown == "Unknown") && !assigned.is_empty() && !hands.is_empty() {
            gives.push(Held::new(String::new(), assigned));
        }
        gives
    }

    /// A loop as the step it is: what it takes (what it goes over), and what it gives, each
    /// element under the name it binds (`for n in g.nodes` takes `g` and gives `n`). It gives
    /// nothing where its elements are no values Atlas draws.
    fn each(&self, l: TsNode, ext: &HashMap<(String, String), String>, made: &HashSet<String>) -> (Vec<Held>, Vec<Held>) {
        let (Some(left), Some(right)) = (l.child_by_field_name("left"), l.child_by_field_name("right")) else { return (vec![], vec![]) };
        let src = &self.m.src;
        let names: Vec<TsNode> = if left.kind() == "identifier" { vec![left] } else { walk(left, &["identifier"]) };
        let hands = self.held_in(right, ext, made);
        // one name holds an element of what the loop goes over: of a `list[Node]` a `Node`, where ty does not say it of the name
        let over = self.value(right).filter(|v| !v.is_class && !v.is_code);
        let element: Vec<String> = over.map(|v| self.drawn(v, ext)).unwrap_or_default();
        // what it goes over is of a known type that holds none Atlas draws (a `list[str]`): its elements are no values it draws
        let undrawn = element.is_empty() && over.is_some_and(|v| v.shown != "Unknown");
        let one = names.len() == 1;
        let gives: Vec<Held> = names
            .into_iter()
            .flat_map(|n| {
                let named = self.gives(n, text(n, src), &hands, ext);
                if !named.iter().all(|h| h.ty.is_empty()) {
                    named
                } else if one && !element.is_empty() {
                    element.iter().map(|t| Held::new(t.clone(), text(n, src))).collect()
                } else if undrawn {
                    vec![]
                } else {
                    named
                }
            })
            .collect();
        (hands, gives)
    }

    /// Whether a call is a step of its own: a call of the program's code, or one Atlas cannot
    /// follow. A library's call is glued into the call around it.
    fn steps_itself(&self, c: TsNode) -> bool {
        c.child_by_field_name("function").is_some_and(|f| !matches!(self.callee(f), Some(Callee::Lib(_))))
    }

    /// What an expression hands a call: the value it gives where it is a call, and the names in
    /// it with the values they hold (`data[before]` hands `data`, `self.path` the instance the
    /// method runs on). A call in it that is a step of its own takes its names itself and hands
    /// what it gives (`Message(level.prefix())` is handed what `prefix` gives, not `level`). A
    /// class or a function named is no value, and neither is a name a `lambda` in it binds.
    fn held_in(&self, e: TsNode, ext: &HashMap<(String, String), String>, made: &HashSet<String>) -> Vec<Held> {
        self.held(e, ext, made, false)
    }

    /// What a call changes in `e`, the value it is called on: as `held_in`, with the instance the
    /// method runs on by its name, `self`, the value the change makes anew.
    fn changed_in(&self, e: TsNode, ext: &HashMap<(String, String), String>, made: &HashSet<String>) -> Vec<Held> {
        self.held(e, ext, made, true)
    }

    fn held(&self, e: TsNode, ext: &HashMap<(String, String), String>, made: &HashSet<String>, changed: bool) -> Vec<Held> {
        let mut out: Vec<Held> = vec![];
        let mut add = |ty: String, name: String, at: usize, param: bool| {
            if !out.iter().any(|h| h.ty == ty && h.name == name) {
                out.push(Held::at(ty, name, at, param));
            }
        };
        let data = |v: &&Value| !v.is_class && !v.is_code;
        let src = &self.m.src;
        let mut names = vec![];
        // what a library's call `e` gives, glued into the call it is handed to
        let mut glued = vec![];
        let mut stack = vec![e];
        while let Some(x) = stack.pop() {
            let nested = x.kind() == "call" && self.steps_itself(x);
            if x.kind() == "call" && (nested || x.id() == e.id()) {
                for ty in self.value(x).filter(data).map(|v| self.drawn(v, ext)).unwrap_or_default() {
                    match nested {
                        true => add(ty, String::new(), x.start_byte(), false),
                        false => glued.push((ty, x.start_byte())),
                    }
                }
            }
            // a loop in it that is a step takes what it goes over itself
            if nested || (x.kind() == "for_in_clause" && !self.each(x, ext, made).1.is_empty()) {
                continue;
            }
            // a name, not the attribute a projection reads (`.width`) or the keyword an argument is handed by
            let label = x.parent().is_some_and(|p| {
                let field = |f: &str| p.child_by_field_name(f).is_some_and(|n| n.id() == x.id());
                (p.kind() == "attribute" && field("attribute")) || (p.kind() == "keyword_argument" && field("name"))
            });
            if x.kind() == "identifier" && !label && !lambda_binds(x, e, src) {
                names.extend(self.projected(x, made, !changed));
            }
            stack.extend(named_children(x).into_iter().rev());
        }
        for i in names {
            let name = text(i, src);
            if name == "cls" {
                continue;
            }
            // `self` holds the instance the method runs on, from beyond the body until a step makes
            // it anew
            if name == "self" {
                let named = if changed || made.contains(name) { name } else { "" };
                if let Some(cls) = &self.receiver {
                    add(cls.clone(), named.to_string(), i.start_byte(), false);
                }
                continue;
            }
            let v = self.value(i);
            if v.map_or(false, |v| v.is_class || v.is_code) {
                continue;
            }
            let tys = v.map(|v| self.drawn(v, ext)).unwrap_or_default();
            // a name of no type Atlas draws is still a value where an earlier step made it, and where
            // the call makes it anew; one of the function's own parameters holds what its calls fill it with,
            // which is known once every body is read (`builder::settle_parameters`)
            if tys.is_empty() && (made.contains(name) || changed) {
                add(String::new(), name.to_string(), i.start_byte(), false);
            } else if tys.is_empty() && own_parameter(i, src) {
                add(String::new(), name.to_string(), i.start_byte(), true);
            }
            for ty in tys {
                add(ty, name.to_string(), i.start_byte(), false);
            }
        }
        // what a library's call gives goes on from the values it is handed; only where it is handed
        // none is it handed on by its type
        if out.is_empty() {
            for (ty, at) in glued {
                out.push(Held::at(ty, String::new(), at, false));
            }
        }
        out
    }

    /// The names a projection bound to `i`'s name starts from, where the bindings of that name that
    /// reach `i` bind it to one: `width = settings.width` makes `width` in `f(width)` the projection
    /// `settings.width`, which hands `settings`, and so do `width, height = settings.size`, a lookup
    /// `row = rows[k]`, a name a `case` pattern binds (a part of what the `match` takes apart, through
    /// the variant the case matches), and either part of `a.x if c else b.y`. A name bound in the arms
    /// of a choice before `i` holds what each arm binds. A name a step made holds what the step gave.
    /// Where `computed`, a name holds what an operator computes it from as well (`sources`); a change
    /// made through a name changes only what the name is or is a part of.
    fn projected<'t>(&self, i: TsNode<'t>, made: &HashSet<String>, computed: bool) -> Vec<TsNode<'t>> {
        self.followed(i, made, computed).0
    }

    /// The names the name `i` stands for where it stands, as `projected` follows it with `computed`: those a binding it
    /// follows binds a name to by what Python's own code computes of them (`gap = first.db - second.db`,
    /// `body::stands_for`).
    fn stands<'t>(&self, i: TsNode<'t>, made: &HashSet<String>) -> Vec<TsNode<'t>> {
        self.followed(i, made, true).1
    }

    /// The names `i` is followed to (`projected`), and the names it is followed through that stand where a binding binds a
    /// name to what Python's own code computes of them (`stands`).
    fn followed<'t>(&self, i: TsNode<'t>, made: &HashSet<String>, computed: bool) -> (Vec<TsNode<'t>>, Vec<TsNode<'t>>) {
        let src = &self.m.src;
        let (mut out, mut stood, mut todo, mut seen) = (vec![], vec![], vec![(i, 0)], HashSet::new());
        while let Some((at, depth)) = todo.pop() {
            let name = text(at, src);
            if !seen.insert(at.id()) || depth > 8 || made.contains(name) || name == "self" {
                out.push(at);
                continue;
            }
            let rights: Vec<TsNode> = match reaching(at, name, src) {
                bound if !bound.is_empty() => bound.into_iter().filter_map(|b| b.child_by_field_name("right")).collect(),
                _ => matched_by(at, name, src).or_else(|| looped_by(at, name, src)).into_iter().collect(),
            };
            let parts: Vec<(TsNode, Vec<Option<TsNode>>)> =
                rights.into_iter().map(|r| (r, if computed { sources(r, src) } else { projections(r) })).collect();
            let roots: Vec<Option<TsNode>> = parts.iter().flat_map(|(_, roots)| roots.iter().copied()).collect();
            if roots.is_empty() || roots.iter().any(|r| r.is_none_or(|r| r.id() == at.id())) {
                out.push(at);
                continue;
            }
            for (_, roots) in parts.iter().filter(|(r, _)| computed && (USES.computes)(beneath(*r, WRAPPERS), src)) {
                stood.extend(roots.iter().flatten());
            }
            todo.extend(roots.into_iter().flatten().map(|r| (r, depth + 1)));
        }
        (out, stood)
    }

    /// Whether class `c` is `b` or descends from it.
    fn descends(&self, c: &str, b: &str) -> bool {
        let mut todo = vec![c.to_string()];
        let mut seen = HashSet::new();
        while let Some(x) = todo.pop() {
            if x == b {
                return true;
            }
            if seen.insert(x.clone()) {
                todo.extend(self.store.classes.get(&x).map(|i| i.resolved.clone()).unwrap_or_default());
            }
        }
        false
    }

    /// A class of the code Atlas reads, handed on as a value.
    fn handed_class(&self, a: TsNode) -> Option<String> {
        let reached = self.facts.map_or(&[][..], |x| x.callees(Self::range(a)));
        reached.iter().filter(|c| c.by == By::Construct).find_map(|c| self.node(&c.target).filter(|t| self.is_class(t)).cloned())
    }

    /// Whether a library call that its package's table marks as reaching a place does move data,
    /// as its types say: building a client (a web or LLM library's class) moves nothing, and a
    /// network library's call moves data only when it is handed a URL or sends on a socket. Where
    /// the types are not known, the table stands.
    fn moves(&self, f: TsNode, kind: &Crossing, lib: Option<&str>, op: &str) -> bool {
        let reached = self.facts.map_or(&[][..], |x| x.callees(Self::range(f)));
        if lib.map_or(false, effects::client) && reached.iter().any(|c| c.by == By::Construct && matches!(c.target, Target::Lib(_))) {
            return false;
        }
        match self.facts.and_then(|x| x.signature(Self::range(f))) {
            Some(sig) if kind.world == World::Net && lib.map_or(false, effects::network) => {
                effects::requests(&sig.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), op)
            }
            _ => true,
        }
    }

    /// What a dotted name reaches through what the module imports or defines.
    fn bound(&self, f: TsNode) -> Option<Callee> {
        let parts = call_path(f, &self.m.src);
        let head = parts.first()?;
        if !self.m.bindings.contains_key(head) && !self.m.symbols.contains_key(head) {
            return None;
        }
        let r = resolve(self.m, &parts, self.modules, &self.store.nodes, self.own)?;
        Some(match lib_of(&r) {
            Some(lib) => Callee::Lib(lib),
            None => Callee::Def(r),
        })
    }

    /// The type ty gives the expression `e`.
    pub(super) fn value(&self, e: TsNode) -> Option<&'a Value> {
        self.facts?.value(Self::range(e))
    }

    /// The classes of the package a type names: `list[Node]` -> `Node`.
    pub(super) fn classes(&self, v: &Value) -> Vec<String> {
        v.classes.iter().filter_map(|t| self.node(t)).filter(|id| self.is_class(id)).cloned().collect()
    }

    /// The class of the package a library's classmethod makes when called on it:
    /// `Node.model_validate(row)` gives back a `Node`, so it makes one.
    fn made_on(&self, call: TsNode, f: TsNode) -> Option<String> {
        let on = self.value(f.child_by_field_name("object")?).filter(|v| v.is_class)?;
        let made = self.value(call)?;
        let cls = self.classes(on).into_iter().next()?;
        (made.classes.len() == 1 && self.classes(made) == [cls.clone()]).then_some(cls)
    }

    /// The names a call is handed that hold the package's types, with the type ty gives them.
    fn handed(&self, call: TsNode) -> HashMap<String, String> {
        let Some(args) = call.child_by_field_name("arguments") else { return HashMap::new() };
        walk(args, &["identifier"])
            .into_iter()
            .filter_map(|id| {
                let v = self.value(id)?;
                (!self.classes(v).is_empty()).then(|| (text(id, &self.m.src).to_string(), v.shown.clone()))
            })
            .collect()
    }
}

/// What a call hands a parameter: at its position, or by its name.
pub(super) fn handed<'t>(call: TsNode<'t>, at: usize, name: &str, src: &[u8]) -> Option<TsNode<'t>> {
    let args = named_children(call.child_by_field_name("arguments")?);
    let positional =
        args.into_iter().filter(|a| !matches!(a.kind(), "keyword_argument" | "comment" | "list_splat" | "dictionary_splat")).nth(at);
    positional.or_else(|| by_name(call, name, src))
}

/// What a call hands a parameter by its name.
pub(super) fn by_name<'t>(call: TsNode<'t>, name: &str, src: &[u8]) -> Option<TsNode<'t>> {
    named_children(call.child_by_field_name("arguments")?)
        .into_iter()
        .find(|a| a.kind() == "keyword_argument" && a.child_by_field_name("name").map(|n| text(n, src)) == Some(name))
        .and_then(|a| a.child_by_field_name("value"))
}

/// I/O functions named without being called: `READERS = {".csv": pd.read_csv}` reads csv
/// wherever `READERS` is used. Keyed by the module-level name that holds them.
fn io_tables(m: &Module, facts: Option<&Facts>, own: &Own) -> HashMap<String, Vec<Touch>> {
    let mut out: HashMap<String, Vec<Touch>> = HashMap::new();
    for a in top_assignments(m) {
        let (Some(left), Some(right)) = (a.child_by_field_name("left"), a.child_by_field_name("right")) else { continue };
        if left.kind() != "identifier" {
            continue;
        }
        let ts = io_refs(m, facts, right, own, text(left, &m.src));
        if !ts.is_empty() {
            out.insert(text(left, &m.src).to_string(), ts);
        }
    }
    out
}

/// Attributes under `n` that name an I/O function of a library without calling it.
fn io_refs(m: &Module, facts: Option<&Facts>, n: TsNode, own: &Own, what: &str) -> Vec<Touch> {
    let mut out = vec![];
    for a in walk(n, &["attribute"]) {
        let called = a.parent().map_or(false, |p| p.kind() == "call" && p.child_by_field_name("function").map(|f| f.id()) == Some(a.id()));
        // `openai.api_key = ..` assigns the attribute; it names no function
        let assigned =
            a.parent().map_or(false, |p| p.kind() == "assignment" && p.child_by_field_name("left").map(|l| l.id()) == Some(a.id()));
        if called || assigned || a.parent().map_or(false, |p| p.kind() == "attribute") {
            continue;
        }
        let parts = dotted(Some(a), &m.src);
        // a function, as ty types it; `RoutingControl.READ` is a constant, not a call to come
        if !facts.and_then(|f| f.value((a.start_byte(), a.end_byte()))).map_or(false, |v| v.is_code) {
            continue;
        }
        let Some(pkg) = parts.first().and_then(|h| m.bindings.get(h)).filter(|b| !own.holds(b)).and_then(|b| b.split('.').next()) else {
            continue;
        };
        let Some(tag) = effects::boundary(Some(pkg), &parts) else { continue };
        let op = parts.last().cloned().unwrap_or_default();
        let f = effects::facts(tag, Some(pkg), &op, &[]);
        out.push(Touch { what: what.into(), op, artifact: f.artifact, ..Touch::new(f.kind, f.dir) });
    }
    out
}

/// A body's calls, as edges and touches. Gives back the functions it hands to a managed
/// transaction, with the database and the direction the transaction says.
/// One call's arguments to a function of the package: which parameter each is handed to, where
/// the call is, and which of the callee's parameters it leaves out.
pub(super) struct Arg<'t> {
    pub(super) caller: String,
    pub(super) callee: String,
    pub(super) module: String,
    pub(super) scope: Option<TsNode<'t>>,
    /// The caller's parameters.
    pub(super) params: Vec<String>,
    pub(super) passed: Vec<(String, TsNode<'t>)>,
}

/// A call with what it hands each parameter, evaluated in the caller.
pub(super) struct Evaluated<'a, 't> {
    pub(super) arg: &'a Arg<'t>,
    pub(super) modules: &'a IndexMap<String, Module>,
    pub(super) eval: &'a Eval<'a>,
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
        let m = self.modules.get(&self.arg.module)?;
        Some(self.eval.of(At { m, scope: self.arg.scope, params: &self.arg.params }, *node, 0))
    }
}

/// What reading a body gives besides its edges and touches: the work it hands a managed
/// transaction, its file and database calls with their paths and statements as built (placed
/// once the parameters' values are known), and the arguments of its calls.
#[derive(Default)]
pub(super) struct Linked<'t> {
    pub(super) handed: Vec<(String, World, Dir)>,
    pub(super) waiting: Vec<(Touch, Vec<Built>)>,
    pub(super) args: Vec<Arg<'t>>,
}

pub(super) fn link_calls<'t>(
    modules: &'t IndexMap<String, Module>,
    store: &mut GraphBuilder,
    types: &Types,
    sites: &HashMap<(String, usize), String>,
    ext: &HashMap<(String, String), String>,
    eval: &Eval,
    own: &Own,
    changing: &Changing,
    id: &str,
) -> Linked<'t> {
    let n = store.nodes[id].clone();
    let m = &modules[&n.module];
    let Some(d) = def_node(m, &n) else { return Linked::default() };
    let in_module = n.kind == NodeKind::Module;
    let Some(body) = (if in_module { Some(d) } else { d.child_by_field_name("body") }) else { return Linked::default() };
    let mut steps = Steps::new(body, &m.src, &CHOICES, &USES);
    let read = {
        let mut seen = Seen::new(m, types, sites, modules, store, own);
        seen.receiver = n.cls.clone().filter(|_| n.kind == NodeKind::Method && !n.on_class && n.params.iter().any(|p| p.receiver));
        seen.class = n.cls.clone().filter(|_| n.kind == NodeKind::Method && n.on_class);
        let mut b = Body::new(seen, modules, ext, eval, changing, &n, d, body);
        for r in b.steps() {
            let c = match r {
                Ran::Loop(l) => {
                    b.each(&mut steps, l);
                    continue;
                }
                Ran::Words(w) => {
                    b.words(&mut steps, w);
                    continue;
                }
                Ran::Call(c) => c,
            };
            let (t, w) = (b.read.touches.len(), b.read.waiting.len());
            b.call(&mut steps, c);
            steps.mark(c, b.read.touches[t..].iter_mut().chain(b.read.waiting[w..].iter_mut().map(|(t, _)| t)));
        }
        let kept = steps.kept.clone();
        let held = |n| if b.holds(n) { b.seen.held_in(n, b.ext, &kept) } else { vec![] };
        let stands = |n| if b.holds(n) { b.seen.stands(n, &kept) } else { vec![] };
        steps.uses(&held, &stands);
        steps.fills(&held);
        b.class_state(&mut steps);
        b.ends();
        b.read
    };
    let compares = (!in_module).then(|| {
        // a qualifier is a type when ty says it is a class, or when it names a class of the program
        let facts = types.of(&crate::tree::path_str(&m.path));
        let is_type = |(a, b): (usize, usize)| {
            facts.and_then(|f| f.value((a, b))).is_some_and(|v| v.is_class) || {
                let parts: Vec<String> = String::from_utf8_lossy(&m.src[a..b]).split('.').map(str::to_string).collect();
                resolve(m, &parts, modules, &store.nodes, own).and_then(|t| store.nodes.get(&t)).is_some_and(|t| t.kind == NodeKind::Class)
            }
        };
        let receiver = n.params.iter().find(|p| p.receiver && !n.on_class).map(|p| p.name.as_str());
        crate::tree::compares(body, &m.src, steps.choices(), &COMPARES, &CHOICES, &crate::tree::Scope { receiver, is_type: &is_type })
    });
    steps.finish(store, id);
    if let Some(node) = store.nodes.get_mut(id) {
        node.calls_handed = read.unresolved.iter().any(|(u, _)| n.params.iter().any(|p| &p.name == u));
    }
    for (name, bucket) in read.unresolved {
        store.not_followed(name, bucket);
    }
    for t in read.new_ext {
        store.ext_node(&t);
    }
    for t in read.used {
        store.used.insert((id.to_string(), t));
    }
    if let (Some(compares), Some(node)) = (compares, store.nodes.get_mut(id)) {
        node.compares = compares;
        node.raises = read.exits
            || crate::tree::holds(
                body,
                &m.src,
                |n, _| matches!(n.kind(), "raise_statement" | "assert_statement"),
                &["function_definition", "class_definition"],
            );
    }
    for (t, input) in read.changes {
        store.add_edge(id, &t, EdgeKind::Mutates, Some(input));
    }
    for t in read.touches {
        store.touch(id, t);
    }
    Linked { handed: read.handed, waiting: read.waiting, args: read.args }
}

/// What reading a body finds besides its steps: what it touches, the file and database calls
/// whose paths and statements wait for what callers hand, the calls of the package's functions with their arguments, the work a
/// managed transaction is handed, the directions the body says to a database's driver, the
/// calls Atlas could not follow, the functions whose value it uses, the inputs it changes in
/// place, the library types its values have, and whether it calls what ends the program.
#[derive(Default)]
struct Read<'t> {
    touches: Vec<Touch>,
    waiting: Vec<(Touch, Vec<Built>)>,
    args: Vec<Arg<'t>>,
    handed: Vec<(String, World, Dir)>,
    declared: Vec<(World, Dir)>,
    unresolved: Vec<(String, Bucket)>,
    used: Vec<String>,
    changes: Vec<(String, String)>,
    new_ext: Vec<String>,
    exits: bool,
}

/// A function's body, or a module's own code, as Python reads it: the types ty gives its
/// expressions, and where its parameters come from.
/// What a body runs, in order: a call, or a loop that binds a name to each element of a value.
enum Ran<'t> {
    Call(TsNode<'t>),
    Loop(TsNode<'t>),
    /// The command line the program is run with, read word by word and kept under a name
    /// (`args = sys.argv[1:]`): the end of what reads it.
    Words(TsNode<'t>),
}

struct Body<'a, 't> {
    seen: Seen<'a>,
    modules: &'t IndexMap<String, Module>,
    m: &'t Module,
    src: &'t [u8],
    ext: &'a HashMap<(String, String), String>,
    eval: &'a Eval<'a>,
    id: String,
    cls: Option<String>,
    initializer: bool,
    in_module: bool,
    body: TsNode<'t>,
    /// The function the body belongs to, and its parameters by name.
    scope: Option<TsNode<'t>>,
    own_params: Vec<String>,
    /// What a change to each typed parameter changes (`changed_types`), and the parameters that
    /// hold one of the language's own containers that change in place.
    params: HashMap<String, String>,
    changeable: HashSet<String>,
    own: &'a Own,
    changing: &'a Changing,
    read: Read<'t>,
    /// The names that hold what a library's call gave: a method called on one is that library's
    /// operation, also where its types do not list it (`response.read()` of `urlopen(..) as response`).
    library_values: HashSet<String>,
}

impl<'a, 't> Body<'a, 't> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        seen: Seen<'a>,
        modules: &'t IndexMap<String, Module>,
        ext: &'a HashMap<(String, String), String>,
        eval: &'a Eval<'a>,
        changing: &'a Changing,
        n: &Node,
        d: TsNode<'t>,
        body: TsNode<'t>,
    ) -> Self {
        let m = &modules[&n.module];
        let in_module = n.kind == NodeKind::Module;
        let own = seen.own;
        let own_params = if in_module { vec![] } else { paths::parameters(d, &m.src, false).into_iter().map(|(p, _)| p).collect() };
        let (params, changeable) =
            if in_module { Default::default() } else { changed_types(m, d, modules, &seen.store.nodes, own, seen.facts) };
        Body {
            seen,
            modules,
            m,
            src: &m.src,
            ext,
            eval,
            id: n.id.clone(),
            cls: n.cls.clone(),
            initializer: n.ctor && !n.on_class,
            in_module,
            body,
            scope: (!in_module).then_some(d),
            own_params,
            params,
            changeable,
            own,
            changing,
            read: Read::default(),
            library_values: HashSet::new(),
        }
    }

    /// Where an expression of the body is evaluated.
    fn at(&self) -> At<'_, 't> {
        At { m: self.m, scope: self.scope, params: &self.own_params }
    }

    /// Whether `n` is the body's own: in a module's own code, not inside a definition, which
    /// belongs to that definition.
    fn holds(&self, n: TsNode) -> bool {
        let mut p = n.parent();
        while let Some(pp) = p.filter(|pp| pp.id() != self.body.id()) {
            if self.in_module && matches!(pp.kind(), "function_definition" | "class_definition") {
                return false;
            }
            p = pp.parent();
        }
        true
    }

    /// The body's calls and the loops that bind a name to each element of a value, in the order
    /// they run: a loop binds its element once what it goes over is made and before its body runs,
    /// and the element of a comprehension (`[show(n) for n in nodes]`) runs after its loops.
    fn steps(&self) -> Vec<Ran<'t>> {
        let comprehensions = ["list_comprehension", "set_comprehension", "dictionary_comprehension", "generator_expression"];
        let loops = walk(self.body, &["for_statement", "for_in_clause"]).into_iter().filter(|l| self.holds(*l));
        let mut out: Vec<((usize, u8, usize), Ran<'t>)> =
            loops.filter_map(|l| l.child_by_field_name("right").map(|r| ((r.end_byte(), 1, 0), Ran::Loop(l)))).collect();
        for c in evaluation_order(walk(self.body, &["call"])).into_iter().filter(|c| self.holds(*c)) {
            // a call in a comprehension's element runs after the comprehension's loops
            let mut key = (c.end_byte(), 0, 0);
            let mut x = c;
            while let Some(q) = x.parent().filter(|_| x.id() != self.body.id()) {
                if comprehensions.contains(&q.kind()) && q.child_by_field_name("body").map(|b| b.id()) == Some(x.id()) {
                    let after =
                        named_children(q).into_iter().filter(|k| k.kind() == "for_in_clause").map(|k| k.end_byte()).max().unwrap_or(0);
                    key = (key.0.max(after), 2, c.end_byte());
                }
                x = q;
            }
            out.push((key, Ran::Call(c)));
        }
        // the command line kept under a name; one a loop goes over is that loop's
        for w in self.words_read() {
            let looped = w.parent().is_some_and(|l| matches!(l.kind(), "for_statement" | "for_in_clause"));
            if !looped && (kept_as(w, self.src).is_some() || !unpacked_as(w, self.src).is_empty()) {
                out.push(((w.end_byte(), 0, 0), Ran::Words(w)));
            }
        }
        out.sort_by_key(|(k, _)| *k);
        out.into_iter().map(|(_, r)| r).collect()
    }

    /// Where the body reads the command line the program is run with (`knowledge::PYTHON_COMMAND_LINE`,
    /// `sys.argv`): the end of each chain that starts at it, through the items and the methods taken
    /// of it (`sys.argv[1:]`).
    fn words_read(&self) -> Vec<TsNode<'t>> {
        let src = self.src;
        let reads = |n: &TsNode| {
            let parts = dotted(Some(*n), src);
            let Some((head, rest)) = parts.split_first() else { return false };
            let Some(bound) = self.m.bindings.get(head) else { return false };
            let full: Vec<&str> = bound.trim_start_matches("ext:").split('.').chain(rest.iter().map(String::as_str)).collect();
            knowledge::PYTHON_COMMAND_LINE.contains(&full.as_slice())
        };
        let mut out = vec![];
        for n in walk(self.body, &["attribute", "identifier"]).into_iter().filter(|n| self.holds(*n) && reads(n)) {
            // `sys.argv` itself, not the `sys` in it
            if n.parent().is_some_and(|p| p.kind() == "attribute" && p.child_by_field_name("object").is_some_and(|o| o.id() == n.id())) {
                continue;
            }
            let mut end = n;
            loop {
                let value = outermost(end, WRAPPERS);
                let Some(p) = value.parent() else { break };
                let on = |f: &str| p.child_by_field_name(f).is_some_and(|x| x.id() == value.id());
                end = match p.kind() {
                    "subscript" if on("value") => p,
                    "attribute" if on("object") => match p.parent().filter(|c| c.kind() == "call") {
                        Some(call) => call,
                        None => break,
                    },
                    _ => break,
                };
            }
            out.push(outermost(end, WRAPPERS));
        }
        out
    }

    /// The command line kept under a name: a step that parses it with no options, since the code
    /// names none, giving the words under the names it keeps them by.
    fn words(&mut self, steps: &mut Steps<'t, '_>, w: TsNode<'t>) {
        let unpacked = unpacked_as(w, self.src);
        let names: Vec<String> = match kept_as(w, self.src) {
            Some(n) => vec![n],
            None => unpacked.iter().map(|n| text(*n, self.src).to_string()).collect(),
        };
        let gives = names.into_iter().map(|n| Held::new(String::new(), n)).collect();
        steps.parses(w, "argv", vec![], gives, vec![]);
    }

    /// A loop as the step it is: it takes what it goes over, and gives each element under the
    /// name it binds (`for n in g.nodes` takes `g` and gives `n`).
    fn each(&mut self, steps: &mut Steps<'t, '_>, l: TsNode<'t>) {
        let (Some(left), Some(right)) = (l.child_by_field_name("left"), l.child_by_field_name("right")) else { return };
        let (hands, mut gives) = self.seen.each(l, self.ext, &steps.kept);
        self.read.new_ext.extend(hands.iter().chain(&gives).map(|h| h.ty.clone()).filter(|t| t.starts_with("ext:")));
        // the step stands where what it goes over is written: a call there hands it what it gives, the loop's body does not
        let label = format!("for {} in {}", text(left, self.src), text(right, self.src));
        // a loop over the command line keeps each word under the name it binds: it parses the command line
        if self.words_read().iter().any(|w| w.id() == right.id()) {
            let names = if left.kind() == "identifier" { vec![left] } else { walk(left, &["identifier"]) };
            if gives.is_empty() {
                gives = names.into_iter().map(|n| Held::new(String::new(), text(n, self.src))).collect();
            }
            return steps.parses(right, &label, hands, gives, vec![]);
        }
        steps.each(right, &label, hands, gives);
    }

    /// What a call of the language's own that runs the function `a` it is handed on each element of
    /// what it goes over hands that function (`knowledge::PYTHON_EACH`): the values the element is a
    /// part of, and the parameter it fills (`map(f, rows)`, `sorted(rows, key=f)`, `rows.sort(key=f)`).
    fn per_element(&self, c: TsNode<'t>, a: TsNode<'t>, receiver: Option<TsNode<'t>>, kept: &HashSet<String>) -> Option<(Vec<Held>, Slot)> {
        let name = dotted(c.child_by_field_name("function"), self.src).last().cloned()?;
        let e = knowledge::PYTHON_EACH.iter().find(|e| e.name == name && (e.over.is_some() || receiver.is_some()))?;
        let function = match e.function {
            knowledge::Handed::At(i) => positional_at(c, i),
            knowledge::Handed::Key(k) => keyword_node(c, self.src, k),
        };
        if function.map(|f| f.id()) != Some(a.id()) {
            return None;
        }
        let over = match e.over {
            Some(i) => positional_at(c, i),
            None => receiver,
        }?;
        Some((self.seen.held_in(over, self.ext, kept), Slot::At(e.element)))
    }

    /// What a call is handed: the value it is called on, and its arguments. A method called on
    /// `self` itself runs on the instance the body runs on, which its step says (`on_receiver`);
    /// `self` inside an argument or a part of it (`self.path`, `self.cells.clear()`) is handed.
    fn hands(&self, c: TsNode, receiver: Option<TsNode>, kept: &HashSet<String>) -> Vec<Held> {
        let receiver = receiver.filter(|r| !(r.kind() == "identifier" && text(*r, self.src) == "self"));
        let args = c.child_by_field_name("arguments").map(named_children).unwrap_or_default();
        let values = args.into_iter().filter(|x| x.kind() != "comment").map(|x| {
            if x.kind() == "keyword_argument" {
                x.child_by_field_name("value").unwrap_or(x)
            } else {
                x
            }
        });
        let mut hands: Vec<Held> = receiver.into_iter().chain(values).flat_map(|v| self.seen.held_in(v, self.ext, kept)).collect();
        let mut seen = HashSet::new();
        hands.retain(|h| seen.insert(h.clone()));
        hands
    }

    /// What a call gives back, by the name it is assigned to: the type ty says, or, for a value
    /// whose type ty does not know, the name alone, when the call made it from what it was handed.
    fn gives(&self, c: TsNode, assigned: &str, hands: &[Held]) -> Vec<Held> {
        let mut gives = self.seen.gives(c, assigned, hands, self.ext);
        // a value of no type of the code's is still the value the name holds
        if gives.is_empty() && !assigned.is_empty() {
            gives.push(Held::new(String::new(), assigned));
        }
        gives
    }

    /// The library's class of the error a call builds, as ty says it: `ext:builtins.RuntimeError`.
    fn raised_class(&self, c: TsNode) -> Option<String> {
        let v = self.seen.value(c)?;
        v.classes.iter().zip(&v.names).find_map(|(t, name)| match t {
            Target::Lib(module) => {
                let pkg = package_of(module);
                Some(self.ext.get(&(pkg.clone(), name.clone())).cloned().unwrap_or_else(|| format!("ext:{pkg}.{name}")))
            }
            Target::Here { .. } => None,
        })
    }

    /// The values a class keeps that the body changes by assignment: `Counter.total += 1`,
    /// `cls.handlers[name] = fn`. An attribute of a class is no place Atlas draws, so what the
    /// change leaves behind it does not see.
    fn class_state(&self, steps: &mut Steps<'t, '_>) {
        if self.in_module {
            return;
        }
        for a in walk(self.body, &["assignment", "augmented_assignment"]) {
            let Some(mut root) = a.child_by_field_name("left") else { continue };
            let mut through = false;
            while let Some(inner) = match root.kind() {
                "attribute" => root.child_by_field_name("object"),
                "subscript" => root.child_by_field_name("value"),
                _ => None,
            } {
                (root, through) = (inner, true);
            }
            if through && root.kind() == "identifier" && self.seen.value(root).map_or(false, |v| v.is_class) {
                steps.unseen(a, text(a, self.src));
            }
        }
    }

    /// One call of the body, as the step it is: the program's own function or class, or a
    /// library's, which may make a value the body keeps and may cross into the world.
    fn call(&mut self, steps: &mut Steps<'t, '_>, c: TsNode<'t>) {
        let Some(fnode) = c.child_by_field_name("function") else { return };
        let callee = self.seen.callee(fnode);
        let receiver = (fnode.kind() == "attribute").then(|| fnode.child_by_field_name("object")).flatten();
        let hands = self.hands(c, receiver, &steps.kept);
        let assigned = kept_as(c, self.src).unwrap_or_default();
        // `a, b = f(..)` names each part of what the call gives
        let unpacked = unpacked_as(c, self.src);
        let gives = match unpacked.is_empty() {
            true => self.gives(c, &assigned, &hands),
            false => unpacked.iter().flat_map(|n| self.gives(*n, text(*n, self.src), &hands)).collect(),
        };
        let assigned =
            if unpacked.is_empty() { assigned } else { unpacked.iter().map(|n| text(*n, self.src)).collect::<Vec<_>>().join(", ") };
        self.read.new_ext.extend(hands.iter().chain(&gives).map(|h| h.ty.clone()).filter(|t| t.starts_with("ext:")));
        steps.keep(&gives);
        // a function of the package handed on as a value (`executor.submit(self.run, x)`,
        // `Thread(target=work)`) is called by whoever receives it; a class handed to code Atlas
        // reads (`pipeline.add(DiffValues, ..)`) is built there
        for a in handed_on(c) {
            if let Some(t) = self.seen.handed_function(a) {
                // a key the call orders or picks by is a value the call uses
                if handed_for_its_value(a, self.src) {
                    self.read.used.push(t.clone());
                }
                let each = matches!(callee, Some(Callee::Lib(_))).then(|| self.per_element(c, a, receiver, &steps.kept)).flatten();
                steps.handed(c, a, &t, EdgeKind::Calls, each);
            } else if let (Some(Callee::Def(_)), Some(t)) = (&callee, self.seen.handed_class(a)) {
                steps.handed(c, a, &t, EdgeKind::Instantiates, None);
            }
        }
        // a method on what a library's call gave is that library's, also where ty does not list it
        let on_library = receiver
            .map(|r| beneath(r, WRAPPERS))
            .filter(|r| r.kind() == "identifier")
            .is_some_and(|r| self.library_values.contains(text(r, self.src)));
        if matches!(callee, Some(Callee::Lib(_))) {
            self.library_values.extend(gives.iter().map(|g| g.name.clone()).filter(|n| !n.is_empty()));
        }
        match callee {
            Some(Callee::Def(t)) => self.own(steps, c, &t, receiver, hands, gives),
            None if on_library => {
                // a step where the body names what it gives; else what it gives goes into the code around it
                if !assigned.is_empty() {
                    let op = fnode.child_by_field_name("attribute").map(|a| text(a, self.src).to_string()).unwrap_or_default();
                    self.library_values.extend(gives.iter().map(|g| g.name.clone()).filter(|n| !n.is_empty()));
                    steps.op(c, &op, hands, gives);
                }
                steps.dropped(c, &written(fnode, self.src));
                self.read.unresolved.push((named_path(fnode, self.src).into_iter().take(2).collect::<Vec<_>>().join("."), Bucket::Library));
                self.library(steps, c, fnode, None);
            }
            Some(Callee::Lib(lib)) => {
                // a library's call not read that makes a value the body keeps under a name, from
                // values it was handed, is a step of its own; what a call gives without a name
                // goes into the call around it. A parse of the command line is a step whatever
                // it gives.
                // what the tables say a call does: parse the command line, change a value, or, for
                // Python's own, compute what it gives
                let known = if let Some((op, options)) = self.command_line(&lib, fnode) {
                    steps.parses(c, &op, hands, gives, options);
                    true
                } else if let Some(changed) = library_change(self.seen.facts, c, self.src) {
                    let changed = self.seen.changed_in(changed, self.ext, &steps.kept);
                    let op = text(fnode, self.src).rsplit('.').next().unwrap_or("");
                    steps.changes(c, op, hands, gives, changed);
                    true
                } else {
                    if !assigned.is_empty() {
                        let op = match fnode.kind() {
                            "attribute" => fnode.child_by_field_name("attribute").map(|a| text(a, self.src).to_string()),
                            _ => Some(text(fnode, self.src).rsplit('.').next().unwrap_or("").to_string()),
                        };
                        steps.op(c, &op.unwrap_or_default(), hands, gives);
                    } else if let Some(class) = steps.raised(c).then(|| self.raised_class(c)).flatten() {
                        // the error the body raises is a step of its class, whatever library the class is of
                        self.read.new_ext.push(class.clone());
                        let op = format!("{}()", class.rsplit('.').next().unwrap_or(""));
                        steps.op(c, &op, hands, vec![Held::new(class, "")]);
                    }
                    knowledge::PYTHON_OWN.contains(&lib.as_str())
                };
                let op = named_path(fnode, self.src).last().cloned().unwrap_or_default();
                let exits = knowledge::ends_the_run(&lib, &op);
                self.read.exits |= exits;
                let crossed = (self.read.touches.len(), self.read.waiting.len(), self.read.handed.len());
                self.library(steps, c, fnode, Some(lib));
                // what the tables say nothing of beside what it gives, Atlas does not see
                if !known && !exits && crossed == (self.read.touches.len(), self.read.waiting.len(), self.read.handed.len()) {
                    steps.dropped(c, &written(fnode, self.src));
                }
            }
            None => {
                // a call Atlas cannot follow runs all the same: a step named as the code writes
                // it, giving the value the body keeps by the name it assigns, and a call not
                // followed
                let mut gives = gives;
                if gives.is_empty() && !assigned.is_empty() {
                    gives.push(Held::new(String::new(), &assigned));
                }
                let written = written(fnode, self.src);
                steps.not_followed(c, &written, hands, gives);
                steps.dropped(c, &written);
                let name = match named_path(fnode, self.src) {
                    p if p.is_empty() => written,
                    p => p.iter().take(2).cloned().collect::<Vec<_>>().join("."),
                };
                self.read.unresolved.push((name, self.bucket(fnode)));
                self.library(steps, c, fnode, None);
            }
        }
    }

    /// A call of the program's own function or class; a method called on a subclass's value is
    /// that subclass's step. A method called through a class (`Cached.read(self)`) runs on the
    /// value it is handed first, and one called through `super()` on the body's own receiver.
    fn own(&mut self, steps: &mut Steps<'t, '_>, c: TsNode<'t>, t: &str, receiver: Option<TsNode<'t>>, hands: Vec<Held>, gives: Vec<Held>) {
        let seen = &self.seen;
        let after = receiver.and_then(|o| self.written_in(o));
        let method = seen.store.nodes.get(t).filter(|n| n.kind == NodeKind::Method);
        let named = after.is_none() && method.is_some() && receiver.and_then(|o| seen.value(o)).map_or(false, |v| v.is_class);
        let instance = method.map_or(false, |n| !n.on_class);
        // the value the method runs on: what `super(..)` names, the first argument of a method
        // called through a class, else what it is called on
        let value = match &after {
            Some((_, v)) => *v,
            None if named && instance => first_argument(c),
            None => receiver,
        };
        // a value built at the call (`Section(..).show()`) is of its class and none below it, also where that is the
        // method's own; one the code declares is of the class it declares or one below it
        let built = value
            .filter(|o| o.kind() == "call")
            .and_then(|o| o.child_by_field_name("function"))
            .and_then(|f| seen.value(f))
            .map_or(false, |v| v.is_class);
        let on = value.and_then(|o| {
            let b = seen.store.nodes.get(t)?.cls.clone()?;
            seen.value(o)
                .map(|v| seen.drawn(v, self.ext))
                .unwrap_or_default()
                .into_iter()
                .find(|c| seen.is_class(c) && if *c == b { built } else { seen.descends(c, &b) })
        });
        let kind = if seen.is_class(t) { EdgeKind::Instantiates } else { EdgeKind::Calls };
        let on_receiver = match &after {
            Some((_, None)) => true,
            _ => value.map_or(false, |o| o.kind() == "identifier" && text(o, self.src) == "self"),
        };
        // a method of the program's own that changes its receiver gives it, beside what it returns
        let mut gives = gives;
        let mut changed: Vec<Held> = vec![];
        if seen.store.mutating.contains(t) {
            changed.extend(value.and_then(root_of).map(|r| seen.changed_in(r, self.ext, &steps.kept)).unwrap_or_default());
        }
        // and each value it hands a parameter the callee changes, under the name it was handed by (`swap(board, other)`
        // gives both), of the type the change is to where the caller's value is of none Atlas draws (`add_tag(tags, ..)`
        // gives the list `tags`); a part of a value handed changes its owner (`clear(board.cells)` gives `board`), and a
        // value a call or an operator computes is no value of the caller's
        let args = arguments(self.modules, seen.store, &self.id, t, c, named && instance, self.at());
        if let Some(arg) = &args {
            let ps = self.changing.get(&arg.callee).map_or(&[][..], Vec::as_slice);
            for (p, a) in &arg.passed {
                let Some((_, ty)) = ps.iter().find(|(q, _)| q == p) else { continue };
                for mut h in root_of(*a).map(|r| seen.changed_in(r, self.ext, &steps.kept)).unwrap_or_default() {
                    if h.ty.is_empty() {
                        h.ty = ty.clone();
                    }
                    changed.push(h);
                }
            }
        }
        for r in changed.into_iter().filter(|h| !h.name.is_empty()) {
            if r.ty.starts_with("ext:") {
                self.read.new_ext.push(r.ty.clone());
            }
            if !gives.contains(&r) {
                gives.push(r);
            }
        }
        let after_class = after.map(|(k, _)| k).unwrap_or_default();
        steps.call(
            c,
            t,
            kind,
            None,
            Step {
                built: built && on.is_some(),
                on: on.unwrap_or_default(),
                on_receiver,
                named,
                receiver_first: named && instance,
                after: after_class,
                hands,
                gives,
                ..Default::default()
            },
        );
        if kind == EdgeKind::Calls && value_used(c) {
            self.read.used.push(t.to_string());
        }
        self.read.args.extend(args);
    }

    /// For what a method is called on, when it is `super()` or `super(K, x)`: the class the call
    /// looks after (the body's own class, or `K`), and the value it runs on when that is not the
    /// body's own receiver.
    fn written_in(&self, o: TsNode<'t>) -> Option<(String, Option<TsNode<'t>>)> {
        if o.kind() != "call" || o.child_by_field_name("function").map(|f| text(f, self.src)) != Some("super") {
            return None;
        }
        let args: Vec<TsNode> = o.child_by_field_name("arguments").map(named_children).unwrap_or_default();
        match args.as_slice() {
            [] => Some((self.cls.clone()?, None)),
            [k, x] => {
                let k = self.seen.value(*k).filter(|v| v.is_class).and_then(|v| self.seen.classes(v).into_iter().next())?;
                Some((k, Some(*x)))
            }
            _ => None,
        }
    }

    /// The method a call of library `lib` is, with the options of the command line it parses,
    /// where it parses the command line into an object (`parser.parse_args()`): each argument the
    /// parser it is made on declares in the module (`parser.add_argument("-o", "--out")` is
    /// `--out`, `parser.add_argument("title")` is `<title>`), in the order they are written.
    fn command_line(&self, lib: &str, fnode: TsNode) -> Option<(String, Vec<String>)> {
        let src = self.src;
        let parser = knowledge::ARG_PARSERS.iter().find(|p| p.package == lib)?;
        let op = text(fnode.child_by_field_name("attribute").filter(|_| fnode.kind() == "attribute")?, src).to_string();
        if !parser.parses.contains(&op.as_str()) {
            return None;
        }
        let on = fnode.child_by_field_name("object").filter(|o| o.kind() == "identifier").map(|o| text(o, src));
        let mut options = vec![];
        for call in walk(self.m.tree.root_node(), &["call"]) {
            let Some(f) = call.child_by_field_name("function").filter(|f| f.kind() == "attribute") else { continue };
            let declares = f.child_by_field_name("attribute").map(|a| text(a, src)) == Some(parser.declares);
            if !declares
                || on.is_none()
                || f.child_by_field_name("object").map(|o| text(o, src)) != on
                || self.seen.library(f).as_deref() != Some(lib)
            {
                continue;
            }
            let flags: Vec<String> = call
                .child_by_field_name("arguments")
                .map(named_children)
                .unwrap_or_default()
                .into_iter()
                .filter(|a| a.kind() == "string")
                .map(|a| unquote(text(a, src)))
                .collect();
            match flags.iter().any(|f| f.starts_with('-')) {
                true => options.push(crate::entries::flag(&flags, "")),
                false => options.extend(flags.first().map(|a| crate::entries::argument(a))),
            }
        }
        Some((op, options))
    }

    /// A library's call, or one Atlas cannot follow: a class it makes, and what it crosses into. A
    /// method on a value written out (`(root / name).write_text`) is its name alone.
    fn library(&mut self, steps: &mut Steps<'t, '_>, c: TsNode<'t>, fnode: TsNode<'t>, lib: Option<String>) {
        let parts = named_path(fnode, self.src);
        if parts.is_empty() {
            return;
        }
        if let Some(cls) = lib.as_ref().and(self.seen.made_on(c, fnode)) {
            steps.edge(c, &cls, EdgeKind::Instantiates);
        }
        let op = parts.last().map_or("", String::as_str);
        // traffic through a file an `open` made is at the file it was opened at, wherever the file
        // object is handed (`docs/reference/mathematics.md`, "A door keeps its place")
        let traffic = knowledge::PYTHON_FILE_TRAFFIC.iter().find(|(o, _)| *o == op).map(|(_, d)| *d);
        let file = called_on(fnode).filter(|o| self.seen.value(*o).is_some_and(|v| v.open_file()));
        if let (Some(dir), Some(on)) = (traffic, file) {
            let what = text(beneath(on, WRAPPERS), self.src).to_string();
            let payload = payload_of(c, self.src, &what, "arguments", &self.seen.handed(c));
            let t =
                Touch { what, payload, op: op.to_string(), line: c.start_position().row as u32 + 1, ..Touch::new(World::Disk, Some(dir)) };
            // a file the body opened is at the place its opening touches, named as the opening names
            // it; one handed in is at the term it is opened at, which the callers make whole
            let opened = self
                .opening(on)
                .map(|(o, built)| (Touch { what: o.what.clone(), artifact: o.artifact.clone(), fixed: o.fixed, ..t.clone() }, built));
            match opened.unwrap_or_else(|| (t, self.eval.of(self.at(), on, 0))) {
                (t, built) if built.is_empty() => self.read.touches.push(t),
                waiting => self.read.waiting.push(waiting),
            }
            return;
        }
        let Some(tag) = effects::boundary(lib.as_deref(), &parts).filter(|t| self.seen.moves(fnode, t, lib.as_deref(), op)) else { return };
        // a print handed a file writes that file (where it was opened), not the screen; one handed
        // a stream the human sees (`file=sys.stderr`) still shows it
        let to = (tag.world == World::Screen).then(|| keyword_node(c, self.src, knowledge::PRINT_TO)).flatten();
        if to.map_or(
            false,
            |f| !matches!(dotted(Some(f), self.src).as_slice(), [m, s] if knowledge::STREAMS.contains(&(m.as_str(), s.as_str()))),
        ) {
            return;
        }
        let crossed = crossed(&self.seen, self.eval, self.at(), c, fnode, tag, lib.as_deref(), op);
        self.read.touches.extend(crossed.touches);
        self.read.waiting.extend(crossed.waiting);
        self.read.declared.extend(crossed.said);
        self.read.handed.extend(crossed.handed);
    }

    /// The touch of the call that opened the file `on`, with the term its place waits for, where the
    /// body binds `on` to that call before this use (`with open(p) as f`, `f = open(p)`): the last
    /// such binding's touch on a disk.
    fn opening(&self, on: TsNode) -> Option<(&Touch, Vec<Built>)> {
        let on = beneath(on, WRAPPERS);
        let name = (on.kind() == "identifier").then(|| text(on, self.src))?;
        let named = |n: Option<TsNode>| n.is_some_and(|n| text(n, self.src) == name);
        let opened = walk(self.body, &["as_pattern", "assignment"])
            .into_iter()
            .filter(|n| n.end_byte() <= on.start_byte())
            .filter_map(|n| match n.kind() {
                "as_pattern" => named(n.child_by_field_name("alias")).then(|| n.named_child(0)).flatten(),
                _ => named(n.child_by_field_name("left")).then(|| n.child_by_field_name("right")).flatten(),
            })
            .last()?;
        let line = beneath(opened, WRAPPERS).start_position().row as u32 + 1;
        let made = self.read.touches.iter().map(|t| (t, vec![])).chain(self.read.waiting.iter().map(|(t, b)| (t, b.clone())));
        made.filter(|(t, _)| t.line == line && t.kind == World::Disk).last()
    }

    /// What the name a call not followed starts from is bound to, as ty finds where it is
    /// defined: in the body (a parameter, a local), in the builtins or a library, or elsewhere in
    /// the program.
    fn bucket(&self, f: TsNode) -> Bucket {
        // the name its path starts from, through what it is written on: `G.nodes[n].get`,
        // `G.nodes(..).items`
        let mut head = f;
        while let Some(o) = match head.kind() {
            "attribute" => head.child_by_field_name("object"),
            "subscript" => head.child_by_field_name("value"),
            "call" => head.child_by_field_name("function"),
            _ => None,
        } {
            head = o;
        }
        if head.kind() != "identifier" {
            return Bucket::Unknown;
        }
        let in_body = |at: &usize| self.scope.map_or(true, |d| d.byte_range().contains(at));
        match self.seen.facts.and_then(|x| x.def(Seen::range(head))) {
            Some(Target::Lib(m)) if m == knowledge::PYTHON_BUILTINS_MODULE => Bucket::Builtin,
            Some(Target::Lib(_)) => Bucket::Library,
            Some(Target::Here { file, at }) if *file == crate::tree::path_str(&self.m.path) && in_body(at) => Bucket::Local,
            None if self.own_params.iter().any(|p| p == text(head, self.src)) => Bucket::Local,
            _ => Bucket::Unknown,
        }
    }

    /// What the body does besides its calls one by one: what it runs inside the session or
    /// transaction it opened goes the way it said; the inputs it changes in place (`T -> T` for
    /// the receiver or a parameter; an initializer builds its object); and the I/O functions it
    /// refers to, directly or through a module-level table, which may be called later.
    fn ends(&mut self) {
        for t in self.read.touches.iter_mut().filter(|t| t.dir.is_none()) {
            let said: HashSet<Dir> = self.read.declared.iter().filter(|(k, _)| *k == t.kind).map(|(_, d)| *d).collect();
            if let [dir] = said.into_iter().collect::<Vec<_>>()[..] {
                t.dir = Some(dir);
            }
        }
        if self.in_module {
            return;
        }
        let (seen, changing, cls) = (&self.seen, self.changing, self.cls.as_deref());
        // a call of the package's that changes what it runs on or is handed, or a library call that changes a value
        let mut roots = changed_roots(self.body, self.src, &|c| changed_by(seen, changing, c, cls));
        // `xs += ys` on a parameter that holds a list, a dict or a set changes it in place
        for a in walk(self.body, &["augmented_assignment"]) {
            let Some(l) = a.child_by_field_name("left").filter(|l| l.kind() == "identifier") else { continue };
            if self.changeable.contains(text(l, self.src)) {
                roots.extend(changed_through(self.body, self.src, &BINDS, text(l, self.src), a.start_byte()));
            }
        }
        for r in roots {
            let ty = if r == "self" { self.cls.clone().filter(|_| !self.initializer) } else { self.params.get(r).cloned() };
            if let Some(t) = ty {
                if t.starts_with("ext:") {
                    self.read.new_ext.push(t.clone());
                }
                self.read.changes.push((t, r.to_string()));
            }
        }
        self.read.touches.extend(io_refs(self.m, self.seen.facts, self.body, self.own, ""));
        let tables = io_tables(self.m, self.seen.facts, self.own);
        for ident in walk(self.body, &["identifier"]) {
            if let Some(ts) = tables.get(text(ident, self.src)) {
                self.read.touches.extend(ts.iter().cloned());
            }
        }
    }
}

/// What a call that crosses into the world leaves: the touches it makes, the file and database
/// touches whose paths and statements wait for what the callers hand, and the direction it says
/// to a database's driver, with the work a managed transaction it opens is handed.
#[derive(Default)]
struct Crossed {
    touches: Vec<Touch>,
    waiting: Vec<(Touch, Vec<Built>)>,
    said: Option<(World, Dir)>,
    handed: Option<(String, World, Dir)>,
}

/// A call that crosses `tag`: a file call by the files it is handed, a database call by the
/// statement it is handed, any other by what its facts and literals name.
#[allow(clippy::too_many_arguments)]
fn crossed<'t>(
    seen: &Seen,
    eval: &Eval,
    at: At<'_, 't>,
    c: TsNode<'t>,
    fnode: TsNode<'t>,
    tag: Crossing,
    lib: Option<&str>,
    op: &str,
) -> Crossed {
    let src = &at.m.src;
    match tag.world {
        World::Disk => {
            let (touches, waiting): (Vec<(Touch, Vec<Built>)>, _) =
                file_calls(seen, eval, at, c, fnode, tag, lib, op).into_iter().partition(|(_, built)| built.is_empty());
            Crossed { touches: touches.into_iter().map(|(t, _)| t).collect(), waiting, ..Default::default() }
        }
        _ => {
            let path_function = |f: TsNode| eval.path_function_of(at.m, f);
            let mut cr = Crosses::read(c, src, tag, lib, op, &literals(c, src, &["string"]), &["string"], &seen.handed(c), &path_function);
            // on the web, what a call asks for is the argument its library names so
            let url = asked(seen, c, fnode).filter(|_| cr.facts.kind == World::Net);
            if let Some(a) = url {
                cr.what = literals(a, src, &["string"]).first().map(|l| l.trim().to_string()).unwrap_or_else(|| text(a, src).to_string());
                cr.payload = payload_of(c, src, &cr.what, "arguments", &seen.handed(c));
                cr.fixed = crate::tree::head_literal(a, src, &["string"], &path_function);
            }
            let said = match cr.facts.kind {
                World::Db(db) => effects::access(db, op, keyword(c, src, knowledge::DRIVER_ACCESS).as_deref()),
                _ => None,
            };
            // the work a managed transaction is handed runs in its direction
            let work = said.filter(|_| effects::transaction(op)).and_then(|_| positional(c)).and_then(|a| seen.callee(a));
            let handed = match (work, said) {
                (Some(Callee::Def(w)), Some(d)) => Some((w, cr.facts.kind, d)),
                _ => None,
            };
            // a statement is read where it is whole: from a constant, an f-string, or what the
            // callers hand the parameters it is built from
            let statement = match cr.facts.kind {
                World::Db(db) => positional(c).map(|a| eval.of(at, a, 0)).filter(|b| crate::paths::is_statement(db, b)),
                _ => None,
            };
            // a file database is opened on the file it is handed
            let opened = match cr.facts.kind {
                World::Db(db) if knowledge::opens_a_file(db, op) => positional(c).map(|a| eval.of(at, a, 0)),
                _ => None,
            };
            if let Some(built) = statement.or(opened) {
                let t = Touch {
                    what: cr.what.clone(),
                    payload: cr.payload.clone(),
                    op: cr.op.clone(),
                    line: cr.line,
                    ..Touch::new(cr.facts.kind, said)
                };
                return Crossed { waiting: vec![(t, built)], said: said.map(|d| (cr.facts.kind, d)), handed, ..Default::default() };
            }
            // a request is its method and its URL, whole where the callers make it, as a path is
            let request = url.filter(|_| lib.map_or(false, effects::network)).map(|a| {
                let (way, a) = sent(seen, c, a, op);
                crate::paths::request_of(&way, eval.of(at, a, 0))
            });
            // a socket's traffic handed no address is at the door's place, the address of the socket it
            // runs on, which the callers hand as they hand a path (`docs/reference/mathematics.md`, "A
            // door keeps its place")
            let request = request.or_else(|| {
                let socket = lib.is_some_and(knowledge::python_socket_package) && knowledge::python_socket_op(op).is_some();
                let on = called_on(fnode).filter(|_| socket)?;
                Some(crate::paths::request_of(op, eval.of(at, on, 0)))
            });
            if let (Some(built), Some(t)) = (request, cr.touches(said).into_iter().next()) {
                return Crossed { waiting: vec![(t, built)], said: said.map(|d| (cr.facts.kind, d)), handed, ..Default::default() };
            }
            Crossed { touches: cr.touches(said), said: said.map(|d| (cr.facts.kind, d)), handed, ..Default::default() }
        }
    }
}

/// The method a request `op` goes by, with the URL it asks for, where its library says the method by
/// what the call is handed (`knowledge::REQUESTS_BY_DATA`): a `Request(..)` built in place where the
/// call opens one, by that one's `method` or data and with its URL; else the call's own name.
fn sent<'t>(seen: &Seen, c: TsNode<'t>, url: TsNode<'t>, op: &str) -> (String, TsNode<'t>) {
    let src = &seen.m.src;
    let Some(by) = knowledge::request_by_data(op) else { return (op.to_string(), url) };
    let built = (url.kind() == "call").then(|| dotted(url.child_by_field_name("function"), src).last().cloned()).flatten();
    if let Some(inner) = built.as_deref().and_then(knowledge::request_by_data).filter(|r| r.method.is_some()) {
        if let Some(u) = keyword_node(url, src, knowledge::ASKED_BY[0]).or_else(|| positional_at(url, 0)) {
            return (way_by_data(seen, url, inner).unwrap_or_else(|| op.to_string()), u);
        }
    }
    (way_by_data(seen, c, by).unwrap_or_else(|| op.to_string()), url)
}

/// The method a call of `by` says by what it is handed: the method it is handed as a literal, else a
/// `GET` where it is handed no data (none, or `None` written), a `POST` where what it is handed is
/// never `None` (a literal, or a value of a type without `None` as ty says), and none where it may
/// be or the method is a value: the code does not say which way the request goes.
fn way_by_data(seen: &Seen, c: TsNode, by: &knowledge::RequestByData) -> Option<String> {
    let src = &seen.m.src;
    if let Some(m) = by.method.and_then(|k| keyword_node(c, src, k)) {
        return (m.kind() == "string").then(|| unquote(text(m, src)).to_lowercase());
    }
    let Some(data) = keyword_node(c, src, by.data.1).or_else(|| positional_at(c, by.data.0)) else {
        return Some(knowledge::REQUEST_WITHOUT_DATA.to_string());
    };
    if data.kind() == "none" {
        return Some(knowledge::REQUEST_WITHOUT_DATA.to_string());
    }
    let literal = matches!(data.kind(), "string" | "concatenated_string" | "integer" | "float" | "list" | "dictionary" | "tuple");
    let typed = seen.facts.and_then(|f| f.value(Seen::range(data))).map(|v| v.shown.split('|').all(|p| p.trim() != "None"));
    (literal || typed == Some(true)).then(|| knowledge::REQUEST_WITH_DATA.to_string())
}

/// The files a file call touches, each a touch with its direction, what the code calls it (the
/// literal it is built from, else the expression that holds it: `datafile` in
/// `datafile.open(..)`), its name when the code writes it out and whether the code alone decides
/// it, and with its path as built (placed once the parameters' values are known). The file is what the call's library types as one: the path a method of a path is
/// called on, an argument typed as a path, or a file opened to read from or write to; a library
/// whose types are not known hands its first argument. The direction is what the type says
/// (`SupportsRead`, `SupportsWrite`), what the call's `mode` says, or for a call handed two paths
/// the API's own names for them (`src`, `dst`); else the operation's.
fn file_calls<'t>(
    seen: &Seen,
    eval: &Eval,
    at: At<'_, 't>,
    c: TsNode<'t>,
    fnode: TsNode<'t>,
    tag: Crossing,
    lib: Option<&str>,
    op: &str,
) -> Vec<(Touch, Vec<Built>)> {
    let src = &at.m.src;
    let path_function = |f: TsNode| eval.path_function_of(at.m, f);
    let touch = |kind: Crossing, what: String, artifact: String, fixed: bool| {
        let payload = payload_of(c, src, &what, "arguments", &seen.handed(c));
        Touch {
            what,
            payload,
            op: op.to_string(),
            artifact,
            fixed,
            line: c.start_position().row as u32 + 1,
            ..Touch::new(kind.world, kind.dir)
        }
    };
    let sig = seen.facts.and_then(|x| x.signature(Seen::range(fnode)));
    let typed = sig.map(|s| paths::file_args(s, c, src)).unwrap_or_default();
    // the mode the call is handed, where the code writes it out; a mode that is a value says no way
    let handed = typed.iter().find(|(p, _)| p.name == "mode").map(|(_, a)| *a);
    let mode = handed.and_then(|a| match a.kind() {
        "string" => Some(unquote(text(a, src))),
        _ => match eval.of(at, a, 0).as_slice() {
            [b] if crate::paths::known(b) => Some(crate::paths::render(b)),
            _ => None,
        },
    });
    let said = |f: effects::Facts| match handed.is_some() && mode.is_none() {
        true => effects::Facts { dir: None, ..f },
        false => f,
    };
    let files: Vec<(Option<FileArg>, &str, TsNode)> =
        match (fnode.kind() == "attribute" && lib.map_or(false, |l| knowledge::PATH_LIBRARIES.contains(&l)), sig) {
            (true, _) => fnode.child_by_field_name("object").map(|o| vec![(Some(FileArg::Path), "self", o)]).unwrap_or_default(),
            (false, Some(_)) => typed.iter().filter(|(p, _)| p.file.is_some()).map(|(p, a)| (p.file, p.name.as_str(), *a)).collect(),
            (false, None) => positional(c).map(|a| vec![(None, "", a)]).unwrap_or_default(),
        };
    let paths = files.iter().filter(|(k, _, _)| *k == Some(FileArg::Path)).count();
    let sides = crate::body::sides(op, paths);
    if files.is_empty() {
        let f = said(effects::facts(tag, lib, op, &mode.iter().cloned().collect::<Vec<_>>()));
        let what = what_of(c, src, &["string"], "arguments");
        return f
            .places()
            .into_iter()
            .map(|(artifact, _)| (touch(Crossing { world: f.kind, dir: f.dir }, what.clone(), artifact, false), vec![]))
            .collect();
    }
    let mut nth_path = 0;
    files
        .into_iter()
        .map(|(file, name, a)| {
            let nth = nth_path;
            nth_path += usize::from(file == Some(FileArg::Path));
            // a method of a path names its file by the path and by its own arguments (a pattern)
            let mut lits = literals(a, src, &["string"]);
            if name == "self" {
                lits.extend(c.child_by_field_name("arguments").map(|x| literals(x, src, &["string"])).unwrap_or_default());
            }
            lits.extend(mode.clone());
            let f = said(effects::facts(tag, lib, op, &lits));
            let kind = file_direction(file, nth, sides, &f);
            let artifact = f.places().into_iter().next().map(|(a, _)| a).unwrap_or_default();
            let what = match literals(a, src, &["string"]).into_iter().next() {
                Some(l) => l.trim().to_string(),
                None => text(beneath(a, WRAPPERS), src).to_string(),
            };
            // a pattern below a directory is the path the call gives, not the directory alone
            let patterned = name == "self" && knowledge::PATH_PATTERNS.iter().any(|(p, _, _)| *p == op);
            (
                touch(kind, what, artifact, crate::tree::head_literal(a, src, &["string"], &path_function)),
                eval.of(at, if patterned { c } else { a }, 0),
            )
        })
        .collect()
}

/// The name the body keeps a call's value by: the target of the assignment (or `:=`) it is the
/// value of, through its wrappers and the arms of a choice (`a if c else b`, `x or y`).
/// A value assigned to `_` is not kept.
fn kept_as(c: TsNode, src: &[u8]) -> Option<String> {
    let mut n = c;
    loop {
        let p = n.parent()?;
        let value = |field: &str| p.child_by_field_name(field).map(|v| v.id()) == Some(n.id());
        match p.kind() {
            k if WRAPPERS.contains(&k) => n = p,
            _ if (CHOICES.chooses)(p, src) && !matches!((CHOICES.part)(p, n, src), Part::Decides) => n = p,
            "assignment" if value("right") => {
                return p
                    .child_by_field_name("left")
                    .filter(|l| l.kind() == "identifier" && text(*l, src) != "_")
                    .map(|l| text(l, src).to_string())
            }
            "named_expression" if value("value") => return p.child_by_field_name("name").map(|l| text(l, src).to_string()),
            // `with X as con` names what `X` gives, as `con = X` does
            "as_pattern" if p.named_child(0).is_some_and(|v| v.id() == n.id()) && p.parent().is_some_and(|w| w.kind() == "with_item") => {
                let alias = p.child_by_field_name("alias")?;
                let name = if alias.kind() == "identifier" { alias } else { named_children(alias).into_iter().next()? };
                return (name.kind() == "identifier" && text(name, src) != "_").then(|| text(name, src).to_string());
            }
            _ => return None,
        }
    }
}

/// The names a statement binds to the parts of what a call gives, where the call is the whole
/// value of an assignment to a pattern: `uuid, soort, tekst = line.split(" ", 2)`.
fn unpacked_as<'t>(c: TsNode<'t>, src: &[u8]) -> Vec<TsNode<'t>> {
    let value = outermost(c, WRAPPERS);
    let Some(p) = value.parent().filter(|p| p.kind() == "assignment") else { return vec![] };
    if p.child_by_field_name("right").map(|r| r.id()) != Some(value.id()) {
        return vec![];
    }
    match p.child_by_field_name("left") {
        Some(l) if matches!(l.kind(), "pattern_list" | "tuple_pattern" | "list_pattern") => {
            named_children(l).into_iter().filter(|n| n.kind() == "identifier" && text(*n, src) != "_").collect()
        }
        _ => vec![],
    }
}

/// Which way a file argument goes: what its parameter's type says (a file to read from or to write
/// to); for the `nth` of the paths a call that moves or copies is handed, the side it is on;
/// else what the operation does.
fn file_direction(file: Option<FileArg>, nth: usize, sides: Option<[Crossing; 2]>, f: &effects::Facts) -> Crossing {
    match (file, sides) {
        (Some(FileArg::Reads), _) => Crossing::read(World::Disk),
        (Some(FileArg::Writes), _) => Crossing::write(World::Disk),
        (Some(FileArg::Path), Some(sides)) if nth < 2 => sides[nth],
        _ => Crossing { world: f.kind, dir: f.dir },
    }
}

/// Which parameter of the function a call reaches each argument is handed to: positional
/// arguments in order (after `self`), keyword arguments by name. A class's call reaches the
/// `__init__` it runs: its own, else the fields it declares, else the first along its bases. A
/// method called through its class (`receiver_first`) is handed its receiver first, `self` itself.
pub(super) fn arguments<'t>(
    modules: &'t IndexMap<String, Module>,
    store: &GraphBuilder,
    caller: &str,
    target: &str,
    c: TsNode<'t>,
    receiver_first: bool,
    at: At<'_, 't>,
) -> Option<Arg<'t>> {
    let (callee, passed) = passed(modules, store, target, c, receiver_first, &at.m.src)?;
    Some(Arg { caller: caller.to_string(), callee, module: at.m.id.clone(), scope: at.scope, params: at.params.to_vec(), passed })
}

/// What a call of the package's `target` hands each parameter, by name, with the function that
/// takes them (`arguments`); `src` is the caller's.
pub(super) fn passed<'t>(
    modules: &IndexMap<String, Module>,
    store: &GraphBuilder,
    target: &str,
    c: TsNode<'t>,
    receiver_first: bool,
    src: &[u8],
) -> Option<(String, Vec<(String, TsNode<'t>)>)> {
    let callee = match store.nodes.get(target)?.kind {
        NodeKind::Class => {
            let own = format!("{target}.__init__");
            let fields = store.classes.get(target).map_or(false, |c| !c.fields.is_empty());
            if store.nodes.contains_key(&own) || fields {
                own
            } else {
                store.initializer_of(target).unwrap_or(own)
            }
        }
        _ => target.to_string(),
    };
    // a class without an `__init__` of its own is built from the fields it declares, in order
    let params: Vec<String> = match store.nodes.get(&callee) {
        Some(cn) => {
            let cm = modules.get(&cn.module)?;
            let def = def_node(cm, cn)?;
            paths::parameters(def, &cm.src, cn.kind == NodeKind::Method).into_iter().map(|(p, _)| p).collect()
        }
        None => store.classes.get(target)?.fields.iter().map(|(f, _)| f.clone()).collect(),
    };
    let mut passed = vec![];
    let mut i = 0;
    let mut receiver = receiver_first;
    for a in named_children(c.child_by_field_name("arguments")?).into_iter().filter(|a| a.kind() != "comment") {
        match a.kind() {
            "keyword_argument" => {
                if let (Some(k), Some(v)) = (a.child_by_field_name("name"), a.child_by_field_name("value")) {
                    passed.push((text(k, src).to_string(), v));
                }
            }
            "list_splat" | "dictionary_splat" => break,
            _ if receiver => receiver = false,
            _ => {
                if let Some(p) = params.get(i) {
                    passed.push((p.clone(), a));
                }
                i += 1;
            }
        }
    }
    Some((callee, passed))
}

/// For a class built from its fields (no `__init__` of its own, `f` is `Class.__init__`): the
/// default its body declares for field `p`, with its module.
pub(super) fn field_default<'t>(
    modules: &'t IndexMap<String, Module>,
    store: &GraphBuilder,
    f: &str,
    p: &str,
) -> Option<(&'t Module, TsNode<'t>)> {
    let class = f.strip_suffix(".__init__").filter(|_| !store.nodes.contains_key(f))?;
    let cn = store.nodes.get(class).filter(|n| n.kind == NodeKind::Class)?;
    let m = modules.get(&cn.module)?;
    let body = def_node(m, cn)?.child_by_field_name("body")?;
    named_children(body)
        .into_iter()
        .filter(|st| st.kind() == "expression_statement")
        .filter_map(|st| named_children(st).into_iter().next().filter(|a| a.kind() == "assignment"))
        .find(|a| a.child_by_field_name("type").is_some() && a.child_by_field_name("left").map(|l| text(l, &m.src)) == Some(p))?
        .child_by_field_name("right")
        .map(|d| (m, d))
}

/// The arguments of a call that name a value without computing one: a name or an attribute,
/// handed positionally or by keyword.
fn handed_on(call: TsNode) -> Vec<TsNode> {
    let Some(args) = call.child_by_field_name("arguments") else { return vec![] };
    named_children(args)
        .into_iter()
        .filter_map(|a| match a.kind() {
            "keyword_argument" => a.child_by_field_name("value"),
            _ => Some(a),
        })
        .filter(|a| matches!(a.kind(), "identifier" | "attribute"))
        .collect()
}

/// The first argument a call is handed by place.
fn first_argument(call: TsNode) -> Option<TsNode> {
    named_children(call.child_by_field_name("arguments")?).into_iter().find(|a| !matches!(a.kind(), "comment" | "keyword_argument"))
}

/// Whether a function handed to a call is handed by a keyword whose function's value the call
/// uses (`key=score`).
fn handed_for_its_value(a: TsNode, src: &[u8]) -> bool {
    a.parent()
        .filter(|k| k.kind() == "keyword_argument")
        .and_then(|k| k.child_by_field_name("name"))
        .map_or(false, |n| knowledge::PYTHON_USES_HANDED.contains(&text(n, src)))
}

/// Whether what a call gives back is used: anything but a call standing as a statement.
fn value_used(call: TsNode) -> bool {
    outermost(call, WRAPPERS).parent().map_or(false, |p| p.kind() != "expression_statement")
}

/// A call's first positional argument.
pub(super) fn positional(call: TsNode) -> Option<TsNode> {
    named_children(call.child_by_field_name("arguments")?)
        .into_iter()
        .find(|a| !matches!(a.kind(), "keyword_argument" | "comment" | "list_splat" | "dictionary_splat"))
}

/// The argument a call to the web names what it asks for by: the one handed to the parameter its
/// library calls so (knowledge::ASKED_BY), by the library's signature; where ty knows none, the
/// one handed by such a keyword, else the first.
fn asked<'t>(seen: &Seen, c: TsNode<'t>, fnode: TsNode<'t>) -> Option<TsNode<'t>> {
    let src = &seen.m.src;
    let bound: Vec<(&str, TsNode)> = match seen.facts.and_then(|x| x.signature(Seen::range(fnode))) {
        Some(sig) => paths::file_args(sig, c, src).into_iter().map(|(p, a)| (p.name.as_str(), a)).collect(),
        None => {
            let args = named_children(c.child_by_field_name("arguments")?);
            let keywords = args
                .iter()
                .filter(|a| a.kind() == "keyword_argument")
                .filter_map(|a| Some((text(a.child_by_field_name("name")?, src), a.child_by_field_name("value")?)));
            let first = positional(c).map(|a| (knowledge::ASKED_BY[0], a));
            keywords.chain(first).collect()
        }
    };
    knowledge::ASKED_BY.iter().find_map(|name| bound.iter().find(|(p, _)| p == name).map(|(_, a)| *a))
}

/// The text of the first of these keyword arguments a call is given.
pub(super) fn keyword(call: TsNode, src: &[u8], names: &[&str]) -> Option<String> {
    let args = named_children(call.child_by_field_name("arguments")?);
    let k = args
        .iter()
        .find(|a| a.kind() == "keyword_argument" && a.child_by_field_name("name").map_or(false, |n| names.contains(&text(n, src))))?;
    Some(text(k.child_by_field_name("value")?, src).to_string())
}

/// Python's choices: `if`/`elif`/`else`, `match`, `try`/`except`, `a if c else b`, and the right of
/// `x or y` and `x and y`, which runs only where the left is false or true: the left always runs, and
/// its value goes on where it holds, so it is no arm.
const CHOICES: ChoiceSyntax = ChoiceSyntax {
    chooses: |n, src| match n.kind() {
        "if_statement" => !is_main_guard(n, src),
        "elif_clause" | "conditional_expression" | "try_statement" | "case_clause" => true,
        "boolean_operator" => matches!(n.child_by_field_name("operator").map(|o| text(o, src)), Some("or" | "and")),
        // the cases of a `match` sit in its body block
        "block" => n.parent().map_or(false, |p| p.kind() == "match_statement"),
        _ => false,
    },
    continues: |n| if matches!(n.kind(), "elif_clause" | "case_clause") { n.parent() } else { None },
    part: |c, child, _| {
        let is = |f: &str| c.child_by_field_name(f).map(|x| x.id()) == Some(child.id());
        match c.kind() {
            "if_statement" if is("condition") => Part::Decides,
            "if_statement" if is("consequence") => Part::Arm(0),
            "if_statement" if child.kind() == "else_clause" => {
                Part::Arm(1 + named_children(c).iter().filter(|x| x.kind() == "elif_clause").count() as u32)
            }
            "elif_clause" if is("condition") => Part::Decides,
            "elif_clause" if is("consequence") => Part::Arm(1 + c.parent().map_or(0, |p| nth_of_kind(p, c))),
            "conditional_expression" => match named_children(c).iter().position(|x| x.id() == child.id()) {
                Some(0) => Part::Arm(0),
                Some(1) => Part::Decides,
                Some(2) => Part::Arm(1),
                _ => Part::Through,
            },
            "boolean_operator" if is("left") => Part::Through,
            "boolean_operator" if is("right") => Part::Arm(1),
            "block" if child.kind() == "case_clause" => Part::Arm(nth_of_kind(c, child)),
            // a pattern and its guard decide which case runs
            "case_clause" if child.kind() == "case_pattern" || is("guard") => Part::Decides,
            "try_statement" if is("body") || child.kind() == "else_clause" => Part::Arm(0),
            "try_statement" if child.kind() == "except_clause" => Part::Arm(1 + nth_of_kind(c, child)),
            _ => Part::Through,
        }
    },
    // an `if` without an `else` whose every arm returns, raises, continues or breaks
    guards: |n, src| {
        let exits = |b: Option<TsNode>| {
            b.and_then(|b| named_children(b).into_iter().filter(|s| s.kind() != "comment").last())
                .is_some_and(|s| matches!(s.kind(), "return_statement" | "raise_statement" | "continue_statement" | "break_statement"))
        };
        let parts = named_children(n);
        n.kind() == "if_statement"
            && !is_main_guard(n, src)
            && !parts.iter().any(|p| p.kind() == "else_clause")
            && exits(n.child_by_field_name("consequence"))
            && parts.iter().filter(|p| p.kind() == "elif_clause").all(|e| exits(e.child_by_field_name("consequence")))
    },
};

/// Where Python's values go: `x = v`, `x += v`, `for x in v`, `(x := v)`; `return` and `yield`
/// leave; `raise` and `assert` fail.
const USES: UseSyntax = UseSyntax {
    binds: |n| match n.kind() {
        "assignment" | "augmented_assignment" | "for_statement" => n.child_by_field_name("left").zip(n.child_by_field_name("right")),
        "named_expression" => n.child_by_field_name("name").zip(n.child_by_field_name("value")),
        // `with open(p) as f:` binds `f` to what `open(p)` gives
        "as_pattern" => n.child_by_field_name("alias").zip(n.named_child(0)),
        _ => None,
    },
    leaves: |n, _| matches!(n.kind(), "return_statement" | "yield"),
    fails: |n, _| matches!(n.kind(), "raise_statement" | "assert_statement"),
    hands: |n, child| n.kind() == "call" && n.child_by_field_name("arguments").map(|a| a.id()) == Some(child.id()),
    changes: |_| vec![],
    splits: |n, child| n.kind() == "match_statement" && n.child_by_field_name("subject").map(|v| v.id()) == Some(child.id()),
    names: &["identifier"],
    keeps: &["attribute", "subscript"],
    field: |n, src| {
        (n.kind() == "attribute")
            .then(|| Some((n.child_by_field_name("object")?, text(n.child_by_field_name("attribute")?, src).to_string())))
            .flatten()
    },
    pairs: |targets, value| {
        matches!(targets.kind(), "pattern_list" | "tuple_pattern" | "list_pattern")
            && matches!(value.kind(), "expression_list" | "tuple" | "list")
    },
    // `and` and `or` are choices
    computes: |n, _| {
        matches!(
            n.kind(),
            "binary_operator"
                | "unary_operator"
                | "not_operator"
                | "comparison_operator"
                | "tuple"
                | "list"
                | "set"
                | "expression_list"
                | "string"
                | "concatenated_string"
                | "interpolation"
        )
    },
    wrappers: WRAPPERS,
    drops: |n, child, _| {
        n.kind() == "expression_statement" && n.named_child_count() == 1 && n.named_child(0).map(|c| c.id()) == Some(child.id())
    },
    nested: &["function_definition", "class_definition", "decorated_definition"],
    reads: &[],
    label: |n, _| {
        n.parent().is_some_and(|p| {
            let is = |f: &str| p.child_by_field_name(f).is_some_and(|x| x.id() == n.id());
            (p.kind() == "attribute" && is("attribute")) || (p.kind() == "keyword_argument" && is("name"))
        })
    },
    binders: |n, child| {
        let is = |f: &str| n.child_by_field_name(f).is_some_and(|x| x.id() == child.id());
        match n.kind() {
            "lambda" | "lambda_within_for_in_clause" if is("parameters") => Some(true),
            "case_clause" if child.kind() == "case_pattern" => Some(false),
            "for_in_clause" if is("left") => Some(false),
            "except_clause" if is("alias") => Some(false),
            "global_statement" | "nonlocal_statement" | "import_statement" | "import_from_statement" => Some(false),
            _ => None,
        }
    },
    // `x += v` reads `x`; `with X as x:` enters and leaves what it binds
    updates: &["augmented_assignment", "as_pattern"],
    parts: |n, child| n.kind() == "for_in_clause" && field_children(n, "right").iter().any(|r| r.id() == child.id()),
    raises: |n, child, _| n.kind() == "raise_statement" && n.named_child(0).map(|c| c.id()) == Some(child.id()),
    errs: |_, _, _| false,
    tails: &[],
    propagates: None,
    catches: handlers,
    args,
};

/// The arguments a Python call writes, each with the parameter it fills (`body::placed`): the object a
/// method is called on (`x` of `x.f(..)`), each argument by its place up to a `*xs`, each `k=v` by its
/// name, and what a `**kw` spreads; a generator written as the one argument fills the first place.
fn args(n: TsNode, src: &[u8]) -> Vec<(Range, Slot)> {
    if n.kind() != "call" {
        return vec![];
    }
    let range = |n: TsNode| (n.start_byte(), n.end_byte());
    let f = n.child_by_field_name("function");
    let mut out: Vec<(Range, Slot)> = f
        .filter(|f| f.kind() == "attribute")
        .and_then(|f| f.child_by_field_name("object"))
        .map(|o| (range(o), Slot::Receiver))
        .into_iter()
        .collect();
    let Some(a) = n.child_by_field_name("arguments") else { return out };
    if a.kind() == "generator_expression" {
        out.push((range(a), Slot::At(0)));
        return out;
    }
    let written = named_children(a).into_iter().filter(|x| x.kind() != "comment");
    out.extend(crate::body::placed(
        written,
        |x| match x.kind() {
            "list_splat" => Some(Slot::Spread(0)),
            "dictionary_splat" => Some(Slot::Keywords),
            _ => None,
        },
        |x| {
            (x.kind() == "keyword_argument")
                .then(|| Some((text(x.child_by_field_name("name")?, src).to_string(), x.child_by_field_name("value")?)))
                .flatten()
        },
    ));
    out
}

/// What the handlers of a `try` catch, where `child` is its body: the classes each `except` names
/// (`except (A, B)`), none where one catches whatever is raised: a bare `except:`, and one that names
/// a class every error is of (`knowledge::PYTHON_CATCH_ALL`).
fn handlers(n: TsNode, child: TsNode, src: &[u8]) -> Option<Vec<String>> {
    if n.kind() != "try_statement" || n.child_by_field_name("body").map(|b| b.id()) != Some(child.id()) {
        return None;
    }
    let clauses: Vec<TsNode> = named_children(n).into_iter().filter(|c| c.kind() == "except_clause").collect();
    if clauses.is_empty() {
        return None;
    }
    let mut names = vec![];
    for c in clauses {
        let values = field_children(c, "value");
        if values.is_empty() {
            return Some(vec![]);
        }
        for v in
            values
                .into_iter()
                .flat_map(|v| if matches!(v.kind(), "tuple" | "parenthesized_expression") { named_children(v) } else { vec![v] })
        {
            let name = text(v, src);
            if name.rsplit('.').next().is_some_and(|n| knowledge::PYTHON_CATCH_ALL.contains(&n)) {
                return Some(vec![]);
            }
            names.push(name.to_string());
        }
    }
    Some(names)
}

/// Python's comparisons: `==`, `!=`, `is`, `is not`, `in` and `not in` against strings or the
/// members of a class (`Mode.FAST`), and a `match` over them.
const COMPARES: CompareSyntax = CompareSyntax {
    kinds: &["comparison_operator", "match_statement"],
    operands: tested,
    each: |n| match n.kind() {
        "set" | "list" | "tuple" => named_children(n).into_iter().filter(|i| i.kind() != "comment").collect(),
        _ => vec![n],
    },
    literal: |n, src| {
        (n.kind() == "string" && !named_children(n).iter().any(|c| c.kind() == "interpolation")).then(|| unquote(text(n, src)))
    },
    member: |n| match n.kind() {
        "attribute" => n
            .child_by_field_name("object")
            .filter(|o| matches!(o.kind(), "identifier" | "attribute"))
            .map(|o| (o.start_byte(), o.end_byte())),
        // a value pattern: `case Mode.FAST:`
        "dotted_name" => {
            let parts = named_children(n);
            (parts.len() > 1).then(|| (parts[0].start_byte(), parts[parts.len() - 2].end_byte()))
        }
        _ => None,
    },
    subject: |n| matches!(n.kind(), "identifier" | "attribute" | "subscript"),
    over: matched,
    copies: |n| match n.kind() {
        "assignment" => n.child_by_field_name("left").zip(n.child_by_field_name("right")),
        "named_expression" => n.child_by_field_name("name").zip(n.child_by_field_name("value")),
        _ => None,
    },
    place: |n| matches!(n.kind(), "identifier" | "attribute" | "subscript").then_some(n),
    binds: USES.binds,
};

/// The two operands of a test of equality, identity or membership.
fn tested<'t>(n: TsNode<'t>, src: &[u8]) -> Option<(TsNode<'t>, TsNode<'t>)> {
    let op = field_children(n, "operators").iter().map(|o| text(*o, src)).collect::<Vec<_>>().join(" ");
    match (op.as_str(), named_children(n).as_slice()) {
        ("==" | "!=" | "is" | "is not" | "in" | "not in", [l, r]) => Some((*l, *r)),
        _ => None,
    }
}

/// A `match`: its subject, the cases of its patterns (each string, and each value pattern alone
/// or among alternatives: `case Mode.FAST | Mode.SLOW:`), and the block that holds the cases.
fn matched<'t>(n: TsNode<'t>, src: &[u8]) -> Option<(String, Vec<TsNode<'t>>, Option<TsNode<'t>>)> {
    fn values(p: TsNode) -> Vec<TsNode> {
        named_children(p)
            .into_iter()
            .flat_map(|c| match c.kind() {
                "dotted_name" => vec![c],
                "union_pattern" => values(c),
                _ => vec![],
            })
            .collect()
    }
    let (Some(subject), Some(block)) = (n.child_by_field_name("subject"), n.child_by_field_name("body")) else { return None };
    let cases = walk(block, &["case_pattern"]).into_iter().flat_map(|p| walk(p, &["string"]).into_iter().chain(values(p))).collect();
    Some((text(subject, src).to_string(), cases, Some(block)))
}

/// Python's own types: the standard library's modules that describe code, not data.
pub(super) const OWN: OwnTypes = knowledge::PYTHON_OWN_TYPES;

/// The value a method is called on, where the callee `f` is a method of one: `conn` of `conn.recv`.
fn called_on(f: TsNode) -> Option<TsNode> {
    f.child_by_field_name("object").filter(|_| f.kind() == "attribute")
}
