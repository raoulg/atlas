//! A `Graph` under construction: what a language module fills while it parses, with the
//! scratch it needs for resolution. It never leaves `Language::parse`.

use std::collections::{BTreeMap, HashMap, HashSet};

use indexmap::IndexMap;

use crate::model::{Bucket, Dir, Edge, EdgeKind, Entry, EntryKind, Graph, Node, NodeKind, Stats, Touch, Unresolved, World};

/// Per-class facts that only matter during resolution.
#[derive(Default, Clone, Debug)]
pub struct ClassInfo {
    pub bases: Vec<Vec<String>>,
    pub resolved: Vec<String>,
    pub attr_types: BTreeMap<String, String>,
    /// The library types its fields hold, by field: `conn` holds `ext:rusqlite.Connection`.
    pub foreign: BTreeMap<String, String>,
    pub fields: Vec<(String, Vec<Vec<String>>)>,
    /// For a sum (a Rust `enum`): the types its variants carry.
    pub variants: Vec<Vec<String>>,
}

/// What the program's classes say of one another: the methods each method overrides, each with
/// the method's parameters named as the base's at the same place, and the classes each class
/// inherits from.
#[derive(Default)]
pub struct Family {
    pub overrides: HashMap<String, Vec<(String, Vec<(String, String)>)>>,
    pub bases: HashMap<String, Vec<String>>,
}

impl Family {
    /// Whether `class` is `of` or inherits from it.
    pub fn is_a(&self, class: &str, of: &str) -> bool {
        let (mut todo, mut seen) = (vec![class], HashSet::new());
        while let Some(c) = todo.pop() {
            if c == of {
                return true;
            }
            if seen.insert(c) {
                todo.extend(self.bases.get(c).into_iter().flatten().map(String::as_str));
            }
        }
        false
    }
}

#[derive(Default, Clone, Debug)]
pub struct SigInfo {
    pub ptypes: Vec<Vec<String>>,
    pub rtypes: Vec<Vec<String>>,
}

/// The nodes and edges found so far, and the tables that resolve names into them.
#[derive(Default)]
pub struct GraphBuilder {
    pub nodes: IndexMap<String, Node>,
    pub edges: Vec<Edge>,
    pub edge_set: HashSet<(String, String, EdgeKind)>,
    pub classes: HashMap<String, ClassInfo>,
    pub sigs: HashMap<String, SigInfo>,
    /// The calls not followed: by the name each is written with, and by what that name is bound to.
    pub unresolved: HashMap<String, u32>,
    pub unresolved_by: Unresolved,
    pub files: BTreeMap<String, String>,
    /// Module id -> local name -> fully qualified target, kept for re-export lookups.
    pub bindings: HashMap<String, HashMap<String, String>>,
    /// Methods that change their receiver in place.
    pub mutating: HashSet<String>,
    /// Calls whose caller uses what they give back, as (caller, callee).
    pub used: HashSet<(String, String)>,
}

impl GraphBuilder {
    pub fn add_node(&mut self, mut n: Node) {
        if n.prophecy.is_none() {
            n.prophecy = crate::prophecy::Prophecy::among(&n.decorators);
        }
        self.nodes.insert(n.id.clone(), n);
    }

    pub fn add_edge(&mut self, src: &str, dst: &str, kind: EdgeKind, via: Option<String>) {
        self.add_call(src, dst, kind, via, vec![]);
    }

    /// An edge from a body, with the arms of the choices it sits in; the first call of a
    /// target is the one kept.
    pub fn add_call(&mut self, src: &str, dst: &str, kind: EdgeKind, via: Option<String>, arms: Vec<(u32, u32)>) {
        if src == dst {
            return;
        }
        let key = (src.to_string(), dst.to_string(), kind);
        if self.edge_set.insert(key) {
            self.edges.push(Edge { src: src.into(), dst: dst.into(), kind, via, weight: None, arms, uses: false });
        }
    }

    /// A name a package exports, as a route into it: a function or a class, by its label, unless
    /// it is already a route of another kind.
    pub fn export(&mut self, id: &str, label: String) {
        if let Some(n) = self.nodes.get_mut(id).filter(|n| matches!(n.kind, NodeKind::Function | NodeKind::Class) && n.entry.is_none()) {
            n.entry = Some(crate::model::Entry::new(crate::model::EntryKind::Export, label));
        }
    }

    /// A call the reader could not follow, by the name it is written with (its first two parts)
    /// and what the name it starts from is bound to.
    pub fn not_followed(&mut self, name: String, bucket: Bucket) {
        *self.unresolved.entry(name).or_insert(0) += 1;
        self.unresolved_by.count(bucket);
    }

    /// Record a boundary call on a node; the same call again counts up.
    /// A call as it was read, for the routes that reach none of the callers that make it whole.
    pub fn instead(&mut self, id: &str, t: Touch) {
        let Some(n) = self.nodes.get_mut(id) else { return };
        if !n.instead.iter().any(|x| x.kind == t.kind && x.what == t.what && x.op == t.op && x.line == t.line) {
            n.instead.push(Touch { n: 1, by: vec![], users_of: vec![], ..t });
        }
    }

    /// The classes the program's classes inherit from, and the methods each method overrides
    /// through them, each with the method's parameters named as the base's at the same place.
    pub fn family(&self) -> Family {
        let mut family = Family::default();
        for e in self.edges.iter().filter(|e| e.kind == EdgeKind::Inherits) {
            family.bases.entry(e.src.clone()).or_default().push(e.dst.clone());
        }
        for n in self.nodes.values().filter(|n| n.kind == NodeKind::Method) {
            let Some(cls) = n.cls.as_deref() else { continue };
            let (mut todo, mut seen) = (family.bases.get(cls).cloned().unwrap_or_default(), HashSet::new());
            while let Some(b) = todo.pop() {
                if !seen.insert(b.clone()) {
                    continue;
                }
                let base = format!("{b}.{}", n.name);
                if let Some(m) = self.nodes.get(&base).filter(|m| m.kind == NodeKind::Method) {
                    let named = n.params.iter().zip(&m.params).map(|(own, theirs)| (own.name.clone(), theirs.name.clone())).collect();
                    family.overrides.entry(n.id.clone()).or_default().push((base, named));
                }
                todo.extend(family.bases.get(&b).cloned().unwrap_or_default());
            }
        }
        family
    }

    /// A request `id` makes whole that no route of its part serves.
    pub fn ask(&mut self, id: &str, request: String) {
        let Some(n) = self.nodes.get_mut(id) else { return };
        if !n.asks.contains(&request) {
            n.asks.push(request);
        }
    }

    pub fn touch(&mut self, id: &str, t: Touch) {
        let Some(n) = self.nodes.get_mut(id) else { return };
        // only what is read can be a check
        let t = Touch { checks: t.checks && t.dir == Some(Dir::Read), ..t };
        match n
            .touches
            .iter_mut()
            .find(|x| x.kind == t.kind && x.what == t.what && x.op == t.op && x.artifact == t.artifact && x.dir == t.dir)
        {
            Some(x) => {
                x.n += 1;
                // one call that uses what it reads makes the touch a use
                x.checks &= t.checks;
                // whole in its own function is whole in every route; else in the routes of each
                let whole_here = |y: &Touch| y.by.is_empty() && y.users_of.is_empty();
                let here = whole_here(x) || whole_here(&t);
                // a place is decided where any of the calls that touch it decides it
                x.fixed |= t.fixed;
                for f in t.fixed_by {
                    if !x.fixed_by.contains(&f) {
                        x.fixed_by.push(f);
                    }
                }
                if x.payload.is_empty() {
                    x.payload = t.payload;
                }
                if here {
                    x.by.clear();
                    x.users_of.clear();
                } else {
                    for f in t.by {
                        if !x.by.contains(&f) {
                            x.by.push(f);
                        }
                    }
                    for e in t.users_of {
                        if !x.users_of.contains(&e) {
                            x.users_of.push(e);
                        }
                    }
                }
            }
            None => n.touches.push(Touch { n: 1, ..t }),
        }
    }

    /// The graph, once every file is parsed: the scratch is dropped, the names that did not
    /// resolve are counted.
    pub fn finish(self, root: String, project_root: Option<&std::path::Path>, language: &str) -> Graph {
        let mut top: Vec<(String, u32)> = self.unresolved.iter().map(|(k, v)| (k.clone(), *v)).collect();
        let mut nodes = self.nodes;
        serve_data(&mut nodes);
        crate::flow::wire(&mut nodes, &self.edges);
        mark_doors(&mut nodes, &self.edges);
        mark_options(&mut nodes);
        crate::prophecy::settle(&mut nodes, &self.edges);
        let mut edges = self.edges;
        for e in edges.iter_mut().filter(|e| e.kind == EdgeKind::Calls) {
            e.uses = self.used.contains(&(e.src.clone(), e.dst.clone()));
        }
        top.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let mut g = Graph {
            root,
            language: language.into(),
            project_root: project_root.map(crate::tree::path_str).unwrap_or_default(),
            nodes: nodes.into_values().collect(),
            edges,
            files: self.files,
            stats: Stats {
                unresolved_calls_total: self.unresolved.values().sum(),
                unresolved_calls_top: top.into_iter().take(25).collect(),
                unresolved_calls: self.unresolved_by,
                ..Default::default()
            },
            ..Default::default()
        };
        g.recount();
        g
    }

    /// The method `name` a value of class `cls` runs: the class's own, else the first its bases
    /// give in the order they are written, each base before the next (a trait's provided
    /// method, an inherited one).
    pub fn method_of(&self, cls: &str, name: &str) -> Option<String> {
        self.method_in(cls, name, 0)
    }

    fn method_in(&self, cls: &str, name: &str, depth: u32) -> Option<String> {
        let cand = format!("{cls}.{name}");
        if self.nodes.contains_key(&cand) {
            return Some(cand);
        }
        if depth > 8 {
            return None;
        }
        self.classes.get(cls)?.resolved.iter().filter(|b| !b.starts_with("ext:")).find_map(|b| self.method_in(b, name, depth + 1))
    }

    /// Library code read as code: every node of a library module is the library's, and no route
    /// into the program. A name outside the code is no code of its own.
    pub fn mark_library(&mut self) {
        let library: HashSet<String> = self
            .nodes
            .values()
            .filter(|n| n.kind != NodeKind::External && (n.library || self.nodes.get(&n.module).map_or(false, |m| m.library)))
            .map(|n| n.id.clone())
            .collect();
        for id in &library {
            if let Some(n) = self.nodes.get_mut(id) {
                n.library = true;
                n.entry = None;
            }
        }
    }

    pub fn ext_node(&mut self, eid: &str) {
        if self.nodes.contains_key(eid) {
            return;
        }
        // a shaped type keeps its shape in the name: `jaxtyping` / `Float[Tensor, batch dim]`
        let full = &eid[4..];
        let cut = full.find('[').unwrap_or(full.len());
        let (module, name) = match full[..cut].rsplit_once('.') {
            Some((module, name)) => (module, format!("{name}{}", &full[cut..])),
            None => ("", full.to_string()),
        };
        let package = module.split(['.', ':']).next().unwrap_or("");
        self.add_node(Node {
            id: eid.into(),
            kind: NodeKind::External,
            name,
            module: module.into(),
            door: crate::effects::library_place(package),
            ..Default::default()
        });
    }
}

/// Mark the classes that are doors: one of their operations reaches a service, through what it
/// calls. An operation is a method called on a value of the class, not on the class: an
/// alternative constructor (`Config.from_yaml`) reads into a value and leaves it data.
fn mark_doors(nodes: &mut IndexMap<String, Node>, edges: &[Edge]) {
    let mut out: std::collections::HashMap<&str, Vec<&str>> = std::collections::HashMap::new();
    for e in edges.iter().filter(|e| matches!(e.kind, EdgeKind::Calls | EdgeKind::Instantiates)) {
        out.entry(e.src.as_str()).or_default().push(e.dst.as_str());
    }
    let operation = |n: &Node| n.kind == NodeKind::Method && !n.on_class;
    let reaches = |start: &str| -> Option<World> {
        let mut seen = HashSet::from([start.to_string()]);
        let mut stack = vec![start.to_string()];
        while let Some(id) = stack.pop() {
            let Some(n) = nodes.get(&id) else { continue };
            if let Some(t) = n.touches.iter().find(|t| t.kind.is_service()) {
                return Some(t.kind);
            }
            let next = match n.kind {
                NodeKind::Class => n.initializer(|m| nodes.get(m)).into_iter().collect::<Vec<String>>(),
                _ => out.get(id.as_str()).into_iter().flatten().map(|d| d.to_string()).collect(),
            };
            for d in next {
                if seen.insert(d.clone()) {
                    stack.push(d);
                }
            }
        }
        None
    };
    let doors: Vec<(String, World)> = nodes
        .values()
        .filter(|n| n.kind == NodeKind::Class)
        .filter_map(|c| {
            let prefix = format!("{}.", c.id);
            nodes
                .values()
                .filter(|m| m.cls.as_deref() == Some(c.id.as_str()) || m.id.starts_with(&prefix) && m.kind == NodeKind::Method)
                .filter(|m| operation(m))
                .find_map(|m| reaches(&m.id))
                .map(|k| (c.id.clone(), k))
        })
        .collect();
    for (id, kind) in doors {
        if let Some(n) = nodes.get_mut(&id) {
            n.door = Some(kind);
        }
    }
}

/// An entry run from the command line (a command, a script) is handed what the human types: the
/// options its own declaration names (a command's parameters), else those of the parses of the
/// command line its route runs, in the order it runs them.
fn mark_options(nodes: &mut IndexMap<String, Node>) {
    let run = |e: &Entry| matches!(e.kind, EntryKind::Cli | EntryKind::Script) && e.options.is_empty();
    let mut found = vec![];
    for n in nodes.values().filter(|n| n.entry.as_ref().map_or(false, run)) {
        let mut options = vec![];
        parsed(nodes, &n.id, &mut HashSet::new(), &mut options);
        if !options.is_empty() {
            found.push((n.id.clone(), options));
        }
    }
    for (id, options) in found {
        if let Some(e) = nodes.get_mut(&id).and_then(|n| n.entry.as_mut()) {
            e.options = options;
        }
    }
}

/// The options of the parses of the command line a function runs, in its own steps or through
/// the functions they run, in the order they run; a class runs its initializer.
fn parsed(nodes: &IndexMap<String, Node>, id: &str, seen: &mut HashSet<String>, out: &mut Vec<String>) {
    if !seen.insert(id.to_string()) {
        return;
    }
    let Some(n) = nodes.get(id) else { return };
    if n.kind == NodeKind::Class {
        if let Some(i) = n.initializer(|m| nodes.get(m)) {
            parsed(nodes, &i, seen, out);
        }
        return;
    }
    for s in &n.sites {
        match &s.command_line {
            Some(options) => {
                for o in options {
                    if !out.contains(o) {
                        out.push(o.clone());
                    }
                }
            }
            None if !s.to.is_empty() => parsed(nodes, &s.to, seen, out),
            None => {}
        }
    }
}

/// A foreign type worth a place: a package's type that holds data. Not one of the language's
/// own, and not one reached through a type parameter (`D::Error`, `T.Item`).
pub fn foreign_type(id: &str, own: &crate::knowledge::OwnTypes) -> bool {
    let Some(full) = id.strip_prefix("ext:") else { return false };
    let head = full.split('.').next().unwrap_or("");
    let name = full.rsplit('.').next().unwrap_or("");
    let parameter = head.len() == 1 && head.chars().all(|c| c.is_ascii_uppercase());
    full.contains('.') && !own.packages.contains(&head) && !own.names.contains(&name) && !parameter
}

/// A touch that only prepares a place is part of what is done there. A directory made, or a path
/// checked, for a file the same function reads or writes is that file's (`to.parent.mkdir()`
/// before `to.write_bytes()`); a connection or a session opened on a database the same function
/// reads or writes is part of those statements, and so is a table's shape given where the same
/// function reads or writes the table. A directory made for the file a connection opens is that
/// database's. A table some statement makes temporary is the connection's working memory, no
/// place: its touches go.
fn serve_data(nodes: &mut IndexMap<String, Node>) {
    use crate::effects::{SCHEMA, SCRATCH, SHAPED};
    for t in nodes.values_mut().flat_map(|n| n.touches.iter_mut().chain(n.instead.iter_mut())) {
        if let Some(table) = t.artifact.strip_prefix(SHAPED) {
            (t.artifact, t.op) = (table.to_string(), SCHEMA.to_string());
        }
    }
    let table = |t: &Touch| (t.kind.name().to_string(), t.artifact.strip_prefix(SCRATCH).unwrap_or(&t.artifact).to_string());
    let scratch: HashSet<(String, String)> =
        nodes.values().flat_map(|n| &n.touches).filter(|t| t.artifact.starts_with(SCRATCH)).map(table).collect();
    for n in nodes.values_mut() {
        n.touches.retain(|t| !(matches!(t.kind, World::Db(_)) && scratch.contains(&table(t))));
        let all = n.touches.clone();
        let call = |t: &Touch| (t.line, t.op.clone(), t.kind);
        let folded: Vec<_> = all.iter().filter(|t| prepares(t, &all)).map(call).collect();
        n.touches.retain(|t| !prepares(t, &all));
        // the same call as it was read prepares the same place
        n.instead.retain(|t| !folded.contains(&call(t)));
    }
}

/// Whether touch `t` only prepares a place that another of `all` reads or writes, in every route
/// `t` is in: the other is in every route, or in the same ones.
fn prepares(t: &Touch, all: &[Touch]) -> bool {
    let along = |d: &Touch| (d.by.is_empty() && d.users_of.is_empty()) || (d.by == t.by && d.users_of == t.users_of);
    let data = |d: &&Touch| d.kind == t.kind && !d.shape() && along(d);
    match t.kind {
        World::Disk => {
            let within = |file: &str| file == t.artifact || (t.artifact.ends_with('/') && file.starts_with(&t.artifact));
            // a database's file is made for as a file is
            let opens = |d: &&Touch| matches!(d.kind, World::Db(_)) && !d.file.is_empty() && along(d);
            t.shape()
                && !t.artifact.is_empty()
                && (all.iter().filter(data).filter(|d| !d.artifact.is_empty()).any(|d| within(&d.artifact))
                    || all.iter().filter(opens).any(|d| within(&d.file)))
        }
        World::Db(_) | World::Global if t.shape() => {
            all.iter().filter(data).filter(|d| d.dir.is_some() && !d.artifact.is_empty()).any(|d| d.artifact == t.artifact)
        }
        World::Db(_) => t.dir.is_none() && all.iter().filter(data).any(|d| d.dir.is_some()),
        _ => false,
    }
}
