//! A function's own facts are a function of its body and of what settles them, never of the order in which the bodies
//! beside it are read (#341). Two readings that differ only in what is read besides a function give that function the
//! same facts, the names of the values no name holds among them (`#0`: one call's result handed straight to the call
//! around it, `size(parse(text))`, named by the step that makes it). A function is changed when its own arrow differs
//! (`docs/reference/mathematics.md`, "Yoneda: what changed"), and what is read beside it is no part of that arrow. Each
//! law reads one program twice in one directory and compares the functions both readings hold as the graph's JSON
//! writes them.
//!
//! The program is the package `shop`, with the library `tally` where its environment keeps it (`site-packages`, as
//! `tests/fixture-lib`): `shop.orders` makes values no name holds and calls into `tally.count`, which calls into
//! `tally.rows`, a module only the library calls.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use atlas_engine::{analyze, Graph, Options};

const PROJECT: &str =
    "[project]\nname = \"shop\"\nversion = \"0.1.0\"\nrequires-python = \">=3.12\"\n\n[tool.ty.environment]\nextra-paths = [\"site-packages\"]\n";

const ORDERS: &str = r#"from pathlib import Path

from tally.count import standard, total


class Order:
    def __init__(self, text: str) -> None:
        self.text = text


def parse(text: str) -> Order:
    return Order(text.strip())


def size(order: Order) -> int:
    return len(order.text)


def report(path: Path) -> None:
    print(size(parse(path.read_text())))


def tallied(path: Path) -> int:
    standard()
    return total(path)
"#;

const COUNT: &str = r#"from pathlib import Path

from tally.rows import clean, split


def total(path: Path) -> int:
    return len(split(clean(path.read_text())))


class Pipeline:
    def __init__(self) -> None:
        self.steps: list[str] = []

    def add(self, step: str) -> "Pipeline":
        self.steps.append(step)
        return self


def standard() -> Pipeline:
    return Pipeline().add("double").add("halve")
"#;

const ROWS: &str = r#"def clean(text: str) -> str:
    return text.strip()


def split(text: str) -> list[str]:
    return [line for line in text.splitlines() if line]
"#;

/// Functions the program does without, each making values no name holds, for a module read before `shop.orders` and
/// one read after it, whichever order the reader takes.
const AUDIT: (&str, &str) = (
    "def audit(text: str) -> str:\n    return text.upper()\n",
    "\n\ndef shout(text: str) -> int:\n    return len(audit(audit(text.strip())))\n",
);
const ZONES: (&str, &str) = (
    "def zone(text: str) -> str:\n    return text.lower()\n",
    "\n\ndef whisper(text: str) -> int:\n    return len(zone(zone(text.strip())))\n",
);

/// The program `shop`, written to a directory of its own named after `name`, which the caller removes.
fn shop(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("atlas-reading-order-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let files = [
        ("pyproject.toml", PROJECT),
        ("src/shop/__init__.py", ""),
        ("src/shop/orders.py", ORDERS),
        ("src/shop/audit.py", AUDIT.0),
        ("src/shop/zones.py", ZONES.0),
        ("site-packages/tally/__init__.py", ""),
        ("site-packages/tally/count.py", COUNT),
        ("site-packages/tally/rows.py", ROWS),
    ];
    for (file, text) in files {
        write(&dir.join(file), text);
    }
    dir
}

fn write(file: &Path, text: &str) {
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
}

fn read(dir: &Path, libraries: usize) -> Graph {
    analyze(dir, &Options { libraries, ..Options::default() }).expect("the program parses")
}

/// The facts of every node under `module` that `g` holds, by id, as the graph's JSON writes them.
fn facts(g: &Graph, module: &str) -> BTreeMap<String, serde_json::Value> {
    g.nodes
        .iter()
        .filter(|n| n.id.strip_prefix(module).is_some_and(|rest| rest.starts_with('.')))
        .map(|n| (n.id.clone(), serde_json::to_value(n).unwrap()))
        .collect()
}

/// How many names of values no name holds a node's facts write (`#0`), so that a law is never asked of functions
/// without one.
fn unnamed(fact: &serde_json::Value) -> usize {
    match fact {
        serde_json::Value::Array(xs) => xs.iter().map(unnamed).sum(),
        serde_json::Value::Object(m) => {
            let own = m.get("name").and_then(|n| n.as_str()).is_some_and(|n| n.starts_with('#'));
            usize::from(own) + m.values().map(unnamed).sum::<usize>()
        }
        _ => 0,
    }
}

/// A function added to two other modules of the program, one read before `shop.orders` and one after it, changes no
/// fact of a function of `shop.orders`: not the names of the values its steps hand on by no name.
#[test]
fn a_function_added_to_another_module_renames_no_value() {
    let dir = shop("added");
    let before = read(&dir, 1);
    for (file, (_, more)) in [("src/shop/audit.py", AUDIT), ("src/shop/zones.py", ZONES)] {
        let file = dir.join(file);
        let text = std::fs::read_to_string(&file).unwrap();
        write(&file, &format!("{text}{more}"));
    }
    let after = read(&dir, 1);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(before.node("shop.audit.shout").is_none() && after.node("shop.audit.shout").is_some(), "the second reading reads more");
    let (was, is) = (facts(&before, "shop.orders"), facts(&after, "shop.orders"));
    assert!(was.values().map(unnamed).sum::<usize>() > 0, "shop.orders makes values no name holds: {was:#?}");
    for (id, fact) in &was {
        assert_eq!(Some(fact), is.get(id), "{id} is the same function in both readings");
    }
}

/// Reading a library one round deeper (`tally.rows`, which only the library calls) changes no fact of a function both
/// readings keep and whose body that round does not settle: every one but `tally.count.total`, whose calls go into
/// `tally.rows`. Among them is `standard`, read after `total` and handing on values no name holds.
#[test]
fn a_library_read_deeper_renames_no_value_of_a_function_it_keeps() {
    let dir = shop("deeper");
    let (shallow, deep) = (read(&dir, 1), read(&dir, 2));
    let _ = std::fs::remove_dir_all(&dir);
    assert!(shallow.node("tally.rows.clean").is_none() && deep.node("tally.rows.clean").is_some(), "the second reading reads more");
    let kept = |g: &Graph| {
        let mut out = facts(g, "shop.orders");
        out.extend(facts(g, "tally.count").into_iter().filter(|(id, _)| id != "tally.count.total"));
        out
    };
    let (was, is) = (kept(&shallow), kept(&deep));
    let standard = was.get("tally.count.standard").expect("the program keeps standard");
    assert!(unnamed(standard) > 0, "standard makes values no name holds: {standard:#}");
    for (id, fact) in &was {
        assert_eq!(Some(fact), is.get(id), "{id} is the same function in both readings");
    }
}
