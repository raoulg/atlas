//! What a body does, read the same way in every language: its steps in the order they run, the
//! edges they make, and what its calls touch. A reader says what each call is and how its
//! language writes a string and a choice; the rest is read here.

use std::collections::{HashMap, HashSet};

use tree_sitter::Node as TsNode;

use crate::builder::GraphBuilder;
use crate::effects::{self, Facts};
use crate::model::{Crossing, Dir, EdgeKind, Held, Site, Touch, World};
use crate::tree::{
    arms_of, named_children, number_choices, payload_of, starts_at_literal, text, walk, walk_all, what_of, ChoiceSyntax, Part,
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

/// One call as a step of the route through its body: what it runs, on which class, what it is
/// handed and what it gives back, and the options of the command line it parses when it parses
/// the one the program is run with.
#[derive(Default)]
pub struct Step {
    pub on: String,
    pub on_receiver: bool,
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
    /// The names the steps so far have kept.
    pub kept: HashSet<String>,
}

impl<'t, 's> Steps<'t, 's> {
    pub fn new(body: TsNode<'t>, src: &'s [u8], syntax: &'s ChoiceSyntax, uses: &'s UseSyntax) -> Self {
        let choices = number_choices(body, src, syntax);
        Steps { body, src, syntax, uses, choices, sites: vec![], edges: vec![], kept: HashSet::new() }
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
    /// or one Atlas cannot follow.
    fn unread(&mut self, c: TsNode, name: &str, hands: Vec<Held>, gives: Vec<Held>, not_followed: bool) {
        self.keep(&gives);
        self.sites.push(Site {
            line: line(c),
            op: name.to_string(),
            not_followed,
            hands,
            gives,
            arms: self.arms(c),
            span: span(c),
            ..Default::default()
        });
    }

    /// An edge of `kind` to `to` at call `c`, with no step of its own.
    pub fn edge(&mut self, c: TsNode, to: &str, kind: EdgeKind) {
        self.edges.push((to.to_string(), kind, None, self.arms(c)));
    }

    /// Write the body's edges and its steps to the graph.
    pub fn finish(self, store: &mut GraphBuilder, id: &str) {
        let (arm_lines, whens) = (self.arm_lines(), self.whens());
        for (t, kind, via, arms) in self.edges {
            store.add_call(id, &t, kind, via, arms);
        }
        if let Some(n) = store.nodes.get_mut(id) {
            n.sites = self.sites;
            n.arm_lines = arm_lines;
            n.whens = whens;
        }
    }

    /// What decides each arm of the body's choices, as the code writes it: `(choice, arm, when)`.
    /// An arm that decides itself (a `case`, a `match` arm) runs when its pattern matches; the
    /// first arm of a choice decided in one place (an `if`, an `elif`, a conditional expression)
    /// when that holds, and a later one otherwise. Of two arms nothing decides (an or-else, a
    /// `try` and its handler) the first is tried and the second runs otherwise; the first, and
    /// the arms of a longer choice nothing decides, have no words.
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
                    _ if Some(a) == first => own.clone(),
                    _ if !own.is_empty() || n_arms == 2 => "otherwise".to_string(),
                    _ => String::new(),
                };
                if !when.is_empty() {
                    out.push((k, a, when));
                }
            }
        }
        out
    }

    /// The lines each arm of the body's choices spans.
    fn arm_lines(&self) -> Vec<(u32, u32)> {
        let mut out = vec![];
        for p in walk_all(self.body).into_iter().filter(|p| self.choices.contains_key(&p.id())) {
            for c in named_children(p) {
                if let Part::Arm(_) = (self.syntax.part)(p, c, self.src) {
                    out.push((line(c), c.end_position().row as u32 + 1));
                }
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
}
