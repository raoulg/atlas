//! The agent's text and the human's page are one reading of a route (CLAUDE.md, "The goal"): under each step the route
//! as text (`story::route`) names the places the page draws there, as the route runs in order (docs/reference/
//! mathematics.md, "Who makes a path whole"). The text tells its steps as `story::told_steps`: each the occurrence it is,
//! named as `flow::along` and the page name it, with the places its own body touches there, or the step it sends the
//! reader to (`→ see`). The page's model reads the same graph (`viewer/plan.js`, through `ran_before.mjs`, as
//! `engine/tests/ran_before.rs` runs it). The law: for every route, every step the text tells touches the places the
//! page draws at that occurrence; a step the text sends on touches there what the step it names touches, and one the
//! page leaves closed, a call of a function the route is already inside, is sent to a step on the calls that lead to it.
//! Its subjects are the `setup` pair, where the step that sets the shelf up runs beside the methods that keep files on
//! it, two programs made for the order in a loop and in a nested arm (`plans/setup-in-a-loop`,
//! `plans/setup-in-an-arm`), the `collections` pair, whose class is a route of doors that read the path its constructor
//! holds, and the `cooperative` pair, whose call may run an override in place of the method it reaches.

use atlas_engine::model::EntryKind;
use atlas_engine::{flow, parts, story, Graph, Options};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The page's reading of each route: each occurrence by its key, with the places its own body runs.
type Reading = BTreeMap<String, BTreeMap<String, BTreeSet<String>>>;

/// How deep the text opens each route: past the deepest call of every subject.
const DEPTH: usize = 12;

/// The program's graph and the page's reading of every route of it but its tests.
fn page(program: &Path) -> (Graph, Reading) {
    let graph = parts::analyze_repo(&[program.to_path_buf()], &Options::default()).expect("reads the program");
    let mut data = serde_json::to_value(&graph).expect("the graph is data");
    data["flow"] = serde_json::to_value(flow::of(&graph)).expect("the flow is data");
    let name = program.to_string_lossy().replace(['/', '\\', ':'], "_");
    let file = std::env::temp_dir().join(format!("atlas-route-text-{}-{name}.json", std::process::id()));
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
    (graph, serde_json::from_slice(&out.stdout).expect("the model prints JSON"))
}

/// Where the text of one route and the page read it apart, each difference in words.
fn differences(route: &str, told: &[story::Told], page: &BTreeMap<String, BTreeSet<String>>) -> Vec<String> {
    let none = BTreeSet::new();
    let mut out = vec![];
    for t in told {
        let drawn = page.get(&t.at);
        let says = match &t.see {
            None => &t.places,
            // the step of that label that runs the same function: the steps of one branch share its label
            Some(label) => match told.iter().find(|s| s.label == *label && s.runs == t.runs && s.see.is_none()) {
                Some(s) if drawn.is_some() => &s.places,
                // a call the page leaves closed stands in a step on the calls that lead to it
                Some(s) if t.at.starts_with(&s.at) => continue,
                Some(s) => {
                    out.push(format!("{route} {}: sent to {label}, {}, which is not on the calls to {}", t.label, s.at, t.at));
                    continue;
                }
                None => {
                    out.push(format!("{route} {}: sent to {label}, a step the text does not tell", t.label));
                    continue;
                }
            },
        };
        let drawn = drawn.unwrap_or(&none);
        if says != drawn {
            out.push(format!("{route} {} ({}): the text says {says:?}, the page draws {drawn:?}", t.label, t.at));
        }
    }
    out
}

/// Every route of each subject, but its tests, tells under each step the places the page draws there, and a step of
/// each subject touches a place a step before it or a caller above it makes whole, so that there is an order to read.
#[test]
fn the_route_text_names_the_places_the_page_draws() {
    let tests = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests");
    let subjects = [
        "pairs/setup/python/clean",
        "pairs/setup/python/shortcut",
        "plans/setup-in-a-loop",
        "plans/setup-in-an-arm",
        "pairs/collections/python/clean",
        "pairs/cooperative/python/clean",
    ];
    let mut fails = vec![];
    for s in subjects {
        let (graph, page) = page(&tests.join(s));
        let mut said = 0;
        for (kind, _, route) in story::entries(&graph) {
            if kind == EntryKind::Test {
                continue;
            }
            let told = story::told_steps(&graph, route, DEPTH).expect("an entry is a route");
            said += told.iter().filter(|t| !t.places.is_empty()).count();
            let none = BTreeMap::new();
            fails.extend(differences(route, &told, page.get(route).unwrap_or(&none)).into_iter().map(|d| format!("{s}: {d}")));
        }
        assert!(said > 0, "{s}: a step of the subject touches a place");
    }
    assert!(fails.is_empty(), "the route text and the page read {} steps apart:\n{}", fails.len(), fails.join("\n"));
}
