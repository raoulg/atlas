//! Two readings of a repository as one picture: the union of the graph at revision A and the graph
//! at revision B, each node, edge, step and touch marked as a diff marks lines. What B adds is
//! added, what B no longer has is removed, and what both hold is changed when its own arrows
//! differ: Yoneda, applied to the generators.
//!
//! A function is an arrow, so its identity is its own arrow: what it takes and gives, what its own
//! calls read and write, the places its calls make whole by what it hands them, and for an entry
//! what it is handed on the command line. A type is an object, so it is changed when the arrows
//! directly into or out of it differ, also when its name is the same. Only direct arrows count: a
//! change upstream marks the step that changed, never what reaches it or what it reaches, and a step
//! of a body is the same while it makes the same call on values of the same types, whichever step
//! made them.
//!
//! A node B no longer has and a node B adds whose direct relations are the same (the same typed
//! arrows in and out, the node itself and what is named under it counting as the same) are one
//! node, renamed, when that match is the only one either way. Atlas never guesses between two.
//!
//! `atlas changes` reads the same union, one reading at a time (`reading`), so the list it gives
//! holds what the picture marks.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use serde::Serialize;

use crate::model::{EdgeKind, Entry, Graph, Held, Node, NodeKind, Site, Touch};

/// How a thing of the union stands to the two readings: B adds it, B no longer has it, or both
/// hold it and its own arrows differ.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Added,
    Removed,
    Changed,
}

/// The union of two readings, as the diff marks it. What is the same in both is absent.
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct Diff {
    /// Each node's status, by its id in the union: B's id for a node B holds.
    pub nodes: BTreeMap<String, Status>,
    /// The id a renamed node has in B, against the id it had in A.
    pub renamed: BTreeMap<String, String>,
    /// The edges of the union, by their index in it.
    pub edges: BTreeMap<usize, Status>,
    /// For a node in both readings, the steps of its body (`sites`) by index: B's steps and the
    /// steps of A that B no longer has, in call order.
    pub sites: BTreeMap<String, BTreeMap<usize, Status>>,
    /// For a node in both readings, the steps both hold that A hands other values than B (a step
    /// put in before one, or taken out), by index: what A hands each, under the union's names. The
    /// step itself holds what B hands it.
    pub hands: BTreeMap<String, BTreeMap<usize, Vec<Held>>>,
    /// For a node in both readings, the steps only A has whose value B makes by a step only B has,
    /// by index: that step's index. A step both hold is handed the value under one name in both,
    /// so it is one value, made by another step.
    pub remade: BTreeMap<String, BTreeMap<usize, usize>>,
    /// For a node in both readings, its touches by index, counted as `Flow::drawn` counts them:
    /// its touches, then its calls as read (`instead`). B's, then those of A that B no longer has.
    pub touches: BTreeMap<String, BTreeMap<usize, Status>>,
    /// For a node in both readings that A has as an entry, when B has it otherwise (another usage
    /// line, or no entry): the entry as A had it.
    pub entries: BTreeMap<String, Entry>,
    /// The nodes in both readings that only B has as entries.
    #[serde(skip)]
    pub entered: BTreeSet<String>,
    /// Each place's status, by its id as the flow names it (`w:File:out.csv`), and by the coarser
    /// place it is drawn in until that is split (`w:Disk:in`): a place is changed when the calls
    /// that touch it, or the way they touch it, differ.
    pub places: BTreeMap<String, Status>,
    /// For each changed node and place, what it was in A in words: for a function its signature
    /// and the places its own calls touch, for a type the arrows into and out of it, for a place
    /// the functions that touch it and how.
    pub was: BTreeMap<String, String>,
}

/// The union of the parts read at A (`before`) and at B (`after`), as one graph named `name`, and
/// how each of its things stands to the two readings.
pub fn union(name: &str, before: Vec<Graph>, after: Vec<Graph>) -> (Graph, Diff) {
    let mut a = crate::parts::joined(name, before);
    let b = crate::parts::joined(name, after);
    apart(&mut a);
    let mut renames = renames(&a, &b);
    let types = renames.types(&a);
    // what a node was is said as A has it, before A's renamed nodes take their names in B
    let read = (!renames.direct.is_empty()).then(|| a.clone());
    renames.relabel(&mut a);
    let (ia, ib) = (Index::new(&a), Index::new(&b));
    let ir = read.as_ref().map(Index::new);
    let was_in_a = |id: &str, na: &Node| match ir.as_ref().and_then(|ir| Some((ir, *ir.nodes.get(old_id(&renames, id).as_str())?))) {
        Some((ir, n)) => was(ir, n, &|x| x.to_string()),
        None => was(&ia, na, &|x| old_id(&renames, x)),
    };
    let mut diff = Diff::default();
    for (old, new) in &renames.all {
        if ib.nodes.contains_key(new.as_str()) {
            diff.renamed.insert(new.clone(), old.clone());
        }
    }
    // a node is renamed itself when its own name is another; one named under a renamed node keeps
    // its own, and is changed only when its own arrow is
    let named_anew: HashSet<&str> = renames.direct.values().map(String::as_str).collect();

    let mut out = Graph {
        root: b.root.clone(),
        language: b.language.clone(),
        project_root: b.project_root.clone(),
        parts: b.parts.clone(),
        files: b.files.clone(),
        make: b.make.clone(),
        stats: b.stats.clone(),
        ..Default::default()
    };
    // the nodes of B, each with A's steps and touches that B no longer has; then what only A had
    for nb in &b.nodes {
        let Some(na) = ia.nodes.get(nb.id.as_str()) else {
            diff.nodes.insert(nb.id.clone(), Status::Added);
            out.nodes.push(nb.clone());
            continue;
        };
        if named_anew.contains(nb.id.as_str()) || !same_node(&ia, na, &ib, nb, &types) {
            diff.nodes.insert(nb.id.clone(), Status::Changed);
            diff.was.insert(nb.id.clone(), was_in_a(&nb.id, na));
        }
        match (&na.entry, &nb.entry) {
            (Some(ea), Some(eb)) if crate::story::usage(ea) != crate::story::usage(eb) || ea.kind != eb.kind => {
                diff.entries.insert(nb.id.clone(), ea.clone());
            }
            (Some(ea), None) => {
                diff.entries.insert(nb.id.clone(), ea.clone());
            }
            (None, Some(_)) => {
                diff.entered.insert(nb.id.clone());
            }
            _ => {}
        }
        let mut n = nb.clone();
        let body = merged_sites(&na.body().sites, &nb.body().sites);
        if let Some(b) = n.body_mut() {
            b.sites = body.sites;
        }
        if !body.marks.is_empty() {
            diff.sites.insert(n.id.clone(), body.marks);
        }
        if !body.hands.is_empty() {
            diff.hands.insert(n.id.clone(), body.hands);
        }
        if !body.remade.is_empty() {
            diff.remade.insert(n.id.clone(), body.remade);
        }
        let (own, mut marks) = merged_touches(&na.body().touches, &nb.body().touches, 0);
        let (read, more) = merged_touches(&na.body().instead, &nb.body().instead, own.len());
        marks.extend(more);
        if let Some(b) = n.body_mut() {
            (b.touches, b.instead) = (own, read);
        }
        if !marks.is_empty() {
            diff.touches.insert(n.id.clone(), marks);
        }
        out.nodes.push(n);
    }
    for na in a.nodes.iter().filter(|n| !ib.nodes.contains_key(n.id.as_str())) {
        diff.nodes.insert(na.id.clone(), Status::Removed);
        if let Some(text) = a.files.get(&na.file) {
            out.files.entry(na.file.clone()).or_insert_with(|| text.clone());
        }
        out.nodes.push(na.clone());
    }

    // the edges of B, then A's that B no longer has; an edge is its ends, its kind and what it is via
    let key = |e: &crate::model::Edge| (e.src.clone(), e.dst.clone(), e.kind, e.via.clone());
    let mut in_a: HashMap<_, usize> = HashMap::new();
    for e in &a.edges {
        *in_a.entry(key(e)).or_default() += 1;
    }
    for e in &b.edges {
        match in_a.get_mut(&key(e)).filter(|k| **k > 0) {
            Some(k) => *k -= 1,
            None => {
                diff.edges.insert(out.edges.len(), Status::Added);
            }
        }
        out.edges.push(e.clone());
    }
    let mut in_b: HashMap<_, usize> = HashMap::new();
    for e in &b.edges {
        *in_b.entry(key(e)).or_default() += 1;
    }
    for e in &a.edges {
        match in_b.get_mut(&key(e)).filter(|k| **k > 0) {
            Some(k) => *k -= 1,
            None => {
                diff.edges.insert(out.edges.len(), Status::Removed);
                out.edges.push(e.clone());
            }
        }
    }

    let (pa, pb) = (places(&a), places(&b));
    for (id, touched) in &pb {
        match pa.get(id) {
            None => {
                diff.places.insert(id.clone(), Status::Added);
            }
            Some(before) if before != touched => {
                diff.places.insert(id.clone(), Status::Changed);
                diff.was.insert(id.clone(), before.join(" · "));
            }
            Some(_) => {}
        }
    }
    for id in pa.keys().filter(|id| !pb.contains_key(*id)) {
        diff.places.insert(id.clone(), Status::Removed);
    }

    let parts: HashSet<String> = b.parts.iter().map(|p| p.id.clone()).collect();
    out.parts.extend(a.parts.iter().filter(|p| !parts.contains(&p.id)).cloned());
    let made: HashSet<String> = b.make.iter().map(|t| t.id.clone()).collect();
    out.make.extend(a.make.iter().filter(|t| !made.contains(&t.id)).cloned());
    out.recount();
    (out, diff)
}

/// One of the two readings a union holds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Reading {
    A,
    B,
}

impl Reading {
    /// Whether a thing of the union with status `s` is in this reading: what both hold, and what
    /// only this one holds.
    fn holds(self, s: Option<&Status>) -> bool {
        match s {
            None | Some(Status::Changed) => true,
            Some(Status::Removed) => self == Reading::A,
            Some(Status::Added) => self == Reading::B,
        }
    }
}

/// One reading of a union, under the union's names: the nodes, edges, steps and touches it holds,
/// what it hands each step, and its entries as it has them. The page draws a reading so, and
/// `atlas changes` compares the two: one union, so both tell one story.
pub fn reading(g: &Graph, d: &Diff, r: Reading) -> Graph {
    let mut out = Graph {
        root: g.root.clone(),
        language: g.language.clone(),
        project_root: g.project_root.clone(),
        parts: g.parts.clone(),
        make: g.make.clone(),
        stats: g.stats.clone(),
        ..Default::default()
    };
    let kept = |marks: Option<&BTreeMap<usize, Status>>, i: usize| r.holds(marks.and_then(|m| m.get(&i)));
    for n in g.nodes.iter().filter(|n| r.holds(d.nodes.get(&n.id))) {
        let mut n = n.clone();
        // what A has otherwise than the union holds it: what it hands the steps both hold, its entry
        if r == Reading::A {
            for (i, hands) in d.hands.get(&n.id).into_iter().flatten() {
                if let Some(s) = n.body_mut().and_then(|b| b.sites.get_mut(*i)) {
                    s.hands = hands.clone();
                }
            }
            if let Some(e) = d.entries.get(&n.id) {
                n.entry = Some(e.clone());
            } else if d.entered.contains(&n.id) {
                n.entry = None;
            }
        }
        let (steps, touches) = (d.sites.get(&n.id), d.touches.get(&n.id));
        if let Some(b) = n.body_mut() {
            b.sites = std::mem::take(&mut b.sites).into_iter().enumerate().filter(|(i, _)| kept(steps, *i)).map(|(_, s)| s).collect();
            let own = b.touches.len();
            b.touches = std::mem::take(&mut b.touches).into_iter().enumerate().filter(|(i, _)| kept(touches, *i)).map(|(_, t)| t).collect();
            b.instead =
                std::mem::take(&mut b.instead).into_iter().enumerate().filter(|(i, _)| kept(touches, own + i)).map(|(_, t)| t).collect();
        }
        out.nodes.push(n);
    }
    out.edges = g.edges.iter().enumerate().filter(|(i, _)| r.holds(d.edges.get(i))).map(|(_, e)| e.clone()).collect();
    out.recount();
    out
}

/// The values A's bodies name by their step (`#3`), apart from B's: each reading numbers its own.
fn apart(a: &mut Graph) {
    for b in a.nodes.iter_mut().filter_map(Node::body_mut) {
        for s in &mut b.sites {
            for h in s.hands.iter_mut().chain(s.gives.iter_mut()) {
                if let Some(k) = h.name.strip_prefix('#') {
                    h.name = format!("#a{k}");
                }
            }
        }
    }
}

/// The nodes A renamed, by their id in A: each node under one of them is renamed with it.
#[derive(Default)]
struct Renames {
    direct: BTreeMap<String, String>,
    /// Every id of A that is another in B, the nodes under a renamed one included.
    all: BTreeMap<String, String>,
}

impl Renames {
    /// The id `id` of A has in B: its own new name, or that of the nearest node it is named under.
    fn map(&self, id: &str) -> String {
        if let Some(n) = self.direct.get(id) {
            return n.clone();
        }
        let mut cut = id.len();
        while let Some(i) = id[..cut].rfind('.') {
            if let Some(n) = self.direct.get(&id[..i]) {
                return format!("{n}{}", &id[i..]);
            }
            cut = i;
        }
        id.to_string()
    }

    /// The names of the types A renamed, old against new, as signatures write them.
    fn types(&self, a: &Graph) -> Vec<(String, String)> {
        let short = |id: &str| id.rsplit('.').next().unwrap_or(id).to_string();
        a.nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Class && self.direct.contains_key(&n.id))
            .map(|n| (short(&n.id), short(&self.direct[&n.id])))
            .filter(|(o, n)| o != n)
            .collect()
    }

    /// A with every renamed node under its name in B, and every id that names one: the values its
    /// steps hand and give by their type, and a place named by the value of a renamed function or
    /// class (`plot.out`, `Store.path`) by its new name.
    fn relabel(&mut self, a: &mut Graph) {
        let ids: Vec<String> = a.nodes.iter().map(|n| n.id.clone()).collect();
        for id in ids {
            let to = self.map(&id);
            if to != id {
                self.all.insert(id, to);
            }
        }
        if self.direct.is_empty() {
            return;
        }
        // the name a place holds a value of a function by, old against new: the function's own,
        // or for a constructor its class's
        let owners: HashMap<String, (String, String)> = a
            .nodes
            .iter()
            .filter(|n| matches!(n.kind.tag(), NodeKind::Function | NodeKind::Method))
            .map(|n| {
                let owner = match (n.ctor(), n.cls()) {
                    (true, Some(c)) => c,
                    _ => n.id.as_str(),
                };
                (n.id.clone(), (short(owner), short(&self.map(owner))))
            })
            .filter(|(_, (old, new))| old != new)
            .collect();
        let m = |s: &mut String| *s = self.map(s);
        for n in &mut a.nodes {
            let id = n.id.clone();
            let Some(b) = n.body_mut() else { continue };
            for t in b.touches.iter_mut().chain(b.instead.iter_mut()) {
                let holder = match (t.by.as_slice(), t.users_of.as_slice()) {
                    ([h], _) | ([], [h]) => h.as_str(),
                    _ => id.as_str(),
                };
                if let Some((old, new)) = owners.get(holder) {
                    if let Some(rest) = t.what.strip_prefix(&format!("{old}.")) {
                        t.what = format!("{new}.{rest}");
                    }
                }
            }
        }
        for n in &mut a.nodes {
            m(&mut n.id);
            m(&mut n.module);
            if let Some(method) = n.method_mut() {
                m(&mut method.cls);
            }
            let Some(b) = n.body_mut() else { continue };
            for s in &mut b.sites {
                m(&mut s.to);
                m(&mut s.on);
                m(&mut s.after);
                s.hands.iter_mut().chain(s.gives.iter_mut()).for_each(|h| m(&mut h.ty));
            }
            for t in b.touches.iter_mut().chain(b.instead.iter_mut()) {
                for f in t.by.iter_mut() {
                    *f = f.split('&').map(|x| self.map(x)).collect::<Vec<_>>().join("&");
                }
                t.users_of.iter_mut().for_each(m);
            }
        }
        for e in &mut a.edges {
            m(&mut e.src);
            m(&mut e.dst);
        }
        for t in &mut a.make {
            t.calls.iter_mut().for_each(m);
        }
    }
}

/// The renames from A to B: a node only A has and a node only B has with the same relations, each
/// the only match of the other. A rename found lets the nodes around it match in the next round.
fn renames(a: &Graph, b: &Graph) -> Renames {
    let mut r = Renames::default();
    let (ia, ib) = (Index::new(a), Index::new(b));
    let named = |n: &&Node| n.kind != NodeKind::External;
    loop {
        let in_a: HashSet<String> = a.nodes.iter().map(|n| r.map(&n.id)).collect();
        let gone: Vec<&Node> = a.nodes.iter().filter(named).filter(|n| !ib.nodes.contains_key(r.map(&n.id).as_str())).collect();
        let new: Vec<&Node> = b.nodes.iter().filter(named).filter(|n| !in_a.contains(&n.id)).collect();
        let types = r.types(a);
        let mut by_a: HashMap<Vec<String>, Vec<&str>> = HashMap::new();
        for n in gone {
            by_a.entry(relations(&ia, n, &|o| r.map(o), &types)).or_default().push(&n.id);
        }
        let mut by_b: HashMap<Vec<String>, Vec<&str>> = HashMap::new();
        for n in new {
            by_b.entry(relations(&ib, n, &|o| o.to_string(), &[])).or_default().push(&n.id);
        }
        let found: Vec<(String, String)> = by_a
            .iter()
            .filter_map(|(sig, olds)| match (olds.as_slice(), by_b.get(sig).map(Vec::as_slice)) {
                ([old], Some([new])) => Some((old.to_string(), new.to_string())),
                _ => None,
            })
            .collect();
        if found.is_empty() {
            return r;
        }
        r.direct.extend(found);
    }
}

/// What a node is to the rest of its graph: its kind, a function's typed arrow (the types it takes
/// and gives, `types` naming A's renamed types as B does), the edges into and out of it by kind and
/// by what is at their other end, and the places its own arrow touches. The node itself, and what
/// is named under it, count as itself, also where a place is named by its value (`plot.out`).
fn relations(ix: &Index, n: &Node, map: &dyn Fn(&str) -> String, types: &[(String, String)]) -> Vec<String> {
    let under = format!("{}.", n.id);
    let other = |o: &str| {
        if o == n.id {
            "·".to_string()
        } else if let Some(rest) = o.strip_prefix(&under) {
            format!("·.{rest}")
        } else {
            map(o)
        }
    };
    let mut out = vec![format!("{:?}", n.kind.tag())];
    if matches!(n.kind.tag(), NodeKind::Function | NodeKind::Method) {
        out.push(typed_arrow(n, types));
    }
    out.extend(ix.out(&n.id).map(|e| format!("→ {:?} {} {}", e.kind, other(&e.dst), e.via.as_deref().unwrap_or(""))));
    out.extend(ix.into(&n.id).map(|e| format!("← {:?} {} {}", e.kind, other(&e.src), e.via.as_deref().unwrap_or(""))));
    let own = format!("w:Disk:{}.", n.name);
    out.extend(ix.own_touches(n).iter().map(|t| {
        let key = touch_key(t);
        format!("⇢ {}", key.strip_prefix(&own).map_or(key.clone(), |rest| format!("w:Disk:·.{rest}")))
    }));
    out.sort();
    out
}

/// A function's arrow by types alone, `(Frame, mut Path) → Frame`: what it takes, what it may change
/// in place, and what it gives, with A's renamed types under their names in B (`types`). The
/// names of its parameters are labels, no part of it.
fn typed_arrow(n: &Node, types: &[(String, String)]) -> String {
    let params: Vec<String> = n
        .params()
        .iter()
        .filter(|p| !p.receiver)
        .map(|p| renamed(&format!("{}{}", if p.changes { "mut " } else { "" }, p.ty), types))
        .collect();
    let gives = n.returns().map(|r| renamed(&r.written, types)).unwrap_or_default();
    format!("({}) → {gives}", params.join(", "))
}

/// A type as written, with each of A's renamed types under its name in B.
fn renamed(text: &str, types: &[(String, String)]) -> String {
    let mut t = text.to_string();
    for (old, new) in types {
        t = regex::Regex::new(&format!(r"\b{}\b", regex::escape(old))).map(|re| re.replace_all(&t, new.as_str()).to_string()).unwrap_or(t);
    }
    t
}

/// The arrows into and out of each place of a graph: each function whose calls touch it, and what
/// they do there (`plot writes`), by the place's id and by the coarser place it is drawn in.
fn places(g: &Graph) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for n in &g.nodes {
        for t in n.body().touches.iter().chain(&n.body().instead) {
            let w = t.drawn();
            let arrow = format!("{} {}", short_fn(&n.id, n.cls()), w.verb);
            for id in std::iter::once(w.place.id.clone()).chain(w.place.coarse.map(|c| c.id)) {
                out.entry(id).or_default().push(arrow.clone());
            }
        }
    }
    for touched in out.values_mut() {
        touched.sort();
        touched.dedup();
    }
    out
}

/// A function by the name a picture gives it: a method with its class (`Trend.save`).
fn short_fn(id: &str, cls: Option<&str>) -> String {
    match cls {
        Some(c) => format!("{}.{}", short(c), short(id)),
        None => short(id),
    }
}

/// A touch by where it rests and what it does there.
fn touch_key(t: &Touch) -> String {
    let w = t.drawn();
    format!("{} {:?} {}", w.place.id, w.side, w.verb)
}

/// Whether a node both readings hold is the same: a function by its own arrow, a type by the arrows
/// directly into and out of it. A module holds what it holds, and is the same while it is there.
fn same_node(ia: &Index, na: &Node, ib: &Index, nb: &Node, types: &[(String, String)]) -> bool {
    match nb.kind.tag() {
        NodeKind::Function | NodeKind::Method => arrow(ia, na, types).1 == arrow(ib, nb, &[]).1,
        NodeKind::Class | NodeKind::External => typed(ia, &na.id) == typed(ib, &nb.id),
        NodeKind::Module => true,
    }
}

/// A function's own arrow: in words, and as what tells it apart (the words without its name).
fn arrow(ix: &Index, n: &Node, types: &[(String, String)]) -> (String, Vec<String>) {
    let params: Vec<String> = n.params().iter().map(|p| renamed(&format!("{}{p}", if p.changes { "mut " } else { "" }), types)).collect();
    let gives = n.returns().map(|r| renamed(&r.written, types)).unwrap_or_default();
    let mut parts = vec![format!("({}){}", params.join(", "), if gives.is_empty() { String::new() } else { format!(" → {gives}") })];
    let changes: Vec<String> =
        ix.out(&n.id).filter(|e| e.kind == EdgeKind::Mutates).map(|e| e.via.clone().unwrap_or_else(|| short(&e.dst))).collect();
    if !changes.is_empty() {
        parts.push(format!("changes {}", changes.join(", ")));
    }
    let own = ix.own_touches(n);
    let mut touches: Vec<String> = own.iter().map(|t| touch_key(t)).collect();
    touches.sort();
    touches.dedup();
    let mut words: Vec<String> = own.iter().map(|t| format!("{} {}", t.verb(), t.drawn().place.label)).collect();
    words.sort();
    words.dedup();
    parts.extend(words);
    if let Some(e) = &n.entry {
        parts.push(format!("$ {}", crate::story::usage(e)));
    }
    let mut identity = vec![parts[0].clone()];
    identity.extend(parts.iter().skip(1).filter(|p| p.starts_with("changes ") || p.starts_with("$ ")).cloned());
    identity.extend(touches);
    (format!("{}{}", n.name, parts.join(" · ")), identity)
}

/// The arrows directly into and out of a type, by kind and by what is at their other end: what
/// takes it, gives it, builds it, changes it, holds it or is held by it, its bases and variants,
/// and its members.
fn typed(ix: &Index, id: &str) -> Vec<String> {
    let typing = |k: EdgeKind| {
        matches!(
            k,
            EdgeKind::Takes
                | EdgeKind::Returns
                | EdgeKind::Has
                | EdgeKind::Mutates
                | EdgeKind::Variant
                | EdgeKind::Inherits
                | EdgeKind::Instantiates
        )
    };
    let mut out: Vec<String> =
        ix.out(id).filter(|e| typing(e.kind) || e.kind == EdgeKind::Contains).map(|e| format!("→ {:?} {}", e.kind, e.dst)).collect();
    out.extend(ix.into(id).filter(|e| typing(e.kind)).map(|e| format!("← {:?} {}", e.kind, e.src)));
    out.sort();
    out
}

/// A node's own arrow in A, in words, each node under the name it had there (`as_in_a` gives the
/// id in A of a node B renamed).
fn was(ix: &Index, n: &Node, as_in_a: &dyn Fn(&str) -> String) -> String {
    let short = |id: &str| short(&as_in_a(id));
    match n.kind.tag() {
        NodeKind::Function | NodeKind::Method => arrow(ix, n, &[]).0,
        _ => {
            let mut groups: Vec<(&str, Vec<String>)> = vec![];
            let mut put = |what: &'static str, who: String| match groups.iter_mut().find(|(w, _)| *w == what) {
                Some((_, xs)) => xs.push(who),
                None => groups.push((what, vec![who])),
            };
            for e in ix.into(&n.id) {
                let who = short(&e.src);
                match e.kind {
                    EdgeKind::Returns => put("given by", who),
                    EdgeKind::Instantiates => put("built by", who),
                    EdgeKind::Takes => put("taken by", who),
                    EdgeKind::Mutates => put("changed by", who),
                    EdgeKind::Has => put("held by", who),
                    EdgeKind::Inherits => put("base of", who),
                    EdgeKind::Variant => put("a variant of", who),
                    _ => {}
                }
            }
            for e in ix.out(&n.id) {
                let what = e.via.clone().map(|v| format!("{v}: {}", short(&e.dst))).unwrap_or_else(|| short(&e.dst));
                match e.kind {
                    EdgeKind::Has => put("holds", what),
                    EdgeKind::Inherits => put("a kind of", what),
                    EdgeKind::Variant => put("one of", what),
                    EdgeKind::Contains => put("with", what),
                    _ => {}
                }
            }
            let text: Vec<String> = groups
                .into_iter()
                .map(|(w, mut xs)| {
                    xs.sort();
                    xs.dedup();
                    let more = xs.len().saturating_sub(4);
                    xs.truncate(4);
                    format!("{w} {}{}", xs.join(", "), if more > 0 { format!(" +{more}") } else { String::new() })
                })
                .collect();
            format!("{}: {}", n.name, text.join(" · "))
        }
    }
}

fn short(id: &str) -> String {
    id.trim_start_matches("ext:").rsplit('.').next().unwrap_or(id).to_string()
}

/// A body's steps in both readings, as `merged_sites` unites them.
struct Merged {
    sites: Vec<Site>,
    marks: BTreeMap<usize, Status>,
    /// The shared steps A hands other values than B, by index: what A hands them (`Diff::hands`).
    hands: BTreeMap<usize, Vec<Held>>,
    /// The steps only A has whose value a step only B has makes, by index (`Diff::remade`).
    remade: BTreeMap<usize, usize>,
}

/// The steps of a body in both readings, in call order, and which of them only one reading has:
/// the steps both have once, as B has them; where the two part, A's before B's, as a diff
/// interleaves lines. A step is the same in both when it makes the same call on values of the same
/// types; where a value comes from is the arrow of the step that made it, so a change before a step
/// leaves it the same step. A shared step aliases A's anonymous values to B's names, those it gives
/// and those it is handed from a step only A has, so each reading keeps its own chain of values;
/// what A hands a shared step otherwise than B (a step put in before it, one taken out) is kept
/// beside it. Where two of A's values would take one name in the union, the shared step that names
/// them so is one step removed and one added.
fn merged_sites(a: &[Site], b: &[Site]) -> Merged {
    let (ka, kb) = (site_shapes(a), site_shapes(b));
    // the longest run of steps both share, from the end
    let mut lcs = vec![vec![0u32; kb.len() + 1]; ka.len() + 1];
    for i in (0..ka.len()).rev() {
        for j in (0..kb.len()).rev() {
            lcs[i][j] = if ka[i] == kb[j] { lcs[i + 1][j + 1] + 1 } else { lcs[i + 1][j].max(lcs[i][j + 1]) };
        }
    }
    let mut pairs = vec![];
    let (mut i, mut j) = (0, 0);
    while i < ka.len() && j < kb.len() {
        if ka[i] == kb[j] {
            pairs.push((i, j));
            (i, j) = (i + 1, j + 1);
        } else if lcs[i + 1][j] >= lcs[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    loop {
        let aliases = aliased(a, b, &pairs);
        let Some(k) = unreadable(a, b, &pairs, &aliases) else { return interleaved(a, b, &pairs, &aliases) };
        pairs.remove(k);
    }
}

/// The names A's anonymous values have in the union, with the shared step each comes from: what a
/// shared step gives, and what it is handed from a step only A has, under B's name. A value a
/// shared step gives in B keeps that step's name.
fn aliased(a: &[Site], b: &[Site], pairs: &[(usize, usize)]) -> HashMap<String, (String, usize)> {
    let mut aliases = HashMap::new();
    for (k, &(i, j)) in pairs.iter().enumerate() {
        for (old, new) in a[i].gives.iter().zip(&b[j].gives) {
            if old.name.starts_with('#') && !new.name.is_empty() {
                aliases.entry(old.name.clone()).or_insert((new.name.clone(), k));
            }
        }
    }
    let made: HashSet<&str> = pairs.iter().flat_map(|&(_, j)| &b[j].gives).map(|h| h.name.as_str()).collect();
    for (k, &(i, j)) in pairs.iter().enumerate() {
        for (old, new) in a[i].hands.iter().zip(&b[j].hands) {
            if old.name.starts_with('#') && !new.name.is_empty() && !made.contains(new.name.as_str()) {
                aliases.entry(old.name.clone()).or_insert((new.name.clone(), k));
            }
        }
    }
    aliases
}

/// The first shared step (by its place in `pairs`) that makes reading A of the union another
/// program: a step handed, under the union's names, a value another step made than the one A hands
/// it, where two of A's values take one name. None when A reads as A.
fn unreadable(a: &[Site], b: &[Site], pairs: &[(usize, usize)], aliases: &HashMap<String, (String, usize)>) -> Option<usize> {
    let shared: HashMap<usize, (usize, usize)> = pairs.iter().enumerate().map(|(k, &(i, j))| (i, (k, j))).collect();
    let name = |h: &Held| aliases.get(&h.name).map_or_else(|| h.name.clone(), |(n, _)| n.clone());
    let (mut in_a, mut in_union): (HashMap<&str, usize>, HashMap<String, usize>) = (HashMap::new(), HashMap::new());
    for (i, s) in a.iter().enumerate() {
        let hands: Vec<String> = s.hands.iter().map(name).collect();
        for (h, n) in s.hands.iter().zip(&hands) {
            if in_a.get(h.name.as_str()) != in_union.get(n) {
                // a shared step undone is A's again; a step only A has is misnamed by the alias it
                // was handed, or else by the nearest shared step before it
                return match shared.get(&i) {
                    Some(&(k, _)) => Some(k),
                    None => aliases.get(&h.name).map(|(_, k)| *k).or_else(|| pairs.iter().rposition(|&(p, _)| p < i)),
                };
            }
        }
        let gives: Vec<String> = match shared.get(&i) {
            Some(&(_, j)) => b[j].gives.iter().map(|h| h.name.clone()).collect(),
            None => s.gives.iter().map(name).collect(),
        };
        for (g, n) in s.gives.iter().zip(gives) {
            if !g.name.is_empty() {
                in_a.insert(&g.name, i);
            }
            if !n.is_empty() {
                in_union.insert(n, i);
            }
        }
    }
    None
}

/// The steps of both readings in call order, the `pairs` once as B has them, A's other steps
/// before B's between them, with A's anonymous values under their names in the union, and what A
/// hands a shared step where that differs from what B hands it.
fn interleaved(a: &[Site], b: &[Site], pairs: &[(usize, usize)], aliases: &HashMap<String, (String, usize)>) -> Merged {
    let in_union = |hs: &mut Vec<Held>| {
        for h in hs {
            if let Some((n, _)) = aliases.get(&h.name) {
                h.name = n.clone();
            }
        }
    };
    let (mut out, mut marks, mut hands) = (vec![], BTreeMap::new(), BTreeMap::new());
    let (mut i, mut j) = (0, 0);
    for &(pi, pj) in pairs.iter().chain(std::iter::once(&(a.len(), b.len()))) {
        while i < pi {
            let mut s = a[i].clone();
            in_union(&mut s.hands);
            in_union(&mut s.gives);
            marks.insert(out.len(), Status::Removed);
            out.push(s);
            i += 1;
        }
        while j < pj {
            marks.insert(out.len(), Status::Added);
            out.push(b[j].clone());
            j += 1;
        }
        if pi < a.len() {
            let mut handed = a[pi].hands.clone();
            in_union(&mut handed);
            if handed != b[pj].hands {
                hands.insert(out.len(), handed);
            }
            out.push(b[pj].clone());
            (i, j) = (pi + 1, pj + 1);
        }
    }
    let remade = remade(&out, &marks, &hands);
    Merged { sites: out, marks, hands, remade }
}

/// The steps only A has whose value a step only B has makes: a shared step is handed it under one
/// name in both readings, the one step making it in A and the other in B. By their index in the
/// union.
fn remade(sites: &[Site], marks: &BTreeMap<usize, Status>, hands: &BTreeMap<usize, Vec<Held>>) -> BTreeMap<usize, usize> {
    let (mut in_a, mut in_b): (HashMap<&str, usize>, HashMap<&str, usize>) = (HashMap::new(), HashMap::new());
    let mut out = BTreeMap::new();
    for (u, s) in sites.iter().enumerate() {
        let mark = marks.get(&u);
        if mark.is_none() {
            for (ha, hb) in hands.get(&u).unwrap_or(&s.hands).iter().zip(&s.hands) {
                let (Some(&i), Some(&j)) = (in_a.get(ha.name.as_str()), in_b.get(hb.name.as_str())) else { continue };
                if ha.name == hb.name && marks.get(&i) == Some(&Status::Removed) && marks.get(&j) == Some(&Status::Added) {
                    out.entry(i).or_insert(j);
                }
            }
        }
        for g in s.gives.iter().filter(|g| !g.name.is_empty()) {
            if mark != Some(&Status::Added) {
                in_a.insert(g.name.as_str(), u);
            }
            if mark != Some(&Status::Removed) {
                in_b.insert(g.name.as_str(), u);
            }
        }
    }
    out
}

#[derive(PartialEq, Eq)]
enum Input {
    Named(String),
    Output(usize),
}

#[derive(PartialEq, Eq)]
struct SiteShape {
    to: String,
    op: String,
    on: String,
    named: bool,
    after: String,
    hands: Vec<(String, Input)>,
    gives: Vec<(String, Option<String>)>,
}

/// A step's own arrow in its body: the call, the types of what it is handed and gives, and of each
/// input whether the body has it from its start (by name) or from an earlier step (by which of
/// that step's values it is). Which step made an input is no part of it, nor are step numbers.
fn site_shapes(sites: &[Site]) -> Vec<SiteShape> {
    let (mut shapes, mut values) = (vec![], HashMap::new());
    for s in sites {
        let hands = s
            .hands
            .iter()
            .map(|h| {
                (
                    h.ty.clone(),
                    match values.get(&h.name) {
                        Some(slot) => Input::Output(*slot),
                        None => Input::Named(h.name.clone()),
                    },
                )
            })
            .collect();
        let gives = s
            .gives
            .iter()
            .enumerate()
            .map(|(slot, h)| {
                if !h.name.is_empty() {
                    values.insert(h.name.clone(), slot);
                }
                (h.ty.clone(), (!h.name.starts_with('#')).then(|| h.name.clone()))
            })
            .collect();
        shapes.push(SiteShape {
            to: s.to.clone(),
            op: s.op.clone(),
            on: s.on.clone(),
            named: s.named,
            after: s.after.clone(),
            hands,
            gives,
        });
    }
    shapes
}

/// The touches of a node in both readings: B's, then A's that B no longer has, each by where it
/// rests and what it does there; marked by index from `from` on.
fn merged_touches(a: &[Touch], b: &[Touch], from: usize) -> (Vec<Touch>, BTreeMap<usize, Status>) {
    let mut left: HashMap<String, usize> = HashMap::new();
    for t in a {
        *left.entry(touch_key(t)).or_default() += 1;
    }
    let (mut out, mut marks) = (vec![], BTreeMap::new());
    for t in b {
        match left.get_mut(&touch_key(t)).filter(|k| **k > 0) {
            Some(k) => *k -= 1,
            None => {
                marks.insert(from + out.len(), Status::Added);
            }
        }
        out.push(t.clone());
    }
    let mut kept: HashMap<String, usize> = HashMap::new();
    for t in b {
        *kept.entry(touch_key(t)).or_default() += 1;
    }
    for t in a {
        match kept.get_mut(&touch_key(t)).filter(|k| **k > 0) {
            Some(k) => *k -= 1,
            None => {
                marks.insert(from + out.len(), Status::Removed);
                out.push(t.clone());
            }
        }
    }
    (out, marks)
}

/// Whether a touch is of a file named by one of `h`'s own parameters, as a value is named where
/// it is whole (`load.path`, and `Store.path` for what a constructor takes): what `h` is handed,
/// not what it hands.
fn holds_open(h: &Node, t: &Touch) -> bool {
    let owner = match (h.ctor(), h.cls()) {
        (true, Some(c)) => short(c),
        _ => h.name.clone(),
    };
    t.kind == crate::model::World::Disk
        && t.artifact.is_empty()
        && t.what.strip_prefix(&format!("{owner}.")).map_or(false, |p| h.params().iter().any(|q| !q.receiver && q.name == p))
}

/// The id a node of the union had in A: its old one when B renamed it.
fn old_id(r: &Renames, id: &str) -> String {
    r.all.iter().find(|(_, new)| new.as_str() == id).map_or_else(|| id.to_string(), |(old, _)| old.clone())
}

/// A graph's nodes by id, its edges by their ends, and the touches each function makes whole by
/// what its calls hand.
struct Index<'g> {
    nodes: HashMap<&'g str, &'g Node>,
    out: HashMap<&'g str, Vec<&'g crate::model::Edge>>,
    into: HashMap<&'g str, Vec<&'g crate::model::Edge>>,
    made: HashMap<&'g str, Vec<&'g Touch>>,
}

impl<'g> Index<'g> {
    fn new(g: &'g Graph) -> Self {
        let (mut out, mut into): (HashMap<&str, Vec<_>>, HashMap<&str, Vec<_>>) = (HashMap::new(), HashMap::new());
        for e in &g.edges {
            out.entry(e.src.as_str()).or_default().push(e);
            into.entry(e.dst.as_str()).or_default().push(e);
        }
        let nodes: HashMap<&str, &Node> = g.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
        let mut made: HashMap<&str, Vec<&Touch>> = HashMap::new();
        for t in g.nodes.iter().flat_map(|n| &n.body().touches) {
            for f in t.by.iter().flat_map(|f| f.split('&')) {
                // a file named by a value the caller is handed itself (`Store.path`) waits for the
                // caller's own callers: the caller does not make it whole
                if !nodes.get(f).map_or(false, |h| holds_open(h, t)) {
                    made.entry(f).or_default().push(t);
                }
            }
        }
        Index { nodes, out, into, made }
    }

    /// The touches of a function's own arrow: those its own calls make, whole in its body or as
    /// read where its callers make them whole, and those its calls make whole by what it hands
    /// them. A place a caller makes whole is the caller's arrow, not the callee's.
    fn own_touches(&self, n: &'g Node) -> Vec<&'g Touch> {
        let own = n.body().touches.iter().filter(|t| t.by.is_empty() && t.users_of.is_empty()).chain(&n.body().instead);
        own.chain(self.made.get(n.id.as_str()).into_iter().flatten().copied()).collect()
    }

    fn out(&self, id: &str) -> impl Iterator<Item = &'g crate::model::Edge> + '_ {
        self.out.get(id).into_iter().flatten().copied()
    }

    fn into(&self, id: &str) -> impl Iterator<Item = &'g crate::model::Edge> + '_ {
        self.into.get(id).into_iter().flatten().copied()
    }
}
