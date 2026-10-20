//! A table a step writes and then reads back, in Python, Rust and TypeScript. On the clean side the
//! read is a check: the count decides whether the step fails, and goes no further, so the table is
//! where the route leaves its notes, an end point. On the shortcut side what is read back is
//! returned, so the table is where one step leaves them for the next. Both sides read and write the
//! table; what tells them apart is where what is read goes, a fact on the read (`Touch::checks`) the
//! viewer places the table by.

use atlas_engine::{analyze, flow, Graph, Options, Touch};
use std::path::PathBuf;

const LANGUAGES: [&str; 3] = ["python", "rust", "typescript"];

fn graph(language: &str, side: &str) -> Graph {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/readback").join(language).join(side);
    analyze(&dir, &Options::default()).unwrap()
}

/// The function that keeps the notes, `Ledger.keep`, whatever its module is called.
fn keep_id(g: &Graph) -> String {
    g.nodes.iter().find(|n| n.id.ends_with("Ledger.keep")).map(|n| n.id.clone()).expect("Ledger.keep is in the graph")
}

/// What `Ledger.keep` does at the table `notes`.
fn notes_touches(g: &Graph) -> Vec<&Touch> {
    g.node(&keep_id(g)).unwrap().touches.iter().filter(|t| t.artifact == "notes").collect()
}

/// Both sides write the table and read it back, and the flow draws the table on both sides of the
/// step on each: what is done with what was read is not where the touches differ.
#[test]
fn both_sides_write_the_table_and_read_it_back() {
    for language in LANGUAGES {
        for side in ["clean", "shortcut"] {
            let g = graph(language, side);
            let dirs: Vec<_> = notes_touches(&g).iter().map(|t| t.dir_name()).collect();
            assert!(dirs.contains(&"read") && dirs.contains(&"write"), "{language} {side}: {dirs:?}");
            let drawn = flow::of(&g);
            let sides: Vec<String> =
                drawn.drawn[&keep_id(&g)].iter().filter(|p| p.place.id.contains("notes")).map(|p| format!("{:?}", p.side)).collect();
            assert!(sides.iter().any(|s| s == "In") && sides.iter().any(|s| s == "Out"), "{language} {side}: {sides:?}");
        }
    }
}

/// The two sides are told apart by where what is read goes: on the clean side the read only decides
/// whether the step fails, a check; on the shortcut side it is what the step returns.
#[test]
fn a_read_that_is_only_a_check_is_no_use() {
    for language in LANGUAGES {
        let reads = |side: &str| -> Vec<bool> {
            notes_touches(&graph(language, side)).iter().filter(|t| t.dir_name() == "read").map(|t| t.checks).collect()
        };
        assert_eq!(reads("clean"), [true], "{language}: the count read back is a check");
        assert_eq!(reads("shortcut"), [false], "{language}: the notes read back are handed on");
    }
}
