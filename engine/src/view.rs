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
        let mut out = Graph {
            root: g.root.clone(),
            language: g.language.clone(),
            project_root: g.project_root.clone(),
            roles: g.roles.clone(),
            parts: g.parts.clone(),
            nodes: g.nodes.iter().filter(|n| keep.contains(n.id.as_str())).cloned().collect(),
            edges: g
                .edges
                .iter()
                .filter(|e| keep.contains(e.src.as_str()) && keep.contains(e.dst.as_str()))
                .cloned()
                .collect(),
            files: g.files.clone(),
            make: g.make.clone(),
            stats: Default::default(),
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
            roles: g.roles.clone(),
            parts: g.parts.clone(),
            nodes: classes.into_values().collect(),
            edges: agg
                .into_iter()
                .map(|((src, dst, kind), w)| Edge { src, dst, kind, via: None, weight: Some(w), arms: vec![] })
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

/// Drop private symbols (leading underscore, dunders included); modules always stay.
pub fn hide_private() -> impl View {
    Fiber(|n: &Node| n.kind == NodeKind::Module || !n.name.starts_with('_'))
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

/// One node per role; the class node carries the role as its name.
pub fn by_role() -> impl View {
    Quotient(|n: &Node| {
        if n.kind == NodeKind::External || n.role.is_empty() {
            return None;
        }
        Some(Node {
            id: format!("role:{}", n.role),
            kind: NodeKind::Place,
            name: n.role.clone(),
            module: String::new(),
            role: n.role.clone(),
            ..Default::default()
        })
    })
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
            role: n.role.clone(),
            file: n.file.clone(),
            ..Default::default()
        })
    })
}
