//! A prophecy: sketch code kept in the tree, marked as not built yet.
//!
//! Each language marks it in its own form, and all three are one construct: the decorator
//! `@prophecy(supersedes="pkg.Old")` in Python, the attribute `#[doc = "prophecy: supersedes=pkg::Old"]`
//! (or `#[prophecy(supersedes = "pkg::Old")]` where a macro writes it) in Rust, the JSDoc tag
//! `/** @prophecy supersedes=pkg.Old */` in TypeScript. The readers find the marker; what it says is
//! read here, once.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::{Edge, EdgeKind, Entry, EntryKind, Graph, Node, NodeKind};

/// What a prophecy says of itself.
#[derive(Serialize, Deserialize, TS, Clone, Debug, Default, PartialEq, Eq)]
#[ts(export)]
pub struct Prophecy {
    /// The existing thing the prophecy replaces: it stays until the prophecy is built.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<String>,
    /// The place is known and the shape is not.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub veiled: bool,
    /// The shape waits on an open question, as an issue (`#144`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sealed: Option<String>,
    /// The node `supersedes` names, once the graph is whole: its id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replaces: Option<String>,
}

impl Prophecy {
    /// The arguments of a marker: `supersedes=pkg.Old veiled sealed="#144"`, or the same as a call
    /// would hand them, `(supersedes="pkg.Old", veiled=True)`. Names it does not know are ignored.
    pub fn parse(args: &str) -> Prophecy {
        let mut p = Prophecy::default();
        let args = args.trim().trim_start_matches('(').trim_end_matches(')');
        for token in tokens(args) {
            let (key, value) = token.split_once('=').map_or((token.as_str(), ""), |(k, v)| (k.trim(), v.trim()));
            let value = value.trim_matches(['"', '\'']).to_string();
            match key {
                "supersedes" if !value.is_empty() => p.supersedes = Some(value),
                "sealed" if !value.is_empty() => p.sealed = Some(value),
                "veiled" => p.veiled = !matches!(value.to_lowercase().as_str(), "false" | "0"),
                _ => {}
            }
        }
        p
    }

    /// The prophecy a decorator or an attribute marks, as the reader hands it over: `prophecy`,
    /// `prophecy(supersedes="x")`, `markers.prophecy(..)`, or the Rust attribute `doc = "prophecy: .."`.
    pub fn of_decorator(decorator: &str) -> Option<Prophecy> {
        let d = decorator.trim();
        if let Some(rest) = d.strip_prefix("doc").map(str::trim_start).and_then(|r| r.strip_prefix('=')) {
            let literal = rest.trim().trim_matches('"');
            return literal.trim_start().strip_prefix("prophecy:").map(Prophecy::parse);
        }
        let (name, args) = d.split_once('(').map_or((d, ""), |(n, a)| (n, a));
        (name.trim().rsplit(['.', ':']).next() == Some("prophecy")).then(|| Prophecy::parse(args))
    }

    /// The prophecy a comment's `@prophecy` tag marks: the rest of the tag's line is its arguments.
    pub fn of_jsdoc(comment: &str) -> Option<Prophecy> {
        comment.lines().find_map(|l| {
            let l = l.trim().trim_start_matches("/**").trim_start_matches('*').trim();
            let rest = l.strip_prefix("@prophecy")?;
            (rest.is_empty() || rest.starts_with(char::is_whitespace)).then(|| Prophecy::parse(rest.trim_end_matches("*/")))
        })
    }

    /// The first prophecy among a node's decorators or attributes.
    pub fn among(decorators: &[String]) -> Option<Prophecy> {
        decorators.iter().find_map(|d| Prophecy::of_decorator(d))
    }
}

/// What the graph, once whole, says of its prophecies: the node each `supersedes` names (by its id,
/// where `::` and `.` part a path alike, or by the one id that ends so), and a route for each
/// operation of a prophecy that no other prophecy calls, so that a sketch is read as a route is.
pub fn settle(nodes: &mut IndexMap<String, Node>, edges: &[Edge]) {
    let ids: Vec<String> = nodes.keys().cloned().collect();
    let replaces: Vec<(String, String)> = nodes
        .values()
        .filter_map(|p| {
            let name = p.prophecy.as_ref()?.supersedes.as_deref()?.replace("::", ".");
            let tail = format!(".{name}");
            let mut ends =
                ids.iter().filter(|id| **id != p.id && nodes.get(*id).is_some_and(|n| n.prophecy.is_none()) && id.ends_with(&tail));
            let found = ids.iter().find(|id| **id == name && **id != p.id).or_else(|| ends.next().filter(|_| ends.next().is_none()));
            Some((p.id.clone(), found?.clone()))
        })
        .collect();
    for (id, target) in replaces {
        if let Some(p) = nodes.get_mut(&id).and_then(|n| n.prophecy.as_mut()) {
            p.replaces = Some(target);
        }
    }
    let prophetic = |nodes: &IndexMap<String, Node>, id: &str| {
        nodes
            .get(id)
            .is_some_and(|n| n.prophecy.is_some() || n.cls.as_deref().and_then(|c| nodes.get(c)).is_some_and(|c| c.prophecy.is_some()))
    };
    let called_by_prophecy: std::collections::HashSet<&str> =
        edges.iter().filter(|e| e.kind == EdgeKind::Calls && prophetic(nodes, &e.src)).map(|e| e.dst.as_str()).collect();
    let routes: Vec<(String, String)> = nodes
        .values()
        .filter(|n| matches!(n.kind, NodeKind::Function | NodeKind::Method) && n.entry.is_none() && !n.library && prophetic(nodes, &n.id))
        .filter(|n| !called_by_prophecy.contains(n.id.as_str()))
        .map(|n| (n.id.clone(), n.cls.as_deref().and_then(|c| nodes.get(c)).map_or(n.name.clone(), |c| format!("{}.{}", c.name, n.name))))
        .collect();
    for (id, label) in routes {
        if let Some(n) = nodes.get_mut(&id) {
            n.entry = Some(Entry::new(EntryKind::Prophecy, label));
        }
    }
}

/// Where one prophecy stands.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Stand {
    /// The prophecy: a class, or a function that is not a member of one.
    pub id: String,
    pub file: String,
    pub line: u32,
    /// What it replaces: the node, or the name when the code holds no such node.
    pub supersedes: Option<String>,
    /// The name resolves to a node.
    pub holds: bool,
    pub veiled: bool,
    pub sealed: Option<String>,
    /// The built code that leans on it.
    pub leaned_on_by: Vec<String>,
    /// What is still empty: the operations of a class, or the function itself.
    pub bodies: usize,
}

/// The revelation: every prophecy, what it replaces and what is left of it. The epic is done at
/// zero. A prophecy that is fulfilled has lost its marker, and is not here.
pub fn revelation(g: &Graph) -> Vec<Stand> {
    let nodes: std::collections::HashMap<&str, &Node> = g.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let mut out: Vec<Stand> = g
        .nodes
        .iter()
        .filter(|n| n.prophecy.is_some() && !n.library)
        .filter(|n| n.cls.as_deref().and_then(|c| nodes.get(c)).map_or(true, |c| c.prophecy.is_none()))
        .map(|n| {
            let p = n.prophecy.clone().unwrap_or_default();
            let members: Vec<&str> = n
                .methods
                .iter()
                .map(|m| format!("{}.{m}", n.id))
                .filter_map(|id| nodes.get_key_value(id.as_str()).map(|(k, _)| *k))
                .collect();
            let mut leaned: Vec<String> = g
                .edges
                .iter()
                .filter(|e| !matches!(e.kind, EdgeKind::Contains | EdgeKind::Imports))
                .filter(|e| e.dst == n.id || members.contains(&e.dst.as_str()))
                .filter(|e| {
                    nodes.get(e.src.as_str()).is_some_and(|s| s.prophecy.is_none() && s.cls.as_deref() != Some(n.id.as_str()) && !s.library)
                })
                .map(|e| e.src.clone())
                .collect();
            leaned.sort();
            leaned.dedup();
            Stand {
                id: n.id.clone(),
                file: std::path::Path::new(&n.file)
                    .strip_prefix(&g.project_root)
                    .map(crate::tree::path_str)
                    .unwrap_or_else(|_| n.file.clone()),
                line: n.span.map_or(1, |s| s.0),
                supersedes: p.replaces.clone().or(p.supersedes.clone()),
                holds: p.supersedes.is_none() || p.replaces.is_some(),
                veiled: p.veiled,
                sealed: p.sealed,
                leaned_on_by: leaned,
                bodies: if n.kind == NodeKind::Class { members.len() } else { 1 },
            }
        })
        .collect();
    out.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
    out
}

/// The revelation of one part as text: a line for each prophecy, and under it what is left.
pub fn report(root: &str, stands: &[Stand]) -> String {
    let many = |n: usize, one: &str, more: &str| format!("{n} {}", if n == 1 { one } else { more });
    let mut out = format!("{root} · {}\n", many(stands.len(), "prophecy", "prophecies"));
    for s in stands {
        let replaces = match (&s.supersedes, s.holds) {
            (Some(r), true) => format!("  supersedes `{r}`"),
            (Some(r), false) => format!("  supersedes `{r}`, which the code does not hold (a false prophecy)"),
            (None, _) => String::new(),
        };
        out += &format!("\n  {}:{}  {}{replaces}\n", s.file, s.line, s.id.rsplit('.').next().unwrap_or(&s.id));
        let mut left = vec![many(s.bodies, "body to write", "bodies to write")];
        if s.veiled {
            left.push("veiled: the place is known, the shape is not".into());
        }
        if let Some(q) = &s.sealed {
            left.push(format!("sealed on {q}"));
        }
        if !s.leaned_on_by.is_empty() {
            left.push(format!("leaned on by {}", s.leaned_on_by.iter().map(|l| format!("`{l}`")).collect::<Vec<_>>().join(", ")));
        }
        out += &format!("      {}\n", left.join(" · "));
    }
    out
}

/// Split on commas and spaces that are not inside quotes.
fn tokens(s: &str) -> Vec<String> {
    let (mut out, mut cur, mut quote) = (vec![], String::new(), None);
    for c in s.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => {
                quote = None;
                cur.push(c);
            }
            (Some(_), c) => cur.push(c),
            (None, '"' | '\'') => {
                quote = Some(c);
                cur.push(c);
            }
            (None, c) if c == ',' || c.is_whitespace() => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            (None, c) => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    // `supersedes = "x"` written with spaces round the sign
    let mut joined: Vec<String> = vec![];
    for t in out {
        match joined.last_mut() {
            Some(last) if last.ends_with('=') || t.starts_with('=') => last.push_str(&t),
            _ => joined.push(t),
        }
    }
    joined
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_meaning_in_three_forms() {
        let want = Prophecy { supersedes: Some("pkg.Old".into()), veiled: false, sealed: Some("#144".into()), replaces: None };
        assert_eq!(Prophecy::of_decorator(r##"prophecy(supersedes="pkg.Old", sealed="#144")"##), Some(want.clone()));
        assert_eq!(Prophecy::of_decorator(r##"doc = "prophecy: supersedes=pkg.Old sealed=#144""##), Some(want.clone()));
        assert_eq!(Prophecy::of_decorator(r##"prophecy(supersedes = "pkg.Old", sealed = "#144")"##), Some(want.clone()));
        assert_eq!(Prophecy::of_jsdoc("/**\n * What it will be.\n * @prophecy supersedes=pkg.Old sealed=#144\n */"), Some(want));
    }

    #[test]
    fn bare_marker_and_veil() {
        assert_eq!(Prophecy::of_decorator("prophecy"), Some(Prophecy::default()));
        assert_eq!(Prophecy::of_decorator("kit.prophecy(veiled=True)").map(|p| p.veiled), Some(true));
        assert_eq!(Prophecy::of_jsdoc("/** @prophecy veiled */").map(|p| p.veiled), Some(true));
    }

    #[test]
    fn other_decorators_are_not_prophecies() {
        assert_eq!(Prophecy::of_decorator("prophecies"), None);
        assert_eq!(Prophecy::of_decorator(r#"doc = "what it does""#), None);
        assert_eq!(Prophecy::of_decorator("derive(Debug)"), None);
        assert_eq!(Prophecy::of_jsdoc("/** @prophecyish */"), None);
    }
}
