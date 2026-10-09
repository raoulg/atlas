//! A `Graph` under construction: what a language module fills while it parses, with the
//! scratch it needs for resolution. It never leaves `Language::parse`.

use std::collections::{BTreeMap, HashMap, HashSet};

use indexmap::IndexMap;

use crate::model::{Edge, EdgeKind, Graph, Node, NodeKind, Stats, Touch};

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

/// The nodes and edges found so far, and the tables that resolve names into them.
#[derive(Default)]
pub struct GraphBuilder {
    pub nodes: IndexMap<String, Node>,
    pub edges: Vec<Edge>,
    pub edge_set: HashSet<(String, String, EdgeKind)>,
    pub classes: HashMap<String, ClassInfo>,
    pub sigs: HashMap<String, SigInfo>,
    pub unresolved: HashMap<String, u32>,
    pub files: BTreeMap<String, String>,
    /// Module id -> local name -> fully qualified target, kept for re-export lookups.
    pub bindings: HashMap<String, HashMap<String, String>>,
    /// Methods that change their receiver in place.
    pub mutating: HashSet<String>,
}

impl GraphBuilder {
    pub fn add_node(&mut self, n: Node) {
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
            self.edges.push(Edge { src: src.into(), dst: dst.into(), kind, via, weight: None, arms });
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

    /// The graph, once every file is parsed: the scratch is dropped, the names that did not
    /// resolve are counted.
    pub fn finish(self, root: String, project_root: Option<&std::path::Path>, language: &str) -> Graph {
        let mut top: Vec<(String, u32)> = self.unresolved.iter().map(|(k, v)| (k.clone(), *v)).collect();
        top.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let mut g = Graph {
            root,
            language: language.into(),
            project_root: project_root.map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
            nodes: self.nodes.into_values().collect(),
            edges: self.edges,
            files: self.files,
            stats: Stats {
                unresolved_calls_total: self.unresolved.values().sum(),
                unresolved_calls_top: top.into_iter().take(25).collect(),
                ..Default::default()
            },
            ..Default::default()
        };
        g.recount();
        g
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

