//! Everything a parser produces before it becomes a `Graph`, shared by all languages.

use std::collections::{BTreeMap, HashMap, HashSet};

use indexmap::IndexMap;

use crate::model::{Edge, EdgeKind, Node, NodeKind, Touch};

/// Per-class facts that only matter during resolution.
#[derive(Default, Clone, Debug)]
pub struct ClassInfo {
    pub bases: Vec<Vec<String>>,
    pub resolved: Vec<String>,
    pub attr_types: BTreeMap<String, String>,
    pub fields: Vec<(String, Vec<Vec<String>>)>,
}

#[derive(Default, Clone, Debug)]
pub struct SigInfo {
    pub ptypes: Vec<Vec<String>>,
    pub rtypes: Vec<Vec<String>>,
}

/// Everything the parser produces before it becomes a `Graph`.
#[derive(Default)]
pub struct Store {
    pub nodes: IndexMap<String, Node>,
    pub edges: Vec<Edge>,
    pub edge_set: HashSet<(String, String, EdgeKind)>,
    pub classes: HashMap<String, ClassInfo>,
    pub sigs: HashMap<String, SigInfo>,
    pub unresolved: HashMap<String, u32>,
    pub files: BTreeMap<String, String>,
    /// Module id -> local name -> fully qualified target, kept for re-export lookups.
    pub bindings: HashMap<String, HashMap<String, String>>,
}

impl Store {
    pub fn add_node(&mut self, n: Node) {
        self.nodes.insert(n.id.clone(), n);
    }

    pub fn add_edge(&mut self, src: &str, dst: &str, kind: EdgeKind, via: Option<String>) {
        if src == dst {
            return;
        }
        let key = (src.to_string(), dst.to_string(), kind);
        if self.edge_set.insert(key) {
            self.edges.push(Edge { src: src.into(), dst: dst.into(), kind, via, weight: None });
        }
    }

    /// Record a boundary call on a node; the same call again counts up.
    pub fn touch(&mut self, id: &str, t: Touch) {
        let Some(n) = self.nodes.get_mut(id) else { return };
        *n.boundaries.entry(t.kind.clone()).or_insert(0) += 1;
        match n.touches.iter_mut().find(|x| x.kind == t.kind && x.what == t.what && x.op == t.op && x.artifact == t.artifact && x.dir == t.dir) {
            Some(x) => {
                x.n += 1;
                if x.payload.is_empty() {
                    x.payload = t.payload;
                }
            }
            None => n.touches.push(Touch { n: 1, ..t }),
        }
    }

    pub fn ext_node(&mut self, eid: &str) {
        if self.nodes.contains_key(eid) {
            return;
        }
        let full = &eid[4..];
        let (module, name) = full.rsplit_once('.').unwrap_or(("", full));
        self.add_node(Node {
            id: eid.into(),
            kind: NodeKind::External,
            name: name.into(),
            module: module.into(),
            ..Default::default()
        });
    }
}

