//! A table a step writes and then reads back. On the clean side the read is a check: the count
//! goes nowhere, and the table is where the route leaves its notes, an end point. On the shortcut
//! side what is read back is carried on, so the table is where one step leaves them for the next.
//! The viewer places a place from what the data does to it; both sides read and write the table.

use atlas_engine::{analyze, flow, Options};
use std::path::PathBuf;

fn graph(side: &str) -> atlas_engine::Graph {
    analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/readback").join(side), &Options::default()).unwrap()
}

/// The places `ledger.Ledger.keep` goes to in the table `notes`, with how the graph says each is gone to.
fn notes_places(side: &str) -> Vec<String> {
    let g = graph(side);
    let drawn = flow::of(&g);
    drawn.drawn["ledger.Ledger.keep"]
        .iter()
        .filter(|p| p.place.id.contains("notes"))
        .map(|p| format!("{} {:?}", p.place.id, p.side))
        .collect()
}

/// Both sides write the table and read it back: the graph holds the same two touches and places on each
/// side, whatever is done with what was read. That is the gap: no fact of the graph tells the sides apart.
#[test]
fn both_sides_write_the_table_and_read_it_back() {
    for side in ["clean", "shortcut"] {
        let g = graph(side);
        let keep = g.node("ledger.Ledger.keep").unwrap_or_else(|| panic!("{side}: keep is in the graph"));
        let kinds: Vec<_> = keep.touches.iter().map(|t| t.kind_name()).collect();
        assert!(kinds.iter().any(|k| k.contains("db")), "{side}: {kinds:?}");
        let places = notes_places(side);
        assert!(places.iter().any(|p| p.ends_with("In")) && places.iter().any(|p| p.ends_with("Out")), "{side}: {places:?}");
    }
}

/// The two sides are told apart: on the clean side the table is an end point (a place the route only
/// leaves its notes in), on the shortcut side it stands between the steps.
/// Ignored: the graph does not say whether what a step reads from a place reaches its result, so the
/// viewer takes any other output of the step as use (#55).
#[test]
#[ignore = "Atlas takes a read that is only a check as use when the step hands anything else on (#55)"]
fn a_read_that_is_only_a_check_leaves_the_table_an_end_point() {
    let clean = notes_places("clean");
    let shortcut = notes_places("shortcut");
    assert_ne!(clean, shortcut, "clean {clean:?} and shortcut {shortcut:?} are drawn alike");
}
