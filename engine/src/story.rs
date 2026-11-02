//! A route as text: an entry and its steps in call order, each with its own signature and its
//! own effects. Alternatives share a number and are lettered by branch: the arms of a choice in
//! the body, each with what decides it (`when not raw.exists()`), or the overrides a call may run.
//! Past `z` the letters go on as a spreadsheet names its columns (`aa`, `ab`, …), and the overrides
//! of a step lettered already are set off from its letters by `-` (`3a-a`), so that no label names
//! two branches. A choice with one arm is lettered too: its steps run when it holds.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

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
/// levels deep: the same steps, from the same bodies, as the page draws, each touching the places
/// the route reads it to touch where it runs (`flow::Along`), under the calls that lead there and
/// after what ran before it. A step no call in its body's sites is (a door of a class, what a door
/// or the entry may run instead) runs with nothing above it and nothing before it, and a call of a
/// function the route is already inside is the step it stands in.
pub fn route(g: &Graph, entry: &str, depth: usize) -> Option<String> {
    told(g, entry, depth).map(|(text, _)| text)
}

/// A step of a route as `route` tells it: its label (the entry's is `the entry`), the occurrence it is, as
/// `flow::along` names it, the function it runs (a class's initializer where it builds the class), the places its own
/// body touches there (`Touch::place`), and, where the text sends the reader to the step of that label that runs the
/// same function (`→ see`) and tells no places of its own, that label. The steps of one branch share its label.
#[derive(Debug, Clone, PartialEq)]
pub struct Told {
    pub label: String,
    pub at: String,
    pub runs: String,
    pub places: BTreeSet<String>,
    pub see: Option<String>,
}

/// The steps `route` tells, in its order: the entry, then each step that names a function or a class.
pub fn told_steps(g: &Graph, entry: &str, depth: usize) -> Option<Vec<Told>> {
    told(g, entry, depth).map(|(_, steps)| steps)
}

/// The route as text, and the steps it tells.
fn told(g: &Graph, entry: &str, depth: usize) -> Option<(String, Vec<Told>)> {
    let mut s = Story::new(g);
    let start = s.find(entry)?;
    let depth = depth.max(1);
    s.entry = start.to_string();
    let n = s.nodes[start];
    s.parses = n.entry.as_ref().map_or(false, |e| matches!(e.kind, EntryKind::Cli | EntryKind::Script));
    let kind = n.entry.as_ref().map(|e| format!("{} · {}", kind_label(e.kind), usage(e))).unwrap_or_else(|| s.name(start, ""));
    let mut t = Telling {
        text: format!("{kind}\n{}{}", n.id, s.signature(n)),
        steps: vec![],
        shown: HashMap::new(),
        along: flow::Along::new(g, start),
    };
    let entry = "the entry";
    let (touches, reading) = t.at(start, start, "", depth);
    t.tell(entry, start, start, &touches, None);
    let effects = effects(&touches);
    if !effects.is_empty() {
        t.text.push_str(&format!("   ·   {effects}"));
    }
    t.text.push('\n');
    t.shown.extend(reading.map(|r| (r, entry.to_string())));
    let alts = Step::of(start).runs_instead(&s.bodies);
    let above = [Above { id: start.to_string(), label: entry.to_string() }];
    s.walk(start, start, "", depth, &mut t, &above, &alts);
    Some((t.text, t.steps))
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
    let (mut seen, mut calls, mut queue) = (HashSet::new(), HashSet::new(), std::collections::VecDeque::from([Step::of(start)]));
    let mut found = 0;
    while let Some(call) = queue.pop_front() {
        let k = flow::key(&call.to, &call.on);
        let Some(b) = s.bodies.get(&k).filter(|_| calls.insert((k.clone(), call.named))) else { continue };
        // a body's choices once, however many calls run it; what each call may run instead
        let items = if seen.insert(k) { b.items.as_slice() } else { &[] };
        let sites = s.nodes.get(call.to.as_str()).map(|n| n.sites.as_slice()).unwrap_or(&[]);
        let named = |st: &Step| {
            if s.nodes.contains_key(st.to.as_str()) {
                s.step_name(st)
            } else {
                st.to.trim_start_matches("op:").to_string()
            }
        };
        // a body's choices are listed once, whatever calls run it: a branch touches a place a caller makes whole where the
        // route reaches that caller
        let touched = |st: &Step| {
            s.nodes.get(st.to.as_str()).map(|n| effects(&n.touches_in(&s.entry, &|f| s.reached.contains(f)))).unwrap_or_default()
        };
        for item in items {
            let steps: Vec<&Step> = match item {
                Item::Step(st) => vec![st],
                Item::Choice { members, when, .. } => {
                    found += 1;
                    let line = members.iter().filter_map(|m| m.step.site.and_then(|i| sites.get(i))).map(|s| s.line).min().unwrap_or(0);
                    out.push_str(&format!("\n{} · line {line}\n", s.step_name(&call)));
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
            queue.extend(steps.into_iter().filter(|st| s.nodes.contains_key(st.to.as_str())).cloned());
        }
        queue.extend(call.runs_instead(&s.bodies));
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
    /// The initializer building each class runs (`flow::initializers`).
    inits: HashMap<String, String>,
    /// The route's entry, and what it can run.
    entry: String,
    reached: HashSet<String>,
    /// The route is run from the command line: its parses of the command line are the entry's.
    parses: bool,
}

/// A route as told so far: its text, the steps it told, the label each reading of a body was first shown under, and
/// the route's occurrences as the engine reads them, each part read when a step first asks for it.
struct Telling<'g> {
    text: String,
    steps: Vec<Told>,
    shown: HashMap<usize, String>,
    along: flow::Along<'g>,
}

impl<'g> Telling<'g> {
    /// What the occurrence named `at` of body `id` run on class `on` touches, and the reading of its body
    /// (`flow::Occurrence`): as the route reads it there, or, where no call in a body's sites leads there, as a part of
    /// the route with nothing above it and nothing before it, read `depth` calls deep (`flow::Along::read`).
    fn at(&mut self, at: &str, id: &str, on: &str, depth: usize) -> (Vec<&'g Touch>, Option<usize>) {
        if self.along.occurrence(at).is_none() {
            self.along.read(at, id, on, depth);
        }
        self.along.occurrence(at).map_or((vec![], None), |o| (o.touches.clone(), Some(o.reading)))
    }

    /// Tells the step labelled `label`, the occurrence `at` that runs `runs` (`Told`).
    fn tell(&mut self, label: &str, at: &str, runs: &str, touches: &[&Touch], see: Option<&str>) {
        let places = touches.iter().map(|t| t.place().id).collect();
        let (label, at, runs, see) = (label.to_string(), at.to_string(), runs.to_string(), see.map(str::to_string));
        self.steps.push(Told { label, at, runs, places, see });
    }
}

/// A step the route is inside, on the calls that lead to a step: the function its body is, and its label.
#[derive(Clone)]
struct Above {
    id: String,
    label: String,
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
            inits: flow::initializers(g),
            entry: String::new(),
            reached: Default::default(),
            parses: false,
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
    /// branches lettered, then `alts`, what the call into it may run instead. `at` is the body's
    /// occurrence in the route, as `flow::along` names it, and `above` the steps on the calls that
    /// lead into the body, the body's own last.
    #[allow(clippy::too_many_arguments)]
    fn walk(&self, body: &str, at: &str, prefix: &str, depth: usize, t: &mut Telling<'g>, above: &[Above], alts: &[Step]) {
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
        // a call is named by its place among the body's sites, a step no site is by what it runs
        let occurrence = |st: &Step| format!("{at}/{}", st.site.map_or_else(|| st.to.clone(), |i| i.to_string()));
        for item in &b.items {
            match item {
                Item::Step(st) if !shown_step(st) => {}
                Item::Step(st) => {
                    let site = st.site.and_then(|i| sites.get(i));
                    self.step(st, site, &occurrence(st), &next(), "", level, depth, t, above)
                }
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
                            &occurrence(&m.step),
                            &format!("{k}{}", letter(m.branch)),
                            when.get(m.branch).map_or("", String::as_str),
                            level,
                            depth,
                            t,
                            above,
                        );
                    }
                }
            }
        }
        // after a branch's letters an override's would read as one longer letter: `3a` and `a` as `3aa`
        let apart = if prefix.ends_with(|c: char| c.is_ascii_lowercase()) { "-" } else { "" };
        // what the call may run instead runs in place of the body, as the same call: the body is not on the calls to it
        let outside = &above[..above.len().saturating_sub(1)];
        for (i, o) in alts.iter().enumerate() {
            let instead = format!("{at}|{}", flow::key(&o.to, &o.on));
            self.step(o, None, &instead, &format!("{prefix}{apart}{}", letter(i)), "", level, depth, t, outside);
        }
    }

    /// The step `st`, the occurrence named `at` (`walk`), labelled `num`, and the steps it opens.
    #[allow(clippy::too_many_arguments)]
    fn step(
        &self,
        st: &Step,
        site: Option<&Site>,
        at: &str,
        num: &str,
        when: &str,
        level: usize,
        depth: usize,
        t: &mut Telling<'g>,
        above: &[Above],
    ) {
        let indent = "  ".repeat(level);
        let Some(n) = self.nodes.get(st.to.as_str()) else {
            // a library's operation: what the call hands it and what it gives, by name
            let names = |hs: &[Held]| {
                hs.iter().map(|h| if h.name.starts_with('#') { "·".to_string() } else { h.name.clone() }).collect::<Vec<_>>().join(", ")
            };
            let (hands, gives) = site.map(|s| (names(&s.hands), names(&s.gives))).unwrap_or_default();
            let op = st.to.strip_prefix("op:").unwrap_or(&st.to);
            t.text.push_str(&format!(
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
        // building a class runs its initializer along its order and none of the class's methods, whether or not the class
        // is a route of its own; a class built from its fields runs nothing
        let built = n.kind == NodeKind::Class;
        let runs = if built { self.inits.get(st.to.as_str()).map(String::as_str) } else { Some(st.to.as_str()) };
        let runner = runs.and_then(|r| self.nodes.get(r).copied()).unwrap_or(n);
        let k = flow::key(runs.unwrap_or(&st.to), &st.on);
        let body = runs.and_then(|_| self.bodies.get(&k));
        let when = match when {
            "" => String::new(),
            "otherwise" => "   ·   otherwise".to_string(),
            w => format!("   ·   when {w}"),
        };
        // a call of a function the route is already inside is the step it stands in
        if let Some(a) = runs.and_then(|r| above.iter().find(|a| a.id == r)) {
            t.tell(num, at, &runner.id, &[], Some(&a.label));
            t.text.push_str(&format!("{indent}{num}  {}  → see {}{when}\n", self.step_name(st), a.label));
            return;
        }
        // what it runs on: the class it builds, or the class of its value
        let on = if built { st.to.as_str() } else { st.on.as_str() };
        let (touches, reading) = runs.map_or_else(Default::default, |r| t.at(at, r, on, depth.saturating_sub(level + 1)));
        // a body is shown once for each reading of it: under other callers, or after another set-up, it is another step
        if let Some(seen) = reading.and_then(|r| t.shown.get(&r)).filter(|_| body.is_some()).cloned() {
            t.tell(num, at, &runner.id, &[], Some(&seen));
            t.text.push_str(&format!("{indent}{num}  {}  → see {seen}{when}\n", self.step_name(st)));
            return;
        }
        if let Some(r) = reading {
            t.shown.insert(r, num.to_string());
        }
        t.tell(num, at, &runner.id, &touches, None);
        let effects = effects(&touches);
        let mut line = format!("{indent}{num}  {}{}", self.step_name(st), self.signature(n));
        if !effects.is_empty() {
            line.push_str(&format!("   ·   {effects}"));
        }
        let alts = if built { &[][..] } else { st.alts(&self.bodies) };
        if !alts.is_empty() {
            line.push_str(&format!("   ·   one of {}", alts.len()));
        }
        line.push_str(&when);
        t.text.push_str(&line);
        t.text.push('\n');
        if level + 1 < depth && body.is_some() {
            let mut inner = above.to_vec();
            inner.push(Above { id: runner.id.clone(), label: num.to_string() });
            self.walk(&k, at, num, depth, t, &inner, &if built { vec![] } else { st.runs_instead(&self.bodies) });
        }
    }

    /// A step by `name`: a method its call names (`Cached.read(self)`) by its own class, whatever
    /// the class of the value it runs on.
    fn step_name(&self, st: &Step) -> String {
        self.name(&st.to, if st.named { "" } else { &st.on })
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
        let mut changes: Vec<String> = vec![];
        for t in self.edges(&n.id, EdgeKind::Mutates).filter_map(|e| self.nodes.get(e.dst.as_str()).map(|t| t.name.clone())) {
            if !changes.contains(&t) {
                changes.push(t);
            }
        }
        let mut out = format!("({})", names.join(", "));
        if !ret.is_empty() {
            out.push_str(&format!(" → {ret}"));
        }
        if !changes.is_empty() {
            out.push_str(&format!("  ↻ {}", changes.join(", ")));
        }
        out
    }
}

/// What a step's own body does at the edges, from the touches it runs: `reads Makefile · writes
/// *.png · prints`.
fn effects(touches: &[&Touch]) -> String {
    let mut groups: Vec<(String, Vec<String>)> = vec![];
    for t in touches {
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

/// The letters of the `i`th branch, from 0, as a spreadsheet names its columns: `a` to `z`, then
/// `aa` to `zz`, then `aaa`.
fn letter(i: usize) -> String {
    let (mut n, mut out) = (i + 1, vec![]);
    while n > 0 {
        n -= 1;
        out.push(b'a' + (n % 26) as u8);
        n /= 26;
    }
    out.iter().rev().map(|&b| b as char).collect()
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

#[cfg(test)]
mod tests {
    use super::letter;

    /// The next string of lowercase letters in shortlex order (the shorter first, then the
    /// alphabet's): `az` → `ba`, `zz` → `aaa`.
    fn successor(s: &str) -> String {
        let mut b = s.as_bytes().to_vec();
        for i in (0..b.len()).rev() {
            if b[i] < b'z' {
                b[i] += 1;
                return String::from_utf8(b).unwrap();
            }
            b[i] = b'a';
        }
        format!("a{}", String::from_utf8(b).unwrap())
    }

    /// Branches are named as a spreadsheet names its columns: from `a`, each the next string of
    /// letters in shortlex order, so that every branch has letters of its own however many there are.
    #[test]
    fn branch_letters_are_the_strings_of_letters_in_order() {
        assert_eq!(letter(0), "a");
        for i in 0..30_000 {
            let (this, next) = (letter(i), letter(i + 1));
            assert!(this.bytes().all(|b| b.is_ascii_lowercase()), "{i}: {this}");
            assert_eq!(next, successor(&this), "{i}: {this} is followed by {next}");
        }
    }
}
