//! A `Graph` under construction: what a language module fills while it parses, with the
//! scratch it needs for resolution. It never leaves `Language::parse`.

use std::collections::{BTreeMap, HashMap, HashSet};

use indexmap::IndexMap;

use crate::model::{Edge, EdgeKind, Graph, Node, NodeKind, Stats, Touch, World};

/// Per-class facts that only matter during resolution.
#[derive(Default, Clone, Debug)]
pub struct ClassInfo {
    pub bases: Vec<Vec<String>>,
    pub resolved: Vec<String>,
    pub attr_types: BTreeMap<String, String>,
    pub fields: Vec<(String, Vec<Vec<String>>)>,
    /// For a sum (a Rust `enum`): the types its variants carry.
    pub variants: Vec<Vec<String>>,
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
    /// Calls whose caller uses what they give back, as (caller, callee).
    pub used: HashSet<(String, String)>,
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
            self.edges.push(Edge { src: src.into(), dst: dst.into(), kind, via, weight: None, arms, uses: false });
        }
    }

    /// Record a boundary call on a node; the same call again counts up.
    pub fn touch(&mut self, id: &str, t: Touch) {
        let Some(n) = self.nodes.get_mut(id) else { return };
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
        let mut nodes = self.nodes;
        crate::flow::wire(&mut nodes, &self.edges);
        mark_doors(&mut nodes, &self.edges);
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
            nodes.values().filter(|m| m.cls.as_deref() == Some(c.id.as_str()) || m.id.starts_with(&prefix) && m.kind == NodeKind::Method).filter(|m| operation(m)).find_map(|m| reaches(&m.id)).map(|k| (c.id.clone(), k))
        })
        .collect();
    for (id, kind) in doors {
        if let Some(n) = nodes.get_mut(&id) {
            n.door = Some(kind);
        }
    }
}

/// The types a language's code describes itself with rather than holds data in: its standard
/// library and the packages that describe code (a UI library's components, a schema library's
/// validators), and the names that wrap or stand for a value.
pub struct OwnTypes {
    pub packages: &'static [&'static str],
    pub names: &'static [&'static str],
}

/// A foreign type worth a place: a package's type that holds data. Not one of the language's
/// own, and not one reached through a type parameter (`D::Error`, `T.Item`).
pub fn foreign_type(id: &str, own: &OwnTypes) -> bool {
    let Some(full) = id.strip_prefix("ext:") else { return false };
    let head = full.split('.').next().unwrap_or("");
    let name = full.rsplit('.').next().unwrap_or("");
    let parameter = head.len() == 1 && head.chars().all(|c| c.is_ascii_uppercase());
    full.contains('.') && !own.packages.contains(&head) && !own.names.contains(&name) && !parameter
}
