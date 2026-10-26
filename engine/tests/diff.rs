//! Two readings as one picture: a small Python program in a repository of its own, committed,
//! changed and committed again, and the union of the two commits marked as a diff marks lines.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use atlas_engine::changes::{self, RefKind};
use atlas_engine::diff::{self, Diff, Reading, Status};
use atlas_engine::flow::Side;
use atlas_engine::model::NodeKind;
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

/// The commit before HEAD and HEAD, each read.
fn readings(repo: &Path) -> (Vec<Graph>, Vec<Graph>) {
    let opts = Options::default();
    let before = changes::parts_at(repo, Some("HEAD~1"), &opts).expect("the first commit reads");
    let after = changes::parts_at(repo, Some("HEAD"), &opts).expect("the second commit reads");
    (before, after)
}

/// The union of the commit before HEAD and HEAD, and what `atlas changes` lists for them, which
/// holds every node and place the union marks: the page and the agent are told one story.
fn both(repo: &Path) -> (Graph, Diff, changes::Changes) {
    let (before, after) = readings(repo);
    let c = changes::between("HEAD~1", Some("HEAD"), &before, &after);
    let (g, d) = diff::union("spots", before, after);
    let mut nodes: BTreeSet<&str> = BTreeSet::new();
    let mut places: BTreeSet<&str> = BTreeSet::new();
    for r in &c.routes {
        nodes.insert(&r.id);
        nodes.extend(r.steps_added.iter().chain(&r.steps_removed).map(String::as_str));
        nodes.extend(r.steps_changed.iter().map(|s| s.id.as_str()));
        places.extend(r.touches_added.iter().chain(&r.touches_removed).map(|t| t.at.place.id.as_str()));
        places.extend(r.places_changed.iter().map(|p| p.id.as_str()));
    }
    let e = &c.elsewhere;
    nodes.extend(e.added.iter().chain(&e.removed).map(String::as_str).chain(e.changed.iter().map(|s| s.id.as_str())));
    places.extend(e.places_added.iter().chain(&e.places_removed).chain(&e.places_changed).map(|p| p.id.as_str()));
    let report = changes::report(&c);
    for id in d.nodes.keys() {
        let n = g.node(id).unwrap();
        if n.kind != NodeKind::External && !n.library {
            assert!(nodes.contains(id.as_str()), "the page marks {id}, `atlas changes` does not list it:\n{report}");
        }
    }
    // a coarser place stands for the places it holds until they are split, and is listed by them
    let drawn: BTreeSet<String> = g.nodes.iter().flat_map(|n| n.touches.iter().chain(&n.instead)).map(|t| t.drawn().place.id).collect();
    for id in d.places.keys().filter(|id| drawn.contains(*id)) {
        assert!(places.contains(id.as_str()), "the page marks {id}, `atlas changes` does not list it:\n{report}");
    }
    (g, d, c)
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

/// The functions whose touches rest at `place` in reading `r`, each with the side it stands on.
fn touching(g: &Graph, d: &Diff, r: Reading, place: &str) -> Vec<(String, Side)> {
    let side = diff::reading(g, d, r);
    let mut out: Vec<(String, Side)> = side
        .nodes
        .iter()
        .flat_map(|n| n.touches.iter().chain(&n.instead).map(move |t| (n, t.drawn())))
        .filter(|(_, w)| w.place.id == place || w.place.coarse.as_ref().is_some_and(|c| c.id == place))
        .map(|(n, w)| (n.id.clone(), w.side))
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out.dedup();
    out
}

/// The places `caller` makes whole in reading `r` (it hands the path a function it calls touches),
/// each with the side the touch stands on: the places of its own arrow that its own body does not
/// touch.
fn made_whole(g: &Graph, d: &Diff, r: Reading, caller: &str) -> Vec<(String, Side)> {
    let side = diff::reading(g, d, r);
    side.nodes
        .iter()
        .flat_map(|n| &n.touches)
        .filter(|t| t.by.iter().any(|b| b.split('&').any(|f| f == caller)))
        .map(|t| (t.drawn().place.id, t.drawn().side))
        .collect()
}

/// `plot` gives way to `Trend(frame).save(..)`: the step B no longer has is removed, the steps it
/// adds are added, in call order, and the file both write keeps its name and is changed, since
/// another function writes it now. `main` takes and gives what it did, so it is not changed.
#[test]
fn steps_come_and_go_and_a_place_changes_its_writer() {
    let repo = repository("writer", &format!("{FRAME}{PLOT}"));
    commit(&repo, &format!("{FRAME}{TREND}"), "Trend draws");
    let (g, d, _) = both(&repo);
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
    let writers = |r: Reading| touching(&g, &d, r, "w:File:figures/spots.txt");
    assert_eq!(writers(Reading::A), [("spots.cli.plot".to_string(), Side::Out)]);
    assert_eq!(writers(Reading::B), [("spots.cli.Trend.save".to_string(), Side::Out)]);
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

/// A step whose input comes from a call that changed is still the same step: in B the raw file is
/// fetched only when it is missing, and `load` is handed the path `raw = Path(..)` names rather than
/// what `fetch` gives back. Which step made the value is that step's arrow, so `Path` is added and
/// `load` stays the same step, and so do `smooth` and what follows it. A vision that adds a condition
/// marks the calls it changes, not the whole route.
#[test]
fn a_step_after_a_changed_call_stays_the_same() {
    let fetch = "\n\ndef fetch(path: Path) -> Path:\n    path.write_text(\"1 2 3\")\n    return path\n";
    let a = format!("{FRAME}{fetch}{PLOT}")
        .replace("frame = smooth(load(Path(\"data/raw.csv\")))", "frame = smooth(load(fetch(Path(\"data/raw.csv\"))))");
    let b = format!("{FRAME}{fetch}{PLOT}").replace(
        "    frame = smooth(load(Path(\"data/raw.csv\")))",
        "    raw = Path(\"data/raw.csv\")\n    if not raw.exists():\n        fetch(raw)\n    frame = smooth(load(raw))",
    );
    assert!(a.contains("load(fetch(") && b.contains("if not raw.exists()"));
    let repo = repository("chain", &a);
    commit(&repo, &b, "fetch once");
    let (g, d, _) = both(&repo);
    let main = steps(&g, &d, "spots.cli.main");
    let same = |to: &str| main.iter().any(|(t, s)| t == to && s.is_none());
    assert!(main.iter().any(|(t, s)| t == "op:Path" && *s == Some(Status::Added)), "{main:?}");
    assert!(same("spots.cli.load") && same("spots.cli.smooth") && same("spots.cli.plot"), "{main:?}");
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
    let (_, d, _) = both(&repo);
    assert_eq!(d.nodes.get("spots.cli.smooth"), Some(&Status::Changed));
    let was = d.was.get("spots.cli.smooth").unwrap();
    assert!(was.contains("None"), "what it gave in A: {was}");
    assert_eq!(d.nodes.get("spots.cli.plot"), None);
    assert_eq!(d.nodes.get("spots.cli.load"), None);
    done(&repo);
}

/// `plot` renamed to `draw`, with the same relations: one node, changed, under its old name too;
/// never removed and added, on the page or in `atlas changes`.
#[test]
fn a_renamed_function_is_one_node() {
    let repo = repository("rename", &format!("{FRAME}{PLOT}"));
    commit(&repo, &format!("{FRAME}{}", PLOT.replace("plot(", "draw(")), "plot is draw");
    let (g, d, c) = both(&repo);
    assert_eq!(d.renamed.get("spots.cli.draw").map(String::as_str), Some("spots.cli.plot"), "{:?}", d.nodes);
    assert_eq!(d.nodes.get("spots.cli.draw"), Some(&Status::Changed));
    assert_eq!(d.nodes.get("spots.cli.plot"), None, "not removed");
    assert!(g.node("spots.cli.plot").is_none(), "one node");
    assert!(d.sites.get("spots.cli.main").is_none(), "the call is the same call: {:?}", d.sites);
    let route = &c.routes[0];
    assert!(route.steps_added.is_empty() && route.steps_removed.is_empty(), "{}", changes::report(&c));
    assert_eq!(
        route.steps_changed.iter().map(|s| (s.id.as_str(), s.renamed_from.as_deref())).collect::<Vec<_>>(),
        [("spots.cli.draw", Some("spots.cli.plot"))]
    );
    let was = &route.steps_changed[0].was;
    assert!(was.contains("plot") && !was.contains("draw"), "what it was is said as A had it: {was}");
    done(&repo);
}

/// `Frame` gains a field that holds a `Meta`: both types have another arrow between them, so both
/// are changed, on the page and in the list `atlas changes` gives.
#[test]
fn a_type_that_gains_a_field_is_listed() {
    let program = |field: &str| {
        format!("from dataclasses import dataclass\nfrom pathlib import Path\n\n\n@dataclass\nclass Meta:\n    name: str\n\n\n@dataclass\nclass Frame:\n    values: list[float]{field}\n\n\ndef load(path: Path) -> Frame:\n    return Frame([float(x) for x in path.read_text().split()])\n\n\ndef describe(meta: Meta) -> str:\n    return meta.name\n\n\ndef main() -> None:\n    print(load(Path(\"data/raw.csv\")).values)\n    print(describe(Meta(\"raw\")))\n")
    };
    let repo = repository("field", &program(""));
    commit(&repo, &program("\n    meta: Meta | None = None"), "Frame holds a Meta");
    let (_, d, c) = both(&repo);
    assert_eq!(d.nodes.get("spots.cli.Frame"), Some(&Status::Changed), "{:?}", d.nodes);
    let listed: Vec<&str> = c.routes.iter().flat_map(|r| &r.steps_changed).map(|s| s.id.as_str()).collect();
    assert!(listed.contains(&"spots.cli.Frame"), "{}", changes::report(&c));
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
    let (_, d, _) = both(&repo);
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
    let (_, d, _) = both(&repo);
    assert_eq!(d.nodes.get("spots.cli.load"), Some(&Status::Changed));
    for id in ["spots.cli.smooth", "spots.cli.plot", "spots.cli.main", "spots.cli.Frame"] {
        assert_eq!(d.nodes.get(id), None, "{id}: {:?}", d.was.get(id));
    }
    done(&repo);
}

const SWAP: &str = r#"from pathlib import Path


class Frame:
    def __init__(self, values: list[float]) -> None:
        self.values = values


def load(path: Path) -> Frame:
    return Frame([float(x) for x in path.read_text().split()])


def fetch(path: Path) -> Frame:
    return Frame([float(x) for x in path.read_text().split(",")])


def smooth(frame: Frame) -> Frame:
    return Frame(frame.values[1:])


def plot(frame: Frame) -> None:
    Path("figures/spots.txt").write_text(str(frame.values))


def main() -> None:
MAIN
"#;

/// `main` makes its frame with `fetch` where it made it with `load`, the same arrow `Path → Frame`:
/// that one step is removed and one added, and nothing else is marked but the file, which another
/// function reads now. `smooth` takes a frame in both, from whichever step made it: where a value
/// comes from is the arrow of the step that made it, not of the step it goes to. Each reading keeps
/// its chain of values, also where no name holds them.
#[test]
fn a_swapped_producer_marks_the_swap_alone() {
    for (body, name) in [
        ("    raw = PRODUCER(Path(\"data/raw.csv\"))\n    plot(smooth(raw))", "named"),
        ("    plot(smooth(PRODUCER(Path(\"data/raw.csv\"))))", "inline"),
    ] {
        let program = |producer: &str| SWAP.replace("MAIN", &body.replace("PRODUCER", producer));
        let repo = repository(&format!("swap-{name}"), &program("load"));
        commit(&repo, &program("fetch"), "fetch the frame");
        let (g, d, c) = both(&repo);
        assert!(d.nodes.is_empty(), "{name}: no node's own arrow changed: {:?} {:?}", d.nodes, d.was);
        let main = steps(&g, &d, "spots.cli.main");
        assert_eq!(
            main,
            [
                ("spots.cli.load".to_string(), Some(Status::Removed)),
                ("spots.cli.fetch".to_string(), Some(Status::Added)),
                ("spots.cli.smooth".to_string(), None),
                ("spots.cli.plot".to_string(), None)
            ],
            "{name}"
        );
        assert_eq!(d.places.keys().collect::<Vec<_>>(), ["w:File:data/raw.csv"], "{name}");
        let readers = |r: Reading| touching(&g, &d, r, "w:File:data/raw.csv");
        assert_eq!(readers(Reading::A), [("spots.cli.load".to_string(), Side::In)], "{name}");
        assert_eq!(readers(Reading::B), [("spots.cli.fetch".to_string(), Side::In)], "{name}");
        // the frame smooth is handed is one value in both readings, made by load in A and by fetch in B
        assert!(d.hands.is_empty(), "{name}: A hands smooth what B hands it: {:?}", d.hands);
        assert_eq!(
            d.remade.get("spots.cli.main").map(|m| m.iter().map(|(a, b)| (*a, *b)).collect::<Vec<_>>()),
            Some(vec![(0, 1)]),
            "{name}"
        );
        for (reading, producer) in [(Reading::A, "spots.cli.load"), (Reading::B, "spots.cli.fetch")] {
            let side = diff::reading(&g, &d, reading);
            let sites = &side.node("spots.cli.main").unwrap().sites;
            assert_eq!(sites.iter().map(|s| s.to.as_str()).collect::<Vec<_>>(), [producer, "spots.cli.smooth", "spots.cli.plot"]);
            assert_eq!(sites[0].gives[0].name, sites[1].hands[0].name, "{name} {reading:?}: smooth takes what {producer} gives");
        }
        let route = &c.routes[0];
        assert_eq!(
            (route.steps_added.as_slice(), route.steps_removed.as_slice()),
            (["spots.cli.fetch".to_string()].as_slice(), ["spots.cli.load".to_string()].as_slice())
        );
        assert!(route.steps_changed.is_empty(), "{}", changes::report(&c));
        done(&repo);
    }
}

/// `clean` is put in between `load` and `smooth` (and, the other way, taken out): that step alone
/// is marked. `smooth` makes the same call on a frame in both readings, so it is one step, handed
/// in A what `load` gives and in B what `clean` gives; the frame `load` gives is one value too, and
/// only what takes it differs. No value is made by another step.
#[test]
fn a_step_put_in_a_chain_marks_it_alone() {
    let clean = "\n\ndef clean(frame: Frame) -> Frame:\n    return Frame([v for v in frame.values if v >= 0])\n";
    let short = format!("{FRAME}{clean}{PLOT}");
    let long = short.replace("smooth(load(Path(\"data/raw.csv\")))", "smooth(clean(load(Path(\"data/raw.csv\"))))");
    assert_ne!(short, long);
    for (name, a, b, status) in [("insert", &short, &long, Status::Added), ("delete", &long, &short, Status::Removed)] {
        let repo = repository(&format!("chain-{name}"), a);
        commit(&repo, b, name);
        let (g, d, c) = both(&repo);
        assert!(d.nodes.is_empty(), "{name}: no node's own arrow changed: {:?} {:?}", d.nodes, d.was);
        assert!(d.places.is_empty(), "{name}: {:?}", d.places);
        assert_eq!(
            steps(&g, &d, "spots.cli.main"),
            [
                ("spots.cli.load".to_string(), None),
                ("spots.cli.clean".to_string(), Some(status)),
                ("spots.cli.smooth".to_string(), None),
                ("spots.cli.plot".to_string(), None)
            ],
            "{name}"
        );
        assert_eq!(d.hands["spots.cli.main"].keys().collect::<Vec<_>>(), [&2], "{name}: A hands smooth another value than B");
        assert!(d.remade.is_empty(), "{name}: {:?}", d.remade);
        for reading in [Reading::A, Reading::B] {
            let side = diff::reading(&g, &d, reading);
            let sites = &side.node("spots.cli.main").unwrap().sites;
            let calls: Vec<&str> = sites.iter().map(|s| s.to.as_str()).collect();
            let cleans = (reading == Reading::B) == (status == Status::Added);
            let before = if cleans { "spots.cli.clean" } else { "spots.cli.load" };
            assert_eq!(calls.contains(&"spots.cli.clean"), cleans, "{name} {reading:?}: {calls:?}");
            let (at, smooth) =
                (calls.iter().position(|c| *c == before).unwrap(), calls.iter().position(|c| *c == "spots.cli.smooth").unwrap());
            assert_eq!(sites[at].gives[0].name, sites[smooth].hands[0].name, "{name} {reading:?}: smooth takes what {before} gives");
            if cleans {
                assert_eq!(sites[0].gives[0].name, sites[1].hands[0].name, "{name} {reading:?}: clean takes what load gives");
            }
        }
        let route = &c.routes[0];
        let (added, removed) = (route.steps_added.as_slice(), route.steps_removed.as_slice());
        let listed = if status == Status::Added { (added, removed) } else { (removed, added) };
        assert_eq!(listed, (["spots.cli.clean".to_string()].as_slice(), [].as_slice()), "{name}: {}", changes::report(&c));
        assert!(route.steps_changed.is_empty(), "{name}: {}", changes::report(&c));
        done(&repo);
    }
}

const SERIES: &str = r#"from pathlib import Path


class Frame:
    def __init__(self, values: list[float]) -> None:
        self.values = values

    def mean(self) -> float:
        return sum(self.values) / len(self.values)

    def trimmed(self) -> "Frame":
        return Frame(self.values[1:])


def load(path: Path) -> Frame:
    return Frame([float(x) for x in path.read_text().split()])


def smooth(frame: Frame) -> Frame:
    return frame.trimmed()


def main() -> None:
    frame = smooth(load(Path("data/raw.csv")))
    print(frame.mean())
"#;

/// `Frame` renamed to `Series`: the class is one node, renamed, and the steps that carry a frame
/// carry a series, each the same step; its methods keep their names and their arrows.
#[test]
fn a_renamed_class_leaves_the_steps_that_carry_it() {
    let repo = repository("series", SERIES);
    commit(&repo, &SERIES.replace("Frame", "Series"), "Frame is Series");
    let (_, d, c) = both(&repo);
    assert_eq!(d.renamed.get("spots.cli.Series").map(String::as_str), Some("spots.cli.Frame"));
    assert_eq!(d.nodes.iter().collect::<Vec<_>>(), [(&"spots.cli.Series".to_string(), &Status::Changed)], "{:?}", d.was);
    assert!(d.sites.is_empty(), "no step is doubled: {:?}", d.sites);
    let route = &c.routes[0];
    assert!(
        route.steps_added.is_empty() && route.steps_removed.is_empty() && route.compositions_changed.is_empty(),
        "{}",
        changes::report(&c)
    );
    assert_eq!(
        route.steps_changed.iter().map(|s| (s.id.as_str(), s.renamed_from.as_deref())).collect::<Vec<_>>(),
        [("spots.cli.Series", Some("spots.cli.Frame"))]
    );
    done(&repo);
}

/// A rename is a node removed and one added with the same typed arrows in and out. `mean` goes and
/// `label` comes at the same place in the calls, but `list[float] → float` is not `str → str`; a
/// `plot` that becomes `draw` and takes a width more has another arrow too. Both are removed and
/// added.
#[test]
fn a_rename_keeps_the_types_of_its_arrow() {
    let mean = format!("{FRAME}\n\ndef mean(xs: list[float]) -> float:\n    return sum(xs) / len(xs)\n\n\ndef main() -> None:\n    frame = load(Path(\"data/raw.csv\"))\n    print(mean(frame.values))\n");
    let label = format!("{FRAME}\n\ndef label(name: str) -> str:\n    return name.upper()\n\n\ndef main() -> None:\n    frame = load(Path(\"data/raw.csv\"))\n    print(label(\"raw\"))\n");
    let width = PLOT
        .replace("def plot(frame: Frame, out: Path) -> None:", "def draw(frame: Frame, out: Path, width: int) -> None:")
        .replace("plot(frame, Path(\"figures/spots.txt\"))", "draw(frame, Path(\"figures/spots.txt\"), 80)");
    for (name, a, b, gone, new) in
        [("mean", mean, label, "mean", "label"), ("width", format!("{FRAME}{PLOT}"), format!("{FRAME}{width}"), "plot", "draw")]
    {
        let repo = repository(&format!("typed-{name}"), &a);
        commit(&repo, &b, "another function");
        let (_, d, _) = both(&repo);
        assert!(d.renamed.is_empty(), "{name}: {:?}", d.renamed);
        assert_eq!(d.nodes.get(&format!("spots.cli.{gone}")), Some(&Status::Removed), "{name}");
        assert_eq!(d.nodes.get(&format!("spots.cli.{new}")), Some(&Status::Added), "{name}");
        done(&repo);
    }
}

/// `plot` renamed to `draw`, a function nobody calls, which writes the path it is handed: the place
/// is named by its parameter (`plot.out`), and is the same place under the new name.
#[test]
fn a_rename_carries_the_place_named_after_it() {
    let program = |f: &str| {
        format!("from pathlib import Path\n\n\ndef {f}(values: list[float], out: Path) -> None:\n    out.write_text(str(values))\n\n\ndef main() -> None:\n    print(sum([1.0, 2.0]))\n")
    };
    let repo = repository("own-place", &program("plot"));
    commit(&repo, &program("draw"), "plot is draw");
    let (_, d, c) = both(&repo);
    assert_eq!(d.renamed.get("spots.cli.draw").map(String::as_str), Some("spots.cli.plot"), "{:?}", d.nodes);
    assert!(d.places.is_empty(), "{:?}", d.places);
    assert!(d.was["spots.cli.draw"].contains("plot.out"), "what it was is said as A had it: {}", d.was["spots.cli.draw"]);
    assert_eq!(c.elsewhere.changed.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), ["spots.cli.draw"], "{}", changes::report(&c));
    done(&repo);
}

const CALLER: &str = r#"from pathlib import Path


def load(path: Path) -> list[float]:
    return [float(x) for x in path.read_text().split()]


def save(xs: list[float], out: Path) -> None:
    out.write_text(" ".join(str(x) for x in xs))


def main() -> None:
MAIN
"#;

/// A place a caller makes whole is the caller's arrow. `main` stops calling `load`: `main` no longer
/// reads the file, and `load`, written as it was, is the same, reading the path it is handed under
/// one name whoever hands it. `main` hands `save` another path: `main` changes, `save` does not.
#[test]
fn a_place_a_caller_makes_whole_is_the_callers_arrow() {
    let program = |body: &str| CALLER.replace("MAIN", body);
    let repo = repository("drops", &program("    print(sum(load(Path(\"data/raw.csv\"))))"));
    commit(&repo, &program("    print(sum([1.0, 2.0]))"), "main does not load");
    let (g, d, _) = both(&repo);
    assert_eq!(d.nodes.get("spots.cli.load"), None, "{:?}", d.was.get("spots.cli.load"));
    assert_eq!(d.nodes.get("spots.cli.main"), Some(&Status::Changed));
    let raw = ("w:File:data/raw.csv".to_string(), Side::In);
    assert!(made_whole(&g, &d, Reading::A, "spots.cli.main").contains(&raw), "main reads the file in A");
    assert!(!made_whole(&g, &d, Reading::B, "spots.cli.main").contains(&raw), "and not in B");
    done(&repo);
    let save = |out: &str| program(&format!("    save(load(Path(\"data/raw.csv\")), Path(\"{out}\"))"));
    let repo = repository("hands", &save("out/a.txt"));
    commit(&repo, &save("out/b.txt"), "main hands out/b.txt");
    let (g, d, c) = both(&repo);
    assert_eq!(d.nodes.iter().collect::<Vec<_>>(), [(&"spots.cli.main".to_string(), &Status::Changed)], "{:?}", d.was);
    let out = |file: &str| (format!("w:File:{file}"), Side::Out);
    assert!(made_whole(&g, &d, Reading::A, "spots.cli.main").contains(&out("out/a.txt")), "main writes out/a.txt in A");
    assert!(made_whole(&g, &d, Reading::B, "spots.cli.main").contains(&out("out/b.txt")), "main writes out/b.txt in B");
    assert_eq!(d.places.get("w:File:out/a.txt"), Some(&Status::Removed));
    assert_eq!(d.places.get("w:File:out/b.txt"), Some(&Status::Added));
    let route = &c.routes[0];
    assert_eq!(route.steps_changed.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), ["spots.cli.main"]);
    let at = |ts: &[atlas_engine::flow::Drawn]| ts.iter().map(|t| (t.at.place.id.clone(), t.at.side)).collect::<Vec<_>>();
    assert_eq!((at(&route.touches_added), at(&route.touches_removed)), (vec![out("out/b.txt")], vec![out("out/a.txt")]));
    assert_eq!((route.places_added.len(), route.places_removed.len()), (1, 1), "one place each way");
    done(&repo);
}

/// The history as the page lists it: a commit above its parents, and of two where neither is the other's
/// ancestor the newer above, so a branch worked on now stands at the top however old the commit it
/// starts from.
#[test]
fn the_newest_work_stands_at_the_top() {
    let repo = repository("dates", &format!("{FRAME}{PLOT}"));
    let at = |when: &str, args: &[&str]| {
        let done = Command::new("git")
            .args(args)
            .current_dir(&repo)
            .env("GIT_AUTHOR_DATE", when)
            .env("GIT_COMMITTER_DATE", when)
            .status()
            .unwrap();
        assert!(done.success(), "git {args:?}");
    };
    // the two branches take turns: each has work older and newer than the other's
    git(&repo, &["checkout", "-q", "-b", "side"]);
    git(&repo, &["checkout", "-q", "main"]);
    at("2030-01-02T00:00:00", &["commit", "-q", "--allow-empty", "-m", "main one"]);
    git(&repo, &["checkout", "-q", "side"]);
    at("2030-01-03T00:00:00", &["commit", "-q", "--allow-empty", "-m", "side one"]);
    git(&repo, &["checkout", "-q", "main"]);
    at("2030-01-04T00:00:00", &["commit", "-q", "--allow-empty", "-m", "main two"]);
    git(&repo, &["checkout", "-q", "side"]);
    at("2030-01-05T00:00:00", &["commit", "-q", "--allow-empty", "-m", "side, now"]);
    let log = changes::log(&repo, true, 300, 0).unwrap();
    let subjects: Vec<&str> = log.commits.iter().map(|c| c.subject.as_str()).collect();
    assert_eq!(subjects, ["side, now", "main two", "side one", "main one", "first"]);
    let place = |hash: &str| log.commits.iter().position(|c| c.hash == hash);
    for (i, c) in log.commits.iter().enumerate() {
        assert!(c.parents.iter().all(|p| place(p).is_some_and(|j| j > i)), "{} stands above its parents", c.subject);
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
