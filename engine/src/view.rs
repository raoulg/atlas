//! `view: Graph -> Graph`, in exactly two families.
//!
//! A `Fiber` keeps the subgraph where a predicate holds: the pullback of `true` along the
//! predicate, the nodes it holds for and the edges between them. A `Quotient` identifies
//! nodes (the coequalizer of the classes) and aggregates the edges between the classes.
//! One map `f: G -> B` gives both: its quotient is the overview, the fiber over one `b` is
//! that `b` alone. Views compose, and the order is meaning: fiber and quotient do not
//! commute. A view only reads the graph; nothing is written back through it.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use indexmap::IndexMap;

use crate::model::{Atom, Both, Edge, EdgeKind, Graph, Node, NodeKind, Touch};

pub trait View {
    fn apply(&self, g: &Graph) -> Graph;
}

/// The fiber of a predicate: the nodes that satisfy it, and the edges between them.
pub struct Fiber<F: Fn(&Node) -> bool>(pub F);

impl<F: Fn(&Node) -> bool> View for Fiber<F> {
    fn apply(&self, g: &Graph) -> Graph {
        let keep: HashSet<&str> = g.nodes.iter().filter(|n| (self.0)(n)).map(|n| n.id.as_str()).collect();
        // a file's text goes with its nodes: kept while one of them is
        let kept_files: HashSet<&str> = g.nodes.iter().filter(|n| keep.contains(n.id.as_str())).map(|n| n.file.as_str()).collect();
        let dropped_files: HashSet<&str> = g.nodes.iter().filter(|n| !keep.contains(n.id.as_str())).map(|n| n.file.as_str()).collect();
        let mut out = Graph {
            root: g.root.clone(),
            language: g.language.clone(),
            project_root: g.project_root.clone(),
            parts: g.parts.clone(),
            nodes: g.nodes.iter().filter(|n| keep.contains(n.id.as_str())).cloned().collect(),
            edges: g.edges.iter().filter(|e| keep.contains(e.src.as_str()) && keep.contains(e.dst.as_str())).cloned().collect(),
            libraries: g.libraries.clone(),
            files: g
                .files
                .iter()
                .filter(|(f, _)| kept_files.contains(f.as_str()) || !dropped_files.contains(f.as_str()))
                .map(|(f, t)| (f.clone(), t.clone()))
                .collect(),
            make: g.make.clone(),
            stats: g.stats.clone(),
        };
        out.recount();
        out
    }
}

/// Identify nodes: every node maps to the id of its class (`None` drops it). Edges
/// between classes are aggregated per kind with a `weight`; edges inside a class vanish.
pub struct Quotient<F: Fn(&Node) -> Option<Node>>(pub F);

impl<F: Fn(&Node) -> Option<Node>> View for Quotient<F> {
    fn apply(&self, g: &Graph) -> Graph {
        let mut classes: IndexMap<String, Node> = IndexMap::new();
        let mut of: HashMap<&str, String> = HashMap::new();
        for n in &g.nodes {
            if let Some(c) = (self.0)(n) {
                of.insert(n.id.as_str(), c.id.clone());
                classes.entry(c.id.clone()).or_insert(c);
            }
        }
        let mut agg: IndexMap<(String, String, EdgeKind), u32> = IndexMap::new();
        for e in &g.edges {
            let (Some(a), Some(b)) = (of.get(e.src.as_str()), of.get(e.dst.as_str())) else { continue };
            if a == b {
                continue;
            }
            *agg.entry((a.clone(), b.clone(), e.kind)).or_insert(0) += e.weight.unwrap_or(1);
        }
        let mut out = Graph {
            root: g.root.clone(),
            language: g.language.clone(),
            project_root: g.project_root.clone(),
            parts: g.parts.clone(),
            nodes: classes.into_values().collect(),
            edges: agg
                .into_iter()
                .map(|((src, dst, kind), w)| Edge { src, dst, kind, via: None, weight: Some(w), arms: vec![], uses: false })
                .collect(),
            libraries: g.libraries.clone(),
            files: BTreeMap::new(),
            make: g.make.clone(),
            stats: Default::default(),
        };
        out.recount();
        out
    }
}

/// Views in sequence; the first is applied first.
pub struct Compose(pub Vec<Box<dyn View>>);

impl View for Compose {
    fn apply(&self, g: &Graph) -> Graph {
        let mut cur = g.clone();
        for l in &self.0 {
            cur = l.apply(&cur);
        }
        cur
    }
}

// ---------- the built-in views ----------

/// A graph's library code, indexed for what the program reaches of it.
struct Library<'g> {
    nodes: HashMap<&'g str, &'g Node>,
    library: HashSet<&'g str>,
    /// What each node calls or builds.
    calls: HashMap<&'g str, Vec<(&'g str, EdgeKind)>>,
    /// The types each node takes, gives, changes, holds, extends or may be.
    types: HashMap<&'g str, Vec<&'g str>>,
    bases: HashMap<&'g str, Vec<&'g str>>,
    /// Each class's methods.
    members: HashMap<&'g str, Vec<&'g Node>>,
}

/// What the program reaches of a library: each function or class by the number of calls from the
/// program's code it takes, and the types the program and those functions name.
struct Reach {
    far: HashMap<String, usize>,
    named: HashSet<String>,
}

impl Reach {
    /// The classes reached: built, run on, or named.
    fn classes<'r>(&'r self, lib: &'r Library) -> impl Iterator<Item = &'r String> {
        self.far.keys().chain(&self.named).filter(|id| lib.nodes.get(id.as_str()).map_or(false, |n| n.kind == NodeKind::Class))
    }
}

impl<'g> Library<'g> {
    fn of(g: &'g Graph) -> Self {
        let nodes: HashMap<&str, &Node> = g.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
        let library: HashSet<&str> = g.nodes.iter().filter(|n| n.library).map(|n| n.id.as_str()).collect();
        let (mut calls, mut types, mut bases, mut members): (
            HashMap<&str, Vec<(&str, EdgeKind)>>,
            HashMap<&str, Vec<&str>>,
            HashMap<&str, Vec<&str>>,
            HashMap<&str, Vec<&Node>>,
        ) = Default::default();
        for e in &g.edges {
            match e.kind {
                EdgeKind::Calls | EdgeKind::Instantiates => calls.entry(e.src.as_str()).or_default().push((e.dst.as_str(), e.kind)),
                EdgeKind::Takes | EdgeKind::Returns | EdgeKind::Mutates | EdgeKind::Has | EdgeKind::Inherits | EdgeKind::Variant => {
                    types.entry(e.src.as_str()).or_default().push(e.dst.as_str())
                }
                _ => {}
            }
            if e.kind == EdgeKind::Inherits {
                bases.entry(e.src.as_str()).or_default().push(e.dst.as_str());
            }
        }
        for n in &g.nodes {
            if let Some(c) = n.cls.as_deref() {
                members.entry(c).or_default().push(n);
            }
        }
        Library { nodes, library, calls, types, bases, members }
    }

    fn ancestors(&self, c: &str) -> Vec<String> {
        let (mut out, mut todo) = (vec![], vec![c.to_string()]);
        while let Some(x) = todo.pop() {
            for b in self.bases.get(x.as_str()).into_iter().flatten() {
                if !out.iter().any(|o: &String| o == b) && out.len() < 32 {
                    out.push(b.to_string());
                    todo.push(b.to_string());
                }
            }
        }
        out
    }

    /// The library's code the program's calls reach within `rounds` calls. A class built runs its
    /// initializer, and a class reached answers the calls into its bases with its own methods, as
    /// far as those calls reach.
    fn reach(&self, rounds: usize) -> Reach {
        let (nodes, library) = (&self.nodes, &self.library);
        let mut far: HashMap<String, usize> = HashMap::new();
        let mut named: HashSet<String> = HashSet::new();
        let mut todo: VecDeque<(String, usize)> = VecDeque::new();
        for n in nodes.values().filter(|n| !library.contains(n.id.as_str())) {
            todo.extend(
                self.calls.get(n.id.as_str()).into_iter().flatten().filter(|(d, _)| library.contains(*d)).map(|(d, _)| (d.to_string(), 1)),
            );
            named.extend(self.types.get(n.id.as_str()).into_iter().flatten().filter(|d| library.contains(**d)).map(|d| d.to_string()));
        }
        loop {
            while let Some((id, d)) = todo.pop_front() {
                if d > rounds || far.get(&id).map_or(false, |&f| f <= d) {
                    continue;
                }
                far.insert(id.clone(), d);
                let Some(n) = nodes.get(id.as_str()) else { continue };
                // a class built runs its initializer
                if n.kind == NodeKind::Class {
                    todo.extend(n.initializer(|m| nodes.get(m).copied()).map(|i| (i, d)));
                }
                // a function gives, takes and runs on its types
                if matches!(n.kind, NodeKind::Function | NodeKind::Method) {
                    named
                        .extend(self.types.get(id.as_str()).into_iter().flatten().filter(|t| library.contains(**t)).map(|t| t.to_string()));
                    named.extend(n.cls.clone());
                }
                todo.extend(
                    self.calls
                        .get(id.as_str())
                        .into_iter()
                        .flatten()
                        .filter(|(c, _)| library.contains(*c))
                        .map(|(c, _)| (c.to_string(), d + 1)),
                );
            }
            // a class reached answers the calls into its bases with its own methods
            let reach = Reach { far, named };
            for c in reach.classes(self) {
                for b in self.ancestors(c) {
                    for m in self.members.get(b.as_str()).into_iter().flatten() {
                        let at = if library.contains(m.id.as_str()) { reach.far.get(&m.id).copied() } else { Some(1) };
                        let own = format!("{c}.{}", m.name);
                        if let Some(d) = at.filter(|&d| d <= rounds && library.contains(own.as_str()) && !reach.far.contains_key(&own)) {
                            todo.push_back((own, d));
                        }
                    }
                }
            }
            (far, named) = (reach.far, reach.named);
            if todo.is_empty() {
                break;
            }
        }
        Reach { far, named }
    }

    /// For each method of a library's class, the methods that answer a call to it in the classes
    /// `reach` reaches: their own, where they override it.
    fn overrides(&self, reach: &Reach) -> HashMap<&'g str, Vec<&'g str>> {
        let mut out: HashMap<&str, Vec<&str>> = HashMap::new();
        for c in reach.classes(self) {
            for b in self.ancestors(c) {
                for m in self.members.get(b.as_str()).into_iter().flatten() {
                    if let Some((own, _)) =
                        self.nodes.get_key_value(format!("{c}.{}", m.name).as_str()).filter(|(o, _)| self.library.contains(**o))
                    {
                        out.entry(m.id.as_str()).or_default().push(own);
                    }
                }
            }
        }
        for answers in out.values_mut() {
            answers.sort();
            answers.dedup();
        }
        out
    }
}

/// What the program reaches: its own code, and of a library read as code only what the program
/// reaches of it, `rounds` calls deep. A library's function or class is reached by the calls that
/// lead to it from the program's own code: the program's calls go into the first, their calls
/// into the second, up to `rounds`. A class built runs its initializer, and a class reached
/// answers the calls into its bases with its own methods, as far as those calls reach. With them
/// the types the program names and those functions take, give and run on. The rest of a
/// library's module is no part of the picture, nor is a name outside the code that only the rest
/// names; what is kept stands in its class and its module.
///
/// What the program reaches of a library beyond the rounds is glued into the library's function
/// at the border, the one whose call leads there: that function is one arrow, which touches every
/// place its code reaches inside the library as far as Atlas read it, and runs what is kept that
/// this code runs. Of a library's file the graph keeps the text its kept nodes show.
pub fn reached(g: &Graph, rounds: usize) -> impl View {
    let lib = Library::of(g);
    let reach = lib.reach(rounds);
    let mut kept: HashSet<String> = reach.far.keys().cloned().chain(reach.named.iter().cloned()).collect();
    let holders: Vec<String> =
        kept.iter().filter_map(|id| lib.nodes.get(id.as_str())).flat_map(|n| [n.cls.clone(), Some(n.module.clone())]).flatten().collect();
    kept.extend(holders);
    // a name outside the code that only the rest of a library names is none the program reaches
    let mut outside: HashMap<&str, bool> = HashMap::new();
    for e in g.edges.iter().filter(|e| lib.nodes.get(e.dst.as_str()).map_or(false, |n| n.kind == NodeKind::External)) {
        let by_program = !lib.library.contains(e.src.as_str()) || reach.far.contains_key(&e.src);
        *outside.entry(e.dst.as_str()).or_insert(false) |= by_program;
    }
    let mut dropped: HashSet<String> = outside.into_iter().filter(|(_, by_program)| !by_program).map(|(id, _)| id.to_string()).collect();
    dropped.extend(lib.library.iter().filter(|id| !kept.contains(**id)).map(|id| id.to_string()));
    let glued = glue(&lib, &dropped);
    Reached { dropped, glued }
}

/// What the code beyond the fiber gives the library function at its border: the places it
/// touches, its calls as read, and what it calls or builds of what is kept.
#[derive(Default)]
struct Glued {
    touches: Vec<Touch>,
    instead: Vec<Touch>,
    runs: Vec<(String, EdgeKind)>,
}

/// The fiber `reached` keeps, with what lies beyond it glued into its border.
struct Reached {
    dropped: HashSet<String>,
    glued: HashMap<String, Glued>,
}

/// For each kept library function, what the dropped code its calls lead into does: everything
/// that code reaches through its calls, the classes it builds and the overrides that answer its
/// calls, as far as it is dropped. Each touch is drawn at the border function's call that leads
/// there, and is made whole by the same callers, the dropped ones standing for the border.
fn glue(lib: &Library, dropped: &HashSet<String>) -> HashMap<String, Glued> {
    let overrides = lib.overrides(&lib.reach(usize::MAX));
    let gone = |id: &str| dropped.contains(id) && lib.library.contains(id);
    let mut out = HashMap::new();
    for f in lib.nodes.values().filter(|n| n.library && matches!(n.kind, NodeKind::Function | NodeKind::Method) && !dropped.contains(&n.id))
    {
        let line = |to: &str| f.sites.iter().find(|s| s.to == to).map_or(0, |s| s.line);
        // a class built runs its initializer: dropped, it lies beyond the border
        let init = |x: &str, kind: EdgeKind| -> Option<&str> {
            let i = lib.nodes.get(x).filter(|_| kind == EdgeKind::Instantiates)?.initializer(|m| lib.nodes.get(m).copied())?;
            lib.nodes.get_key_value(i.as_str()).map(|(k, _)| *k).filter(|i| gone(i))
        };
        // the dropped code the border leads into, each with the line of the call that leads there
        let mut todo: Vec<(&str, EdgeKind, u32)> = vec![];
        for &(d, kind) in lib.calls.get(f.id.as_str()).into_iter().flatten().rev() {
            match init(d, kind) {
                _ if gone(d) => todo.push((d, kind, line(d))),
                Some(i) => todo.push((i, EdgeKind::Calls, line(d))),
                None => {}
            }
        }
        let mut beyond: Vec<(&str, u32)> = vec![];
        let mut seen: HashSet<&str> = HashSet::new();
        let mut runs: Vec<(String, EdgeKind)> = vec![];
        while let Some((x, kind, at)) = todo.pop() {
            if !gone(x) {
                // what is kept that the dropped code runs, the border now runs
                if x != f.id && !dropped.contains(x) && !runs.iter().any(|(r, k)| r == x && *k == kind) {
                    runs.push((x.to_string(), kind));
                }
                todo.extend(init(x, kind).map(|i| (i, EdgeKind::Calls, at)));
                continue;
            }
            if !seen.insert(x) {
                continue;
            }
            beyond.push((x, at));
            todo.extend(init(x, kind).map(|i| (i, EdgeKind::Calls, at)));
            for &(y, k) in lib.calls.get(x).into_iter().flatten() {
                todo.push((y, k, at));
            }
            for &o in overrides.get(x).into_iter().flatten().filter(|o| gone(o)) {
                todo.push((o, EdgeKind::Calls, at));
            }
        }
        if beyond.is_empty() {
            continue;
        }
        // a caller beyond the border that makes a touch whole is the border itself, which a route
        // through it reaches; one outside what the border leads into makes it whole on no route
        // through the border
        let within: HashSet<&str> = beyond.iter().map(|(x, _)| *x).collect();
        let mut glued = Glued { runs, ..Default::default() };
        for (x, at) in beyond {
            let n = lib.nodes[x];
            for t in &n.touches {
                let mut by = std::collections::BTreeSet::new();
                let mut whole = t.by.is_empty();
                for alt in &t.by {
                    if alt.members().any(|p| gone(p) && !within.contains(p)) {
                        continue;
                    }
                    // the callers beyond the border are the border itself; an entry stays a condition
                    let rest: Both = alt
                        .0
                        .iter()
                        .filter(|a| !matches!(a, Atom::Reaches(p) if within.contains(p.as_str()) || *p == f.id))
                        .cloned()
                        .collect();
                    if rest.0.is_empty() {
                        whole = true;
                        break;
                    }
                    by.insert(rest);
                }
                crate::model::absorb(&mut by);
                let by = if whole { Default::default() } else { by };
                if by.is_empty() && !whole {
                    continue;
                }
                add(&mut glued.touches, Touch { line: at, by, ..t.clone() });
            }
            for t in &n.instead {
                add(&mut glued.instead, Touch { line: at, ..t.clone() });
            }
        }
        out.insert(f.id.clone(), glued);
    }
    out
}

/// A touch among others: the same call again counts up.
fn add(into: &mut Vec<Touch>, t: Touch) {
    match into.iter_mut().find(|o| Touch { n: t.n, ..(*o).clone() } == t) {
        Some(o) => o.n += t.n,
        None => into.push(t),
    }
}

impl View for Reached {
    fn apply(&self, g: &Graph) -> Graph {
        let mut out = Fiber(|n: &Node| !self.dropped.contains(&n.id)).apply(g);
        let mut edges: HashSet<(String, String, EdgeKind)> = out.edges.iter().map(|e| (e.src.clone(), e.dst.clone(), e.kind)).collect();
        for n in out.nodes.iter_mut() {
            let Some(glued) = self.glued.get(&n.id) else { continue };
            for t in &glued.touches {
                add(&mut n.touches, t.clone());
            }
            for t in &glued.instead {
                add(&mut n.instead, t.clone());
            }
            for (to, kind) in &glued.runs {
                if edges.insert((n.id.clone(), to.clone(), *kind)) {
                    out.edges.push(Edge {
                        src: n.id.clone(),
                        dst: to.clone(),
                        kind: *kind,
                        via: None,
                        weight: None,
                        arms: vec![],
                        uses: false,
                    });
                }
            }
        }
        shown_text(&mut out);
        out.recount();
        out
    }
}

/// Of a library's file, the lines its nodes show: a module's head, and the lines of everything
/// else it holds, as the code panel shows them. The rest of its text is blank, so that the lines
/// keep their numbers.
fn shown_text(g: &mut Graph) {
    let mut shown: HashMap<&str, Vec<(u32, u32)>> = HashMap::new();
    for n in g.nodes.iter().filter(|n| n.library) {
        let Some((a, b)) = n.span else { continue };
        let lines = if n.kind == NodeKind::Module { (1, b.min(HEAD)) } else { (a, b.min(a + SHOWN - 1)) };
        shown.entry(n.file.as_str()).or_default().push(lines);
    }
    let texts: Vec<(String, String)> = shown
        .into_iter()
        .filter_map(|(file, spans)| {
            let text = g.files.get(file)?;
            let kept: String = text
                .split('\n')
                .enumerate()
                .map(|(i, l)| if spans.iter().any(|(a, b)| (*a..=*b).contains(&(i as u32 + 1))) { l } else { "" })
                .collect::<Vec<_>>()
                .join("\n");
            Some((file.to_string(), kept))
        })
        .collect();
    g.files.extend(texts);
}

/// The lines the code panel shows of a module: its head.
const HEAD: u32 = 60;
/// The lines the code panel shows of anything else: its first.
const SHOWN: u32 = 400;

/// Drop what only the code beside it may use; modules always stay.
pub fn hide_private() -> impl View {
    Fiber(|n: &Node| n.kind == NodeKind::Module || !n.private)
}

/// Keep what an entry point reaches through calls and instantiations, the entry included.
pub fn reach_from(g: &Graph, entry: &str) -> impl View {
    let mut out: HashMap<&str, Vec<&str>> = HashMap::new();
    for e in &g.edges {
        if matches!(e.kind, EdgeKind::Calls | EdgeKind::Instantiates) {
            out.entry(e.src.as_str()).or_default().push(e.dst.as_str());
        }
    }
    let mut seen: HashSet<String> = HashSet::new();
    let mut q = VecDeque::from([entry.to_string()]);
    while let Some(cur) = q.pop_front() {
        if !seen.insert(cur.clone()) {
            continue;
        }
        for &t in out.get(cur.as_str()).into_iter().flatten() {
            q.push_back(t.to_string());
        }
    }
    Fiber(move |n: &Node| seen.contains(&n.id))
}

/// One node per module; symbols fold into the module that contains them.
pub fn by_module() -> impl View {
    Quotient(|n: &Node| {
        if n.kind == NodeKind::External {
            return None;
        }
        Some(Node {
            id: n.module.clone(),
            kind: NodeKind::Module,
            name: n.module.rsplit('.').next().unwrap_or(&n.module).to_string(),
            module: n.module.clone(),
            file: n.file.clone(),
            ..Default::default()
        })
    })
}
