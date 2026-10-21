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
use crate::body::{changed_through, BindSyntax, Crosses, Step, Steps, UseSyntax};
use crate::builder::{foreign_type, GraphBuilder};
use crate::effects::{self, Crossing};
use crate::knowledge;
use crate::knowledge::OwnTypes;
use crate::model::{Bucket, EdgeKind, Held, Node, NodeKind, Touch, World};
use crate::tree::{
    beneath, evaluation_order, literals, named_children, nth_of_kind, outermost, text, unquote, walk, what_of, wrapped, ChoiceSyntax,
    CompareSyntax, Part,
};

pub(super) const OWN: OwnTypes = knowledge::TYPESCRIPT_OWN_TYPES;

/// The package a client was made by: `new Anthropic()`, `new Pool()`, `axios.create()`,
/// `createClient()`. What a package's other calls give back is a result, not a client.
fn made_by_package(value: TsNode, m: &Module, src: &[u8]) -> Option<String> {
    let v = beneath(value, WRAPPERS);
    let maker = match v.kind() {
        "new_expression" => v.child_by_field_name("constructor")?,
        "call_expression" => v.child_by_field_name("function").filter(|f| {
            let last = member_parts(*f, src).pop().unwrap_or_default();
            last.starts_with("create") || matches!(last.as_str(), "connect" | "open")
        })?,
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
            _ if (CHOICES.chooses)(p, src) && matches!((CHOICES.part)(p, n, src), Part::Arm(_)) => n = p,
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
    resolve(home, home.values.get(&local)?, modules, nodes).filter(|r| nodes.get(r).map_or(false, |x| x.kind == NodeKind::Class))
}

/// Inside a module's body, what belongs to the module and not to a function defined in it.
fn module_level(n: TsNode, root: TsNode, bodies: &HashMap<String, (usize, usize)>) -> bool {
    let inside = bodies.values().any(|(s, e)| *s <= n.start_byte() && n.end_byte() <= *e);
    !inside && n.start_byte() >= root.start_byte()
}

pub(super) fn link_calls<'t>(
    m: &'t Module,
    modules: &'t IndexMap<String, Module>,
    store: &mut GraphBuilder,
    id: &str,
    returned: &HashMap<String, String>,
    eval: &Eval,
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
        let mut b = Body::new(m, modules, store, &n, body, returned, eval);
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
            steps.mark(c, b.read.touches[t..].iter_mut().chain(b.read.calls[w..].iter_mut().map(|(t, _)| t)));
        }
        b.class_state(&mut steps);
        b.ends();
        b.read
    };
    let compares = (n.kind != NodeKind::Module).then(|| {
        // a qualifier is a type when it names an `enum` or a class of the program
        let is_type = |(a, b): (usize, usize)| {
            let parts: Vec<String> = String::from_utf8_lossy(&m.src[a..b]).split('.').map(str::to_string).collect();
            resolve(m, &parts, modules, &store.nodes).and_then(|t| store.nodes.get(&t)).is_some_and(|t| t.kind == NodeKind::Class)
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
        if t.starts_with("ext:") {
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
    /// The classes the body's names hold: its typed parameters, and locals declared with a type,
    /// built with `new`, or handed on by a function.
    pub(super) locals: HashMap<String, String>,
    /// What a change to each typed parameter changes (`inputs_of`).
    pub(super) inputs: HashMap<String, String>,
    /// The values a package made, by the package: a call on one is the package's.
    pub(super) made: HashMap<String, String>,
    /// The names that hold one of the language's own containers, as the body declares or builds
    /// them: `rows: Row[]`, `const seen = new Set<string>()`, `const out = []`.
    pub(super) containers: HashSet<String>,
    /// The calls, constructors, elements and names of the body, in the order they run.
    pub(super) sites: Vec<TsNode<'t>>,
    pub(super) read: Read<'t>,
}

const STRINGS: &[&str] = &["string", "template_string"];

impl<'a, 't> Body<'a, 't> {
    pub(super) fn new(
        m: &'t Module,
        modules: &'t IndexMap<String, Module>,
        store: &'a GraphBuilder,
        n: &Node,
        body: TsNode<'t>,
        returned: &'a HashMap<String, String>,
        eval: &'a Eval<'a>,
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
            locals: HashMap::new(),
            inputs: HashMap::new(),
            made: HashMap::new(),
            containers: HashSet::new(),
            sites: vec![],
            read: Read::default(),
        };
        b.typed(n);
        b.made();
        b.sites = evaluation_order(walk(
            body,
            &["call_expression", "new_expression", "object", "jsx_opening_element", "jsx_self_closing_element", "identifier"],
        ))
        .into_iter()
        .filter(|x| b.mine(*x))
        .collect();
        b
    }

    /// Whether a node is the body's own: inside a module's own code, not in a function it defines.
    fn mine(&self, x: TsNode) -> bool {
        !self.is_module || module_level(x, self.root, &self.m.bodies)
    }

    fn resolve(&self, parts: &[String]) -> Option<String> {
        resolve(self.m, parts, self.modules, &self.store.nodes)
    }

    fn class(&self, r: Option<String>) -> Option<String> {
        r.filter(|r| self.store.nodes.get(r).map_or(false, |x| x.kind == NodeKind::Class))
    }

    /// The values a class keeps that the body changes by assignment: `Counter.total += 1`,
    /// `Registry.handlers[name] = fn`. A static of a class is no place Atlas draws, so what the
    /// change leaves behind it does not see.
    fn class_state(&self, steps: &mut Steps) {
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
        !r.starts_with("ext:") && self.store.nodes.contains_key(r)
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
                if r.starts_with("ext:") {
                    foreign_type(r, &OWN)
                } else {
                    self.class(Some(r.clone())).is_some()
                }
            });
            let written = text(ty, src).trim_start_matches(':').trim();
            let container = container_type(written, self.m).then(|| {
                let head = written.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$')).next().unwrap_or("");
                format!("ext:javascript.{}", if knowledge::TYPESCRIPT_CHANGEABLE.contains(&head) { head } else { "Array" })
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
            "identifier" => self.containers.contains(text(v, src)),
            "member_expression" => {
                let (Some(o), Some(field)) = (v.child_by_field_name("object"), v.child_by_field_name("property")) else { return false };
                // the class of the value the field is read from: the body's own, a name's, a field's of the body's own
                let class = match o.kind() {
                    "this" => self.cls.clone(),
                    "identifier" => self.locals.get(text(o, src)).cloned(),
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
        // the types the body's names are written with: its parameters' and its typed locals'
        let mut written: HashMap<String, String> = n.params.iter().map(|p| (p.name.clone(), p.ty.clone())).collect();
        for p in n.params.iter().filter(|p| !p.ty.is_empty()) {
            if container_type(&p.ty, self.m) {
                self.containers.insert(p.name.clone());
            }
            let ty = p.ty.split(['<', '[', ' ', '|']).next().unwrap_or("").to_string();
            if let Some(r) = self.class(self.resolve(&[ty])) {
                self.locals.insert(p.name.clone(), r);
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
                self.containers.insert(text(name, src).to_string());
            }
            if let Some(t) = ty {
                written.insert(text(name, src).to_string(), t.to_string());
            }
            let declared = v.child_by_field_name("type").and_then(|t| named_children(t).into_iter().next()).map(|t| member_parts(t, src));
            let built = (value.kind() == "new_expression")
                .then(|| value.child_by_field_name("constructor").map(|c| member_parts(c, src)))
                .flatten();
            let typed = self.class(declared.or(built).and_then(|t| self.resolve(&t)));
            // `const p = await getProvider()`: what the function hands on
            let handed = || {
                let call = walk(value, &["call_expression"]).into_iter().next()?;
                let f = self.resolve(&member_parts(function_of(call)?, src))?;
                self.returned.get(&f).cloned()
            };
            if let Some(r) = typed.or_else(handed) {
                self.locals.insert(text(name, src).to_string(), r);
            }
        }
        // `for (const r of rows)`: an element of the container the loop goes over, as its type is
        // written (`rows: Row[]`)
        let loops: Vec<TsNode> = walk(self.body, &["for_in_statement"]).into_iter().filter(|l| self.mine(*l)).collect();
        for l in loops {
            let (Some(left), Some(right)) = (l.child_by_field_name("left"), l.child_by_field_name("right")) else { continue };
            let of = l.child_by_field_name("operator").map_or(false, |o| text(o, src) == "of");
            if !of || left.kind() != "identifier" {
                continue;
            }
            // over a field of a value of the program's class (`g.nodes`): the class the field's type holds
            if right.kind() == "member_expression" {
                let (Some(obj), Some(prop)) = (right.child_by_field_name("object"), right.child_by_field_name("property")) else {
                    continue;
                };
                let Some(owner) = self.locals.get(text(obj, src)).cloned() else { continue };
                let types = self
                    .store
                    .classes
                    .get(&owner)
                    .and_then(|k| k.fields.iter().find(|(f, _)| f == text(prop, src)).map(|(_, t)| t.clone()));
                if let Some(r) = types.unwrap_or_default().into_iter().find_map(|t| self.class(self.resolve(&t))) {
                    self.locals.insert(text(left, src).to_string(), r);
                }
                continue;
            }
            // over a name of the body, or one its module declares (`const STATIONS: Station[]`), as its type is written
            let declared = || {
                walk(self.root, &["variable_declarator"])
                    .into_iter()
                    .filter(|v| module_level(*v, self.root, &self.m.bodies))
                    .find(|v| v.child_by_field_name("name").map(|n| text(n, src)) == Some(text(right, src)))
                    .and_then(|v| v.child_by_field_name("type"))
                    .map(|t| text(t, src).trim_start_matches(':').trim().to_string())
            };
            let Some(ty) = written.get(text(right, src)).cloned().or_else(declared).filter(|t| container_type(t, self.m)) else {
                continue;
            };
            let element = ty
                .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
                .filter(|w| !w.is_empty() && !knowledge::TYPESCRIPT_CHANGEABLE.contains(w))
                .find_map(|w| self.class(self.resolve(&[w.to_string()])));
            if let Some(r) = element {
                self.locals.insert(text(left, src).to_string(), r);
            }
        }
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
        let typed: Vec<(String, String)> = self.locals.iter().map(|(n, k)| (n.clone(), k.clone())).collect();
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
    fn each(&mut self, steps: &mut Steps, l: TsNode<'t>) {
        let (Some(left), Some(right)) = (l.child_by_field_name("left"), l.child_by_field_name("right")) else { return };
        let names: Vec<String> = walk(left, &["identifier"]).into_iter().map(|i| text(i, self.src).to_string()).collect();
        let hands = self.hands(&[Some(right)], &steps.kept);
        let gives: Vec<Held> = names
            .iter()
            .filter_map(|n| match self.locals.get(n) {
                Some(t) => Some(Held::new(t.clone(), n.clone())),
                None => (!hands.is_empty()).then(|| Held::new(String::new(), n.clone())),
            })
            .collect();
        // the step stands where what it goes over is written: a call there hands it what it gives, the loop's body does not
        steps.op(right, &format!("for {} of {}", text(left, self.src), text(right, self.src)), hands, gives);
    }

    /// What a call is handed: the value it is called on, its arguments, an element's attributes.
    fn hands(&self, parts: &[Option<TsNode>], kept: &HashSet<String>) -> Vec<Held> {
        let ty_of = |name: &str| self.locals.get(name).cloned().or_else(|| value_type(self.m, name, self.modules, &self.store.nodes));
        let mut out: Vec<Held> = vec![];
        for p in parts.iter().flatten() {
            for h in crate::tree::held_names(*p, self.src, &[], &ty_of, kept) {
                if !out.contains(&h) {
                    out.push(h);
                }
            }
        }
        out
    }

    /// What a call gives back, by the name the body keeps it by. Its type is one a step gives: what
    /// the program's function `to` hands on, as the name it is kept by is declared or as the
    /// function says. A call Atlas does not follow gives no type the program names: a cast or an
    /// annotation on its value (`JSON.parse(..) as Config`) is asserted, not given.
    fn gives(&self, c: TsNode, to: Option<&str>) -> Vec<Held> {
        let ty =
            |name: &String| to.and_then(|t| self.locals.get(name).cloned().or_else(|| self.returned.get(t).cloned())).unwrap_or_default();
        kept_as(c, self.src).map(|name| Held::new(ty(&name), name)).into_iter().collect()
    }

    /// The values the body keeps of what a parse of the command line gives: the name it is kept
    /// by, or the names a pattern takes out of it (`const { values } = parseArgs(..)`).
    fn parsed_into(&self, c: TsNode) -> Vec<Held> {
        let v = outermost(c, WRAPPERS);
        let pattern = v
            .parent()
            .filter(|d| d.kind() == "variable_declarator" && d.child_by_field_name("value").map(|x| x.id()) == Some(v.id()))
            .and_then(|d| d.child_by_field_name("name"));
        match pattern.filter(|p| p.kind() == "object_pattern") {
            Some(p) => named_children(p)
                .into_iter()
                .filter_map(|x| match x.kind() {
                    "shorthand_property_identifier_pattern" => Some(x),
                    "pair_pattern" => x.child_by_field_name("value").filter(|v| v.kind() == "identifier"),
                    _ => None,
                })
                .map(|x| Held::new("", text(x, self.src)))
                .collect(),
            None => self.gives(c, None),
        }
    }

    /// One site of the body, as the step it is.
    fn call(&mut self, steps: &mut Steps, c: TsNode<'t>) {
        match c.kind() {
            "new_expression" => self.constructed(steps, c),
            "object" => self.literal(steps, c),
            "jsx_opening_element" | "jsx_self_closing_element" => self.rendered(steps, c),
            "identifier" => self.handed_on(steps, c),
            _ => self.called(steps, c),
        }
    }

    /// `new T(..)` of one of the program's classes, handing its constructor its arguments.
    fn constructed(&mut self, steps: &mut Steps, c: TsNode<'t>) {
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
        }
    }

    /// An object literal a type of the program's is written for, which the compiler checks it
    /// against, makes a value of that type, as `new` does: one declared as the type (`const
    /// settings: Settings = { .. }`), and one a function gives back where its return type names the
    /// type (`return { .. }` in `parseLine(): Month`, `(): Month => ({ .. })`). A step to the type,
    /// handed what the literal is built from.
    fn literal(&self, steps: &mut Steps, c: TsNode<'t>) {
        let v = outermost(c, WRAPPERS);
        let is = |p: TsNode, field: &str| p.child_by_field_name(field).map(|x| x.id()) == Some(v.id());
        let Some(p) = v.parent() else { return };
        let written = match p.kind() {
            "variable_declarator" if is(p, "value") => p.child_by_field_name("type"),
            // the function the value leaves: the nearest around the `return`, or an arrow's body
            "return_statement" => std::iter::successors(p.parent(), |n| n.parent())
                .find(|n| paths::function_node(*n).map_or(false, |f| f.id() == n.id()))
                .and_then(|f| f.child_by_field_name("return_type")),
            "arrow_function" if is(p, "body") => p.child_by_field_name("return_type"),
            _ => None,
        };
        let Some(ty) = written.and_then(|t| named_children(t).into_iter().next()).map(|t| promised(t, self.src)) else { return };
        if let Some(r) = self.class(self.resolve(&type_head(ty, self.src))).filter(|r| self.own(r)) {
            let step = Step { hands: self.hands(&[Some(c)], &steps.kept), gives: self.gives(c, Some(&r)), ..Default::default() };
            steps.call(c, &r, EdgeKind::Instantiates, None, step);
        }
    }

    /// `<Kaart/>` renders a component, handed its attributes; a lowercase name is an element of
    /// the page.
    fn rendered(&self, steps: &mut Steps, c: TsNode<'t>) {
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
    fn handed_on(&self, steps: &mut Steps, c: TsNode<'t>) {
        let handed = c.parent().map_or(false, |p| match p.kind() {
            "arguments" | "array" | "jsx_expression" | "return_statement" => true,
            "pair" | "variable_declarator" => p.child_by_field_name("value").map(|v| v.id()) == Some(c.id()),
            _ => false,
        });
        let fun = |r: &String| matches!(self.store.nodes[r].kind, NodeKind::Function | NodeKind::Method);
        if let Some(r) = self.resolve(&[text(c, self.src).to_string()]).filter(|r| handed && self.own(r) && fun(r)) {
            steps.handed(c, c, &r, EdgeKind::Calls);
        }
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
            "identifier" => {
                self.locals.get(text(o, src)).cloned().or_else(|| value_type(self.m, text(o, src), self.modules, &self.store.nodes))
            }
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
    fn through_prototype(&mut self, steps: &mut Steps, c: TsNode<'t>, parts: &[String]) -> bool {
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
        let step = Step { on, on_receiver, named: true, hands, gives: self.gives(c, Some(&t)), ..Default::default() };
        steps.call(c, &t, EdgeKind::Calls, None, step);
        true
    }

    /// `f(..)` or `x.m(..)`: the program's function or method, found on the class of the value it
    /// is called on, or a call Atlas does not follow: a library's operation, which may cross into
    /// the world, or a call through a value the body cannot name.
    fn called(&mut self, steps: &mut Steps, c: TsNode<'t>) {
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
        let hands = self.hands(&[object, c.child_by_field_name("arguments")], &steps.kept);
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
                let on = class
                    .filter(|k| !on_receiver && self.store.nodes[&t].cls.as_deref().map_or(false, |b| b != k.as_str()))
                    .unwrap_or_default();
                let kind = if self.store.nodes[&t].kind == NodeKind::Class { EdgeKind::Instantiates } else { EdgeKind::Calls };
                if kind == EdgeKind::Calls && value_used(c) {
                    self.read.used.push(t.clone());
                }
                let step = Step { on, on_receiver, named, after, hands, gives: self.gives(c, Some(&t)), ..Default::default() };
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
                self.unread(steps, c, &name, bucket, hands, vec![]);
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
                // a library's operation by its name, a call Atlas cannot follow as written
                let name = if matches!(bucket, Bucket::Library | Bucket::Builtin) {
                    parts.last().cloned().unwrap_or_default()
                } else {
                    text(f, src).to_string()
                };
                let parsed = self.eval.command_line(At { m: self.m, scope: self.scope, params: &self.params }, c);
                // the language's own, a change and a parse of the command line are what the tables say
                let known = parsed.is_some() || bucket == Bucket::Builtin;
                match parsed {
                    Some(options) => steps.parses(c, &name, hands, self.parsed_into(c), options),
                    None => self.unread(steps, c, &name, bucket, hands, changed),
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

    /// What a call's name path starts from is bound to; a field that holds what a package made is
    /// that package's, as a local is.
    /// A call the body reads no further, as a step: a library's operation where the body keeps what
    /// it gives or what it `changed`, any other call Atlas cannot follow always, since it runs all
    /// the same.
    fn unread(&self, steps: &mut Steps, c: TsNode<'t>, name: &str, bucket: Bucket, hands: Vec<Held>, changed: Vec<Held>) {
        if !changed.is_empty() {
            steps.changes(c, name, hands, self.gives(c, None), changed);
        } else if matches!(bucket, Bucket::Library | Bucket::Builtin) {
            steps.op(c, name, hands, self.gives(c, None));
        } else {
            steps.not_followed(c, name, hands, self.gives(c, None));
        }
    }

    /// The container a method of the language's own changes, as the value the call gives: `rows`
    /// for `rows.push(row)`, where the body holds an array, a `Map` or a `Set` by that name.
    fn changes(&self, c: TsNode) -> Vec<Held> {
        let Some(f) = function_of(c).filter(|f| f.kind() == "member_expression") else { return vec![] };
        let (Some(o), Some(p)) = (f.child_by_field_name("object"), f.child_by_field_name("property")) else { return vec![] };
        let name = text(o, self.src);
        if o.kind() != "identifier"
            || !(self.containers.contains(name) || self.narrowed(o))
            || !knowledge::mutates(knowledge::Registry::Npm, "javascript", text(p, self.src))
        {
            return vec![];
        }
        let ty = self.locals.get(name).cloned().unwrap_or_default();
        vec![Held { ty, name: name.to_string(), at: o.start_byte() as u32 }]
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
            None => Bucket::Unknown,
        }
    }

    /// The names the body binds: its parameters, each variable it declares, the parameters of
    /// the functions in it, a caught error, a loop's variable, and the functions it declares.
    pub(super) fn bound_in(&self) -> HashSet<String> {
        fn pattern(p: TsNode, src: &[u8], out: &mut HashSet<String>) {
            match p.kind() {
                "identifier" | "shorthand_property_identifier_pattern" => {
                    out.insert(text(p, src).to_string());
                }
                // a default's value binds nothing
                "assignment_pattern" | "object_assignment_pattern" => {
                    p.child_by_field_name("left").into_iter().for_each(|l| pattern(l, src, out))
                }
                "pair_pattern" => p.child_by_field_name("value").into_iter().for_each(|v| pattern(v, src, out)),
                "object_pattern" | "array_pattern" | "rest_pattern" => named_children(p).into_iter().for_each(|c| pattern(c, src, out)),
                _ => {}
            }
        }
        let fields: &[(&str, &str)] = &[
            ("variable_declarator", "name"),
            ("required_parameter", "pattern"),
            ("optional_parameter", "pattern"),
            ("arrow_function", "parameter"),
            ("catch_clause", "parameter"),
            ("for_in_statement", "left"),
            ("function_declaration", "name"),
            ("generator_function_declaration", "name"),
        ];
        let kinds: Vec<&str> = fields.iter().map(|(k, _)| *k).collect();
        let mut out = HashSet::new();
        for n in walk(self.scope.unwrap_or(self.body), &kinds).into_iter().filter(|n| self.mine(*n)) {
            let field = fields.iter().find(|(k, _)| *k == n.kind()).map_or("", |(_, f)| *f);
            if let Some(p) = n.child_by_field_name(field) {
                pattern(p, self.src, &mut out);
            }
        }
        out
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
        // some parts of the world are reached only through a client the package made, not its helpers
        let Some(kind) = effects::boundary_ts(pkg.as_deref(), parts)
            .filter(|k| !knowledge::THROUGH_A_CLIENT.contains(&k.world) || self.made.contains_key(&head))
        else {
            return;
        };
        let op = parts.last().cloned().unwrap_or_default();
        let cr = Crosses::read(c, src, kind, pkg.as_deref(), &op, &literals(c, src, STRINGS), STRINGS, &HashMap::new());
        let args = c.child_by_field_name("arguments").map(named_children).unwrap_or_default();
        let at = At { m, scope: self.scope, params: &self.params };
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
    /// changes nothing (`http.delete(..)` on a library's client).
    pub(super) fn changed(&self) -> std::collections::BTreeSet<&'t str> {
        let changes = |c: TsNode<'t>| -> Option<TsNode<'t>> {
            let f = function_of(c).filter(|f| f.kind() == "member_expression")?;
            let on = f.child_by_field_name("object")?;
            let changing = match self.callee(c).filter(|t| self.own(t)) {
                Some(t) => self.store.mutating.contains(&t),
                None => {
                    f.child_by_field_name("property")
                        .map_or(false, |p| knowledge::mutates(knowledge::Registry::Npm, "javascript", text(p, self.src)))
                        && self.holds_container(on)
                }
            };
            changing.then_some(on)
        };
        changes_in(self.body, self.src, &changes)
            .into_iter()
            .filter_map(|(x, root)| changed_through(self.body, self.src, &BINDS, root, x.start_byte()))
            .collect()
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
        for r in self.changed() {
            let ty = if r == "this" { self.cls.clone().filter(|_| !self.initializer) } else { self.inputs.get(r).cloned() };
            if let Some(t) = ty {
                self.read.mutates.push((t, r.to_string()));
            }
        }
    }
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
    match n.kind() {
        "identifier" | "this" => Some(text(n, src)),
        "member_expression" | "subscript_expression" => chain_root(n.child_by_field_name("object")?, src),
        k if WRAPPERS.contains(&k) => chain_root(wrapped(n)?, src),
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
        (l.kind() == "identifier").then(|| (text(l, src), held_by(v, src)))
    },
    loops: |n, src| {
        let of = n.child_by_field_name("operator").map_or(false, |o| o.kind() == "of");
        let (l, v, b) = (n.child_by_field_name("left")?, n.child_by_field_name("right")?, n.child_by_field_name("body")?);
        (n.kind() == "for_in_statement" && of && l.kind() == "identifier").then(|| (text(l, src), chain_root(v, src), b))
    },
};

/// The names whose value a TypeScript value may be or be a part of: `graph`, `this.items[0]`,
/// either side of `a ?? b`, `a || b` and `c ? a : b`. A call, a spread (`{ ...e }`) and a literal
/// make a value anew.
fn held_by<'a>(v: TsNode, src: &'a [u8]) -> Vec<&'a str> {
    match v.kind() {
        "binary_expression" if v.child_by_field_name("operator").map_or(false, |o| matches!(o.kind(), "??" | "||" | "&&")) => {
            [v.child_by_field_name("left"), v.child_by_field_name("right")].into_iter().flatten().flat_map(|x| held_by(x, src)).collect()
        }
        "ternary_expression" => [v.child_by_field_name("consequence"), v.child_by_field_name("alternative")]
            .into_iter()
            .flatten()
            .flat_map(|x| held_by(x, src))
            .collect(),
        k if WRAPPERS.contains(&k) => wrapped(v).into_iter().flat_map(|x| held_by(x, src)).collect(),
        _ => chain_root(v, src).into_iter().collect(),
    }
}

/// Each change a body makes in place: the node that makes it, and the name whose object it
/// changes, the root of every assignment to a member or an item (`this.x = ..`, `p.items[k] +=
/// ..`, `p.n++`, `delete p.k`), and of the value every call that changes something is made on, as
/// `changes` says (`p.rows.push(..)`). Changing a part changes the whole.
pub(super) fn changes_in<'a, 't>(
    body: TsNode<'t>,
    src: &'a [u8],
    changes: &dyn Fn(TsNode<'t>) -> Option<TsNode<'t>>,
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
        out.extend(changes(c).and_then(|o| chain_root(o, src)).map(|r| (c, r)));
    }
    out
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

/// TypeScript's choices: `if`/`else if`/`else`, `a ? b : c`, `switch`, `try`/`catch`,
/// `x ?? y` and `x || y`.
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
    splits: |n, child| n.kind() == "switch_statement" && n.child_by_field_name("value").map(|v| v.id()) == Some(child.id()),
    names: &["identifier", "shorthand_property_identifier"],
    keeps: &["member_expression", "subscript_expression"],
};

const CHOICES: ChoiceSyntax = ChoiceSyntax {
    chooses: |n, src| match n.kind() {
        "if_statement" | "ternary_expression" | "switch_body" | "try_statement" => true,
        "binary_expression" => n.child_by_field_name("operator").map_or(false, |o| matches!(text(o, src), "||" | "??")),
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
            "binary_expression" if is("left") => Part::Arm(0),
            "binary_expression" if is("right") => Part::Arm(1),
            _ => Part::Through,
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
