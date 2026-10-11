//! A library the project uses is read as code, as far as the project's calls reach into it.

use std::path::PathBuf;

use atlas_engine::{analyze, principles, EdgeKind, Graph};

fn fixture() -> Graph {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixture-lib");
    analyze(&repo, None, None).expect("fixture parses")
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
    assert!(load.touches.iter().any(|t| t.kind == "disk_read"), "{:?}", load.touches);
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
