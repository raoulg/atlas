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

use crate::model::{Edge, EdgeKind, Graph, Node, NodeKind};

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
            edges: g
                .edges
                .iter()
                .filter(|e| keep.contains(e.src.as_str()) && keep.contains(e.dst.as_str()))
                .cloned()
                .collect(),
            files: g.files.iter().filter(|(f, _)| kept_files.contains(f.as_str()) || !dropped_files.contains(f.as_str())).map(|(f, t)| (f.clone(), t.clone())).collect(),
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

/// What the program reaches: its own code, and of a library read as code what its code reaches
/// into, through calls, the types its signatures and fields name, and the bases of what it
/// reaches. A class reached is built by its initializer and answers its bases' calls with its own
/// methods; what is kept stands in its class and its module.
pub fn reached(g: &Graph) -> impl View {
    let nodes: HashMap<&str, &Node> = g.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let library: HashSet<&str> = g.nodes.iter().filter(|n| n.library).map(|n| n.id.as_str()).collect();
    let follows = |k: EdgeKind| matches!(k, EdgeKind::Calls | EdgeKind::Instantiates | EdgeKind::Takes | EdgeKind::Returns | EdgeKind::Has | EdgeKind::Mutates | EdgeKind::Inherits | EdgeKind::Variant);
    let mut out: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut bases: HashMap<&str, Vec<&str>> = HashMap::new();
    for e in &g.edges {
        if follows(e.kind) {
            out.entry(e.src.as_str()).or_default().push(e.dst.as_str());
        }
        if e.kind == EdgeKind::Inherits {
            bases.entry(e.src.as_str()).or_default().push(e.dst.as_str());
        }
    }
    let ancestors = |c: &str| -> Vec<String> {
        let (mut out, mut todo) = (vec![], vec![c.to_string()]);
        while let Some(x) = todo.pop() {
            for b in bases.get(x.as_str()).into_iter().flatten() {
                if !out.iter().any(|o: &String| o == b) && out.len() < 32 {
                    out.push(b.to_string());
                    todo.push(b.to_string());
                }
            }
        }
        out
    };
    let mut kept: HashSet<String> = HashSet::new();
    let mut stack: Vec<String> = g.nodes.iter().filter(|n| !library.contains(n.id.as_str())).map(|n| n.id.clone()).collect();
    loop {
        while let Some(id) = stack.pop() {
            let mut next: Vec<String> = out.get(id.as_str()).map(|v| v.iter().map(|s| s.to_string()).collect()).unwrap_or_default();
            // a class built is built by its initializer
            if let Some(n) = nodes.get(id.as_str()).filter(|n| n.kind == NodeKind::Class) {
                next.extend(n.initializer(|m| nodes.get(m).copied()));
            }
            for d in next {
                if library.contains(d.as_str()) && kept.insert(d.clone()) {
                    stack.push(d);
                }
            }
        }
        // a class kept answers the calls into its bases with its own methods
        for c in kept.iter().filter(|id| nodes.get(id.as_str()).map_or(false, |n| n.kind == NodeKind::Class)) {
            for b in ancestors(c) {
                for m in g.nodes.iter().filter(|n| n.cls.as_deref() == Some(b.as_str()) && (kept.contains(&n.id) || !library.contains(n.id.as_str()))) {
                    let own = format!("{c}.{}", m.name);
                    if library.contains(own.as_str()) && !kept.contains(&own) {
                        stack.push(own);
                    }
                }
            }
        }
        if stack.is_empty() {
            break;
        }
        kept.extend(stack.iter().cloned());
    }
    let holders: Vec<String> = kept.iter().filter_map(|id| nodes.get(id.as_str())).flat_map(|n| [n.cls.clone(), Some(n.module.clone())]).flatten().collect();
    kept.extend(holders);
    let library: HashSet<String> = library.into_iter().map(str::to_string).collect();
    Fiber(move |n: &Node| !library.contains(&n.id) || kept.contains(&n.id))
}

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
