//! The steps of a body as a string diagram: each call is an arrow, and the values between them
//! are its wires. Composition is call order, and `f(g(x))` is `f ∘ g`: what the inner call gives
//! is an input of the outer one, and what the inner call is handed is its own input, never the
//! outer call's.
//!
//! The readers record each call with where it is written and each value it is handed with where
//! that is written; what follows is the same in every language.

use std::collections::{BTreeMap, HashMap, HashSet};

use indexmap::IndexMap;
use serde::Serialize;

use crate::model::{Db, Dir, Edge, EdgeKind, Graph, Held, Node, NodeKind, Site, Touch, World};

/// Wire the steps of every body: a call written inside another call hands it its result, under
/// the name the body gives it or, without one, under the step's own (`#n`, unique in the graph);
/// a library's operation that only turns a value of one type into a value of the same type is an
/// identity, and the value it names is the one the step before it makes.
pub fn wire(nodes: &mut IndexMap<String, Node>, edges: &[Edge]) {
    let gives_class = classes_given(nodes, edges);
    let mut next = 0;
    for id in nodes.keys().cloned().collect::<Vec<_>>() {
        let mut sites = std::mem::take(&mut nodes[&id].sites);
        nest(&mut sites, &gives_class, &mut next);
        drop_identities(&mut sites);
        nodes[&id].sites = sites;
    }
}

/// The class a step gives when the body does not name its result: the class a constructor builds,
/// or the class a function's signature returns.
fn classes_given(nodes: &IndexMap<String, Node>, edges: &[Edge]) -> HashMap<String, String> {
    let mut out: HashMap<String, String> = HashMap::new();
    for e in edges.iter().filter(|e| e.kind == EdgeKind::Returns) {
        if nodes.get(&e.dst).map_or(false, |n| matches!(n.kind, NodeKind::Class | NodeKind::External)) {
            out.entry(e.src.clone()).or_insert_with(|| e.dst.clone());
        }
    }
    for n in nodes.values().filter(|n| n.kind == NodeKind::Class) {
        out.insert(n.id.clone(), n.id.clone());
    }
    out
}

fn inside(inner: (u32, u32), outer: (u32, u32)) -> bool {
    inner != outer && outer.0 <= inner.0 && inner.1 <= outer.1
}

fn written(s: &Site) -> bool {
    s.span != (0, 0)
}

/// Each step written inside another hands it its result; the outer step is no longer handed what
/// the inner one is.
fn nest(sites: &mut [Site], gives_class: &HashMap<String, String>, next: &mut usize) {
    // the call each step is written in: the smallest other step around it
    let around: Vec<Option<usize>> = (0..sites.len())
        .map(|i| {
            (0..sites.len())
                .filter(|&j| j != i && written(&sites[i]) && written(&sites[j]) && inside(sites[i].span, sites[j].span))
                .min_by_key(|&j| sites[j].span.1 - sites[j].span.0)
        })
        .collect();
    for i in 0..sites.len() {
        let Some(outer) = around[i] else { continue };
        // the result without a name is named by its step
        let name = format!("#{next}");
        *next += 1;
        let s = &mut sites[i];
        if !s.gives.iter().any(|g| !g.name.is_empty()) {
            if s.gives.is_empty() {
                let ty = if s.to.is_empty() { String::new() } else { gives_class.get(&s.to).cloned().unwrap_or_default() };
                s.gives.push(Held::new(ty, name.clone()));
            }
            for g in s.gives.iter_mut() {
                g.name = name.clone();
            }
        }
        let made: Vec<Held> = s.gives.clone();
        let o = &mut sites[outer];
        for h in made {
            if !o.hands.contains(&h) {
                o.hands.push(h);
            }
        }
    }
    // what a step written inside another is handed is that step's, not the outer one's
    for j in 0..sites.len() {
        let inner: Vec<(u32, u32)> = (0..sites.len()).filter(|&i| i != j && written(&sites[i]) && written(&sites[j]) && inside(sites[i].span, sites[j].span)).map(|i| sites[i].span).collect();
        if inner.is_empty() {
            continue;
        }
        sites[j].hands.retain(|h| h.at == 0 || !inner.iter().any(|(a, b)| *a <= h.at && h.at < *b));
    }
}

/// A library's operation handed one value and giving a value of the same type, either the result of
/// the step before it or the value it is named for, is an identity (`option.ok_or_else(..)`,
/// `x = x.copy()`): the arrow is not drawn, and the step before it gives what it names.
fn drop_identities(sites: &mut Vec<Site>) {
    let mut gone = vec![];
    for b in 0..sites.len() {
        let s = &sites[b];
        let ([h], [g]) = (s.hands.as_slice(), s.gives.as_slice()) else { continue };
        if !s.to.is_empty() || g.ty.is_empty() || g.ty != h.ty || !(h.name.starts_with('#') || h.name == g.name) {
            continue;
        }
        let (h, g) = (h.clone(), g.clone());
        if let Some(maker) = sites.iter().position(|x| x.gives.contains(&h)) {
            for x in sites[maker].gives.iter_mut().filter(|x| **x == h) {
                *x = g.clone();
            }
        }
        gone.push(b);
    }
    for b in gone.into_iter().rev() {
        sites.remove(b);
    }
}

// ---------- places: where a touch rests, which side of its step, and what it does there ----------

/// A part of the world as the picture names it. A touch rests at its finest place; until the
/// picture splits `group`, it is drawn as `coarse` (the disk a route reads, a database for its
/// tables), when that differs.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Place {
    pub id: String,
    pub label: String,
    pub short: String,
    /// The kind of place, as one glyph: `file`, `db`, `web`, `screen`, `proc`, `model`.
    pub glyph: &'static str,
    /// The path, for a file the code names: files fold into their directories.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coarse: Option<Box<Place>>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub group: String,
}

/// Which side of its step a touch stands on.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    In,
    Out,
}

impl World {
    /// The name the picture gives this part of the world.
    pub fn name(self) -> &'static str {
        match self {
            World::Disk => "Disk",
            World::Net => "Web",
            World::Db(Db::Sql) => "SQL",
            World::Db(Db::Sqlite) => "SQLite",
            World::Db(Db::Duckdb) => "DuckDB",
            World::Db(Db::Graph) => "GraphDB",
            World::Db(Db::Kv) => "KV",
            World::Llm => "LLM",
            World::Model => "Model",
            World::Process => "open",
            World::Screen => "Screen",
            World::Figure => "Figure",
        }
    }

    fn glyph(self) -> &'static str {
        match self {
            World::Disk => "file",
            World::Db(_) => "db",
            World::Net => "web",
            World::Screen | World::Figure => "screen",
            World::Process => "proc",
            World::Llm | World::Model => "model",
        }
    }
}

fn place(id: String, label: String, short: String, glyph: &'static str) -> Place {
    Place { id, label, short, glyph, file: String::new(), coarse: None, group: String::new() }
}

impl Touch {
    /// Which side of its step the touch stands on: what the call says (a file's mode, a
    /// statement's verb, a download), else what its kind of place does by default. The web and a
    /// file whose way is not said are read; everything else is written to.
    pub fn side(&self) -> Side {
        match (self.dir, self.kind) {
            (Some(Dir::Read), _) => Side::In,
            (Some(Dir::Write), _) => Side::Out,
            (None, World::Net | World::Disk) => Side::In,
            (None, _) => Side::Out,
        }
    }

    /// Where the touch rests. A file the code names is that file, unless the operation looks at
    /// the shape of the file system rather than at a file's data (whether a path exists, what a
    /// directory lists), which is the disk, except on a directory, a place of its own
    /// (`assets/regelingen/`). A file only a variable names is the disk read or written, split
    /// by that name. A database is one place, split by its tables; a host on the web and a
    /// model's checkpoint are named where the code names them; a program by its command.
    pub fn place(&self) -> Place {
        let (w, name) = (self.kind, self.kind.name());
        let base = format!("w:{name}");
        let world = || place(base.clone(), name.to_string(), name.to_string(), w.glyph());
        match w {
            World::Process => {
                let what = if self.what.is_empty() { "process" } else { self.what.as_str() };
                place(format!("w:open:{what}"), what.to_string(), what.to_string(), "proc")
            }
            World::Disk => {
                let shape = crate::effects::fs_op(&self.op) != crate::effects::FsOp::Data;
                if !self.artifact.is_empty() && (!shape || self.artifact.ends_with('/')) {
                    let a = &self.artifact;
                    return Place { file: a.clone(), ..place(format!("w:File:{a}"), format!("File({a})"), a.clone(), "file") };
                }
                let coarse = match self.side() {
                    Side::In => place("w:Disk:in".into(), "Disk in".into(), "Disk in".into(), "file"),
                    Side::Out => place("w:Disk:out".into(), "Disk out".into(), "Disk out".into(), "file"),
                };
                if self.what.is_empty() {
                    return coarse;
                }
                let w = &self.what;
                Place { coarse: Some(Box::new(coarse)), group: "w:Disk".into(), ..place(format!("w:Disk:{w}"), format!("Disk({w})"), w.clone(), "file") }
            }
            World::Db(_) if !self.artifact.is_empty() => {
                let label = format!("{name}({})", self.artifact);
                Place { coarse: Some(Box::new(world())), group: base.clone(), ..place(format!("{base}:{}", self.artifact), label.clone(), label, "db") }
            }
            World::Net | World::Model if !self.artifact.is_empty() => place(format!("{base}:{}", self.artifact), format!("{name}({})", self.artifact), self.artifact.clone(), w.glyph()),
            _ => world(),
        }
    }

    /// What the touch does at its place, as a verb: the operation when it says more than the kind
    /// (`checks`, `lists`, `GETs`), the way the data goes when the kind leaves it open (a
    /// statement reads or writes, a transfer downloads or uploads), else the kind's own.
    pub fn verb(&self) -> String {
        use crate::effects::{fs_op, FsOp};
        let op = self.op.as_str();
        let by_op = match (self.kind, fs_op(op)) {
            (World::Disk, FsOp::Checks) => Some("checks"),
            (World::Disk, FsOp::Lists) => Some("lists"),
            (World::Disk, FsOp::Creates) => Some("creates"),
            (World::Disk, FsOp::Removes) => Some("removes"),
            (World::Disk, FsOp::Moves) => Some("moves"),
            (World::Disk, FsOp::Copies) => Some("copies"),
            _ => None,
        };
        if let Some(v) = by_op {
            return v.into();
        }
        match (self.kind, self.dir) {
            (World::Db(_), Some(Dir::Read)) => "reads".into(),
            (World::Db(_), Some(Dir::Write)) => "writes".into(),
            (World::Net, _) if matches!(op, "get" | "post" | "put" | "delete" | "patch") => format!("{}s", op.to_uppercase()),
            (World::Net, Some(Dir::Read)) => "downloads".into(),
            (World::Net, Some(Dir::Write)) => "uploads".into(),
            (World::Net, None) => "calls".into(),
            (World::Disk, Some(Dir::Read)) => "reads".into(),
            (World::Disk, Some(Dir::Write)) => "writes".into(),
            (World::Disk, None) => "opens".into(),
            (World::Db(_), None) => format!("queries {}", self.kind.name()),
            (World::Llm, _) => "asks an LLM".into(),
            (World::Model, _) => "runs a model".into(),
            (World::Process, _) => "runs".into(),
            (World::Screen, _) if op == "render" => "renders".into(),
            (World::Screen, _) if matches!(op, "figure" | "subplots" | "subplot_mosaic" | "show") => "draws".into(),
            (World::Screen, _) => "prints".into(),
            (World::Figure, _) => "draws".into(),
        }
    }
}

// ---------- arrows: what a function takes and gives, as types ----------

/// What a function takes and gives as types: the effect functor on an arrow, before the places its
/// body touches. A class is the arrow that builds it.
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct Arrow {
    /// The class of the value a method runs on: none for a constructor, or a method called on
    /// its class.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub receiver: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub takes: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub gives: Vec<String>,
    /// The class a constructor builds.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub builds: String,
    /// What the body changes in place: `T -> T`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub changes: Vec<String>,
    /// The signature gives a value back: a type of the code's, or what it gives on success
    /// (unwrapped from a `Result` or a `Promise`) is no unit. An opened body makes it from what
    /// its steps give.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub gives_back: bool,
    /// What it gives back on success is a value, not a unit: a value with no place of its own (a
    /// `str`) goes back to the step that called it.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub returns_value: bool,
}

/// The types that stand for nothing given back, in the languages Atlas reads: Python's `None`,
/// Rust's `()`, TypeScript's `void` and `undefined`, and `Self`, the value the method was
/// called on.
fn unit(ty: &str) -> bool {
    matches!(ty, "None" | "()" | "Self" | "void" | "undefined")
}

/// A `Result<T, E>` or a `Promise<T>` as the `T` it gives on success.
fn unwrapped(ty: &str) -> &str {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"^(?:[\w:]*::)?(?:Result|Promise)<(.*?)(?:,.*)?>$").unwrap());
    re.captures(ty).and_then(|c| c.get(1)).map_or(ty, |m| m.as_str())
}

impl<'g> Ix<'g> {
    fn targets(&self, id: &str, kind: EdgeKind) -> Vec<String> {
        let mut out: Vec<String> = vec![];
        for e in self.out(id, kind) {
            if !out.contains(&e.dst) {
                out.push(e.dst.clone());
            }
        }
        out
    }

    fn ctor(&self, class: &str) -> Option<String> {
        let c = format!("{class}.__init__");
        self.nodes.contains_key(c.as_str()).then_some(c)
    }

    /// What building a class takes: its constructor's parameters, or its nearest base's when it
    /// has none of its own.
    fn built_from(&self, class: &str, depth: usize) -> Vec<String> {
        if let Some(c) = self.ctor(class) {
            return self.targets(&c, EdgeKind::Takes);
        }
        let base = self.out(class, EdgeKind::Inherits).find(|e| self.nodes.get(e.dst.as_str()).map_or(false, |n| n.kind == NodeKind::Class));
        match base {
            Some(b) if depth < 10 => self.built_from(&b.dst, depth + 1),
            _ => vec![],
        }
    }
}

fn arrow_of(ix: &Ix, n: &Node) -> Arrow {
    if n.kind == NodeKind::Class {
        return Arrow { takes: ix.built_from(&n.id, 0), gives: vec![n.id.clone()], ..Default::default() };
    }
    let ctor = n.cls.as_deref().map_or(false, |c| n.id == format!("{c}.__init__"));
    let gives = ix.targets(&n.id, EdgeKind::Returns);
    let returned = unwrapped(&n.returns);
    let returns_value = !returned.is_empty() && !unit(returned);
    Arrow {
        receiver: n.cls.clone().filter(|_| !ctor && !n.on_class).unwrap_or_default(),
        takes: ix.targets(&n.id, EdgeKind::Takes),
        gives_back: !gives.is_empty() || returns_value,
        gives,
        builds: if ctor { n.cls.clone().unwrap_or_default() } else { String::new() },
        changes: ix.targets(&n.id, EdgeKind::Mutates),
        returns_value,
    }
}

/// The arrow of every function, method and class.
pub fn arrows(g: &Graph) -> BTreeMap<String, Arrow> {
    let ix = Ix::new(g);
    g.nodes.iter().filter(|n| matches!(n.kind, NodeKind::Class | NodeKind::Function | NodeKind::Method)).map(|n| (n.id.clone(), arrow_of(&ix, n))).filter(|(_, a)| *a != Arrow::default()).collect()
}

/// What a body's steps make: the class a step builds, else what the function it runs gives back.
fn made_from(ix: &Ix, body: &Body) -> Vec<String> {
    let mut callees: Vec<&str> = vec![];
    for t in body.steps().into_iter().map(|s| s.to.as_str()).chain(body.alts.iter().map(String::as_str)) {
        if ix.nodes.contains_key(t) && !callees.contains(&t) {
            callees.push(t);
        }
    }
    let mut out = vec![];
    for c in callees {
        let made = if ix.nodes[c].kind == NodeKind::Class { vec![c.to_string()] } else { ix.targets(c, EdgeKind::Returns) };
        out.extend(made);
    }
    out
}

// ---------- bodies: what a step runs, the same for the agent and the page ----------

/// One step of a body as a route runs it: the function it reaches, or a library's operation
/// (`op:to_numpy`); the class of the value it runs on, when the call says; and the call in the
/// body's `sites` it is, when the body lists its calls.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Step {
    pub to: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub on: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub site: Option<usize>,
}

/// A step that is one branch of a choice.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Member {
    #[serde(flatten)]
    pub step: Step,
    pub branch: usize,
}

/// A body's steps in call order; steps in different arms of one choice (an `if`, a `match`, an
/// or-else) are alternatives, one item.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(untagged)]
pub enum Item {
    Step(Step),
    Choice { choice: u32, members: Vec<Member> },
}

/// What running a function means, on a value of a known class or not: the overrides a call may
/// run instead (a call to `Language.parse` runs `Python.parse` or `Rust.parse`, unless the class
/// is known), and its own steps.
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct Body {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub alts: Vec<String>,
    pub items: Vec<Item>,
    /// What an opened body makes the value it gives back from: the classes its steps build and
    /// what the functions they run give back.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub made_from: Vec<String>,
}

impl Body {
    fn is_empty(&self) -> bool {
        self.alts.is_empty() && self.items.is_empty()
    }

    /// Every step, branches included, in order.
    pub fn steps(&self) -> Vec<&Step> {
        self.items
            .iter()
            .flat_map(|it| match it {
                Item::Step(s) => vec![s],
                Item::Choice { members, .. } => members.iter().map(|m| &m.step).collect(),
            })
            .collect()
    }
}

/// The key of a body: the function, and `@` the class it runs on when that is known.
pub fn key(to: &str, on: &str) -> String {
    if on.is_empty() { to.to_string() } else { format!("{to}@{on}") }
}

/// The edges of a graph by their ends, in the order the graph holds them.
struct Ix<'g> {
    nodes: HashMap<&'g str, &'g Node>,
    out: HashMap<&'g str, Vec<&'g Edge>>,
    into: HashMap<&'g str, Vec<&'g Edge>>,
}

impl<'g> Ix<'g> {
    fn new(g: &'g Graph) -> Self {
        let mut out: HashMap<&str, Vec<&Edge>> = HashMap::new();
        let mut into: HashMap<&str, Vec<&Edge>> = HashMap::new();
        for e in &g.edges {
            out.entry(e.src.as_str()).or_default().push(e);
            into.entry(e.dst.as_str()).or_default().push(e);
        }
        Ix { nodes: g.nodes.iter().map(|n| (n.id.as_str(), n)).collect(), out, into }
    }

    fn out(&self, id: &str, kind: EdgeKind) -> impl Iterator<Item = &'g Edge> + '_ {
        self.out.get(id).into_iter().flatten().copied().filter(move |e| e.kind == kind)
    }

    /// The overrides a call to a method may run: the methods of that name of every class below
    /// the method's own.
    fn overrides(&self, n: &Node) -> Vec<String> {
        let Some(cls) = n.cls.as_deref() else { return vec![] };
        let (mut out, mut stack, mut seen) = (vec![], vec![cls.to_string()], HashSet::from([cls.to_string()]));
        while let Some(c) = stack.pop() {
            for e in self.into.get(c.as_str()).into_iter().flatten().filter(|e| e.kind == EdgeKind::Inherits) {
                if seen.insert(e.src.clone()) {
                    stack.push(e.src.clone());
                    let m = format!("{}.{}", e.src, n.name);
                    if self.nodes.contains_key(m.as_str()) {
                        out.push(m);
                    }
                }
            }
        }
        out
    }

    /// The method a value of class `on` runs for `name`: its class's own, or the nearest base's.
    fn method_on(&self, on: &str, name: &str, depth: usize) -> Option<String> {
        let own = format!("{on}.{name}");
        if self.nodes.contains_key(own.as_str()) {
            return Some(own);
        }
        if depth > 10 {
            return None;
        }
        self.out(on, EdgeKind::Inherits).find_map(|e| self.method_on(&e.dst, name, depth + 1))
    }
}

/// The steps in call order, the alternatives of each choice with two or more branches together.
fn grouped(steps: Vec<(Step, Vec<(u32, u32)>)>) -> Vec<Item> {
    let choice_of = |arms: &Vec<(u32, u32)>| arms.first().map(|a| a.0);
    let mut items = vec![];
    let mut placed = HashSet::new();
    for (step, arms) in &steps {
        let Some(k) = choice_of(arms) else {
            items.push(Item::Step(step.clone()));
            continue;
        };
        let group: Vec<&(Step, Vec<(u32, u32)>)> = steps.iter().filter(|(_, a)| choice_of(a) == Some(k)).collect();
        let mut paths: Vec<&Vec<(u32, u32)>> = vec![];
        for (_, a) in &group {
            if !paths.contains(&a) {
                paths.push(a);
            }
        }
        if paths.len() < 2 {
            items.push(Item::Step(step.clone()));
        } else if placed.insert(k) {
            let members = group.iter().map(|(s, a)| Member { step: s.clone(), branch: paths.iter().position(|p| *p == a).unwrap_or(0) }).collect();
            items.push(Item::Choice { choice: k, members });
        }
    }
    items
}

/// What a function runs on a value of class `on`: its calls as the body lists them, each method
/// call on a known class that class's method; or, where the body lists no calls, what the call
/// graph says. A library class (an export) runs its constructor and its public methods.
fn body_of(ix: &Ix, n: &Node, on: &str) -> Body {
    let alts = if on.is_empty() { ix.overrides(n) } else { vec![] };
    if !n.sites.is_empty() {
        let steps = n
            .sites
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let runs_on = if !s.on.is_empty() { s.on.as_str() } else if s.this { on } else { "" };
                let mut to = if s.to.is_empty() { format!("op:{}", s.op) } else { s.to.clone() };
                if let Some(m) = ix.nodes.get(s.to.as_str()).filter(|m| m.cls.is_some() && !runs_on.is_empty()) {
                    to = ix.method_on(runs_on, &m.name, 0).unwrap_or(to);
                }
                (Step { to, on: runs_on.to_string(), site: Some(i) }, s.arms.clone())
            })
            .collect();
        return Body { alts, items: grouped(steps), made_from: vec![] };
    }
    let callees: Vec<String> = if n.kind == NodeKind::Class && n.entry.is_some() {
        let ctor = format!("{}.__init__", n.id);
        let mut out: Vec<String> = ix.nodes.contains_key(ctor.as_str()).then_some(ctor).into_iter().collect();
        out.extend(ix.out(&n.id, EdgeKind::Contains).map(|e| e.dst.clone()).filter(|m| ix.nodes.get(m.as_str()).map_or(false, |m| m.kind == NodeKind::Method && !m.name.starts_with('_'))));
        out
    } else {
        let mut seen = HashSet::new();
        ix.out
            .get(n.id.as_str())
            .into_iter()
            .flatten()
            .filter(|e| matches!(e.kind, EdgeKind::Calls | EdgeKind::Instantiates) && ix.nodes.contains_key(e.dst.as_str()))
            .map(|e| e.dst.clone())
            .filter(|d| seen.insert(d.clone()) && !alts.contains(d))
            .collect()
    };
    let mut arms: HashMap<&str, Vec<(u32, u32)>> = HashMap::new();
    for e in ix.out.get(n.id.as_str()).into_iter().flatten().filter(|e| matches!(e.kind, EdgeKind::Calls | EdgeKind::Instantiates) && !e.arms.is_empty()) {
        arms.entry(e.dst.as_str()).or_insert_with(|| e.arms.clone());
    }
    let steps = callees.iter().map(|c| (Step { to: c.clone(), on: String::new(), site: None }, arms.get(c.as_str()).cloned().unwrap_or_default())).collect();
    Body { alts, items: grouped(steps), made_from: vec![] }
}

/// The body of every function, and of every function on each class a step runs it on, by `key`:
/// what a route through it runs, for whoever reads the route. A Makefile target runs the targets
/// and commands its recipe names.
pub fn bodies(g: &Graph) -> BTreeMap<String, Body> {
    let ix = Ix::new(g);
    let mut out = BTreeMap::new();
    for t in &g.make {
        let items: Vec<Item> = t.calls.iter().filter(|c| ix.nodes.contains_key(c.as_str()) || g.make.iter().any(|m| &m.id == *c)).map(|c| Item::Step(Step { to: c.clone(), on: String::new(), site: None })).collect();
        if !items.is_empty() {
            out.insert(t.id.clone(), Body { alts: vec![], items, made_from: vec![] });
        }
    }
    let mut todo: Vec<(String, String)> = g.nodes.iter().rev().map(|n| (n.id.clone(), String::new())).collect();
    let mut seen = HashSet::new();
    while let Some((f, on)) = todo.pop() {
        let k = key(&f, &on);
        if !seen.insert(k.clone()) {
            continue;
        }
        let Some(n) = ix.nodes.get(f.as_str()) else { continue };
        let mut body = body_of(&ix, n, &on);
        for s in body.steps().into_iter().filter(|s| !s.on.is_empty()) {
            todo.push((s.to.clone(), s.on.clone()));
        }
        body.made_from = made_from(&ix, &body);
        if !body.is_empty() {
            out.insert(k, body);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site(to: &str, op: &str, span: (u32, u32), hands: Vec<Held>, gives: Vec<Held>) -> Site {
        Site { to: to.into(), op: op.into(), span, hands, gives, ..Default::default() }
    }

    fn at(ty: &str, name: &str, at: u32) -> Held {
        Held { ty: ty.into(), name: name.into(), at }
    }

    /// `lang = detect(opts).ok_or_else(..)`: `detect` is handed `opts` and gives `lang`; the
    /// unwrapping between them is an identity.
    #[test]
    fn a_nested_call_hands_its_result_on() {
        let mut sites = vec![
            site("detect", "", (10, 22), vec![at("Options", "opts", 17)], vec![]),
            site("", "ok_or_else", (10, 40), vec![at("Options", "opts", 17)], vec![Held::new("Language", "lang")]),
        ];
        let gives_class = HashMap::from([("detect".to_string(), "Language".to_string())]);
        nest(&mut sites, &gives_class, &mut 0);
        assert_eq!(sites[1].hands, [Held::new("Language", "#0")], "the outer call is handed the inner one's result, not its arguments");
        drop_identities(&mut sites);
        assert_eq!(sites.len(), 1, "an identity is not drawn");
        assert_eq!((sites[0].hands.clone(), sites[0].gives.clone()), (vec![Held::new("Options", "opts")], vec![Held::new("Language", "lang")]));
    }

    /// `f(g(x), y)`: `g` takes `x`, `f` takes what `g` gives and `y`.
    #[test]
    fn composition_is_wired() {
        let mut sites = vec![site("g", "", (2, 6), vec![at("X", "x", 4)], vec![]), site("f", "", (0, 10), vec![at("X", "x", 4), at("Y", "y", 8)], vec![])];
        nest(&mut sites, &HashMap::from([("g".to_string(), "Z".to_string())]), &mut 0);
        assert_eq!(sites[0].gives, [Held::new("Z", "#0")]);
        assert_eq!(sites[1].hands, [Held::new("Y", "y"), Held::new("Z", "#0")]);
    }

    /// `lang = detect(opts).unwrap()` read by a language that names the unwrapped value already:
    /// the arrow from `lang` to `lang` is the identity.
    /// `Result<(), E>` and `Promise<void>` give nothing back on success; `Result<Graph>` gives a
    /// `Graph`.
    #[test]
    fn a_unit_wrapped_is_nothing_given_back() {
        assert_eq!(unwrapped("std::io::Result<()>"), "()");
        assert_eq!(unwrapped("Promise<void>"), "void");
        assert_eq!(unwrapped("Result<Vec<String>, Error>"), "Vec<String>");
        assert!(unit(unwrapped("anyhow::Result<()>")) && !unit(unwrapped("Result<Graph>")));
    }

    #[test]
    fn an_identity_on_a_named_value_is_not_drawn() {
        let mut sites = vec![
            site("detect", "", (10, 22), vec![at("Options", "opts", 17)], vec![Held::new("Language", "lang")]),
            site("", "unwrap", (10, 31), vec![at("Options", "opts", 17)], vec![Held::new("Language", "lang")]),
        ];
        nest(&mut sites, &HashMap::new(), &mut 0);
        drop_identities(&mut sites);
        assert_eq!(sites.iter().map(|s| s.to.as_str()).collect::<Vec<_>>(), ["detect"]);
    }
}
