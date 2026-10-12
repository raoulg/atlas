//! What a body does, read the same way in every language. A reader says which call crosses into
//! the world and how its language writes a string; what the call then touches is read here.

use std::collections::HashMap;

use tree_sitter::Node as TsNode;

use crate::effects::{self, Facts};
use crate::model::{Crossing, Dir, Touch, World};
use crate::tree::{payload_of, starts_at_literal, what_of};

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
