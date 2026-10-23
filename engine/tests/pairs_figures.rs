//! The figures pair (`tests/pairs/figures/python`): two series meant for one chart, drawn by a notebook. The clean
//! `Chart` is one figure: `create_figure` keeps what `plt.subplots()` makes in the chart it runs on, and `line` runs it
//! only where the chart holds none (`if self.ax is None`), so every line draws on the figure the chart holds. The
//! shortcut's `line` calls `plt.subplots()` itself, so every line makes a figure of its own. A figure rests in what holds
//! it (`docs/reference/mathematics.md`, "One fine plan, every picture its image"): the plans beside the programs write
//! down the notebook's route; these tests read what the plan is drawn from, and the figures the notebook's screen shows.

// the helpers the other pairs use for prophecies are none of these tests'
#[allow(dead_code)]
mod common;

use common::Pair;

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

use atlas_engine::{flow, parts, Graph, Options};

fn figures() -> &'static Pair {
    static PAIR: OnceLock<Pair> = OnceLock::new();
    PAIR.get_or_init(|| common::pair("figures/python"))
}

/// The operations of `id`'s figures, by the kind of place each touches.
fn figure_touches(g: &Graph, id: &str) -> Vec<String> {
    g.node(id)
        .unwrap_or_else(|| panic!("{id} is in the graph"))
        .touches
        .iter()
        .filter(|t| t.kind_name() == "figure")
        .map(|t| t.op.clone())
        .collect()
}

/// The clean chart keeps its figure in itself: `create_figure` makes it and changes the chart it runs on, and `line`
/// calls it only in the arm where the chart holds no axes. The shortcut's `line` makes a figure each time it runs and
/// changes nothing of the chart, which holds no figure.
#[test]
fn the_clean_chart_keeps_its_figure_and_the_shortcut_makes_one_per_line() {
    let (clean, shortcut) = (&figures().clean, &figures().shortcut);
    let arrows = flow::arrows(clean);
    assert_eq!(figure_touches(clean, "charts.chart.Chart.create_figure"), ["subplots"]);
    assert_eq!(arrows["charts.chart.Chart.create_figure"].changes, ["charts.chart.Chart"], "it keeps the figure in the chart");
    assert!(figure_touches(clean, "charts.chart.Chart.line").is_empty());
    let line = clean.node("charts.chart.Chart.line").expect("line is in the graph");
    let call = line.sites.iter().find(|s| s.to == "charts.chart.Chart.create_figure").expect("line calls create_figure");
    let when: Vec<&str> =
        call.arms.iter().filter_map(|(c, a)| line.whens.iter().find(|w| w.0 == *c && w.1 == *a)).map(|w| w.2.as_str()).collect();
    assert_eq!(when, ["self.ax is None"], "made only where the chart holds none");

    let arrows = flow::arrows(shortcut);
    assert_eq!(figure_touches(shortcut, "charts.chart.Chart.line"), ["subplots"]);
    assert!(arrows.get("charts.chart.Chart.line").map_or(true, |a| a.changes.is_empty()), "the shortcut's line keeps nothing");
}

/// The route says where the figure is made: in the clean chart, in an arm of `line`, only where the chart holds no
/// axes; in the shortcut, by `line` on every call.
#[test]
fn the_route_says_where_each_figure_is_made() {
    let clean = atlas_engine::story::route(&figures().clean, "charts.chart.compare", 4).expect("compare is a route");
    assert!(clean.contains("Chart.create_figure()  ↻ Chart   ·   draws   ·   when self.ax is None"), "{clean}");
    let shortcut = atlas_engine::story::route(&figures().shortcut, "charts.chart.compare", 4).expect("compare is a route");
    assert!(shortcut.contains("Chart.line(xs, ys)   ·   draws") && !shortcut.contains("create_figure"), "{shortcut}");
}

/// The figures on the screen of the notebook that draws the chart, as the viewer's model plans its route: `side` facts
/// of the places `Figure n · ..` (`tests/plans.mjs`, run in node as `tests/plans.rs` runs it). With a `budget`, the plan
/// is cut as a large program's is (`ATLAS_FINE_BUDGET`).
fn screen(side: &str, budget: Option<usize>) -> BTreeSet<String> {
    screen_of("figures/python", side, "notebooks.compare", budget)
}

/// The figures on the screen of a notebook's route of a side of the pair under `tests/pairs/{pair}`.
fn screen_of(pair: &str, side: &str, route: &str, budget: Option<usize>) -> BTreeSet<String> {
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let program = here.join("tests/pairs").join(pair).join(side);
    let graph = parts::analyze_repo(&[program], &Options::default()).expect("reads the program");
    let mut data = serde_json::to_value(&graph).expect("the graph is data");
    data["flow"] = serde_json::to_value(flow::of(&graph)).expect("the flow is data");
    let name = pair.replace('/', "-");
    let file = std::env::temp_dir().join(format!("atlas-figures-{}-{name}-{side}-{budget:?}.json", std::process::id()));
    std::fs::write(&file, data.to_string()).expect("writes the data");
    let mut node = Command::new("node");
    if let Some(b) = budget {
        node.env("ATLAS_FINE_BUDGET", b.to_string());
    }
    let out = node
        .arg(here.join("tests/plans.mjs"))
        .arg(here.join("../viewer/plan.js"))
        .arg(&file)
        .arg(route)
        .output()
        .expect("node runs the viewer's model: the test needs node on the PATH");
    let _ = std::fs::remove_file(&file);
    assert!(out.status.success(), "the model fails\n{}", String::from_utf8_lossy(&out.stderr));
    let read: serde_json::Value = serde_json::from_slice(&out.stdout).expect("the model prints JSON");
    read[route]["facts"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|f| f.as_str()?.strip_prefix("side Figure ").map(|s| format!("Figure {s}")))
        .collect()
}

/// Five functions on one figure are one figure, five plots five (#11): the clean notebook's screen shows the one figure
/// the chart holds, named by the chart, and the shortcut's a figure for each line, named by the call that made it.
#[test]
fn a_notebook_shows_a_figure_for_each_thing_that_holds_one() {
    assert_eq!(screen("clean", None), BTreeSet::from(["Figure 1 · Chart · chart right".to_string()]));
    assert_eq!(
        screen("shortcut", None),
        BTreeSet::from(["Figure 1 · Chart.line right".to_string(), "Figure 2 · Chart.line right".to_string()])
    );
}

/// Across the cut the figures are the same (#11): a step planned by itself does not know the value its caller hands it,
/// so a figure it keeps in that value is held by it, and the caller says which value that is. Cut at every depth, the
/// clean notebook still shows the chart's one figure and the shortcut a figure for each line.
#[test]
fn a_figure_is_held_by_the_same_value_beyond_the_cut() {
    for budget in [1, 2, 3, 5] {
        assert_eq!(screen("clean", Some(budget)), BTreeSet::from(["Figure 1 · Chart · chart right".to_string()]), "budget {budget}");
        assert_eq!(
            screen("shortcut", Some(budget)),
            BTreeSet::from(["Figure 1 · Chart.line right".to_string(), "Figure 2 · Chart.line right".to_string()]),
            "budget {budget}"
        );
    }
}

/// Building a class runs its initializer along its order, in the route's text as on the page: `Chart("compare")` opens
/// into `Chart.__init__`, which calls nothing, and never into the methods of the class, though `Chart` is a route of its
/// own (its doors). The first `line` is then the one that shows what it runs.
#[test]
fn building_a_class_opens_into_its_initializer_in_the_route_s_text() {
    let r = atlas_engine::story::route(&figures().clean, "charts.chart.compare", 4).expect("compare is a route");
    let lines: Vec<&str> = r.lines().collect();
    assert_eq!(
        lines[2..],
        [
            "1  Chart()",
            "2  Chart.line(xs, ys)  ↻ Chart",
            "  2.1a  Chart.create_figure()  ↻ Chart   ·   draws   ·   when self.ax is None",
            "3  Chart.line  → see 2"
        ],
        "{r}"
    );
    let doors = atlas_engine::story::route(&figures().clean, "charts.chart.Chart", 2).expect("Chart is a route");
    assert!(doors.contains("Chart.create_figure()") && doors.contains("Chart.line(xs, ys)"), "the class's own route is its doors\n{doors}");
}

/// The lent pair (`tests/pairs/figures/lent`): a figure handed between charts through a field (#11). The clean first chart
/// lends its figure by handing what it holds to the second's `take`, which sets its own fields; the shortcut's `lend`
/// reaches into the second chart and sets its fields there (`other.fig, other.ax = self.fig, self.ax`). Either way the
/// second chart holds the first's figure (`Node::sets`, a lens's put), so the notebook's screen shows one figure, also
/// beyond the cut; what tells them apart is who sets the second chart's fields.
#[test]
fn a_figure_lent_through_a_field_is_the_one_figure() {
    let one = BTreeSet::from(["Figure 1 · Chart · first right".to_string()]);
    for side in ["clean", "shortcut"] {
        for budget in [None, Some(1), Some(2), Some(3), Some(5)] {
            assert_eq!(screen_of("figures/lent", side, "notebooks.overlay", budget), one, "{side}, budget {budget:?}");
        }
    }
    let lent = common::pair("figures/lent");
    assert_eq!(sets(&lent.clean, "charts.chart.Chart.take"), [("", "fig", ""), ("", "ax", "")], "the second chart sets its own fields");
    assert!(sets(&lent.clean, "charts.chart.Chart.lend").is_empty(), "the first hands them to it");
    assert_eq!(
        sets(&lent.shortcut, "charts.chart.Chart.lend"),
        [("other", "fig", "fig"), ("other", "ax", "ax")],
        "the first sets the second's fields itself, each to its own field of the same name"
    );
}

/// What a body sets of `id`'s values, as `Node::sets` names it: the name that holds the value, the field set and the field
/// of a value it is set to.
fn sets<'g>(g: &'g Graph, id: &str) -> Vec<(&'g str, &'g str, &'g str)> {
    g.node(id).unwrap().sets.iter().map(|s| (s.on.name.as_str(), s.field.as_str(), s.of.as_str())).collect()
}

/// The titled pair (`tests/pairs/figures/titled`): two charts, the second titled after the first (#168). The clean first
/// chart hands its title to the second's `retitle`, which sets its own `title`; the shortcut's `lend_title` reaches into
/// the second chart and sets its title there (`other.title = self.title`). A figure rests in the fields that hold it: a
/// chart keeps its figure in `fig` and `ax`, which `create_figure` sets to what `plt.subplots()` makes, and the title is
/// none of them, so the second chart holds no figure of the first's and the screen shows two, also beyond the cut. A field
/// set is named, so a figure follows only the fields that hold it.
#[test]
fn a_field_that_holds_no_figure_lends_none() {
    let two = BTreeSet::from(["Figure 1 · Chart · first right".to_string(), "Figure 2 · Chart · second right".to_string()]);
    for side in ["clean", "shortcut"] {
        for budget in [None, Some(1), Some(2), Some(3), Some(5)] {
            assert_eq!(screen_of("figures/titled", side, "notebooks.side_by_side", budget), two, "{side}, budget {budget:?}");
        }
    }
    let titled = common::pair("figures/titled");
    assert_eq!(sets(&titled.clean, "charts.chart.Chart.create_figure"), [("", "fig", ""), ("", "ax", "")], "where the figure rests");
    assert_eq!(sets(&titled.clean, "charts.chart.Chart.retitle"), [("", "title", "")], "the second chart sets its own title");
    assert_eq!(sets(&titled.shortcut, "charts.chart.Chart.lend_title"), [("other", "title", "title")], "the first sets the second's");
}
