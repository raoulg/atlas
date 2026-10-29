//! `check: Graph -> [Finding]`: the principles' findings, those the author has answered set aside. A decision is
//! written beside the code it answers, after the language's line comment
//! (`# atlas: ignore[changes] in place for the frame rate`), or for whole paths in a `.atlasignore`
//! at the top of what is read, one entry a line, in gitignore's patterns:
//!
//! ```text
//! src/sim/particles.py  changes   # in place for the frame rate, see docs/perf.md
//! src/legacy/**                   # ported pipeline, rewrite planned
//! ```
//!
//! A decision names the principles it answers, or none for all of them, and gives its reason. A
//! finding it covers stays a finding: it is set aside with the decision, counted, and listed when
//! asked for. A decision that sets nothing aside, gives no reason or names no principle is itself
//! a question.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use serde::Serialize;

use crate::model::{Graph, NodeKind};
use crate::principles::{Aside, Finding, Ink, Look, Principle};

/// The file of decisions by path, at the top of what is read.
pub const FILE: &str = ".atlasignore";

/// What a decision in the code starts with, after the comment that holds it.
const MARK: &str = "atlas:";

/// A decision that sets nothing aside, and why, ending in a question for the author.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Stale {
    /// `file:line`, the file from the project root.
    pub at: String,
    /// The decision as written.
    pub says: String,
    pub why: String,
}

/// One decision, as read.
struct Decision {
    /// The file it is written in, from the root of the file system, and its line.
    file: String,
    line: u32,
    says: String,
    covers: Covers,
    /// The principles it answers; all of them when it names none.
    principles: Vec<Principle>,
    reason: String,
    /// Why it covers nothing whatever the findings.
    fault: Option<String>,
}

enum Covers {
    /// The files a pattern of `.atlasignore` matches, below the directory that holds it.
    Paths(PathBuf, Gitignore),
    /// The line of a file.
    Line(String, u32),
}

/// Every finding of `g` (`principles::check`), each one a decision covers with that decision: the
/// first in the code, else the first of a `.atlasignore`.
pub fn check(g: &Graph) -> Vec<Finding> {
    let mut findings = crate::principles::check(g);
    let decisions = decisions(g);
    for f in &mut findings {
        f.aside = covering(g, &decisions, f).map(|i| Aside { by: shown(g, &decisions[i]), reason: decisions[i].reason.clone() });
    }
    findings
}

/// The decisions of the parts that set nothing aside in any of them: each part with the findings
/// `check` gives it.
pub fn stale(parts: &[(&Graph, &[Finding])]) -> Vec<Stale> {
    let mut seen: BTreeMap<(String, u32), (Stale, bool)> = BTreeMap::new();
    for (g, findings) in parts {
        let decisions = decisions(g);
        let mut used = vec![false; decisions.len()];
        for i in findings.iter().filter_map(|f| covering(g, &decisions, f)) {
            used[i] = true;
        }
        for (d, used) in decisions.iter().zip(used) {
            let why = d.fault.clone().unwrap_or_else(|| "It covers no finding: is the decision still needed?".to_string());
            let entry =
                seen.entry((d.file.clone(), d.line)).or_insert_with(|| (Stale { at: shown(g, d), says: d.says.clone(), why }, false));
            entry.1 |= used;
        }
    }
    seen.into_values().filter(|(_, used)| !used).map(|(s, _)| s).collect()
}

/// The decisions that set nothing aside, each with where it stands, what it says and why, ending
/// in its question.
pub fn report(decisions: &[Stale], look: &Look) -> String {
    if decisions.is_empty() {
        return String::new();
    }
    let n = decisions.len();
    let mut out =
        format!("\n{}\n", look.paint(Ink::Count, &format!("{n} {} set nothing aside", if n == 1 { "decision" } else { "decisions" })));
    for d in decisions {
        out += &format!("\n  {}  {}\n", look.paint(Ink::At, &d.at), look.wrapped(Ink::Text, &d.says, 4 + d.at.len(), 4));
        out += &format!("    {} {}\n", look.paint(Ink::Ask, "?"), look.wrapped(Ink::Text, &d.why, 6, 6));
    }
    out
}

/// The decision that covers `f`, by its place among `decisions`, when one does: one in the code
/// before one of a `.atlasignore`.
fn covering(g: &Graph, decisions: &[Decision], f: &Finding) -> Option<usize> {
    if f.file.is_empty() {
        return None;
    }
    let file = absolute(g, g.node(&f.node).map_or(f.file.as_str(), |n| n.file.as_str()));
    let covers = |d: &Decision| match &d.covers {
        Covers::Line(at, line) => *at == file && *line == f.line,
        Covers::Paths(dir, patterns) => {
            Path::new(&file).strip_prefix(dir).is_ok_and(|rel| patterns.matched_path_or_any_parents(rel, false).is_ignore())
        }
    };
    let answers = |d: &Decision| d.fault.is_none() && (d.principles.is_empty() || d.principles.contains(&f.principle));
    let in_code = |d: &Decision| matches!(d.covers, Covers::Line(..));
    let first = |code: bool| decisions.iter().position(|d| in_code(d) == code && answers(d) && covers(d));
    first(true).or_else(|| first(false))
}

/// Every decision `g` holds: those in its own code, by file and line, then those of each
/// `.atlasignore` its files hold.
fn decisions(g: &Graph) -> Vec<Decision> {
    let own: std::collections::BTreeSet<&str> =
        g.nodes.iter().filter(|n| !n.library && n.kind != NodeKind::External && !n.file.is_empty()).map(|n| n.file.as_str()).collect();
    let mut out = vec![];
    for file in own {
        let (Some(text), Some(comment)) = (g.files.get(file), crate::language::comment_of(file)) else { continue };
        out.extend(marks(&absolute(g, file), text, comment));
    }
    for (file, text) in &g.files {
        if Path::new(file).file_name().is_some_and(|n| n == FILE) {
            out.extend(entries(file, text));
        }
    }
    out
}

/// The decisions written in a file's comments, each covering its own line, or, alone on its line,
/// the first line below that is not only a comment.
fn marks(file: &str, text: &str, comment: &str) -> Vec<Decision> {
    let lines: Vec<&str> = text.lines().collect();
    let only_comment = |l: &str| l.trim_start().starts_with(comment);
    let mut out = vec![];
    for (i, l) in lines.iter().enumerate() {
        let Some(said) = mark(l, comment) else { continue };
        let covers = match only_comment(l) {
            false => i,
            true => match (i + 1..lines.len()).find(|&j| !only_comment(lines[j])) {
                Some(j) => j,
                None => continue,
            },
        };
        let (principles, reason, fault) = said;
        out.push(Decision {
            file: file.to_string(),
            line: i as u32 + 1,
            says: l[l.find(MARK).unwrap_or(0)..].trim().to_string(),
            covers: Covers::Line(file.to_string(), covers as u32 + 1),
            principles,
            reason,
            fault,
        });
    }
    out
}

/// The decision in a line's comment (`# atlas: ignore[changes] the reason`), alone or after
/// another comment on its line: the principles it names, its reason, and its fault.
fn mark(line: &str, comment: &str) -> Option<(Vec<Principle>, String, Option<String>)> {
    let said = &line[opens(line, comment)?..];
    let rest = said.split(comment).skip(1).find_map(|c| c.trim_start().strip_prefix(MARK)?.trim_start().strip_prefix("ignore"))?;
    let (ids, rest) = match rest.strip_prefix('[') {
        Some(r) => match r.split_once(']') {
            Some((ids, rest)) => (named(ids.split(',').map(str::trim).filter(|p| !p.is_empty())), rest),
            None => return Some((vec![], String::new(), Some("Its `[` is left open: which principles does it name?".to_string()))),
        },
        None if rest.is_empty() || rest.starts_with(char::is_whitespace) => (Ok(vec![]), rest),
        None => return None,
    };
    let reason = rest.trim_start_matches(|c: char| c.is_whitespace() || matches!(c, '-' | '—' | '–' | ':' | '#' | '/')).trim();
    let (principles, fault) = fault(ids, reason);
    Some((principles, reason.to_string(), fault))
}

/// Where a line's comment opens: the first `comment` outside a string. A `#` language quotes
/// strings with either mark; a `//` language with `"` or a template's backtick, since `'` also
/// opens a lifetime.
fn opens(line: &str, comment: &str) -> Option<usize> {
    let quotes: &[char] = if comment == "#" { &['"', '\''] } else { &['"', '`'] };
    let mut open: Option<char> = None;
    let mut chars = line.char_indices();
    while let Some((i, c)) = chars.next() {
        match open {
            Some(_) if c == '\\' => {
                chars.next();
            }
            Some(q) if c == q => open = None,
            Some(_) => {}
            None if line[i..].starts_with(comment) => return Some(i),
            None if quotes.contains(&c) => open = Some(c),
            None => {}
        }
    }
    None
}

/// The entries of a `.atlasignore`, each a gitignore pattern, the principles it answers, and its
/// reason after `#`; a line that starts with `#` is a comment.
fn entries(file: &str, text: &str) -> Vec<Decision> {
    let dir = Path::new(file).parent().map(Path::to_path_buf).unwrap_or_default();
    let mut out = vec![];
    for (i, l) in text.lines().enumerate() {
        let said = l.trim();
        if said.is_empty() || said.starts_with('#') {
            continue;
        }
        let (rule, reason) = match said.find(" #").or_else(|| said.find("\t#")) {
            Some(at) => (said[..at].trim(), said[at + 2..].trim()),
            None => (said, ""),
        };
        let mut words = rule.split_whitespace();
        let pattern = words.next().unwrap_or_default();
        let (principles, mut fault) = fault(named(words), reason);
        if pattern.starts_with('!') {
            fault = Some("It takes back what another entry sets aside: can that entry be narrowed instead?".to_string());
        }
        let mut patterns = GitignoreBuilder::new(&dir);
        let built = patterns.add_line(None, pattern).ok().and_then(|b| b.build().ok());
        if built.is_none() {
            fault = fault.or(Some(format!("`{pattern}` is no pattern: which files does it mean?")));
        }
        out.push(Decision {
            file: file.to_string(),
            line: i as u32 + 1,
            says: said.to_string(),
            covers: Covers::Paths(dir.clone(), built.unwrap_or_else(Gitignore::empty)),
            principles,
            reason: reason.to_string(),
            fault,
        });
    }
    out
}

/// The principles a decision names by their ids, read once; the first id that names none is a fault.
fn named<'a>(ids: impl IntoIterator<Item = &'a str>) -> Result<Vec<Principle>, String> {
    ids.into_iter()
        .map(|id| Principle::parse(id).ok_or_else(|| format!("It names `{id}`, which is no principle: which one does it answer?")))
        .collect()
}

/// The principles a decision names, and why a decision naming them with `reason` covers nothing, when it
/// does not.
fn fault(named: Result<Vec<Principle>, String>, reason: &str) -> (Vec<Principle>, Option<String>) {
    match named {
        Err(fault) => (vec![], Some(fault)),
        Ok(principles) => (principles, reason.is_empty().then(|| "It gives no reason: why does the code stand as it does?".to_string())),
    }
}

/// A file of `g` from the root of the file system.
fn absolute(g: &Graph, file: &str) -> String {
    match Path::new(file).is_absolute() || g.project_root.is_empty() {
        true => file.to_string(),
        false => crate::tree::path_str(&Path::new(&g.project_root).join(file)),
    }
}

/// Where a decision is written, `file:line`, the file from the project root.
fn shown(g: &Graph, d: &Decision) -> String {
    format!("{}:{}", relative(Path::new(&g.project_root), Path::new(&d.file)), d.line)
}

/// `path` from `root`, with a `..` for each directory of `root` it lies above.
fn relative(root: &Path, path: &Path) -> String {
    if root.as_os_str().is_empty() {
        return crate::tree::path_str(path);
    }
    let (r, p): (Vec<Component>, Vec<Component>) = (root.components().collect(), path.components().collect());
    let shared = r.iter().zip(&p).take_while(|(a, b)| a == b).count();
    let mut out = PathBuf::new();
    for _ in shared..r.len() {
        out.push("..");
    }
    for c in &p[shared..] {
        out.push(c);
    }
    crate::tree::path_str(&out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A decision in the code is read in the comment that holds it, alone or after another; one in
    /// a string is no decision.
    #[test]
    fn a_mark_is_read_in_a_comment() {
        let (named, reason, fault) = mark("def f(xs):  # atlas: ignore[changes] in place, for speed", "#").unwrap();
        assert_eq!((named, reason.as_str(), fault), (vec![Principle::Changes], "in place, for speed", None));
        assert_eq!(mark("fn f() {} // clippy says so // atlas: ignore — ported", "//").unwrap().1, "ported");
        assert!(mark("let s = \"// atlas: ignore[changes] x\";", "//").is_none(), "a string holds no comment");
        assert!(mark("# atlas: ignored the rest", "#").is_none(), "`ignored` is no `ignore`");
        assert!(mark("fn f(s: &'static str) {  // atlas: ignore[gives] kept", "//").is_some(), "a lifetime opens no string");
        assert!(mark("x = 1  # don't # atlas: ignore[gives] kept", "#").is_some(), "a comment's apostrophe opens no string");
        assert!(mark("# atlas: ignore[changes]", "#").unwrap().2.is_some(), "a decision without a reason has a fault");
        assert!(mark("# atlas: ignore[chnages] typo", "#").unwrap().2.is_some(), "a principle that is none is a fault");
    }

    /// A mark alone on its line covers the first line below it that is not only a comment; at the
    /// end of a line, that line.
    #[test]
    fn a_mark_covers_its_line_or_the_one_below() {
        let text = "# atlas: ignore[changes] in place\n# more said\ndef f(xs):  # atlas: ignore[gives] none\n    pass\n";
        let covered: Vec<(u32, u32)> = marks("/r/a.py", text, "#")
            .iter()
            .map(|d| match d.covers {
                Covers::Line(_, l) => (d.line, l),
                Covers::Paths(..) => unreachable!(),
            })
            .collect();
        assert_eq!(covered, [(1, 3), (3, 3)]);
    }

    /// An entry of `.atlasignore` is a pattern, the principles it answers and a reason after `#`.
    #[test]
    fn an_entry_is_a_pattern_principles_and_a_reason() {
        let text = "# decisions\n\nsrc/sim/particles.py  changes gives  # in place\nsrc/legacy/**  # ported\nsrc/x.py\n!src/y.py  # back\n";
        let read: Vec<(u32, Vec<Principle>, String, bool)> =
            entries("/r/.atlasignore", text).into_iter().map(|d| (d.line, d.principles, d.reason, d.fault.is_none())).collect();
        assert_eq!(
            read,
            [
                (3, vec![Principle::Changes, Principle::Gives], "in place".to_string(), true),
                (4, vec![], "ported".to_string(), true),
                (5, vec![], String::new(), false),
                (6, vec![], "back".to_string(), false),
            ]
        );
    }

    /// A path from the project root climbs above it with `..`.
    #[test]
    fn a_path_from_the_root() {
        assert_eq!(relative(Path::new("/r/api"), Path::new("/r/api/src/a.py")), "src/a.py");
        assert_eq!(relative(Path::new("/r/api"), Path::new("/r/.atlasignore")), "../.atlasignore");
    }
}
