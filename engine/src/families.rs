//! The types of a program as the picture groups them. A family is rooted in a base the program uses
//! as a type (some signature takes or returns it, a field holds it, a sum names it); a subclass
//! stands in its base's place, a part in its one owner's. A base that only shares code (a config
//! mixin, pydantic's `BaseModel`, torch's `Module`) is how a class is built, not what flows.
//!
//! A shaped array (`Float[Tensor, batch dim]`) stands in its rank family, named by its dtype, array
//! type and rank (`Float Tensor · 2D`), where the graph holds two types of that rank: the family is a
//! quotient of the shapes, its members stay types of their own.
//!
//! A type is data, or a door to a part of the world (a sink, a repository, a client): what a
//! family's member reaches makes the whole family a door.

use std::collections::{BTreeMap, HashMap, HashSet};

use indexmap::IndexMap;
use serde::Serialize;

use crate::model::{Edge, EdgeKind, Graph, NodeKind};

/// What one field of a type holds that other types hold too: inside the type, as a reference.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Shared {
    pub via: String,
    pub to: String,
}

/// The types of a graph, grouped.
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct Families {
    /// The base a class stands in for, where that is not the class itself: the outermost base
    /// the program uses as a type.
    pub kind: BTreeMap<String, String>,
    /// The family a type belongs to, where that is not the type itself: its kind, or the family
    /// of its one owner.
    pub family: BTreeMap<String, String>,
    /// A family's members, in the order of the graph.
    pub members: BTreeMap<String, Vec<String>>,
    /// What a family holds that other types hold too.
    pub shared: BTreeMap<String, Vec<Shared>>,
    /// The types that are doors to the world, by their family.
    pub doors: Vec<String>,
    /// Who makes each type (gives it back, builds it, changes it) and who takes it, in the
    /// program's own code, in the order of its edges, as pairs: a type crosses between two
    /// packages when one makes it and the other takes it.
    pub makes: Vec<(String, Vec<String>)>,
    pub takes: Vec<(String, Vec<String>)>,
}

struct Ix<'g> {
    kinds: HashMap<&'g str, NodeKind>,
    out: HashMap<&'g str, Vec<&'g Edge>>,
    into: HashMap<&'g str, Vec<&'g Edge>>,
    used_as_type: HashSet<&'g str>,
    /// The rank family of each shaped array whose rank another type of the graph shares.
    ranks: HashMap<&'g str, String>,
}

impl<'g> Ix<'g> {
    fn new(g: &'g Graph) -> Self {
        let mut out: HashMap<&str, Vec<&Edge>> = HashMap::new();
        let mut into: HashMap<&str, Vec<&Edge>> = HashMap::new();
        for e in &g.edges {
            out.entry(e.src.as_str()).or_default().push(e);
            into.entry(e.dst.as_str()).or_default().push(e);
        }
        let used_as_type = g
            .edges
            .iter()
            .filter(|e| matches!(e.kind, EdgeKind::Takes | EdgeKind::Returns | EdgeKind::Has | EdgeKind::Mutates | EdgeKind::Variant))
            .map(|e| e.dst.as_str())
            .collect();
        let shaped: Vec<(&str, String)> = g
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::External)
            .filter_map(|n| Some((n.id.as_str(), crate::shapes::family_id(&n.id)?)))
            .collect();
        let mut fiber: HashMap<&str, usize> = HashMap::new();
        for (_, f) in &shaped {
            *fiber.entry(f.as_str()).or_default() += 1;
        }
        let ranks = shaped.iter().filter(|(_, f)| fiber[f.as_str()] > 1).map(|(t, f)| (*t, f.clone())).collect();
        Ix { kinds: g.nodes.iter().map(|n| (n.id.as_str(), n.kind)).collect(), out, into, used_as_type, ranks }
    }

    fn is_class(&self, id: &str) -> bool {
        self.kinds.get(id) == Some(&NodeKind::Class)
    }

    /// The outermost ancestor the program uses as a type, through bases that only share code; for
    /// a shaped array, its rank family.
    fn used_base(&self, id: &str, depth: usize) -> Option<String> {
        if let Some(f) = self.ranks.get(id) {
            return Some(f.clone());
        }
        let bases: Vec<&str> = self
            .out
            .get(id)
            .into_iter()
            .flatten()
            .filter(|e| e.kind == EdgeKind::Inherits && self.is_class(&e.dst))
            .map(|e| e.dst.as_str())
            .collect();
        for b in bases {
            if let Some(up) = (depth < 10).then(|| self.used_base(b, depth + 1)).flatten() {
                return Some(up);
            }
            if self.used_as_type.contains(b) {
                return Some(b.to_string());
            }
        }
        None
    }

    fn family_of(&self, id: &str, depth: usize, memo: &mut HashMap<String, String>) -> String {
        if let Some(f) = memo.get(id) {
            return f.clone();
        }
        let fam = match self.used_base(id, depth) {
            Some(up) => up,
            None => {
                let mut owners: Vec<&str> = vec![];
                for e in self.into.get(id).into_iter().flatten().filter(|e| e.kind == EdgeKind::Has && self.is_class(&e.src)) {
                    if !owners.contains(&e.src.as_str()) {
                        owners.push(e.src.as_str());
                    }
                }
                match owners.as_slice() {
                    [o] if *o != id && depth < 10 => self.family_of(o, depth + 1, memo),
                    _ => id.to_string(),
                }
            }
        };
        memo.insert(id.to_string(), fam.clone());
        fam
    }
}

/// The families of a graph, its doors, and who makes and takes each type.
pub fn families(g: &Graph) -> Families {
    let ix = Ix::new(g);
    let mut out = Families::default();
    let mut memo = HashMap::new();
    let types: Vec<&str> =
        g.nodes.iter().filter(|n| matches!(n.kind, NodeKind::Class | NodeKind::External)).map(|n| n.id.as_str()).collect();
    for &t in &types {
        let f = ix.family_of(t, 0, &mut memo);
        if f != t {
            out.members.entry(f).or_default().push(t.to_string());
        }
    }
    for &t in &types {
        if let Some(k) = ix.used_base(t, 0) {
            out.kind.insert(t.to_string(), k);
        }
    }
    for (t, f) in &memo {
        if f != t {
            out.family.insert(t.clone(), f.clone());
        }
    }
    for &f in &types {
        let in_family: HashSet<&str> = std::iter::once(f).chain(out.members.get(f).into_iter().flatten().map(String::as_str)).collect();
        let shared: Vec<Shared> = ix
            .out
            .get(f)
            .into_iter()
            .flatten()
            .filter(|e| e.kind == EdgeKind::Has && !in_family.contains(e.dst.as_str()) && ix.is_class(&e.dst))
            .filter_map(|e| e.via.clone().map(|via| Shared { via, to: e.dst.clone() }))
            .collect();
        if !shared.is_empty() {
            out.shared.insert(f.to_string(), shared);
        }
    }
    // a family is a door when one of its kin is: the type, its family, the family's members
    let door: HashSet<&str> = g.nodes.iter().filter(|n| n.door.is_some()).map(|n| n.id.as_str()).collect();
    for &t in &types {
        let fam = memo.get(t).cloned().unwrap_or_else(|| t.to_string());
        let kin =
            std::iter::once(t.to_string()).chain(std::iter::once(fam.clone())).chain(out.members.get(&fam).into_iter().flatten().cloned());
        if kin.into_iter().any(|k| door.contains(k.as_str())) {
            out.doors.push(t.to_string());
        }
    }
    let library: HashSet<&str> = g.nodes.iter().filter(|n| n.library).map(|n| n.id.as_str()).collect();
    let (mut makes, mut takes): (IndexMap<String, Vec<String>>, IndexMap<String, Vec<String>>) = Default::default();
    for e in g.edges.iter().filter(|e| !library.contains(e.src.as_str())) {
        let use_ = match e.kind {
            EdgeKind::Takes => &mut takes,
            EdgeKind::Returns | EdgeKind::Instantiates | EdgeKind::Mutates => &mut makes,
            _ => continue,
        };
        let by = use_.entry(e.dst.clone()).or_default();
        if !by.contains(&e.src) {
            by.push(e.src.clone());
        }
    }
    out.makes = makes.into_iter().collect();
    out.takes = takes.into_iter().collect();
    out
}
