//! A prophecy: sketch code kept in the tree, marked as not built yet.
//!
//! Each language marks it in its own form, and all three are one construct: the decorator
//! `@prophecy(supersedes="pkg.Old")` in Python, the attribute `#[doc = "prophecy: supersedes=pkg::Old"]`
//! (or `#[prophecy(supersedes = "pkg::Old")]` where a macro writes it) in Rust, the JSDoc tag
//! `/** @prophecy supersedes=pkg.Old */` in TypeScript. The readers find the marker; what it says is
//! read here, once, and a marker is written here in each form.

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

/// The form a language marks a prophecy in: one construct, written three ways.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Form {
    /// Python's decorator, `@prophecy(supersedes="pkg.Old")`: a call of the project's own
    /// `prophecy`, which Python reads by its name.
    Decorator,
    /// Rust's attribute, `#[doc = "prophecy: supersedes=pkg::Old"]`.
    Attribute,
    /// TypeScript's JSDoc tag, `/** @prophecy supersedes=pkg.Old */`.
    JsDoc,
}

impl Form {
    /// The line that marks the item that starts at a line, at the item's indentation. A decorator is
    /// always a call: the project's `prophecy` takes keywords and gives the decorator, so a bare
    /// `@prophecy` would make the item what the call gives, and ty would type it so.
    pub fn marker(self, indent: &str, supersedes: Option<&str>) -> String {
        let by = supersedes.map(|s| format!(" supersedes={s}")).unwrap_or_default();
        match self {
            Form::Decorator => format!("{indent}@prophecy({})", supersedes.map(|s| format!("supersedes=\"{s}\"")).unwrap_or_default()),
            Form::Attribute => format!("{indent}#[doc = \"prophecy:{by}\"]"),
            Form::JsDoc => format!("{indent}/** @prophecy{by} */"),
        }
    }

    /// Put the marker above the item at `line` (0-based). An item that has a JSDoc right above it
    /// takes the tag into that comment, which a second comment would hide.
    pub fn insert(self, lines: &mut Vec<String>, line: usize, marker: String) {
        match self {
            Form::JsDoc if line > 0 && lines[line - 1].trim_end().ends_with("*/") => {
                let tag = marker.trim().trim_start_matches("/**").trim_end_matches("*/").trim().to_string();
                let indent: String = marker.chars().take_while(|c| c.is_whitespace()).collect();
                let above = lines[line - 1].trim().to_string();
                if above.starts_with("/**") && above.ends_with("*/") && above.len() > 5 {
                    let said = above.trim_start_matches("/**").trim_end_matches("*/").trim().to_string();
                    lines.splice(
                        line - 1..line,
                        [format!("{indent}/**"), format!("{indent} * {said}"), format!("{indent} * {tag}"), format!("{indent} */")],
                    );
                } else if above.starts_with("*/") || above.starts_with('*') {
                    lines.insert(line - 1, format!("{indent} * {tag}"));
                } else {
                    lines.insert(line, marker);
                }
            }
            _ => lines.insert(line, marker),
        }
    }
}

/// What the graph, once whole, says of its prophecies: the node each `supersedes` names (`named`),
/// or the nodes when it names several, and a route for each operation of a prophecy that no other
/// prophecy calls, so that a sketch is read as a route is.
pub fn settle(nodes: &mut IndexMap<String, Node>, edges: &[Edge]) {
    let named: Vec<(String, Vec<String>)> = nodes
        .values()
        .filter_map(|p| {
            let written = p.prophecy()?.supersedes.as_deref()?;
            Some((p.id.clone(), named(written, p, nodes)))
        })
        .collect();
    for (id, mut found) in named {
        if let Some(p) = nodes.get_mut(&id).and_then(|n| n.prophecy_mut()?.as_mut()) {
            match found.len() {
                1 => p.replaces = found.pop(),
                _ => p.candidates = found,
            }
        }
    }
    let prophetic = |nodes: &IndexMap<String, Node>, id: &str| {
        nodes.get(id).is_some_and(|n| n.prophecy().is_some() || n.cls().and_then(|c| nodes.get(c)).is_some_and(|c| c.prophecy().is_some()))
    };
    let called_by_prophecy: std::collections::HashSet<&str> =
        edges.iter().filter(|e| e.kind == EdgeKind::Calls && prophetic(nodes, &e.src)).map(|e| e.dst.as_str()).collect();
    let routes: Vec<(String, String)> = nodes
        .values()
        .filter(|n| {
            matches!(n.kind.tag(), NodeKind::Function | NodeKind::Method) && n.entry.is_none() && !n.library && prophetic(nodes, &n.id)
        })
        .filter(|n| !called_by_prophecy.contains(n.id.as_str()))
        .map(|n| (n.id.clone(), n.cls().and_then(|c| nodes.get(c)).map_or(n.name.clone(), |c| format!("{}.{}", c.name, n.name))))
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
    nodes.values().filter(|n| n.id != p.id && n.prophecy().is_none() && !n.library && n.id.ends_with(&tail)).map(|n| n.id.clone()).collect()
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
    fn each_form_writes_what_it_reads() {
        for form in [Form::Decorator, Form::Attribute, Form::JsDoc] {
            let marker = form.marker("    ", Some("pkg.Old"));
            assert!(marker.starts_with("    "), "{marker}");
            let read = match form {
                Form::JsDoc => Prophecy::of_jsdoc(&marker),
                _ => Prophecy::of_decorator(marker.trim().trim_start_matches('@').trim_start_matches("#[").trim_end_matches(']')),
            };
            assert_eq!(read.and_then(|p| p.supersedes).as_deref(), Some("pkg.Old"), "{form:?}: {marker}");
        }
        assert_eq!(Form::Decorator.marker("", None), "@prophecy()");
    }

    #[test]
    fn a_jsdoc_tag_goes_into_the_comment_above() {
        let mut lines: Vec<String> = ["/** Where reports wait. */", "export class Vault {}"].map(String::from).to_vec();
        Form::JsDoc.insert(&mut lines, 1, Form::JsDoc.marker("", None));
        assert_eq!(lines, ["/**", " * Where reports wait.", " * @prophecy", " */", "export class Vault {}"]);
        let mut lines: Vec<String> = ["# Where reports wait.", "class Vault:"].map(String::from).to_vec();
        Form::Decorator.insert(&mut lines, 1, Form::Decorator.marker("", None));
        assert_eq!(lines, ["# Where reports wait.", "@prophecy()", "class Vault:"]);
    }

    #[test]
    fn other_decorators_are_not_prophecies() {
        assert_eq!(Prophecy::of_decorator("prophecies"), None);
        assert_eq!(Prophecy::of_decorator(r#"doc = "what it does""#), None);
        assert_eq!(Prophecy::of_decorator("derive(Debug)"), None);
        assert_eq!(Prophecy::of_jsdoc("/** @prophecyish */"), None);
    }
}
