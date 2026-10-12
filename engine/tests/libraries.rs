//! A library the project uses is read as code, as far as the project's calls reach into it.

use std::path::PathBuf;

use atlas_engine::{analyze, principles, EdgeKind, Graph, Options};

fn fixture() -> Graph {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixture-lib");
    analyze(&repo, &Options::default()).expect("fixture parses")
}

fn edge(g: &Graph, src: &str, dst: &str, kind: EdgeKind) -> bool {
    g.edges.iter().any(|e| e.src == src && e.dst == dst && e.kind == kind)
}

/// The project's calls go into the library's own functions, which read the disk as the
/// project's would; what the library holds but the project does not reach is not drawn.
#[test]
fn a_library_is_read_as_far_as_the_project_reaches() {
    let g = fixture();
    let node = |id: &str| g.nodes.iter().find(|n| n.id == id);
    let load = node("plotkit.data.load").expect("the function the app calls is read");
    assert!(load.library, "a library's function is marked as one");
    assert!(load.touches.iter().any(|t| t.kind_name() == "disk_read"), "{:?}", load.touches);
    assert!(edge(&g, "app.main.main", "plotkit.data.load", EdgeKind::Calls));
    assert!(node("plotkit.data.unused").is_none(), "a function nothing reaches is not drawn");
    // a call on a subclass into its base's method reaches the subclass's override
    assert!(node("plotkit.figures.LinePlot.build").is_some());
    assert!(node("plotkit.figures.BarPlot").is_none(), "a class the project never builds is not drawn");
    // a class handed to code Atlas reads is built there
    assert!(edge(&g, "plotkit.data.standard", "plotkit.data.Double", EdgeKind::Instantiates));
}

/// What a library calls in another library stays that library's call, and a package with
/// compiled code is not read.
#[test]
fn only_the_libraries_the_project_calls_are_read() {
    let g = fixture();
    assert!(g.nodes.iter().all(|n| !n.id.starts_with("otherlib")), "a library of a library is not read");
    assert!(g.nodes.iter().all(|n| !n.id.starts_with("compiled")), "a package with compiled code is not read");
}

/// A library's code is not the project's to change: `atlas check` does not judge it.
#[test]
fn a_library_is_not_judged() {
    let g = fixture();
    let findings = principles::check(&g);
    assert!(findings.iter().all(|f| !f.node.starts_with("plotkit")), "{:?}", findings.iter().map(|f| &f.node).collect::<Vec<_>>());
}

/// A body's calls are its steps, in the order they run: what each reaches, the class of the
/// value a method runs on, what it is handed and gives back by the names that hold them, and a
/// library's operation Atlas does not read where the body names its result.
#[test]
fn a_body_lists_its_calls() {
    let g = fixture();
    let main = g.nodes.iter().find(|n| n.id == "app.main.main").unwrap();
    let to: Vec<&str> = main.sites.iter().map(|s| if s.to.is_empty() { s.op.as_str() } else { s.to.as_str() }).collect();
    assert_eq!(
        to,
        ["plotkit.data.load", "plotkit.data.lines", "plotkit.data.standard", "plotkit.figures.Settings", "plotkit.figures.LinePlot", "plotkit.figures.BasePlot.plot", "fast"],
        "{:#?}",
        main.sites
    );
    let held = |ty: &str, name: &str| atlas_engine::Held::new(ty, name);
    let settings = &main.sites[3];
    assert_eq!(settings.gives, [held("plotkit.figures.Settings", "settings")]);
    let plot = &main.sites[5];
    assert_eq!(plot.on, "plotkit.figures.LinePlot", "a method called on a subclass's value runs on that subclass");
    assert!(main.sites[4].hands.contains(&held("plotkit.figures.Settings", "settings")));
    // a value whose type ty does not know is followed by the name that holds it
    let fast = &main.sites[6];
    assert_eq!((fast.hands.clone(), fast.gives.clone()), (vec![held("plotkit.figures.Settings", "settings")], vec![held("", "shown")]));
    // in the library's own body, a call on `self` says so
    let base = g.nodes.iter().find(|n| n.id == "plotkit.figures.BasePlot.plot").unwrap();
    assert!(base.sites.iter().any(|s| s.to == "plotkit.figures.BasePlot.build" && s.on_receiver), "{:#?}", base.sites);
}
