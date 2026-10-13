//! One program written five ways: web → data → preprocess → figure. It downloads SILSO's monthly
//! mean sunspot numbers (`https://www.sidc.be/SILSO/DATA/SN_m_tot_V2.0.csv`), keeps the raw file at
//! `data/raw/sunspots.csv`, loads it with a date per month and without the months that have no
//! mean (`-1`), smooths it with a 13-month rolling mean, and saves a line chart at
//! `figures/sunspots.png`, or `figures/sunspots.svg` where the language's plotting path writes an
//! SVG document. Each version is read as a repository of its own:
//!
//! - `python-clean/` as a scientific toolkit is written: typed functions that take their places as
//!   arguments, a frozen dataclass of settings, a `Plot` family, and a typer command that names the
//!   URL and the files and hands them down;
//! - `python-sloppy/` as a student writes it: one module, `sunspots.py`, whose functions share a
//!   module-level `df`, name their URL and files inside, and are called in a row under
//!   `if __name__ == "__main__":`;
//! - `rust/` the quick way: the compiler asks for every type, but the paths and the URL are written
//!   inside the functions that use them, and `main` calls them in a row;
//! - `typescript/` as a TypeScript developer writes it: a command in `bin` that names the URL and
//!   the files and hands them down, typed records (`Month`, `Smoothed`, `Settings`), `fetch` and
//!   `node:fs`, and the figure drawn as an SVG document with d3's scales and lines and written as
//!   text;
//! - `rust-clean/` as a Rust developer writes it, the port of `python-clean/` and `typescript/`: a
//!   library crate whose root re-exports its steps and records (`Month`, `Smoothed`, `Settings`),
//!   the raw file read into serde records through the csv crate, a `Plot` trait whose `Trend`
//!   draws through plotters, an error type that every step that may fail gives in its `Result`,
//!   and a binary whose `main` parses its options with clap, names the URL and hands the files
//!   down.
//!
//! From outside the five are the same program: they reach the same places. Inside, Atlas lays bare
//! what differs: what the language repairs (types, what a function gives back) and what it cannot
//! (where data lives, where a choice is made, a value shared through a global). The three clean
//! versions are one port, each written well, so what differs between them is the language's doing.
//! A statement Atlas does not meet yet is a test of its own, ignored with what Atlas gets wrong:
//! (a) where it misreads code that is fine, (b) where it drops a shortcut or draws it as if it were
//! clean.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::OnceLock;

use atlas_engine::principles::check;
use atlas_engine::{analyze, EdgeKind, EntryKind, Graph, Held, Node, NodeKind, Options, Site, Touch, World};

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

fn typescript() -> &'static Graph {
    static G: OnceLock<Graph> = OnceLock::new();
    G.get_or_init(|| read("typescript"))
}

fn rust_clean() -> &'static Graph {
    static G: OnceLock<Graph> = OnceLock::new();
    G.get_or_init(|| read("rust-clean"))
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

/// Whether a type is one of the program's own.
fn own(g: &Graph, ty: &str) -> bool {
    g.node(ty).map_or(false, |n| n.kind == NodeKind::Class)
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
    // a module's global is the program's own memory, not the world's
    let world = |t: &&Touch| t.kind != World::Global;
    reached.iter().filter_map(|id| g.node(id)).flat_map(|n| n.touches_in(start, &runs)).filter(world).map(|t| (t.kind_name().to_string(), t.artifact.clone())).collect()
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

/// The outside with the figure's format left out. matplotlib and plotters write the figure as a
/// PNG, the TypeScript version as an SVG document: the same place in the format each language's
/// plotting path writes. Only the extension of a file in `figures/` is left out; every other place,
/// the directory `figures/` with them, is compared as it is.
fn in_any_format(out: BTreeSet<(String, String)>) -> BTreeSet<(String, String)> {
    out.into_iter().map(|(kind, artifact)| (kind, any_format(&artifact))).collect()
}

/// A place with the extension of a file in `figures/` left out.
fn any_format(artifact: &str) -> String {
    match artifact.strip_prefix("figures/").and_then(|file| file.rsplit_once('.')) {
        Some((stem, _)) => format!("figures/{stem}.*"),
        None => artifact.to_string(),
    }
}

/// The module whose `if __name__ == "__main__":` block runs the student's program: the calls of
/// the block are the module's own steps.
const STUDENT: &str = "sunspots";

/// The four ask SILSO and keep its file at `data/raw/sunspots.csv`, each from where its program
/// starts: the clean command, the student's main block, the Rust `main`, the TypeScript command.
#[test]
fn the_same_from_outside_the_request_and_the_raw_file() {
    for (g, start) in [(clean(), program(clean())), (sloppy(), STUDENT), (rust(), program(rust())), (typescript(), program(typescript()))] {
        let out = outside(g, start);
        for place in [("net", "www.sidc.be"), ("disk_write", "data/raw/sunspots.csv")] {
            assert!(out.contains(&(place.0.to_string(), place.1.to_string())), "{start} reaches {place:?}: {out:#?}");
        }
    }
}

/// From the clean command, the route reaches the program's outside.
#[test]
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

/// The student's main block is where the program starts: the module that holds it runs as a
/// command (`python -m sunspots`), a way in. It is still a module others import, so its functions
/// stay library code.
#[test]
fn the_same_from_outside_the_students_main_block_is_a_way_in() {
    let entry = node(sloppy(), STUDENT).entry.clone().map(|e| (e.kind, e.label));
    assert_eq!(entry, Some((EntryKind::Cli, "python -m sunspots".to_string())));
    assert_eq!(program(sloppy()), STUDENT);
}

/// From the Rust `main`, the route reaches the program's outside.
#[test]
fn the_same_from_outside_rust() {
    assert_eq!(program(rust()), "sunspots.main");
    assert_eq!(outside(rust(), "sunspots.main"), the_outside());
}

/// The places the TypeScript command names are the program's outside: the request to SILSO, the
/// raw file written and read again, the directories, and the figure, written as an SVG document
/// where the others write a PNG. The command is the module `package.json` names in `bin`, whose
/// own code runs `main`. What `--raw` and `--figure` say is the command's input, which holds its
/// default (`values.raw ?? RAW`), so the route reaches no place the code does not name.
#[test]
fn the_same_from_outside_typescript_names_the_same_places() {
    let t = typescript();
    assert_eq!(program(t), "typescript.cli");
    let out = outside(t, "typescript.cli");
    assert!(out.contains(&("disk_write".to_string(), "figures/sunspots.svg".to_string())), "{out:#?}");
    let (named, unnamed): (BTreeSet<_>, BTreeSet<_>) = out.into_iter().partition(|(_, artifact)| !artifact.is_empty());
    assert_eq!(in_any_format(named), in_any_format(the_outside()));
    assert!(unnamed.is_empty(), "{unnamed:?}");
}

/// From the TypeScript command, the route reaches the program's outside, as from the clean Python
/// command: both take the raw file and the figure as options of the command line with a default
/// (`raw: Path = RAW` in typer's signature, `values.raw ?? RAW` after `parseArgs`).
#[test]
fn the_same_from_outside_typescript() {
    assert_eq!(in_any_format(outside(typescript(), "typescript.cli")), in_any_format(the_outside()));
}

/// The clean `load` reads the file the command names: `load(download(SILSO, raw))` hands it what
/// `download` gives back, which is `to`, the path the command handed `download`.
#[test]
fn the_same_from_outside_python_clean_reads_the_path_download_gives_back() {
    let read = touches(clean(), "sunspots.series.load", "disk_read");
    assert!(read.iter().any(|t| t.artifact == "data/raw/sunspots.csv" && !t.fixed && t.by == ["sunspots.cli.run"]), "{read:#?}");
}

/// The clean functions create the directory their file goes in: `to.parent.mkdir(..)` creates
/// the directory of the path the command hands in.
#[test]
fn the_same_from_outside_python_clean_creates_the_parent_of_the_path_handed_in() {
    for (f, dir) in [("sunspots.download.download", "data/raw/"), ("sunspots.plot.Plot.save", "figures/")] {
        let made: Vec<&Touch> = touches(clean(), f, "disk_write").into_iter().filter(|t| t.op == "mkdir").collect();
        assert!(made.iter().all(|t| t.artifact == dir && t.by == ["sunspots.cli.run"]) && !made.is_empty(), "{f}: {made:#?}");
    }
}

/// The TypeScript `load` reads the file the command names: `load(await download(SILSO, raw))` hands
/// it what `download` gives back, which is `to`, the path the command handed `download`.
#[test]
fn the_same_from_outside_typescript_reads_the_path_download_gives_back() {
    let read = touches(typescript(), "typescript.series.load", "disk_read");
    assert!(read.iter().any(|t| t.artifact == "data/raw/sunspots.csv" && !t.fixed && t.by == ["typescript.cli.main"]), "{read:#?}");
}

/// The TypeScript functions create the directory their file goes in: `mkdir(dirname(to), ..)`
/// creates the directory of the path the command hands in.
#[test]
fn the_same_from_outside_typescript_creates_the_parent_of_the_path_handed_in() {
    for (f, dir) in [("typescript.download.download", "data/raw/"), ("typescript.plot.saveSvg", "figures/")] {
        let made: Vec<&Touch> = touches(typescript(), f, "disk_write").into_iter().filter(|t| t.op == "mkdir" && !t.artifact.is_empty()).collect();
        assert!(made.iter().all(|t| t.artifact == dir && t.by == ["typescript.cli.main"]) && !made.is_empty(), "{f}: {made:#?}");
    }
}

/// From the clean Rust `main`, the route reaches the program's outside, as from the clean Python
/// command: `main` names the URL, and clap's `--raw` and `--figure` name the raw file and the
/// figure with their defaults (`#[arg(long, default_value = RAW)]`).
#[test]
fn the_same_from_outside_rust_clean() {
    assert_eq!(program(rust_clean()), "sunspots.main.main");
    assert_eq!(outside(rust_clean(), "sunspots.main.main"), the_outside());
}

/// The clean Rust `load` reads the file the command names: `load(&download(SILSO, &cli.raw)?)`
/// hands it what `download` gives back, `Ok(to.to_path_buf())`, the path `main` handed `download`.
#[test]
fn the_same_from_outside_rust_clean_reads_the_path_download_gives_back() {
    let read = touches(rust_clean(), "sunspots.series.load", "disk_read");
    assert!(read.iter().any(|t| t.artifact == "data/raw/sunspots.csv" && !t.fixed && t.by == ["sunspots.main.main"]), "{read:#?}");
}

/// The clean Rust functions create the directory their file goes in:
/// `if let Some(dir) = to.parent() { fs::create_dir_all(dir)? }` creates the directory of the path
/// `main` hands in.
#[test]
fn the_same_from_outside_rust_clean_creates_the_parent_of_the_path_handed_in() {
    for (f, dir) in [("sunspots.download.download", "data/raw/"), ("sunspots.plot.Plot.save", "figures/")] {
        let made: Vec<&Touch> = touches(rust_clean(), f, "disk_write").into_iter().filter(|t| t.op == "create_dir_all" && !t.artifact.is_empty()).collect();
        assert!(made.iter().all(|t| t.artifact == dir && t.by == ["sunspots.main.main"]) && !made.is_empty(), "{f}: {made:#?}");
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
/// travels beside the route, through the module's `df`: no function takes or gives a value, and
/// the main block's steps hand on nothing.
#[test]
fn python_sloppy_the_frame_travels_beside_the_route() {
    let s = sloppy();
    assert!(s.edges.iter().all(|e| !matches!(e.kind, EdgeKind::Takes | EdgeKind::Returns)), "no function takes or gives a type");
    let steps = &node(s, STUDENT).sites;
    assert_eq!(steps.iter().map(|st| st.to.as_str()).collect::<Vec<_>>(), ["sunspots.get_data", "sunspots.clean", "sunspots.smooth", "sunspots.plot"]);
    assert!(steps.iter().all(|st| st.hands.is_empty() && st.gives.is_empty()), "{steps:#?}");
}

/// A value a module keeps between calls, which its functions change, is a place: the student's
/// frame is `Global(sunspots.df)`. `get_data` writes it, `clean` and `smooth` read and change it
/// (`df.drop(.., inplace=True)`, `df["smooth"] = ..`), `plot` reads it. Drawn, not judged: a global
/// is sometimes the right choice.
#[test]
fn python_sloppy_the_frame_is_a_global_place() {
    let s = sloppy();
    let global = |f: &str| node(s, f).touches.iter().filter(|t| t.kind == World::Global && t.artifact == "sunspots.df").map(|t| t.dir_name()).collect::<BTreeSet<_>>();
    assert_eq!(global("sunspots.get_data"), BTreeSet::from(["read", "write"]));
    assert_eq!(global("sunspots.clean"), BTreeSet::from(["read", "write"]));
    assert_eq!(global("sunspots.smooth"), BTreeSet::from(["read", "write"]));
    assert_eq!(global("sunspots.plot"), BTreeSet::from(["read"]));
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

/// What the compiler does not repair: where data lives. `download`, `load` and `plot` name their
/// URL and their paths inside, and `places` asks all three.
#[test]
fn rust_the_compiler_does_not_ask_where_data_lives() {
    let r = rust();
    assert_eq!(findings(r), [("places", "sunspots.download".to_string()), ("places", "sunspots.load".to_string()), ("places", "sunspots.plot".to_string())]);
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
fn rust_load_reads_the_raw_file_through_the_csv_crate() {
    let r = rust();
    let read = touches(r, "sunspots.load", "disk_read");
    assert!(read.iter().any(|t| t.artifact == "data/raw/sunspots.csv" && t.fixed), "{read:#?}");
    assert!(found(r, "places").contains(&"sunspots.load".to_string()));
}

/// `plot` writes the PNG through plotters (`BitMapBackend::new(path, ..)`), at a path it names
/// itself.
#[test]
fn rust_plot_writes_the_png_through_plotters() {
    let written = touches(rust(), "sunspots.plot", "disk_write");
    assert!(written.iter().any(|t| t.artifact == "figures/sunspots.png" && t.fixed), "{written:#?}");
}

// ---------- TypeScript ----------

/// The TypeScript version follows every principle: `atlas check` has nothing to ask. The package
/// names its doors in `index.ts`, each typed: the four steps, `parse` beside `load`, and the
/// records they hand on.
#[test]
fn typescript_follows_every_principle() {
    let t = typescript();
    let fs = check(t);
    assert!(fs.is_empty(), "{:#?}", fs.iter().map(|f| format!("{} {} {}", f.principle, f.node, f.what)).collect::<Vec<_>>());
    let doors: BTreeSet<&str> = t.nodes.iter().filter(|n| n.entry.as_ref().map_or(false, |e| e.kind == EntryKind::Export)).map(|n| n.id.as_str()).collect();
    let want = [
        "typescript.download.download",
        "typescript.series.load",
        "typescript.series.parse",
        "typescript.series.smooth",
        "typescript.plot.trend",
        "typescript.plot.saveSvg",
        "typescript.series.Month",
        "typescript.series.Smoothed",
        "typescript.settings.Settings",
    ];
    assert_eq!(doors, want.into_iter().collect());
}

/// Composition is call order, and every step says what it takes and gives. TypeScript names the
/// records the program keeps: `load` gives `Month`s, `smooth` takes them and gives `Smoothed`
/// means, and `trend` draws both with the `Settings`. The command hands them from step to step by
/// the names it keeps them by, and nothing is changed in place.
#[test]
fn typescript_the_records_go_from_step_to_step() {
    let t = typescript();
    let (month, smoothed, settings) = ("typescript.series.Month", "typescript.series.Smoothed", "typescript.settings.Settings");
    assert!(edge(t, EdgeKind::Returns, "typescript.series.load", month));
    assert!(edge(t, EdgeKind::Takes, "typescript.series.smooth", month));
    assert!(edge(t, EdgeKind::Returns, "typescript.series.smooth", smoothed));
    for ty in [month, smoothed, settings] {
        assert!(edge(t, EdgeKind::Takes, "typescript.plot.trend", ty), "trend takes {ty}");
    }
    let main = "typescript.cli.main";
    let loading = step(t, main, "typescript.series.load");
    assert_eq!((names(&loading.gives), types(&loading.gives)), (vec!["series"], vec![month]));
    let smoothing = step(t, main, "typescript.series.smooth");
    assert!(smoothing.hands.iter().any(|h| h.name == "series" && h.ty == month), "{:?}", smoothing.hands);
    assert_eq!((names(&smoothing.gives), types(&smoothing.gives)), (vec!["smoothed"], vec![smoothed]));
    let drawing = step(t, main, "typescript.plot.trend");
    assert_eq!((names(&drawing.hands), types(&drawing.hands)), (vec!["series", "smoothed", "settings"], vec![month, smoothed, settings]));
    assert!(t.edges.iter().all(|e| e.kind != EdgeKind::Mutates));
}

/// The figure travels as its document: `trend` gives the SVG as a `string`, and `saveSvg` writes the
/// text it is handed. A string is a value of the language, not of the program, as the path
/// `download` gives back is; Python's `Trend.draw` gives a matplotlib `Figure`.
#[test]
fn typescript_the_figure_is_a_document() {
    let t = typescript();
    assert_eq!(node(t, "typescript.plot.trend").returns.as_ref().map(|r| r.written.as_str()), Some("string"));
    let main = "typescript.cli.main";
    let drawing = step(t, main, "typescript.plot.trend");
    assert_eq!(types(&drawing.gives), [""]);
    assert_eq!(step(t, main, "typescript.plot.saveSvg").hands, drawing.gives);
}

/// Library code does not decide where data lives: the command names the URL and the files and hands
/// them down, and `--raw` and `--figure` may name others. Each place is made whole at `main`.
#[test]
fn typescript_the_places_are_made_whole_at_the_entry() {
    let t = typescript();
    assert_eq!(found(t, "places"), Vec::<String>::new());
    for (f, kind, artifact) in [
        ("typescript.download.download", "net", "www.sidc.be"),
        ("typescript.download.download", "disk_write", "data/raw/sunspots.csv"),
        ("typescript.series.load", "disk_read", "data/raw/sunspots.csv"),
        ("typescript.plot.saveSvg", "disk_write", "figures/sunspots.svg"),
    ] {
        let ts = touches(t, f, kind);
        assert!(ts.iter().any(|x| x.artifact == artifact && !x.fixed && x.by == ["typescript.cli.main"]), "{f} {kind} {artifact}: {ts:#?}");
    }
}

/// Creating a value is an arrow into Memory(T). The command makes its `Settings` from the defaults
/// and `--months` (`const settings: Settings = { ...DEFAULT_SETTINGS, months: .. }`), and a step of
/// `main` gives `settings`, as Python's `Settings(months=months)` does.
#[test]
fn typescript_main_makes_its_settings() {
    let main = node(typescript(), "typescript.cli.main");
    let made = main.sites.iter().any(|s| s.gives.iter().any(|g| g.name == "settings" && g.ty == "typescript.settings.Settings"));
    assert!(made, "{:#?}", main.sites);
}

// ---------- Rust, clean ----------

/// The clean Rust version follows every principle: `atlas check` has nothing to ask. The binary's
/// `main` is the program's way in.
#[test]
fn rust_clean_follows_every_principle() {
    let r = rust_clean();
    let fs = check(r);
    assert!(fs.is_empty(), "{:#?}", fs.iter().map(|f| format!("{} {} {}", f.principle, f.node, f.what)).collect::<Vec<_>>());
    assert_eq!(program(r), "sunspots.main.main");
}

/// Composition is call order, and every step says what it takes and gives. Rust names the records
/// the program keeps, as TypeScript does: `load` gives `Month`s, `smooth` takes them and gives
/// `Smoothed` means, and `main` hands them from step to step by the names it keeps them by.
/// Creating a value is an arrow into Memory(T): `main` makes its `Settings` from `--months` and the
/// defaults (`Settings { months: cli.months, ..Settings::default() }`), as Python's
/// `Settings(months=months)`, and hands them to `smooth` and to the plot. Nothing is changed in
/// place.
#[test]
fn rust_clean_the_records_go_from_step_to_step() {
    let r = rust_clean();
    let (month, smoothed, settings) = ("sunspots.series.Month", "sunspots.series.Smoothed", "sunspots.settings.Settings");
    assert!(edge(r, EdgeKind::Returns, "sunspots.series.load", month));
    assert!(edge(r, EdgeKind::Takes, "sunspots.series.smooth", month));
    assert!(edge(r, EdgeKind::Returns, "sunspots.series.smooth", smoothed));
    for ty in [month, smoothed] {
        assert!(edge(r, EdgeKind::Takes, "sunspots.plot.Plot.save", ty), "save takes {ty}");
    }
    let main = "sunspots.main.main";
    assert_eq!(names(&step(r, main, settings).gives), ["settings"]);
    let loading = step(r, main, "sunspots.series.load");
    assert_eq!((names(&loading.gives), types(&loading.gives)), (vec!["series"], vec![month]));
    let smoothing = step(r, main, "sunspots.series.smooth");
    assert_eq!((names(&smoothing.hands), types(&smoothing.hands)), (vec!["series", "settings"], vec![month, settings]));
    assert_eq!((names(&smoothing.gives), types(&smoothing.gives)), (vec!["smoothed"], vec![smoothed]));
    assert_eq!(names(&step(r, main, "sunspots.plot.Trend.new").hands), ["settings"]);
    assert!(r.edges.iter().all(|e| e.kind != EdgeKind::Mutates));
}

/// Zoom is a quotient by base type: `Trend` is a `Plot` and draws what the trait asks it to, and
/// the trait's own `save` draws through `draw` before it writes the figure. What the trait's
/// methods take, its kind takes.
#[test]
fn rust_clean_trend_is_a_plot() {
    let r = rust_clean();
    assert!(edge(r, EdgeKind::Inherits, "sunspots.plot.Trend", "sunspots.plot.Plot"));
    assert!(edge(r, EdgeKind::Contains, "sunspots.plot.Trend", "sunspots.plot.Trend.draw"));
    assert!(edge(r, EdgeKind::Calls, "sunspots.plot.Plot.save", "sunspots.plot.Plot.draw"));
    for f in ["sunspots.plot.Plot.draw", "sunspots.plot.Trend.draw"] {
        for ty in ["sunspots.series.Month", "sunspots.series.Smoothed"] {
            assert!(edge(r, EdgeKind::Takes, f, ty), "{f} takes {ty}");
        }
    }
}

/// Composition is call order: `main` downloads SILSO's file, and `load` is handed the path
/// `download` gives back. The crate's root re-exports `download` from the module of that name, as
/// Python's `__init__` and TypeScript's `index.ts` do, so it is a door.
#[test]
fn rust_clean_main_downloads_the_file() {
    let r = rust_clean();
    let main = "sunspots.main.main";
    let pairs = |hs: &[Held]| hs.iter().map(|h| (h.name.clone(), h.ty.clone())).collect::<Vec<_>>();
    let downloading = step(r, main, "sunspots.download.download");
    assert_eq!(pairs(&step(r, main, "sunspots.series.load").hands), pairs(&downloading.gives));
    assert_eq!(node(r, "sunspots.download.download").entry.as_ref().map(|e| e.kind), Some(EntryKind::Export));
}

/// Zoom is a quotient by base type, and a route opens the family at the kind it builds:
/// `Trend::new(settings).save(..)` runs the trait's `save` on a `Trend`, handed the series and the
/// smoothed means, as Python's `Trend(settings).save(frame, figure)` runs `Plot.save` on a `Trend`.
#[test]
fn rust_clean_main_saves_a_trend() {
    let saving = step(rust_clean(), "sunspots.main.main", "sunspots.plot.Plot.save");
    assert_eq!(saving.on, "sunspots.plot.Trend");
    assert!(names(&saving.hands).contains(&"series") && names(&saving.hands).contains(&"smoothed"), "{:?}", saving.hands);
}

/// Library code does not decide where data lives: `main` names the URL, clap's `--raw` and
/// `--figure` name the files with their defaults, and each step is handed its place. Each place is
/// made whole at `main`.
#[test]
fn rust_clean_the_places_are_made_whole_at_main() {
    let r = rust_clean();
    for (f, kind, artifact) in [
        ("sunspots.download.download", "net", "www.sidc.be"),
        ("sunspots.download.download", "disk_write", "data/raw/sunspots.csv"),
        ("sunspots.plot.Plot.save", "disk_write", "figures/sunspots.png"),
    ] {
        let ts = touches(r, f, kind);
        assert!(ts.iter().any(|t| t.artifact == artifact && !t.fixed && t.by == ["sunspots.main.main"]), "{f} {kind} {artifact}: {ts:#?}");
    }
}

/// The figure is drawn on the area it is handed: `draw` takes plotters' `DrawingArea`, on whatever
/// backend `save` makes. `DrawingArea<DB, Shift>` is a `DrawingArea`, as Python's
/// `ndarray[..]` is an `ndarray`: `Shift`, the kind of coordinates the area has, is a type of the
/// class, not a value `draw` takes or `save` hands it.
#[test]
fn rust_clean_trend_draws_on_the_area_it_is_handed() {
    let r = rust_clean();
    let takes = |f: &str, ty: &str| r.edges.iter().any(|e| e.kind == EdgeKind::Takes && e.src == f && e.dst.starts_with("ext:plotters.") && e.dst.ends_with(ty));
    for f in ["sunspots.plot.Plot.draw", "sunspots.plot.Trend.draw"] {
        assert!(takes(f, ".DrawingArea") && !takes(f, ".Shift"), "{f}");
    }
    let drawing = step(r, "sunspots.plot.Plot.save", "sunspots.plot.Plot.draw");
    assert!(!types(&drawing.hands).iter().any(|ty| ty.ends_with(".Shift")), "{:?}", drawing.hands);
}

// ---------- the port ----------

/// The program written well in each language: the function whose steps a run is, and the steps
/// that download SILSO's file, load it, smooth it and save the figure.
struct Port {
    language: &'static str,
    g: &'static Graph,
    run: &'static str,
    download: &'static str,
    load: &'static str,
    smooth: &'static str,
    save: &'static str,
}

fn ports() -> [Port; 3] {
    [
        Port {
            language: "Python",
            g: clean(),
            run: "sunspots.cli.run",
            download: "sunspots.download.download",
            load: "sunspots.series.load",
            smooth: "sunspots.series.smooth",
            save: "sunspots.plot.Plot.save",
        },
        Port {
            language: "TypeScript",
            g: typescript(),
            run: "typescript.cli.main",
            download: "typescript.download.download",
            load: "typescript.series.load",
            smooth: "typescript.series.smooth",
            save: "typescript.plot.saveSvg",
        },
        Port {
            language: "Rust",
            g: rust_clean(),
            run: "sunspots.main.main",
            download: "sunspots.download.download",
            load: "sunspots.series.load",
            smooth: "sunspots.series.smooth",
            save: "sunspots.plot.Plot.save",
        },
    ]
}

/// What the three clean versions share: no principle has anything to ask, and a run loads the
/// months before it smooths them, handing `smooth` what `load` gives and the run's `Settings`, a
/// type of the program in each language.
#[test]
fn the_port_shares_its_principles_and_its_steps() {
    for p in ports() {
        let fs = check(p.g);
        assert!(fs.is_empty(), "{}: {:#?}", p.language, fs.iter().map(|f| format!("{} {}", f.principle, f.node)).collect::<Vec<_>>());
        let sites = &node(p.g, p.run).sites;
        let at = |to: &str| sites.iter().position(|s| s.to == to).unwrap_or_else(|| panic!("{}: {} has no step to {to}", p.language, p.run));
        assert!(at(p.load) < at(p.smooth), "{}", p.language);
        let pairs = |hs: &[Held]| hs.iter().map(|h| (h.name.clone(), h.ty.clone())).collect::<Vec<_>>();
        let (loaded, handed) = (pairs(&step(p.g, p.run, p.load).gives), pairs(&step(p.g, p.run, p.smooth).hands));
        assert!(!loaded.is_empty() && loaded.iter().all(|h| handed.contains(h)), "{}: {loaded:?} {handed:?}", p.language);
        assert!(handed.iter().any(|(name, ty)| name == "settings" && ty.ends_with(".settings.Settings") && own(p.g, ty)), "{}: {handed:?}", p.language);
    }
}

/// The three run the same steps in the same order: download, load, smooth, save the figure.
#[test]
fn the_port_the_same_steps_in_the_same_order() {
    for p in ports() {
        let sites = &node(p.g, p.run).sites;
        let at: Vec<usize> = [p.download, p.load, p.smooth, p.save]
            .iter()
            .map(|to| sites.iter().position(|s| s.to == *to).unwrap_or_else(|| panic!("{}: {} has no step to {to}", p.language, p.run)))
            .collect();
        assert!(at.windows(2).all(|w| w[0] < w[1]), "{}: {at:?}", p.language);
    }
}

/// The three reach the same places, the figure in the format each writes: the request to SILSO,
/// the raw file written and read again, the directories, and the figure.
#[test]
fn the_port_the_same_places() {
    for p in ports() {
        let named: BTreeSet<_> = outside(p.g, program(p.g)).into_iter().filter(|(_, artifact)| !artifact.is_empty()).collect();
        assert_eq!(in_any_format(named), in_any_format(the_outside()), "{}", p.language);
    }
}

/// The raw file and the figure are options of the run, each with its default: typer's
/// `raw: Path = RAW`, parseArgs' `values.raw ?? RAW`, clap's `#[arg(long, default_value = RAW)]`.
/// An option is the run's input whatever parser reads it, as a parameter of a command is, so the
/// three reach the two files the same way: the step that writes or reads a file is handed it, the
/// place is made whole at the run, and the code does not alone decide it, since the command line
/// may name another. The run itself opens neither.
#[test]
fn the_port_reaches_the_raw_file_and_the_figure_through_its_options() {
    let raw = "data/raw/sunspots.csv";
    let named = |t: &&Touch| t.artifact == raw || t.artifact.starts_with("figures/sunspots.");
    let want: BTreeSet<(&str, String, String, bool, bool)> = [("download", "disk_write", raw), ("load", "disk_read", raw), ("save", "disk_write", "figures/sunspots.*")]
        .into_iter()
        .map(|(step, kind, artifact)| (step, kind.to_string(), artifact.to_string(), false, true))
        .collect();
    for p in ports() {
        let reached: BTreeSet<(&str, String, String, bool, bool)> = [("download", p.download), ("load", p.load), ("save", p.save)]
            .into_iter()
            .flat_map(|(step, f)| node(p.g, f).touches.iter().filter(named).map(move |t| (step, t.kind_name().to_string(), any_format(&t.artifact), t.fixed, t.by == [p.run] && t.users_of.is_empty())))
            .collect();
        assert_eq!(reached, want, "{}", p.language);
        assert!(!node(p.g, p.run).touches.iter().any(|t| named(&t)), "{}", p.language);
    }
}

/// Where the three differ, each the language's doing:
///
/// - The values. pandas is how Python keeps a table, so `load` and `smooth` give a `DataFrame`, a
///   type of the library. TypeScript and Rust keep a month as a record the program declares:
///   `load` gives `Month`s and `smooth` gives `Smoothed` means.
/// - The figure. matplotlib makes a `Figure`, which `Trend.draw` gives and `Plot.save` writes
///   (`savefig`). d3 computes the scales and the lines, so `trend` writes the SVG document itself
///   and gives it as a `string`, which `saveSvg` writes (`writeFile`). plotters draws on the area
///   of a backend, so `Trend::draw` gives no value: `save` makes the backend that writes the file
///   (`BitMapBackend::new`) and hands `draw` its area. Opening `Plot.save` shows it: in Python
///   `draw` gives `save` a `Figure`, in Rust `save` hands `draw` a `DrawingArea`, the type Rust
///   fixes for the `let` that holds it by the parameter it is handed to.
/// - Errors. A Rust step that may fail says so in what it gives (`Result<_, Error>`, with the
///   program's own error), every step but `smooth`, which cannot fail, and none of them throws.
///   TypeScript says it only of the steps that wait (`Promise`); `download` and `trend` throw as
///   well, which no signature says. Python's signatures say it of no step.
#[test]
fn the_port_differs_by_language() {
    let [py, ts, rs] = ports();
    let gives = |p: &Port, to: &str| types(&step(p.g, p.run, to).gives).into_iter().map(String::from).collect::<Vec<_>>();

    // the values
    for to in [py.load, py.smooth] {
        assert_eq!(gives(&py, to), ["ext:pandas.DataFrame"], "{to}");
    }
    assert!(!own(py.g, "ext:pandas.DataFrame"));
    for (p, scope) in [(&ts, "typescript"), (&rs, "sunspots")] {
        let (month, smoothed) = (format!("{scope}.series.Month"), format!("{scope}.series.Smoothed"));
        assert_eq!(gives(p, p.load), [month.clone()], "{}", p.language);
        assert_eq!(gives(p, p.smooth), [smoothed.clone()], "{}", p.language);
        assert!(own(p.g, &month) && own(p.g, &smoothed), "{}", p.language);
    }

    // the figure
    let wrote = |p: &Port, op: &str| touches(p.g, p.save, "disk_write").iter().any(|t| t.op == op);
    assert!(edge(py.g, EdgeKind::Returns, "sunspots.plot.Trend.draw", "ext:matplotlib.figure.Figure"));
    assert!(wrote(&py, "savefig"));
    let trend = node(ts.g, "typescript.plot.trend").returns.as_ref().unwrap();
    assert_eq!((trend.written.as_str(), trend.value.as_str()), ("string", "string"));
    assert!(wrote(&ts, "writeFile"));
    let draw = node(rs.g, "sunspots.plot.Trend.draw").returns.as_ref().unwrap();
    assert_eq!((draw.written.as_str(), draw.value.as_str()), ("Result<(), Error>", ""));
    assert!(wrote(&rs, "new"));
    let pairs = |hs: &[Held]| hs.iter().map(|h| (h.name.clone(), h.ty.clone())).collect::<Vec<_>>();
    let drawing = |p: &Port| step(p.g, p.save, "sunspots.plot.Plot.draw");
    assert_eq!(pairs(&drawing(&py).gives), [("figure".to_string(), "ext:matplotlib.figure.Figure".to_string())]);
    assert_eq!(names(&drawing(&rs).hands), ["area", "series", "smoothed"]);
    assert_eq!(types(&drawing(&rs).hands), ["ext:plotters.prelude.DrawingArea", "sunspots.series.Month", "sunspots.series.Smoothed"]);
    assert!(drawing(&rs).gives.is_empty());
    let made = node(rs.g, rs.save).sites.iter().find(|s| s.op == "into_drawing_area").expect("save makes the area");
    assert_eq!(pairs(&made.gives), [("area".to_string(), "ext:plotters.prelude.DrawingArea".to_string())]);

    // errors
    let fails = |p: &Port, f: &str| node(p.g, f).returns.as_ref().map_or(false, |r| r.fails);
    let raises = |p: &Port, f: &str| node(p.g, f).raises;
    for f in [rs.download, rs.load, rs.save, "sunspots.plot.Trend.draw"] {
        let written = &node(rs.g, f).returns.as_ref().unwrap().written;
        assert!(fails(&rs, f) && written.starts_with("Result<") && written.ends_with(", Error>"), "{f}: {written}");
    }
    assert!(own(rs.g, "sunspots.error.Error"));
    assert!(!fails(&rs, rs.smooth));
    for f in [rs.download, rs.load, rs.smooth, rs.save, "sunspots.plot.Trend.draw"] {
        assert!(!raises(&rs, f), "{f}");
    }
    let effects = |f: &str| (fails(&ts, f), raises(&ts, f));
    assert_eq!(
        [ts.download, ts.load, ts.smooth, "typescript.plot.trend", ts.save].map(effects),
        [(true, true), (true, false), (false, false), (false, true), (true, false)]
    );
    for f in [py.download, py.load, py.smooth, py.save, "sunspots.plot.Trend.draw"] {
        assert!(!fails(&py, f) && !raises(&py, f), "{f}");
    }
}
