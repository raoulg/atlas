//! A construct has one name wherever Atlas says it. `atlas changes` lists the nodes a change touches and says what a
//! changed type was by the types and functions at its arrows; it says each by the name the view draws it by. The page's
//! own model (`viewer/plan.js`, run in node on the page's data of the union) gives the view's names: `typeLabel` a
//! type's, `stepName` a function's as a step of a route. The program is a Python package of blocks whose rows are shaped
//! arrays with a dot in their shape (`{self.size}`, `...`), where a name cut at the last dot of an id is no name the view
//! draws; two of its types go by one name, `Rows`, in two modules; a method, an initializer, a function on its class and
//! a function of its module stand at their arrows; and a sum written in place, whose id is no path, changes.

use std::path::{Path, PathBuf};
use std::process::Command;

use atlas_engine::changes::{self, Changes};
use atlas_engine::diff::{self, Diff, Reading};
use atlas_engine::model::{EdgeKind, NodeKind};
use atlas_engine::{flow, Graph, Options};

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

const PYPROJECT: &str = "[project]\nname = \"blocks\"\nversion = \"0.1.0\"\nrequires-python = \">=3.12\"\n\n\
                         [tool.ty.environment]\nextra-paths = [\"typings\"]\n";
const JAXTYPING: &str = "from typing import Annotated\n\nFloat = Annotated\n";
const TORCH: &str =
    "class Tensor:\n    def __mul__(self, other: Tensor) -> Tensor: ...\n    def __ne__(self, other: object) -> Tensor: ...\n";

const DENSE: &str = r#""""Rows of a block's size, in one batch or under any leading dimensions."""

from jaxtyping import Float
from torch import Tensor

Rows = Float[Tensor, "batch {self.size}"] | Float[Tensor, "... {self.size}"]


def ones(size: int) -> Rows:
    return Tensor()
"#;

const SPARSE: &str = r#""""Rows kept beside where they are not zero."""

from torch import Tensor

from blocks import dense


class Rows:
    def __init__(self, rows: dense.Rows, nonzero: Tensor) -> None:
        self.rows = rows
        self.nonzero = nonzero

    @classmethod
    def of(cls, rows: dense.Rows) -> "Rows":
        return cls(rows, rows != 0)

    def filled(self) -> dense.Rows:
        return self.rows
"#;

const BLOCKS: &str = r#""""Blocks that scale rows of the size each block names, and mask them where a sparse mask is not zero."""

from jaxtyping import Float
from torch import Tensor

from blocks import sparse
from blocks.dense import Rows


class Block:
    def __init__(self, size: int, mask: sparse.Rows) -> None:
        self.size = size
        self.mask = mask

    def scaled(self, x: Rows, by: Tensor) -> Rows:
        return x * by

    def masked(self, x: Float[Tensor, "batch {self.size}"] | Tensor, by: Tensor) -> Tensor:
        return x * by
"#;

/// The block with two more methods: one on its rows, so `Rows` gains arrows, and one that takes the union `masked` writes
/// in place, so that sum gains an arrow; `changes` says what each was, and what the block was.
const MORE: &str = r#"
    def doubled(self, x: Rows, by: Tensor) -> Rows:
        return self.scaled(x, by) * by

    def unmasked(self, x: Float[Tensor, "batch {self.size}"] | Tensor) -> Tensor:
        return x
"#;

/// The package `blocks` in a repository of its own under `dir`, committed once as written and once with `MORE` added.
fn repository(dir: &str) -> PathBuf {
    let repo = std::env::temp_dir().join(format!("atlas-one-name-{dir}-{}", std::process::id())).join("blocks");
    let _ = std::fs::remove_dir_all(&repo);
    for (file, text) in [
        ("pyproject.toml", PYPROJECT),
        ("typings/jaxtyping/__init__.pyi", JAXTYPING),
        ("typings/torch/__init__.pyi", TORCH),
        ("src/blocks/__init__.py", BLOCKS),
        ("src/blocks/dense.py", DENSE),
        ("src/blocks/sparse.py", SPARSE),
    ] {
        std::fs::create_dir_all(repo.join(file).parent().unwrap()).unwrap();
        std::fs::write(repo.join(file), text).unwrap();
    }
    git(&repo, &["init", "-q"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "rows"]);
    std::fs::write(repo.join("src/blocks/__init__.py"), format!("{BLOCKS}{MORE}")).unwrap();
    git(&repo, &["commit", "-q", "-am", "more"]);
    repo
}

/// The program read at both commits: what `changes` says, and the union it is read from, with its reading A.
struct Read {
    dir: &'static str,
    c: Changes,
    report: String,
    union: Graph,
    d: Diff,
    a: Graph,
}

fn read(dir: &'static str) -> Read {
    let repo = repository(dir);
    let opts = Options::default();
    let before = changes::parts_at(&repo, Some("HEAD~1"), &opts).expect("the first commit reads");
    let after = changes::parts_at(&repo, Some("HEAD"), &opts).expect("the second commit reads");
    let c = changes::between("HEAD~1", Some("HEAD"), &before, &after);
    let (union, d) = diff::union("blocks", before, after);
    let a = diff::reading(&union, &d, Reading::A);
    let _ = std::fs::remove_dir_all(repo.parent().unwrap());
    Read { dir, report: changes::report(&c), c, union, d, a }
}

/// The names the view draws the nodes `ids` by: `typeLabel` (a type) or `stepName` (a function, as a step of a route) of
/// the page's model, run in node on the page's data of `r`'s union (its graph, its flow and how its things stand to the
/// two readings).
fn drawn(r: &Read, by: &str, ids: &[String]) -> Vec<String> {
    let mut data = serde_json::to_value(&r.union).expect("the graph is data");
    data["flow"] = serde_json::to_value(flow::of(&r.union)).expect("the flow is data");
    data["diff"] = serde_json::to_value(&r.d).expect("the diff is data");
    let file = std::env::temp_dir().join(format!("atlas-one-name-{}-{}.json", r.dir, std::process::id()));
    std::fs::write(&file, data.to_string()).expect("writes the data");
    let model = r#"
        const fs = require("node:fs");
        const [planJs, dataJson, by, ...ids] = process.argv.slice(1);
        const D = JSON.parse(fs.readFileSync(dataJson, "utf8"));
        const M = new Function("D", "window", fs.readFileSync(planJs, "utf8") + "\nreturn {typeLabel, stepName, V};")(D, {});
        // a function is drawn as a step of a route; outside one, the box of an entry says its route instead
        M.V.route = ids[0];
        process.stdout.write(JSON.stringify(ids.map(id => M[by](id))));
    "#;
    let out = Command::new("node")
        .arg("-e")
        .arg(model)
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../viewer/plan.js"))
        .arg(&file)
        .arg(by)
        .args(ids)
        .output()
        .expect("node runs the viewer's model: the test needs node on the PATH");
    let _ = std::fs::remove_file(&file);
    assert!(out.status.success(), "the model fails\n{}", String::from_utf8_lossy(&out.stderr));
    serde_json::from_slice(&out.stdout).expect("the model prints the names")
}

/// Whether `text` says `name` as one of the names it lists: after a space, and before a comma, a space or its end.
fn says(text: &str, name: &str) -> bool {
    text.match_indices(name)
        .any(|(i, _)| text[..i].ends_with(' ') && matches!(text[i + name.len()..].chars().next(), None | Some(',') | Some(' ')))
}

/// The arrows of reading A that the words of each changed type say (what takes it, gives it, builds it, changes it, holds it
/// or is held by it, its bases, its variants and its members): the words, and the id at the arrow's other end.
fn at_arrows<'r>(r: &'r Read) -> Vec<(&'r str, &'r str)> {
    let changed = r.c.routes.iter().flat_map(|x| &x.steps_changed).chain(&r.c.elsewhere.changed);
    let typed = |id: &str| r.a.node(id).map_or(false, |n| matches!(n.kind, NodeKind::Class | NodeKind::External));
    let mut said = vec![];
    for s in changed.filter(|s| typed(&s.id)) {
        for e in &r.a.edges {
            let into = e.dst == s.id
                && matches!(
                    e.kind,
                    EdgeKind::Returns
                        | EdgeKind::Instantiates
                        | EdgeKind::Takes
                        | EdgeKind::Mutates
                        | EdgeKind::Has
                        | EdgeKind::Inherits
                        | EdgeKind::Variant
                );
            let out = e.src == s.id && matches!(e.kind, EdgeKind::Has | EdgeKind::Inherits | EdgeKind::Variant | EdgeKind::Contains);
            if into || out {
                said.push((s.was.as_str(), if into { e.src.as_str() } else { e.dst.as_str() }));
            }
        }
    }
    said
}

/// Law: what `changes` says a changed type was names each type at its arrows by the name the view draws that type by.
#[test]
fn changes_names_a_type_as_the_view_draws_it() {
    let r = read("types");
    let typed = |id: &str| r.a.node(id).map_or(false, |n| matches!(n.kind, NodeKind::Class | NodeKind::External));
    let said: Vec<(&str, &str)> = at_arrows(&r).into_iter().filter(|(_, id)| typed(id)).collect();
    let ids: Vec<String> = said.iter().map(|(_, id)| id.to_string()).collect();
    let names = drawn(&r, "typeLabel", &ids);
    assert!(
        names.iter().any(|n| n.find('[').map_or(false, |i| n[i..].contains('.'))),
        "the program has a changed type at a shaped array whose shape holds a dot: {names:?}\n{}",
        r.report
    );
    let shared = |n: &str| r.union.nodes.iter().filter(|m| m.kind == NodeKind::Class && m.name == n).count() > 1;
    assert!(
        said.iter().any(|(_, id)| r.a.node(id).map_or(false, |n| shared(&n.name))),
        "the program has a changed type at a type whose name another type shares: {names:?}\n{}",
        r.report
    );
    for ((was, id), name) in said.iter().zip(&names) {
        assert!(
            says(was, name),
            "`changes` says what a type was without the name the view draws {id} by, `{name}`:\n{was}\n\n{}",
            r.report
        );
    }
}

/// Law: what `changes` says a changed type was names each function at its arrows by the name the view draws it by as a step:
/// a method with its class, an initializer as the call to its class.
#[test]
fn changes_names_a_function_at_a_types_arrows_as_the_view_draws_it() {
    let r = read("functions");
    let function = |id: &str| r.a.node(id).map_or(false, |n| matches!(n.kind, NodeKind::Function | NodeKind::Method));
    let said: Vec<(&str, &str)> = at_arrows(&r).into_iter().filter(|(_, id)| function(id)).collect();
    let ids: Vec<String> = said.iter().map(|(_, id)| id.to_string()).collect();
    let names = drawn(&r, "stepName", &ids);
    let kinds = |pick: &dyn Fn(&atlas_engine::model::Node) -> bool| said.iter().any(|(_, id)| r.a.node(id).map_or(false, |n| pick(n)));
    assert!(
        kinds(&|n| n.cls.is_some() && !n.ctor) && kinds(&|n| n.ctor && !n.on_class) && kinds(&|n| n.cls.is_none()),
        "the program has a method, an initializer and a function of its module at a changed type's arrows: {names:?}\n{}",
        r.report
    );
    for ((was, id), name) in said.iter().zip(&names) {
        assert!(
            says(was, name),
            "`changes` says what a type was without the name the view draws {id} by, `{name}`:\n{was}\n\n{}",
            r.report
        );
    }
}

/// Law: `changes` lists a node of the program's own whose id lies under no part's root (a sum written in place, whose id
/// is the set of its variants, no path) by the name the view draws it by.
#[test]
fn changes_lists_a_node_with_no_path_as_the_view_draws_it() {
    let r = read("listed");
    let own = |id: &str| r.union.node(id).map_or(false, |n| n.kind != NodeKind::External && !n.library);
    let under = |id: &str| r.c.roots.iter().any(|root| id.starts_with(&format!("{root}.")));
    let e = &r.c.elsewhere;
    let mut listed: Vec<(char, &str, bool)> = vec![];
    for x in &r.c.routes {
        listed.extend(x.steps_added.iter().map(|id| ('+', id.as_str(), true)));
        listed.extend(x.steps_removed.iter().map(|id| ('-', id.as_str(), true)));
        listed.extend(x.steps_changed.iter().map(|s| ('~', s.id.as_str(), true)));
    }
    listed.extend(e.added.iter().map(|id| ('+', id.as_str(), false)));
    listed.extend(e.removed.iter().map(|id| ('-', id.as_str(), false)));
    listed.extend(e.changed.iter().map(|s| ('~', s.id.as_str(), false)));
    let pathless: Vec<(char, &str, bool)> = listed.into_iter().filter(|(_, id, _)| own(id) && !under(id)).collect();
    assert!(!pathless.is_empty(), "the program lists a node of its own whose id lies under no root:\n{}", r.report);
    let ids: Vec<String> = pathless.iter().map(|(_, id, _)| id.to_string()).collect();
    let names = drawn(&r, "typeLabel", &ids);
    for ((sign, id, step), name) in pathless.iter().zip(&names) {
        let head = format!("    {sign} {}{name}", if *step { "step " } else { "" });
        let line = r.report.lines().any(|l| if *sign == '~' { l.starts_with(&format!("{head}: ")) } else { l == head });
        assert!(line, "`changes` lists {id} without the name the view draws it by, `{name}`:\n\n{}", r.report);
    }
}
