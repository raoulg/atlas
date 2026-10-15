//! A prophecy: sketch code kept in the tree, marked as not built yet.
//!
//! Each language marks it in its own form, and all three are one construct: the decorator
//! `@prophecy(supersedes="pkg.Old")` in Python, the attribute `#[doc = "prophecy: supersedes=pkg::Old"]`
//! (or `#[prophecy(supersedes = "pkg::Old")]` where a macro writes it) in Rust, the JSDoc tag
//! `/** @prophecy supersedes=pkg.Old */` in TypeScript. The readers find the marker; what it says is
//! read here, once.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

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
        let want = Prophecy { supersedes: Some("pkg.Old".into()), veiled: false, sealed: Some("#144".into()) };
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
