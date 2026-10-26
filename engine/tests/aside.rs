//! A decision answers findings, it does not change them: the findings of a program with decisions
//! are its findings without them, some set aside. The same decision is written alike in every
//! language, after its line comment, and a `.atlasignore` answers by path in any of them.

use std::path::{Path, PathBuf};

use atlas_engine::aside::{check, stale};
use atlas_engine::changes::raised;
use atlas_engine::parts::analyze_parts;
use atlas_engine::principles::Finding;
use atlas_engine::{Graph, Options};

/// The `changes` shortcut of a language, copied where a test may write in it. Each finds something:
/// what Python and TypeScript change unseen, and in Rust, where `&mut` shows a change, a call that
/// leaves nothing behind.
fn copied(language: &str, test: &str) -> PathBuf {
    let from = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/changes").join(language).join("shortcut");
    let to = std::env::temp_dir().join(format!("atlas-aside-{test}-{language}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&to);
    copy(&from, &to);
    to
}

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let target = to.join(e.file_name());
        match e.file_type().unwrap().is_dir() {
            true => copy(&e.path(), &target),
            false => drop(std::fs::copy(e.path(), target).unwrap()),
        }
    }
}

fn read(dir: &Path) -> Vec<Graph> {
    analyze_parts(&[dir.to_path_buf()], &Options::default()).expect("the pair reads")
}

fn found(parts: &[Graph]) -> Vec<Finding> {
    parts.iter().flat_map(check).collect()
}

/// A finding as it is found, whatever answers it.
fn bare(f: &Finding) -> Finding {
    Finding { aside: None, ..f.clone() }
}

/// A finding by what it says, wherever its line stands.
fn said(f: &Finding) -> (&'static str, String, String) {
    (f.principle, f.node.clone(), f.what.clone())
}

const COMMENT: [(&str, &str); 3] = [("python", "#"), ("rust", "//"), ("typescript", "//")];

/// Write `line` in the file, at the 1-based `at`: after what stands there, or as a line of its own
/// before it.
fn write(file: &Path, at: u32, line: &str, own: bool) {
    let text = std::fs::read_to_string(file).unwrap();
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let i = at as usize - 1;
    match own {
        true => {
            let indent: String = lines[i].chars().take_while(|c| c.is_whitespace()).collect();
            lines.insert(i, format!("{indent}{line}"));
        }
        false => lines[i] = format!("{}  {line}", lines[i]),
    }
    std::fs::write(file, lines.join("\n") + "\n").unwrap();
}

/// The first finding that stands in a file.
fn first(findings: &[Finding]) -> Finding {
    findings.iter().find(|f| !f.file.is_empty()).expect("the shortcut finds something").clone()
}

/// A decision at the end of the line a finding stands on sets aside that line's findings of the
/// principle it names, and changes no finding: in every language, the findings with it are the
/// findings without it.
#[test]
fn a_decision_sets_aside_and_finds_the_same() {
    for (language, comment) in COMMENT {
        let dir = copied(language, "line");
        let before = found(&read(&dir));
        let first = first(&before);
        let (file, line, p) = (dir.join(&first.file), first.line, first.principle);
        write(&file, line, &format!("{comment} atlas: ignore[{p}] in place, by design"), false);
        let after = found(&read(&dir));
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(after.iter().map(bare).collect::<Vec<_>>(), before, "{language}: a decision changes no finding");
        for f in &after {
            let answered = f.file == first.file && f.line == line && f.principle == p;
            assert_eq!(
                f.aside.is_some(),
                answered,
                "{language}: {}:{} {} is set aside only where the decision answers it",
                f.file,
                f.line,
                f.what
            );
        }
        let aside = after.iter().find_map(|f| f.aside.as_ref()).unwrap();
        assert_eq!((aside.by.clone(), aside.reason.as_str()), (format!("{}:{line}", first.file), "in place, by design"));
    }
}

/// A decision alone on its line answers the first line below it that is not only a comment, in
/// every language; one that names another principle answers nothing there and is itself asked.
#[test]
fn a_decision_on_its_own_line_answers_the_line_below() {
    for (language, comment) in COMMENT {
        let dir = copied(language, "above");
        let before = found(&read(&dir));
        let first = first(&before);
        let (file, p) = (dir.join(&first.file), first.principle);
        let other = if p == "gives" { "doors" } else { "gives" };
        write(&file, first.line, &format!("{comment} atlas: ignore[{p}] in place, by design"), true);
        write(&file, first.line + 1, &format!("{comment} what the function is for"), true);
        let parts = read(&dir);
        let after = found(&parts);
        let answered: Vec<_> = after.iter().filter(|f| f.aside.is_some()).map(said).collect();
        let at_line: Vec<_> =
            before.iter().filter(|f| f.file == first.file && f.line == first.line && f.principle == p).map(said).collect();
        assert_eq!(answered, at_line, "{language}: the decision answers the line below its comments");
        let sorted = |mut fs: Vec<(&'static str, String, String)>| {
            fs.sort();
            fs
        };
        assert_eq!(sorted(after.iter().map(said).collect()), sorted(before.iter().map(said).collect()), "{language}: and finds the same");
        assert!(stale(&parts.iter().map(|g| (g, after.as_slice())).collect::<Vec<_>>()).is_empty(), "{language}: it is used");

        write(&file, first.line + 2, &format!("{comment} atlas: ignore[{other}] says nothing of {p}"), false);
        let parts = read(&dir);
        let again = found(&parts);
        let _ = std::fs::remove_dir_all(&dir);
        let decisions = stale(&parts.iter().map(|g| (g, again.as_slice())).collect::<Vec<_>>());
        assert_eq!(decisions.len(), 1, "{language}: a decision that answers no finding there is asked about: {decisions:?}");
        assert!(decisions[0].says.contains(&format!("ignore[{other}]")), "{language}: {decisions:?}");
    }
}

/// An entry of `.atlasignore` answers the findings of the principles it names in the files its
/// pattern matches; an entry that matches nothing, gives no reason or names no principle answers
/// nothing and is asked about, each once.
#[test]
fn an_entry_answers_by_path() {
    for (language, _) in COMMENT {
        let dir = copied(language, "file");
        let before = found(&read(&dir));
        let p = first(&before).principle;
        std::fs::write(
            dir.join(".atlasignore"),
            format!("# decisions\nsrc/**  {p}  # ported, rewrite planned\nsrc/**  gives\ngone/**  # removed\n*  chnages  # a typo\n"),
        )
        .unwrap();
        let parts = read(&dir);
        let after = found(&parts);
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(after.iter().map(bare).collect::<Vec<_>>(), before, "{language}: an entry changes no finding");
        for f in &after {
            let answered = f.principle == p && f.file.starts_with("src/");
            assert_eq!(f.aside.is_some(), answered, "{language}: {}:{} {}", f.file, f.line, f.what);
            if let Some(a) = &f.aside {
                assert_eq!((a.by.as_str(), a.reason.as_str()), (".atlasignore:2", "ported, rewrite planned"));
            }
        }
        let asked: Vec<String> =
            stale(&parts.iter().map(|g| (g, after.as_slice())).collect::<Vec<_>>()).into_iter().map(|s| s.at).collect();
        assert_eq!(asked, [".atlasignore:3", ".atlasignore:4", ".atlasignore:5"], "{language}");
    }
}

/// What a decision sets aside is neither raised nor resolved between two readings: it is still
/// found, and answered.
#[test]
fn a_finding_set_aside_is_not_resolved() {
    let dir = copied("python", "raised");
    let before = found(&read(&dir));
    std::fs::write(dir.join(".atlasignore"), "src/**  changes  # ported\n").unwrap();
    let after = found(&read(&dir));
    let _ = std::fs::remove_dir_all(&dir);
    let (up, resolved, aside) = raised(&before, &after);
    let changes: Vec<_> = before.iter().filter(|f| f.principle == "changes").map(said).collect();
    assert!(up.is_empty() && resolved.is_empty(), "nothing is raised or resolved");
    assert_eq!(aside.iter().map(said).collect::<Vec<_>>(), changes, "the findings answered are set aside");
    let (up, resolved, aside) = raised(&after, &before);
    assert!(resolved.is_empty() && aside.is_empty(), "taking the decision back resolves nothing");
    assert_eq!(up.iter().map(said).collect::<Vec<_>>(), changes, "and asks its findings again");
}
