//! A route as text: an entry and its steps in call order, each with its own signature and its
//! own effects, as Story shows them. Alternatives share a number and are lettered by branch:
//! the arms of a choice in the body, or the overrides a call may run.

use std::collections::{HashMap, HashSet};

use crate::model::{Edge, EdgeKind, EntryKind, Graph, Node, NodeKind, Touch};

/// Every route into the program: its kind, its label and the id of the node it starts at.
pub fn entries(g: &Graph) -> Vec<(EntryKind, &str, &str)> {
    let mut out: Vec<(EntryKind, &str, &str)> = g.nodes.iter().filter_map(|n| n.entry.as_ref().map(|e| (e.kind, e.label.as_str(), n.id.as_str()))).collect();
    out.sort_by_key(|(k, l, _)| (kind_order(*k), *l));
    out
}

/// The route that starts at `entry` (an id, or an entry's label), its steps opened `depth`
/// levels deep.
pub fn route(g: &Graph, entry: &str, depth: usize) -> Option<String> {
    let s = Story::new(g);
    let start = s.find(entry)?;
    let n = s.nodes[start];
    let kind = n.entry.as_ref().map(|e| format!("{} · {}", kind_label(e.kind), e.label)).unwrap_or_else(|| s.name(start));
    let mut out = format!("{kind}\n{}{}", n.id, s.signature(n));
    let effects = s.effects(n);
    if !effects.is_empty() {
        out.push_str(&format!("   ·   {effects}"));
    }
    out.push('\n');
    let mut shown: HashMap<&str, String> = HashMap::from([(start, "the entry".to_string())]);
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

/// One step of a body, or the alternatives of one choice in it.
enum Item<'g> {
    Step(&'g str),
    Choice(Vec<(&'g str, usize)>),
}

struct Story<'g> {
    g: &'g Graph,
    nodes: HashMap<&'g str, &'g Node>,
    out: HashMap<&'g str, Vec<&'g Edge>>,
    into: HashMap<&'g str, Vec<&'g Edge>>,
}

impl<'g> Story<'g> {
    fn new(g: &'g Graph) -> Self {
        let mut out: HashMap<&str, Vec<&Edge>> = HashMap::new();
        let mut into: HashMap<&str, Vec<&Edge>> = HashMap::new();
        for e in &g.edges {
            out.entry(e.src.as_str()).or_default().push(e);
            into.entry(e.dst.as_str()).or_default().push(e);
        }
        Story { g, nodes: g.nodes.iter().map(|n| (n.id.as_str(), n)).collect(), out, into }
    }

    fn find(&self, entry: &str) -> Option<&'g str> {
        let n = self.g.nodes.iter().find(|n| n.id == entry).or_else(|| self.g.nodes.iter().find(|n| n.entry.as_ref().map_or(false, |e| e.label == entry)))?;
        Some(n.id.as_str())
    }

    fn edges(&self, id: &str, kind: EdgeKind) -> impl Iterator<Item = &'g Edge> + '_ {
        self.out.get(id).into_iter().flatten().copied().filter(move |e| e.kind == kind)
    }

    /// What a step calls and builds, in call order. A library class is its constructor and
    /// its public methods.
    fn callees(&self, id: &str) -> Vec<&'g str> {
        let Some(n) = self.nodes.get(id) else { return vec![] };
        if n.kind == NodeKind::Class && n.entry.is_some() {
            let init = format!("{id}.__init__");
            let mut out: Vec<&str> = self.nodes.get_key_value(init.as_str()).map(|(k, _)| *k).into_iter().collect();
            out.extend(self.edges(id, EdgeKind::Contains).map(|e| e.dst.as_str()).filter(|m| self.nodes.get(m).map_or(false, |m| m.kind == NodeKind::Method && !m.name.starts_with('_'))));
            return out;
        }
        let mut seen = HashSet::new();
        self.out
            .get(id)
            .into_iter()
            .flatten()
            .filter(|e| matches!(e.kind, EdgeKind::Calls | EdgeKind::Instantiates) && self.nodes.contains_key(e.dst.as_str()))
            .map(|e| e.dst.as_str())
            .filter(|d| seen.insert(*d))
            .collect()
    }

    /// The overrides a call to a method may run: Language.parse runs Python.parse or Rust.parse.
    fn overrides(&self, id: &str) -> Vec<&'g str> {
        let Some(n) = self.nodes.get(id) else { return vec![] };
        let Some(cls) = n.cls.as_deref() else { return vec![] };
        let (mut out, mut stack, mut seen) = (vec![], vec![cls.to_string()], HashSet::new());
        while let Some(c) = stack.pop() {
            for e in self.into.get(c.as_str()).into_iter().flatten().filter(|e| e.kind == EdgeKind::Inherits) {
                if seen.insert(e.src.as_str()) {
                    stack.push(e.src.clone());
                    let m = format!("{}.{}", e.src, n.name);
                    if let Some((k, _)) = self.nodes.get_key_value(m.as_str()) {
                        out.push(*k);
                    }
                }
            }
        }
        out
    }

    /// The body's steps, each choice with two or more branches as one item.
    fn items(&self, id: &str) -> Vec<Item<'g>> {
        let calls = self.callees(id);
        let mut arms: HashMap<&str, &Vec<(u32, u32)>> = HashMap::new();
        for e in self.out.get(id).into_iter().flatten().filter(|e| matches!(e.kind, EdgeKind::Calls | EdgeKind::Instantiates) && !e.arms.is_empty()) {
            arms.entry(e.dst.as_str()).or_insert(&e.arms);
        }
        let choice_of = |c: &str| arms.get(c).map(|a| a[0].0);
        let mut items = vec![];
        let mut placed = HashSet::new();
        for &c in &calls {
            let Some(k) = choice_of(c) else {
                items.push(Item::Step(c));
                continue;
            };
            let members: Vec<&str> = calls.iter().copied().filter(|m| choice_of(m) == Some(k)).collect();
            let mut paths: Vec<&Vec<(u32, u32)>> = vec![];
            for m in &members {
                if !paths.contains(&arms[m]) {
                    paths.push(arms[m]);
                }
            }
            if paths.len() < 2 {
                items.push(Item::Step(c));
            } else if placed.insert(k) {
                items.push(Item::Choice(members.iter().map(|m| (*m, paths.iter().position(|p| *p == arms[m]).unwrap_or(0))).collect()));
            }
        }
        items
    }

    fn walk(&self, id: &str, prefix: &str, depth: usize, shown: &mut HashMap<&'g str, String>, out: &mut String) {
        let mut seq = 0;
        let mut next = || {
            seq += 1;
            if prefix.is_empty() { seq.to_string() } else { format!("{prefix}.{seq}") }
        };
        let level = prefix.matches('.').count() + usize::from(!prefix.is_empty());
        for item in self.items(id) {
            match item {
                Item::Step(c) => self.step(c, &next(), level, depth, shown, out),
                Item::Choice(members) => {
                    let k = next();
                    for (m, branch) in members {
                        self.step(m, &format!("{k}{}", letter(branch)), level, depth, shown, out);
                    }
                }
            }
        }
        for (i, o) in self.overrides(id).into_iter().enumerate() {
            self.step(o, &format!("{prefix}{}", letter(i)), level, depth, shown, out);
        }
    }

    fn step(&self, id: &'g str, num: &str, level: usize, depth: usize, shown: &mut HashMap<&'g str, String>, out: &mut String) {
        let n = self.nodes[id];
        let indent = "  ".repeat(level);
        let kids = !self.callees(id).is_empty() || !self.overrides(id).is_empty();
        if let Some(at) = shown.get(id).filter(|_| kids) {
            out.push_str(&format!("{indent}{num}  {}  → see {at}\n", self.name(id)));
            return;
        }
        shown.insert(id, num.to_string());
        let mut line = format!("{indent}{num}  {}{}", self.name(id), self.signature(n));
        let effects = self.effects(n);
        if !effects.is_empty() {
            line.push_str(&format!("   ·   {effects}"));
        }
        let alts = self.overrides(id).len();
        if alts > 0 {
            line.push_str(&format!("   ·   one of {alts}"));
        }
        out.push_str(&line);
        out.push('\n');
        if level + 1 < depth {
            self.walk(id, num, depth, shown, out);
        }
    }

    /// `Graph()` for a type built in place, `BasePlot` for its constructor (the signature
    /// follows), `BasePlot.plot`, `analyze`.
    fn name(&self, id: &str) -> String {
        let n = self.nodes[id];
        if n.kind == NodeKind::Class {
            return format!("{}()", n.name);
        }
        match n.cls.as_deref().and_then(|c| self.nodes.get(c)) {
            Some(c) if n.name == "__init__" => c.name.clone(),
            Some(c) => format!("{}.{}", c.name, n.name),
            None => n.name.clone(),
        }
    }

    /// `(repo, package) → Graph  ↻ Graph`: the parameters by name, what comes back, what changes.
    fn signature(&self, n: &Node) -> String {
        if !matches!(n.kind, NodeKind::Function | NodeKind::Method) {
            return String::new();
        }
        let names: Vec<&str> = n
            .params
            .iter()
            .map(|p| p.split([':', '=']).next().unwrap_or(p).trim().trim_start_matches('*').trim_start_matches("mut ").trim())
            .filter(|p| !matches!(p.trim_start_matches('&').trim_start_matches("mut ").trim(), "self" | "cls"))
            .collect();
        let ret = n.returns.strip_prefix("Result<").and_then(|r| r.strip_suffix('>')).unwrap_or(&n.returns).replace("&mut ", "").replace("'static ", "").replace('&', "");
        let changes: Vec<String> = self.edges(&n.id, EdgeKind::Mutates).filter_map(|e| self.nodes.get(e.dst.as_str()).map(|t| t.name.clone())).collect();
        let mut out = format!("({})", names.join(", "));
        if !ret.is_empty() && ret != "None" && ret != "()" {
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
            let (v, what) = (verb(t), place(t));
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

/// The verb for a touch: the operation when it says more than the kind, then the direction
/// when the kind leaves it open (a database statement, a transfer over the web).
fn verb(t: &Touch) -> String {
    let by_op = match t.op.as_str() {
        "exists" | "try_exists" | "is_file" | "is_dir" => Some("checks"),
        "read_dir" | "iterdir" | "glob" | "rglob" | "listdir" | "scandir" | "walk" => Some("lists"),
        o if o.starts_with("create_dir") || o.starts_with("mkdir") || o.starts_with("makedirs") => Some("creates"),
        o if o.starts_with("remove") || o.starts_with("unlink") || o.starts_with("rmtree") => Some("removes"),
        _ => None,
    };
    let by_dir = match (t.kind.as_str(), t.dir.as_str()) {
        (k, "read") if k.starts_with("db_") => Some("reads"),
        (k, "write") if k.starts_with("db_") => Some("writes"),
        ("net", "read") => Some("downloads"),
        ("net", "write") => Some("uploads"),
        ("screen", _) if t.op == "render" => Some("renders"),
        _ => None,
    };
    if let Some(v) = by_dir.filter(|_| t.kind.starts_with("db_")).or(by_op).or(by_dir) {
        return v.into();
    }
    match t.kind.as_str() {
        "net" => "calls",
        "disk_read" => "reads",
        "disk_write" => "writes",
        "disk" => "opens",
        "llm" => "asks an LLM",
        "model" => "runs a model",
        "process" => "runs",
        "screen" => "prints",
        k if k.starts_with("db_") => "queries",
        k => k,
    }
    .into()
}

/// Where a touch lands, as the code names it.
fn place(t: &Touch) -> String {
    if t.kind == "screen" {
        return String::new();
    }
    if !t.artifact.is_empty() {
        return t.artifact.clone();
    }
    t.what.clone()
}
