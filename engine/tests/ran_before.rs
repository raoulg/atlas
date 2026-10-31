//! The engine and the page read a route in one order (docs/reference/mathematics.md, "Who makes a path whole"). Under a
//! step, what ran before it is the prefix of the route's composite: the calls that lead there and, in each of their
//! bodies, the calls that ran before the one that leads on, not one in another arm of a choice, and in a loop any call of
//! it. A touch whose path callers make whole runs at their place where every condition of one of its conjunctions holds
//! under the step: on the calls, a constructor of a class a method on them runs on, or a set-up that ran before.
//!
//! The engine reads the route so in `flow::orders`, which works out what a function reads back and changes, and says it
//! occurrence by occurrence (`flow::along`); the page's model reads it so in `viewer/plan.js` (`routeTree`'s `before`,
//! `inRoute`) and says it of its finest plan (`ran_before.mjs`, in node). The law: for every route that starts at a
//! function, the two readings name the same occurrences, and under each the same places. (The route of a class is its
//! doors, each called by the entry with no order between them; `orders` reads each door as a function of its own, never
//! the class as one body.) Its subjects are the `setup` pair, where the step that sets the shelf up runs beside the
//! methods that keep files on it, and two programs made for the order in a loop and in a nested arm
//! (`plans/setup-in-a-loop`, `plans/setup-in-an-arm`).

use atlas_engine::model::{EntryKind, NodeKind};
use atlas_engine::{flow, parts, Options};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The reading of each route, by the route: each occurrence by its key, with the places its own body runs.
type Reading = BTreeMap<String, BTreeMap<String, BTreeSet<String>>>;

/// The engine's reading of every route of a program that starts at a function, and the page's, from the same graph.
fn readings(program: &Path) -> (Reading, Reading) {
    let graph = parts::analyze_repo(&[program.to_path_buf()], &Options::default()).expect("reads the program");
    let routes: Vec<String> = graph
        .nodes
        .iter()
        .filter(|n| n.kind != NodeKind::Class && n.entry.as_ref().is_some_and(|e| e.kind != EntryKind::Test))
        .map(|n| n.id.clone())
        .collect();
    let engine: Reading = routes.iter().map(|r| (r.clone(), flow::along(&graph, r))).collect();
    let mut data = serde_json::to_value(&graph).expect("the graph is data");
    data["flow"] = serde_json::to_value(flow::of(&graph)).expect("the flow is data");
    let name = program.to_string_lossy().replace(['/', '\\', ':'], "_");
    let file = std::env::temp_dir().join(format!("atlas-ran-before-{}-{name}.json", std::process::id()));
    std::fs::write(&file, data.to_string()).expect("writes the data");
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out = Command::new("node")
        .arg(here.join("tests/ran_before.mjs"))
        .arg(here.join("../viewer/plan.js"))
        .arg(&file)
        .output()
        .expect("node runs the viewer's model: the test needs node on the PATH");
    let _ = std::fs::remove_file(&file);
    assert!(out.status.success(), "{}: the model fails\n{}", program.display(), String::from_utf8_lossy(&out.stderr));
    let mut page: Reading = serde_json::from_slice(&out.stdout).expect("the model prints JSON");
    page.retain(|r, _| routes.contains(r));
    (engine, page)
}

/// Where the two readings of one program differ, each difference in words.
fn differences(engine: &Reading, page: &Reading) -> Vec<String> {
    let mut out = vec![];
    let routes: BTreeSet<&String> = engine.keys().chain(page.keys()).collect();
    let none = BTreeMap::new();
    for r in routes {
        let (e, p) = (engine.get(r).unwrap_or(&none), page.get(r).unwrap_or(&none));
        let keys: BTreeSet<&String> = e.keys().chain(p.keys()).collect();
        for k in keys {
            match (e.get(k), p.get(k)) {
                (Some(a), Some(b)) if a != b => out.push(format!("{k}: the engine runs {a:?}, the page {b:?}")),
                (Some(_), None) => out.push(format!("{k}: an occurrence of the engine's the page does not open")),
                (None, Some(_)) => out.push(format!("{k}: an occurrence of the page's the engine does not read")),
                _ => {}
            }
        }
    }
    out
}

/// The engine and the page read every route of each subject alike, occurrence by occurrence, and a route of each opens
/// into steps, so that there is an order to read.
#[test]
fn the_engine_and_the_page_read_what_ran_before_alike() {
    let tests = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests");
    let subjects = ["pairs/setup/python/clean", "pairs/setup/python/shortcut", "plans/setup-in-a-loop", "plans/setup-in-an-arm"];
    let mut fails = vec![];
    for s in subjects {
        let (engine, page) = readings(&tests.join(s));
        assert!(engine.values().any(|r| r.len() > 1), "{s}: a route of the subject opens into steps");
        fails.extend(differences(&engine, &page).into_iter().map(|d| format!("{s}: {d}")));
    }
    assert!(fails.is_empty(), "the engine and the page read {} occurrences apart:\n{}", fails.len(), fails.join("\n"));
}
