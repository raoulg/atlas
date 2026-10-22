//! What a body does, read the same way in every language: its steps in the order they run, the
//! edges they make, and what its calls touch. A reader says what each call is and how its
//! language writes a string and a choice; the rest is read here.

use std::collections::{HashMap, HashSet};

use tree_sitter::Node as TsNode;

use crate::builder::GraphBuilder;
use crate::effects::{self, Facts};
use crate::model::{Crossing, Dir, EdgeKind, Held, Site, Touch, World};
use crate::tree::{
    arms_of, guards, named_children, number_choices, payload_of, starts_at_literal, text, walk, walk_all, what_of, ChoiceSyntax, Part,
};

/// A call that crosses into the world, as the code writes it.
pub struct Crosses {
    /// What the effect tables say, refined by the literals the call is handed.
    pub facts: Facts,
    /// What it touches as the code names it: the literal it is handed, else the expression.
    pub what: String,
    /// What flows through it, as the author named it.
    pub payload: String,
    /// The code alone decides where: the path or URL starts at a literal.
    pub fixed: bool,
    pub op: String,
    pub line: u32,
}

impl Crosses {
    /// Read a call that crosses `kind` through operation `op` of `lib`: the facts its literals
    /// refine, what it touches, what flows (by the types of the names `held`), and whether its
    /// place starts at a literal. `strings` are the kinds of node a string is in the language.
    pub fn read(
        c: TsNode,
        src: &[u8],
        kind: Crossing,
        lib: Option<&str>,
        op: &str,
        literals: &[String],
        strings: &[&str],
        held: &HashMap<String, String>,
    ) -> Crosses {
        let facts = effects::facts(kind, lib, op, literals);
        let what = what_of(c, src, strings, "arguments");
        let payload = payload_of(c, src, &what, "arguments", held);
        let fixed = facts.kind.located() && starts_at_literal(c, src, strings, "arguments", effects::on_receiver(op));
        Crosses { facts, what, payload, fixed, op: op.to_string(), line: c.start_position().row as u32 + 1 }
    }

    /// The touches it makes, one per place it names; where the call leaves the direction open,
    /// the one the body `said` to its driver.
    pub fn touches(&self, said: Option<Dir>) -> Vec<Touch> {
        self.facts
            .places()
            .into_iter()
            .map(|(artifact, dir)| {
                // a URL without a host is the page's own origin, decided by whoever serves it
                let fixed = self.fixed && (self.facts.kind != World::Net || !artifact.is_empty());
                Touch {
                    what: self.what.clone(),
                    payload: self.payload.clone(),
                    op: self.op.clone(),
                    artifact,
                    fixed,
                    line: self.line,
                    ..Touch::new(self.facts.kind, dir.or(said))
                }
            })
            .collect()
    }
}

/// The ways a file call goes with the first two paths it is handed: one that moves or copies
/// reads the first and writes the second (`fs::copy(a, b)`, `shutil.move(src, dst)`,
/// `copyFile(a, b)`); any other goes its own way with each.
pub fn sides(op: &str, paths: usize) -> Option<[Crossing; 2]> {
    (paths > 1 && matches!(effects::fs_op(op), effects::FsOp::Moves | effects::FsOp::Copies))
        .then_some([Crossing::read(World::Disk), Crossing::write(World::Disk)])
}

/// How a language writes where a value goes once an expression gives it: the names it binds, the
/// ways out of a function, the ways to an error, and what a call is handed.
pub struct UseSyntax {
    /// The names a node binds and the value it binds them to: `x = v`, `let x = v`, `for x in v`.
    pub binds: fn(TsNode) -> Option<(TsNode, TsNode)>,
    /// Does what this node holds leave the function `body`: a `return`, a `yield`, the last
    /// expression of a block that gives its value.
    pub leaves: fn(TsNode, TsNode) -> bool,
    /// Does what this node holds go to an error: `raise`, `throw`, `assert`, `bail!`.
    pub fails: fn(TsNode, &[u8]) -> bool,
    /// Is `child` what the call `node` is handed: its arguments.
    pub hands: fn(TsNode, TsNode) -> bool,
    /// Is `child` what `node` takes apart into the names its arms bind: a `match`'s subject.
    /// Where those names go is not followed, so the value is taken as going on.
    pub splits: fn(TsNode, TsNode) -> bool,
    /// The kinds of node a name is.
    pub names: &'static [&'static str],
    /// The kinds of target that keep a value beyond a name: a field, an element (`self.n = v`,
    /// `rows[i] = v`).
    pub keeps: &'static [&'static str],
}

/// One call as a step of the route through its body: what it runs, on which class and how the
/// call finds it (`Site::named`, `Site::after`), what it is handed and what it gives back, and the
/// options of the command line it parses when it parses the one the program is run with.
#[derive(Default)]
pub struct Step {
    pub on: String,
    pub on_receiver: bool,
    pub named: bool,
    pub after: String,
    pub hands: Vec<Held>,
    pub gives: Vec<Held>,
    pub command_line: Option<Vec<String>>,
}

/// What a call gives: the values it returns, and the named values it changes in place
/// (`rows.append(row)` gives `rows`).
fn changed_values(mut gives: Vec<Held>, changed: Vec<Held>) -> Vec<Held> {
    for value in changed.into_iter().filter(|h| !h.name.is_empty()) {
        if !gives.contains(&value) {
            gives.push(value);
        }
    }
    gives
}

/// The steps of a body in the order they run, the names they keep, and the edges they make: read
/// the same way for every language. The reader says what each call is; where it sits among the
/// body's choices is read here.
pub struct Steps<'t, 's> {
    body: TsNode<'t>,
    src: &'s [u8],
    syntax: &'s ChoiceSyntax,
    uses: &'s UseSyntax,
    choices: HashMap<usize, u32>,
    sites: Vec<Site>,
    edges: Vec<(String, EdgeKind, Option<String>, Vec<(u32, u32)>)>,
    unseen: Vec<(u32, String)>,
    /// The names the steps so far have kept.
    pub kept: HashSet<String>,
}

impl<'t, 's> Steps<'t, 's> {
    pub fn new(body: TsNode<'t>, src: &'s [u8], syntax: &'s ChoiceSyntax, uses: &'s UseSyntax) -> Self {
        let choices = number_choices(body, src, syntax);
        Steps { body, src, syntax, uses, choices, sites: vec![], edges: vec![], unseen: vec![], kept: HashSet::new() }
    }

    /// A call `c`, `name`d as the code writes it, of which the graph holds no more than what it
    /// gives: a library's that crosses into no place, changes no value and ends nothing as the
    /// tables say, or one Atlas cannot follow. Where what it gives goes nowhere the function hands
    /// on, the call is made for an effect Atlas does not see.
    pub fn dropped(&mut self, c: TsNode, name: &str) {
        if !self.goes_on(c, &mut HashSet::new()) {
            self.unseen(c, &format!("{name}(..)"));
        }
    }

    /// What the body does at `n` that the graph holds nothing else of, as the code writes it.
    pub fn unseen(&mut self, n: TsNode, written: &str) {
        let one = worded(written);
        if !self.unseen.iter().any(|(l, w)| *l == line(n) && *w == one) {
            self.unseen.push((line(n), one));
        }
    }

    /// Mark the touches call `c` made as checks when what it gives goes to nothing the function
    /// hands on.
    pub fn mark<'a>(&self, c: TsNode, touches: impl IntoIterator<Item = &'a mut Touch>) {
        let checks = !self.goes_on(c, &mut HashSet::new());
        for t in touches {
            t.checks = checks;
        }
    }

    /// Whether what `v` gives reaches what the function hands on: it is returned, handed to a
    /// call, kept in a field or an element, or taken apart by a `match`, or a name it is bound to is. What only decides a
    /// choice, goes to an error or is dropped goes no further. `bound` are the names followed.
    fn goes_on(&self, v: TsNode, bound: &mut HashSet<String>) -> bool {
        let (u, mut child, mut handed) = (self.uses, v, false);
        while let Some(p) = child.parent() {
            if (u.leaves)(child, self.body) {
                return true;
            }
            if (self.syntax.part)(p, child, self.src) == Part::Decides || (u.fails)(p, self.src) {
                return false;
            }
            if (u.splits)(p, child) {
                return true;
            }
            handed |= (u.hands)(p, child);
            if let Some((targets, value)) = (u.binds)(p).filter(|(_, value)| value.id() == child.id()) {
                if walk(targets, u.keeps).first().is_some() {
                    return true;
                }
                let after = value.end_byte();
                let names: Vec<String> =
                    walk(targets, u.names).into_iter().map(|n| text(n, self.src).to_string()).filter(|n| bound.insert(n.clone())).collect();
                return names.into_iter().any(|name| {
                    walk(self.body, u.names)
                        .into_iter()
                        .filter(|n| n.start_byte() >= after && text(*n, self.src) == name && !self.rebinds(*n))
                        .any(|n| self.goes_on(n, bound))
                });
            }
            if p.id() == self.body.id() {
                break;
            }
            child = p;
        }
        handed
    }

    /// Whether the name `n` is what a binding binds, not a value it reads.
    fn rebinds(&self, n: TsNode) -> bool {
        let mut child = n;
        while let Some(p) = child.parent().filter(|p| p.id() != self.body.id()) {
            if let Some((targets, _)) = (self.uses.binds)(p) {
                return targets.id() == child.id();
            }
            child = p;
        }
        false
    }

    /// The choices of the body, numbered.
    pub fn choices(&self) -> &HashMap<usize, u32> {
        &self.choices
    }

    /// The arms of the choices `n` sits in.
    pub fn arms(&self, n: TsNode) -> Vec<(u32, u32)> {
        arms_of(n, self.body, self.src, &self.choices, self.syntax)
    }

    /// Keep the names a call's value is held by.
    pub fn keep(&mut self, gives: &[Held]) {
        self.kept.extend(gives.iter().map(|g| g.name.clone()).filter(|n| !n.is_empty()));
    }

    /// A call `c` of the program's own code: a step to `to`, and an edge of `kind` (`via` the
    /// name it was called by, when no such function is known).
    pub fn call(&mut self, c: TsNode, to: &str, kind: EdgeKind, via: Option<String>, step: Step) {
        let arms = self.arms(c);
        self.keep(&step.gives);
        let site = Site {
            line: line(c),
            to: to.to_string(),
            on: step.on,
            on_receiver: step.on_receiver,
            named: step.named,
            after: step.after,
            hands: step.hands,
            gives: step.gives,
            arms: arms.clone(),
            span: span(c),
            command_line: step.command_line,
            ..Default::default()
        };
        self.sites.push(site);
        self.edges.push((to.to_string(), kind, via, arms));
    }

    /// A function or class `to`, handed on as a value at `a` inside call `c`: run by whoever
    /// receives it.
    pub fn handed(&mut self, c: TsNode, a: TsNode, to: &str, kind: EdgeKind) {
        let arms = self.arms(a);
        self.sites.push(Site { line: line(c), to: to.to_string(), arms: arms.clone(), ..Default::default() });
        self.edges.push((to.to_string(), kind, None, arms));
    }

    /// A library's operation `op` that the body reads no further: a step when it makes a value the
    /// body keeps, whatever it is handed.
    pub fn op(&mut self, c: TsNode, op: &str, hands: Vec<Held>, gives: Vec<Held>) {
        if gives.is_empty() {
            return;
        }
        self.unread(c, op, hands, gives, false);
    }

    /// A call `op` the body reads no further that changes the values `changed` it is called on
    /// (`rows.append(row)`, `random.shuffle(xs)`): a step that gives them anew from what it is
    /// handed, `T × A → T`, beside what it returns.
    pub fn changes(&mut self, c: TsNode, op: &str, hands: Vec<Held>, gives: Vec<Held>, changed: Vec<Held>) {
        let gives = changed_values(gives, changed);
        if gives.is_empty() {
            return;
        }
        self.unread(c, op, hands, gives, false);
        if let Some(s) = self.sites.last_mut() {
            s.changes = true;
        }
    }

    /// A library's call `op` that parses the command line the program is run with into values,
    /// with the `options` it declares: a step whatever it gives, since what it gives is the
    /// entry's.
    pub fn parses(&mut self, c: TsNode, op: &str, hands: Vec<Held>, gives: Vec<Held>, options: Vec<String>) {
        self.unread(c, op, hands, gives, false);
        if let Some(s) = self.sites.last_mut() {
            s.command_line = Some(options);
        }
    }

    /// A call Atlas cannot follow, `name`d as the code writes it: it runs all the same, so it is
    /// a step of the body, whatever it is handed and gives.
    pub fn not_followed(&mut self, c: TsNode, name: &str, hands: Vec<Held>, gives: Vec<Held>) {
        self.unread(c, name, hands, gives, true);
    }

    /// A call the body reads no further, `name`d as the code writes it: a library's operation,
    /// or one Atlas cannot follow. A line break and the indentation after it are the layout of the
    /// source, no part of the name: `text\n    .split` is `text.split`.
    fn unread(&mut self, c: TsNode, name: &str, hands: Vec<Held>, gives: Vec<Held>, not_followed: bool) {
        self.keep(&gives);
        let op = name.lines().map(str::trim).collect::<String>();
        self.sites.push(Site { line: line(c), op, not_followed, hands, gives, arms: self.arms(c), span: span(c), ..Default::default() });
    }

    /// An edge of `kind` to `to` at call `c`, with no step of its own.
    pub fn edge(&mut self, c: TsNode, to: &str, kind: EdgeKind) {
        self.edges.push((to.to_string(), kind, None, self.arms(c)));
    }

    /// Write the body's edges and its steps to the graph.
    pub fn finish(self, store: &mut GraphBuilder, id: &str) {
        let (arm_lines, loop_lines, whens) = (self.arm_lines(), self.loop_lines(), self.whens());
        for (t, kind, via, arms) in self.edges {
            store.add_call(id, &t, kind, via, arms);
        }
        if let Some(n) = store.nodes.get_mut(id) {
            n.sites = self.sites;
            (n.arm_lines, n.arm_choices) = arm_lines.into_iter().unzip();
            n.loop_lines = loop_lines;
            n.whens = whens;
            n.unseen = self.unseen;
        }
    }

    /// What decides each arm of the body's choices, as the code writes it: `(choice, arm, when)`.
    /// An arm that decides itself (a `case`, a `match` arm) runs when its pattern matches; the
    /// first arm of a choice decided in one place (an `if`, an `elif`, a conditional expression)
    /// when that holds, and a later one otherwise. Of two arms nothing decides (a `try` and its
    /// handler) the first is tried and the second runs otherwise, and the one arm of a choice
    /// nothing decides (what an or-else falls back on) runs otherwise; the first, and the arms of a
    /// longer choice nothing decides, have no words.
    fn whens(&self) -> Vec<(u32, u32, String)> {
        let decided = |n: TsNode| -> String {
            let parts: Vec<&str> = named_children(n)
                .into_iter()
                .filter(|c| (self.syntax.part)(n, *c, self.src) == Part::Decides)
                .map(|c| crate::tree::text(c, self.src))
                .collect();
            worded(&parts.join(" if "))
        };
        let mut out = vec![];
        for p in walk_all(self.body).into_iter().filter(|p| self.choices.contains_key(&p.id())) {
            let k = self.choices[&p.id()];
            let arms: Vec<(TsNode, u32)> = named_children(p)
                .into_iter()
                .filter_map(|c| match (self.syntax.part)(p, c, self.src) {
                    Part::Arm(a) => Some((c, a)),
                    _ => None,
                })
                .collect();
            let first = arms.iter().map(|(_, a)| *a).min();
            let n_arms = arms.len();
            let own = decided(p);
            for (c, a) in arms {
                let itself = if self.choices.contains_key(&c.id()) { decided(c) } else { String::new() };
                let when = match () {
                    _ if !itself.is_empty() => itself,
                    _ if Some(a) == first && !(own.is_empty() && n_arms == 1) => own.clone(),
                    _ if n_arms == 1 => "otherwise".to_string(),
                    _ if !own.is_empty() || n_arms == 2 => "otherwise".to_string(),
                    _ => String::new(),
                };
                if !when.is_empty() {
                    out.push((k, a, when));
                }
            }
        }
        // what follows a guard runs when none of its arms does
        for (k, a, _, _) in guards(self.body, self.src, &self.choices, self.syntax) {
            out.push((k, a, "otherwise".to_string()));
        }
        out
    }

    /// The lines each loop of the body spans: a statement that binds a name to each element of a
    /// value and runs a body for it (`for x in v`).
    fn loop_lines(&self) -> Vec<(u32, u32)> {
        walk_all(self.body)
            .into_iter()
            .filter(|n| (self.uses.binds)(*n).is_some() && n.child_by_field_name("body").is_some())
            .map(|n| (line(n), n.end_position().row as u32 + 1))
            .collect()
    }

    /// The lines each arm of the body's choices spans, with the choice it is an arm of.
    fn arm_lines(&self) -> Vec<((u32, u32), u32)> {
        let mut out = vec![];
        for p in walk_all(self.body).into_iter().filter(|p| self.choices.contains_key(&p.id())) {
            for c in named_children(p) {
                if let Part::Arm(_) = (self.syntax.part)(p, c, self.src) {
                    out.push(((line(c), c.end_position().row as u32 + 1), self.choices[&p.id()]));
                }
            }
        }
        for (k, _, first, last) in guards(self.body, self.src, &self.choices, self.syntax) {
            out.push(((line(first), last.end_position().row as u32 + 1), k));
        }
        out
    }
}

/// How a language binds names in a body, as far as a change made through a name is followed to
/// the value it changes.
pub struct BindSyntax {
    /// A statement that binds one name to a whole value: the name, and the names whose value it may
    /// be or be a part of (`x = graph`, `x = self.items[0]`, `x = a or b`); none for a value made
    /// anew (`x = copy(v)`, `x = xs[:]`). An augmented assignment (`x += v`) binds nothing new.
    pub binds: for<'a> fn(TsNode, &'a [u8]) -> Option<(&'a str, Vec<&'a str>)>,
    /// A loop that binds one name to each element of a value: the name, the name that value is a
    /// part of (`for x in graph.nodes`), and the loop's body.
    pub loops: for<'t, 'a> fn(TsNode<'t>, &'a [u8]) -> Option<(&'a str, Option<&'a str>, TsNode<'t>)>,
}

/// What a change made through `name` at byte `at` of `body` changes: the name whose value that
/// is when the change runs, or none when it is a value the body made. A loop binds the name to an
/// element of what it goes over, so inside the loop a change through it changes that value (`for
/// node in graph.nodes: node.status = ..` changes `graph`). A statement of the body's own, which
/// runs whenever the body does, decides what the name holds after it: another name or a part of
/// one makes it that value (`g = graph`), a value that may be what the name held keeps that, and
/// a new value makes a change after it one to the new value, not to what the name held before
/// (`graph = copy.copy(graph)`). A binding in a branch or a loop may not have run, so the name
/// holds what it held before it: the change may be one to that.
pub fn changed_through<'a>(body: TsNode, src: &'a [u8], syn: &BindSyntax, name: &'a str, at: usize) -> Option<&'a str> {
    let nodes = walk_all(body);
    let binds: Vec<(TsNode, &str, Vec<&str>)> = nodes.iter().filter_map(|n| (syn.binds)(*n, src).map(|(x, v)| (*n, x, v))).collect();
    let loops: Vec<(TsNode, &str, Option<&str>, TsNode)> =
        nodes.iter().filter_map(|n| (syn.loops)(*n, src).map(|(x, v, b)| (*n, x, v, b))).collect();
    let own = |s: TsNode| s.parent().into_iter().chain(s.parent().and_then(|p| p.parent())).any(|p| p.id() == body.id());
    let (mut name, mut at) = (name, at);
    // each step goes back to where a loop or a statement before `at` binds the name, so it ends
    'follow: loop {
        // the innermost loop around the change that binds the name, where its body binds it nowhere anew
        let around = loops
            .iter()
            .filter(|(_, x, _, b)| {
                *x == name
                    && b.byte_range().contains(&at)
                    && !binds.iter().any(|(s, y, _)| *y == name && b.byte_range().contains(&s.start_byte()))
            })
            .max_by_key(|(l, ..)| l.start_byte());
        if let Some((l, _, over, _)) = around {
            name = (*over)?;
            at = l.start_byte();
            continue;
        }
        let mut before: Vec<&(TsNode, &str, Vec<&str>)> =
            binds.iter().filter(|(s, x, _)| *x == name && own(*s) && s.end_byte() <= at).collect();
        before.sort_by_key(|(s, ..)| s.start_byte());
        for (s, _, held) in before.into_iter().rev() {
            if held.contains(&name) {
                continue;
            }
            name = *held.first()?;
            at = s.start_byte();
            continue 'follow;
        }
        return Some(name);
    }
}

/// What the names of a body hold, binding by binding: each binding is visible over a stretch of the
/// source, and a use takes the innermost binding visible where it stands. A name bound twice (a
/// loop's `b`, then `let b = Body::new()`) is two values, as two names would be. Where a binding is
/// visible is the language's to say.
pub struct Bindings<T> {
    all: Vec<(String, usize, usize, T)>,
}

impl<T> Default for Bindings<T> {
    fn default() -> Self {
        Bindings { all: vec![] }
    }
}

impl<T> Bindings<T> {
    /// A binding of `name` to `value`, visible over the bytes `from..to`.
    pub fn bind(&mut self, name: &str, from: usize, to: usize, value: T) {
        self.all.push((name.to_string(), from, to, value));
    }

    /// The index of the binding of `name` visible at `at`: of those that hold it, the one visible
    /// from the latest byte, which is the innermost.
    fn at(&self, name: &str, at: usize) -> Option<usize> {
        (0..self.all.len())
            .filter(|&i| self.all[i].0 == name && self.all[i].1 <= at && at < self.all[i].2)
            .max_by_key(|&i| (self.all[i].1, i))
    }

    /// Where the binding of `name` visible at `at` is visible from: which binding it is.
    pub fn which(&self, name: &str, at: usize) -> Option<usize> {
        self.at(name, at).map(|i| self.all[i].1)
    }

    /// What the binding of `name` visible at `at` holds.
    pub fn get(&self, name: &str, at: usize) -> Option<&T> {
        self.at(name, at).map(|i| &self.all[i].3)
    }

    /// The binding of `name` visible at `at`: the byte it is visible from, and what it holds.
    pub fn binding(&self, name: &str, at: usize) -> Option<(usize, &T)> {
        self.at(name, at).map(|i| (self.all[i].1, &self.all[i].3))
    }

    /// Whether `name` holds a value at `at`.
    pub fn holds(&self, name: &str, at: usize) -> bool {
        self.at(name, at).is_some()
    }

    /// What the binding of `name` visible at `at` holds, to change it.
    pub fn get_mut(&mut self, name: &str, at: usize) -> Option<&mut T> {
        self.at(name, at).map(|i| &mut self.all[i].3)
    }

    /// Every binding, by its name.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &T)> {
        self.all.iter().map(|(n, _, _, v)| (n.as_str(), v))
    }

    /// Every binding, by its name and the byte it is visible from.
    pub fn all(&self) -> impl Iterator<Item = (&str, usize, &T)> {
        self.all.iter().map(|(n, from, _, v)| (n.as_str(), *from, v))
    }

    /// Every name visible at `at`, with what it holds there.
    pub fn visible(&self, at: usize) -> HashMap<String, T>
    where
        T: Clone,
    {
        let mut out = HashMap::new();
        for (name, ..) in &self.all {
            if let Some(v) = self.get(name, at) {
                out.entry(name.clone()).or_insert_with(|| v.clone());
            }
        }
        out
    }
}

fn line(c: TsNode) -> u32 {
    c.start_position().row as u32 + 1
}

/// Code as words on a picture: one line, at most 60 characters, without the parentheses a
/// language puts around a condition (`if (!existsSync(raw))`).
fn worded(code: &str) -> String {
    let mut one: String = code.split_whitespace().collect::<Vec<_>>().join(" ");
    while one.starts_with('(') && one.ends_with(')') && closes_at_end(&one) {
        one = one[1..one.len() - 1].trim().to_string();
    }
    if one.chars().count() <= 60 {
        one
    } else {
        one.chars().take(59).collect::<String>() + "…"
    }
}

fn span(c: TsNode) -> (u32, u32) {
    (c.start_byte() as u32, c.end_byte() as u32)
}

/// Whether the parenthesis that opens `s` is the one that closes it.
fn closes_at_end(s: &str) -> bool {
    let mut depth = 0;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            return i == s.len() - 1;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `fs::copy(a, b)`, `shutil.move(src, dst)`, `os.replace(a, b)` and `copyFile(a, b)` all read
    /// their first path and write their second.
    #[test]
    fn a_move_or_a_copy_reads_its_first_path_and_writes_its_second() {
        let both = Some([Crossing::read(World::Disk), Crossing::write(World::Disk)]);
        for op in ["copy", "copyfile", "copyFile", "cpSync", "rename", "renameSync", "move", "replace"] {
            assert_eq!(sides(op, 2), both, "{op}");
        }
        assert_eq!(sides("copy", 1), None, "one path is the operation's own way");
        assert_eq!(sides("write_text", 2), None);
    }

    /// `for b in bases { resolve(b) }` then `let b = Body::new()`: the use in the loop takes the
    /// loop's `b`, a use after it the later binding, and a use before both neither.
    #[test]
    fn a_use_takes_the_innermost_binding_visible_where_it_stands() {
        let mut b = Bindings::default();
        b.bind("b", 10, 40, "String");
        b.bind("b", 50, 100, "Body");
        b.bind("b", 60, 70, "Leaf");
        assert_eq!(b.get("b", 5), None);
        assert_eq!(b.get("b", 20), Some(&"String"));
        assert_eq!(b.get("b", 45), None);
        assert_eq!(b.get("b", 55), Some(&"Body"));
        assert_eq!(b.get("b", 65), Some(&"Leaf"));
        assert_eq!(b.get("b", 80), Some(&"Body"));
        assert_eq!(b.visible(65).get("b"), Some(&"Leaf"));
    }
}
