//! `lens: Graph -> Graph`, in exactly two families.
//!
//! A `Restrict` keeps a subgraph (the inverse image of a predicate); a `Quotient`
//! identifies nodes (a pushout) and aggregates the edges between the classes. Lenses
//! compose, and the order is meaning: restriction and quotient do not commute.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use indexmap::IndexMap;

use crate::model::{Edge, EdgeKind, Graph, Node, NodeKind};

pub trait Lens {
    fn apply(&self, g: &Graph) -> Graph;
}

/// Keep the nodes that satisfy the predicate, and the edges between them.
pub struct Restrict<F: Fn(&Node) -> bool>(pub F);

impl<F: Fn(&Node) -> bool> Lens for Restrict<F> {
    fn apply(&self, g: &Graph) -> Graph {
        let keep: HashSet<&str> = g.nodes.iter().filter(|n| (self.0)(n)).map(|n| n.id.as_str()).collect();
        let mut out = Graph {
            root: g.root.clone(),
            project_root: g.project_root.clone(),
            roles: g.roles.clone(),
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

impl<F: Fn(&Node) -> Option<Node>> Lens for Quotient<F> {
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
            project_root: g.project_root.clone(),
            roles: g.roles.clone(),
            nodes: classes.into_values().collect(),
            edges: agg
                .into_iter()
                .map(|((src, dst, kind), w)| Edge { src, dst, kind, via: None, weight: Some(w) })
                .collect(),
            files: BTreeMap::new(),
            make: g.make.clone(),
            stats: Default::default(),
        };
        out.recount();
        out
    }
}

/// Lenses in sequence; the first is applied first.
pub struct Compose(pub Vec<Box<dyn Lens>>);

impl Lens for Compose {
    fn apply(&self, g: &Graph) -> Graph {
        let mut cur = g.clone();
        for l in &self.0 {
            cur = l.apply(&cur);
        }
        cur
    }
}

// ---------- the built-in lenses ----------

/// Drop private symbols (leading underscore, dunders included); modules always stay.
pub fn hide_private() -> impl Lens {
    Restrict(|n: &Node| n.kind == NodeKind::Module || !n.name.starts_with('_'))
}

/// Keep what an entry point reaches through calls and instantiations, the entry included.
pub fn reach_from(g: &Graph, entry: &str) -> impl Lens {
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
    Restrict(move |n: &Node| seen.contains(&n.id))
}

/// One node per role; the class node carries the role as its name.
pub fn by_role() -> impl Lens {
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
pub fn by_module() -> impl Lens {
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
