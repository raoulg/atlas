//! A change shows as itself: the TypeScript fixture in a repository of its own, committed,
//! then edited.

use std::path::{Path, PathBuf};
use std::process::Command;

use atlas_engine::changes;
use atlas_engine::flow::Side;

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=atlas", "-c", "user.email=atlas@example.org", "-c", "commit.gpgsign=false"])
        .args(args)
        .status()
        .expect("git runs")
        .success();
    assert!(ok, "git {args:?}");
}

/// The fixture copied into a fresh repository, committed: its root keeps the fixture's name.
fn repository(name: &str) -> PathBuf {
    let from = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixture-ts");
    let repo = std::env::temp_dir().join(format!("atlas-test-{name}-{}", std::process::id())).join("fixture-ts");
    let _ = std::fs::remove_dir_all(&repo);
    for e in walkdir::WalkDir::new(&from).into_iter().flatten().filter(|e| e.file_type().is_file()) {
        let dest = repo.join(e.path().strip_prefix(&from).unwrap());
        std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
        std::fs::copy(e.path(), dest).unwrap();
    }
    git(&repo, &["init", "-q"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "fixture"]);
    repo
}

/// A handler that no longer asks the model and one that writes a log: two routes changed, each
/// by exactly its step and its place. The report's lines are a contract of their own (a route
/// changed, and a step it no longer reaches), pinned here once.
#[test]
fn a_new_write_and_a_removed_call_show_as_themselves() {
    let repo = repository("changes");
    let regels = repo.join("src/server/regels.ts");
    let mut src = std::fs::read_to_string(&regels).unwrap();
    for (from, to) in [
        ("[await vat(\"x\")]", "[\"x\"]"),
        ("import { Hono } from \"hono\";", "import fs from \"node:fs\";\nimport { Hono } from \"hono\";"),
        (
            "    await pool.query(BEZOEK);",
            "    fs.appendFileSync(\"logs/regels.log\", c.req.param(\"id\"));\n    await pool.query(BEZOEK);",
        ),
    ] {
        assert!(src.contains(from), "the fixture writes {from}");
        src = src.replacen(from, to, 1);
    }
    std::fs::write(&regels, src).unwrap();
    let c = changes::since(&repo, "HEAD", &atlas_engine::Options::default()).expect("the working tree against HEAD");
    let routes: Vec<&str> = c.routes.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(routes, ["fixture-ts.server.regels.regels.GET /:id", "fixture-ts.server.regels.regels.POST /"], "{}", changes::report(&c));
    let (get, post) = (&c.routes[0], &c.routes[1]);
    assert!(get.places_removed.is_empty() && get.steps_added.is_empty() && get.steps_removed.is_empty());
    assert_eq!(post.steps_removed, ["fixture-ts.server.regels.vat"]);
    assert_eq!(post.root, "fixture-ts");
    let touched = |ts: &[atlas_engine::flow::Drawn]| {
        ts.iter().map(|t| (t.touch.kind_name(), t.touch.artifact.clone(), t.at.side)).collect::<Vec<_>>()
    };
    assert_eq!(touched(&get.touches_added), [("disk_write", "logs/regels.log".to_string(), Side::Out)]);
    assert_eq!(get.places_added.len(), 1, "one place: {:?}", get.places_added);
    assert_eq!(touched(&post.touches_removed).iter().map(|t| t.0).collect::<Vec<_>>(), ["llm"], "{:?}", post.touches_removed);
    assert_eq!(post.places_removed.len(), 1, "one place: {:?}", post.places_removed);
    assert!(post.places_added.is_empty() && post.steps_added.is_empty());
    let report = changes::report(&c);
    assert!(report.contains("~ API · POST /api/regels\n    - step server.regels.vat\n"), "{report}");
    let _ = std::fs::remove_dir_all(repo.parent().unwrap());
}

/// A tree against the commit it is at differs in nothing: no route, nothing on no route, no finding.
#[test]
fn an_unchanged_tree_says_so() {
    let repo = repository("same");
    let c = changes::since(&repo, "HEAD", &atlas_engine::Options::default()).unwrap();
    assert!(c.routes.is_empty() && c.findings_added.is_empty() && c.findings_resolved.is_empty(), "{}", changes::report(&c));
    let e = &c.elsewhere;
    assert!(
        e.added.is_empty()
            && e.removed.is_empty()
            && e.changed.is_empty()
            && e.places_added.is_empty()
            && e.places_removed.is_empty()
            && e.places_changed.is_empty(),
        "{}",
        changes::report(&c)
    );
    let _ = std::fs::remove_dir_all(repo.parent().unwrap());
}

/// A route serves a set of methods, so the order the code lists them in says nothing: listed in
/// either order, in a list, a tuple or a set, `laws/methods` serves one route, labelled by the
/// words of the requests it serves, and `changes` between any two of its readings is empty
/// ("Isomorphic is the same"; "Words are a label, never an identity").
#[test]
fn methods_listed_in_any_order_are_one_route() {
    use atlas_engine::{analyze, Graph, Methods, Options};
    let tests = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests");
    let program = tests.join("laws/methods");
    let app = std::fs::read_to_string(program.join("src/menu/app.py")).unwrap();
    let listed = r#"["POST", "GET"]"#;
    assert!(app.contains(listed), "the program lists its methods as {listed}");
    let forms = [listed, r#"["GET", "POST"]"#, r#"("POST", "GET")"#, r#"{"GET", "POST"}"#, r#"{"POST", "GET"}"#];
    // the program with its methods written in `form`, its libraries' stubs where it finds them
    let read = |i: usize, form: &str| -> Graph {
        let dir = std::env::temp_dir().join(format!("atlas-test-methods-{}-{i}", std::process::id())).join("methods");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src/menu")).unwrap();
        let typings = tests.join("fixture-parts/typings").display().to_string().replace('\\', "/");
        let manifest = std::fs::read_to_string(program.join("pyproject.toml")).unwrap().replace("../../fixture-parts/typings", &typings);
        std::fs::write(dir.join("pyproject.toml"), manifest).unwrap();
        std::fs::copy(program.join("src/menu/__init__.py"), dir.join("src/menu/__init__.py")).unwrap();
        std::fs::write(dir.join("src/menu/app.py"), app.replacen(listed, form, 1)).unwrap();
        let g = analyze(&dir, &Options::default()).unwrap_or_else(|e| panic!("{form}: {e}"));
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
        g
    };
    let readings: Vec<Graph> = forms.iter().enumerate().map(|(i, f)| read(i, f)).collect();
    let route = |g: &Graph| {
        let n = g.nodes.iter().find(|n| n.name == "items").expect("the handler is read");
        n.entry.clone().unwrap_or_else(|| panic!("{} is a route", n.id))
    };
    let first = route(&readings[0]);
    let request = first.request.clone().expect("a route serves requests");
    assert_eq!(request.methods, Methods::of(["GET", "POST"]), "the methods the code names");
    assert_eq!(first.label, request.to_string(), "the label is the words of the requests it serves");
    for (form, g) in forms.iter().zip(&readings) {
        assert_eq!(route(g), first, "{form}: one route");
        let c = changes::between("listed", None, std::slice::from_ref(&readings[0]), std::slice::from_ref(g));
        let e = &c.elsewhere;
        let nothing = c.routes.is_empty()
            && e.added.is_empty()
            && e.removed.is_empty()
            && e.changed.is_empty()
            && e.places_added.is_empty()
            && e.places_removed.is_empty()
            && e.places_changed.is_empty();
        assert!(nothing, "{form}: {}", changes::report(&c));
    }
}

const KEPT_PYPROJECT: &str = "[project]\nname = \"blocks\"\nversion = \"0.1.0\"\nrequires-python = \">=3.12\"\n\n\
                              [tool.ty.environment]\nextra-paths = [\"typings\"]\n";
const KEPT_TORCH: &str = "class Tensor:\n    def __ne__(self, other: object) -> Tensor: ...\n";
const KEPT_SPARSE: &str = r#"from torch import Tensor


class Rows:
    def __init__(self, nonzero: Tensor) -> None:
        self.nonzero = nonzero

    @classmethod
    def of(cls, rows: Tensor) -> "Rows":
        return cls(rows != 0)
"#;
const KEPT_BLOCKS: &str = r#"from torch import Tensor

from blocks import sparse


class Block:
    def scaled(self, x: Tensor) -> Tensor:
        return x
"#;
/// The block with one more method that builds a `sparse.Rows`: the type gains an arrow, and the block's route reaches it
/// in this reading only.
const KEPT_MORE: &str = "\n    def kept(self, x: Tensor) -> sparse.Rows:\n        return sparse.Rows.of(x)\n";

/// Law: the nodes the union marks changed that a route reaches are the nodes the report says changed, with what they were,
/// wherever the route reaches them: in both readings or in one only. The program's second commit gives a type an arrow
/// the block's route reaches only then.
#[test]
fn a_changed_node_a_route_reaches_in_one_reading_only_is_said_with_what_it_was() {
    let repo = std::env::temp_dir().join(format!("atlas-test-kept-{}", std::process::id())).join("blocks");
    let _ = std::fs::remove_dir_all(&repo);
    for (file, text) in [
        ("pyproject.toml", KEPT_PYPROJECT),
        ("typings/torch/__init__.pyi", KEPT_TORCH),
        ("src/blocks/__init__.py", KEPT_BLOCKS),
        ("src/blocks/sparse.py", KEPT_SPARSE),
    ] {
        std::fs::create_dir_all(repo.join(file).parent().unwrap()).unwrap();
        std::fs::write(repo.join(file), text).unwrap();
    }
    git(&repo, &["init", "-q"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "block"]);
    std::fs::write(repo.join("src/blocks/__init__.py"), format!("{KEPT_BLOCKS}{KEPT_MORE}")).unwrap();
    git(&repo, &["commit", "-q", "-am", "kept"]);
    let opts = atlas_engine::Options::default();
    let before = changes::parts_at(&repo, Some("HEAD~1"), &opts).expect("the first commit reads");
    let after = changes::parts_at(&repo, Some("HEAD"), &opts).expect("the second commit reads");
    let c = changes::between("HEAD~1", Some("HEAD"), &before, &after);
    let (_, d) = atlas_engine::diff::union("blocks", before, after);
    let _ = std::fs::remove_dir_all(repo.parent().unwrap());
    let report = changes::report(&c);
    let mut witnessed = false;
    for r in &c.routes {
        let said: Vec<&str> = r.steps_changed.iter().map(|s| s.id.as_str()).collect();
        let listed = r.steps_added.iter().chain(&r.steps_removed);
        let marked: Vec<&str> =
            listed.filter(|id| d.nodes.get(id.as_str()) == Some(&atlas_engine::diff::Status::Changed)).map(String::as_str).collect();
        witnessed |= !marked.is_empty();
        for id in marked {
            assert!(said.contains(&id), "{id} is listed as added or removed and the union marks it changed: {report}");
        }
        assert!(r.steps_changed.iter().all(|s| !s.was.is_empty()), "every changed step says what it was: {report}");
    }
    assert!(witnessed, "the program has a changed node a route reaches in one reading only: {report}");
}
