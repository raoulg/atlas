//! The steps of a body as a string diagram: each call is an arrow, and the values between them
//! are its wires. Composition is call order, and `f(g(x))` is `f ∘ g`: what the inner call gives
//! is an input of the outer one, and what the inner call is handed is its own input, never the
//! outer call's.
//!
//! The readers record each call with where it is written and each value it is handed with where
//! that is written; what follows is the same in every language.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::rc::Rc;

use indexmap::IndexMap;
use serde::Serialize;

use crate::model::{Db, Dir, Edge, EdgeKind, Graph, Held, Leaves, Node, NodeKind, Site, Touch, World};

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
        let inner: Vec<(u32, u32)> = (0..sites.len())
            .filter(|&i| i != j && written(&sites[i]) && written(&sites[j]) && inside(sites[i].span, sites[j].span))
            .map(|i| sites[i].span)
            .collect();
        if inner.is_empty() {
            continue;
        }
        sites[j].hands.retain(|h| h.at == 0 || !inner.iter().any(|(a, b)| *a <= h.at && h.at < *b));
    }
}

/// A library's operation handed one value and giving a value of the same type, the result of the
/// step it is written on (`option.ok_or_else(..)`, `lang = detect(opts).unwrap()`), is an identity: the
/// arrow is not drawn, and the step before it gives what it names. One handed a value the body named
/// before it makes that value anew (`board = copy.copy(board)`), and a call that changes what it is
/// called on is none, even where the graph follows nothing else it is handed.
fn drop_identities(sites: &mut Vec<Site>) {
    let mut gone = vec![];
    for b in 0..sites.len() {
        let s = &sites[b];
        let ([h], [g]) = (s.hands.as_slice(), s.gives.as_slice()) else { continue };
        let maker = sites.iter().position(|x| x.gives.contains(h));
        let chained = h.name.starts_with('#') || (h.name == g.name && maker.is_some_and(|m| m != b && inside(sites[m].span, s.span)));
        if !s.to.is_empty() || s.changes || g.ty.is_empty() || g.ty != h.ty || !chained {
            continue;
        }
        let (h, g) = (h.clone(), g.clone());
        if let Some(maker) = maker {
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

/// The image of the effect functor: what every step runs, what every function takes and gives as
/// types, how the types group, and where each touch is drawn. The agent and the page read routes
/// from it alike.
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct Flow {
    pub bodies: BTreeMap<String, Body>,
    pub arrows: BTreeMap<String, Arrow>,
    pub types: crate::families::Families,
    pub units: Units,
    /// For each node with touches, where each of them is drawn: its touches in order, then its
    /// calls as read (`instead`).
    pub drawn: BTreeMap<String, Vec<Where>>,
}

/// The flow of a graph.
pub fn of(g: &Graph) -> Flow {
    let drawn = g
        .nodes
        .iter()
        .filter(|n| !n.touches.is_empty() || !n.instead.is_empty())
        .map(|n| (n.id.clone(), n.touches.iter().chain(&n.instead).map(Touch::drawn).collect()))
        .collect();
    Flow { bodies: bodies(g), arrows: arrows(g), types: crate::families::families(g), units: units(g), drawn }
}

/// The code quotiented by module path, one level at a time. A part's top level is the children
/// of its own package, and each tree beside it (scripts, notebooks) as one; a level with a single
/// child is passed through; a package with code of its own has that code beside its children as a
/// unit of its own (`app.serve$`); a Makefile is a unit of its part.
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct Units {
    /// Each part's top level, in the order of the parts.
    pub tops: Vec<(String, Vec<String>)>,
    /// What a unit opens into.
    pub kids: BTreeMap<String, Vec<String>>,
}

pub fn units(g: &Graph) -> Units {
    let first = g.parts.first().map_or_else(|| g.root.clone(), |p| p.id.clone());
    let part_of = |n: &Node| if n.part.is_empty() { first.clone() } else { n.part.clone() };
    let modules: Vec<(&str, String)> =
        g.nodes.iter().filter(|n| n.kind == NodeKind::Module && !n.library).map(|n| (n.id.as_str(), part_of(n))).collect();
    let holds: HashSet<&str> = g.edges.iter().filter(|e| e.kind == EdgeKind::Contains).map(|e| e.src.as_str()).collect();
    let nodes: HashMap<&str, &Node> = g.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let has_code = |m: &str| holds.contains(m) || nodes.get(m).map_or(false, |n| !n.touches.is_empty());
    let mods_of = |part: &str| modules.iter().filter(|(_, p)| p == part).map(|(m, _)| *m).collect::<Vec<&str>>();
    fn below(prefix: &str, mods: &[&str], has_code: &dyn Fn(&str) -> bool, known: &dyn Fn(&str) -> bool) -> Vec<String> {
        let depth = prefix.split('.').count();
        let inside = format!("{prefix}.");
        let mut kids: Vec<String> =
            mods.iter().filter(|m| m.starts_with(&inside)).map(|m| m.split('.').take(depth + 1).collect::<Vec<_>>().join(".")).collect();
        kids.sort();
        kids.dedup();
        if let [only] = kids.as_slice() {
            if mods.iter().any(|m| m.starts_with(&format!("{only}."))) && !has_code(prefix) {
                return below(only, mods, has_code, known);
            }
        }
        if !kids.is_empty() && known(prefix) && has_code(prefix) {
            kids.insert(0, format!("{prefix}$"));
        }
        kids
    }
    let known = |id: &str| nodes.contains_key(id);
    let makefiles: HashSet<&str> = g.make.iter().map(|t| t.module.as_str()).collect();
    let parts: Vec<String> = if g.parts.is_empty() { vec![first.clone()] } else { g.parts.iter().map(|p| p.id.clone()).collect() };
    let mut out = Units::default();
    for part in parts {
        let mods = mods_of(&part);
        let mut heads: Vec<&str> = vec![];
        for m in &mods {
            let h = m.split('.').next().unwrap_or(m);
            if !heads.contains(&h) {
                heads.push(h);
            }
        }
        let mut top: Vec<String> = heads
            .iter()
            .flat_map(|h| {
                let kids = if *h == part { below(h, &mods, &has_code, &known) } else { vec![] };
                if kids.is_empty() {
                    vec![h.to_string()]
                } else {
                    kids
                }
            })
            .collect();
        for t in g.make.iter().filter(|t| if t.part.is_empty() { part == first } else { t.part == part }) {
            if !top.contains(&t.module) {
                top.push(t.module.clone());
            }
        }
        let mut todo = top.clone();
        while let Some(u) = todo.pop() {
            if u.ends_with('$') || makefiles.contains(u.as_str()) || out.kids.contains_key(&u) {
                continue;
            }
            let kids = below(&u, &mods, &has_code, &known);
            if !kids.is_empty() {
                todo.extend(kids.iter().cloned());
                out.kids.insert(u, kids);
            }
        }
        out.tops.push((part, top));
    }
    out
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

/// Where a touch is drawn: the place it rests in, the side of its step it stands on, and what it
/// does there.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Where {
    pub place: Place,
    pub side: Side,
    pub verb: String,
    /// It makes or checks the place's shape, not its content.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub shape: bool,
    /// It only tests the place, and what the test says only decides a choice (`if path.exists()`):
    /// the test is the choice's condition, and the place no box and no line of its own.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub test: bool,
}

/// A touch and where it is drawn, for whoever reads it apart from its graph.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Drawn {
    #[serde(flatten)]
    pub touch: Touch,
    #[serde(flatten)]
    pub at: Where,
}

/// Which side of its step a touch stands on: in, out, or open where the code does not say which way
/// it goes.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    In,
    Out,
    Open,
}

impl Side {
    /// The disk a step reads from or writes to, where no file is named. A file whose way is not
    /// said is read, so no disk is open.
    pub fn disk(self) -> Place {
        match self {
            Side::In | Side::Open => place("w:Disk:in".into(), "from disk".into(), "from disk".into(), "file"),
            Side::Out => place("w:Disk:out".into(), "into disk".into(), "into disk".into(), "file"),
        }
    }
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
            World::Global => "Global",
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
            World::Global => "global",
        }
    }
}

fn place(id: String, label: String, short: String, glyph: &'static str) -> Place {
    Place { id, label, short, glyph, file: String::new(), coarse: None, group: String::new() }
}

/// A text as a label: its first three words, at most 48 characters. A place is named by all of
/// its text and labelled by this.
pub fn shown(text: &str) -> String {
    let text = text.trim();
    let words: Vec<&str> = text.split_whitespace().collect();
    let out = if words.len() > 3 { words[..3].join(" ") + " …" } else { text.to_string() };
    out.chars().take(48).collect()
}

impl Touch {
    /// Where the picture draws it.
    pub fn drawn(&self) -> Where {
        Where { place: self.place(), side: self.side(), verb: self.verb(), shape: self.shape(), test: self.shape() && self.checks }
    }

    /// Whether it makes or checks its place's shape rather than its content: a directory made, a
    /// path tested, a table created, a key looked for.
    pub fn shape(&self) -> bool {
        use crate::effects::{fs_op, FsOp};
        match self.kind {
            World::Disk => matches!(fs_op(&self.op), FsOp::Creates | FsOp::Checks),
            World::Db(_) => self.op == crate::effects::SCHEMA,
            World::Global => self.op == crate::effects::MEMBERSHIP,
            _ => false,
        }
    }

    /// Which side of its step the touch stands on: what the call says (a file's mode, a
    /// statement's verb, a request's method, a download), else what its kind of place does by
    /// default. A file, a request or a database whose way the code does not say (a mode or a
    /// method that is a value) is open, in neither domain nor codomain; everything else is written
    /// to. A file opened with no mode is read: the language says so.
    pub fn side(&self) -> Side {
        match (self.dir, self.kind) {
            (Some(Dir::Read), _) => Side::In,
            (Some(Dir::Write), _) => Side::Out,
            (None, World::Disk | World::Net | World::Db(_)) => Side::Open,
            (None, _) => Side::Out,
        }
    }

    /// Where the touch rests. A file the code names is that file, whatever operation touches it:
    /// its data read or written, a test whether it is there, the making of it, a copy or a move
    /// into it or out of it, its removal; one file is one place. What a directory lists is that
    /// directory, a place of its own (`assets/regelingen/`), and so is a pattern below it; a listing
    /// the code names no directory or pattern for is the disk. A file only a variable names is the
    /// disk read or written, split by that name. A database is one place, split by its tables; a
    /// host on the web and a model's checkpoint are named where the code names them; a program by
    /// its command.
    pub fn place(&self) -> Place {
        let (w, name) = (self.kind, self.kind.name());
        let base = format!("w:{name}");
        let world = || place(base.clone(), name.to_string(), name.to_string(), w.glyph());
        match w {
            World::Process => {
                let what = if self.what.is_empty() { "process" } else { self.what.as_str() };
                place(format!("w:open:{what}"), shown(what), shown(what), "proc")
            }
            World::Disk => {
                // the type whose code gives the path its root keeps the place: its group, closed
                let held = (!self.holder.is_empty()).then(|| {
                    let short = self.holder.rsplit('.').next().unwrap_or(&self.holder).to_string();
                    place(format!("w:Type:{}", self.holder), short.clone(), short, "file")
                });
                let lists = crate::effects::fs_op(&self.op) == crate::effects::FsOp::Lists;
                if !self.artifact.is_empty() && (!lists || self.artifact.ends_with('/') || self.artifact.contains('*')) {
                    let a = &self.artifact;
                    let file = Place { file: a.clone(), ..place(format!("w:File:{a}"), format!("File({a})"), a.clone(), "file") };
                    return match held {
                        Some(h) => Place { group: h.id.clone(), coarse: Some(Box::new(h)), ..file },
                        None => file,
                    };
                }
                let coarse = self.side().disk();
                if self.what.is_empty() && held.is_none() {
                    return coarse;
                }
                // a place only its type names is named by that type
                let w = match &held {
                    Some(h) if self.what.is_empty() => h.label.clone(),
                    _ => self.what.clone(),
                };
                let w = w.as_str();
                let (group, coarse) = match held {
                    Some(h) => (h.id.clone(), h),
                    None => ("w:Disk".to_string(), coarse),
                };
                Place {
                    coarse: Some(Box::new(coarse)),
                    group,
                    ..place(format!("w:Disk:{w}"), format!("Disk({})", shown(w)), shown(w), "file")
                }
            }
            World::Db(_) if !self.artifact.is_empty() => {
                let label = format!("{name}({})", self.artifact);
                Place {
                    coarse: Some(Box::new(world())),
                    group: base.clone(),
                    ..place(format!("{base}:{}", self.artifact), label.clone(), label, "db")
                }
            }
            World::Net | World::Model | World::Global if !self.artifact.is_empty() => {
                place(format!("{base}:{}", self.artifact), format!("{name}({})", self.artifact), self.artifact.clone(), w.glyph())
            }
            _ => world(),
        }
    }

    /// What the touch does at its place, as a verb: the operation when it says more than the kind
    /// (`checks`, `lists`, `GETs`), the way the data goes when the kind leaves it open (a
    /// statement reads or writes, a transfer downloads or uploads), else the kind's own.
    pub fn verb(&self) -> String {
        use crate::effects::{fs_op, FsOp};
        let op = self.op.as_str();
        if matches!(self.kind, World::Db(_)) && op == crate::effects::SCHEMA {
            return "creates".into();
        }
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
            (World::Net, _) if crate::knowledge::HTTP_METHODS.contains(&op) => format!("{}s", op.to_uppercase()),
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
            (World::Screen, Some(Dir::Read)) => "asks".into(),
            (World::Screen, _) => "prints".into(),
            (World::Figure, _) => "draws".into(),
            (World::Global, Some(Dir::Read)) => "reads".into(),
            (World::Global, _) => "changes".into(),
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
    /// The names of what it changes in place: each parameter (or `self`, `this`) a change is of, of whatever type.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub changed: Vec<String>,
    /// The signature gives a value back: a type of the code's, or what it gives on success
    /// (unwrapped from a `Result` or a `Promise`) is no unit. An opened body makes it from what
    /// its steps give.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub gives_back: bool,
    /// What it gives back on success is a value, not a unit: a value with no place of its own (a
    /// `str`) goes back to the step that called it. A signature that says nothing of it, where its
    /// language leaves that to the author, gives back a value where a caller uses what it gives.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub returns_value: bool,
    /// The places where it reads only what it wrote, its body and what it calls in the order they
    /// run (a store that inserts rows and then counts them): what it reads there is no input, so
    /// the place is in its codomain only.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub reads_back: Vec<String>,
    /// The places where its own body reads only what its own body wrote, apart from what it calls.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub own_reads_back: Vec<String>,
    /// The places where it writes after it read what was there: it changes what it found (a
    /// register's next version, a cache of results). A place it writes only before it reads there,
    /// and only in an arm (a file downloaded where it is missing), it does not change: what it
    /// reads there is its input, made sure of.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub updates: Vec<String>,
    /// For a class: the initializer building a value of it runs (`model::initializer_along`), its
    /// own or a base's. A step that builds the class runs that body, and none of its methods.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub runs: String,
    /// What it does at places where it runs on a value of a known class, by the class, where that differs from what it
    /// does on a value of any class it may be: there a `super()` call, or a call on its receiver, runs one method.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub on: BTreeMap<String, Orders>,
}

/// What a body does at places as a route reads it: where it reads only what it wrote, and where it writes on what it found
/// (`Arrow::reads_back`, `Arrow::updates`).
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct Orders {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub reads_back: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub updates: Vec<String>,
}

/// What a function does at one place, in the order it runs: whether it reads what was there before
/// it ran, whether it reads what it wrote, whether it writes, whether it writes whenever it runs,
/// and whether it writes after it read what was there.
#[derive(Clone, Copy, Default, PartialEq)]
struct Order {
    finds: bool,
    reads_back: bool,
    writes: bool,
    always_writes: bool,
    updates: bool,
}

/// What every function does at each place, worked out once each round. A call back into a
/// function being worked out gives what that function did the round before, and the rounds go on
/// until nothing changes: a recursive cycle is the same whichever of its functions is worked out
/// first.
#[derive(Default)]
struct Memo {
    done: HashMap<String, BTreeMap<String, Order>>,
    before: HashMap<String, BTreeMap<String, Order>>,
    open: HashSet<String>,
}

/// The most rounds a recursive cycle is worked out in.
const ROUNDS: usize = 8;

/// What `id` does at each place, run on a value of class `on` (none where that is not known), its own touches and its
/// calls in line order (`Memo`, by the body's `key` and the callers above it that make a path whole): on a known class a
/// call on the body's receiver or a `super()` call runs the one method that class runs. Its calls in a loop run again
/// after what follows them there. A read reads back what was written when the write ran whenever it did:
/// unconditionally, or in an arm of a choice the read is in too; a write in one arm says nothing of a read outside it.
/// Without `done`, its own body alone, its calls left out. `above` is the chain of calls that leads to it from the
/// function whose arrow is worked out: a touch a caller makes whole is at that caller's place where the caller is on
/// the chain, and as it was read where none is (`Node::touches_in`, mathematics.md rule 5), so a read reads back only
/// what was written at the place the route touches.
fn orders(ix: &Ix, id: &str, on: &str, above: &[&str], mut done: Option<&mut Memo>) -> BTreeMap<String, Order> {
    let chain: Vec<&str> = above.iter().copied().chain(std::iter::once(id)).collect();
    // which callers on the chain make a path whole that `id` or what it runs touches, as a set, and the chain's entry
    // where its users make one whole: what the chain changes below `id`, so a recursive cycle comes back to the same
    // key and a body is worked out once for every chain that touches alike in it
    let consulted = ix.consulted(id, on);
    let made: Vec<&str> = chain.iter().copied().filter(|f| consulted.contains(f)).collect::<BTreeSet<_>>().into_iter().collect();
    let entry = consulted.contains(chain[0]).then_some(chain[0]).unwrap_or("");
    let k = if made.is_empty() { key(id, on) } else { format!("{}|{}|{entry}", key(id, on), made.join(",")) };
    if let Some(m) = done.as_deref_mut() {
        if let Some(o) = m.done.get(&k) {
            return o.clone();
        }
        if !m.open.insert(k.clone()) {
            return m.before.get(&k).cloned().unwrap_or_default();
        }
    }
    let Some(&n) = ix.nodes.get(id) else { return BTreeMap::new() };
    // the innermost arm a line is in
    let arm = |line: u32| n.arm_lines.iter().copied().filter(|&(a, b)| a <= line && line <= b).min_by_key(|&(a, b)| b - a);
    let within = |inner: Option<(u32, u32)>, outer: (u32, u32)| inner.map_or(false, |(a, b)| outer.0 <= a && b <= outer.1);
    // two lines in different arms of one choice never run in one run of the body
    let around = |line: u32| n.arm_lines.iter().zip(&n.arm_choices).filter(move |((a, b), _)| *a <= line && line <= *b);
    let apart = |x: u32, y: u32| around(x).any(|(r, k)| around(y).any(|(s, j)| k == j && r != s));
    // two lines in one loop: what the first does runs again after the second
    let looped = |x: u32, y: u32| n.loop_lines.iter().any(|&(a, b)| a <= x.min(y) && x.max(y) <= b);
    enum Event<'n> {
        Touch(&'n Touch),
        /// A call: what it may run, one of them, each on the class it runs on, and whether it stands in an arm.
        Call(Vec<(String, String)>, bool),
    }
    // a place's shape is no content: what is read after it was made is what was there
    // a touch that stands for several calls runs on the line of each
    // a call whose place a caller makes whole is read as written where no such caller runs (`instead`): at each place
    let mut events: Vec<(u32, Event)> = n
        .touches_in(chain[0], &|f: &str| chain.contains(&f))
        .into_iter()
        .filter(|t| t.dir.is_some() && !t.shape())
        .flat_map(|t| std::iter::once(t.line).chain(t.also_at.iter().copied()).map(move |l| (l, Event::Touch(t))))
        .collect();
    if done.is_some() {
        events.extend(n.sites.iter().filter(|s| !s.to.is_empty()).map(|s| (s.line, Event::Call(ix.may_run(s, on), !s.arms.is_empty()))));
    }
    // on one line what is read comes first, then the calls, then what is written: `x ??= v` reads
    // `x` before it may write it, `y = f(x)` calls `f` before `y` is written
    let rank = |e: &Event| match e {
        Event::Touch(t) if t.dir == Some(Dir::Read) => 0,
        Event::Call(..) => 1,
        Event::Touch(_) => 2,
    };
    events.sort_by_key(|(line, e)| (*line, rank(e)));
    // per place: what is done there, and the arms its conditional writes ran in
    let mut out: BTreeMap<String, Order> = BTreeMap::new();
    let mut written_in: HashMap<String, Vec<(u32, u32)>> = HashMap::new();
    // per place: the lines it is written on
    let mut written_at: HashMap<String, Vec<u32>> = HashMap::new();
    for (line, e) in events {
        let at = arm(line);
        let happened: Vec<(String, Order)> = match &e {
            Event::Touch(t) => {
                let writes = t.dir == Some(Dir::Write);
                vec![(t.place().id, Order { finds: !writes, writes, always_writes: writes, ..Order::default() })]
            }
            Event::Call(to, _) => {
                one_of(to.iter().map(|(t, c)| orders(ix, t, c, &chain, done.as_deref_mut())).collect()).into_iter().collect()
            }
        };
        let conditional = match e {
            Event::Touch(_) => at.is_some(),
            Event::Call(_, in_arm) => in_arm || at.is_some(),
        };
        for (place, b) in happened {
            let lines = written_at.entry(place.clone()).or_default();
            let arms = written_in.entry(place.clone()).or_default();
            let o = out.entry(place).or_default();
            // what it finds was written before it, when that write runs whenever the read does
            let back = o.always_writes || arms.iter().any(|w| within(at, *w));
            // a write after what was found changes it; a call that changes what it finds changes what this body wrote
            // there, when that is what it finds
            o.updates |= (b.updates && !back) || (b.writes && o.finds);
            if b.finds {
                if back {
                    o.reads_back = true;
                } else {
                    o.finds = true;
                    // a write in another arm of a choice the read is in never runs before it, and one in a loop
                    // the read is in runs again after it: the body changes what it found
                    o.updates |= lines.iter().any(|&w| apart(w, line) || looped(w, line));
                    // a call in a loop that writes there runs again after what it found: its next round writes over it
                    o.updates |= b.writes && looped(line, line);
                }
            }
            if b.writes {
                lines.push(line);
            }
            o.reads_back |= b.reads_back;
            o.writes |= b.writes;
            if b.always_writes {
                match at.filter(|_| conditional) {
                    None => o.always_writes = true,
                    Some(w) => arms.push(w),
                }
            }
        }
    }
    if let Some(m) = done {
        m.open.remove(&k);
        m.done.insert(k, out.clone());
    }
    out
}

/// What one of several functions does at each place, where a call runs one of them, as the arms of a choice: what any
/// of them finds, reads back, writes or changes there, written whenever the call runs only where all of them write it.
fn one_of(runs: Vec<BTreeMap<String, Order>>) -> BTreeMap<String, Order> {
    let all = runs.len();
    let mut out: BTreeMap<String, (Order, usize)> = BTreeMap::new();
    for run in runs {
        for (place, b) in run {
            let (o, always) = out.entry(place).or_default();
            o.finds |= b.finds;
            o.reads_back |= b.reads_back;
            o.writes |= b.writes;
            o.updates |= b.updates;
            *always += usize::from(b.always_writes);
        }
    }
    out.into_iter().map(|(p, (o, always))| (p, Order { always_writes: always == all, ..o })).collect()
}

/// What `id` does at places in each context its arrow stands in, together: with no caller above it that makes a path
/// whole, and under each such caller a touch it reaches names (`Touch::by`). In one context every touch is at one place
/// (`orders`); an arrow stands in all of them, so it reads back, or changes, what it does in any one.
fn in_every_context(
    ix: &Ix,
    id: &str,
    on: &str,
    own: bool,
    done: &mut Memo,
    of: fn(BTreeMap<String, Order>) -> Vec<String>,
) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for above in contexts(ix, id) {
        let o = if own { orders(ix, id, on, &above, None) } else { orders(ix, id, on, &above, Some(&mut *done)) };
        for p in of(o) {
            if !out.contains(&p) {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// The callers above `id` that may make a path it reaches whole: none, and each set a touch it reaches names in `by`.
fn contexts<'g>(ix: &Ix<'g>, id: &str) -> Vec<Vec<&'g str>> {
    let mut out: Vec<Vec<&'g str>> = vec![vec![]];
    let mut seen: HashSet<&str> = HashSet::new();
    let mut stack: Vec<&str> = vec![id];
    while let Some(f) = stack.pop() {
        if !seen.insert(f) {
            continue;
        }
        let Some(&n) = ix.nodes.get(f) else { continue };
        for t in &n.touches {
            for by in &t.by {
                let mut makers: Vec<&'g str> = by.split('&').filter_map(|m| ix.nodes.get_key_value(m).map(|(k, _)| *k)).collect();
                makers.sort();
                if !makers.is_empty() && !out.contains(&makers) {
                    out.push(makers);
                }
            }
        }
        stack.extend(
            ix.out
                .get(f)
                .into_iter()
                .flatten()
                .filter(|e| matches!(e.kind, EdgeKind::Calls | EdgeKind::Instantiates))
                .map(|e| e.dst.as_str()),
        );
    }
    out
}

/// The places where what is read is only what was written.
fn reads_back(orders: BTreeMap<String, Order>) -> Vec<String> {
    orders.into_iter().filter(|(_, o)| o.reads_back && !o.finds).map(|(p, _)| p).collect()
}

/// The places where what was found is written on.
fn updates(orders: BTreeMap<String, Order>) -> Vec<String> {
    orders.into_iter().filter(|(_, o)| o.updates).map(|(p, _)| p).collect()
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

    /// The initializer building a value of the class runs (`model::initializer_along`): its own or,
    /// when it has none, a base's.
    fn runs(&self, class: &str) -> Option<String> {
        crate::model::initializer_along(&self.mro(class), |id| self.nodes.get(id).copied())
    }

    /// What building a class takes: the parameters of the initializer it runs.
    fn built_from(&self, class: &str) -> Vec<String> {
        self.runs(class).map_or(vec![], |c| self.targets(&c, EdgeKind::Takes))
    }

    /// The functions a call in a body run on class `on` may run, one of them, each with the class it runs on, as
    /// `body_of` reads it: on a known class (the one the call names, or the body's own for a call on its receiver) the
    /// method that class runs for it, or the next method of a `super()` call along its order; where the class is not
    /// known, the next method of a `super()` call along each class it may be, or the method a call by name reaches and
    /// its overrides.
    fn may_run(&self, s: &Site, on: &str) -> Vec<(String, String)> {
        // building a class runs its initializer along its order, on the class it builds; a class built from its fields
        // runs nothing
        if self.nodes.get(s.to.as_str()).is_some_and(|c| c.kind == NodeKind::Class) {
            return self.runs(&s.to).map(|i| vec![(i, s.to.clone())]).unwrap_or_default();
        }
        let Some(m) = self.nodes.get(s.to.as_str()).filter(|m| m.cls.is_some()) else { return vec![(s.to.clone(), String::new())] };
        let runs_on = if !s.on.is_empty() {
            s.on.as_str()
        } else if s.on_receiver {
            on
        } else {
            ""
        };
        let at = |f: Option<String>, c: &str| (f.unwrap_or_else(|| s.to.clone()), c.to_string());
        let mut out: Vec<(String, String)> = if !s.after.is_empty() && !runs_on.is_empty() {
            vec![at(self.method_after(runs_on, &s.after, &m.name), runs_on)]
        } else if !s.after.is_empty() {
            std::iter::once(at(self.method_after(&s.after, &s.after, &m.name), ""))
                .chain(self.below(&s.after).iter().filter_map(|d| self.method_after(d, &s.after, &m.name).map(|r| (r, d.clone()))))
                .collect()
        } else if s.named {
            vec![(s.to.clone(), runs_on.to_string())]
        } else if !runs_on.is_empty() {
            vec![at(self.method_on(runs_on, &m.name), runs_on)]
        } else {
            std::iter::once((s.to.clone(), String::new()))
                .chain(self.overrides(m).iter().map(|k| {
                    let (f, c) = split(k);
                    (f.to_string(), c.to_string())
                }))
                .collect()
        };
        let mut seen = HashSet::new();
        out.retain(|f| seen.insert(f.clone()));
        out
    }
}

fn arrow_of(ix: &Ix, n: &Node, done: &mut Memo) -> Arrow {
    if n.kind == NodeKind::Class {
        return Arrow {
            takes: ix.built_from(&n.id),
            gives: vec![n.id.clone()],
            runs: ix.runs(&n.id).unwrap_or_default(),
            ..Default::default()
        };
    }
    // a module's own code runs where it is a program (`if __name__ == "__main__":`): what it
    // reads back of what it wrote
    if n.kind == NodeKind::Module {
        return Arrow {
            reads_back: in_every_context(ix, &n.id, "", false, done, reads_back),
            own_reads_back: in_every_context(ix, &n.id, "", true, done, reads_back),
            updates: in_every_context(ix, &n.id, "", false, done, updates),
            ..Default::default()
        };
    }
    let ctor = n.ctor;
    let gives = ix.targets(&n.id, EdgeKind::Returns);
    // a function whose signature does not say what it gives back gives back a value where its callers use one
    let returns_value = n.returns.as_ref().map_or(false, |r| !r.value.is_empty()) || (ix.unsaid(n) && ix.used(&n.id));
    Arrow {
        receiver: n.cls.clone().filter(|_| !ctor && !n.on_class).unwrap_or_default(),
        takes: ix.targets(&n.id, EdgeKind::Takes),
        gives_back: !gives.is_empty() || returns_value,
        gives,
        builds: if ctor { n.cls.clone().unwrap_or_default() } else { String::new() },
        changes: ix.targets(&n.id, EdgeKind::Mutates),
        changed: {
            let mut names: Vec<String> = vec![];
            for v in ix.out(&n.id, EdgeKind::Mutates).filter_map(|e| e.via.clone()) {
                if !names.contains(&v) {
                    names.push(v);
                }
            }
            names
        },
        returns_value,
        reads_back: in_every_context(ix, &n.id, "", false, done, reads_back),
        own_reads_back: in_every_context(ix, &n.id, "", true, done, reads_back),
        updates: in_every_context(ix, &n.id, "", false, done, updates),
        runs: String::new(),
        on: BTreeMap::new(),
    }
}

/// The arrow of every function, method and class, and of a module, where its own code runs; of a method, also what it
/// does at places on each class a route runs it on (`Arrow::on`).
pub fn arrows(g: &Graph) -> BTreeMap<String, Arrow> {
    arrows_in(g, &mut Memo::default())
}

/// The arrows, worked out in `done`.
fn arrows_in(g: &Graph, done: &mut Memo) -> BTreeMap<String, Arrow> {
    let ix = Ix::new(g);
    let kinds = |n: &&Node| matches!(n.kind, NodeKind::Module | NodeKind::Class | NodeKind::Function | NodeKind::Method);
    // the bodies a route runs on a known class
    let on: Vec<(String, String)> = bodies(g)
        .into_keys()
        .filter_map(|k| {
            let (f, c) = split(&k);
            (!c.is_empty()).then(|| (f.to_string(), c.to_string()))
        })
        .collect();
    for round in 1..=ROUNDS {
        for n in g.nodes.iter().filter(kinds) {
            orders(&ix, &n.id, "", &[], Some(&mut *done));
        }
        for (f, c) in &on {
            orders(&ix, f, c, &[], Some(&mut *done));
        }
        if done.done == done.before || round == ROUNDS {
            break;
        }
        done.before = std::mem::take(&mut done.done);
    }
    let mut out: BTreeMap<String, Arrow> = g.nodes.iter().filter(kinds).map(|n| (n.id.clone(), arrow_of(&ix, n, &mut *done))).collect();
    for (f, c) in &on {
        let at = Orders {
            reads_back: in_every_context(&ix, f, c, false, &mut *done, reads_back),
            updates: in_every_context(&ix, f, c, false, &mut *done, updates),
        };
        if let Some(a) = out.get_mut(f).filter(|a| a.reads_back != at.reads_back || a.updates != at.updates) {
            a.on.insert(c.clone(), at);
        }
    }
    out.retain(|_, a| *a != Arrow::default());
    out
}

/// For each class, the initializer building a value of it runs (`Arrow::runs`), its own or a base's.
pub fn initializers(g: &Graph) -> HashMap<String, String> {
    let ix = Ix::new(g);
    g.nodes.iter().filter(|n| n.kind == NodeKind::Class).filter_map(|n| ix.runs(&n.id).map(|i| (n.id.clone(), i))).collect()
}

/// What a body's steps make that it may give back: the class a step builds, else what the function it runs gives back,
/// where that is of a type the function itself gives (`n`'s `returns`, a kind or a variant of one, or a type its written
/// return holds as a value as its language reads it: the error of a `Result<T, E>`, `T + E`), or an error its own calls
/// raise or hand on. A value of another type its steps make is no part of what it gives back: a function that gives back a
/// number is made of none of them, and one that gives back a collection of pairs (`Vec<(String, Vec<Where>)>`) gives back
/// pairs of a type the code does not name, none of what they pair. A signature that says nothing of what it gives back,
/// whose callers use what it gives (`Arrow::returns_value`), rules out no type.
fn made_from(ix: &Ix, n: &Node, body: &Body) -> Vec<String> {
    let mut gives = ix.targets(&n.id, EdgeKind::Returns);
    for s in &n.sites {
        let errors = s.gives.iter().filter(|_| s.raises).map(|g| g.ty.clone()).chain(s.fails.clone());
        for e in errors.filter(|e| !e.is_empty()) {
            if !gives.contains(&e) {
                gives.push(e);
            }
        }
    }
    let holds: HashSet<&str> = n.returns.iter().flat_map(|r| &r.holds).map(String::as_str).collect();
    let names = |t: &str| ix.nodes.get(t).map_or(false, |c| holds.contains(c.name.as_str()));
    // a signature that says nothing of what it gives back rules out no type: every type is one of the unchecked one's
    let unsaid = ix.unsaid(n) && ix.used(&n.id);
    let of = |t: &str| {
        unsaid
            || names(t)
            || gives.iter().any(|g| t == g || ix.mro(t).contains(&g.as_str()) || ix.out(g, EdgeKind::Variant).any(|e| e.dst == t))
    };
    let mut callees: Vec<&str> = vec![];
    for t in body.steps().into_iter().map(|s| s.to.as_str()).chain(body.alts.iter().map(|k| split(k).0)) {
        if ix.nodes.contains_key(t) && !callees.contains(&t) {
            callees.push(t);
        }
    }
    let mut out = vec![];
    for c in callees {
        let made = if ix.nodes[c].kind == NodeKind::Class { vec![c.to_string()] } else { ix.targets(c, EdgeKind::Returns) };
        out.extend(made.into_iter().filter(|t| of(t)));
    }
    out
}

// ---------- bodies: what a step runs, the same for the agent and the page ----------

/// One step of a body as a route runs it: the function it reaches, or a library's operation
/// (`op:to_numpy`); the class of the value it runs on, when the call says; and the call in the
/// body's `sites` it is, when the body lists its calls.
#[derive(Serialize, Clone, Debug, PartialEq, Default)]
pub struct Step {
    pub to: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub on: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub site: Option<usize>,
    /// The call says which method it runs: through a class (`Cached.read(self)`), or through
    /// `super()` on a value whose class is known. No override of it runs in its place, and it is
    /// named by its own class, not by the class of the value.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub named: bool,
    /// What a `super()` call may run in place of `to` where the class of the value is not known,
    /// by the key of each body: the next method after the class it is written in, along the order
    /// of each class the value may be, on that class.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub instead: Vec<String>,
}

impl Step {
    /// A step that runs `to` as a call of it by name does.
    pub fn of(to: &str) -> Self {
        Step { to: to.to_string(), ..Default::default() }
    }

    /// What the call may run in place of `to`, by the key of each body: what its call says, or,
    /// for a method called by name on a value whose class is not known, the overrides `bodies`
    /// gives its body.
    pub fn alts<'b>(&'b self, bodies: &'b BTreeMap<String, Body>) -> &'b [String] {
        match bodies.get(&self.to) {
            _ if self.named => &self.instead,
            Some(b) if self.on.is_empty() => &b.alts,
            _ => &[],
        }
    }

    /// What the call may run in place of `to`, each as a step: the function of each body on the
    /// class of its key. Where the call names its method, each is named too: the next method of a
    /// `super()` call is named by its own class.
    pub fn runs_instead(&self, bodies: &BTreeMap<String, Body>) -> Vec<Step> {
        self.alts(bodies)
            .iter()
            .map(|k| {
                let (to, on) = split(k);
                Step { to: to.to_string(), on: on.to_string(), named: self.named, ..Default::default() }
            })
            .collect()
    }
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
    /// The arms of one choice that hold steps, each a branch, and what decides each branch, by
    /// branch, as the code writes it. A choice with one such arm is still a choice: its steps run
    /// when it holds, and nothing runs otherwise.
    Choice {
        choice: u32,
        members: Vec<Member>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        when: Vec<String>,
    },
}

/// What running a function means, on a value of a known class or not: the overrides a call of it
/// by name may run instead (a call to `Language.parse` runs `Python.parse` or `Rust.parse`, unless
/// the class is known; `Step::alts`), and its own steps.
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct Body {
    /// The overrides, by the key of the body each runs (`Ix::overrides`).
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
    if on.is_empty() {
        to.to_string()
    } else {
        format!("{to}@{on}")
    }
}

/// The function of a body's key and the class it runs on, none where that is not known.
pub fn split(key: &str) -> (&str, &str) {
    key.split_once('@').unwrap_or((key, ""))
}

/// The edges of a graph by their ends, in the order the graph holds them.
struct Ix<'g> {
    nodes: HashMap<&'g str, &'g Node>,
    out: HashMap<&'g str, Vec<&'g Edge>>,
    into: HashMap<&'g str, Vec<&'g Edge>>,
    /// Each class's method resolution order, worked out once.
    mro: RefCell<HashMap<&'g str, Vec<&'g str>>>,
    /// What each part's language leaves its author to write, by the part's id, the first part's for a node of none.
    leaves: Vec<(&'g str, Leaves)>,
    /// The callers a touch of each body or of what it may run is made whole by (`Touch::by`, `Touch::users_of`), by the
    /// body's key: the only functions on a chain of calls that change what it touches below them.
    consulted: RefCell<HashMap<String, Rc<BTreeSet<&'g str>>>>,
}

impl<'g> Ix<'g> {
    fn new(g: &'g Graph) -> Self {
        let mut out: HashMap<&str, Vec<&Edge>> = HashMap::new();
        let mut into: HashMap<&str, Vec<&Edge>> = HashMap::new();
        for e in &g.edges {
            out.entry(e.src.as_str()).or_default().push(e);
            into.entry(e.dst.as_str()).or_default().push(e);
        }
        let leaves = g.parts.iter().map(|p| (p.id.as_str(), p.leaves)).collect();
        Ix {
            nodes: g.nodes.iter().map(|n| (n.id.as_str(), n)).collect(),
            out,
            into,
            mro: RefCell::default(),
            leaves,
            consulted: RefCell::default(),
        }
    }

    /// The callers that make a touch whole in body `id` run on class `on` or in any body its calls may run (`may_run`),
    /// through every call: worked out once a body, a body already worked out answering for all it runs.
    fn consulted(&self, id: &str, on: &str) -> Rc<BTreeSet<&'g str>> {
        let root = key(id, on);
        if let Some(c) = self.consulted.borrow().get(&root) {
            return c.clone();
        }
        let mut out: BTreeSet<&'g str> = BTreeSet::new();
        let mut seen: HashSet<String> = HashSet::from([root.clone()]);
        let mut stack: Vec<(String, String)> = vec![(id.to_string(), on.to_string())];
        while let Some((f, c)) = stack.pop() {
            let k = key(&f, &c);
            if k != root {
                if let Some(done) = self.consulted.borrow().get(&k) {
                    out.extend(done.iter().copied());
                    continue;
                }
            }
            let Some(&n) = self.nodes.get(f.as_str()) else { continue };
            for t in &n.touches {
                out.extend(t.by.iter().flat_map(|b| b.split('&')).chain(t.users_of.iter().map(String::as_str)));
            }
            for s in n.sites.iter().filter(|s| !s.to.is_empty()) {
                for (t, tc) in self.may_run(s, &c) {
                    if seen.insert(key(&t, &tc)) {
                        stack.push((t, tc));
                    }
                }
            }
        }
        let out = Rc::new(out);
        self.consulted.borrow_mut().insert(root, out.clone());
        out
    }

    /// Whether `n`'s signature does not say what it gives back: its language leaves that to the author, and the author
    /// left it out or wrote the unchecked type. A constructor gives its class.
    fn unsaid(&self, n: &Node) -> bool {
        let leaves = self.leaves.iter().find(|(p, _)| *p == n.part).or(self.leaves.first()).map_or(Leaves::default(), |(_, l)| *l);
        matches!(n.kind, NodeKind::Function | NodeKind::Method)
            && leaves.returns
            && !n.ctor
            && !n.returns.as_ref().map_or(false, crate::model::Returns::says)
    }

    /// Whether a caller uses what a call of `id` gives back.
    fn used(&self, id: &str) -> bool {
        self.into.get(id).into_iter().flatten().any(|e| e.kind == EdgeKind::Calls && e.uses)
    }

    fn out(&self, id: &str, kind: EdgeKind) -> impl Iterator<Item = &'g Edge> + '_ {
        self.out.get(id).into_iter().flatten().copied().filter(move |e| e.kind == kind)
    }

    /// What a call to a method by name may run in its place, on a value whose class is not known,
    /// by the key of the body each runs: for each class the value may be, the method's own or one
    /// below it, the method that class runs for the name along its order (`method_on`), each once,
    /// on the values that run it. A method of a class below the method's own runs on any value of
    /// its class, all of which the call may be handed: its body is read where the class is not
    /// known. One of a class beside it runs on the classes below both, the class that runs it: in a
    /// diamond `D(B, C)` over `A` a call of `C.f` may run `B.f` on a `D`, though `B` is not below
    /// `C`, and there its `super().f()` runs `C.f`.
    fn overrides(&self, n: &Node) -> Vec<String> {
        let Some(cls) = n.cls.as_deref() else { return vec![] };
        let below = self.below(cls);
        let mut out: Vec<String> = vec![];
        for c in &below {
            let Some(m) = self.method_on(c, &n.name).filter(|m| *m != n.id) else { continue };
            let of = self.nodes.get(m.as_str()).and_then(|m| m.cls.as_deref()).unwrap_or_default();
            let k = if of == cls || below.iter().any(|b| b == of) { m } else { key(&m, c) };
            if !out.contains(&k) {
                out.push(k);
            }
        }
        out
    }

    /// The classes below `cls`, each once, nearest first along each line of descent.
    fn below(&self, cls: &str) -> Vec<String> {
        let (mut out, mut stack, mut seen) = (vec![], vec![cls.to_string()], HashSet::from([cls.to_string()]));
        while let Some(c) = stack.pop() {
            for e in self.into.get(c.as_str()).into_iter().flatten().filter(|e| e.kind == EdgeKind::Inherits) {
                if seen.insert(e.src.clone()) {
                    stack.push(e.src.clone());
                    out.push(e.src.clone());
                }
            }
        }
        out
    }

    /// The method a value of class `on` runs for `name`: the first along its method resolution
    /// order that defines it, its class's own before a base's.
    fn method_on(&self, on: &str, name: &str) -> Option<String> {
        self.mro(on).into_iter().map(|c| format!("{c}.{name}")).find(|m| self.nodes.contains_key(m.as_str()))
    }

    /// The method a `super()` call written in class `after` runs for `name` on a value of class
    /// `on`: the first along `on`'s method resolution order after `after` that defines it.
    fn method_after(&self, on: &str, after: &str, name: &str) -> Option<String> {
        self.mro(on)
            .into_iter()
            .skip_while(|c| *c != after)
            .skip(1)
            .map(|c| format!("{c}.{name}"))
            .find(|m| self.nodes.contains_key(m.as_str()))
    }

    /// The method resolution order along `inherits` (`model::linearization`): the class, then
    /// its ancestors nearest first.
    fn mro(&self, cls: &str) -> Vec<&'g str> {
        let Some(cls) = self.nodes.get_key_value(cls).map(|(k, _)| *k).or_else(|| self.out.get_key_value(cls).map(|(k, _)| *k)) else {
            return vec![];
        };
        let bases = |c: &'g str| self.out(c, EdgeKind::Inherits).map(|e| e.dst.as_str()).collect();
        crate::model::linearization(cls, &bases, &mut self.mro.borrow_mut())
    }
}

/// The steps in call order, the steps of each choice together, by branch, with what decides each
/// branch (`whens`, as `Node::whens` holds them).
fn grouped(steps: Vec<(Step, Vec<(u32, u32)>)>, whens: &[(u32, u32, String)]) -> Vec<Item> {
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
        if placed.insert(k) {
            let members =
                group.iter().map(|(s, a)| Member { step: s.clone(), branch: paths.iter().position(|p| *p == a).unwrap_or(0) }).collect();
            // a branch runs when every arm on its path does: an `except` inside an `if`
            let when: Vec<String> = paths
                .iter()
                .map(|p| {
                    p.iter()
                        .filter_map(|(c, a)| whens.iter().find(|w| w.0 == *c && w.1 == *a).map(|w| w.2.as_str()))
                        .collect::<Vec<_>>()
                        .join(" · ")
                })
                .collect();
            let when = if when.iter().all(String::is_empty) { vec![] } else { when };
            items.push(Item::Choice { choice: k, members, when });
        }
    }
    items
}

/// What a function runs on a value of class `on`: its calls as the body lists them, each method
/// call on a known class that class's method, a call through a class the method it names, and a
/// `super()` call the next method after its class along the value's order; or, where the body
/// lists no calls, what the call graph says. A library class (an export) runs its constructor and
/// its public methods.
fn body_of(ix: &Ix, n: &Node, on: &str) -> Body {
    let alts = if on.is_empty() { ix.overrides(n) } else { vec![] };
    if !n.sites.is_empty() {
        let steps = n
            .sites
            .iter()
            .enumerate()
            .filter_map(|(i, s)| {
                let runs_on = if !s.on.is_empty() {
                    s.on.as_str()
                } else if s.on_receiver {
                    on
                } else {
                    ""
                };
                let mut to = match ix.nodes.get(s.to.as_str()) {
                    _ if s.to.is_empty() => format!("op:{}", s.op),
                    Some(_) => s.to.clone(),
                    // a call into a library's code the graph does not hold is the library's
                    // operation: a step where the body keeps what it gives
                    None if !s.gives.is_empty() => format!("op:{}", s.to.rsplit('.').next().unwrap_or(&s.to)),
                    None => return None,
                };
                let method = ix.nodes.get(s.to.as_str()).filter(|m| m.cls.is_some());
                let mut instead = vec![];
                match method {
                    // a `super()` call runs the next method after its class along the order of
                    // the value's; where that class is not known, along the order of each class it
                    // may be, the class the call is written in or one below it, on that class
                    Some(m) if !s.after.is_empty() && !runs_on.is_empty() => to = ix.method_after(runs_on, &s.after, &m.name).unwrap_or(to),
                    Some(m) if !s.after.is_empty() => {
                        to = ix.method_after(&s.after, &s.after, &m.name).unwrap_or(to);
                        for d in ix.below(&s.after) {
                            if let Some(r) = ix.method_after(&d, &s.after, &m.name).filter(|r| *r != to).map(|r| key(&r, &d)) {
                                if !instead.contains(&r) {
                                    instead.push(r);
                                }
                            }
                        }
                    }
                    Some(m) if !s.named && !runs_on.is_empty() => to = ix.method_on(runs_on, &m.name).unwrap_or(to),
                    _ => {}
                }
                let named = method.is_some() && (s.named || !s.after.is_empty());
                Some((Step { to, on: runs_on.to_string(), site: Some(i), named, instead }, s.arms.clone()))
            })
            .collect();
        return Body { alts, items: grouped(steps, &n.whens), made_from: vec![] };
    }
    let callees: Vec<String> = if n.kind == NodeKind::Class && n.entry.is_some() {
        let ctor = ix.runs(&n.id);
        let mut out: Vec<String> = ctor.iter().cloned().collect();
        // the initializer building it runs, its own or a base's, and its public methods, the initializer once
        out.extend(
            ix.out(&n.id, EdgeKind::Contains)
                .map(|e| e.dst.clone())
                .filter(|m| Some(m) != ctor.as_ref())
                .filter(|m| ix.nodes.get(m.as_str()).map_or(false, |m| m.kind == NodeKind::Method && !m.private)),
        );
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
    for e in ix
        .out
        .get(n.id.as_str())
        .into_iter()
        .flatten()
        .filter(|e| matches!(e.kind, EdgeKind::Calls | EdgeKind::Instantiates) && !e.arms.is_empty())
    {
        arms.entry(e.dst.as_str()).or_insert_with(|| e.arms.clone());
    }
    let steps = callees.iter().map(|c| (Step::of(c), arms.get(c.as_str()).cloned().unwrap_or_default())).collect();
    Body { alts, items: grouped(steps, &n.whens), made_from: vec![] }
}

/// The body of every function, and of every function on each class a step or what it may run
/// instead runs it on, by `key`: what a route through it runs, for whoever reads the route. A
/// Makefile target runs the targets and commands its recipe names.
pub fn bodies(g: &Graph) -> BTreeMap<String, Body> {
    let ix = Ix::new(g);
    let mut out = BTreeMap::new();
    for t in &g.make {
        let items: Vec<Item> = t
            .calls
            .iter()
            .filter(|c| ix.nodes.contains_key(c.as_str()) || g.make.iter().any(|m| &m.id == *c))
            .map(|c| Item::Step(Step::of(c)))
            .collect();
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
        for k in body.alts.iter().chain(body.steps().into_iter().flat_map(|s| s.instead.iter())) {
            let (to, on) = split(k);
            if !on.is_empty() {
                todo.push((to.to_string(), on.to_string()));
            }
        }
        body.made_from = made_from(&ix, n, &body);
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
        Held { at, ..Held::new(ty, name) }
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
        assert_eq!(
            (sites[0].hands.clone(), sites[0].gives.clone()),
            (vec![Held::new("Options", "opts")], vec![Held::new("Language", "lang")])
        );
    }

    /// `f(g(x), y)`: `g` takes `x`, `f` takes what `g` gives and `y`.
    #[test]
    fn composition_is_wired() {
        let mut sites = vec![
            site("g", "", (2, 6), vec![at("X", "x", 4)], vec![]),
            site("f", "", (0, 10), vec![at("X", "x", 4), at("Y", "y", 8)], vec![]),
        ];
        nest(&mut sites, &HashMap::from([("g".to_string(), "Z".to_string())]), &mut 0);
        assert_eq!(sites[0].gives, [Held::new("Z", "#0")]);
        assert_eq!(sites[1].hands, [Held::new("Y", "y"), Held::new("Z", "#0")]);
    }

    /// Two commands that start with the same words are two places, and two touches of a body:
    /// a place is named by all of its text, and only its label is cut, where it is drawn.
    #[test]
    fn a_place_is_named_by_all_of_its_text() {
        let run = |what: &str| Touch { what: what.into(), ..Touch::new(World::Process, None) };
        let (since, until) = (run("git log --format=%H --since=yesterday"), run("git log --format=%H --until=today"));
        assert_ne!(since.place().id, until.place().id);
        assert_eq!(since.place().label, until.place().label, "both drawn as `git log --format=%H …`");
        let mut b = crate::builder::GraphBuilder::default();
        b.add_node(crate::model::Node { id: "f".into(), ..Default::default() });
        b.touch("f", since);
        b.touch("f", until);
        assert_eq!(b.nodes["f"].touches.len(), 2);
    }

    /// A place one arm of a choice writes and another reads is found and changed: the write never runs before
    /// the read. Written in an arm and read after the choice, it is made sure of; written in one choice and read
    /// in a later one, the write may run first. Neither of those changes what was found.
    #[test]
    fn arms_of_one_choice_never_run_one_after_the_other() {
        let updated = |arms: Vec<((u32, u32), u32)>, read: u32| {
            let touch = |dir, line| Touch { artifact: "a.csv".into(), line, ..Touch::new(World::Disk, Some(dir)) };
            let (arm_lines, arm_choices) = arms.into_iter().unzip();
            let f = crate::model::Node {
                id: "f".into(),
                kind: NodeKind::Function,
                touches: vec![touch(Dir::Write, 3), touch(Dir::Read, read)],
                arm_lines,
                arm_choices,
                ..Default::default()
            };
            let g = Graph { nodes: vec![f], ..Default::default() };
            arrows(&g).get("f").map_or(vec![], |a| a.updates.clone())
        };
        assert_eq!(updated(vec![((3, 3), 1), ((4, 4), 1)], 4), ["w:File:a.csv"], "two arms of one choice");
        assert!(updated(vec![((3, 3), 1)], 5).is_empty(), "made sure of before it is read");
        assert!(updated(vec![((3, 3), 1), ((5, 5), 2)], 5).is_empty(), "two choices, one after the other");
    }

    /// A function of `touches` (way, line) at `data/log.txt` and `calls` (callee, line), with the lines of its loops.
    fn worker(id: &str, touches: &[(Dir, u32)], calls: &[(&str, u32)], loops: Vec<(u32, u32)>) -> crate::model::Node {
        crate::model::Node {
            id: id.into(),
            kind: NodeKind::Function,
            touches: touches
                .iter()
                .map(|&(d, line)| Touch { artifact: "data/log.txt".into(), line, ..Touch::new(World::Disk, Some(d)) })
                .collect(),
            sites: calls.iter().map(|&(to, line)| Site { to: to.into(), line, ..Default::default() }).collect(),
            loop_lines: loops,
            ..Default::default()
        }
    }

    /// What a route does at a place composes from what its calls do there: a callee that reads back what its caller
    /// wrote changes nothing the route found; a read in a loop finds what a later round writes over; and a recursive
    /// cycle is the same whichever of its functions is defined first.
    #[test]
    fn what_a_route_does_at_a_place_composes() {
        use Dir::{Read, Write};
        let arrow = |nodes: Vec<crate::model::Node>, id: &str| {
            let g = Graph { nodes, ..Default::default() };
            arrows(&g).get(id).map(|a| (a.reads_back.clone(), a.updates.clone())).unwrap_or_default()
        };
        let log = || vec!["w:File:data/log.txt".to_string()];
        let bumped = vec![worker("run", &[(Write, 2)], &[("bump", 3)], vec![]), worker("bump", &[(Read, 6), (Write, 7)], &[], vec![])];
        assert_eq!(arrow(bumped, "run"), (log(), vec![]), "bump reads back what run wrote");
        let looped = || crate::model::Node {
            arm_lines: vec![(3, 3)],
            arm_choices: vec![1],
            ..worker("run", &[(Write, 3), (Read, 4)], &[], vec![(2, 4)])
        };
        assert_eq!(arrow(vec![looped()], "run").1, log(), "a later round writes over what this one read");
        let once = crate::model::Node { loop_lines: vec![], ..looped() };
        assert!(arrow(vec![once], "run").1.is_empty(), "without the loop the write in the arm only makes sure of it");
        let save = || worker("save", &[(Write, 2)], &[("show", 3)], vec![]);
        let show = || worker("show", &[(Read, 6)], &[("save", 7)], vec![]);
        let run = || worker("run", &[], &[("show", 10)], vec![]);
        let (a, b) = (arrow(vec![save(), show(), run()], "run"), arrow(vec![show(), save(), run()], "run"));
        assert_eq!(a, b, "the order of definitions");
        assert_eq!(a.1, log(), "show reads the log as it was, then save writes on it");
    }

    /// A body is worked out once for every set of callers above it that change what it touches: a caller that makes
    /// a path whole for another function is no reason to work out again what runs below it. Along a chain of such
    /// callers down to one writer, the writer is worked out once, and the work grows with the chain, not with the
    /// sets of callers on it.
    #[test]
    fn a_body_is_worked_out_once_for_what_the_chain_changes_in_it() {
        let worked_out = |k: usize| {
            let mut nodes = vec![worker("leaf", &[(Dir::Write, 2)], &[], vec![])];
            for i in 0..k {
                let below = if i + 1 < k { format!("f{}", i + 1) } else { "leaf".to_string() };
                let made = Touch { artifact: format!("out{i}/"), by: vec![format!("f{i}")], ..Touch::new(World::Disk, Some(Dir::Write)) };
                nodes.push(crate::model::Node { touches: vec![made], ..worker(&format!("s{i}"), &[], &[], vec![]) });
                nodes.push(worker(&format!("f{i}"), &[], &[(&format!("s{i}"), 3), (&below, 4)], vec![]));
            }
            let g = Graph { nodes, ..Default::default() };
            let mut done = Memo::default();
            let arrows = arrows_in(&g, &mut done);
            let of = |id: &str| done.done.keys().filter(|k| k.split('|').next() == Some(id)).count();
            (of("leaf"), done.done.len(), arrows)
        };
        let (leaf, short, _) = worked_out(6);
        let (_, long, _) = worked_out(12);
        assert_eq!(leaf, 1, "no caller above the writer changes what it touches");
        assert!(long <= 2 * short + 4, "doubling the chain doubles the work: {short} bodies worked out for 6 callers, {long} for 12");
    }

    /// Two equal calls are one touch that runs on the line of each: a file seeded in an arm, read, and then
    /// always written on is found and changed, though the first write stands in the arm.
    #[test]
    fn a_touch_runs_on_the_line_of_every_call_it_stands_for() {
        let touch =
            |dir, op: &str, line| Touch { artifact: "data/note.txt".into(), op: op.into(), line, ..Touch::new(World::Disk, Some(dir)) };
        let mut b = crate::builder::GraphBuilder::default();
        b.add_node(crate::model::Node {
            id: "run".into(),
            kind: NodeKind::Function,
            arm_lines: vec![(3, 3)],
            arm_choices: vec![1],
            ..Default::default()
        });
        for t in [touch(Dir::Write, "write_text", 3), touch(Dir::Read, "read_text", 4), touch(Dir::Write, "write_text", 5)] {
            b.touch("run", t);
        }
        let n = b.nodes["run"].clone();
        assert_eq!(n.touches.len(), 2, "the two writes are one touch");
        assert_eq!(n.touches[0].also_at, [5]);
        let g = Graph { nodes: vec![n], ..Default::default() };
        assert_eq!(arrows(&g)["run"].updates, ["w:File:data/note.txt"], "written on after it was read");
    }

    /// `lang = detect(opts).unwrap()` read by a language that names the unwrapped value already:
    /// the arrow from `lang` to `lang` is the identity.
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

    /// An alternative is the method on the class that runs it (#155): in `cached_lines(source: Cached)` of the
    /// cooperative pair a cached source may be a `Feed(Trimmed, Cached)`, which runs `Trimmed.read`, so the call may run
    /// `Trimmed.read` on a feed, whose `super().read()` runs the next `read` along the feed's order: the clean side's
    /// `Cached.read`, which reads the store; the shortcut's `Source.read(self)` names `Source.read`. A `super()` call on a
    /// value whose class is not known may run the next method on each class the value may be: `Trimmed.read`'s
    /// `super().read()` runs `Cached.read` on a feed.
    #[test]
    fn an_alternative_is_the_method_on_the_class_that_runs_it() {
        let at = |side: &str| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/cooperative/python").join(side);
        let (trimmed, cached, source, feed) =
            ("news.sources.Trimmed.read", "news.sources.Cached.read", "news.sources.Source.read", "news.sources.Feed");
        for (side, next) in [("clean", cached), ("shortcut", source)] {
            let g = crate::analyze(&at(side), &crate::Options::default()).expect("reads the pair");
            let b = bodies(&g);
            let call = &b["news.read.cached_lines"].steps()[0];
            assert_eq!((call.to.as_str(), call.alts(&b)), (cached, [key(trimmed, feed)].as_slice()), "{side}");
            let runs = call.runs_instead(&b);
            assert_eq!((runs[0].to.as_str(), runs[0].on.as_str(), runs[0].named), (trimmed, feed, false), "{side}");
            let on_feed: Vec<(&str, &str)> = b[&key(trimmed, feed)].steps().iter().map(|s| (s.to.as_str(), s.on.as_str())).collect();
            assert_eq!(on_feed, [(next, feed)], "{side}");
        }
        let g = crate::analyze(&at("clean"), &crate::Options::default()).expect("reads the pair");
        let b = bodies(&g);
        let up = &b[trimmed].steps()[0];
        assert_eq!((up.to.as_str(), up.instead.as_slice()), (source, [key(cached, feed)].as_slice()));
        assert!(b.contains_key(&key(cached, feed)) && b[&key(cached, feed)].alts.is_empty(), "on a feed, nothing runs in its place");
        // what a route does at a place is what its calls may run there: on a feed, `lines` reads the store in one arm
        // and writes it in the other, the place as `lines` names it (the store from beyond, `self.root / key`)
        assert_eq!(arrows(&g)["news.read.lines"].updates, ["w:Disk:self.root / key", "w:Disk:{cache}/{url}"]);
        let g = crate::analyze(&at("shortcut"), &crate::Options::default()).expect("reads the pair");
        assert!(arrows(&g)["news.read.lines"].updates.is_empty(), "the shortcut's lines never reads the store");
    }

    /// A body run on a known class runs what that class runs (`tests/plans/next-method-on-a-class`): on a
    /// `Feed(Trimmed, Logged)`, `Trimmed.read`'s `super().read()` runs `Logged.read`, which writes the log whenever it
    /// runs, so the route reads back what its step wrote and changes nothing it found. On a value of any class a trimmed
    /// source may be, the call may run `Source.read` instead, which writes no log.
    #[test]
    fn a_body_on_a_known_class_does_what_that_class_runs() {
        let at = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/plans/next-method-on-a-class");
        let g = crate::analyze(&at, &crate::Options::default()).expect("reads the program");
        let run = &arrows(&g)["feeds.run"];
        assert_eq!(
            (run.reads_back.as_slice(), run.updates.as_slice()),
            (["w:File:data/log.txt".to_string()].as_slice(), [].as_slice()),
            "{run:?}"
        );
    }

    /// Building a class runs its initializer at the place the call stands in its caller's order (#165): `Log("started")`
    /// runs `Log.__init__`, which writes the log, so the route reads back what its step wrote and changes nothing it
    /// found.
    #[test]
    fn building_a_class_runs_its_initializer_in_its_caller_s_order() {
        let at = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/plans/builds-then-reads");
        let g = crate::analyze(&at, &crate::Options::default()).expect("reads the program");
        let run = &arrows(&g)["logs.run"];
        assert_eq!(
            (run.reads_back.as_slice(), run.updates.as_slice()),
            (["w:File:data/log.txt".to_string()].as_slice(), [].as_slice()),
            "{run:?}"
        );
    }

    /// `board = copy.copy(board)` makes the board it is handed anew: a step, no identity.
    #[test]
    fn a_copy_of_a_named_value_is_a_step() {
        let mut sites = vec![
            site("make", "", (10, 22), vec![], vec![Held::new("Board", "board")]),
            site("", "copy", (40, 56), vec![at("Board", "board", 50)], vec![Held::new("Board", "board")]),
        ];
        nest(&mut sites, &HashMap::new(), &mut 0);
        drop_identities(&mut sites);
        assert_eq!(sites.iter().map(|s| s.op.as_str()).collect::<Vec<_>>(), ["", "copy"]);
    }
}
