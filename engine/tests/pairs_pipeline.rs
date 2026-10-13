//! One program written three ways: web → data → preprocess → figure. It downloads SILSO's monthly
//! mean sunspot numbers (`https://www.sidc.be/SILSO/DATA/SN_m_tot_V2.0.csv`), keeps the raw file at
//! `data/raw/sunspots.csv`, loads it with a date per month and without the months that have no
//! mean (`-1`), smooths it with a 13-month rolling mean, and saves a line chart at
//! `figures/sunspots.png`. Each version is read as a repository of its own:
//!
//! - `python-clean/` as a scientific toolkit is written: typed functions that take their places as
//!   arguments, a frozen dataclass of settings, a `Plot` family, and a typer command that names the
//!   URL and the files and hands them down;
//! - `python-sloppy/` as a student writes it: one module, `sunspots.py`, whose functions share a
//!   module-level `df`, name their URL and files inside, and are called in a row under
//!   `if __name__ == "__main__":`;
//! - `rust/` the quick way: the compiler asks for every type, but the paths and the URL are written
//!   inside the functions that use them, and `main` calls them in a row.
//!
//! From outside the three are the same program: they reach the same places. Inside, Atlas lays bare
//! what differs: what the language repairs (types, what a function gives back) and what it cannot
//! (where data lives, where a choice is made, a value shared through a global). A statement Atlas
//! does not meet yet is a test of its own, ignored with what Atlas gets wrong: (a) where it misreads
//! code that is fine, (b) where it drops a shortcut or draws it as if it were clean.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::OnceLock;

use atlas_engine::principles::check;
use atlas_engine::{analyze, EdgeKind, EntryKind, Graph, Held, Node, Options, Site, Touch};

fn read(version: &str) -> Graph {
    analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/pipeline").join(version), &Options::default()).unwrap()
}

fn clean() -> &'static Graph {
    static G: OnceLock<Graph> = OnceLock::new();
    G.get_or_init(|| read("python-clean"))
}

fn sloppy() -> &'static Graph {
    static G: OnceLock<Graph> = OnceLock::new();
    G.get_or_init(|| read("python-sloppy"))
}

fn rust() -> &'static Graph {
    static G: OnceLock<Graph> = OnceLock::new();
    G.get_or_init(|| read("rust"))
}

fn node<'g>(g: &'g Graph, id: &str) -> &'g Node {
    g.node(id).unwrap_or_else(|| panic!("no node {id}"))
}

fn edge(g: &Graph, kind: EdgeKind, src: &str, dst: &str) -> bool {
    g.edges.iter().any(|e| e.kind == kind && e.src == src && e.dst == dst)
}

/// Every finding of `atlas check`, as its principle and the node it asks about.
fn findings(g: &Graph) -> Vec<(&'static str, String)> {
    check(g).into_iter().map(|f| (f.principle, f.node)).collect()
}

/// The nodes a principle asks about.
fn found(g: &Graph, principle: &str) -> Vec<String> {
    check(g).into_iter().filter(|f| f.principle == principle).map(|f| f.node).collect()
}

/// A function's touches of one kind (`disk_read`, `disk_write`, `net`, `figure`).
fn touches<'g>(g: &'g Graph, f: &str, kind: &str) -> Vec<&'g Touch> {
    node(g, f).touches.iter().filter(|t| t.kind_name() == kind).collect()
}

/// The step of `from` that calls or builds `to`.
fn step<'g>(g: &'g Graph, from: &str, to: &str) -> &'g Site {
    node(g, from).sites.iter().find(|s| s.to == to).unwrap_or_else(|| panic!("{from} has no step to {to}"))
}

/// The types of the values a step is handed or gives back.
fn types(hs: &[Held]) -> Vec<&str> {
    hs.iter().map(|h| h.ty.as_str()).collect()
}

/// The names the body keeps those values by.
fn names(hs: &[Held]) -> Vec<&str> {
    hs.iter().map(|h| h.name.as_str()).collect()
}

// ---------- the same from outside ----------

/// The program's way in: a command, a script, a binary's `main`. A door of a library is a way into
/// the library, not into the program.
fn program(g: &Graph) -> &str {
    let ways: Vec<&str> = g.nodes.iter().filter(|n| n.entry.as_ref().map_or(false, |e| matches!(e.kind, EntryKind::Cli | EntryKind::Script))).map(|n| n.id.as_str()).collect();
    assert_eq!(ways.len(), 1, "one way into the program: {ways:?}");
    ways[0]
}

/// What a program is from outside: the places the route from `start` reaches, by kind and artifact.
/// The quotient keeps the entry and the places and forgets every step between, so two programs
/// with the same outside differ only in what Atlas lays bare inside.
fn outside(g: &Graph, start: &str) -> BTreeSet<(String, String)> {
    let reached = g.reach(start);
    let runs = |f: &str| reached.contains(f);
    reached.iter().filter_map(|id| g.node(id)).flat_map(|n| n.touches_in(start, &runs)).map(|t| (t.kind_name().to_string(), t.artifact.clone())).collect()
}

/// The places every version reaches: the request to SILSO, the raw file written and read again, the
/// directories the files are kept in, and the figure written.
fn the_outside() -> BTreeSet<(String, String)> {
    [
        ("net", "www.sidc.be"),
        ("disk_write", "data/raw/"),
        ("disk_write", "data/raw/sunspots.csv"),
        ("disk_read", "data/raw/sunspots.csv"),
        ("disk_write", "figures/"),
        ("disk_write", "figures/sunspots.png"),
    ]
    .into_iter()
    .map(|(k, a)| (k.to_string(), a.to_string()))
    .collect()
}

/// The module whose `if __name__ == "__main__":` block runs the student's program: the calls of
/// the block are the module's own steps.
const STUDENT: &str = "sunspots";

/// The three ask SILSO and keep its file at `data/raw/sunspots.csv`, each from where its program
/// starts: the clean command, the student's main block, the Rust `main`.
#[test]
fn the_same_from_outside_the_request_and_the_raw_file() {
    for (g, start) in [(clean(), program(clean())), (sloppy(), STUDENT), (rust(), program(rust()))] {
        let out = outside(g, start);
        for place in [("net", "www.sidc.be"), ("disk_write", "data/raw/sunspots.csv")] {
            assert!(out.contains(&(place.0.to_string(), place.1.to_string())), "{start} reaches {place:?}: {out:#?}");
        }
    }
}

/// From the clean command, the route reaches the program's outside.
#[test]
#[ignore = "Atlas (a): the path `download` gives back and a path's `.parent` are not carried from the command, so the raw file read is `*.csv` and the directories have no name"]
fn the_same_from_outside_python_clean() {
    assert_eq!(program(clean()), "sunspots.cli.run");
    assert_eq!(outside(clean(), "sunspots.cli.run"), the_outside());
}

/// From the student's main block, the route reaches the program's outside, and one place more: the
/// student's `plot` draws on pyplot's current figure (`plt.figure`), which a notebook that imports
/// `sunspots` shows. The clean `Trend.draw` makes a `Figure` of its own and hands it back, and the
/// only figure the program leaves is the file it saves.
#[test]
fn the_same_from_outside_python_sloppy_and_pyplots_figure() {
    let mut want = the_outside();
    want.insert(("figure".to_string(), String::new()));
    assert_eq!(outside(sloppy(), STUDENT), want);
}

/// The student's main block is where the program starts: the module that holds it is a way in,
/// read as a script, as it is when the file is named `main.py`, after no package.
#[test]
#[ignore = "Atlas (b): an `if __name__ == \"__main__\":` block in a module of the package is no entry, so the student's program has no route"]
fn the_same_from_outside_the_students_main_block_is_a_way_in() {
    assert_eq!(node(sloppy(), STUDENT).entry.as_ref().map(|e| e.kind), Some(EntryKind::Script));
    assert_eq!(program(sloppy()), STUDENT);
}

/// From the Rust `main`, the route reaches the program's outside.
#[test]
#[ignore = "Atlas (a): the csv and plotters crates are unknown, so the raw file read and the PNG written are no places"]
fn the_same_from_outside_rust() {
    assert_eq!(program(rust()), "sunspots.main");
    assert_eq!(outside(rust(), "sunspots.main"), the_outside());
}

/// The clean `load` reads the file the command names: `load(download(SILSO, raw))` hands it what
/// `download` gives back, which is `to`, the path the command handed `download`.
#[test]
#[ignore = "Atlas (a): a function that gives back the path it was handed is not followed, so `load(download(url, raw))` reads `*.csv`"]
fn the_same_from_outside_python_clean_reads_the_path_download_gives_back() {
    let read = touches(clean(), "sunspots.series.load", "disk_read");
    assert!(read.iter().any(|t| t.artifact == "data/raw/sunspots.csv" && !t.fixed && t.by == ["sunspots.cli.run"]), "{read:#?}");
}

/// The clean functions create the directory their file goes in: `to.parent.mkdir(..)` creates
/// the directory of the path the command hands in.
#[test]
#[ignore = "Atlas (a): Python's `path.parent` is no path carried from the caller, so `to.parent.mkdir()` creates a directory without a name"]
fn the_same_from_outside_python_clean_creates_the_parent_of_the_path_handed_in() {
    for (f, dir) in [("sunspots.download.download", "data/raw/"), ("sunspots.plot.Plot.save", "figures/")] {
        let made: Vec<&Touch> = touches(clean(), f, "disk_write").into_iter().filter(|t| t.op == "mkdir").collect();
        assert!(made.iter().all(|t| t.artifact == dir && t.by == ["sunspots.cli.run"]) && !made.is_empty(), "{f}: {made:#?}");
    }
}

// ---------- Python, clean ----------

/// The clean version follows every principle: `atlas check` has nothing to ask.
#[test]
fn python_clean_follows_every_principle() {
    let fs = check(clean());
    assert!(fs.is_empty(), "{:#?}", fs.iter().map(|f| format!("{} {} {}", f.principle, f.node, f.what)).collect::<Vec<_>>());
}

/// Composition is call order, and every step says what it takes and gives: `load` gives a
/// `DataFrame`, `smooth` takes one and gives a new one, and the command hands the frame from step
/// to step until the plot saves it.
#[test]
fn python_clean_the_frame_goes_from_step_to_step() {
    let c = clean();
    assert!(edge(c, EdgeKind::Returns, "sunspots.series.load", "ext:pandas.DataFrame"));
    assert!(edge(c, EdgeKind::Takes, "sunspots.series.smooth", "ext:pandas.DataFrame"));
    assert!(edge(c, EdgeKind::Returns, "sunspots.series.smooth", "ext:pandas.DataFrame"));
    assert!(edge(c, EdgeKind::Takes, "sunspots.plot.Plot.save", "ext:pandas.DataFrame"));
    let run = "sunspots.cli.run";
    assert_eq!(types(&step(c, run, "sunspots.series.load").gives), ["ext:pandas.DataFrame"]);
    let smoothing = step(c, run, "sunspots.series.smooth");
    assert!(types(&smoothing.hands).contains(&"ext:pandas.DataFrame"), "{:?}", smoothing.hands);
    assert_eq!(names(&smoothing.gives), ["frame"]);
    assert_eq!(types(&smoothing.gives), ["ext:pandas.DataFrame"]);
    let saving = step(c, run, "sunspots.plot.Plot.save");
    assert!(names(&saving.hands).contains(&"frame") && types(&saving.hands).contains(&"ext:pandas.DataFrame"), "{:?}", saving.hands);
    // nothing is changed in place: each step gives a new value
    assert!(c.edges.iter().all(|e| e.kind != EdgeKind::Mutates));
}

/// Zoom is a quotient by base type: `Trend` is a `Plot`, its `draw` keeps the promise of the
/// base's, and the command's `save` runs on a `Trend`, so the route opens the family at the kind
/// it builds.
#[test]
fn python_clean_trend_is_a_plot() {
    let c = clean();
    assert!(edge(c, EdgeKind::Inherits, "sunspots.plot.Trend", "sunspots.plot.Plot"));
    assert_eq!(node(c, "sunspots.plot.Trend.draw").keeps, Some(true));
    assert!(edge(c, EdgeKind::Returns, "sunspots.plot.Trend.draw", "ext:matplotlib.figure.Figure"));
    assert_eq!(step(c, "sunspots.cli.run", "sunspots.plot.Plot.save").on, "sunspots.plot.Trend");
    assert!(edge(c, EdgeKind::Calls, "sunspots.plot.Plot.save", "sunspots.plot.Plot.draw"));
}

/// Library code does not decide where data lives: the command names the URL and the files, and
/// the library takes them as arguments. Each place is made whole at the entry.
#[test]
fn python_clean_the_places_are_made_whole_at_the_entry() {
    let c = clean();
    assert_eq!(found(c, "places"), Vec::<String>::new());
    let at_the_entry = |f: &str, kind: &str, artifact: &str| {
        let ts = touches(c, f, kind);
        assert!(ts.iter().any(|t| t.artifact == artifact && !t.fixed && t.by == ["sunspots.cli.run"]), "{f} {kind} {artifact}: {ts:#?}");
    };
    at_the_entry("sunspots.download.download", "net", "www.sidc.be");
    at_the_entry("sunspots.download.download", "disk_write", "data/raw/sunspots.csv");
    at_the_entry("sunspots.plot.Plot.save", "disk_write", "figures/sunspots.png");
    // the read is made whole at the entry too, though Atlas has only its format
    assert!(touches(c, "sunspots.series.load", "disk_read").iter().all(|t| !t.fixed && t.by == ["sunspots.cli.run"]));
}

// ---------- Python, sloppy ----------

/// What `atlas check` asks of the student's module, and nothing else. `sunspots.py` is the project's
/// package by its name, so its functions are its doors:
///
/// - `doors` on all four: none says what it takes or gives. They take nothing and give nothing,
///   because the frame goes from one to the next through the module's `df`, which no signature
///   shows; a reader, and Atlas, lose the route at each of them.
/// - `places` on `get_data` and `plot`: they write the URL, the raw file and the figure inside,
///   so a notebook that imports them, or a test, cannot point them elsewhere.
///
/// Not asked, rightly: `changes`, since `clean` changes no parameter but the global; `nothing`,
/// which asks only a function whose signature says it gives nothing, and these say nothing at all;
/// `gives`, since no caller uses what they give.
#[test]
fn python_sloppy_what_atlas_asks() {
    let want: Vec<(&str, String)> = [
        ("doors", "sunspots.get_data"),
        ("doors", "sunspots.clean"),
        ("doors", "sunspots.smooth"),
        ("doors", "sunspots.plot"),
        ("places", "sunspots.get_data"),
        ("places", "sunspots.plot"),
    ]
    .into_iter()
    .map(|(p, n)| (p, n.to_string()))
    .collect();
    assert_eq!(findings(sloppy()), want);
}

/// A package names its doors, and each door says what goes in and what comes out. The student's
/// functions are its doors, and no parameter or return is annotated; the clean package names its
/// six doors in `__all__`, each typed.
#[test]
fn python_sloppy_the_doors_say_nothing() {
    let s = sloppy();
    for f in ["sunspots.get_data", "sunspots.clean", "sunspots.smooth", "sunspots.plot"] {
        let n = node(s, f);
        assert_eq!(n.entry.as_ref().map(|e| e.kind), Some(EntryKind::Export), "{f} is a door");
        assert!(n.returns.is_none() && n.params.iter().all(|p| p.ty.is_empty()), "{f} says nothing: {:?} {:?}", n.params, n.returns);
    }
    let c = clean();
    assert_eq!(found(c, "doors"), Vec::<String>::new());
    for door in ["sunspots.download.download", "sunspots.series.load", "sunspots.series.smooth", "sunspots.plot.Plot", "sunspots.plot.Trend", "sunspots.settings.Settings"] {
        assert_eq!(node(c, door).entry.as_ref().map(|e| e.kind), Some(EntryKind::Export), "{door} is a door");
    }
}

/// Library code does not decide where data lives. The student's `get_data` and `plot` name the URL
/// and the files themselves; the places are still in the graph, at the functions that name them.
#[test]
fn python_sloppy_the_functions_name_their_own_places() {
    let s = sloppy();
    assert_eq!(found(s, "places"), ["sunspots.get_data", "sunspots.plot"]);
    for (f, kind, artifact) in [
        ("sunspots.get_data", "net", "www.sidc.be"),
        ("sunspots.get_data", "disk_write", "data/raw/sunspots.csv"),
        ("sunspots.get_data", "disk_read", "data/raw/sunspots.csv"),
        ("sunspots.plot", "disk_write", "figures/sunspots.png"),
    ] {
        let ts = touches(s, f, kind);
        assert!(ts.iter().any(|t| t.artifact == artifact && t.fixed && t.by.is_empty()), "{f} {kind} {artifact}: {ts:#?}");
    }
}

/// Composition is call order, and a step hands on what the next one takes. The student's frame
/// travels outside the route, through the module's `df`: no function takes or gives a value, and
/// the main block's steps hand on nothing. `clean` and `smooth` are steps that reach no place and
/// give nothing back.
#[test]
fn python_sloppy_the_frame_travels_outside_the_route() {
    let s = sloppy();
    assert!(s.edges.iter().all(|e| !matches!(e.kind, EdgeKind::Takes | EdgeKind::Returns)), "no function takes or gives a type");
    let steps = &node(s, STUDENT).sites;
    assert_eq!(steps.iter().map(|st| st.to.as_str()).collect::<Vec<_>>(), ["sunspots.get_data", "sunspots.clean", "sunspots.smooth", "sunspots.plot"]);
    assert!(steps.iter().all(|st| st.hands.is_empty() && st.gives.is_empty()), "{steps:#?}");
    for f in ["sunspots.clean", "sunspots.smooth"] {
        assert!(node(s, f).touches.is_empty(), "{f} reaches no place");
    }
}

/// A change shows where a reader sees it. The student's `clean` changes the module's frame in place
/// (`df.drop(.., inplace=True)`): the graph holds that change, as it holds one to a parameter.
#[test]
#[ignore = "Atlas (b): a change to a module global (`df.drop(.., inplace=True)`) is no mutates edge, so `clean` reads as an arrow from nothing to nothing"]
fn python_sloppy_clean_changes_the_global_frame() {
    let s = sloppy();
    assert!(s.edges.iter().any(|e| e.kind == EdgeKind::Mutates && e.src == "sunspots.clean" && e.via.as_deref() == Some("df")), "{:#?}", s.edges.iter().filter(|e| e.src == "sunspots.clean").collect::<Vec<_>>());
}

// ---------- Rust, the quick way ----------

/// What the compiler repairs. Every signature says what it takes and gives, so `doors` and `gives`
/// have nothing to ask, and the values go from step to step as arguments: `load` gives `rows`,
/// `smooth` is handed them and gives `smoothed`, `plot` is handed both. Where the student's frame
/// travels through a global, the quick Rust hands it on.
#[test]
fn rust_the_compiler_asks_for_every_type() {
    let r = rust();
    assert_eq!(found(r, "doors"), Vec::<String>::new());
    assert_eq!(found(r, "gives"), Vec::<String>::new());
    for f in ["sunspots.download", "sunspots.load", "sunspots.smooth", "sunspots.plot"] {
        assert!(node(r, f).returns.as_ref().map_or(false, |x| !x.written.is_empty()), "{f} says what it gives");
    }
    let main = "sunspots.main";
    assert_eq!(names(&step(r, main, "sunspots.load").gives), ["rows"]);
    assert_eq!(names(&step(r, main, "sunspots.smooth").hands), ["rows"]);
    assert_eq!(names(&step(r, main, "sunspots.smooth").gives), ["smoothed"]);
    assert_eq!(names(&step(r, main, "sunspots.plot").hands), ["rows", "smoothed"]);
}

/// What the compiler does not repair: where data lives. `download` and `plot` name their URL and
/// their paths inside, and `places` asks both.
#[test]
fn rust_the_compiler_does_not_ask_where_data_lives() {
    let r = rust();
    assert_eq!(findings(r), [("places", "sunspots.download".to_string()), ("places", "sunspots.plot".to_string())]);
    for (f, kind, artifact) in [
        ("sunspots.download", "net", "www.sidc.be"),
        ("sunspots.download", "disk_write", "data/raw/sunspots.csv"),
        ("sunspots.plot", "disk_write", "figures/"),
    ] {
        let ts = touches(r, f, kind);
        assert!(ts.iter().any(|t| t.artifact == artifact && t.fixed && t.by.is_empty()), "{f} {kind} {artifact}: {ts:#?}");
    }
}

/// What the compiler does not repair either: a value of a type that names no what. The quick
/// version keeps a month as a pair, `(NaiveDate, f64)`, so the values its steps hand on have no
/// type of the program, as the Rust pair's `split_amount` gives `(String, String)`.
#[test]
fn rust_a_pair_names_no_what() {
    let r = rust();
    assert_eq!(node(r, "sunspots.smooth").returns.as_ref().map(|x| x.written.as_str()), Some("Vec<(NaiveDate, f64)>"));
    for to in ["sunspots.load", "sunspots.smooth"] {
        assert_eq!(types(&step(r, "sunspots.main", to).gives), [""], "{to}");
    }
}

/// `load` reads the raw file through the csv crate (`ReaderBuilder::from_path`), at a path it names
/// itself, so `places` asks it too.
#[test]
#[ignore = "Atlas (a): the csv crate is unknown, so `ReaderBuilder::from_path(..)` reads no file"]
fn rust_load_reads_the_raw_file_through_the_csv_crate() {
    let r = rust();
    let read = touches(r, "sunspots.load", "disk_read");
    assert!(read.iter().any(|t| t.artifact == "data/raw/sunspots.csv" && t.fixed), "{read:#?}");
    assert!(found(r, "places").contains(&"sunspots.load".to_string()));
}

/// `plot` writes the PNG through plotters (`BitMapBackend::new(path, ..)`), at a path it names
/// itself.
#[test]
#[ignore = "Atlas (a): the plotters crate is unknown, so `BitMapBackend::new(path, ..)` writes no file"]
fn rust_plot_writes_the_png_through_plotters() {
    let written = touches(rust(), "sunspots.plot", "disk_write");
    assert!(written.iter().any(|t| t.artifact == "figures/sunspots.png" && t.fixed), "{written:#?}");
}
