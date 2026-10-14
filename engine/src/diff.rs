//! Two readings of a repository as one picture: the union of the graph at revision A and the graph
//! at revision B, each node, edge, step and touch marked as a diff marks lines. What B adds is
//! added, what B no longer has is removed, and what both hold is changed when its own arrows
//! differ: Yoneda, applied to the generators.
//!
//! A function is an arrow, so its identity is its own arrow: what it takes and gives, what its own
//! calls read and write, and for an entry what it is handed on the command line. A type is an
//! object, so it is changed when the arrows directly into or out of it differ, also when its name
//! is the same. Only direct arrows count: a change upstream marks the step that changed, never
//! what reaches it or what it reaches.
//!
//! A node B no longer has and a node B adds whose direct relations are the same (the same typed
//! arrows in and out, the node itself and what is named under it counting as the same) are one
//! node, renamed, when that match is the only one either way. Atlas never guesses between two.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::Serialize;

use crate::model::{EdgeKind, Entry, Graph, Node, NodeKind, Site, Touch};

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
    /// For a node in both readings, its touches by index, counted as `Flow::drawn` counts them:
    /// its touches, then its calls as read (`instead`). B's, then those of A that B no longer has.
    pub touches: BTreeMap<String, BTreeMap<usize, Status>>,
    /// For an entry in both readings whose usage line differs: the entry as A had it.
    pub entries: BTreeMap<String, Entry>,
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
    renames.relabel(&mut a);
    let (ia, ib) = (Index::new(&a), Index::new(&b));
    let mut diff = Diff::default();
    for (old, new) in &renames.all {
        if ib.nodes.contains_key(new.as_str()) {
            diff.renamed.insert(new.clone(), old.clone());
        }
    }

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
        if diff.renamed.contains_key(&nb.id) || !same_node(&ia, na, &ib, nb, &types) {
            diff.nodes.insert(nb.id.clone(), Status::Changed);
            diff.was.insert(nb.id.clone(), was(&ia, na, &|id| old_id(&renames, id)));
        }
        if let (Some(ea), Some(eb)) = (&na.entry, &nb.entry) {
            if crate::story::usage(ea) != crate::story::usage(eb) || ea.kind != eb.kind {
                diff.entries.insert(nb.id.clone(), ea.clone());
            }
        }
        let mut n = nb.clone();
        let (steps, marks) = merged_sites(&na.sites, &nb.sites);
        n.sites = steps;
        if !marks.is_empty() {
            diff.sites.insert(n.id.clone(), marks);
        }
        let (own, mut marks) = merged_touches(&na.touches, &nb.touches, 0);
        let (read, more) = merged_touches(&na.instead, &nb.instead, own.len());
        marks.extend(more);
        n.touches = own;
        n.instead = read;
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

/// The values A's bodies name by their step (`#3`), apart from B's: each reading numbers its own.
fn apart(a: &mut Graph) {
    for n in &mut a.nodes {
        for s in &mut n.sites {
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

    /// A with every renamed node under its name in B, and every id that names one.
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
        let m = |s: &mut String| *s = self.map(s);
        for n in &mut a.nodes {
            m(&mut n.id);
            m(&mut n.module);
            if let Some(c) = n.cls.as_mut() {
                m(c);
            }
            for s in &mut n.sites {
                m(&mut s.to);
                m(&mut s.on);
            }
            for t in n.touches.iter_mut().chain(n.instead.iter_mut()) {
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
        let mut by_a: HashMap<Vec<String>, Vec<&str>> = HashMap::new();
        for n in gone {
            by_a.entry(relations(&ia, n, &|o| r.map(o))).or_default().push(&n.id);
        }
        let mut by_b: HashMap<Vec<String>, Vec<&str>> = HashMap::new();
        for n in new {
            by_b.entry(relations(&ib, n, &|o| o.to_string())).or_default().push(&n.id);
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

/// What a node is to the rest of its graph: its kind, the edges into and out of it by kind and by
/// what is at their other end, and the places its own calls touch. The node itself, and what is
/// named under it, count as itself.
fn relations(ix: &Index, n: &Node, map: &dyn Fn(&str) -> String) -> Vec<String> {
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
    let mut out = vec![format!("{:?}", n.kind)];
    out.extend(ix.out(&n.id).map(|e| format!("→ {:?} {} {}", e.kind, other(&e.dst), e.via.as_deref().unwrap_or(""))));
    out.extend(ix.into(&n.id).map(|e| format!("← {:?} {} {}", e.kind, other(&e.src), e.via.as_deref().unwrap_or(""))));
    out.extend(own_touches(n).iter().map(|t| format!("⇢ {}", touch_key(t))));
    out.sort();
    out
}

/// The arrows into and out of each place of a graph: each function whose calls touch it, and what
/// they do there (`plot writes`), by the place's id and by the coarser place it is drawn in.
fn places(g: &Graph) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for n in &g.nodes {
        for t in n.touches.iter().chain(&n.instead) {
            let w = t.drawn();
            let arrow = format!("{} {}", short_fn(&n.id, n.cls.as_deref()), w.verb);
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

/// The touches a function's own calls make: whole in its own body, or as read where its callers
/// make them whole. What a caller hands is the caller's arrow, not this one's.
fn own_touches(n: &Node) -> Vec<&Touch> {
    n.touches.iter().filter(|t| t.by.is_empty() && t.users_of.is_empty()).chain(&n.instead).collect()
}

/// Whether a node both readings hold is the same: a function by its own arrow, a type by the arrows
/// directly into and out of it. A module holds what it holds, and is the same while it is there.
fn same_node(ia: &Index, na: &Node, ib: &Index, nb: &Node, types: &[(String, String)]) -> bool {
    match nb.kind {
        NodeKind::Function | NodeKind::Method => arrow(ia, na, types).1 == arrow(ib, nb, &[]).1,
        NodeKind::Class | NodeKind::External => typed(ia, &na.id) == typed(ib, &nb.id),
        NodeKind::Module => true,
    }
}

/// A function's own arrow: in words, and as what tells it apart (the words without its name).
fn arrow(ix: &Index, n: &Node, types: &[(String, String)]) -> (String, Vec<String>) {
    let renamed = |text: &str| {
        let mut t = text.to_string();
        for (old, new) in types {
            t = regex::Regex::new(&format!(r"\b{}\b", regex::escape(old)))
                .map(|re| re.replace_all(&t, new.as_str()).to_string())
                .unwrap_or(t);
        }
        t
    };
    let params: Vec<String> = n.params.iter().map(|p| renamed(&format!("{}{p}", if p.changes { "mut " } else { "" }))).collect();
    let gives = n.returns.as_ref().map(|r| renamed(&r.written)).unwrap_or_default();
    let mut parts = vec![format!("({}){}", params.join(", "), if gives.is_empty() { String::new() } else { format!(" → {gives}") })];
    let changes: Vec<String> =
        ix.out(&n.id).filter(|e| e.kind == EdgeKind::Mutates).map(|e| e.via.clone().unwrap_or_else(|| short(&e.dst))).collect();
    if !changes.is_empty() {
        parts.push(format!("changes {}", changes.join(", ")));
    }
    let mut touches: Vec<String> = own_touches(n).iter().map(|t| touch_key(t)).collect();
    touches.sort();
    touches.dedup();
    let mut words: Vec<String> = own_touches(n).iter().map(|t| format!("{} {}", t.verb(), t.drawn().place.label)).collect();
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
    match n.kind {
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

/// The steps of a body in both readings, in call order, and which of them only one reading has:
/// the steps both have once, as B has them; where the two part, A's before B's, as a diff
/// interleaves lines. A shared occurrence has the same call shape and consumes outputs of
/// shared producers. Each shared output aliases A's anonymous name to B's name.
fn merged_sites(a: &[Site], b: &[Site]) -> (Vec<Site>, BTreeMap<usize, Status>) {
    let ((ka, da), (kb, db)) = (site_shapes(a), site_shapes(b));
    // the longest run of steps both share, from the end
    let mut lcs = vec![vec![0u32; kb.len() + 1]; ka.len() + 1];
    for i in (0..ka.len()).rev() {
        for j in (0..kb.len()).rev() {
            lcs[i][j] = if ka[i] == kb[j] { lcs[i + 1][j + 1] + 1 } else { lcs[i + 1][j].max(lcs[i][j + 1]) };
        }
    }
    let (mut out, mut marks) = (vec![], BTreeMap::new());
    let (mut shared, mut aliases) = (HashMap::new(), HashMap::new());
    let (mut i, mut j) = (0, 0);
    while i < ka.len() || j < kb.len() {
        let same = i < ka.len()
            && j < kb.len()
            && ka[i] == kb[j]
            && da[i].iter().zip(&db[j]).all(|(x, y)| match (x, y) {
                (Some(x), Some(y)) => shared.get(x) == Some(y),
                (None, None) => true,
                _ => false,
            });
        if same {
            shared.insert(i, j);
            for (old, new) in a[i].gives.iter().zip(&b[j].gives) {
                if old.name.starts_with('#') && !new.name.is_empty() {
                    aliases.insert(old.name.clone(), new.name.clone());
                }
            }
            out.push(b[j].clone());
            i += 1;
            j += 1;
        } else if i < ka.len() && (j == kb.len() || lcs[i + 1][j] >= lcs[i][j + 1]) {
            marks.insert(out.len(), Status::Removed);
            out.push(a[i].clone());
            i += 1;
        } else {
            marks.insert(out.len(), Status::Added);
            out.push(b[j].clone());
            j += 1;
        }
    }
    for (&i, status) in &marks {
        if *status == Status::Removed {
            let s = &mut out[i];
            for h in s.hands.iter_mut().chain(s.gives.iter_mut()) {
                if let Some(name) = aliases.get(&h.name) {
                    h.name = name.clone();
                }
            }
        }
    }
    (out, marks)
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
    hands: Vec<(String, Input)>,
    gives: Vec<(String, Option<String>)>,
}

/// Call shapes are independent of step numbers. Dependencies retain each input's producer;
/// an output slot is comparable only after the corresponding producers have been shared.
fn site_shapes(sites: &[Site]) -> (Vec<SiteShape>, Vec<Vec<Option<usize>>>) {
    let (mut shapes, mut dependencies, mut values) = (vec![], vec![], HashMap::new());
    for (i, s) in sites.iter().enumerate() {
        let mut deps = vec![];
        let hands = s
            .hands
            .iter()
            .map(|h| {
                let value = values.get(&h.name).copied();
                deps.push(value.map(|(producer, _)| producer));
                (
                    h.ty.clone(),
                    match value {
                        Some((_, slot)) => Input::Output(slot),
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
                    values.insert(h.name.clone(), (i, slot));
                }
                (h.ty.clone(), (!h.name.starts_with('#')).then(|| h.name.clone()))
            })
            .collect();
        shapes.push(SiteShape { to: s.to.clone(), op: s.op.clone(), on: s.on.clone(), hands, gives });
        dependencies.push(deps);
    }
    (shapes, dependencies)
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

/// The id a node of the union had in A: its old one when B renamed it.
fn old_id(r: &Renames, id: &str) -> String {
    r.all.iter().find(|(_, new)| new.as_str() == id).map_or_else(|| id.to_string(), |(old, _)| old.clone())
}

/// A graph's nodes by id and its edges by their ends.
struct Index<'g> {
    nodes: HashMap<&'g str, &'g Node>,
    out: HashMap<&'g str, Vec<&'g crate::model::Edge>>,
    into: HashMap<&'g str, Vec<&'g crate::model::Edge>>,
}

impl<'g> Index<'g> {
    fn new(g: &'g Graph) -> Self {
        let (mut out, mut into): (HashMap<&str, Vec<_>>, HashMap<&str, Vec<_>>) = (HashMap::new(), HashMap::new());
        for e in &g.edges {
            out.entry(e.src.as_str()).or_default().push(e);
            into.entry(e.dst.as_str()).or_default().push(e);
        }
        Index { nodes: g.nodes.iter().map(|n| (n.id.as_str(), n)).collect(), out, into }
    }

    fn out(&self, id: &str) -> impl Iterator<Item = &'g crate::model::Edge> + '_ {
        self.out.get(id).into_iter().flatten().copied()
    }

    fn into(&self, id: &str) -> impl Iterator<Item = &'g crate::model::Edge> + '_ {
        self.into.get(id).into_iter().flatten().copied()
    }
}
