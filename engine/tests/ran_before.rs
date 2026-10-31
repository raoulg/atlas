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
//! methods that keep files on it, the `order` pair, where the route sets a second shelf up after it kept its report, and
//! two programs made for the order in a loop and in a nested arm (`plans/setup-in-a-loop`, `plans/setup-in-an-arm`).
//!
//! A function's arrow (`flow::arrows`) reads back what it does in the contexts it is called in, one of them the route
//! that starts at it. The second law: what a function does at places holds what that route, written out occurrence by
//! occurrence as `flow::along` reads it, does there, and is exactly that for a function nothing calls.

use atlas_engine::model::{EdgeKind, EntryKind, Graph, Node, NodeKind, Site, Touch};
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

/// The subjects: the pairs and programs where a step sets up the object a later call runs on.
const SUBJECTS: [&str; 6] = [
    "pairs/setup/python/clean",
    "pairs/setup/python/shortcut",
    "pairs/order/python/clean",
    "pairs/order/python/shortcut",
    "plans/setup-in-a-loop",
    "plans/setup-in-an-arm",
];

/// The engine and the page read every route of each subject alike, occurrence by occurrence, and a route of each opens
/// into steps, so that there is an order to read.
#[test]
fn the_engine_and_the_page_read_what_ran_before_alike() {
    let tests = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests");
    let subjects = SUBJECTS;
    let mut fails = vec![];
    for s in subjects {
        let (engine, page) = readings(&tests.join(s));
        assert!(engine.values().any(|r| r.len() > 1), "{s}: a route of the subject opens into steps");
        fails.extend(differences(&engine, &page).into_iter().map(|d| format!("{s}: {d}")));
    }
    assert!(fails.is_empty(), "the engine and the page read {} occurrences apart:\n{}", fails.len(), fails.join("\n"));
}

/// The route that starts at `route`, written out as a program of its occurrences as the route reads them (`flow::along`):
/// each occurrence a function of its own, named by its path, with the touches its body runs there, whole in place, and
/// each call it makes a call of the occurrence below it. No touch of it waits for a caller to make it whole, so what it
/// does at places is the composite of its bodies alone (the effect functor, `arrows.rs`).
fn written_out(g: &Graph, route: &str) -> Graph {
    let reading = flow::along(g, route);
    assert!(!reading.keys().any(|k| k.contains('|')), "{route}: a call of the subject runs one function");
    let (bodies, arrows) = (flow::bodies(g), flow::arrows(g));
    let mut nodes = vec![];
    let mut todo = vec![(route.to_string(), route.to_string(), String::new())];
    while let Some((at, f, on)) = todo.pop() {
        let n = g.node(&f).unwrap_or_else(|| panic!("{at}: runs {f}, a function of the graph"));
        let places = &reading[&at];
        // the touches the body runs there, by their places: in the subjects a body's touches at one place are made whole
        // alike, so the place says which run
        let all: Vec<&Touch> = n.touches.iter().chain(&n.instead).collect();
        for t in &all {
            let p = t.place().id;
            assert!(all.iter().filter(|u| u.place().id == p).all(|u| u.by == t.by), "{f}: its touches at {p} are made whole alike");
        }
        let touches =
            all.into_iter().filter(|t| places.contains(&t.place().id)).map(|t| Touch { by: Default::default(), ..t.clone() }).collect();
        // what each call runs: the step the body reads it as, and for a class built, the initializer it runs on it
        let steps: BTreeMap<usize, (String, String)> = bodies
            .get(&flow::key(&f, &on))
            .into_iter()
            .flat_map(|b| b.steps())
            .filter_map(|s| s.site.map(|i| (i, (s.to.clone(), s.on.clone()))))
            .collect();
        let sites = n
            .sites
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let below = format!("{at}/{i}");
                if !reading.contains_key(&below) {
                    return Site { to: String::new(), ..s.clone() };
                }
                let (to, on) = steps[&i].clone();
                let run = match arrows.get(&to).filter(|a| !a.runs.is_empty()) {
                    Some(class) => (class.runs.clone(), to),
                    None => (to, on),
                };
                todo.push((below.clone(), run.0, run.1));
                Site { to: below, ..s.clone() }
            })
            .collect();
        nodes.push(Node { id: at, kind: NodeKind::Function, touches, instead: vec![], sites, cls: None, ctor: false, ..n.clone() });
    }
    Graph { nodes, ..Default::default() }
}

/// What a function does at places as its arrow says: where it reads only what it wrote, and where it writes on what it
/// found.
fn at_places(g: &Graph, id: &str) -> (BTreeSet<String>, BTreeSet<String>) {
    let a = flow::arrows(g).remove(id).unwrap_or_default();
    (a.reads_back.into_iter().collect(), a.updates.into_iter().collect())
}

/// Composition is call order (CLAUDE.md, the third sentence): an arrow stands in the contexts its function is called in,
/// each a chain of callers above it with what ran before it there, and one of them is the chain that starts at it, its
/// route's own reading. A function it calls itself runs below it, in the order its body calls it, never above it. So
/// what a function does at places holds what its route, written out occurrence by occurrence (`written_out`), does there,
/// and for a function nothing calls, whose only context is its route's own, it is exactly that: a set-up the route runs
/// after a write counts for nothing that ran before it.
#[test]
fn an_arrow_reads_back_what_its_route_reads_back() {
    let tests = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests");
    let (mut fails, mut alone) = (vec![], 0);
    for s in SUBJECTS {
        let g = parts::analyze_repo(&[tests.join(s)], &Options::default()).expect("reads the program");
        let called = |f: &str| g.edges.iter().any(|e| e.dst == f && matches!(e.kind, EdgeKind::Calls | EdgeKind::Instantiates));
        for n in g.nodes.iter().filter(|n| matches!(n.kind, NodeKind::Function | NodeKind::Method)) {
            let (arrow, route) = (at_places(&g, &n.id), at_places(&written_out(&g, &n.id), &n.id));
            if called(&n.id) {
                if !(route.0.is_subset(&arrow.0) && route.1.is_subset(&arrow.1)) {
                    fails.push(format!("{s}: {}: the arrow {arrow:?} lacks what its route does, {route:?}", n.id));
                }
            } else {
                alone += 1;
                if arrow != route {
                    fails.push(format!("{s}: {}: the arrow {arrow:?}, its route {route:?}", n.id));
                }
            }
        }
    }
    assert!(alone > 0, "a subject has a function nothing calls");
    assert!(fails.is_empty(), "{} arrows say otherwise than their routes:\n{}", fails.len(), fails.join("\n"));
}
