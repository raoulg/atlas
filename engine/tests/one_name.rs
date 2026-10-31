//! A construct has one name wherever Atlas says it. `atlas changes` says what a changed type was by the types at its
//! arrows, and says each by the name the view draws it by: the page's own model (`viewer/plan.js`, run in node on the
//! page's data) gives the view's name, the reading `changes` lists gives the other. The program is a Python package
//! whose types are shaped arrays with a dot in their shape (`{self.size}`, `...`), where a name cut at the last dot of
//! an id is no name the view draws.

use std::path::{Path, PathBuf};
use std::process::Command;

use atlas_engine::changes;
use atlas_engine::diff::{self, Reading};
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
const TORCH: &str = "class Tensor:\n    def __mul__(self, other: Tensor) -> Tensor: ...\n";

const BLOCKS: &str = r#""""Blocks that scale rows of the size each block names."""

from jaxtyping import Float
from torch import Tensor

# rows of the block's size, in one batch or under any leading dimensions
Rows = Float[Tensor, "batch {self.size}"] | Float[Tensor, "... {self.size}"]


class Block:
    def __init__(self, size: int) -> None:
        self.size = size

    def scaled(self, x: Rows, by: Tensor) -> Rows:
        return x * by
"#;

/// The block with a second method on its rows: `Rows` gains arrows, so `changes` says what it was.
const DOUBLED: &str = r#"
    def doubled(self, x: Rows, by: Tensor) -> Rows:
        return self.scaled(x, by) * by
"#;

/// The package `blocks` in a repository of its own, committed once as `BLOCKS` and once with `DOUBLED` added.
fn repository() -> PathBuf {
    let repo = std::env::temp_dir().join(format!("atlas-one-name-{}", std::process::id())).join("blocks");
    let _ = std::fs::remove_dir_all(&repo);
    for (file, text) in [
        ("pyproject.toml", PYPROJECT),
        ("typings/jaxtyping/__init__.pyi", JAXTYPING),
        ("typings/torch/__init__.pyi", TORCH),
        ("src/blocks/__init__.py", BLOCKS),
    ] {
        std::fs::create_dir_all(repo.join(file).parent().unwrap()).unwrap();
        std::fs::write(repo.join(file), text).unwrap();
    }
    git(&repo, &["init", "-q"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "rows"]);
    std::fs::write(repo.join("src/blocks/__init__.py"), format!("{BLOCKS}{DOUBLED}")).unwrap();
    git(&repo, &["commit", "-q", "-am", "doubled"]);
    repo
}

/// The names the view draws the types `ids` by: `typeLabel` of the page's model, run in node on the page's data of the
/// union (its graph, its flow and how its things stand to the two readings).
fn drawn(g: &Graph, d: &diff::Diff, ids: &[String]) -> Vec<String> {
    let mut data = serde_json::to_value(g).expect("the graph is data");
    data["flow"] = serde_json::to_value(flow::of(g)).expect("the flow is data");
    data["diff"] = serde_json::to_value(d).expect("the diff is data");
    let file = std::env::temp_dir().join(format!("atlas-one-name-{}.json", std::process::id()));
    std::fs::write(&file, data.to_string()).expect("writes the data");
    let model = r#"
        const fs = require("node:fs");
        const [planJs, dataJson, ...ids] = process.argv.slice(1);
        const D = JSON.parse(fs.readFileSync(dataJson, "utf8"));
        const M = new Function("D", "window", fs.readFileSync(planJs, "utf8") + "\nreturn {typeLabel};")(D, {});
        process.stdout.write(JSON.stringify(ids.map(id => M.typeLabel(id))));
    "#;
    let out = Command::new("node")
        .arg("-e")
        .arg(model)
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../viewer/plan.js"))
        .arg(&file)
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

/// Law: what `changes` says a changed type was names each type at its arrows by the name the view draws that type by.
/// The arrows are those of reading A that the words of a type say (what takes it, gives it, builds it, changes it, holds
/// it or is held by it, its bases, its variants and its members), each whose other end is a type.
#[test]
fn changes_names_a_type_as_the_view_draws_it() {
    let repo = repository();
    let opts = Options::default();
    let before = changes::parts_at(&repo, Some("HEAD~1"), &opts).expect("the first commit reads");
    let after = changes::parts_at(&repo, Some("HEAD"), &opts).expect("the second commit reads");
    let c = changes::between("HEAD~1", Some("HEAD"), &before, &after);
    let (g, d) = diff::union("blocks", before, after);
    let a = diff::reading(&g, &d, Reading::A);
    let report = changes::report(&c);
    let changed: Vec<&changes::StepChange> = c.routes.iter().flat_map(|r| &r.steps_changed).chain(&c.elsewhere.changed).collect();
    let typed = |id: &str| a.node(id).map_or(false, |n| matches!(n.kind, NodeKind::Class | NodeKind::External));
    let mut said: Vec<(&str, String)> = vec![];
    for s in changed.iter().filter(|s| typed(&s.id)) {
        for e in &a.edges {
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
            let other = if into { &e.src } else { &e.dst };
            if (into || out) && typed(other) {
                said.push((&s.was, other.clone()));
            }
        }
    }
    let ids: Vec<String> = said.iter().map(|(_, id)| id.clone()).collect();
    let names = drawn(&g, &d, &ids);
    assert!(
        names.iter().any(|n| n.find('[').map_or(false, |i| n[i..].contains('.'))),
        "the program has a changed type at a shaped array whose shape holds a dot: {names:?}\n{report}"
    );
    for ((was, id), name) in said.iter().zip(&names) {
        assert!(says(was, name), "`changes` says what a type was without the name the view draws {id} by, `{name}`:\n{was}\n\n{report}");
    }
    let _ = std::fs::remove_dir_all(repo.parent().unwrap());
}
