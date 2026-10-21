//! A prophecy: sketch code kept in the tree, marked as not built yet.
//!
//! Each language marks it in its own form, and all three are one construct: the decorator
//! `@prophecy(supersedes="pkg.Old")` in Python, the attribute `#[doc = "prophecy: supersedes=pkg::Old"]`
//! (or `#[prophecy(supersedes = "pkg::Old")]` where a macro writes it) in Rust, the JSDoc tag
//! `/** @prophecy supersedes=pkg.Old */` in TypeScript. The readers find the marker; what it says is
//! read here, once.

use indexmap::IndexMap;

use crate::model::{Edge, EdgeKind, Entry, EntryKind, Node, NodeKind, Prophecy};

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

/// What the graph, once whole, says of its prophecies: the node each `supersedes` names (`named`),
/// or the nodes when it names several, and a route for each operation of a prophecy that no other
/// prophecy calls, so that a sketch is read as a route is.
pub fn settle(nodes: &mut IndexMap<String, Node>, edges: &[Edge]) {
    let named: Vec<(String, Vec<String>)> = nodes
        .values()
        .filter_map(|p| {
            let written = p.prophecy.as_ref()?.supersedes.as_deref()?;
            Some((p.id.clone(), named(written, p, nodes)))
        })
        .collect();
    for (id, mut found) in named {
        if let Some(p) = nodes.get_mut(&id).and_then(|n| n.prophecy.as_mut()) {
            match found.len() {
                1 => p.replaces = found.pop(),
                _ => p.candidates = found,
            }
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

/// The nodes a prophecy's `supersedes` names: the node whose id it is, else the code's nodes whose
/// id ends so, prophecies and libraries aside. `::` and `.` part a path alike; a path written in
/// Rust's form starts where the prophecy's module says: `crate::` at its crate (as does the crate's
/// name, written with `_` for `-`), `self::` at the module, `super::` at the module above it.
fn named(written: &str, p: &Node, nodes: &IndexMap<String, Node>) -> Vec<String> {
    let mut path: Vec<&str> = written.split("::").flat_map(|s| s.split('.')).collect();
    if written.contains("::") {
        let mut at: Vec<&str> = p.module.split('.').collect();
        let supers = path.iter().take_while(|s| **s == "super").count();
        // how much of the module's path the written one starts from, and how many of its words say so
        let from = match path[0] {
            "crate" => Some((1, 1)),
            head if head == at[0].replace('-', "_") => Some((1, 1)),
            "self" => Some((at.len(), 1)),
            "super" => Some((at.len().saturating_sub(supers).max(1), supers)),
            _ => None,
        };
        if let Some((keep, said)) = from {
            at.truncate(keep);
            path = at.into_iter().chain(path.into_iter().skip(said)).collect();
        }
    }
    let name = path.join(".");
    if nodes.get(&name).is_some_and(|n| n.id != p.id) {
        return vec![name];
    }
    let tail = format!(".{name}");
    nodes.values().filter(|n| n.id != p.id && n.prophecy.is_none() && !n.library && n.id.ends_with(&tail)).map(|n| n.id.clone()).collect()
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
        let want = Prophecy { supersedes: Some("pkg.Old".into()), sealed: Some("#144".into()), ..Default::default() };
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
