//! A library the project uses is read as code, as far as the project's calls reach into it.

use std::path::PathBuf;

use atlas_engine::{analyze, principles, EdgeKind, Graph, Options};

fn fixture() -> Graph {
    rounds(1)
}

/// The fixture, read `libraries` rounds into its libraries.
fn rounds(libraries: usize) -> Graph {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixture-lib");
    analyze(&repo, &Options { libraries, ..Options::default() }).expect("fixture parses")
}

fn edge(g: &Graph, src: &str, dst: &str, kind: EdgeKind) -> bool {
    g.edges.iter().any(|e| e.src == src && e.dst == dst && e.kind == kind)
}

/// The project's calls go into the library's own functions, which read the disk as the
/// project's would; what the library holds but the project does not reach is not drawn. A
/// library exists as far as the project reaches it: each round one call further into it, with the
/// types those functions take, give and run on.
#[test]
fn a_library_is_read_as_far_as_the_project_reaches() {
    let (g, two) = (fixture(), rounds(2));
    let node = |id: &str| g.nodes.iter().find(|n| n.id == id);
    let load = node("plotkit.data.load").expect("the function the app calls is read");
    assert!(load.library, "a library's function is marked as one");
    // the library as the dependency it is: the directory its code is read from, which its files lie under
    assert!(load.file.starts_with(&g.libraries["plotkit"].dir), "{:?}", g.libraries);
    assert!(load.touches.iter().any(|t| t.kind_name() == "disk_read"), "{:?}", load.touches);
    assert!(edge(&g, "app.main.main", "plotkit.data.load", EdgeKind::Calls));
    assert!(node("plotkit.data.unused").is_none(), "a function nothing reaches is not drawn");
    assert!(node("plotkit.figures.BarPlot").is_none(), "a class the project never builds is not drawn");
    assert!(node("plotkit.data.Pipeline").is_some(), "the type a function the project calls gives");
    // a call on a subclass into its base's method reaches the subclass's override, one call further
    assert!(node("plotkit.figures.LinePlot.build").is_none() && two.node("plotkit.figures.LinePlot.build").is_some());
    // a class handed to code Atlas reads is built there, one call further
    assert!(node("plotkit.data.Double").is_none() && edge(&two, "plotkit.data.standard", "plotkit.data.Double", EdgeKind::Instantiates));
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
        [
            "plotkit.data.load",
            "plotkit.data.lines",
            "plotkit.data.standard",
            "plotkit.figures.Settings",
            "plotkit.figures.LinePlot",
            "plotkit.figures.BasePlot.plot",
            "fast"
        ],
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

/// A project written to a directory of its own with the library it calls beside it, where ty
/// finds it as it finds an environment's, read `libraries` rounds deep, and removed.
fn written(name: &str, files: &[(&str, &str)], libraries: usize) -> Graph {
    let dir = std::env::temp_dir().join(format!("atlas-libraries-{name}-{libraries}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let manifest =
        format!("[project]\nname = \"{name}\"\nversion = \"0.1.0\"\n\n[tool.ty.environment]\nextra-paths = [\"site-packages\"]\n");
    for (path, text) in std::iter::once(&("pyproject.toml", manifest.as_str())).chain(files) {
        let file = dir.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }
    let g = analyze(&dir, &Options { libraries, ..Options::default() });
    let _ = std::fs::remove_dir_all(&dir);
    g.expect("the project parses")
}

/// What lies beyond the rounds is glued into the library's function at the border: the project
/// calls `save`, whose file is written two calls deeper, in functions the graph does not hold.
/// `save` is one arrow that writes the file the project hands it, drawn at its own call that
/// leads there, so the project's route reaches the place. Where the rounds reach the function
/// that writes, the touch is that function's own, not `save`'s. Of the library's file the graph
/// keeps the text its nodes show, each line where it was.
#[test]
fn what_lies_beyond_the_rounds_is_the_border_functions() {
    let padding = "# a line of the library's own\n".repeat(70);
    let store = format!(
        "from pathlib import Path\n\n\ndef save(path: str, text: str) -> None:\n    prepare(path, text)\n\n\n{padding}\ndef prepare(path: str, text: str) -> None:\n    write(path, text.strip())\n\n\ndef write(path: str, text: str) -> None:\n    Path(path).write_text(text)\n"
    );
    let files: &[(&str, &str)] = &[
        ("src/deep/__init__.py", ""),
        ("src/deep/main.py", "from deepkit.store import save\n\n\ndef main() -> None:\n    save(\"out.txt\", \"rows\")\n"),
        ("site-packages/deepkit/__init__.py", ""),
        ("site-packages/deepkit/store.py", &store),
    ];
    let (one, three) = (written("deep", files, 1), written("deep", files, 3));
    assert!(
        one.node("deepkit.store.prepare").is_none() && one.node("deepkit.store.write").is_none(),
        "what lies beyond the rounds is not drawn"
    );
    let save = one.node("deepkit.store.save").expect("the function the project calls");
    let wrote = save
        .touches
        .iter()
        .find(|t| t.kind_name() == "disk_write")
        .unwrap_or_else(|| panic!("the border writes what its code beyond writes: {:?}", save.touches));
    assert_eq!(wrote.artifact, "out.txt", "at the path the project hands it: {wrote:?}");
    let call = save.sites.iter().find(|s| s.to == "deepkit.store.prepare").expect("save's call into what lies beyond");
    assert_eq!(wrote.line, call.line, "drawn at the border's call that leads there");
    // the project's route reaches the place
    let reached = one.reach("deep.main.main");
    assert!(reached.contains(&save.id));
    let in_route: Vec<(&str, &str)> =
        save.touches_in("deep.main.main", &|f| reached.contains(f)).into_iter().map(|t| (t.kind_name(), t.artifact.as_str())).collect();
    assert!(in_route.contains(&("disk_write", "out.txt")), "{in_route:?}");
    // `main` hands the library its own `out.txt`: where that lives is `main`'s decision
    let found = principles::check(&one);
    assert!(found.iter().any(|f| f.node == "deep.main.main" && f.principle == "places"), "{found:#?}");
    assert!(found.iter().all(|f| f.node != "deep.main.main" || f.principle == "places"), "main leaves a file behind");
    // the rounds reach the function that writes: the touch is its own
    assert!(three.node("deepkit.store.write").unwrap().touches.iter().any(|t| t.kind_name() == "disk_write"));
    assert!(
        three.node("deepkit.store.save").unwrap().touches.iter().all(|t| t.kind_name() != "disk_write"),
        "a function's own arrow is not what it reaches"
    );
    // the text the graph keeps of the library's file
    let text: Vec<&str> = one.files[&save.file].split('\n').collect();
    assert_eq!(text.len(), store.split('\n').count(), "each line where it was");
    assert!(
        text.contains(&"def save(path: str, text: str) -> None:") && !text.contains(&"def write(path: str, text: str) -> None:"),
        "{text:?}"
    );
}
