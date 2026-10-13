//! What a body does, read the same way in every language: its steps in the order they run, the
//! edges they make, and what its calls touch. A reader says what each call is and how its
//! language writes a string and a choice; the rest is read here.

use std::collections::{HashMap, HashSet};

use tree_sitter::Node as TsNode;

use crate::builder::GraphBuilder;
use crate::effects::{self, Facts};
use crate::model::{Crossing, Dir, EdgeKind, Held, Site, Touch, World};
use crate::tree::{arms_of, number_choices, payload_of, starts_at_literal, what_of, ChoiceSyntax};

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
    pub fn read(c: TsNode, src: &[u8], kind: Crossing, lib: Option<&str>, op: &str, literals: &[String], strings: &[&str], held: &HashMap<String, String>) -> Crosses {
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
                Touch { what: self.what.clone(), payload: self.payload.clone(), op: self.op.clone(), artifact, fixed, line: self.line, ..Touch::new(self.facts.kind, dir.or(said)) }
            })
            .collect()
    }
}

/// The ways a file call goes with the first two paths it is handed: one that moves or copies
/// reads the first and writes the second (`fs::copy(a, b)`, `shutil.move(src, dst)`,
/// `copyFile(a, b)`); any other goes its own way with each.
pub fn sides(op: &str, paths: usize) -> Option<[Crossing; 2]> {
    (paths > 1 && matches!(effects::fs_op(op), effects::FsOp::Moves | effects::FsOp::Copies)).then_some([Crossing::read(World::Disk), Crossing::write(World::Disk)])
}

/// One call as a step of the route through its body: what it runs, on which class, what it is
/// handed and what it gives back.
#[derive(Default)]
pub struct Step {
    pub on: String,
    pub on_receiver: bool,
    pub hands: Vec<Held>,
    pub gives: Vec<Held>,
}

/// The steps of a body in the order they run, the names they keep, and the edges they make: read
/// the same way for every language. The reader says what each call is; where it sits among the
/// body's choices is read here.
pub struct Steps<'t, 's> {
    body: TsNode<'t>,
    src: &'s [u8],
    syntax: &'s ChoiceSyntax,
    choices: HashMap<usize, u32>,
    sites: Vec<Site>,
    edges: Vec<(String, EdgeKind, Option<String>, Vec<(u32, u32)>)>,
    /// The names the steps so far have kept.
    pub kept: HashSet<String>,
}

impl<'t, 's> Steps<'t, 's> {
    pub fn new(body: TsNode<'t>, src: &'s [u8], syntax: &'s ChoiceSyntax) -> Self {
        let choices = number_choices(body, src, syntax);
        Steps { body, src, syntax, choices, sites: vec![], edges: vec![], kept: HashSet::new() }
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
        self.sites.push(Site { line: line(c), to: to.to_string(), on: step.on, on_receiver: step.on_receiver, hands: step.hands, gives: step.gives, arms: arms.clone(), span: span(c), ..Default::default() });
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
        self.not_followed(c, op, hands, gives);
    }

    /// A call Atlas cannot follow, `name`d as the code writes it: it runs all the same, so it is
    /// a step of the body, whatever it is handed and gives.
    pub fn not_followed(&mut self, c: TsNode, name: &str, hands: Vec<Held>, gives: Vec<Held>) {
        self.keep(&gives);
        self.sites.push(Site { line: line(c), op: name.to_string(), hands, gives, arms: self.arms(c), span: span(c), ..Default::default() });
    }

    /// An edge of `kind` to `to` at call `c`, with no step of its own.
    pub fn edge(&mut self, c: TsNode, to: &str, kind: EdgeKind) {
        self.edges.push((to.to_string(), kind, None, self.arms(c)));
    }

    /// Write the body's edges and its steps to the graph.
    pub fn finish(self, store: &mut GraphBuilder, id: &str) {
        for (t, kind, via, arms) in self.edges {
            store.add_call(id, &t, kind, via, arms);
        }
        if let Some(n) = store.nodes.get_mut(id) {
            n.sites = self.sites;
        }
    }
}

fn line(c: TsNode) -> u32 {
    c.start_position().row as u32 + 1
}

fn span(c: TsNode) -> (u32, u32) {
    (c.start_byte() as u32, c.end_byte() as u32)
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
