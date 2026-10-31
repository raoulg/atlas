//! A function's body as TypeScript reads it: its steps in the order they run, what its calls touch,
//! the values its names hold and what made them, and the syntax of its choices and comparisons.

use std::collections::{HashMap, HashSet};

use indexmap::IndexMap;
use tree_sitter::Node as TsNode;

use super::module::{
    class_named, function_of, is_property, member_parts, package_from_ext, promised, resolve, type_head, type_names, Bound, Module,
    WRAPPERS,
};
use super::paths::{self, Arg, At, Eval};
use crate::body::{changed_through, BindSyntax, Bindings, Crosses, Range, Step, Steps, UseSyntax};
use crate::builder::{foreign_type, GraphBuilder};
use crate::effects::{self, Crossing};
use crate::knowledge;
use crate::knowledge::OwnTypes;
use crate::model::{ext, Bucket, EdgeKind, Held, Node, NodeKind, Resolved, Slot, Touch, World};
use crate::tree::{
    beneath, evaluation_order, literals, named_children, nth_of_kind, outermost, text, unquote, walk, what_of, wrapped, ChoiceSyntax,
    CompareSyntax, Part,
};

pub(super) const OWN: OwnTypes = knowledge::TYPESCRIPT_OWN_TYPES;

/// The package a client was made by: `new Anthropic()`, `new Pool()`, `axios.create()`,
/// `createClient()` (knowledge::TYPESCRIPT_CLIENT_MAKERS). What a package's other calls give back
/// is a result, not a client.
fn made_by_package(value: TsNode, m: &Module, src: &[u8]) -> Option<String> {
    let v = beneath(value, WRAPPERS);
    let maker = match v.kind() {
        "new_expression" => v.child_by_field_name("constructor")?,
        "call_expression" => v
            .child_by_field_name("function")
            .filter(|f| knowledge::TYPESCRIPT_CLIENT_MAKERS.makes(&member_parts(*f, src).pop().unwrap_or_default()))?,
        _ => return None,
    };
    match m.bindings.get(member_parts(maker, src).first()?)? {
        Bound::Ext(e) => Some(package_from_ext(e, &m.ext_pkgs)),
        Bound::Own(_) => None,
    }
}

/// The package a type annotation names: `Database.Database`, `Pool`, `: Client`.
/// Whether a type as written is one of the language's own containers that change in place
/// (`Row[]`, `Array<Row>`, `Map<string, Row>`) where the module names nothing of its own so; a
/// read-only one is not.
pub(super) fn container_type(t: &str, m: &Module) -> bool {
    let t = t.trim().trim_start_matches(':').trim();
    if t.starts_with("readonly ") || t.starts_with("Readonly") {
        return false;
    }
    let head = t.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$')).next().unwrap_or("");
    t.ends_with("[]")
        || (knowledge::TYPESCRIPT_CHANGEABLE.contains(&head) && !m.bindings.contains_key(head) && !m.symbols.contains_key(head))
}

/// Whether a value as written builds one: `[]`, `new Map()`, `new Set<string>()`.
pub(super) fn builds_container(v: TsNode, m: &Module, src: &[u8]) -> bool {
    match v.kind() {
        "array" => true,
        "new_expression" => v.child_by_field_name("constructor").map_or(false, |c| container_type(text(c, src), m)),
        _ => false,
    }
}

pub(super) fn typed_package(ty: TsNode, m: &Module, src: &[u8]) -> Option<String> {
    let head = text(ty, src).trim_start_matches(':').trim().split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$')).next()?;
    match m.bindings.get(head)? {
        Bound::Ext(e) => Some(package_from_ext(e, &m.ext_pkgs)),
        Bound::Own(_) => None,
    }
}

/// What each of the program's functions changes among what it is handed, by the function: `this`
/// for the value a method runs on and the name of each parameter it changes, each with the type its
/// change is drawn as (`T -> T`).
pub(super) type Changing = HashMap<String, Vec<(String, String)>>;

/// What the bodies hand the places pass: file and network calls with what their path or URL
/// is built from, and the calls of the package's functions with the arguments they pass.
#[derive(Default)]
pub(super) struct Linked<'t> {
    pub(super) calls: Vec<(String, Touch, Vec<crate::paths::Built>)>,
    pub(super) args: Vec<Arg<'t>>,
}

/// The fields of a class that hold what a package made: declared with a library's type, set from
/// what a package made, or a constructor's parameter property (`this.db`).
fn class_made(class: TsNode, m: &Module) -> HashMap<String, String> {
    let src = &m.src[..];
    let field = |name: TsNode| format!("this.{}", text(name, src));
    let mut out = HashMap::new();
    for f in walk(class, &["public_field_definition"]) {
        if let (Some(name), Some(pkg)) =
            (f.child_by_field_name("name"), f.child_by_field_name("type").and_then(|t| typed_package(t, m, src)))
        {
            out.insert(field(name), pkg);
        }
    }
    for p in walk(class, &["required_parameter", "optional_parameter"]).into_iter().filter(|p| is_property(*p)) {
        if let (Some(name), Some(pkg)) =
            (p.child_by_field_name("pattern"), p.child_by_field_name("type").and_then(|t| typed_package(t, m, src)))
        {
            out.insert(field(name), pkg);
        }
    }
    for a in walk(class, &["assignment_expression"]) {
        let (Some(l), Some(r)) = (a.child_by_field_name("left"), a.child_by_field_name("right")) else { continue };
        if let ([this, name], Some(pkg)) = (member_parts(l, src).as_slice(), made_by_package(r, m, src)) {
            if this == "this" {
                out.entry(format!("this.{name}")).or_insert(pkg);
            }
        }
    }
    out
}

/// The class a module-level value holds, in its own module or the one it is imported from.
/// The name a call's value is kept by: `const x = await f(..)`, `x = f(..)`, through the arms of
/// a choice (`c ? a : b`, `x ?? y`).
fn kept_as(c: TsNode, src: &[u8]) -> Option<String> {
    let mut n = c;
    loop {
        let p = n.parent()?;
        match p.kind() {
            k if WRAPPERS.contains(&k) => n = p,
            // through an arm of a choice, and the left of `x ?? y`, whose value goes on where it holds
            _ if (CHOICES.chooses)(p, src) && (CHOICES.part)(p, n, src) != Part::Decides && p.kind() != "if_statement" => n = p,
            "variable_declarator" if p.child_by_field_name("value").map(|v| v.id()) == Some(n.id()) => {
                return p.child_by_field_name("name").filter(|x| x.kind() == "identifier").map(|x| text(x, src).to_string());
            }
            "assignment_expression" if p.child_by_field_name("right").map(|r| r.id()) == Some(n.id()) => {
                return p.child_by_field_name("left").filter(|l| l.kind() == "identifier").map(|l| text(l, src).to_string());
            }
            _ => return None,
        }
    }
}

/// The pattern a call's value is taken apart by: `const { values } = parseArgs(..)`, `const [a, b] =
/// f(..)`.
fn pattern_of(c: TsNode) -> Option<TsNode> {
    let v = outermost(c, WRAPPERS);
    v.parent()
        .filter(|d| d.kind() == "variable_declarator" && d.child_by_field_name("value").map(|x| x.id()) == Some(v.id()))
        .and_then(|d| d.child_by_field_name("name"))
        .filter(|p| matches!(p.kind(), "object_pattern" | "array_pattern"))
}

fn value_type(m: &Module, name: &str, modules: &IndexMap<String, Module>, nodes: &IndexMap<String, Node>) -> Option<String> {
    let (home, local) = match m.values.contains_key(name) {
        true => (m, name.to_string()),
        false => match m.bindings.get(name)? {
            Bound::Own(target) => {
                let (mid, n) = target.rsplit_once('.')?;
                (modules.get(mid)?, n.to_string())
            }
            Bound::Ext(_) => return None,
        },
    };
    resolve(home, home.values.get(&local)?, modules, nodes)
        .map(Resolved::id)
        .filter(|r| nodes.get(r).map_or(false, |x| x.kind == NodeKind::Class))
}

/// The classes a body's names hold, binding by binding (`body::Bindings`). A binding (a parameter,
/// a `const`, `let` or `var`, a loop's variable, a caught error, a parameter of a function written
/// inside) is visible from where it is declared to the end of its block, its loop, its clause or its
/// function, and a name read at a place holds what the innermost binding of it visible there holds:
/// a loop's `b` and a later `const b = new Body(..)` are two values, each with its own type, as two
/// names would be.
#[derive(Default)]
pub(super) struct Locals<'t>(Bindings<Local<'t>>);

#[derive(Default)]
struct Local<'t> {
    /// The program's class it holds.
    ty: Option<String>,
    /// The type it is written with (`rows: Row[]`).
    written: Option<String>,
    /// Whether it holds one of the language's own containers that change in place.
    container: bool,
    /// Where it is bound from: what the name is set to, `whole` or a part of it a pattern takes
    /// (`const [a, b] = v`), or what a loop goes over, of which it is each element.
    from: Option<(TsNode<'t>, BoundFrom)>,
}

/// How a binding is bound from the value it names.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum BoundFrom {
    /// The name is set to the value: `const x = v`.
    Whole,
    /// A pattern takes a part of the value: `const { a } = v`, `const [a, b] = v`.
    Part,
    /// The name is each element of the value: `for (const x of v)`, and a parameter of a function
    /// handed to a method of `v` (`v.map((x) => ..)`).
    Element,
}

impl<'t> Locals<'t> {
    /// A binding of the name `n`, visible from where it stands to the end of `scope`, bound `from`
    /// a value.
    fn bind(&mut self, n: TsNode, src: &[u8], scope: TsNode, from: Option<(TsNode<'t>, BoundFrom)>) {
        self.0.bind(text(n, src), n.start_byte(), scope.end_byte(), Local { from, ..Local::default() });
    }

    /// The binding the name `n` declares, or the one visible where it stands.
    fn declared(&mut self, n: TsNode, src: &[u8]) -> Option<&mut Local<'t>> {
        self.0.get_mut(text(n, src), n.start_byte())
    }

    /// What the binding of the name `n` visible where it stands is bound from.
    pub(super) fn from(&self, n: TsNode, src: &[u8]) -> Option<(TsNode<'t>, BoundFrom)> {
        self.0.get(text(n, src), n.start_byte())?.from
    }

    /// The class of the binding the name `n` declares, where none is known yet.
    fn type_at(&mut self, n: TsNode, src: &[u8], ty: String) {
        if let Some(b) = self.declared(n, src) {
            b.ty.get_or_insert(ty);
        }
    }

    /// The type the binding the name `n` declares is written with.
    fn written_at(&mut self, n: TsNode, src: &[u8], ty: &str) {
        if let Some(b) = self.declared(n, src) {
            b.written = Some(ty.to_string());
        }
    }

    /// The binding the name `n` declares holds one of the language's own containers.
    fn container_at(&mut self, n: TsNode, src: &[u8]) {
        if let Some(b) = self.declared(n, src) {
            b.container = true;
        }
    }

    /// The type the name `n` is written with where it stands.
    fn written(&self, n: TsNode, src: &[u8]) -> Option<&String> {
        self.0.get(text(n, src), n.start_byte())?.written.as_ref()
    }

    /// Whether the name `n` holds one of the language's own containers where it stands.
    pub(super) fn container(&self, n: TsNode, src: &[u8]) -> bool {
        self.0.get(text(n, src), n.start_byte()).is_some_and(|b| b.container)
    }

    /// Which binding of the name `n` is visible where it stands: where it is visible from.
    pub(super) fn which(&self, n: TsNode, src: &[u8]) -> Option<usize> {
        self.0.which(text(n, src), n.start_byte())
    }

    /// Whether `n` is a name the body binds where it stands.
    pub(super) fn bound(&self, n: TsNode, src: &[u8]) -> bool {
        self.0.get(text(n, src), n.start_byte()).is_some()
    }

    /// The class the binding of `name` visible at byte `at` holds.
    pub(super) fn get(&self, name: &str, at: usize) -> Option<&String> {
        self.0.get(name, at)?.ty.as_ref()
    }

    /// The class the name `n` holds where it stands.
    pub(super) fn of(&self, n: TsNode, src: &[u8]) -> Option<&String> {
        self.get(text(n, src), n.start_byte())
    }

    /// Each binding that is each element of a value, with the value: its name and where it is declared.
    fn elements(&self) -> Vec<(String, usize, TsNode<'t>)> {
        self.0
            .all()
            .filter_map(|(n, at, b)| match b.from {
                Some((v, BoundFrom::Element)) => Some((n.to_string(), at, v)),
                _ => None,
            })
            .collect()
    }

    /// The class of the binding of `name` declared at `at`, where none is known yet.
    fn type_binding(&mut self, name: &str, at: usize, ty: String) {
        if let Some(b) = self.0.get_mut(name, at) {
            b.ty.get_or_insert(ty);
        }
    }

    /// Each binding that holds a class, by its name.
    fn typed(&self) -> impl Iterator<Item = (&str, &str)> {
        self.0.iter().filter_map(|(n, b)| Some((n, b.ty.as_deref()?)))
    }
}

/// Where a binding is visible until: the end of the block a `const` or a `let` is declared in (of the
/// loop, for a `for`'s header), of the function of a `var` or a parameter, of the loop of a loop's
/// variable, of the clause of a caught error.
fn scope_of<'t>(binder: TsNode<'t>, top: TsNode<'t>) -> TsNode<'t> {
    let function = |n: TsNode<'t>| {
        std::iter::successors(Some(n), |x| x.parent())
            .find(|x| x.id() == top.id() || paths::function_node(*x).is_some_and(|f| f.id() == x.id()))
            .unwrap_or(top)
    };
    match binder.kind() {
        "variable_declarator" => match binder.parent() {
            Some(d) if d.kind() == "lexical_declaration" => d.parent().unwrap_or(top),
            Some(d) => function(d.parent().unwrap_or(d)),
            None => top,
        },
        "for_in_statement" | "catch_clause" => binder,
        _ => function(binder),
    }
}

/// The names a pattern binds: `x`, `{ a, b: c }`, `[y = 0, , ...rest]`. A default's value binds
/// nothing.
pub(super) fn pattern_names<'t>(p: TsNode<'t>, out: &mut Vec<TsNode<'t>>) {
    match p.kind() {
        "identifier" | "shorthand_property_identifier_pattern" => out.push(p),
        "assignment_pattern" | "object_assignment_pattern" => p.child_by_field_name("left").into_iter().for_each(|l| pattern_names(l, out)),
        "pair_pattern" => p.child_by_field_name("value").into_iter().for_each(|v| pattern_names(v, out)),
        "object_pattern" | "array_pattern" | "rest_pattern" => named_children(p).into_iter().for_each(|c| pattern_names(c, out)),
        _ => {}
    }
}

/// What binds names, and the field that holds the names it binds.
const BINDERS: &[(&str, &str)] = &[
    ("variable_declarator", "name"),
    ("required_parameter", "pattern"),
    ("optional_parameter", "pattern"),
    ("arrow_function", "parameter"),
    ("catch_clause", "parameter"),
    ("for_in_statement", "left"),
];

/// Inside a module's body, what belongs to the module and not to a function defined in it.
fn module_level(n: TsNode, root: TsNode, bodies: &HashMap<String, (usize, usize)>) -> bool {
    let inside = bodies.values().any(|(s, e)| *s <= n.start_byte() && n.end_byte() <= *e);
    !inside && n.start_byte() >= root.start_byte()
}

#[allow(clippy::too_many_arguments)]
pub(super) fn link_calls<'t>(
    m: &'t Module,
    modules: &'t IndexMap<String, Module>,
    store: &mut GraphBuilder,
    id: &str,
    returned: &HashMap<String, String>,
    eval: &Eval,
    changing: &Changing,
    linked: &mut Linked<'t>,
) {
    let n = store.nodes[id].clone();
    let root = m.tree.root_node();
    let body = match n.kind {
        NodeKind::Module => root,
        _ => match m.bodies.get(id).and_then(|(s, e)| root.descendant_for_byte_range(*s, *e)) {
            Some(b) => b,
            None => return,
        },
    };
    let mut steps = Steps::new(body, &m.src, &CHOICES, &USES);
    let read = {
        let mut b = Body::new(m, modules, store, &n, body, returned, eval, changing);
        // a loop binds its element once what it goes over is made, and before its body runs
        let mut ran: Vec<((usize, u8), TsNode)> = b.sites.iter().map(|c| ((c.end_byte(), 0), *c)).collect();
        ran.extend(
            walk(body, &["for_in_statement"])
                .into_iter()
                .filter(|l| b.mine(*l) && l.child_by_field_name("operator").map_or(false, |o| text(o, &m.src) == "of"))
                .filter_map(|l| l.child_by_field_name("right").map(|r| ((r.end_byte(), 1), l))),
        );
        ran.sort_by_key(|(k, _)| *k);
        for (_, c) in ran {
            if c.kind() == "for_in_statement" {
                b.each(&mut steps, c);
                continue;
            }
            let (t, w) = (b.read.touches.len(), b.read.calls.len());
            b.call(&mut steps, c);
            b.read.made.record(c, t..b.read.touches.len(), w..b.read.calls.len());
            let received = b.received(c);
            let made = b.read.touches[t..].iter_mut().chain(b.read.calls[w..].iter_mut().map(|(t, _)| t));
            match received {
                Some(names) => steps.mark_received(&names, made),
                None => steps.mark(c, made),
            }
        }
        let kept = steps.kept.clone();
        let held = |n| {
            let mut out = vec![];
            if b.mine(n) {
                b.held(n, &kept, 0, &mut out);
            }
            out
        };
        let stands = |n| if b.mine(n) { b.stands(n, &kept) } else { vec![] };
        steps.uses(&held, &stands);
        if let Some(cls) = b.cls.clone().filter(|_| b.initializer) {
            steps.keeps(Held::new(cls, ""), b.properties());
        }
        steps.fills(&held);
        b.class_state(&mut steps);
        b.ends();
        b.read
    };
    let compares = (n.kind != NodeKind::Module).then(|| {
        // a qualifier is a type when it names an `enum` or a class of the program
        let is_type = |(a, b): (usize, usize)| {
            let parts: Vec<String> = String::from_utf8_lossy(&m.src[a..b]).split('.').map(str::to_string).collect();
            resolve(m, &parts, modules, &store.nodes).and_then(|t| store.nodes.get(&t.id())).is_some_and(|t| t.kind == NodeKind::Class)
        };
        // the value a method runs on is `this`, unless it is called on its class
        let receiver =
            n.params.iter().find(|p| p.receiver).map(|p| p.name.as_str()).or((n.kind == NodeKind::Method && !n.on_class).then_some("this"));
        crate::tree::compares(body, &m.src, steps.choices(), &COMPARES, &CHOICES, &crate::tree::Scope { receiver, is_type: &is_type })
    });
    steps.finish(store, id);
    if let Some(node) = store.nodes.get_mut(id) {
        node.calls_handed = read.unresolved.iter().any(|(u, _)| n.params.iter().any(|p| &p.name == u));
    }
    for (name, bucket) in read.unresolved {
        store.not_followed(name, bucket);
    }
    for t in read.used {
        store.used.insert((id.to_string(), t));
    }
    for t in read.touches {
        store.touch(id, t);
    }
    for (t, input) in read.mutates {
        if ext::is(&t) {
            store.ext_node(&t);
        }
        store.add_edge(id, &t, EdgeKind::Mutates, Some(input));
    }
    if let (Some(compares), Some(node)) = (compares, store.nodes.get_mut(id)) {
        node.compares = compares;
        node.raises = read.exits
            || crate::tree::holds(
                body,
                &m.src,
                |n, _| n.kind() == "throw_statement",
                &["function_declaration", "class_declaration", "method_definition"],
            );
    }
    linked.args.extend(read.args);
    linked.calls.extend(read.calls.into_iter().map(|(t, built)| (id.to_string(), t, built)));
}

/// What reading a body finds besides its steps: what it touches, the calls Atlas could not
/// follow, the functions whose value it uses, the inputs it changes in place, the calls of the
/// package's functions with the arguments they pass, the file and network calls whose path
/// or URL waits for them, and whether it calls what ends the program.
#[derive(Default)]
pub(super) struct Read<'t> {
    touches: Vec<Touch>,
    unresolved: Vec<(String, Bucket)>,
    used: Vec<String>,
    mutates: Vec<(String, String)>,
    args: Vec<Arg<'t>>,
    calls: Vec<(Touch, Vec<crate::paths::Built>)>,
    made: crate::body::MadeBy,
    exits: bool,
}

/// A function's body, or a module's own code, as TypeScript reads it: the program around it,
/// and the classes and packages its names hold.
pub(super) struct Body<'a, 't> {
    pub(super) m: &'t Module,
    pub(super) modules: &'t IndexMap<String, Module>,
    pub(super) store: &'a GraphBuilder,
    pub(super) src: &'t [u8],
    pub(super) root: TsNode<'t>,
    pub(super) body: TsNode<'t>,
    pub(super) id: String,
    pub(super) is_module: bool,
    /// The function the body belongs to, and its parameters by name.
    pub(super) scope: Option<TsNode<'t>>,
    pub(super) params: Vec<String>,
    pub(super) cls: Option<String>,
    pub(super) initializer: bool,
    /// The classes the fields of the body's own class hold.
    pub(super) attrs: std::collections::BTreeMap<String, String>,
    /// The class each of the program's functions hands on.
    pub(super) returned: &'a HashMap<String, String>,
    pub(super) eval: &'a Eval<'a>,
    /// What each of the program's functions changes among what it is handed.
    pub(super) changing: &'a Changing,
    /// The classes the body's names hold: its typed parameters, and locals declared with a type,
    /// built with `new`, or handed on by a function.
    pub(super) locals: Locals<'t>,
    /// Assignments to each name in this body, in source order.
    assigned: HashMap<String, Vec<(TsNode<'t>, TsNode<'t>)>>,
    /// What a change to each typed parameter changes (`inputs_of`).
    pub(super) inputs: HashMap<String, String>,
    /// The values a package made, by the package: a call on one is the package's.
    pub(super) made: HashMap<String, String>,
    /// The calls, constructors, elements and names of the body, in the order they run.
    pub(super) sites: Vec<TsNode<'t>>,
    /// The calls that keep the command line the program is run with under a name (`command_line`).
    pub(super) keeps_command_line: HashSet<usize>,
    pub(super) read: Read<'t>,
}

const STRINGS: &[&str] = &["string", "template_string"];

impl<'a, 't> Body<'a, 't> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        m: &'t Module,
        modules: &'t IndexMap<String, Module>,
        store: &'a GraphBuilder,
        n: &Node,
        body: TsNode<'t>,
        returned: &'a HashMap<String, String>,
        eval: &'a Eval<'a>,
        changing: &'a Changing,
    ) -> Self {
        let is_module = n.kind == NodeKind::Module;
        let scope = if is_module { None } else { paths::function_node(body) };
        let cls = n.cls.clone();
        let attrs = cls.as_ref().and_then(|c| store.classes.get(c)).map(|c| c.attr_types.clone()).unwrap_or_default();
        let mut b = Body {
            m,
            modules,
            store,
            src: &m.src,
            root: m.tree.root_node(),
            body,
            id: n.id.clone(),
            is_module,
            scope,
            params: scope.map(|f| paths::param_names(f, &m.src)).unwrap_or_default(),
            cls,
            initializer: n.ctor && !n.on_class,
            attrs,
            returned,
            eval,
            changing,
            locals: Locals::default(),
            assigned: HashMap::new(),
            inputs: HashMap::new(),
            made: HashMap::new(),
            sites: vec![],
            keeps_command_line: HashSet::new(),
            read: Read::default(),
        };
        b.typed(n);
        for a in walk(body, &["assignment_expression"]) {
            let (Some(left), Some(right)) = (a.child_by_field_name("left"), a.child_by_field_name("right")) else { continue };
            if left.kind() == "identifier" {
                b.assigned.entry(text(left, b.src).to_string()).or_default().push((left, right));
            }
        }
        b.made();
        b.keeps_command_line = b.command_line();
        b.sites = evaluation_order(walk(
            body,
            &["call_expression", "new_expression", "object", "jsx_opening_element", "jsx_self_closing_element", "identifier"],
        ))
        .into_iter()
        .filter(|x| b.mine(*x))
        .collect();
        b
    }

    /// The class the name `n` holds where it stands: what its binding in the body holds, else, where
    /// the body binds no such name, the value of the program's class its module keeps by it.
    fn type_of(&self, n: TsNode) -> Option<String> {
        if let Some(t) = self.locals.of(n, self.src) {
            return Some(t.clone());
        }
        (self.is_module || !self.locals.bound(n, self.src))
            .then(|| value_type(self.m, text(n, self.src), self.modules, &self.store.nodes))
            .flatten()
    }

    /// Where the body keeps the command line the program is run with (`process.argv`,
    /// `knowledge::TYPESCRIPT_COMMAND_LINE`): the end of the chain of methods called on it, where a
    /// name or a pattern keeps what that gives (`const [kind = "", ...flags] = process.argv.slice(2)`).
    /// The step there takes the entry's words and declares no options: the code names none. Words
    /// handed on without a name (`main(process.argv.slice(2))`) go into the call around them, which
    /// parses them where it parses them.
    fn command_line(&self) -> HashSet<usize> {
        let src = self.src;
        let mut out = HashSet::new();
        for m in walk(self.body, &["member_expression"]).into_iter().filter(|m| self.mine(*m)) {
            let parts = member_parts(m, src);
            if !knowledge::TYPESCRIPT_COMMAND_LINE.contains(&parts.iter().map(String::as_str).collect::<Vec<_>>().as_slice())
                || self.m.bindings.contains_key(&parts[0])
                || self.m.symbols.contains_key(&parts[0])
                || self.locals.bound(m, src)
            {
                continue;
            }
            let (mut value, mut end) = (outermost(m, WRAPPERS), None);
            while let Some(call) = value
                .parent()
                .filter(|f| f.kind() == "member_expression" && f.child_by_field_name("object").map(|o| o.id()) == Some(value.id()))
                .and_then(|f| f.parent().filter(|c| c.kind() == "call_expression" && function_of(*c).map(|x| x.id()) == Some(f.id())))
            {
                end = Some(call);
                value = outermost(call, WRAPPERS);
            }
            if let Some(end) = end.filter(|e| kept_as(*e, src).is_some() || pattern_of(*e).is_some()) {
                out.insert(end.id());
            }
        }
        out
    }

    /// Whether a node is the body's own: inside a module's own code, not in a function it defines.
    fn mine(&self, x: TsNode) -> bool {
        !self.is_module || module_level(x, self.root, &self.m.bodies)
    }

    /// What a name path names, by the id the graph knows it by.
    fn resolve(&self, parts: &[String]) -> Option<String> {
        resolve(self.m, parts, self.modules, &self.store.nodes).map(Resolved::id)
    }

    fn class(&self, r: Option<String>) -> Option<String> {
        r.filter(|r| self.store.nodes.get(r).map_or(false, |x| x.kind == NodeKind::Class))
    }

    /// The values a class keeps that the body changes by assignment: `Counter.total += 1`,
    /// `Registry.handlers[name] = fn`. A static of a class is no place Atlas draws, so what the
    /// change leaves behind it does not see.
    fn class_state(&self, steps: &mut Steps<'t, '_>) {
        if self.is_module {
            return;
        }
        let src = self.src;
        for a in walk(self.body, &["assignment_expression", "augmented_assignment_expression", "update_expression"]) {
            let Some(mut root) = a.child_by_field_name(if a.kind() == "update_expression" { "argument" } else { "left" }) else { continue };
            let mut through = false;
            while let Some(inner) = match root.kind() {
                "member_expression" | "subscript_expression" => root.child_by_field_name("object"),
                _ => None,
            } {
                (root, through) = (inner, true);
            }
            if through && root.kind() == "identifier" && self.class(self.resolve(&[text(root, src).to_string()])).is_some() {
                steps.unseen(a, text(a, src));
            }
        }
    }

    /// Whether `r` is one of the program's own functions or classes.
    pub(super) fn own(&self, r: &str) -> bool {
        !ext::is(r) && self.store.nodes.contains_key(r)
    }

    /// What a change to each typed parameter changes, by name: the type the picture draws it as,
    /// the program's class its type names, also inside a container (`e: Expense`, `rows: Row[]`,
    /// `Map<string, Row>`), or a library's class (`http: AxiosInstance`); else the language's own
    /// container it is (`lines: string[]` is an `Array`).
    fn inputs_of(&self) -> HashMap<String, String> {
        let src = self.src;
        let mut out = HashMap::new();
        for p in self.scope.and_then(|f| f.child_by_field_name("parameters")).map(named_children).unwrap_or_default() {
            let (Some(name), Some(ty)) =
                (p.child_by_field_name("pattern").filter(|x| x.kind() == "identifier"), p.child_by_field_name("type"))
            else {
                continue;
            };
            let drawn = type_names(Some(ty), src).into_iter().filter_map(|t| self.resolve(&t)).find(|r| {
                if ext::is(r) {
                    foreign_type(r, &OWN)
                } else {
                    self.class(Some(r.clone())).is_some()
                }
            });
            let written = text(ty, src).trim_start_matches(':').trim();
            let container = container_type(written, self.m).then(|| {
                let head = written.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$')).next().unwrap_or("");
                ext::of(&format!("javascript.{}", if knowledge::TYPESCRIPT_CHANGEABLE.contains(&head) { head } else { "Array" }))
            });
            if let Some(t) = drawn.or(container) {
                out.insert(text(name, src).to_string(), t);
            }
        }
        out
    }

    /// Whether a value holds one of the language's own containers that change in place: a name the
    /// body declares or builds as one, or a field declared as one of the class the value is
    /// (`this.rows`, `bag.items`), or of a class it inherits from.
    fn holds_container(&self, v: TsNode) -> bool {
        let src = self.src;
        let v = beneath(v, WRAPPERS);
        match v.kind() {
            "identifier" => self.locals.container(v, src),
            "member_expression" => {
                let (Some(o), Some(field)) = (v.child_by_field_name("object"), v.child_by_field_name("property")) else { return false };
                // the class of the value the field is read from: the body's own, a name's, a field's of the body's own
                let class = match o.kind() {
                    "this" => self.cls.clone(),
                    "identifier" => self.locals.of(o, src).cloned(),
                    "member_expression" if o.child_by_field_name("object").map_or(false, |t| t.kind() == "this") => {
                        o.child_by_field_name("property").and_then(|p| self.attrs.get(text(p, src)).cloned())
                    }
                    _ => None,
                };
                let (mut todo, mut seen) = (class.into_iter().collect::<Vec<_>>(), HashSet::new());
                while let Some(c) = todo.pop() {
                    if !seen.insert(c.clone()) {
                        continue;
                    }
                    let home = self.store.nodes.get(&c).and_then(|n| self.modules.get(&n.module));
                    if home.and_then(|m| m.containers.get(&c)).map_or(false, |fs| fs.contains(text(field, src))) {
                        return true;
                    }
                    todo.extend(self.store.classes.get(&c).map(|i| i.resolved.clone()).unwrap_or_default());
                }
                false
            }
            _ => false,
        }
    }

    /// Parameters and locals with a class type give receivers a type.
    fn typed(&mut self, n: &Node) {
        let src = self.src;
        // every name the body binds, each binding in its scope
        let top = self.scope.unwrap_or(self.body);
        let binders: Vec<TsNode> =
            walk(top, &BINDERS.iter().map(|(k, _)| *k).collect::<Vec<_>>()).into_iter().filter(|b| self.mine(*b)).collect();
        for b in binders {
            let field = BINDERS.iter().find(|(k, _)| *k == b.kind()).map_or("", |(_, f)| *f);
            // `for (x of xs)` binds no name anew: it assigns the one in scope
            if b.kind() == "for_in_statement" && b.child_by_field_name("kind").is_none() {
                continue;
            }
            let mut names = vec![];
            let Some(p) = b.child_by_field_name(field) else { continue };
            pattern_names(p, &mut names);
            let from = match b.kind() {
                "variable_declarator" => {
                    b.child_by_field_name("value").map(|v| (v, if p.kind() == "identifier" { BoundFrom::Whole } else { BoundFrom::Part }))
                }
                // a pattern takes parts of each element apart, which no name is
                _ if p.kind() != "identifier" && b.kind() != "variable_declarator" => None,
                "for_in_statement" => b.child_by_field_name("right").map(|v| (v, BoundFrom::Element)),
                "catch_clause" => None,
                _ => self.handed_each(b),
            };
            let scope = scope_of(b, top);
            for name in names {
                self.locals.bind(name, src, scope, from);
            }
        }
        // the function's own parameters, with the types they are written with
        let own: Vec<TsNode> = self
            .scope
            .and_then(|f| f.child_by_field_name("parameters"))
            .map(named_children)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|p| p.child_by_field_name("pattern").filter(|x| x.kind() == "identifier"))
            .collect();
        for p in n.params.iter().filter(|p| !p.ty.is_empty()) {
            let Some(name) = own.iter().find(|x| text(**x, src) == p.name).copied() else { continue };
            self.locals.written_at(name, src, &p.ty);
            if container_type(&p.ty, self.m) {
                self.locals.container_at(name, src);
            }
            let ty = p.ty.trim_start_matches("readonly ").split(['<', '[', ' ', '|']).next().unwrap_or("").to_string();
            // a container holds what its type holds: `Array<Plan>` and `Record<Mode, Plan>` hold Plans, as `Plan[]` does
            let held = || {
                let t = name.parent()?.child_by_field_name("type")?;
                type_names(Some(t), src).into_iter().find_map(|t| self.class(self.resolve(&t)))
            };
            let class = if knowledge::TYPESCRIPT_CONTAINERS.contains(&ty.as_str()) { held() } else { self.class(self.resolve(&[ty])) };
            if let Some(r) = class {
                self.locals.type_at(name, src, r);
            }
        }
        self.inputs = self.inputs_of();
        let declarators: Vec<TsNode> = walk(self.body, &["variable_declarator"]).into_iter().filter(|v| self.mine(*v)).collect();
        for v in declarators {
            let (Some(name), Some(value)) = (v.child_by_field_name("name"), v.child_by_field_name("value")) else { continue };
            if name.kind() != "identifier" {
                continue;
            }
            let ty = v.child_by_field_name("type").map(|t| text(t, src));
            if ty.map_or_else(|| builds_container(value, self.m, src), |t| container_type(t, self.m)) {
                self.locals.container_at(name, src);
            }
            if let Some(t) = ty {
                self.locals.written_at(name, src, t);
            }
            let declared = v.child_by_field_name("type").and_then(|t| named_children(t).into_iter().next()).map(|t| member_parts(t, src));
            let built = (value.kind() == "new_expression")
                .then(|| value.child_by_field_name("constructor").map(|c| member_parts(c, src)))
                .flatten();
            // a container holds what its type holds, as a parameter's does: `const out: Where[] = []` holds Wheres
            let holds = || {
                let t = v.child_by_field_name("type").filter(|_| ty.is_some_and(|t| container_type(t, self.m)))?;
                type_names(Some(t), src).into_iter().find_map(|t| self.class(self.resolve(&t)))
            };
            let typed = self.class(declared.or(built).and_then(|t| self.resolve(&t))).or_else(holds);
            // `const p = await getProvider()`: what the function hands on
            let handed = || {
                let call = walk(value, &["call_expression"]).into_iter().next()?;
                let f = self.resolve(&member_parts(function_of(call)?, src))?;
                self.returned.get(&f).cloned()
            };
            if let Some(r) = typed.or_else(handed) {
                self.locals.type_at(name, src, r);
            }
        }
        // `for (const r of rows)`, `rows.map((r) => ..)`: an element of the container, as its type is written (`rows: Row[]`)
        for (name, at, value) in self.locals.elements() {
            if let Some(r) = self.element_class(value) {
                self.locals.type_binding(&name, at, r);
            }
        }
    }

    /// The parameter `p` (a parameter, or the one an arrow binds bare) of a function handed to a
    /// method of a value (`rows.map((r) => ..)`, `rows.forEach(r => ..)`), where it is each element of
    /// that value: the first parameter, the second of a `reduce`, whose first is what it gathers.
    fn handed_each(&self, p: TsNode<'t>) -> Option<(TsNode<'t>, BoundFrom)> {
        // an arrow's bare parameter is its first; any other is the function's by its place among them
        let (fun, nth) = match p.kind() {
            "arrow_function" => (p, 0),
            _ => {
                let list = p.parent()?;
                let nth = named_children(list).into_iter().filter(|x| x.kind() != "comment").position(|x| x.id() == p.id())?;
                (list.parent().filter(|f| paths::function_node(*f).is_some_and(|x| x.id() == f.id()))?, nth)
            }
        };
        if Some(fun.id()) == self.scope.map(|s| s.id()) {
            return None;
        }
        let call =
            outermost(fun, WRAPPERS).parent().filter(|a| a.kind() == "arguments")?.parent().filter(|c| c.kind() == "call_expression")?;
        let f = function_of(call).filter(|f| f.kind() == "member_expression")?;
        let method = text(f.child_by_field_name("property")?, self.src);
        (nth == usize::from(matches!(method, "reduce" | "reduceRight"))).then_some((f.child_by_field_name("object")?, BoundFrom::Element))
    }

    /// The program's class each element of `value` is, as the type of what holds it is written: a
    /// field of a value of the program's class (`g.nodes`), a name of the body or one its module
    /// declares (`rows: Row[]`, `const STATIONS: readonly Station[]`).
    fn element_class(&self, value: TsNode) -> Option<String> {
        let src = self.src;
        let value = beneath(value, WRAPPERS);
        if value.kind() == "member_expression" {
            let (obj, prop) = (value.child_by_field_name("object")?, value.child_by_field_name("property")?);
            let owner = self.locals.of(obj, src).cloned()?;
            let types =
                self.store.classes.get(&owner).and_then(|k| k.fields.iter().find(|(f, _)| f == text(prop, src)).map(|(_, t)| t.clone()));
            return types.unwrap_or_default().into_iter().find_map(|t| self.class(self.resolve(&t)));
        }
        if value.kind() != "identifier" {
            return None;
        }
        let declared = || {
            walk(self.root, &["variable_declarator"])
                .into_iter()
                .filter(|v| module_level(*v, self.root, &self.m.bodies))
                .find(|v| v.child_by_field_name("name").map(|n| text(n, src)) == Some(text(value, src)))
                .and_then(|v| v.child_by_field_name("type"))
                .map(|t| text(t, src).to_string())
        };
        let written = self.locals.written(value, src).cloned().or_else(|| (!self.locals.bound(value, src)).then(declared).flatten())?;
        let ty = written.trim().trim_start_matches(':').trim().trim_start_matches("readonly ").trim();
        let head = ty.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$')).next().unwrap_or("");
        if !(ty.ends_with("[]") || knowledge::TYPESCRIPT_COLLECTIONS.contains(&head)) {
            return None;
        }
        // each element of a map is one of its values: its key, the first argument, chooses it
        let ty = match ty.find('<').filter(|_| knowledge::TYPESCRIPT_MAPS.contains(&head)) {
            Some(open) => {
                let mut depth = 0;
                let key = ty[open..].char_indices().find_map(|(i, c)| {
                    match c {
                        '<' | '[' | '(' | '{' => depth += 1,
                        '>' | ']' | ')' | '}' => depth -= 1,
                        ',' if depth == 1 => return Some(open + i + 1),
                        _ => {}
                    }
                    None
                });
                key.map_or("", |at| &ty[at..])
            }
            None => ty,
        };
        ty.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
            .filter(|w| !w.is_empty() && !knowledge::TYPESCRIPT_CONTAINERS.contains(w))
            .find_map(|w| self.class(self.resolve(&[w.to_string()])))
    }

    /// The values a package made, by the package, in the body and at its module's top level.
    fn made(&mut self) {
        let (m, src, root) = (self.m, self.src, self.root);
        let named =
            |v: TsNode<'t>| Some((v.child_by_field_name("name").filter(|n| n.kind() == "identifier")?, v.child_by_field_name("value")?));
        for v in walk(if self.is_module { root } else { self.body }, &["variable_declarator"]) {
            let Some((name, value)) = named(v) else { continue };
            if self.mine(v) || self.is_module || module_level(v, root, &m.bodies) {
                if let Some(p) = made_by_package(value, m, src) {
                    self.made.insert(text(name, src).to_string(), p);
                }
            }
        }
        // so does the parameter of a function a package's server runs on each connection it accepts
        // (`net.createServer((conn) => ..)`), as a parameter declared with the type would
        for f in walk(if self.is_module { root } else { self.body }, &["arrow_function", "function_expression"]) {
            let Some(server) = self.eval.served(m, f, 0) else { continue };
            let pkg = match server.kind() {
                "call_expression" => made_by_package(server, m, src),
                _ => self.made.get(text(server, src)).cloned(),
            };
            if let (Some(name), Some(pkg)) = (paths::param_names(f, src).into_iter().next().filter(|n| !n.is_empty()), pkg) {
                self.made.insert(name, pkg);
            }
        }
        // a parameter declared with a library's type holds a value its package made
        for p in self.scope.and_then(|f| f.child_by_field_name("parameters")).map(named_children).unwrap_or_default() {
            if let (Some(name), Some(pkg)) = (
                p.child_by_field_name("pattern").filter(|x| x.kind() == "identifier"),
                p.child_by_field_name("type").and_then(|t| typed_package(t, m, src)),
            ) {
                self.made.insert(text(name, src).to_string(), pkg);
            }
        }
        // so does a field of a value of the program's class, declared with a library's type, set
        // from what a package made, or a constructor's parameter property, in the class or one it
        // inherits from: of the body's own receiver (`this.db`), and of a name the body types (`api.http`)
        if let Some(k) = self.cls.clone().filter(|_| !self.is_module) {
            for (field, p) in self.fields_made(&k) {
                self.made.entry(format!("this.{field}")).or_insert(p);
            }
        }
        let typed: Vec<(String, String)> = self.locals.typed().map(|(n, k)| (n.to_string(), k.to_string())).collect();
        for (name, k) in typed {
            for (field, p) in self.fields_made(&k) {
                self.made.entry(format!("{name}.{field}")).or_insert(p);
            }
        }
        if !self.is_module {
            let tops = m
                .made
                .borrow_mut()
                .entry(None)
                .or_insert_with(|| {
                    let mut out = HashMap::new();
                    for v in walk(root, &["variable_declarator"]).into_iter().filter(|v| module_level(*v, root, &m.bodies)) {
                        let Some((name, value)) = named(v) else { continue };
                        if let Some(p) = made_by_package(value, m, src) {
                            out.entry(text(name, src).to_string()).or_insert(p);
                        }
                    }
                    out
                })
                .clone();
            for (k, p) in tops {
                self.made.entry(k).or_insert(p);
            }
        }
    }

    /// The fields of a value of the program's class `class` that hold what a package made, by name:
    /// the first class along its method resolution order that holds the field says which package.
    /// A class extends one class, so the order is the chain of its bases.
    fn fields_made(&self, class: &str) -> HashMap<String, String> {
        let mut out = HashMap::new();
        let (mut todo, mut seen) = (vec![class.to_string()], HashSet::new());
        while let Some(k) = todo.pop() {
            if !seen.insert(k.clone()) {
                continue;
            }
            todo.extend(self.store.classes.get(&k).into_iter().flat_map(|i| i.resolved.iter().rev()).filter(|b| self.own(b)).cloned());
            let Some(home) = self.store.nodes.get(&k).and_then(|n| self.modules.get(&n.module)) else { continue };
            let local = k.rsplit('.').next().unwrap_or(&k);
            let Some(node) = class_named(home, local) else { continue };
            let fields = home.made.borrow_mut().entry(Some(node.start_byte())).or_insert_with(|| class_made(node, home)).clone();
            for (field, p) in fields {
                out.entry(field.trim_start_matches("this.").to_string()).or_insert(p);
            }
        }
        out
    }

    /// A loop as the step it is: it takes what it goes over, and gives each element under the
    /// name it binds (`for (const n of g.nodes)` takes `g` and gives `n`).
    fn each(&mut self, steps: &mut Steps<'t, '_>, l: TsNode<'t>) {
        let (Some(left), Some(right)) = (l.child_by_field_name("left"), l.child_by_field_name("right")) else { return };
        let hands = self.hands(&[Some(right)], &steps.kept);
        // an element of what holds a value of the plan is one; of the body's own parameters alone, where they hold one
        let params = hands.iter().all(|h| h.param);
        let gives: Vec<Held> = walk(left, &["identifier"])
            .into_iter()
            .filter_map(|n| match self.locals.of(n, self.src) {
                Some(t) => Some(Held::new(t.clone(), text(n, self.src))),
                None => (!hands.is_empty()).then(|| Held { param: params, ..Held::new(String::new(), text(n, self.src)) }),
            })
            .collect();
        // the step stands where what it goes over is written: a call there hands it what it gives, the loop's body does not
        steps.each(right, &format!("for {} of {}", text(left, self.src), text(right, self.src)), hands, gives);
    }

    /// What a call is handed: the value it is called on, its arguments, an element's attributes. A
    /// call is handed the values the names in them hold (`mathematics.md`, "One fine plan"): each name
    /// is the value its binding holds where the name stands, with the class the body knows for it, or
    /// by its name alone where an earlier step of the body made it; a name the body binds to a value
    /// without a step (`const first = series[0]`, `const { width } = settings`, `const bottom = height -
    /// MARGIN.bottom`) stands for the values the names in that value hold. `this` in an argument, or
    /// in the value a call runs on, is the instance the body runs on (`new Database(this.path)`,
    /// `this.rows.push(e)`); a method called on `this` itself runs on it, which its step says
    /// (`on_receiver`), and is handed nothing more. Of a name written twice, the occurrence outside a
    /// call written inside counts, since a call that is a step of its own takes its names itself.
    fn hands(&self, parts: &[Option<TsNode>], kept: &HashSet<String>) -> Vec<Held> {
        self.hands_on(None, parts, kept)
    }

    /// What a method call is handed: the value it is called `on`, and its arguments (`hands`).
    fn hands_on(&self, on: Option<TsNode>, parts: &[Option<TsNode>], kept: &HashSet<String>) -> Vec<Held> {
        let mut out: Vec<(Held, bool)> = vec![];
        for (p, receiver) in on.map(|o| (o, true)).into_iter().chain(parts.iter().flatten().map(|p| (*p, false))) {
            for i in names_in(p) {
                if i.kind() == "this" && (self.cls.is_none() || receiver && beneath(p, WRAPPERS).id() == i.id()) {
                    continue;
                }
                let inner = std::iter::successors(i.parent(), |x| x.parent())
                    .take_while(|x| x.id() != p.id())
                    .any(|x| matches!(x.kind(), "call_expression" | "new_expression"));
                let mut held = vec![];
                self.held(i, kept, 0, &mut held);
                for mut h in held {
                    h.at = i.start_byte() as u32;
                    match out.iter_mut().find(|(o, _)| *o == h) {
                        Some((o, nested)) if *nested && !inner => (*o, *nested) = (h, false),
                        Some(_) => {}
                        None => out.push((h, inner)),
                    }
                }
            }
        }
        out.into_iter().map(|(h, _)| h).collect()
    }

    /// The constructor's parameters that declare a field of its class as well (`constructor(readonly until: string)`), as
    /// the values they hold: what the constructor keeps in the value it builds.
    fn properties(&self) -> Vec<Held> {
        let params = self.scope.and_then(|f| f.child_by_field_name("parameters")).map(named_children).unwrap_or_default();
        params
            .into_iter()
            .filter(|p| super::module::is_property(*p))
            .filter_map(|p| p.child_by_field_name("pattern").filter(|x| x.kind() == "identifier"))
            .map(|x| match self.type_of(x) {
                Some(ty) => Held::new(ty, text(x, self.src)),
                None => Held { param: true, ..Held::new("", text(x, self.src)) },
            })
            .collect()
    }

    /// Whether the name `i` is one of the function's own parameters where it stands: declared as one, and bound by
    /// nothing nearer.
    fn own_parameter(&self, i: TsNode) -> bool {
        let name = text(i, self.src);
        let declared = self
            .scope
            .and_then(|f| f.child_by_field_name("parameters"))
            .map(named_children)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|p| p.child_by_field_name("pattern").filter(|x| x.kind() == "identifier"))
            .find(|x| text(*x, self.src) == name);
        declared.is_some_and(|d| d.id() != i.id() && self.locals.which(d, self.src) == self.locals.which(i, self.src))
    }

    /// The values the name `i` holds where it stands (`hands`).
    fn held(&self, i: TsNode, kept: &HashSet<String>, depth: u32, out: &mut Vec<Held>) {
        let name = text(i, self.src);
        if i.kind() == "this" {
            out.extend(self.cls.clone().map(|k| Held::new(k, "")));
            return;
        }
        // the parameter of a function an array's method hands each element (`touches.map((t) => t.drawn())`) is that
        // element, a part of the array glued into the call as a projection is: it stands for what the array's names hold,
        // as `.map(|t| ..)` does in Rust. A loop's variable is the element its step gives
        if let Some((v, BoundFrom::Element)) =
            self.locals.from(i, self.src).filter(|(v, _)| v.parent().is_some_and(|p| p.kind() != "for_in_statement"))
        {
            if depth > 8
                || !walk(v, &["call_expression", "new_expression", "arrow_function", "function_expression", "await_expression"]).is_empty()
            {
                return;
            }
            for n in names_in(v) {
                let before = out.len();
                self.held(n, kept, depth + 1, out);
                for h in out[before..].iter_mut().filter(|h| h.bound == 0) {
                    h.bound = n.start_byte() as u32;
                }
            }
            return;
        }
        // a name bound to values without a step stands for what they are made of
        let (values, element) = self.bound_to(i);
        let computed = if depth <= 8 { self.stands(i, kept) } else { vec![] };
        if !computed.is_empty() {
            for n in computed {
                let before = out.len();
                self.held(n, kept, depth + 1, out);
                for h in out[before..].iter_mut().filter(|h| h.bound == 0) {
                    h.bound = n.start_byte() as u32;
                }
            }
            return;
        }
        if let Some(ty) = self.type_of(i) {
            return out.push(Held::new(ty, name));
        }
        if kept.contains(name) {
            return out.push(Held::new("", name));
        }
        if depth > 8 || element {
            return;
        }
        let stepless = |v: &TsNode| {
            walk(*v, &["call_expression", "new_expression", "arrow_function", "function_expression", "await_expression"]).is_empty()
        };
        // one of the function's own parameters of no type of the code's holds what its calls fill it with, which is known
        // once every body is read (`builder::settle_parameters`)
        if values.is_empty() && self.own_parameter(i) {
            return out.push(Held { param: true, ..Held::new("", name) });
        }
        if values.is_empty() || !values.iter().all(stepless) {
            return;
        }
        for n in values.iter().flat_map(|v| names_in(*v)) {
            let before = out.len();
            self.held(n, kept, depth + 1, out);
            // what a name bound to a projection holds comes from the name the projection starts from (`const t = this.title`)
            for h in out[before..].iter_mut().filter(|h| h.bound == 0) {
                h.bound = n.start_byte() as u32;
            }
        }
    }

    /// What the name `i` is bound to where it stands, by no step: what it is declared with, and what each assignment to it
    /// sets (`let unit; if (..) unit = CELSIUS; else unit = KELVIN`); and whether it is declared an element of what it is
    /// declared with (`for (const x of xs)`, `xs.map((x) => ..)`).
    fn bound_to(&self, i: TsNode) -> (Vec<TsNode<'t>>, bool) {
        let from = self.locals.from(i, self.src);
        let element = matches!(from, Some((_, BoundFrom::Element)));
        let declared = match from {
            Some((v, BoundFrom::Whole | BoundFrom::Part)) => Some(v),
            _ => None,
        };
        let binding = self.locals.which(i, self.src);
        let assigned = self
            .assigned
            .get(text(i, self.src))
            .into_iter()
            .flatten()
            .filter(|(left, _)| binding.is_some() && self.locals.which(*left, self.src) == binding)
            .map(|(_, right)| *right);
        (declared.into_iter().chain(assigned).collect(), element)
    }

    /// The names the name `i` stands for where it stands: where everything it is bound to (`bound_to`) is what TypeScript's
    /// own code computes of names, whatever type the name has (`const gap = first.db - second.db`, `const both: Level[] =
    /// [first, second]`), and no step made it anew, the names those values are computed of (`body::stands_for`); none for any
    /// other name.
    fn stands(&self, i: TsNode, kept: &HashSet<String>) -> Vec<TsNode<'t>> {
        let (values, element) = self.bound_to(i);
        if element || values.is_empty() || kept.contains(text(i, self.src)) {
            return vec![];
        }
        let computed: Vec<Vec<TsNode<'t>>> = values.iter().map(|v| crate::body::stands_for(*v, &USES, self.src, &part)).collect();
        if computed.iter().any(Vec::is_empty) {
            return vec![];
        }
        computed.into_iter().flatten().collect()
    }

    /// What a call gives back, by the name the body keeps it by. Its type is one a step gives: what
    /// the program's function `to` hands on, as the name it is kept by is declared or as the
    /// function says. A call Atlas does not follow gives no type the program names: a cast or an
    /// annotation on its value (`JSON.parse(..) as Config`) is asserted, not given.
    fn gives(&self, c: TsNode, to: Option<&str>) -> Vec<Held> {
        let ty = |name: &String| {
            to.and_then(|t| self.locals.get(name, c.start_byte()).cloned().or_else(|| self.returned.get(t).cloned())).unwrap_or_default()
        };
        kept_as(c, self.src).map(|name| Held::new(ty(&name), name)).into_iter().collect()
    }

    /// The values the body keeps of what a call Atlas does not follow gives: the name it is kept
    /// by, or the names a pattern takes out of it (`const { values } = parseArgs(..)`, `const [kind
    /// = "", ...flags] = process.argv.slice(2)`).
    fn parsed_into(&self, c: TsNode) -> Vec<Held> {
        match pattern_of(c) {
            Some(p) => {
                let mut names = vec![];
                pattern_names(p, &mut names);
                names.into_iter().map(|x| Held::new("", text(x, self.src))).collect()
            }
            None => self.gives(c, None),
        }
    }

    /// One site of the body, as the step it is.
    fn call(&mut self, steps: &mut Steps<'t, '_>, c: TsNode<'t>) {
        match c.kind() {
            "new_expression" => self.constructed(steps, c),
            "object" => self.literal(steps, c),
            "jsx_opening_element" | "jsx_self_closing_element" => self.rendered(steps, c),
            "identifier" => self.handed_on(steps, c),
            _ => self.called(steps, c),
        }
    }

    /// `new T(..)` of one of the program's classes, handing its constructor its arguments.
    fn constructed(&mut self, steps: &mut Steps<'t, '_>, c: TsNode<'t>) {
        let parts = c.child_by_field_name("constructor").map(|x| member_parts(x, self.src)).unwrap_or_default();
        if let Some(r) = self.class(self.resolve(&parts).filter(|r| self.own(r))) {
            let step = Step {
                hands: self.hands(&[c.child_by_field_name("arguments")], &steps.kept),
                gives: self.gives(c, Some(&r)),
                ..Default::default()
            };
            steps.call(c, &r, EdgeKind::Instantiates, None, step);
            let ctor = self.store.initializer_of(&r);
            if let Some(a) = ctor.and_then(|t| argument(self.modules, self.store, &self.id, &t, c, self.m, self.scope, &self.params)) {
                self.read.args.push(a);
            }
        } else if self.world(c, 0) {
            // a constructor of the language's or a library's: an operation, a step where the body names what it builds,
            // and where it builds the error the body throws, of its class whatever library the class is of
            let hands = self.hands(&[c.child_by_field_name("arguments")], &steps.kept);
            let mut gives = self.parsed_into(c);
            // what it builds holds the program's type its type arguments name, the last of them (`new Array<Item>()`, the
            // values of `new Map<string, Row>()`), as `[]` written for an `Item[]` does: a type the call gives, not asserts
            let holds = c.child_by_field_name("type_arguments").and_then(|t| {
                walk(t, &["type_identifier"]).into_iter().filter_map(|x| self.class(self.resolve(&[text(x, self.src).to_string()]))).last()
            });
            for g in gives.iter_mut().filter(|g| g.ty.is_empty() && !g.name.is_empty()) {
                g.ty = holds.clone().unwrap_or_default();
            }
            if gives.is_empty() && steps.raised(c) {
                let class =
                    self.resolve(&parts).filter(|r| ext::is(r)).unwrap_or_else(|| ext::of(&format!("javascript.{}", parts.join("."))));
                gives.push(Held::new(class, ""));
            }
            steps.op(c, &format!("{}()", parts.last().cloned().unwrap_or_default()), hands, gives);
        }
    }

    /// An object literal a type of the program's is written for, which the compiler checks it
    /// against, makes a value of that type, as `new` does, a struct expression in Rust and a call of
    /// the class in Python: one written `satisfies` or `as` the type (`{ .. } satisfies Settings`),
    /// one declared as the type (`const settings: Settings = { .. }`), one a function gives back where
    /// its return type names the type (`return { .. }` in `parseLine(): Month`, `(): Month => ({ ..
    /// })`), and one handed to a parameter of the program's function that its signature writes with
    /// the type (`rename({ category: "a" }, ..)` where `rename(e: Expense, ..)`). A step to the type,
    /// handed what the literal is built from; the call it is handed to is handed what it builds.
    fn literal(&self, steps: &mut Steps<'t, '_>, c: TsNode<'t>) {
        if let Some(r) = self.literal_of(c).filter(|r| self.own(r)) {
            let step = Step { hands: self.hands(&[Some(c)], &steps.kept), gives: self.gives(c, Some(&r)), ..Default::default() };
            steps.call(c, &r, EdgeKind::Instantiates, None, step);
        }
    }

    /// The class of the program's an object literal is written for (`literal`).
    fn literal_of(&self, c: TsNode<'t>) -> Option<String> {
        let src = self.src;
        // the nearest assertion around the literal names its type
        let mut n = c;
        while let Some(w) = n.parent().filter(|p| WRAPPERS.contains(&p.kind())) {
            if matches!(w.kind(), "as_expression" | "satisfies_expression") {
                let ty = named_children(w).into_iter().last()?;
                return self.class(self.resolve(&type_head(ty, src)));
            }
            n = w;
        }
        let v = outermost(c, WRAPPERS);
        let is = |p: TsNode, field: &str| p.child_by_field_name(field).map(|x| x.id()) == Some(v.id());
        let p = v.parent()?;
        let written = match p.kind() {
            "variable_declarator" if is(p, "value") => p.child_by_field_name("type"),
            // the function the value leaves: the nearest around the `return`, or an arrow's body
            "return_statement" => std::iter::successors(p.parent(), |n| n.parent())
                .find(|n| paths::function_node(*n).map_or(false, |f| f.id() == n.id()))
                .and_then(|f| f.child_by_field_name("return_type")),
            "arrow_function" if is(p, "body") => p.child_by_field_name("return_type"),
            "arguments" => return self.handed_as(p, v),
            _ => None,
        };
        let ty = written.and_then(|t| named_children(t).into_iter().next()).map(|t| promised(t, src))?;
        self.class(self.resolve(&type_head(ty, src)))
    }

    /// The class of the program's that the parameter an argument `a` of a call fills is written
    /// with, in the module the function is written in: the parameter at its place among the
    /// arguments, of the function the call reaches or of the initializer `new` runs. A rest
    /// parameter is written with a collection of what it takes, and after a spread no argument says
    /// which parameter it fills: neither is written with the literal's type.
    fn handed_as(&self, args: TsNode<'t>, a: TsNode<'t>) -> Option<String> {
        let call = args.parent()?;
        let callee = match call.kind() {
            "call_expression" => self.callee(call),
            "new_expression" => self.receiver(call).and_then(|k| self.store.initializer_of(&k)),
            _ => None,
        }
        .filter(|t| self.own(t))?;
        let before: Vec<TsNode> =
            named_children(args).into_iter().filter(|x| x.kind() != "comment").take_while(|x| x.id() != a.id()).collect();
        if before.iter().any(|x| x.kind() == "spread_element") {
            return None;
        }
        let node = self.store.nodes.get(&callee)?;
        let p = node.params.iter().filter(|p| !p.receiver).nth(before.len()).filter(|p| !p.rest)?;
        let parts: Vec<String> = p.ty.split('.').map(str::to_string).collect();
        if parts.iter().any(|x| x.is_empty() || !x.chars().all(|ch| ch.is_alphanumeric() || ch == '_' || ch == '$')) {
            return None;
        }
        let home = self.modules.get(&node.module)?;
        self.class(resolve(home, &parts, self.modules, &self.store.nodes).map(Resolved::id))
    }

    /// `<Kaart/>` renders a component, handed its attributes; a lowercase name is an element of
    /// the page.
    fn rendered(&self, steps: &mut Steps<'t, '_>, c: TsNode<'t>) {
        let parts = c.child_by_field_name("name").map(|x| member_parts(x, self.src)).unwrap_or_default();
        if parts.first().map_or(true, |p| !p.starts_with(|ch: char| ch.is_uppercase()) && parts.len() == 1) {
            return;
        }
        if let Some(r) = self.resolve(&parts).filter(|r| self.own(r)) {
            let kind = if self.store.nodes[&r].kind == NodeKind::Class { EdgeKind::Instantiates } else { EdgeKind::Calls };
            let attrs: Vec<Option<TsNode>> = walk(c, &["jsx_attribute"]).into_iter().map(Some).collect();
            steps.call(c, &r, kind, None, Step { hands: self.hands(&attrs, &steps.kept), ..Default::default() });
        }
    }

    /// An own function handed on as a value: called by whoever receives it.
    fn handed_on(&self, steps: &mut Steps<'t, '_>, c: TsNode<'t>) {
        let handed = c.parent().map_or(false, |p| match p.kind() {
            "arguments" | "array" | "jsx_expression" | "return_statement" => true,
            "pair" | "variable_declarator" => p.child_by_field_name("value").map(|v| v.id()) == Some(c.id()),
            _ => false,
        });
        let fun = |r: &String| matches!(self.store.nodes[r].kind, NodeKind::Function | NodeKind::Method);
        if let Some(r) = self.resolve(&[text(c, self.src).to_string()]).filter(|r| handed && self.own(r) && fun(r)) {
            let each = self.per_element(c, &steps.kept);
            steps.handed(c, c, &r, EdgeKind::Calls, each);
        }
    }

    /// What a method of an array that runs the function `f` it is handed first on each element of the
    /// array hands that function (`knowledge::TYPESCRIPT_EACH`): the values the array is a part of, and
    /// the parameter the element fills (`lines.map(parseLine)`).
    fn per_element(&self, f: TsNode<'t>, kept: &HashSet<String>) -> Option<(Vec<Held>, Slot)> {
        let args = f.parent().filter(|p| p.kind() == "arguments")?;
        if named_children(args).into_iter().find(|x| x.kind() != "comment").map(|x| x.id()) != Some(f.id()) {
            return None;
        }
        let method = function_of(args.parent().filter(|c| c.kind() == "call_expression")?).filter(|m| m.kind() == "member_expression")?;
        let name = text(method.child_by_field_name("property")?, self.src);
        let (_, element) = knowledge::TYPESCRIPT_EACH.iter().find(|(m, _)| *m == name)?;
        Some((self.hands(&[method.child_by_field_name("object")], kept), Slot::At(*element)))
    }

    /// The class of the value a method is called on, typed however the body knows it: built with
    /// `new`, or `super`, the class the body's own class extends. A cast names the class itself:
    /// `(x as Store).f()` runs on a Store.
    fn receiver(&self, o: TsNode) -> Option<String> {
        let src = self.src;
        match o.kind() {
            "as_expression" => self.class(self.resolve(&type_head(named_children(o).into_iter().last()?, src))).filter(|r| self.own(r)),
            k if WRAPPERS.contains(&k) => self.receiver(wrapped(o)?),
            "this" => self.cls.clone(),
            "super" => self.class(self.resolve(self.extends()?)).filter(|r| self.own(r)),
            "identifier" => self.type_of(o),
            "call_expression" => self.returned.get(&self.resolve(&member_parts(function_of(o)?, src))?).cloned(),
            "new_expression" => self.class(self.resolve(&member_parts(o.child_by_field_name("constructor")?, src))).filter(|r| self.own(r)),
            "member_expression" if o.child_by_field_name("object").map(|x| x.kind()) == Some("this") => {
                self.attrs.get(text(o.child_by_field_name("property")?, src)).cloned()
            }
            _ => None,
        }
    }

    /// The class the body's own class extends, as written: the first of its bases.
    fn extends(&self) -> Option<&[String]> {
        self.store.classes.get(self.cls.as_deref()?)?.bases.first().map(Vec::as_slice)
    }

    /// What call `c` reaches: the method of the class of the value it is called on, else what
    /// the name it is called by is bound to; nothing when it is called through a value the body
    /// cannot name (`EXPORTERS[format](..)`, `make()(..)`). `super(..)` reaches the initializer
    /// the class the body's class extends runs.
    pub(super) fn callee(&self, c: TsNode) -> Option<String> {
        let f = function_of(c)?;
        if f.kind() == "super" {
            return self.store.initializer_of(&self.receiver(f)?);
        }
        let method = (f.kind() == "member_expression")
            .then(|| Some((f.child_by_field_name("object")?, text(f.child_by_field_name("property")?, self.src))))
            .flatten();
        if let Some(t) = method.and_then(|(o, name)| self.store.method_of(&self.receiver(o)?, name)) {
            return Some(t);
        }
        let parts = member_parts(f, self.src);
        if parts.is_empty() {
            None
        } else {
            self.resolve(&parts)
        }
    }

    /// `Source.prototype.read.call(v, ..)` or `.apply(v, [..])`: the method the class's prototype
    /// finds, called through its class on the value handed first, and no override of it. Whether
    /// the call is one.
    fn through_prototype(&mut self, steps: &mut Steps<'t, '_>, c: TsNode<'t>, parts: &[String]) -> bool {
        let [class @ .., proto, name, how] = parts else { return false };
        if proto != "prototype" || !matches!(how.as_str(), "call" | "apply") {
            return false;
        }
        let Some(t) = self.class(self.resolve(class)).filter(|k| self.own(k)).and_then(|k| self.store.method_of(&k, name)) else {
            return false;
        };
        // the value it runs on: the body's own receiver, or a value of a class below the method's
        let value = c.child_by_field_name("arguments").and_then(|a| named_children(a).into_iter().next());
        let on_receiver = value.map_or(false, |v| v.kind() == "this");
        let on = value
            .filter(|_| !on_receiver)
            .and_then(|v| self.receiver(v))
            .filter(|k| self.store.nodes[&t].cls.as_deref().map_or(false, |b| b != k.as_str()))
            .unwrap_or_default();
        if value_used(c) {
            self.read.used.push(t.clone());
        }
        let hands = self.hands(&[c.child_by_field_name("arguments")], &steps.kept);
        let step = Step { on, on_receiver, named: true, receiver_first: true, hands, gives: self.gives(c, Some(&t)), ..Default::default() };
        steps.call(c, &t, EdgeKind::Calls, None, step);
        true
    }

    /// `f(..)` or `x.m(..)`: the program's function or method, found on the class of the value it
    /// is called on, or a call Atlas does not follow: a library's operation, which may cross into
    /// the world, or a call through a value the body cannot name.
    fn called(&mut self, steps: &mut Steps<'t, '_>, c: TsNode<'t>) {
        let src = self.src;
        let Some(f) = function_of(c) else { return };
        if read_in_place(f) {
            return;
        }
        let parts = member_parts(f, src);
        if self.through_prototype(steps, c, &parts) {
            return;
        }
        let object = (f.kind() == "member_expression").then(|| f.child_by_field_name("object")).flatten();
        let class = object.and_then(|o| self.receiver(o));
        let target = self.callee(c);
        let hands = self.hands_on(object, &[c.child_by_field_name("arguments")], &steps.kept);
        // the words of the command line, kept under a name: a step that parses them with no options
        if self.keeps_command_line.contains(&c.id()) {
            steps.parses(c, &written_callee(f, src), hands, self.parsed_into(c), vec![]);
            return;
        }
        match target {
            Some(t) if self.own(&t) => {
                // `super.f()` runs on the body's own receiver the next `f` after the class it is
                // written in; a static method called through a class (`Base.make()`) is that
                // class's, and no override of it, and so is the initializer `super(..)` runs
                let sup = object.map_or(false, |o| o.kind() == "super");
                let named = f.kind() == "super"
                    || !sup
                        && self.store.nodes[&t].kind == NodeKind::Method
                        && object
                            .filter(|o| o.kind() == "identifier")
                            .and_then(|o| self.resolve(&[text(o, src).to_string()]))
                            .map_or(false, |r| self.store.nodes.get(&r).map_or(false, |n| n.kind == NodeKind::Class));
                let after = if sup { self.cls.clone().unwrap_or_default() } else { String::new() };
                // a method found on a base runs on the subclass the value is; the body's own receiver
                // is of the class the route runs the body on, the one it is written in or one below
                let on_receiver = object.map_or(false, |o| o.kind() == "this") || !after.is_empty();
                // a value built at the call (`new Section().show()`) is of its class and none below it, also where that
                // is the method's own; one the code declares is of the class it declares or one below it
                let built = object
                    .and_then(|o| if WRAPPERS.contains(&o.kind()) { wrapped(o) } else { Some(o) })
                    .map_or(false, |o| o.kind() == "new_expression");
                let on = class
                    .filter(|k| !on_receiver && self.store.nodes[&t].cls.as_deref().map_or(false, |b| built || b != k.as_str()))
                    .unwrap_or_default();
                let kind = if self.store.nodes[&t].kind == NodeKind::Class { EdgeKind::Instantiates } else { EdgeKind::Calls };
                if kind == EdgeKind::Calls && value_used(c) {
                    self.read.used.push(t.clone());
                }
                // what it changes of what it is handed, it gives anew under the name it was handed
                let mut gives = self.gives(c, Some(&t));
                for h in self.changed_values(&t, object, c, &steps.kept) {
                    if !gives.iter().any(|g| g.name == h.name) {
                        gives.push(h);
                    }
                }
                let step = Step { built: built && !on.is_empty(), on, on_receiver, named, after, hands, gives, ..Default::default() };
                steps.call(c, &t, kind, None, step);
                let callee = if kind == EdgeKind::Instantiates { self.store.initializer_of(&t) } else { Some(t.clone()) };
                if let Some(mut a) =
                    callee.and_then(|t| argument(self.modules, self.store, &self.id, &t, c, self.m, self.scope, &self.params))
                {
                    a.on =
                        object.filter(|o| o.kind() == "identifier").map(|o| text(o, src).to_string()).filter(|o| self.params.contains(o));
                    self.read.args.push(a);
                }
            }
            // a call not followed is a step where the body keeps what it gives, and is counted
            // by the name the code calls it by
            _ if parts.is_empty() => {
                let (name, bucket) = (written_callee(f, src), self.bucket_of(f));
                let world = matches!(bucket, Bucket::Library | Bucket::Builtin) || self.world(object.unwrap_or(f), 0);
                self.unread(steps, c, &name, world, hands, vec![]);
                // what the language's own computes Atlas reads; what anything else does, it does not see
                if bucket != Bucket::Builtin {
                    steps.dropped(c, &name);
                }
                self.read.unresolved.push((name, bucket));
            }
            _ => {
                let changed = self.changes(c);
                // a method on what a guard narrowed to the language's own container is the language's
                let narrowed = object.map_or(false, |o| self.narrowed(o));
                let bucket = if changed.is_empty() && !narrowed { self.bucket_path(&parts) } else { Bucket::Builtin };
                let world = matches!(bucket, Bucket::Library | Bucket::Builtin) || self.world(object.unwrap_or(f), 0);
                // an operation of the language's or a library's by its name, a call Atlas cannot follow as written
                let name = if world { parts.last().cloned().unwrap_or_default() } else { text(f, src).to_string() };
                let parsed = self.eval.command_line(At { m: self.m, scope: self.scope, params: &self.params }, c);
                // the language's own, a change and a parse of the command line are what the tables say
                let known = parsed.is_some() || bucket == Bucket::Builtin;
                match parsed {
                    Some(options) => steps.parses(c, &name, hands, self.parsed_into(c), options),
                    None => self.unread(steps, c, &name, world, hands, changed),
                }
                self.read.unresolved.push((parts.iter().take(2).cloned().collect::<Vec<_>>().join("."), bucket));
                let exits = matches!(parts.as_slice(), [.., module, f] if knowledge::ends_the_run(module, f));
                self.read.exits |= exits;
                let crossed = (self.read.touches.len(), self.read.calls.len());
                self.crossed(c, &parts);
                // a package's call the tables say nothing of beside what it gives, or one Atlas
                // cannot follow: what it does, Atlas does not see
                if !known && !exits && crossed == (self.read.touches.len(), self.read.calls.len()) {
                    steps.dropped(c, &text(f, src));
                }
            }
        }
    }

    /// What a callee no name path reaches starts from, as `bucket` reads a name: the first name
    /// path it is written on (`this.db.prepare(..).run` starts at `this.db`). A method on what a
    /// call Atlas follows gives is on a value the body made; one on a literal or on what an
    /// operator computes is the language's own (`"a,b".split`, `[..xs].sort`, `(a - b).toFixed`);
    /// `super` is the class the body's class extends.
    fn bucket_of(&self, n: TsNode) -> Bucket {
        let parts = member_parts(n, self.src);
        if !parts.is_empty() {
            return self.bucket_path(&parts);
        }
        let logical = || n.child_by_field_name("operator").map_or(false, |o| matches!(text(o, self.src), "??" | "||" | "&&"));
        let inner = match n.kind() {
            // what a step Atlas follows gives, or what the body builds of its own classes, is a
            // value the body made
            "call_expression" if self.callee(n).map_or(false, |t| self.own(&t)) => return Bucket::Local,
            "new_expression" if self.receiver(n).is_some() => return Bucket::Local,
            "member_expression" | "subscript_expression" => n.child_by_field_name("object"),
            "call_expression" => function_of(n),
            "new_expression" => n.child_by_field_name("constructor"),
            k if WRAPPERS.contains(&k) => wrapped(n),
            // `a ?? b` is one of its operands, any other operator's value the language's own
            "binary_expression" if logical() => n.child_by_field_name("left"),
            "ternary_expression" => n.child_by_field_name("consequence"),
            "binary_expression" | "unary_expression" | "string" | "template_string" | "number" | "array" | "object" | "regex" | "true"
            | "false" => return Bucket::Builtin,
            "super" => return self.extends().and_then(|b| b.first()).map_or(Bucket::Unknown, |b| self.bucket(b)),
            _ => None,
        };
        inner.map_or(Bucket::Unknown, |i| self.bucket_of(i))
    }

    /// A call the body reads no further, as a step: an operation of the language's or a library's
    /// (`world`) where the body keeps what it gives, by a name or the names a pattern takes out of
    /// it, or changes the value it runs on (`changed`); any other call Atlas cannot follow always,
    /// since it may run the program's code. What an operation gives without a name goes into the
    /// call around it.
    fn unread(&self, steps: &mut Steps<'t, '_>, c: TsNode<'t>, name: &str, world: bool, hands: Vec<Held>, changed: Vec<Held>) {
        if !changed.is_empty() {
            steps.changes(c, name, hands, self.gives(c, None), changed);
        } else if world {
            steps.op(c, name, hands, self.parsed_into(c));
        } else {
            steps.not_followed(c, name, hands, self.gives(c, None));
        }
    }

    /// The value a method of the language's own changes, as the value the call gives: `rows` for
    /// `rows.push(row)`, where the body holds an array, a `Map` or a `Set` by that name; and the
    /// owner of a field that holds one, since a change to a part changes the whole: `this` for
    /// `this.rows.push(e)`, `bag` for `bag.items.push(x)`.
    fn changes(&self, c: TsNode) -> Vec<Held> {
        let src = self.src;
        let Some(f) = function_of(c).filter(|f| f.kind() == "member_expression") else { return vec![] };
        let (Some(o), Some(p)) = (f.child_by_field_name("object"), f.child_by_field_name("property")) else { return vec![] };
        if !knowledge::mutates(knowledge::Registry::Npm, "javascript", text(p, src)) {
            return vec![];
        }
        let o = beneath(o, WRAPPERS);
        let changes = if o.kind() == "identifier" { self.locals.container(o, src) || self.narrowed(o) } else { self.holds_container(o) };
        let Some(owner) = root_of(o).filter(|_| changes) else { return vec![] };
        let (ty, name) = match owner.kind() {
            "this" => (self.cls.clone().unwrap_or_default(), "this"),
            _ => (self.type_of(owner).unwrap_or_default(), text(owner, src)),
        };
        vec![Held::at(ty, name.to_string(), owner.start_byte(), false)]
    }

    /// Whether the name `o` is narrowed where it stands to one of the language's own containers
    /// by a guard of the language's (`Array.isArray(x)`, `knowledge::TYPESCRIPT_GUARDS`): it stands
    /// where the guard holds, on the right of `guard && ..` or of `!guard || ..`, in the arm of an
    /// `if` or a `?:` the guard decides so, or after an `if (!guard)` whose arm leaves. A function
    /// written in the body is a scope of its own, which the guard does not reach.
    fn narrowed(&self, o: TsNode) -> bool {
        let src = self.src;
        // a guard is handed a name, so only a name it is handed is narrowed
        let name = text(o, src);
        let guard = |e: TsNode| -> bool {
            let Some(f) = (e.kind() == "call_expression").then(|| function_of(e)).flatten() else { return false };
            let first = e.child_by_field_name("arguments").and_then(|a| named_children(a).into_iter().next());
            match member_parts(f, src).as_slice() {
                [global, guard] => {
                    knowledge::TYPESCRIPT_GUARDS.contains(&(global.as_str(), guard.as_str()))
                        && self.bucket(global) == Bucket::Builtin
                        && first.map_or(false, |a| a.kind() == "identifier" && text(a, src) == name)
                }
                _ => false,
            }
        };
        // whether `e` evaluating to `truth` says the guard holds
        fn holds(e: TsNode, truth: bool, src: &[u8], guard: &dyn Fn(TsNode) -> bool) -> bool {
            let e = beneath(e, WRAPPERS);
            let operator = e.child_by_field_name("operator").map(|o| text(o, src));
            let sides = || [e.child_by_field_name("left"), e.child_by_field_name("right")].into_iter().flatten();
            match (e.kind(), operator) {
                ("unary_expression", Some("!")) => e.child_by_field_name("argument").map_or(false, |a| holds(a, !truth, src, guard)),
                ("binary_expression", Some("&&")) => truth && sides().any(|s| holds(s, true, src, guard)),
                ("binary_expression", Some("||")) => !truth && sides().any(|s| holds(s, false, src, guard)),
                _ => truth && guard(e),
            }
        }
        let holds = |e: Option<TsNode>, truth: bool| e.map_or(false, |e| holds(e, truth, src, &guard));
        let leaves = |s: TsNode| {
            let last = if s.kind() == "statement_block" { named_children(s).into_iter().last() } else { Some(s) };
            last.map_or(false, |l| matches!(l.kind(), "return_statement" | "throw_statement"))
        };
        let mut n = o;
        while let Some(p) = n.parent() {
            let is = |field: &str| p.child_by_field_name(field).map(|x| x.id()) == Some(n.id());
            let operator = p.child_by_field_name("operator").map(|o| text(o, src));
            let narrows = match (p.kind(), operator) {
                ("binary_expression", Some("&&")) => is("right") && holds(p.child_by_field_name("left"), true),
                ("binary_expression", Some("||")) => is("right") && holds(p.child_by_field_name("left"), false),
                ("ternary_expression" | "if_statement", _) if is("consequence") => holds(p.child_by_field_name("condition"), true),
                ("ternary_expression", _) if is("alternative") => holds(p.child_by_field_name("condition"), false),
                ("else_clause", _) => p.parent().map_or(false, |i| holds(i.child_by_field_name("condition"), false)),
                ("statement_block" | "program", _) => named_children(p).into_iter().take_while(|s| s.id() != n.id()).any(|s| {
                    s.kind() == "if_statement"
                        && s.child_by_field_name("alternative").is_none()
                        && s.child_by_field_name("consequence").map_or(false, leaves)
                        && holds(s.child_by_field_name("condition"), false)
                }),
                _ => false,
            };
            if narrows {
                return true;
            }
            if paths::function_node(p).map_or(false, |f| f.id() == p.id()) || p.id() == self.body.id() {
                return false;
            }
            n = p;
        }
        false
    }

    /// Whether `e` is a value of the language's or of a library's, not of the program's, so that a
    /// method called on it is an operation of theirs: a literal or what an operator computes, what a
    /// global or an import holds, what such a value's operation gives, a value the body names that is
    /// written with such a type (`text: string`, `series: readonly Month[]`, a library's class) or set
    /// to such a value, and each element of one, also as a function handed to its method takes it
    /// (`text.split("\n").filter((line) => line.trim() !== "")`). A value of the program's class, of
    /// no known type or from a function of the program's that names none, may be the program's.
    fn world(&self, e: TsNode, depth: u32) -> bool {
        if depth > 8 {
            return false;
        }
        let src = self.src;
        let e = beneath(e, WRAPPERS);
        match e.kind() {
            // `a ?? b`, `a || b`, `a && b` are one of their operands, what any other operator computes the language's
            "binary_expression" if e.child_by_field_name("operator").is_some_and(|o| matches!(text(o, src), "??" | "||" | "&&")) => {
                ["left", "right"].iter().all(|f| e.child_by_field_name(f).is_some_and(|x| self.world(x, depth + 1)))
            }
            "string" | "template_string" | "number" | "array" | "regex" | "true" | "false" | "binary_expression" | "unary_expression" => {
                true
            }
            "identifier" => {
                // as its type is written: `rows: Row[]` is an Array, whatever its elements are
                if let Some(w) = self.locals.written(e, src) {
                    return self.world_type(w);
                }
                if self.locals.of(e, src).is_some() {
                    return false;
                }
                if !self.locals.bound(e, src) {
                    return self.class(self.type_of(e)).is_none() && matches!(self.bucket(text(e, src)), Bucket::Library | Bucket::Builtin);
                }
                match self.locals.from(e, src) {
                    Some((v, BoundFrom::Whole)) => self.world(v, depth + 1),
                    Some((v, BoundFrom::Element)) => self.element_class(v).is_none() && self.world(v, depth + 1),
                    Some((_, BoundFrom::Part)) | None => false,
                }
            }
            "member_expression" => {
                let Some(o) = e.child_by_field_name("object") else { return false };
                // a field declared as one of the language's own containers is one, as a name written so is (`touches:
                // Touch[]`, of the node `n.touches` is read of)
                if self.holds_container(e) {
                    return true;
                }
                let parts = member_parts(e, src);
                if parts.len() == 2 && parts[0] == "this" {
                    return self.made.contains_key(&format!("this.{}", parts[1]))
                        || self.cls.as_deref().is_some_and(|k| self.world_field(k, &parts[1]));
                }
                // a field of a value of the language's or a library's, or of the program's class as the class declares it
                // (`r.score: number`)
                if self.world(o, depth + 1) || self.made.contains_key(&parts.join(".")) {
                    return true;
                }
                match (self.receiver(o), e.child_by_field_name("property")) {
                    (Some(k), Some(field)) => self.world_field(&k, text(field, src)),
                    _ => false,
                }
            }
            "subscript_expression" => {
                e.child_by_field_name("object").is_some_and(|o| self.element_class(o).is_none() && self.world(o, depth + 1))
            }
            "call_expression" => match self.callee(e).filter(|t| self.own(t)) {
                // what a function of the program's gives, as its signature writes it
                Some(t) => self.store.nodes[&t].returns.as_ref().is_some_and(|r| self.world_type(&r.written)),
                None => {
                    let Some(f) = function_of(e) else { return false };
                    let parts = member_parts(f, src);
                    let theirs = !parts.is_empty() && matches!(self.bucket_path(&parts), Bucket::Library | Bucket::Builtin);
                    match f.child_by_field_name("object").filter(|_| f.kind() == "member_expression") {
                        // a method of the value it is called on
                        Some(o) => theirs || self.world(o, depth + 1),
                        None => theirs || self.world(f, depth + 1),
                    }
                }
            },
            "new_expression" => {
                let parts = e.child_by_field_name("constructor").map(|c| member_parts(c, src)).unwrap_or_default();
                !parts.is_empty() && self.receiver(e).is_none() && matches!(self.bucket_path(&parts), Bucket::Library | Bucket::Builtin)
            }
            "ternary_expression" => {
                ["consequence", "alternative"].iter().all(|f| e.child_by_field_name(f).is_some_and(|x| self.world(x, depth + 1)))
            }
            _ => false,
        }
    }

    /// Whether the field `field` of the program's class `class` holds a value of the language's or a
    /// library's: one its declaration types with a library's class, or with no type the program or
    /// a library names (`score: number`).
    fn world_field(&self, class: &str, field: &str) -> bool {
        let Some(k) = self.store.classes.get(class) else { return false };
        k.foreign.contains_key(field) || k.fields.iter().any(|(f, t)| f == field && t.is_empty())
    }

    /// Whether a type as written is the language's or a library's: one of its own (`string`,
    /// `Date`, `ReadonlyMap<K, V>`), a collection of anything (`Month[]` is an Array), or one a
    /// package exports.
    fn world_type(&self, written: &str) -> bool {
        let t = written.trim().trim_start_matches(':').trim().trim_start_matches("readonly ").trim();
        let t = t.strip_prefix("Promise<").and_then(|x| x.strip_suffix('>')).unwrap_or(t).trim();
        if t.ends_with("[]") {
            return true;
        }
        let head = t.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$')).next().unwrap_or("");
        if matches!(head, "string" | "number" | "boolean" | "bigint" | "symbol" | "void" | "undefined" | "null" | "never")
            || knowledge::TYPESCRIPT_COLLECTIONS.contains(&head)
        {
            return true;
        }
        match self.m.bindings.get(head) {
            Some(Bound::Ext(_)) => true,
            Some(Bound::Own(_)) => false,
            None => !self.m.symbols.contains_key(head) && knowledge::TYPESCRIPT_GLOBALS.contains(&head),
        }
    }

    fn bucket_path(&self, parts: &[String]) -> Bucket {
        self.bucket(self.held_field(parts).as_deref().unwrap_or(&parts[0]))
    }

    /// The field of a value of the program's class a call goes through, when it holds what a package
    /// made: `this.db` of `this.db.prepare(..)`, `api.http` of `api.http.delete(..)`. A call on it is
    /// a call on the value the field holds.
    fn held_field(&self, parts: &[String]) -> Option<String> {
        (parts.len() > 2).then(|| format!("{}.{}", parts[0], parts[1])).filter(|f| self.made.contains_key(f))
    }

    /// What the name a call not followed starts from is bound to: in the body (a parameter, a
    /// local, the receiver), imported from a package, the module's own, or nothing the program
    /// binds: the language's globals. A name that holds a value a package made is the package's.
    fn bucket(&self, head: &str) -> Bucket {
        let made = self.made.contains_key(head);
        if head == "this" || self.params.iter().any(|p| p == head) || self.bound_in().contains(head) {
            return if made { Bucket::Library } else { Bucket::Local };
        }
        match self.m.bindings.get(head) {
            Some(Bound::Ext(_)) => Bucket::Library,
            Some(Bound::Own(_)) => Bucket::Unknown,
            None if made => Bucket::Library,
            None if !self.m.symbols.contains_key(head) && knowledge::TYPESCRIPT_GLOBALS.contains(&head) => Bucket::Builtin,
            None if self.languages(head) => Bucket::Builtin,
            None => Bucket::Unknown,
        }
    }

    /// Whether the module's own constant `name` holds one of the language's own values: built as a literal of one (`[..]`,
    /// `new Map()`) or written as its container (`const CURRENCIES: readonly Currency[] = ..`). A method on it is the
    /// language's operation.
    fn languages(&self, name: &str) -> bool {
        let src = self.src;
        let top = named_children(self.root).into_iter().flat_map(|s| match s.kind() {
            "export_statement" => s.child_by_field_name("declaration").into_iter().collect::<Vec<_>>(),
            _ => vec![s],
        });
        let declared = top
            .filter(|s| matches!(s.kind(), "lexical_declaration" | "variable_declaration"))
            .flat_map(named_children)
            .filter(|v| v.kind() == "variable_declarator")
            .find(|v| v.child_by_field_name("name").is_some_and(|n| text(n, src) == name));
        let Some(v) = declared else { return false };
        let written =
            v.child_by_field_name("type").map(|t| text(t, src).trim_start_matches(':').trim().trim_start_matches("readonly ").to_string());
        let container = |t: &str| {
            t.ends_with("[]")
                || ["Array<", "ReadonlyArray<", "Map<", "ReadonlyMap<", "Set<", "ReadonlySet<"].iter().any(|p| t.starts_with(p))
        };
        written.as_deref().map_or_else(|| v.child_by_field_name("value").is_some_and(|x| builds_container(x, self.m, src)), container)
    }

    /// The names the body binds: its parameters, each variable it declares, the parameters of
    /// the functions in it, a caught error, a loop's variable, and the functions it declares.
    pub(super) fn bound_in(&self) -> HashSet<String> {
        let fields: Vec<(&str, &str)> =
            BINDERS.iter().copied().chain([("function_declaration", "name"), ("generator_function_declaration", "name")]).collect();
        let kinds: Vec<&str> = fields.iter().map(|(k, _)| *k).collect();
        let mut names = vec![];
        for n in walk(self.scope.unwrap_or(self.body), &kinds).into_iter().filter(|n| self.mine(*n)) {
            let field = fields.iter().find(|(k, _)| *k == n.kind()).map_or("", |(_, f)| *f);
            if let Some(p) = n.child_by_field_name(field) {
                pattern_names(p, &mut names);
            }
        }
        names.into_iter().map(|n| text(n, self.src).to_string()).collect()
    }

    /// The names a listener's function binds what the event brings to, where `c` registers one for an
    /// event it names (`socket.on("data", (chunk) => ..)`, `knowledge::NODE_LISTENERS`): its
    /// parameters, none where it takes none.
    fn received(&self, c: TsNode<'t>) -> Option<Vec<TsNode<'t>>> {
        let f = function_of(c).filter(|f| f.kind() == "member_expression")?;
        if !knowledge::NODE_LISTENERS.contains(&text(f.child_by_field_name("property")?, self.src)) {
            return None;
        }
        let args = c.child_by_field_name("arguments").map(named_children).unwrap_or_default();
        if args.first()?.kind() != "string" {
            return None;
        }
        let listener = args[1..]
            .iter()
            .map(|a| beneath(*a, WRAPPERS))
            .find(|a| matches!(a.kind(), "arrow_function" | "function_expression" | "function"))?;
        let mut names = vec![];
        if let Some(p) = listener.child_by_field_name("parameter") {
            pattern_names(p, &mut names);
        }
        for p in listener.child_by_field_name("parameters").map(named_children).unwrap_or_default() {
            if let Some(pattern) = p.child_by_field_name("pattern") {
                pattern_names(pattern, &mut names);
            }
        }
        Some(names)
    }

    /// The touch the body made where it made the door the name `on` holds (`const conn =
    /// net.createConnection(..)`), with the term its place waits for: the last touch in `world` the
    /// calls of the value the name is declared with made.
    fn opening(&self, on: TsNode<'t>, world: World) -> Option<(Touch, Vec<crate::paths::Built>)> {
        let (value, BoundFrom::Whole) = self.locals.from(on, self.src)? else { return None };
        let (now, waiting) = self.read.made.within(value);
        let made = now.into_iter().map(|i| (&self.read.touches[i], vec![])).chain(waiting.into_iter().map(|i| {
            let (t, b) = &self.read.calls[i];
            (t, b.clone())
        }));
        made.filter(|(t, _)| t.kind == world).last().map(|(t, b)| (t.clone(), b))
    }

    /// What a library's call crosses into: a file call, a request or a database statement waits
    /// for the path, URL or statement it is handed, placed once every call is known; any other
    /// touches what its facts name.
    fn crossed(&mut self, c: TsNode<'t>, parts: &[String]) {
        let (m, src) = (self.m, self.src);
        let (head, parts) = match self.held_field(parts) {
            Some(f) => (f, &parts[1..]),
            None => (parts[0].clone(), parts),
        };
        let pkg = match m.bindings.get(&head) {
            Some(Bound::Ext(e)) => Some(package_from_ext(e, &m.ext_pkgs)),
            _ => self.made.get(&head).cloned(),
        };
        let args = c.child_by_field_name("arguments").map(named_children).unwrap_or_default();
        // a Node socket receives by the event a listener waits for, which names the operation
        // (`socket.on("data", ..)`); a method that sends what it is handed first is that sending
        // where it is handed data (`socket.end(data)` is `socket.write(data)`, then `socket.end()`)
        let socket = pkg.as_deref().is_some_and(knowledge::node_socket_package);
        let event = parts
            .last()
            .filter(|l| socket && knowledge::NODE_LISTENERS.contains(&l.as_str()))
            .and(args.first())
            .filter(|a| a.kind() == "string")
            .map(|a| unquote(text(*a, src)));
        let handed = |a: &TsNode| !matches!(beneath(*a, WRAPPERS).kind(), "arrow_function" | "function_expression" | "function");
        let sent = parts
            .last()
            .filter(|_| socket && args.first().is_some_and(handed))
            .and_then(|l| knowledge::NODE_SENDS_FIRST.iter().find(|(m, _)| *m == l.as_str()))
            .map(|(_, op)| op.to_string());
        let listened: Vec<String>;
        let parts = match event.or(sent) {
            Some(e) => {
                listened = parts[..parts.len() - 1].iter().cloned().chain([e]).collect();
                &listened[..]
            }
            None => parts,
        };
        // traffic through a `FileHandle` is at the file it was opened at, wherever the handle is handed
        // (`docs/reference/mathematics.md`, "A door keeps its place"), never at what it is handed
        let imported = matches!(m.bindings.get(&head), Some(Bound::Ext(_)));
        let handle = pkg.as_deref().is_some_and(|p| knowledge::NODE_FS.contains(&p)) && !imported;
        let traffic = parts.last().filter(|_| handle).and_then(|l| knowledge::NODE_FILE_TRAFFIC.iter().find(|(o, _)| *o == l.as_str()));
        let object = function_of(c).filter(|f| f.kind() == "member_expression").and_then(|f| f.child_by_field_name("object"));
        if let (Some((op, dir)), Some(o)) = (traffic, object) {
            let path_function = |f: TsNode| self.eval.path_function_of(m, f);
            let kind = Crossing { world: World::Disk, dir: Some(*dir) };
            let cr = Crosses::read(c, src, kind, pkg.as_deref(), op, &literals(c, src, STRINGS), STRINGS, &HashMap::new(), &path_function);
            let t = Touch {
                what: text(o, src).to_string(),
                payload: cr.payload.clone(),
                op: op.to_string(),
                line: cr.line,
                ..Touch::new(World::Disk, Some(*dir))
            };
            // a handle the body opened is at the place its opening touches, named as the opening names it
            let at = At { m, scope: self.scope, params: &self.params };
            let (t, built) = match self.opening(o, World::Disk) {
                Some((o, built)) => (Touch { what: o.what, artifact: o.artifact, fixed: o.fixed, ..t }, built),
                None => (t, self.eval.of(at, o, 0)),
            };
            match built {
                built if built.is_empty() => self.read.touches.push(t),
                built => self.read.calls.push((t, built)),
            }
            return;
        }
        // some parts of the world are reached only through a client the package made, not its helpers
        let Some(kind) = effects::boundary_ts(pkg.as_deref(), parts)
            .filter(|k| !knowledge::THROUGH_A_CLIENT.contains(&k.world) || self.made.contains_key(&head))
        else {
            return;
        };
        let op = parts.last().cloned().unwrap_or_default();
        let path_function = |f: TsNode| self.eval.path_function_of(m, f);
        let cr = Crosses::read(c, src, kind, pkg.as_deref(), &op, &literals(c, src, STRINGS), STRINGS, &HashMap::new(), &path_function);
        let args = c.child_by_field_name("arguments").map(named_children).unwrap_or_default();
        let at = At { m, scope: self.scope, params: &self.params };
        // a socket's traffic is at the door's place, which the callers hand as they hand a path
        // (`docs/reference/mathematics.md`, "A door keeps its place"): the address the call that
        // opens the door is handed, the one the operation is handed (`send(msg, port, host)`), else
        // the address of the socket it runs on
        if socket && kind.world == World::Net {
            let object = function_of(c).filter(|f| f.kind() == "member_expression").and_then(|f| f.child_by_field_name("object"));
            // the package's own function that opens a door (`net.connect(..)`), not a method of one it made
            let imported = matches!(m.bindings.get(&head), Some(Bound::Ext(_)));
            let opens = imported && knowledge::NODE_DOORS.iter().any(|(p, f, _)| pkg.as_deref() == Some(*p) && *f == op);
            let handed = knowledge::NODE_SOCKET_ADDRESSES.iter().chain(knowledge::NODE_DOOR_PLACES).find(|(o, _)| *o == op);
            let door = match (opens, handed, object) {
                (true, _, _) | (false, None, None) => self.eval.of(at, c, 0),
                (false, Some((_, a)), _) => self.eval.address(at, &args, a, 0),
                (false, None, Some(o)) => self.eval.of(at, o, 0),
            };
            let t = Touch {
                what: text(object.filter(|_| !opens).unwrap_or(c), src).to_string(),
                payload: cr.payload.clone(),
                op: op.clone(),
                line: cr.line,
                ..Touch::new(World::Net, kind.dir)
            };
            // a door the body opened is at the place its opening touches, named as the opening names it
            let opened = object.filter(|_| !opens && handed.is_none()).and_then(|o| self.opening(o, World::Net));
            let (t, built) = match opened {
                Some((o, built)) => (Touch { what: o.what, artifact: o.artifact, fixed: o.fixed, ..t }, built),
                None => (t, crate::paths::request_of(&op, door)),
            };
            match built {
                built if built.is_empty() => self.read.touches.push(t),
                built => self.read.calls.push((t, built)),
            }
            return;
        }
        // a statement is read where it is whole: from a constant, a template, or what the callers
        // hand the parameters it is built from
        let statement = match (cr.facts.kind, args.first()) {
            (World::Db(db), Some(a)) => Some(self.eval.of(at, *a, 0)).filter(|b| crate::paths::is_statement(db, b)),
            _ => None,
        };
        if let Some(built) = statement {
            let t = Touch {
                what: cr.what.clone(),
                payload: cr.payload.clone(),
                op: op.clone(),
                line: cr.line,
                ..Touch::new(cr.facts.kind, None)
            };
            self.read.calls.push((t, built));
            return;
        }
        if !cr.facts.kind.located() || args.is_empty() {
            self.read.touches.extend(cr.touches(None));
            return;
        }
        for (kind, a) in sides(&op, &args, &cr.facts) {
            let t = Touch {
                what: text(a, src).to_string(),
                payload: cr.payload.clone(),
                op: op.clone(),
                artifact: cr.facts.artifact.clone(),
                fixed: cr.fixed,
                line: cr.line,
                ..Touch::new(kind.world, kind.dir)
            };
            let built = self.eval.of(at, a, 0);
            // a request is its method and its URL: `fetch(url, { method })`, `axios.post(url)`
            let built = if kind.world == World::Net {
                crate::paths::cross(&method_of_request(&op, &args, at, self.eval), &built, " ")
            } else {
                built
            };
            // a value the code holds none of (an option without a default) is the call as read
            if built.is_empty() {
                self.read.touches.push(t);
            } else {
                self.read.calls.push((t, built));
            }
        }
    }

    /// The names whose values the body changes in place, followed through the names they are
    /// changed by (`changed_through`). A call changes the value it is made on by what that value
    /// is: a method of the program's own when its body changes its own receiver, a method of the
    /// language's table on a value that holds an array, a `Map` or a `Set`. A method's name alone
    /// changes nothing (`http.delete(..)` on a library's client). A call of the program's own also
    /// changes each argument whose parameter its body changes (`addLine(lines, ..)`).
    pub(super) fn changed(&self) -> std::collections::BTreeSet<&'t str> {
        let changes = |c: TsNode<'t>| -> Vec<TsNode<'t>> {
            let Some(f) = function_of(c) else { return vec![] };
            let on = (f.kind() == "member_expression").then(|| f.child_by_field_name("object")).flatten();
            match self.callee(c).filter(|t| self.own(t)) {
                Some(t) => self.changed_parts(&t, on, c).into_iter().map(|(x, _)| x).collect(),
                None => {
                    let language = f
                        .child_by_field_name("property")
                        .filter(|_| f.kind() == "member_expression")
                        .map_or(false, |p| knowledge::mutates(knowledge::Registry::Npm, "javascript", text(p, self.src)));
                    on.filter(|o| language && self.holds_container(*o)).into_iter().collect()
                }
            }
        };
        changes_in(self.body, self.src, &changes)
            .into_iter()
            .filter_map(|(x, root)| changed_through(self.body, self.src, &BINDS, root, x.start_byte()))
            .collect()
    }

    /// What the body changes among what it is handed, each by its name with the type its change is
    /// drawn as: `this`, the value a method runs on (an initializer builds its object, and changes
    /// nothing it was handed by that), and each parameter whose type names what the picture draws
    /// (`inputs_of`).
    pub(super) fn changes_handed(&self) -> Vec<(String, String)> {
        self.changed()
            .into_iter()
            .filter_map(|r| {
                let ty = if r == "this" { self.cls.clone().filter(|_| !self.initializer) } else { self.inputs.get(r).cloned() };
                Some((r.to_string(), ty?))
            })
            .collect()
    }

    /// The parts of call `c` of the program's function `t` that it changes, each with the type the
    /// change is drawn as: the value it is called `on` where `t` changes its receiver, and each
    /// argument whose parameter `t` changes, in order (a rest parameter takes what is left; after a
    /// spread no argument says which parameter it fills).
    fn changed_parts(&self, t: &str, on: Option<TsNode<'t>>, c: TsNode<'t>) -> Vec<(TsNode<'t>, String)> {
        let changes = self.changing.get(t).map(Vec::as_slice).unwrap_or_default();
        let change = |name: &str| changes.iter().find(|(r, _)| r == name).map(|(_, ty)| ty.clone());
        let mut out: Vec<(TsNode<'t>, String)> = on.zip(change("this")).into_iter().collect();
        let params: Vec<&crate::model::Param> =
            self.store.nodes.get(t).map(|n| n.params.iter().filter(|p| !p.receiver).collect()).unwrap_or_default();
        let args = c.child_by_field_name("arguments").map(named_children).unwrap_or_default();
        let mut params = params.into_iter().peekable();
        for a in args.into_iter().filter(|a| a.kind() != "comment").take_while(|a| a.kind() != "spread_element") {
            let Some(p) = (match params.peek() {
                Some(p) if p.rest => Some(*p),
                _ => params.next(),
            }) else {
                break;
            };
            if let Some(ty) = change(&p.name) {
                out.push((a, ty));
            }
        }
        out
    }

    /// The values a call of the program's function `t` changes among what it is handed, each under
    /// the name it is handed by (`changed_parts`): the names the changed part holds, whose value it
    /// may be or be a part of, so that a change through a projection changes its owner
    /// (`fileAll(ledger.rows)` changes `ledger`), and `this` is the instance the body runs on. Each
    /// is of the type the body knows for it, else of the type the change is drawn as: the Array
    /// `addLine(lines, ..)` changes. A part the call makes anew (a literal, a spread, what a call
    /// gives) changes no value of the body.
    fn changed_values(&self, t: &str, on: Option<TsNode<'t>>, c: TsNode<'t>, kept: &HashSet<String>) -> Vec<Held> {
        let mut out: Vec<Held> = vec![];
        for (part, ty) in self.changed_parts(t, on, c) {
            for r in roots_of(part) {
                let mut held = vec![];
                if r.kind() != "this" {
                    self.held(r, kept, 0, &mut held);
                }
                if held.is_empty() {
                    held.push(Held::new(
                        if r.kind() == "this" { self.cls.clone().unwrap_or_default() } else { String::new() },
                        text(r, self.src),
                    ));
                }
                for mut h in held {
                    if h.ty.is_empty() {
                        h.ty = ty.clone();
                    }
                    h.at = r.start_byte() as u32;
                    if !out.contains(&h) {
                        out.push(h);
                    }
                }
            }
        }
        out
    }

    /// What the body does besides its calls: the connections the platform's own constructors
    /// open (`new WebSocket(url)`), the page a component renders, and the inputs it changes in
    /// place (`T -> T` for the receiver or a parameter; an initializer builds its object).
    fn ends(&mut self) {
        let src = self.src;
        let opened: Vec<Touch> = self
            .sites
            .iter()
            .filter(|c| c.kind() == "new_expression")
            .filter(|c| {
                self.resolve(&c.child_by_field_name("constructor").map(|x| member_parts(x, src)).unwrap_or_default())
                    .filter(|r| self.own(r))
                    .is_none()
            })
            .filter_map(|c| connects(*c, src, &self.m.bindings))
            .collect();
        self.read.touches.extend(opened);
        if self.is_module {
            return;
        }
        if let Some(jsx) = walk(self.body, &["jsx_element", "jsx_self_closing_element", "jsx_fragment"]).into_iter().next() {
            self.read.touches.push(Touch {
                op: "render".into(),
                line: jsx.start_position().row as u32 + 1,
                ..Touch::new(World::Screen, None)
            });
        }
        for (r, t) in self.changes_handed() {
            self.read.mutates.push((t, r));
        }
    }
}

/// The names in an expression that stand for values, and `this`: not the key of a lookup, which
/// chooses among the values of what it looks in (`table[k]` stands for `table`).
fn names_in(e: TsNode) -> Vec<TsNode> {
    walk(e, &["identifier", "shorthand_property_identifier", "this"])
        .into_iter()
        .filter(|i| {
            !std::iter::successors(Some(*i), |x| x.parent()).take_while(|x| x.id() != e.id()).any(|x| {
                x.parent()
                    .is_some_and(|p| p.kind() == "subscript_expression" && p.child_by_field_name("index").is_some_and(|k| k.id() == x.id()))
            })
        })
        .collect()
}

/// Whether a callee is read where it stands rather than called: a function written where it is
/// called (`(() => ..)()`), whose steps are the body's own, and `import(..)`, which binds a module.
fn read_in_place(f: TsNode) -> bool {
    matches!(beneath(f, WRAPPERS).kind(), "arrow_function" | "function_expression" | "function" | "import")
}

/// The name of a call no name path reaches, as the code writes it: a method on a value written out
/// is its name alone (`(..).trim`), any other callee its text (`EXPORTERS[format]`).
fn written_callee(f: TsNode, src: &[u8]) -> String {
    match f.child_by_field_name("property").filter(|_| f.kind() == "member_expression") {
        Some(p) => text(p, src).to_string(),
        None => text(f, src).split_whitespace().collect::<Vec<_>>().join(" "),
    }
}

/// Whether what a call gives back is used: anything but a call standing as a statement.
fn value_used(call: TsNode) -> bool {
    outermost(call, WRAPPERS).parent().map_or(false, |p| p.kind() != "expression_statement")
}

/// The HTTP method of a request: `fetch`'s `init.method` (GET when it has none), or the verb
/// of the call.
fn method_of_request<'t>(op: &str, args: &[TsNode<'t>], at: At<'_, 't>, eval: &Eval) -> Vec<crate::paths::Built> {
    if knowledge::HTTP_METHODS.contains(&op) {
        return vec![crate::paths::lit(&op.to_uppercase())];
    }
    let init = args.get(1).filter(|a| a.kind() == "object").and_then(|o| {
        named_children(*o).into_iter().find_map(|p| match p.kind() {
            "pair" if p.child_by_field_name("key").map(|k| text(k, &at.m.src)) == Some("method") => p.child_by_field_name("value"),
            "shorthand_property_identifier" if text(p, &at.m.src) == "method" => Some(p),
            _ => None,
        })
    });
    match init {
        Some(v) => eval.of(at, v, 0),
        None => vec![crate::paths::lit("GET")],
    }
}

/// The name a chain like `this.items[0].x` starts from.
pub(super) fn chain_root<'a>(n: TsNode, src: &'a [u8]) -> Option<&'a str> {
    root_of(n).map(|r| text(r, src))
}

/// What a part of a value TypeScript's own code computes is made of (`body::computed_of`): a literal, a piece of a string
/// and a comment of none, any other part the name it is a part of (`root_of`).
fn part(p: TsNode) -> Option<Vec<TsNode>> {
    match p.kind() {
        "number" | "string" | "string_fragment" | "escape_sequence" | "true" | "false" | "null" | "undefined" | "regex" | "comment" => {
            Some(vec![])
        }
        _ => root_of(p).map(|r| vec![r]),
    }
}

/// The name, or `this`, a chain like `this.items[0].x` starts from, as written.
fn root_of(n: TsNode) -> Option<TsNode> {
    match n.kind() {
        "identifier" | "this" => Some(n),
        "member_expression" | "subscript_expression" => root_of(n.child_by_field_name("object")?),
        k if WRAPPERS.contains(&k) => root_of(wrapped(n)?),
        _ => None,
    }
}

/// How TypeScript binds a name, as `changes` follows a change through it: `const x = v` and `x = v`
/// bind `x` to `v`, `for (const x of v)` to each element of `v`.
const BINDS: BindSyntax = BindSyntax {
    binds: |n, src| {
        let (l, v) = match n.kind() {
            "variable_declarator" => (n.child_by_field_name("name")?, n.child_by_field_name("value")?),
            "assignment_expression" => (n.child_by_field_name("left")?, n.child_by_field_name("right")?),
            _ => return None,
        };
        (l.kind() == "identifier").then(|| (text(l, src), roots_of(v).into_iter().map(|r| text(r, src)).collect()))
    },
    loops: |n, src| {
        let of = n.child_by_field_name("operator").map_or(false, |o| o.kind() == "of");
        let (l, v, b) = (n.child_by_field_name("left")?, n.child_by_field_name("right")?, n.child_by_field_name("body")?);
        (n.kind() == "for_in_statement" && of && l.kind() == "identifier").then(|| (text(l, src), chain_root(v, src), b))
    },
};

/// Each change a body makes in place: the node that makes it, and the name whose object it
/// changes, the root of every assignment to a member or an item (`this.x = ..`, `p.items[k] +=
/// ..`, `p.n++`, `delete p.k`), and of each value every call that changes something changes, as
/// `changes` says (`p.rows.push(..)`, `addLine(p.lines, ..)`). Changing a part changes the whole.
pub(super) fn changes_in<'a, 't>(
    body: TsNode<'t>,
    src: &'a [u8],
    changes: &dyn Fn(TsNode<'t>) -> Vec<TsNode<'t>>,
) -> Vec<(TsNode<'t>, &'a str)> {
    let member = |n: TsNode| matches!(n.kind(), "member_expression" | "subscript_expression");
    let mut out = vec![];
    for a in walk(body, &["assignment_expression", "augmented_assignment_expression"]) {
        out.extend(a.child_by_field_name("left").filter(|l| member(*l)).and_then(|l| chain_root(l, src)).map(|r| (a, r)));
    }
    for u in walk(body, &["update_expression", "unary_expression"]) {
        let deletes = u.kind() == "update_expression" || (0..u.child_count()).filter_map(|i| u.child(i)).any(|c| c.kind() == "delete");
        if deletes {
            out.extend(u.child_by_field_name("argument").filter(|a| member(*a)).and_then(|a| chain_root(a, src)).map(|r| (u, r)));
        }
    }
    for c in walk(body, &["call_expression"]) {
        out.extend(changes(c).into_iter().filter_map(|o| chain_root(o, src)).map(|r| (c, r)));
    }
    out
}

/// The names, as written, whose value a TypeScript value may be or be a part of: a name, `this`,
/// the name a projection starts from (`graph`, `this.items[0]`), either side of `a ?? b`, `a || b`
/// and `c ? a : b`. A call, a spread (`{ ...e }`) and a literal make a value anew.
fn roots_of(v: TsNode) -> Vec<TsNode> {
    match v.kind() {
        "binary_expression" if v.child_by_field_name("operator").map_or(false, |o| matches!(o.kind(), "??" | "||" | "&&")) => {
            [v.child_by_field_name("left"), v.child_by_field_name("right")].into_iter().flatten().flat_map(roots_of).collect()
        }
        "ternary_expression" => {
            [v.child_by_field_name("consequence"), v.child_by_field_name("alternative")].into_iter().flatten().flat_map(roots_of).collect()
        }
        k if WRAPPERS.contains(&k) => wrapped(v).into_iter().flat_map(roots_of).collect(),
        _ => root_of(v).into_iter().collect(),
    }
}

/// A constructor of the platform's own that connects, as the touch it makes: `new WebSocket(url)`.
/// A name the module imports is that import's, not the platform's.
fn connects(c: TsNode, src: &[u8], bindings: &HashMap<String, Bound>) -> Option<Touch> {
    let parts = c.child_by_field_name("constructor").map(|x| member_parts(x, src)).unwrap_or_default();
    let [name] = parts.as_slice() else { return None };
    if bindings.contains_key(name) {
        return None;
    }
    let kind = effects::boundary_ts(None, &parts)?;
    let kinds = &["string", "template_string"];
    let f = effects::facts(kind, None, name, &literals(c, src, kinds));
    Some(Touch {
        what: what_of(c, src, kinds, "arguments"),
        op: name.clone(),
        artifact: f.artifact,
        line: c.start_position().row as u32 + 1,
        ..Touch::new(f.kind, f.dir)
    })
}

/// The paths a file call is handed, each with the way it goes: a copy or a move reads its first and
/// writes its second; any other call goes its own way with its first.
fn sides<'t>(op: &str, args: &[TsNode<'t>], f: &effects::Facts) -> Vec<(Crossing, TsNode<'t>)> {
    match crate::body::sides(op, args.len()) {
        Some([from, to]) => vec![(from, args[0]), (to, args[1])],
        None => vec![(Crossing { world: f.kind, dir: if f.kind == World::Disk { f.dir } else { None } }, args[0])],
    }
}

/// Which parameter of the function a call reaches each argument is handed to, in order.
#[allow(clippy::too_many_arguments)]
pub(super) fn argument<'t>(
    modules: &'t IndexMap<String, Module>,
    store: &GraphBuilder,
    caller: &str,
    callee: &str,
    c: TsNode<'t>,
    m: &'t Module,
    scope: Option<TsNode<'t>>,
    params: &[String],
) -> Option<Arg<'t>> {
    let cn = store.nodes.get(callee)?;
    let cm = modules.get(&cn.module)?;
    let (s, e) = *cm.bodies.get(callee)?;
    let at = cm.tree.root_node().descendant_for_byte_range(s, e)?;
    // a method an interface or an abstract class only declares is its signature
    let def = if matches!(at.kind(), "method_signature" | "abstract_method_signature") { at } else { paths::function_node(at)? };
    let names = paths::param_names(def, &cm.src);
    let passed = named_children(c.child_by_field_name("arguments")?)
        .into_iter()
        .take_while(|a| a.kind() != "spread_element")
        .zip(names)
        .filter(|(_, n)| !n.is_empty())
        .map(|(a, n)| (n, a))
        .collect();
    Some(Arg {
        caller: caller.to_string(),
        callee: callee.to_string(),
        module: m.id.clone(),
        scope,
        params: params.to_vec(),
        passed,
        on: None,
    })
}

/// Where TypeScript's values go: `const x = v`, `x = v`, `x += v`, `for (const x of v)`; `return`
/// and `yield` leave; `throw` fails.
const USES: UseSyntax = UseSyntax {
    binds: |n| match n.kind() {
        "variable_declarator" => n.child_by_field_name("name").zip(n.child_by_field_name("value")),
        "assignment_expression" | "augmented_assignment_expression" | "for_in_statement" => {
            n.child_by_field_name("left").zip(n.child_by_field_name("right"))
        }
        _ => None,
    },
    leaves: |n, _| matches!(n.kind(), "return_statement" | "yield_expression"),
    fails: |n, _| n.kind() == "throw_statement",
    hands: |n, child| {
        matches!(n.kind(), "call_expression" | "new_expression") && n.child_by_field_name("arguments").map(|a| a.id()) == Some(child.id())
    },
    changes: |_, _| vec![],
    splits: |n, child| n.kind() == "switch_statement" && n.child_by_field_name("value").map(|v| v.id()) == Some(child.id()),
    names: &["identifier", "shorthand_property_identifier"],
    keeps: &["member_expression", "subscript_expression"],
    field: |n, src| {
        (n.kind() == "member_expression")
            .then(|| Some((n.child_by_field_name("object")?, text(n.child_by_field_name("property")?, src).to_string())))
            .flatten()
    },
    pairs: |targets, value| targets.kind() == "array_pattern" && value.kind() == "array",
    // `??`, `||` and `&&` are choices; `void` and `delete` give no value of their operand
    computes: |n, src| match n.kind() {
        "binary_expression" => n.child_by_field_name("operator").is_some_and(|o| !matches!(text(o, src), "??" | "||" | "&&")),
        "unary_expression" => n.child_by_field_name("operator").is_some_and(|o| !matches!(text(o, src), "void" | "delete")),
        "array" | "template_string" | "template_substitution" => true,
        _ => false,
    },
    wrappers: WRAPPERS,
    drops: |n, child, src| match n.kind() {
        "expression_statement" => n.named_child(0).map(|c| c.id()) == Some(child.id()),
        "unary_expression" => {
            n.child_by_field_name("operator").is_some_and(|o| text(o, src) == "void")
                && n.child_by_field_name("argument").map(|a| a.id()) == Some(child.id())
        }
        _ => false,
    },
    nested: &[
        "function_declaration",
        "generator_function_declaration",
        "class_declaration",
        "abstract_class_declaration",
        "method_definition",
        "interface_declaration",
        "type_alias_declaration",
        "enum_declaration",
        "import_statement",
    ],
    reads: &["this"],
    label: |_, _| false,
    binders: |n, child| {
        let is = |f: &str| n.child_by_field_name(f).is_some_and(|x| x.id() == child.id());
        match n.kind() {
            "arrow_function" | "function_expression" | "function" if is("parameters") || is("parameter") => Some(true),
            "catch_clause" if is("parameter") => Some(false),
            _ => None,
        }
    },
    updates: &["augmented_assignment_expression"],
    parts: |_, _| false,
    raises: |n, child, _| n.kind() == "throw_statement" && n.named_child(0).map(|c| c.id()) == Some(child.id()),
    errs: |_, _, _| false,
    tails: &[],
    propagates: None,
    // a `catch` catches whatever its `try` throws
    catches: |n, child, _| {
        (n.kind() == "try_statement"
            && n.child_by_field_name("body").map(|b| b.id()) == Some(child.id())
            && n.child_by_field_name("handler").is_some())
        .then(Vec::new)
    },
    args,
};

/// The arguments a TypeScript call writes, each with the parameter it fills (`body::placed`): the value
/// a method is called on (`x` of `x.f(..)`), each argument of a call or a `new` by its place up to a
/// `...xs`, and the array `C.prototype.f.apply(v, xs)` spreads; the properties an object literal of a
/// type of the program's writes by their names, and what its `...base` spreads. A component an element
/// renders is handed its attributes as the one value it takes, its props.
fn args(n: TsNode, src: &[u8]) -> Vec<(Range, Slot)> {
    let range = |n: TsNode| (n.start_byte(), n.end_byte());
    match n.kind() {
        "call_expression" | "new_expression" => {
            let f = n.child_by_field_name(if n.kind() == "new_expression" { "constructor" } else { "function" });
            let method = f.filter(|f| f.kind() == "member_expression" && n.kind() == "call_expression");
            let mut out: Vec<(Range, Slot)> =
                method.and_then(|f| f.child_by_field_name("object")).map(|o| (range(o), Slot::Receiver)).into_iter().collect();
            let Some(a) = n.child_by_field_name("arguments") else { return out };
            // a tagged template is the one argument of its tag
            if a.kind() != "arguments" {
                out.push((range(a), Slot::At(0)));
                return out;
            }
            let applied = method.is_some_and(|f| {
                f.child_by_field_name("property").is_some_and(|p| text(p, src) == "apply")
                    && f.child_by_field_name("object").is_some_and(|o| text(o, src).contains(".prototype."))
            });
            let mut placed = crate::body::placed(
                named_children(a).into_iter().filter(|x| x.kind() != "comment"),
                |x| (x.kind() == "spread_element").then_some(Slot::Spread(0)),
                |_| None,
            );
            for (_, s) in placed.iter_mut().filter(|(_, s)| applied && *s == Slot::At(1)) {
                *s = Slot::Spread(1);
            }
            out.extend(placed);
            out
        }
        "object" => named_children(n)
            .into_iter()
            .filter_map(|p| match p.kind() {
                "pair" => {
                    let key = p.child_by_field_name("key")?;
                    let name = if key.kind() == "string" { unquote(text(key, src)) } else { text(key, src).to_string() };
                    Some((range(p.child_by_field_name("value")?), Slot::Name(name)))
                }
                "shorthand_property_identifier" => Some((range(p), Slot::Name(text(p, src).to_string()))),
                "spread_element" => Some((range(p), Slot::Keywords)),
                _ => None,
            })
            .collect(),
        "jsx_opening_element" | "jsx_self_closing_element" => named_children(n)
            .into_iter()
            .filter(|a| matches!(a.kind(), "jsx_attribute" | "jsx_expression"))
            .map(|a| (range(a), Slot::At(0)))
            .collect(),
        _ => vec![],
    }
}

/// TypeScript's choices: `if`/`else if`/`else`, `a ? b : c`, `switch`, `try`/`catch`, and `x ?? y`, `x || y`
/// and `x && y`, a choice of one arm.
const CHOICES: ChoiceSyntax = ChoiceSyntax {
    chooses: |n, src| match n.kind() {
        "if_statement" | "ternary_expression" | "switch_body" | "try_statement" => true,
        "binary_expression" => n.child_by_field_name("operator").map_or(false, |o| matches!(text(o, src), "||" | "??" | "&&")),
        _ => false,
    },
    continues: |n| {
        let up = n.parent().filter(|p| p.kind() == "else_clause")?.parent()?;
        (n.kind() == "if_statement" && up.kind() == "if_statement").then_some(up)
    },
    part: |c, child, _| {
        let is = |f: &str| c.child_by_field_name(f).map(|x| x.id()) == Some(child.id());
        let depth = || {
            let mut d = 0;
            let mut cur = c;
            while let Some(up) =
                cur.parent().filter(|p| p.kind() == "else_clause").and_then(|e| e.parent()).filter(|p| p.kind() == "if_statement")
            {
                d += 1;
                cur = up;
            }
            d
        };
        match c.kind() {
            "if_statement" if is("condition") => Part::Decides,
            "if_statement" if is("consequence") => Part::Arm(depth()),
            "if_statement" if is("alternative") => {
                if named_children(child).iter().any(|x| x.kind() == "if_statement") {
                    Part::Through
                } else {
                    Part::Arm(depth() + 1)
                }
            }
            "ternary_expression" if is("condition") => Part::Decides,
            "ternary_expression" if is("consequence") => Part::Arm(0),
            "ternary_expression" if is("alternative") => Part::Arm(1),
            "switch_body" if child.kind() == "switch_case" => Part::Arm(nth_of_kind(c, child)),
            // `default` comes after every case
            "switch_body" if child.kind() == "switch_default" => {
                Part::Arm(named_children(c).iter().filter(|x| x.kind() == "switch_case").count() as u32)
            }
            "try_statement" if is("body") => Part::Arm(0),
            "try_statement" if is("handler") => Part::Arm(1),
            // `x ?? y`, `x || y`, `x && y`: `x` always runs and its value goes on where it holds, so it is no arm; `y` is
            // the one arm, run only sometimes
            "binary_expression" if is("right") => Part::Arm(0),
            _ => Part::Through,
        }
    },
    // an `if` without an `else` whose every arm returns, throws, continues or breaks
    guards: |n, _| {
        let exits = |s: Option<TsNode>| {
            let last = s.and_then(|s| match s.kind() {
                "statement_block" => named_children(s).into_iter().filter(|x| x.kind() != "comment").last(),
                _ => Some(s),
            });
            last.is_some_and(|l| matches!(l.kind(), "return_statement" | "throw_statement" | "continue_statement" | "break_statement"))
        };
        if n.kind() != "if_statement" || n.parent().is_some_and(|p| p.kind() == "else_clause") {
            return false;
        }
        let mut cur = n;
        loop {
            if !exits(cur.child_by_field_name("consequence")) {
                return false;
            }
            match cur.child_by_field_name("alternative") {
                None => return true,
                Some(e) => match named_children(e).into_iter().find(|x| x.kind() == "if_statement") {
                    Some(i) => cur = i,
                    None => return false,
                },
            }
        }
    },
};

/// TypeScript's comparisons: `===`, `==`, `!==` and `!=` against a string or a member of an
/// enumeration (`Mode.Fast`), and a `switch` over them.
const COMPARES: CompareSyntax = CompareSyntax {
    kinds: &["binary_expression", "switch_statement"],
    operands: tested,
    each: |n| vec![n],
    literal: |n, src| (n.kind() == "string").then(|| unquote(text(n, src))),
    member: |n| {
        (n.kind() == "member_expression")
            .then(|| n.child_by_field_name("object"))
            .flatten()
            .filter(|o| matches!(o.kind(), "identifier" | "member_expression"))
            .map(|o| (o.start_byte(), o.end_byte()))
    },
    subject: |n| matches!(n.kind(), "identifier" | "member_expression"),
    over: switched,
    copies: |n| match n.kind() {
        "variable_declarator" => n.child_by_field_name("name").zip(n.child_by_field_name("value")),
        "assignment_expression" => n.child_by_field_name("left").zip(n.child_by_field_name("right")),
        _ => None,
    },
    place: |n| matches!(n.kind(), "identifier" | "this" | "member_expression" | "subscript_expression").then_some(n),
    binds: USES.binds,
};

/// The two operands of a test of equality.
fn tested<'t>(n: TsNode<'t>, src: &[u8]) -> Option<(TsNode<'t>, TsNode<'t>)> {
    let op = n.child_by_field_name("operator").map(|o| text(o, src)).unwrap_or("");
    matches!(op, "===" | "==" | "!==" | "!=").then_some((n.child_by_field_name("left")?, n.child_by_field_name("right")?))
}

/// A `switch`: its value, the values of its cases, and the body that holds them.
fn switched<'t>(n: TsNode<'t>, src: &[u8]) -> Option<(String, Vec<TsNode<'t>>, Option<TsNode<'t>>)> {
    let (Some(value), Some(body)) = (n.child_by_field_name("value"), n.child_by_field_name("body")) else { return None };
    let cases = named_children(body).into_iter().filter_map(|c| c.child_by_field_name("value")).collect();
    Some((text(value, src).trim_start_matches('(').trim_end_matches(')').to_string(), cases, Some(body)))
}
