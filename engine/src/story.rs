//! A route as text: an entry and its steps in call order, each with its own signature and its
//! own effects. Alternatives share a number and are lettered by branch: the arms of a choice in
//! the body, each with what decides it (`when not raw.exists()`), or the overrides a call may run.
//! A choice with one arm is lettered too: its steps run when it holds.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::flow::{self, Body, Item, Step};
use crate::model::{Edge, EdgeKind, Entry, EntryKind, Graph, Held, Node, NodeKind, Site, Touch, World};

/// Every route into the program: its kind, its label and the id of the node it starts at.
pub fn entries(g: &Graph) -> Vec<(EntryKind, &str, &str)> {
    let mut out: Vec<(EntryKind, &str, &str)> =
        g.nodes.iter().filter_map(|n| n.entry.as_ref().map(|e| (e.kind, e.label.as_str(), n.id.as_str()))).collect();
    out.sort_by_key(|(k, l, _)| (kind_order(*k), *l));
    out
}

/// The route that starts at `entry` (an id, or an entry's label), its steps opened `depth`
/// levels deep: the same steps, from the same bodies, as the page draws.
pub fn route(g: &Graph, entry: &str, depth: usize) -> Option<String> {
    let mut s = Story::new(g);
    let start = s.find(entry)?;
    // the route shows the places its own calls hand
    s.reached = g.reach(start);
    s.entry = start.to_string();
    let n = s.nodes[start];
    s.parses = n.entry.as_ref().map_or(false, |e| matches!(e.kind, EntryKind::Cli | EntryKind::Script));
    let kind = n.entry.as_ref().map(|e| format!("{} · {}", kind_label(e.kind), usage(e))).unwrap_or_else(|| s.name(start, ""));
    let mut out = format!("{kind}\n{}{}", n.id, s.signature(n));
    let path = [start.to_string()];
    let effects = s.effects(n, Some(&path));
    if !effects.is_empty() {
        out.push_str(&format!("   ·   {effects}"));
    }
    out.push('\n');
    let mut shown: HashMap<String, String> = HashMap::from([(s.seen(start, start, &path), "the entry".to_string())]);
    s.walk(start, "", depth.max(1), &mut shown, &mut out, &path);
    Some(out)
}

/// The choices a route makes, each where it is made, in call order: what decides each branch as
/// the code writes it, and the steps the branch runs with what they touch. A choice of one branch
/// runs nothing otherwise. What a condition means, Atlas does not say: this is for an agent to
/// read the conditions and say it.
pub fn choices(g: &Graph, entry: &str) -> Option<String> {
    let mut s = Story::new(g);
    let start = s.find(entry)?;
    s.reached = g.reach(start);
    s.entry = start.to_string();
    let n = s.nodes[start];
    let kind = n.entry.as_ref().map(|e| format!("{} · {}", kind_label(e.kind), usage(e))).unwrap_or_else(|| s.name(start, ""));
    let mut out = format!("{kind}\n");
    let (mut seen, mut queue) = (HashSet::new(), std::collections::VecDeque::from([(start.to_string(), String::new())]));
    let mut found = 0;
    while let Some((id, on)) = queue.pop_front() {
        let k = flow::key(&id, &on);
        let Some(b) = s.bodies.get(&k).filter(|_| seen.insert(k.clone())) else { continue };
        let sites = s.nodes.get(id.as_str()).map(|n| n.sites.as_slice()).unwrap_or(&[]);
        let named = |st: &Step| {
            if s.nodes.contains_key(st.to.as_str()) {
                s.name(&st.to, &st.on)
            } else {
                st.to.trim_start_matches("op:").to_string()
            }
        };
        let touched = |st: &Step| s.nodes.get(st.to.as_str()).map(|n| s.effects(n, None)).unwrap_or_default();
        for item in &b.items {
            let steps: Vec<&Step> = match item {
                Item::Step(st) => vec![st],
                Item::Choice { members, when, .. } => {
                    found += 1;
                    let line = members.iter().filter_map(|m| m.step.site.and_then(|i| sites.get(i))).map(|s| s.line).min().unwrap_or(0);
                    out.push_str(&format!("\n{} · line {line}\n", s.name(&id, &on)));
                    let mut branches: Vec<usize> = members.iter().map(|m| m.branch).collect();
                    branches.sort_unstable();
                    branches.dedup();
                    for br in &branches {
                        let runs: Vec<&Step> = members.iter().filter(|m| m.branch == *br).map(|m| &m.step).collect();
                        let words = match when.get(*br).map(String::as_str) {
                            None | Some("") => String::new(),
                            Some("otherwise") => "  otherwise:".to_string(),
                            Some(w) => format!("  when {w}:"),
                        };
                        let effects: Vec<String> = runs.iter().map(|st| touched(st)).filter(|e| !e.is_empty()).collect();
                        out.push_str(&format!(
                            "  {}{words}  {}{}\n",
                            letter(*br),
                            runs.iter().map(|st| named(st)).collect::<Vec<_>>().join(", "),
                            if effects.is_empty() { String::new() } else { format!("   ·   {}", effects.join(" · ")) }
                        ));
                    }
                    if branches.len() == 1 {
                        out.push_str("  -  otherwise: nothing\n");
                    }
                    members.iter().map(|m| &m.step).collect()
                }
            };
            queue.extend(steps.into_iter().filter(|st| s.nodes.contains_key(st.to.as_str())).map(|st| (st.to.clone(), st.on.clone())));
        }
        queue.extend(b.alts.iter().map(|o| (o.clone(), String::new())));
    }
    if found == 0 {
        out.push_str("\nNo choice on this route: every step runs whenever the route does.\n");
    }
    Some(out)
}

/// An entry as the human runs it: its label, then what it is handed on the command line
/// (`sunspots --raw --figure --months`).
pub fn usage(e: &Entry) -> String {
    std::iter::once(e.label.as_str()).chain(e.options.iter().map(String::as_str)).collect::<Vec<_>>().join(" ")
}

/// The steps of a body that belong to the entry of a route run from the command line: the parses
/// of the command line, and the steps whose values only those parses take.
pub fn entry_steps(sites: &[Site]) -> HashSet<usize> {
    let parses: HashSet<usize> = (0..sites.len()).filter(|&i| sites[i].command_line.is_some()).collect();
    let takers = |name: &str| (0..sites.len()).filter(|&i| sites[i].hands.iter().any(|h| h.name == name)).collect::<Vec<_>>();
    let only_parsed = |name: &str| {
        let t = takers(name);
        !name.is_empty() && !t.is_empty() && t.iter().all(|i| parses.contains(i))
    };
    let mut out = parses.clone();
    out.extend((0..sites.len()).filter(|&i| !sites[i].gives.is_empty() && sites[i].gives.iter().all(|g| only_parsed(&g.name))));
    out
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
        EntryKind::Prophecy => "Prophecy",
        EntryKind::Test => "Test",
    }
}

fn kind_order(k: EntryKind) -> usize {
    [
        EntryKind::Make,
        EntryKind::Cli,
        EntryKind::Export,
        EntryKind::Script,
        EntryKind::Notebook,
        EntryKind::Route,
        EntryKind::Mcp,
        EntryKind::Prophecy,
        EntryKind::Test,
    ]
    .iter()
    .position(|x| *x == k)
    .unwrap_or(9)
}

struct Story<'g> {
    g: &'g Graph,
    nodes: HashMap<&'g str, &'g Node>,
    out: HashMap<&'g str, Vec<&'g Edge>>,
    bodies: BTreeMap<String, Body>,
    /// The route's entry, and what it can run.
    entry: String,
    reached: HashSet<String>,
    /// The route is run from the command line: its parses of the command line are the entry's.
    parses: bool,
    /// For each function, the callers that make a path whole for a touch it reaches.
    makers: std::cell::RefCell<HashMap<String, HashSet<String>>>,
}

impl<'g> Story<'g> {
    fn new(g: &'g Graph) -> Self {
        let mut out: HashMap<&str, Vec<&Edge>> = HashMap::new();
        for e in &g.edges {
            out.entry(e.src.as_str()).or_default().push(e);
        }
        Story {
            g,
            nodes: g.nodes.iter().map(|n| (n.id.as_str(), n)).collect(),
            out,
            bodies: flow::bodies(g),
            entry: String::new(),
            reached: Default::default(),
            parses: false,
            makers: Default::default(),
        }
    }

    fn find(&self, entry: &str) -> Option<&'g str> {
        let n = self
            .g
            .nodes
            .iter()
            .find(|n| n.id == entry)
            .or_else(|| self.g.nodes.iter().find(|n| n.entry.as_ref().map_or(false, |e| e.label == entry)))?;
        Some(n.id.as_str())
    }

    fn edges(&self, id: &str, kind: EdgeKind) -> impl Iterator<Item = &'g Edge> + '_ {
        self.out.get(id).into_iter().flatten().copied().filter(move |e| e.kind == kind)
    }

    /// The steps of the body at `body` (a function, or a function on a class), each choice's
    /// branches lettered, then the overrides the call may run instead. `path` is the functions on the
    /// calls that lead into the body.
    fn walk(&self, body: &str, prefix: &str, depth: usize, shown: &mut HashMap<String, String>, out: &mut String, path: &[String]) {
        let Some(b) = self.bodies.get(body) else { return };
        let holder = body.split('@').next().unwrap_or(body);
        let sites = self.nodes.get(holder).map(|n| n.sites.as_slice()).unwrap_or(&[]);
        let mut seq = 0;
        let mut next = || {
            seq += 1;
            if prefix.is_empty() {
                seq.to_string()
            } else {
                format!("{prefix}.{seq}")
            }
        };
        let level = prefix.matches('.').count() + usize::from(!prefix.is_empty());
        // the parse of the command line is the entry's, not a step of the route
        let entrys = if self.parses { entry_steps(sites) } else { HashSet::new() };
        let shown_step = |st: &Step| !st.site.map_or(false, |i| entrys.contains(&i));
        for item in &b.items {
            match item {
                Item::Step(st) if !shown_step(st) => {}
                Item::Step(st) => self.step(st, st.site.and_then(|i| sites.get(i)), &next(), "", level, depth, shown, out, path),
                Item::Choice { members, when, .. } => {
                    let members: Vec<_> = members.iter().filter(|m| shown_step(&m.step)).collect();
                    if members.is_empty() {
                        continue;
                    }
                    let k = next();
                    for m in members {
                        self.step(
                            &m.step,
                            m.step.site.and_then(|i| sites.get(i)),
                            &format!("{k}{}", letter(m.branch)),
                            when.get(m.branch).map_or("", String::as_str),
                            level,
                            depth,
                            shown,
                            out,
                            path,
                        );
                    }
                }
            }
        }
        for (i, o) in b.alts.iter().enumerate() {
            let st = Step { to: o.clone(), on: String::new(), site: None };
            self.step(&st, None, &format!("{prefix}{}", letter(i)), "", level, depth, shown, out, path);
        }
    }

    fn step(
        &self,
        st: &Step,
        site: Option<&Site>,
        num: &str,
        when: &str,
        level: usize,
        depth: usize,
        shown: &mut HashMap<String, String>,
        out: &mut String,
        path: &[String],
    ) {
        let indent = "  ".repeat(level);
        let Some(n) = self.nodes.get(st.to.as_str()) else {
            // a library's operation: what the call hands it and what it gives, by name
            let names = |hs: &[Held]| {
                hs.iter().map(|h| if h.name.starts_with('#') { "·".to_string() } else { h.name.clone() }).collect::<Vec<_>>().join(", ")
            };
            let (hands, gives) = site.map(|s| (names(&s.hands), names(&s.gives))).unwrap_or_default();
            let op = st.to.strip_prefix("op:").unwrap_or(&st.to);
            out.push_str(&format!(
                "{indent}{num}  {op}({hands}){}{}\n",
                if gives.is_empty() { String::new() } else { format!(" → {gives}") },
                match when {
                    "" => String::new(),
                    "otherwise" => "   ·   otherwise".to_string(),
                    w => format!("   ·   when {w}"),
                }
            ));
            return;
        };
        let k = flow::key(&st.to, &st.on);
        let body = self.bodies.get(&k);
        let when = match when {
            "" => String::new(),
            "otherwise" => "   ·   otherwise".to_string(),
            w => format!("   ·   when {w}"),
        };
        let inner: Vec<String> = path.iter().cloned().chain([st.to.clone()]).collect();
        let seen = self.seen(&k, &st.to, &inner);
        if let Some(at) = shown.get(&seen).filter(|_| body.is_some()) {
            out.push_str(&format!("{indent}{num}  {}  → see {at}{when}\n", self.name(&st.to, &st.on)));
            return;
        }
        shown.insert(seen, num.to_string());
        let mut line = format!("{indent}{num}  {}{}", self.name(&st.to, &st.on), self.signature(n));
        let effects = self.effects(n, Some(&inner));
        if !effects.is_empty() {
            line.push_str(&format!("   ·   {effects}"));
        }
        let alts = body.map_or(0, |b| b.alts.len());
        if alts > 0 {
            line.push_str(&format!("   ·   one of {alts}"));
        }
        line.push_str(&when);
        out.push_str(&line);
        out.push('\n');
        if level + 1 < depth {
            self.walk(&k, num, depth, shown, out, &inner);
        }
    }

    /// A body is shown once for each set of callers on the way to it that make a path it touches
    /// whole: under another caller it touches other places, and is another step.
    fn seen(&self, k: &str, id: &str, path: &[String]) -> String {
        let mut makers = self.makers.borrow_mut();
        let made = makers.entry(id.to_string()).or_insert_with(|| {
            // what the function can run, as `Graph::reach` follows it, from the edges this story keeps
            let runs = |e: &Edge| {
                matches!(e.kind, EdgeKind::Calls | EdgeKind::Instantiates | EdgeKind::Requests)
                    || (e.kind == EdgeKind::Contains && self.nodes.get(e.src.as_str()).map_or(false, |n| n.kind == NodeKind::Class))
            };
            let (mut seen, mut stack) = (HashSet::from([id]), vec![id]);
            while let Some(at) = stack.pop() {
                for e in self.out.get(at).into_iter().flatten().copied().filter(|e| runs(e)) {
                    if seen.insert(e.dst.as_str()) {
                        stack.push(e.dst.as_str());
                    }
                }
            }
            seen.iter()
                .filter_map(|f| self.nodes.get(f))
                .flat_map(|n| n.touches.iter().flat_map(|t| t.by.iter().flat_map(|f| f.split('&').map(str::to_string))))
                .collect()
        });
        let mut on: Vec<&str> = path.iter().map(String::as_str).filter(|f| made.contains(*f)).collect();
        on.sort_unstable();
        on.dedup();
        format!("{k}|{}", on.join(","))
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
        let changes: Vec<String> =
            self.edges(&n.id, EdgeKind::Mutates).filter_map(|e| self.nodes.get(e.dst.as_str()).map(|t| t.name.clone())).collect();
        let mut out = format!("({})", names.join(", "));
        if !ret.is_empty() {
            out.push_str(&format!(" → {ret}"));
        }
        if !changes.is_empty() {
            out.push_str(&format!("  ↻ {}", changes.join(", ")));
        }
        out
    }

    /// What a step's own body does at the edges: `reads Makefile · writes *.png · prints`. A place a
    /// caller makes whole is touched where the route reaches that caller, and under a step (`path`,
    /// the functions on the calls that lead to it) where that caller is on the way.
    fn effects(&self, n: &Node, path: Option<&[String]>) -> String {
        let mut groups: Vec<(String, Vec<String>)> = vec![];
        let on_way = |f: &str| path.map_or_else(|| self.reached.contains(f), |p| p.iter().any(|x| x == f));
        for t in n.touches_in(&self.entry, &on_way) {
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
        groups
            .into_iter()
            .map(|(v, items)| if items.is_empty() { v } else { format!("{v} {}", items.join(" | ")) })
            .collect::<Vec<_>>()
            .join(" · ")
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
