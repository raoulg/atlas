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

use atlas_engine::{flow, parts, story, Graph, NodeKind, Options};

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

/// The steps of a route that draw a figure, through the bodies its text and its picture are drawn from (`flow::bodies`),
/// each with what decides it where it is an arm of a choice. Building a class runs its initializer.
fn drawn_by(g: &Graph, entry: &str) -> BTreeSet<(String, String)> {
    let (bodies, inits, drawn) = (flow::bodies(g), flow::initializers(g), flow::of(g).drawn);
    let draws = |id: &str| drawn.get(id).is_some_and(|ws| ws.iter().any(|w| w.place.glyph == "screen" && w.side == flow::Side::Out));
    let (mut out, mut seen, mut todo) = (BTreeSet::new(), BTreeSet::new(), vec![(flow::Step::of(entry), String::new())]);
    while let Some((st, when)) = todo.pop() {
        let built = g.node(&st.to).is_some_and(|n| n.kind == NodeKind::Class);
        let Some(to) = (if built { inits.get(&st.to).cloned() } else { Some(st.to.clone()) }) else { continue };
        if draws(&to) {
            out.insert((to.clone(), when));
        }
        let key = flow::key(&to, &st.on);
        let Some(body) = bodies.get(&key).filter(|_| seen.insert(key.clone())) else { continue };
        for item in &body.items {
            match item {
                flow::Item::Step(s) => todo.push((s.clone(), String::new())),
                flow::Item::Choice { members, when, .. } => {
                    todo.extend(members.iter().map(|m| (m.step.clone(), when.get(m.branch).cloned().unwrap_or_default())))
                }
            }
        }
    }
    out
}

/// The route says where the figure is made: in the clean chart, in an arm of `line`, only where the chart holds no
/// axes; in the shortcut, by `line` on every call.
#[test]
fn the_route_says_where_each_figure_is_made() {
    let at = |id: &str, when: &str| BTreeSet::from([(id.to_string(), when.to_string())]);
    assert_eq!(drawn_by(&figures().clean, "charts.chart.compare"), at("charts.chart.Chart.create_figure", "self.ax is None"));
    assert_eq!(drawn_by(&figures().shortcut, "charts.chart.compare"), at("charts.chart.Chart.line", ""));
}

/// The figures on the screen of the notebook that draws the chart, as the viewer's model plans its route: `side` facts
/// of the places `Figure n · ..` (`tests/plans.mjs`, run in node as `tests/plans.rs` runs it). With a `budget`, the plan
/// is cut as a large program's is (`ATLAS_FINE_BUDGET`).
fn screen(side: &str, budget: Option<usize>) -> BTreeSet<String> {
    screen_of("figures/python", side, "notebooks.compare", budget)
}

/// The figures on the screen of a notebook's route of a side of the pair under `tests/pairs/{pair}`.
fn screen_of(pair: &str, side: &str, route: &str, budget: Option<usize>) -> BTreeSet<String> {
    screen_at(&format!("pairs/{pair}/{side}"), route, budget)
}

/// The figures on the screen of a notebook's route of the program at `tests/{program}`.
fn screen_at(program: &str, route: &str, budget: Option<usize>) -> BTreeSet<String> {
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let graph = parts::analyze_repo(&[here.join("tests").join(program)], &Options::default()).expect("reads the program");
    let mut data = serde_json::to_value(&graph).expect("the graph is data");
    data["flow"] = serde_json::to_value(flow::of(&graph)).expect("the flow is data");
    let name = program.replace('/', "-");
    // a file of this call's own: tests run side by side, and two may read one program at one budget
    static CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let call = CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let file = std::env::temp_dir().join(format!("atlas-figures-{}-{call}-{name}-{budget:?}.json", std::process::id()));
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

/// A figure on a notebook's screen: the names the code gives what holds it (the class of the value and the path to it in
/// the notebook, or the step that made it), and the side of the route it stands on.
type Figure = (Vec<String>, String);

/// The figures of a screen, each by the names its place carries and its side; how the page numbers and spells its places
/// is the page's.
fn figures_on(screen: &BTreeSet<String>) -> Vec<Figure> {
    let name = |w: &&str| w.starts_with(|c: char| c.is_alphabetic() || c == '_');
    let mut out: Vec<Figure> = screen
        .iter()
        .map(|f| {
            let (place, side) = f.rsplit_once(' ').expect("a side fact ends in its side");
            let names = place.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.')).filter(name).skip(1);
            (names.map(String::from).collect(), side.to_string())
        })
        .collect();
    out.sort();
    out
}

/// Figures on the right of a route, each by the names of what holds it.
fn right<S: AsRef<str>>(held: &[impl AsRef<[S]>]) -> Vec<Figure> {
    let names = |h: &[S]| h.iter().map(|n| n.as_ref().to_string()).collect();
    let mut out: Vec<Figure> = held.iter().map(|h| (names(h.as_ref()), "right".to_string())).collect();
    out.sort();
    out
}

/// The budgets a route is cut at, as a large program's plan is cut.
const BUDGETS: [usize; 4] = [1, 2, 3, 5];

/// The screen of a route planned uncut, held to be the screen of the same route cut at every budget: a step planned by
/// itself beyond the cut is the image of its body, and the caller says which value it runs on (`docs/reference/
/// mathematics.md`, "One fine plan, every picture its image").
fn beyond_every_cut(what: &str, screen: impl Fn(Option<usize>) -> BTreeSet<String>) -> BTreeSet<String> {
    let fine = screen(None);
    for b in BUDGETS {
        assert_eq!(screen(Some(b)), fine, "{what}: cut at {b} as uncut");
    }
    fine
}

/// The screen of a notebook's route of both sides of a pair, held to be one screen at every budget: the sides differ in
/// who does the work, not in the figures it makes.
fn one_screen(pair: &str, route: &str) -> BTreeSet<String> {
    let [clean, shortcut] = ["clean", "shortcut"].map(|side| beyond_every_cut(side, |b| screen_of(pair, side, route, b)));
    assert_eq!(clean, shortcut, "{pair}: the clean screen is the shortcut's");
    clean
}

/// Five functions on one figure are one figure, five plots five (#11): the clean notebook's screen shows the one figure
/// the chart holds, named by the chart, and the shortcut's a figure for each line, named by the call that made it.
#[test]
fn a_notebook_shows_a_figure_for_each_thing_that_holds_one() {
    assert_eq!(figures_on(&screen("clean", None)), right(&[&["Chart", "chart"]]));
    assert_eq!(figures_on(&screen("shortcut", None)), right(&[&["Chart.line"], &["Chart.line"]]));
}

/// Across the cut the figures are the same (#11): a step planned by itself does not know the value its caller hands it,
/// so a figure it keeps in that value is held by it, and the caller says which value that is. Cut at every depth, the
/// clean notebook still shows the chart's one figure and the shortcut a figure for each line.
#[test]
fn a_figure_is_held_by_the_same_value_beyond_the_cut() {
    for (side, figures) in [("clean", 1), ("shortcut", 2)] {
        assert_eq!(beyond_every_cut(side, |b| screen(side, b)).len(), figures, "{side}");
    }
}

/// Building a class runs its initializer along its order, in the route's text as on the page: `Chart("compare")` opens
/// into `Chart.__init__`, which calls nothing, and never into the methods of the class, though `Chart` is a route of its
/// own (its doors). The first `line` is then the one that shows what it runs.
#[test]
fn building_a_class_opens_into_its_initializer_in_the_route_s_text() {
    let g = &figures().clean;
    let bodies = flow::bodies(g);
    let steps = |id: &str| -> Vec<String> { bodies[id].steps().iter().map(|s| s.to.clone()).collect() };
    let chart = |m: &str| format!("charts.chart.Chart.{m}");
    assert_eq!(steps("charts.chart.Chart"), ["__init__", "create_figure", "line"].map(chart), "the class's own route is its doors");
    assert_eq!(flow::initializers(g).get("charts.chart.Chart"), Some(&chart("__init__")), "building it runs its initializer");
    assert!(!bodies.contains_key(&chart("__init__")), "which calls nothing");
    // in the route's text, `Chart()` opens into none of the doors, and the second `line` is the first again: each call of
    // `line` is named, and the `create_figure` the first runs once
    let r = story::route(g, "charts.chart.compare", 4).expect("compare is a route");
    let lines = steps("charts.chart.compare").iter().filter(|s| **s == chart("line")).count();
    assert_eq!((r.matches("Chart.line").count(), r.matches("Chart.create_figure").count()), (lines, 1), "{r}");
}

/// The lent pair (`tests/pairs/figures/lent`): a figure handed between charts through a field (#11). The clean first chart
/// lends its figure by handing what it holds to the second's `take`, which sets its own fields; the shortcut's `lend`
/// reaches into the second chart and sets its fields there (`other.fig, other.ax = self.fig, self.ax`). Either way the
/// second chart holds the first's figure (`Node::sets`, a lens's put), so the notebook's screen shows one figure, also
/// beyond the cut; what tells them apart is who sets the second chart's fields.
#[test]
fn a_figure_lent_through_a_field_is_the_one_figure() {
    assert_eq!(figures_on(&one_screen("figures/lent", "notebooks.overlay")), right(&[&["Chart", "first"]]));
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
    let two = right(&[&["Chart", "first"], &["Chart", "second"]]);
    assert_eq!(figures_on(&one_screen("figures/titled", "notebooks.side_by_side")), two);
    let titled = common::pair("figures/titled");
    assert_eq!(sets(&titled.clean, "charts.chart.Chart.create_figure"), [("", "fig", ""), ("", "ax", "")], "where the figure rests");
    assert_eq!(sets(&titled.clean, "charts.chart.Chart.retitle"), [("", "title", "")], "the second chart sets its own title");
    assert_eq!(sets(&titled.shortcut, "charts.chart.Chart.lend_title"), [("other", "title", "title")], "the first sets the second's");
}

/// What `id`'s calls of `to` are handed, each value by its name with the fields of it each parameter is filled through
/// (`Held::through`, #172).
fn handed_through(g: &Graph, id: &str, to: &str) -> Vec<(String, Vec<(String, String)>)> {
    let slot = |s: &atlas_engine::Slot| serde_json::to_value(s).expect("a slot is data").to_string().replace('"', "");
    let sites = g.node(id).unwrap_or_else(|| panic!("{id} is in the graph")).sites.iter().filter(|s| s.to == to);
    sites.flat_map(|s| &s.hands).map(|h| (h.name.clone(), h.through.iter().map(|(s, f)| (slot(s), f.clone())).collect())).collect()
}

/// A call hands the value a projection is of, and the field it reads of it (#172): `lend` hands the first chart to the
/// second's `take` through the fields that hold its figure (`other.take(self.fig, self.ax)`), so the second holds the first's
/// figure; in `tests/plans/takes-a-title` it hands the first's title in the place of the figure (`other.take(self.title,
/// None)`), a field that holds none, so the second chart makes a figure of its own and the screen shows two, at every budget.
#[test]
fn a_parameter_filled_through_a_field_that_holds_no_figure_lends_none() {
    let lent = common::pair("figures/lent");
    let through =
        |fields: &[(&str, &str)]| -> Vec<(String, String)> { fields.iter().map(|(s, f)| (s.to_string(), f.to_string())).collect() };
    let first = |fields: &[(&str, &str)]| vec![("other".to_string(), vec![]), (String::new(), through(fields))];
    assert_eq!(
        handed_through(&lent.clean, "charts.chart.Chart.lend", "charts.chart.Chart.take"),
        first(&[("{at:0}", "fig"), ("{at:1}", "ax")])
    );
    let program = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/plans/takes-a-title");
    let titled = atlas_engine::analyze(&program, &Options::default()).expect("reads the program");
    assert_eq!(handed_through(&titled, "charts.chart.Chart.lend", "charts.chart.Chart.take"), first(&[("{at:0}", "title")]));
    let screen = beyond_every_cut("takes-a-title", |b| screen_at("plans/takes-a-title", "notebooks.overlay", b));
    assert_eq!(figures_on(&screen), right(&[&["Chart", "first"], &["Chart", "second"]]));
}

/// The panels pair (`tests/pairs/figures/panels`): two charts side by side on a board, each on a figure of its own. The
/// clean board draws each chart by the chart's own `line`; the shortcut reaches into its charts, making each one's figure
/// and plotting on its axes itself. Either way a call on `self.left` runs on the board through its field `left` (#172), and
/// the figure the call keeps in the value it runs on rests in that part: two parts, two figures, also beyond the cut. What
/// tells the sides apart is what the board calls on each part.
#[test]
fn a_figure_kept_in_a_part_rests_in_that_part() {
    let two = right(&[&["Board", "board.left"], &["Board", "board.right"]]);
    assert_eq!(figures_on(&one_screen("figures/panels", "notebooks.side_by_side")), two);
    let panels = common::pair("figures/panels");
    let parts = |g: &Graph, to: &str| -> Vec<String> {
        handed_through(g, "charts.chart.Board.draw", to).into_iter().flat_map(|(_, t)| t.into_iter().map(|(_, f)| f)).collect()
    };
    assert_eq!(parts(&panels.clean, "charts.chart.Chart.line"), ["left", "right"], "each chart draws its own line");
    assert_eq!(parts(&panels.shortcut, "charts.chart.Chart.create_figure"), ["left", "right"], "the board makes each chart's figure");
    assert!(parts(&panels.shortcut, "charts.chart.Chart.line").is_empty());
}

/// How many calls in `id`'s body make a figure: each call of `plt.subplots()`, also where one touch stands for two.
fn figure_calls(g: &Graph, id: &str) -> usize {
    let n = g.node(id).unwrap_or_else(|| panic!("{id} is in the graph"));
    n.touches.iter().filter(|t| t.kind_name() == "figure").map(|t| 1 + t.also_at.len()).sum()
}

/// The apart pair (`tests/pairs/figures/apart`): two series, each on a figure of its own (#175). The clean `compare` calls
/// `plot` for each series, and each `plot` makes one figure; the shortcut makes both in its own body, two `plt.subplots()`
/// calls, which the graph keeps as one touch on two lines. No value beyond a call holds what it makes, so each figure is its
/// call's: the screen shows two either way, also beyond the cut. What tells the sides apart is which step makes each: two
/// steps of one figure, or one step of two.
#[test]
fn two_calls_that_make_a_figure_make_two() {
    for (side, by) in [("clean", "plot"), ("shortcut", "compare")] {
        let screen = beyond_every_cut(side, |b| screen_of("figures/apart", side, "notebooks.compare", b));
        assert_eq!(figures_on(&screen), right(&[&[by], &[by]]), "{side}");
    }
    let apart = common::pair("figures/apart");
    assert_eq!(figure_calls(&apart.clean, "charts.chart.plot"), 1, "one call, run once for each series");
    assert_eq!(figure_calls(&apart.clean, "charts.chart.compare"), 0);
    assert_eq!(figure_touches(&apart.shortcut, "charts.chart.compare"), ["subplots"], "one touch");
    assert_eq!(figure_calls(&apart.shortcut, "charts.chart.compare"), 2, "two calls in one body");
}

/// The built pair (`tests/pairs/figures/built`): a board of two charts (#176). The clean board is handed its charts; the
/// shortcut builds them in its initializer and keeps each in a field (`self.left = Chart("left")`). A value kept in a field is
/// the body's own code putting it there, a lens's put: the initializer reads what each `Chart(..)` gives, so its own arrow
/// takes the chart (the `.plan` files), as building the clean board takes the charts it is handed, and the field is set from
/// it. Either way each figure rests in the part of the board that holds it. What tells the sides apart is where the charts
/// are made, and what the initializer sets its fields from: its parameters, or the results of its own steps.
#[test]
fn a_value_an_initializer_keeps_in_a_field_is_taken() {
    let two = right(&[&["Board", "board.left"], &["Board", "board.right"]]);
    assert_eq!(figures_on(&one_screen("figures/built", "notebooks.side_by_side")), two);
    let built = common::pair("figures/built");
    let init = "charts.chart.Board.__init__";
    let from = |g: &Graph| -> Vec<(String, String)> {
        g.node(init).unwrap().sets.iter().flat_map(|s| s.from.iter().map(|h| (s.field.clone(), h.name.clone()))).collect()
    };
    let field = |f: &str, by: &str| (f.to_string(), by.to_string());
    assert_eq!(from(&built.clean), [field("left", "left"), field("right", "right")], "set from the charts it is handed");
    let made: Vec<String> = built.shortcut.node(init).unwrap().sites.iter().flat_map(|s| s.gives.iter().map(|g| g.name.clone())).collect();
    assert_eq!(made.len(), 2, "each chart it builds is named by its step: {made:?}");
    assert_eq!(from(&built.shortcut), [field("left", &made[0]), field("right", &made[1])], "set from the charts it builds");
    let used: Vec<&str> = built.shortcut.node(init).unwrap().uses.iter().map(|u| u.name.as_str()).collect();
    assert!(made.iter().all(|m| used.contains(&m.as_str())), "its own code reads each chart it keeps: {used:?}");
}

/// The bound pair (`tests/pairs/figures/bound`) is the lent pair, where `lend` names what the first chart holds before it
/// lends it (`fig, ax = self.fig, self.ax`); `tests/plans/takes-a-bound-title` is `takes-a-title`, where it binds the title
/// first (`t = self.title; other.take(t, None)`). A projection bound to a name first is still the projection (#121, #174):
/// a call fills each parameter through the field the name is bound to, and a field set from the name is set from that field
/// (`Set::of`). So each form reads as the one without the name: one figure where the fields that hold it are lent, two
/// where the title is lent in their place, at every budget.
#[test]
fn a_projection_bound_to_a_name_is_still_the_projection() {
    let (lent, bound) = (common::pair("figures/lent"), common::pair("figures/bound"));
    let (lend, take) = ("charts.chart.Chart.lend", "charts.chart.Chart.take");
    assert_eq!(
        handed_through(&bound.clean, lend, take),
        handed_through(&lent.clean, lend, take),
        "`take(fig, ax)` is `take(self.fig, self.ax)`"
    );
    assert_eq!(sets(&bound.shortcut, lend), sets(&lent.shortcut, lend), "`other.fig = fig` is `other.fig = self.fig`");
    let program = |name: &str| {
        let at = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/plans").join(name);
        atlas_engine::analyze(&at, &Options::default()).expect("reads the program")
    };
    let (direct, named) = (program("takes-a-title"), program("takes-a-bound-title"));
    assert_eq!(handed_through(&named, lend, take), handed_through(&direct, lend, take), "`take(t, None)` is `take(self.title, None)`");
    // the screens with the name are the screens without it: one figure where the fields that hold it are lent, two where
    // the title is lent in their place
    let bound = one_screen("figures/bound", "notebooks.overlay");
    assert_eq!(bound, screen_of("figures/lent", "clean", "notebooks.overlay", None), "the bound pair is the lent pair");
    assert_eq!(bound.len(), 1);
    let named = beyond_every_cut("takes-a-bound-title", |b| screen_at("plans/takes-a-bound-title", "notebooks.overlay", b));
    assert_eq!(named, screen_at("plans/takes-a-title", "notebooks.overlay", None), "`take(t, None)` is `take(self.title, None)`");
    assert_eq!(named.len(), 2);
}

/// The shared pair (`tests/pairs/figures/shared`): two charts of a board on one figure (#173). The clean left chart lends its
/// figure to the right one by its own `lend` (`self.left.lend(self.right)`, which fills `other` with the board through
/// `right`); the shortcut's board hands its left chart's fields to the right one's `take` itself (`self.right.take(self.left
/// .fig, self.left.ax)`). Either way `take` sets the fields of the board's right part to those of its left part, and the class
/// of each part keeps its figure there (`Node::holds`: a board's `left` and `right` hold Charts), so the right part holds the
/// left part's figure: one figure, also beyond the cut. What tells the sides apart is who reads the left chart's fields: its
/// own `lend`, or the board's call.
#[test]
fn a_figure_lent_between_two_parts_of_one_value_is_the_one_figure() {
    assert_eq!(figures_on(&one_screen("figures/shared", "notebooks.overlay")), right(&[&["Board", "board.left"]]));
    let shared = common::pair("figures/shared");
    let chart = "charts.chart.Chart".to_string();
    let holds = &shared.clean.node("charts.chart.Board").expect("the board is in the graph").holds;
    assert_eq!(holds.iter().collect::<Vec<_>>(), [(&"left".to_string(), &chart), (&"right".to_string(), &chart)], "the class of each part");
    let through = |fields: &[(&str, &str)]| -> Vec<(String, Vec<(String, String)>)> {
        vec![("self".to_string(), fields.iter().map(|(s, f)| (s.to_string(), f.to_string())).collect())]
    };
    assert_eq!(
        handed_through(&shared.clean, "charts.chart.Board.overlay", "charts.chart.Chart.lend"),
        through(&[("receiver", "left"), ("{at:0}", "right")]),
        "the left chart lends, handed the right one"
    );
    assert_eq!(
        handed_through(&shared.shortcut, "charts.chart.Board.overlay", "charts.chart.Chart.take"),
        through(&[("receiver", "right"), ("{at:0}", "left.fig"), ("{at:1}", "left.ax")]),
        "the board reads the left chart's fields itself"
    );
}

/// The bindings of `id`'s body that set fields of the value it runs on, each as the fields it sets.
fn bindings(g: &Graph, id: &str) -> Vec<Vec<String>> {
    let mut out: Vec<(Option<(u32, u32)>, Vec<String>)> = vec![];
    for s in &g.node(id).unwrap_or_else(|| panic!("{id} is in the graph")).sets {
        match out.iter_mut().find(|(l, _)| *l == s.lines) {
            Some((_, fs)) => fs.push(s.field.clone()),
            None => out.push((s.lines, vec![s.field.clone()])),
        }
    }
    out.into_iter().map(|(_, fs)| fs).collect()
}

/// The two pair (`tests/pairs/figures/two`): a fit and its residuals, each on a figure of its own (#178). The clean fit holds
/// two charts, each of which keeps one figure; the shortcut's fit keeps both itself, one method making each in fields of its
/// own (`self.fig, self.ax = plt.subplots()`, `self.res_fig, self.res_ax = plt.subplots()`). A figure rests in the fields the
/// binding that makes it sets, so each binding is a figure the value holds: two either way, also beyond the cut, the
/// shortcut's named by the first field each binding sets. What tells the sides apart is what holds the figures: two parts
/// of the fit, or two slots of one value.
#[test]
fn two_figures_one_value_keeps_in_different_fields_are_two() {
    for (side, held) in [("clean", ["fit.data", "fit.residuals"]), ("shortcut", ["fit.fig", "fit.res_fig"])] {
        let screen = beyond_every_cut(side, |b| screen_of("figures/two", side, "notebooks.residuals", b));
        assert_eq!(figures_on(&screen), right(&held.map(|h| ["Fit", h])), "{side}");
    }
    let two = common::pair("figures/two");
    assert_eq!(bindings(&two.clean, "charts.chart.Chart.create_figure"), [["fig", "ax"]], "a chart keeps one figure");
    let holds = &two.clean.node("charts.chart.Fit").expect("the fit is in the graph").holds;
    assert_eq!(holds.values().collect::<Vec<_>>(), ["charts.chart.Chart", "charts.chart.Chart"], "the clean fit holds two charts");
    assert_eq!(bindings(&two.shortcut, "charts.chart.Fit.create_figures"), [["fig", "ax"], ["res_fig", "res_ax"]], "one value, two");
}

/// The handed pair (`tests/pairs/figures/handed`): two charts on a board (#179). The clean board is handed charts that hold no
/// figure yet and draws every line on them; the shortcut's caller draws on the left chart first and then hands it to the
/// board, which draws on it again. A part set whole to a value (`self.left = left`) is that value, a lens's put, so the
/// board's left part holds the figure the chart holds: two figures either way, also beyond the cut, each named where it is
/// made. What tells the sides apart is who draws on the left chart first, and so what the figure is named by: the board's
/// part, or the caller's chart.
#[test]
fn a_chart_set_whole_into_a_part_draws_there_on_the_figure_it_holds() {
    let two = |left: [&str; 2]| right(&[&left, &["Board", "board.right"]]);
    for (side, left) in [("clean", ["Board", "board.left"]), ("shortcut", ["Chart", "left"])] {
        let screen = beyond_every_cut(side, |b| screen_of("figures/handed", side, "notebooks.side_by_side", b));
        assert_eq!(figures_on(&screen), two(left), "{side}");
    }
    let handed = common::pair("figures/handed");
    let init = "charts.chart.Board.__init__";
    for (side, g) in [("clean", &handed.clean), ("shortcut", &handed.shortcut)] {
        let whole: Vec<(&str, &str)> = g
            .node(init)
            .unwrap()
            .sets
            .iter()
            .filter(|s| s.of.is_empty())
            .flat_map(|s| s.from.iter().map(|h| (s.field.as_str(), h.name.as_str())))
            .collect();
        assert_eq!(whole, [("left", "left"), ("right", "right")], "{side}: each part is set to the whole chart it is handed");
    }
    let lines = |g: &Graph| g.node("charts.chart.side_by_side").unwrap().sites.iter().filter(|s| s.to == "charts.chart.Chart.line").count();
    assert_eq!(lines(&handed.clean), 0, "the board draws every line");
    assert_eq!(lines(&handed.shortcut), 1, "the caller draws on the left chart before it hands it over");
    // the part is the chart either way round: a figure lent to the chart after the put is the part's (`tests/plans/
    // lent-after-the-put`), and one the board makes through the part is the chart's, which its own name draws on again
    // (`tests/plans/drawn-after-the-put`)
    let lent = beyond_every_cut("lent-after-the-put", |b| screen_at("plans/lent-after-the-put", "notebooks.side_by_side", b));
    assert_eq!(figures_on(&lent), two(["Chart", "first"]));
    let drawn = beyond_every_cut("drawn-after-the-put", |b| screen_at("plans/drawn-after-the-put", "notebooks.side_by_side", b));
    assert_eq!(figures_on(&drawn), two(["Board", "board.left"]));
}

/// The guarded pair (`tests/pairs/figures/guarded`): two fits, each keeping a figure for itself and one for its residuals; the
/// first lends its residuals' figure to the second, which draws (#184). The clean fit makes each figure only where its own
/// field holds none (`if self.ax is None:` around `create_figure`, `if self.res_ax is None:` around `create_residuals`); the
/// shortcut makes both under the one condition `self.ax is None`. A condition that reads a field of the slot a figure is made
/// in makes that slot's figure sure of, and one that reads another field makes it anew: the clean second fit draws its
/// residuals on the figure lent (three figures), the shortcut's puts a figure of its own over it (four), at every budget.
/// What tells the sides apart is which field each condition tests (`Node::tests`).
#[test]
fn a_condition_makes_sure_only_of_the_figure_whose_field_it_tests() {
    let fit = |names: &[&str]| right(&names.iter().map(|n| ["Fit", n]).collect::<Vec<_>>());
    let clean = fit(&["fit.fig", "fit.res_fig", "other.fig"]);
    let shortcut = fit(&["fit.fig", "fit.res_fig", "other.fig", "other.res_fig"]);
    for (side, figures) in [("clean", clean), ("shortcut", shortcut)] {
        let screen = beyond_every_cut(side, |b| screen_of("figures/guarded", side, "notebooks.residuals", b));
        assert_eq!(figures_on(&screen), figures, "{side}");
    }
    let guarded = common::pair("figures/guarded");
    let tests = |g: &Graph| -> Vec<(u32, String)> {
        g.node("charts.chart.Fit.draw").expect("draw is in the graph").tests.iter().map(|t| (t.choice, t.field.clone())).collect()
    };
    assert_eq!(tests(&guarded.clean), [(0, "ax".to_string()), (1, "res_ax".to_string())], "a condition for each figure");
    assert_eq!(tests(&guarded.shortcut), [(0, "ax".to_string())], "one condition, on the fit's own figure");
    assert_eq!(bindings(&guarded.shortcut, "charts.chart.Fit.create_figures"), [["fig", "ax"], ["res_fig", "res_ax"]], "two under it");
}

/// The borrowed pair (`tests/pairs/figures/borrowed`): a board borrows another chart's figure for its left part (#181). The
/// clean lender lends it by its own `lend`, handed the board's part (`lender.lend(self.left)` fills `other` with the board
/// through `left`); the shortcut's board reads the lender's fields and hands them to its part's `take` itself
/// (`self.left.take(lender.fig, lender.ax)`). A change made through a part changes its owner, a lens's put: either way the
/// call gives the board anew, its left part holding the lender's figure, and no chart of its own, also where `lend` is beyond
/// the cut and planned by itself. Two figures at every budget. What tells the sides apart is who reads the lender's fields:
/// its own `lend`, or the board's call.
#[test]
fn a_figure_lent_through_a_parameter_filled_with_a_part_is_the_lender_s() {
    let two = right(&[&["Chart", "first"], &["Board", "board.right"]]);
    assert_eq!(figures_on(&one_screen("figures/borrowed", "notebooks.side_by_side")), two);
    let borrowed = common::pair("figures/borrowed");
    let through =
        |fields: &[(&str, &str)]| -> Vec<(String, String)> { fields.iter().map(|(s, f)| (s.to_string(), f.to_string())).collect() };
    assert_eq!(
        handed_through(&borrowed.clean, "charts.chart.Board.borrow", "charts.chart.Chart.lend"),
        [("lender".to_string(), vec![]), (String::new(), through(&[("{at:0}", "left")]))],
        "the lender lends, handed the board's left part"
    );
    assert_eq!(
        handed_through(&borrowed.shortcut, "charts.chart.Board.borrow", "charts.chart.Chart.take"),
        [(String::new(), through(&[("receiver", "left")])), ("lender".to_string(), through(&[("{at:0}", "fig"), ("{at:1}", "ax")]))],
        "the board reads the lender's fields itself"
    );
    let gives = |g: &Graph, to: &str| -> Vec<String> {
        let borrow = g.node("charts.chart.Board.borrow").expect("borrow is in the graph");
        borrow.sites.iter().filter(|s| s.to == to).flat_map(|s| s.gives.iter().map(|h| h.name.clone())).collect()
    };
    assert_eq!(gives(&borrowed.clean, "charts.chart.Chart.lend"), ["self"], "a change made through the part gives the board anew");
    assert_eq!(gives(&borrowed.shortcut, "charts.chart.Chart.take"), ["self"], "a change made through the part gives the board anew");
}
