//! A route as text: an entry and its steps in call order, each with its own signature and its
//! own effects. Alternatives share a number and are lettered by branch:
//! the arms of a choice in the body, or the overrides a call may run.

use std::collections::{BTreeMap, HashMap};

use crate::flow::{self, Body, Item, Step};
use crate::model::{Edge, EdgeKind, EntryKind, Graph, Held, Node, NodeKind, Site, Touch, World};

/// Every route into the program: its kind, its label and the id of the node it starts at.
pub fn entries(g: &Graph) -> Vec<(EntryKind, &str, &str)> {
    let mut out: Vec<(EntryKind, &str, &str)> = g.nodes.iter().filter_map(|n| n.entry.as_ref().map(|e| (e.kind, e.label.as_str(), n.id.as_str()))).collect();
    out.sort_by_key(|(k, l, _)| (kind_order(*k), *l));
    out
}

/// The route that starts at `entry` (an id, or an entry's label), its steps opened `depth`
/// levels deep: the same steps, from the same bodies, as the page draws.
pub fn route(g: &Graph, entry: &str, depth: usize) -> Option<String> {
    let s = Story::new(g);
    let start = s.find(entry)?;
    let n = s.nodes[start];
    let kind = n.entry.as_ref().map(|e| format!("{} · {}", kind_label(e.kind), e.label)).unwrap_or_else(|| s.name(start, ""));
    let mut out = format!("{kind}\n{}{}", n.id, s.signature(n));
    let effects = s.effects(n);
    if !effects.is_empty() {
        out.push_str(&format!("   ·   {effects}"));
    }
    out.push('\n');
    let mut shown: HashMap<String, String> = HashMap::from([(start.to_string(), "the entry".to_string())]);
    s.walk(start, "", depth.max(1), &mut shown, &mut out);
    Some(out)
}

pub fn kind_label(k: EntryKind) -> &'static str {
    match k {
        EntryKind::Make => "Make",
        EntryKind::Cli => "CLI",
        EntryKind::Script => "Script",
        EntryKind::Route => "API",
        EntryKind::Mcp => "MCP",
        EntryKind::Notebook => "Notebook",
        EntryKind::Export => "Library",
        EntryKind::Test => "Test",
    }
}

fn kind_order(k: EntryKind) -> usize {
    [EntryKind::Make, EntryKind::Cli, EntryKind::Export, EntryKind::Script, EntryKind::Notebook, EntryKind::Route, EntryKind::Mcp, EntryKind::Test]
        .iter()
        .position(|x| *x == k)
        .unwrap_or(9)
}

struct Story<'g> {
    g: &'g Graph,
    nodes: HashMap<&'g str, &'g Node>,
    out: HashMap<&'g str, Vec<&'g Edge>>,
    bodies: BTreeMap<String, Body>,
}

impl<'g> Story<'g> {
    fn new(g: &'g Graph) -> Self {
        let mut out: HashMap<&str, Vec<&Edge>> = HashMap::new();
        for e in &g.edges {
            out.entry(e.src.as_str()).or_default().push(e);
        }
        Story { g, nodes: g.nodes.iter().map(|n| (n.id.as_str(), n)).collect(), out, bodies: flow::bodies(g) }
    }

    fn find(&self, entry: &str) -> Option<&'g str> {
        let n = self.g.nodes.iter().find(|n| n.id == entry).or_else(|| self.g.nodes.iter().find(|n| n.entry.as_ref().map_or(false, |e| e.label == entry)))?;
        Some(n.id.as_str())
    }

    fn edges(&self, id: &str, kind: EdgeKind) -> impl Iterator<Item = &'g Edge> + '_ {
        self.out.get(id).into_iter().flatten().copied().filter(move |e| e.kind == kind)
    }

    /// The steps of the body at `body` (a function, or a function on a class), each choice's
    /// branches lettered, then the overrides the call may run instead.
    fn walk(&self, body: &str, prefix: &str, depth: usize, shown: &mut HashMap<String, String>, out: &mut String) {
        let Some(b) = self.bodies.get(body) else { return };
        let holder = body.split('@').next().unwrap_or(body);
        let sites = self.nodes.get(holder).map(|n| n.sites.as_slice()).unwrap_or(&[]);
        let mut seq = 0;
        let mut next = || {
            seq += 1;
            if prefix.is_empty() { seq.to_string() } else { format!("{prefix}.{seq}") }
        };
        let level = prefix.matches('.').count() + usize::from(!prefix.is_empty());
        for item in &b.items {
            match item {
                Item::Step(st) => self.step(st, st.site.and_then(|i| sites.get(i)), &next(), level, depth, shown, out),
                Item::Choice { members, .. } => {
                    let k = next();
                    for m in members {
                        self.step(&m.step, m.step.site.and_then(|i| sites.get(i)), &format!("{k}{}", letter(m.branch)), level, depth, shown, out);
                    }
                }
            }
        }
        for (i, o) in b.alts.iter().enumerate() {
            let st = Step { to: o.clone(), on: String::new(), site: None };
            self.step(&st, None, &format!("{prefix}{}", letter(i)), level, depth, shown, out);
        }
    }

    fn step(&self, st: &Step, site: Option<&Site>, num: &str, level: usize, depth: usize, shown: &mut HashMap<String, String>, out: &mut String) {
        let indent = "  ".repeat(level);
        let Some(n) = self.nodes.get(st.to.as_str()) else {
            // a library's operation: what the call hands it and what it gives, by name
            let names = |hs: &[Held]| hs.iter().map(|h| if h.name.starts_with('#') { "·".to_string() } else { h.name.clone() }).collect::<Vec<_>>().join(", ");
            let (hands, gives) = site.map(|s| (names(&s.hands), names(&s.gives))).unwrap_or_default();
            let op = st.to.strip_prefix("op:").unwrap_or(&st.to);
            out.push_str(&format!("{indent}{num}  {op}({hands}){}\n", if gives.is_empty() { String::new() } else { format!(" → {gives}") }));
            return;
        };
        let k = flow::key(&st.to, &st.on);
        let body = self.bodies.get(&k);
        if let Some(at) = shown.get(&k).filter(|_| body.is_some()) {
            out.push_str(&format!("{indent}{num}  {}  → see {at}\n", self.name(&st.to, &st.on)));
            return;
        }
        shown.insert(k.clone(), num.to_string());
        let mut line = format!("{indent}{num}  {}{}", self.name(&st.to, &st.on), self.signature(n));
        let effects = self.effects(n);
        if !effects.is_empty() {
            line.push_str(&format!("   ·   {effects}"));
        }
        let alts = body.map_or(0, |b| b.alts.len());
        if alts > 0 {
            line.push_str(&format!("   ·   one of {alts}"));
        }
        out.push_str(&line);
        out.push('\n');
        if level + 1 < depth {
            self.walk(&k, num, depth, shown, out);
        }
    }

    /// `Graph()` for a type built in place, `BasePlot` for its constructor (the signature
    /// follows), `BasePlot.plot`, `ComparePlot.plot` for a method run on a subclass, `analyze`.
    fn name(&self, id: &str, on: &str) -> String {
        let n = self.nodes[id];
        if n.kind == NodeKind::Class {
            return format!("{}()", n.name);
        }
        let class = |c: &str| self.nodes.get(c).map_or_else(|| c.rsplit('.').next().unwrap_or(c).to_string(), |c| c.name.clone());
        match n.cls.as_deref() {
            Some(c) if n.ctor && !n.on_class => class(c),
            Some(_) if !on.is_empty() => format!("{}.{}", class(on), n.name),
            Some(c) => format!("{}.{}", class(c), n.name),
            None => n.name.clone(),
        }
    }

    /// `(repo, package) → Graph  ↻ Graph`: the parameters by name, what comes back, what changes.
    fn signature(&self, n: &Node) -> String {
        if !matches!(n.kind, NodeKind::Function | NodeKind::Method) {
            return String::new();
        }
        let names: Vec<&str> = n.params.iter().filter(|p| !p.receiver).map(|p| p.name.as_str()).collect();
        let class = n.cls.as_deref().and_then(|c| self.nodes.get(c)).map(|c| c.name.clone());
        let ret = match &n.returns {
            Some(r) if r.own => class.unwrap_or_default(),
            Some(r) => r.value.clone(),
            None => String::new(),
        };
        let changes: Vec<String> = self.edges(&n.id, EdgeKind::Mutates).filter_map(|e| self.nodes.get(e.dst.as_str()).map(|t| t.name.clone())).collect();
        let mut out = format!("({})", names.join(", "));
        if !ret.is_empty() {
            out.push_str(&format!(" → {ret}"));
        }
        if !changes.is_empty() {
            out.push_str(&format!("  ↻ {}", changes.join(", ")));
        }
        out
    }

    /// What a step's own body does at the edges: `reads Makefile · writes *.png · prints`.
    fn effects(&self, n: &Node) -> String {
        let mut groups: Vec<(String, Vec<String>)> = vec![];
        for t in &n.touches {
            let (v, what) = (t.verb(), place(t));
            match groups.iter_mut().find(|(g, _)| *g == v) {
                Some((_, items)) => {
                    if !what.is_empty() && !items.contains(&what) {
                        items.push(what);
                    }
                }
                None => groups.push((v, if what.is_empty() { vec![] } else { vec![what] })),
            }
        }
        groups.into_iter().map(|(v, items)| if items.is_empty() { v } else { format!("{v} {}", items.join(" | ")) }).collect::<Vec<_>>().join(" · ")
    }
}

fn letter(i: usize) -> char {
    (b'a' + (i % 26) as u8) as char
}

/// Where a touch lands, as the code names it.
fn place(t: &Touch) -> String {
    if t.kind == World::Screen {
        return String::new();
    }
    if !t.artifact.is_empty() {
        return t.artifact.clone();
    }
    crate::flow::shown(&t.what)
}
