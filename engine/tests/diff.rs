//! Two readings as one picture: a small Python program in a repository of its own, committed,
//! changed and committed again, and the union of the two commits marked as a diff marks lines.

use std::path::{Path, PathBuf};
use std::process::Command;

use atlas_engine::changes::{self, RefKind};
use atlas_engine::diff::{self, Diff, Status};
use atlas_engine::{Graph, Options};

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=atlas", "-c", "user.email=atlas@example.org", "-c", "commit.gpgsign=false", "-c", "tag.gpgsign=false"])
        .args(args)
        .status()
        .expect("git runs")
        .success();
    assert!(ok, "git {args:?}");
}

const PYPROJECT: &str = "[project]\nname = \"spots\"\nversion = \"0.1.0\"\n\n[project.scripts]\nspots = \"spots.cli:main\"\n";

/// A repository named `spots` with the program `cli` as its one module, committed.
fn repository(name: &str, cli: &str) -> PathBuf {
    let repo = std::env::temp_dir().join(format!("atlas-diff-{name}-{}", std::process::id())).join("spots");
    let _ = std::fs::remove_dir_all(&repo);
    std::fs::create_dir_all(repo.join("src/spots")).unwrap();
    std::fs::write(repo.join("pyproject.toml"), PYPROJECT).unwrap();
    std::fs::write(repo.join("src/spots/__init__.py"), "from spots.cli import main\n\n__all__ = [\"main\"]\n").unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    commit(&repo, cli, "first");
    repo
}

/// The program changed to `cli`, committed.
fn commit(repo: &Path, cli: &str, message: &str) {
    std::fs::write(repo.join("src/spots/cli.py"), cli).unwrap();
    git(repo, &["add", "."]);
    git(repo, &["commit", "-q", "-m", message]);
}

/// The union of the commit before HEAD and HEAD.
fn union(repo: &Path) -> (Graph, Diff) {
    let opts = Options::default();
    let before = changes::parts_at(repo, Some("HEAD~1"), &opts).expect("the first commit reads");
    let after = changes::parts_at(repo, Some("HEAD"), &opts).expect("the second commit reads");
    diff::union("spots", before, after)
}

fn done(repo: &Path) {
    let _ = std::fs::remove_dir_all(repo.parent().unwrap());
}

const FRAME: &str = r#"from pathlib import Path


class Frame:
    def __init__(self, values: list[float]) -> None:
        self.values = values


def load(path: Path) -> Frame:
    return Frame([float(x) for x in path.read_text().split()])


def smooth(frame: Frame) -> Frame:
    return Frame(frame.values[1:])
"#;

const PLOT: &str = r#"

def plot(frame: Frame, out: Path) -> None:
    out.write_text(" ".join(str(v) for v in frame.values))


def main() -> None:
    frame = smooth(load(Path("data/raw.csv")))
    plot(frame, Path("figures/spots.txt"))
"#;

const TREND: &str = r#"

class Trend:
    def __init__(self, frame: Frame) -> None:
        self.frame = frame

    def save(self, out: Path) -> None:
        out.write_text(" ".join(str(v) for v in self.frame.values))


def main() -> None:
    frame = smooth(load(Path("data/raw.csv")))
    Trend(frame).save(Path("figures/spots.txt"))
"#;

/// The steps of `id` in the union with their marks: (what each reaches, its status).
fn steps(g: &Graph, d: &Diff, id: &str) -> Vec<(String, Option<Status>)> {
    let n = g.node(id).unwrap();
    n.sites
        .iter()
        .enumerate()
        .map(|(i, s)| {
            (if s.to.is_empty() { format!("op:{}", s.op) } else { s.to.clone() }, d.sites.get(id).and_then(|m| m.get(&i)).copied())
        })
        .collect()
}

/// `plot` gives way to `Trend(frame).save(..)`: the step B no longer has is removed, the steps it
/// adds are added, in call order, and the file both write keeps its name and is changed, since
/// another function writes it now. `main` takes and gives what it did, so it is not changed.
#[test]
fn steps_come_and_go_and_a_place_changes_its_writer() {
    let repo = repository("writer", &format!("{FRAME}{PLOT}"));
    commit(&repo, &format!("{FRAME}{TREND}"), "Trend draws");
    let (g, d) = union(&repo);
    assert_eq!(d.nodes.get("spots.cli.plot"), Some(&Status::Removed));
    assert_eq!(d.nodes.get("spots.cli.Trend"), Some(&Status::Added));
    assert_eq!(d.nodes.get("spots.cli.Trend.save"), Some(&Status::Added));
    assert!(g.node("spots.cli.plot").is_some(), "the union holds what B no longer has");
    assert_eq!(d.nodes.get("spots.cli.main"), None, "main's own arrow is the same: {:?}", d.was.get("spots.cli.main"));
    let main = steps(&g, &d, "spots.cli.main");
    let marked: Vec<(&str, Status)> = main.iter().filter_map(|(to, s)| s.map(|s| (to.as_str(), s))).collect();
    assert!(marked.contains(&("spots.cli.plot", Status::Removed)), "{main:?}");
    assert!(marked.contains(&("spots.cli.Trend.save", Status::Added)), "{main:?}");
    let at = |to: &str| main.iter().position(|(t, _)| t == to).unwrap();
    assert!(at("spots.cli.smooth") < at("spots.cli.plot") && at("spots.cli.plot") < at("spots.cli.Trend.save"), "call order: {main:?}");
    assert!(main.iter().filter(|(_, s)| s.is_none()).any(|(t, _)| t == "spots.cli.smooth"), "a step both have is unmarked: {main:?}");
    assert_eq!(d.places.get("w:File:figures/spots.txt"), Some(&Status::Changed), "{:?}", d.places);
    assert_eq!(d.was.get("w:File:figures/spots.txt").map(String::as_str), Some("plot writes"));
    assert_eq!(d.places.get("w:File:data/raw.csv"), None, "the file both read the same way is the same");
    let removed = g
        .edges
        .iter()
        .enumerate()
        .filter(|(i, _)| d.edges.get(i) == Some(&Status::Removed))
        .map(|(_, e)| (e.src.as_str(), e.dst.as_str()))
        .collect::<Vec<_>>();
    assert!(removed.contains(&("spots.cli.main", "spots.cli.plot")), "{removed:?}");
    done(&repo);
}

/// `smooth` changes the frame it is handed in A and gives a new one back in B: its own arrow
/// differs, and what it was is said in words.
#[test]
fn a_function_whose_return_type_changed_is_changed() {
    let a = FRAME.replace(
        "def smooth(frame: Frame) -> Frame:\n    return Frame(frame.values[1:])",
        "def smooth(frame: Frame) -> None:\n    frame.values = frame.values[1:]",
    );
    let main_a = PLOT.replace("frame = smooth(load(Path(\"data/raw.csv\")))", "frame = load(Path(\"data/raw.csv\"))\n    smooth(frame)");
    let repo = repository("returns", &format!("{a}{main_a}"));
    commit(&repo, &format!("{FRAME}{PLOT}"), "smooth gives back a frame");
    let (_, d) = union(&repo);
    assert_eq!(d.nodes.get("spots.cli.smooth"), Some(&Status::Changed));
    let was = d.was.get("spots.cli.smooth").unwrap();
    assert!(was.starts_with("smooth(frame: Frame) → None"), "{was}");
    assert_eq!(d.nodes.get("spots.cli.plot"), None);
    assert_eq!(d.nodes.get("spots.cli.load"), None);
    done(&repo);
}

/// `plot` renamed to `draw`, with the same relations: one node, changed, under its old name too;
/// never removed and added.
#[test]
fn a_renamed_function_is_one_node() {
    let repo = repository("rename", &format!("{FRAME}{PLOT}"));
    commit(&repo, &format!("{FRAME}{}", PLOT.replace("plot(", "draw(")), "plot is draw");
    let (g, d) = union(&repo);
    assert_eq!(d.renamed.get("spots.cli.draw").map(String::as_str), Some("spots.cli.plot"), "{:?}", d.nodes);
    assert_eq!(d.nodes.get("spots.cli.draw"), Some(&Status::Changed));
    assert_eq!(d.nodes.get("spots.cli.plot"), None, "not removed");
    assert!(g.node("spots.cli.plot").is_none(), "one node");
    assert!(d.sites.get("spots.cli.main").is_none(), "the call is the same call: {:?}", d.sites);
    done(&repo);
}

/// Two functions with the same relations, both renamed: either could be either, so Atlas does not
/// guess, and both are removed and both added.
#[test]
fn two_renames_that_could_match_either_way_are_not_guessed() {
    let two = |a: &str, b: &str| {
        format!("{FRAME}\n\ndef {a}(frame: Frame) -> Frame:\n    return frame\n\n\ndef {b}(frame: Frame) -> Frame:\n    return frame\n\n\ndef main() -> None:\n    frame = load(Path(\"data/raw.csv\"))\n    {a}(frame)\n    {b}(frame)\n")
    };
    let repo = repository("either", &two("first", "second"));
    commit(&repo, &two("alpha", "beta"), "renamed both");
    let (_, d) = union(&repo);
    assert!(d.renamed.is_empty(), "{:?}", d.renamed);
    for (id, s) in [("first", Status::Removed), ("second", Status::Removed), ("alpha", Status::Added), ("beta", Status::Added)] {
        assert_eq!(d.nodes.get(&format!("spots.cli.{id}")), Some(&s), "{id}");
    }
    done(&repo);
}

/// `load` takes a second parameter: it is changed, and what runs after it (`smooth`, `plot`) and
/// the step that calls it (`main`) are not. A change marks the step that changed.
#[test]
fn a_change_upstream_marks_only_the_step_that_changed() {
    let repo = repository("upstream", &format!("{FRAME}{PLOT}"));
    let b = FRAME.replace(
        "def load(path: Path) -> Frame:\n    return Frame([float(x) for x in path.read_text().split()])",
        "def load(path: Path, skip: int) -> Frame:\n    return Frame([float(x) for x in path.read_text().split()][skip:])",
    );
    commit(&repo, &format!("{b}{}", PLOT.replace("load(Path(\"data/raw.csv\"))", "load(Path(\"data/raw.csv\"), 1)")), "load skips");
    let (_, d) = union(&repo);
    assert_eq!(d.nodes.get("spots.cli.load"), Some(&Status::Changed));
    for id in ["spots.cli.smooth", "spots.cli.plot", "spots.cli.main", "spots.cli.Frame"] {
        assert_eq!(d.nodes.get(id), None, "{id}: {:?}", d.was.get(id));
    }
    done(&repo);
}

/// The history as the page lists it: newest first, each commit with its parents and the names that
/// point at it, by kind; `all` adds the branches HEAD's history does not hold.
#[test]
fn the_log_names_branches_and_tags() {
    let repo = repository("log", &format!("{FRAME}{PLOT}"));
    git(&repo, &["tag", "v0.1"]);
    git(&repo, &["checkout", "-q", "-b", "trend"]);
    commit(&repo, &format!("{FRAME}{TREND}"), "Trend draws");
    git(&repo, &["checkout", "-q", "main"]);
    commit(&repo, &format!("{FRAME}{}", PLOT.replace("plot(", "draw(")), "plot is draw");
    let log = changes::log(&repo, true, 300, 0).unwrap();
    let subjects: Vec<&str> = log.commits.iter().map(|c| c.subject.as_str()).collect();
    assert_eq!(subjects.len(), 3, "{subjects:?}");
    assert_eq!(subjects[2], "first", "the root comes last");
    assert_eq!(log.head, log.commits.iter().find(|c| c.subject == "plot is draw").unwrap().hash);
    assert!(!log.dirty && log.changed == 0);
    let refs =
        |s: &str| log.commits.iter().find(|c| c.subject == s).unwrap().refs.iter().map(|r| (r.name.as_str(), r.kind)).collect::<Vec<_>>();
    assert_eq!(refs("plot is draw"), [("main", RefKind::Head)]);
    assert_eq!(refs("Trend draws"), [("trend", RefKind::Branch)]);
    assert_eq!(refs("first"), [("v0.1", RefKind::Tag)]);
    let first = log.commits.iter().find(|c| c.subject == "first").unwrap();
    assert!(first.parents.is_empty() && first.short.len() >= 7 && first.time > 0 && first.author == "atlas");
    assert!(log.commits.iter().filter(|c| c.subject != "first").all(|c| c.parents == [first.hash.clone()]));
    let own = changes::log(&repo, false, 300, 0).unwrap();
    assert_eq!(own.commits.iter().map(|c| c.subject.as_str()).collect::<Vec<_>>(), ["plot is draw", "first"]);
    let rest = changes::log(&repo, true, 300, 1).unwrap();
    assert_eq!(
        rest.commits.iter().map(|c| c.hash.as_str()).collect::<Vec<_>>(),
        log.commits[1..].iter().map(|c| c.hash.as_str()).collect::<Vec<_>>(),
        "skip leaves out the newest"
    );
    std::fs::write(repo.join("notes.txt"), "a file git does not know yet").unwrap();
    std::fs::write(repo.join("src/spots/cli.py"), FRAME).unwrap();
    git(&repo, &["mv", "src/spots/__init__.py", "src/spots/package.py"]);
    let edited = changes::log(&repo, false, 1, 0).unwrap();
    assert!(edited.dirty);
    assert_eq!(edited.changed, 3, "a new file, an edited one, a renamed one");
    assert_eq!(changes::refs("HEAD"), [changes::Ref { name: "HEAD".into(), kind: RefKind::Head }], "a detached HEAD");
    assert_eq!(changes::refs("refs/remotes/origin/main, refs/stash"), [changes::Ref { name: "origin/main".into(), kind: RefKind::Remote }]);
    done(&repo);
}
