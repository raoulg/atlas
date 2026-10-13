//! A repository of several parts: each manifest roots one, read in its own language, and a
//! route one part serves joins it to the part that names the route.

use std::path::PathBuf;

use atlas_engine::parts::{analyze_parts, analyze_repo, roots};
use atlas_engine::repo::Repo;
use atlas_engine::{EdgeKind, EntryKind, Graph, NodeKind, Options};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixture-parts")
}

fn shop() -> Graph {
    analyze_repo(&[fixture()], &Options::default()).expect("the two-part fixture parses")
}

fn label(g: &Graph, id: &str) -> String {
    g.node(id).and_then(|n| n.entry.as_ref()).map(|e| e.label.clone()).unwrap_or_default()
}

/// A manifest roots a part in its language; tests' fixtures and a workspace's member crates
/// are not parts of their own.
#[test]
fn every_manifest_roots_a_part() {
    let found: Vec<(String, &str)> = roots(&Repo::read(&fixture(), &[]).unwrap(), &Options::default()).into_iter().map(|(d, l)| (d.file_name().unwrap().to_string_lossy().to_string(), l)).collect();
    assert_eq!(found, [("fixture-parts".to_string(), "python"), ("web".to_string(), "typescript")]);
    let atlas = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    assert_eq!(roots(&Repo::read(&atlas, &[]).unwrap(), &Options::default()).len(), 1, "the workspace is one root; engine/tests holds data");
}

fn manifests() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixture-manifests")
}

/// A manifest roots a part, at any depth, unless a part around it reads it: a Python project
/// reads only its packages, so each member of a uv workspace is a part; a Cargo workspace reads
/// its member (`crates/keeper`) and not the crate beside it that is none (`crates/fuzz`).
#[test]
fn a_manifest_no_part_reads_roots_one() {
    let repo = Repo::read(&manifests(), &[]).unwrap();
    let found: Vec<(String, &str)> = roots(&repo, &Options::default())
        .into_iter()
        // a part's directory as a path relative to the repository, written with `/` on every system
        .map(|(d, l)| (d.strip_prefix(manifests()).unwrap().to_string_lossy().replace('\\', "/"), l))
        .collect();
    let want = [
        ("", "python"),
        ("crates", "rust"),
        ("crates/fuzz", "rust"),
        ("deep/a/b/c/d/e", "python"),
        ("members/hatched", "python"),
        ("members/poems", "python"),
        ("members/settled", "python"),
        ("members/single", "python"),
        ("members/spaced", "python"),
        ("members/zoo", "python"),
    ];
    assert_eq!(found, want.map(|(d, l)| (d.to_string(), l)));
}

/// A Python project's package is the one its manifest names, and the graph is named after it:
/// by the project's name (`zoo-keeper` is `src/zoo_keeper`, though `src/aardvark` sorts first),
/// a namespace package without an `__init__.py` by its name (`spaced`), by Hatch's `packages`
/// (`lib/hatched`, though `aaa` sorts first), setuptools' `packages` in its `package-dir`
/// (`code/settled`) and Poetry's `packages` (`source/verse`); a module of one file by the
/// project's name (`solo.py` in `members/single`). Every other package beside it is a part after
/// it. The workspace's root holds no code and is no part.
#[test]
fn the_manifest_names_the_package() {
    let graphs = analyze_parts(&[manifests()], &Options::default()).unwrap();
    let found: Vec<(String, Vec<String>)> = graphs.iter().map(|g| (g.root.clone(), g.parts.iter().map(|p| p.id.clone()).collect())).collect();
    let want: [(&str, &[&str]); 9] = [
        ("keeper", &["keeper"]),
        ("keeper-fuzz", &["keeper-fuzz"]),
        ("deep", &["deep"]),
        ("hatched", &["hatched", "aaa"]),
        ("verse", &["verse"]),
        ("settled", &["settled"]),
        ("solo", &["solo"]),
        ("spaced", &["spaced"]),
        ("zoo_keeper", &["zoo_keeper", "aardvark", "benchmarks"]),
    ];
    assert_eq!(found, want.map(|(root, parts)| (root.to_string(), parts.iter().map(|p| p.to_string()).collect::<Vec<_>>())));
}

/// A package beside the package is read as the package it is: its modules are named by their
/// import path and a call into it is resolved; a script beside them stays a script.
#[test]
fn a_package_beside_the_package_is_read() {
    let g = analyze_repo(&[manifests()], &Options::default()).unwrap();
    assert!(g.edges.iter().any(|e| e.kind == EdgeKind::Calls && e.src == "zoo_keeper.keep.keep" && e.dst == "aardvark.feed.feed"));
    assert!(g.edges.iter().any(|e| e.kind == EdgeKind::Calls && e.src == "benchmarks.timing.keep_many" && e.dst == "zoo_keeper.keep.keep"));
    assert_eq!(g.node("aardvark.feed").unwrap().part, "aardvark");
    assert_eq!(g.node("benchmarks.timing").unwrap().entry, None, "a module of a package is no script");
    assert_eq!(g.node("scripts.morning").and_then(|n| n.entry.as_ref()).map(|e| e.kind), Some(EntryKind::Script));
    assert_eq!(g.node("scripts.morning").unwrap().part, "zoo_keeper", "a script belongs to the package the project is named after");
    assert!(g.node("settled.inner").is_some() && g.node("spaced.tool").is_some());
}

/// A second package is the project's own as the first is: an import of it is an `imports` edge
/// (`zoo_keeper.keep` imports `aardvark`, `aardvark` its own `feed`, `aardvark.feed` the first
/// package's `Animal`), a name it re-exports is followed to where it is defined (`keep` returns
/// `aardvark.feed.Meal`, not a library's `aardvark.Meal`), and its root offers its doors: the
/// names in its `__all__` (`Meal`, not `feed`), else its public names (`aaa.first`).
#[test]
fn a_second_package_is_the_projects_own() {
    let g = analyze_repo(&[manifests()], &Options::default()).unwrap();
    let edge = |kind: EdgeKind, src: &str, dst: &str| g.edges.iter().any(|e| e.kind == kind && e.src == src && e.dst == dst);
    assert!(edge(EdgeKind::Imports, "zoo_keeper.keep", "aardvark"), "`from aardvark import Meal`");
    assert!(edge(EdgeKind::Imports, "zoo_keeper.keep", "aardvark.feed.feed"), "`from aardvark.feed import feed`");
    assert!(edge(EdgeKind::Imports, "aardvark", "aardvark.feed.Meal"), "the second package imports its own module");
    assert!(edge(EdgeKind::Imports, "aardvark.feed", "zoo_keeper.Animal"), "the second package imports from the first");
    assert!(edge(EdgeKind::Returns, "zoo_keeper.keep.keep", "aardvark.feed.Meal"));
    assert!(g.node("ext:aardvark.Meal").is_none(), "a package of the project's own is no library");
    let export = |id: &str| g.node(id).and_then(|n| n.entry.as_ref()).filter(|e| e.kind == EntryKind::Export).map(|e| e.label.clone());
    assert_eq!(export("aardvark.feed.Meal").as_deref(), Some("Meal"), "named in `__all__`");
    assert_eq!(export("aardvark.feed.feed"), None, "not named in `__all__`");
    assert_eq!(export("aaa.first").as_deref(), Some("first"), "a root without `__all__` offers its public names");
}

/// A project whose name names a module of one file (`solo.py`) is a package of that one module:
/// the package the graph and its part are named after, with no entry of its own, offering its
/// public names; a script beside it imports from it.
#[test]
fn a_single_module_is_a_package() {
    let g = analyze_repo(&[manifests()], &Options::default()).unwrap();
    let solo = g.node("solo").unwrap();
    assert!(solo.kind == NodeKind::Module && solo.is_pkg && solo.entry.is_none(), "a package, not a script");
    assert_eq!(solo.part, "solo");
    let entry = |id: &str| g.node(id).and_then(|n| n.entry.as_ref()).map(|e| (e.kind, e.label.clone()));
    assert_eq!(entry("solo.tune"), Some((EntryKind::Export, "tune".to_string())));
    assert_eq!(entry("solo._octave"), None);
    assert_eq!(entry("_root.run"), Some((EntryKind::Script, "run.py".to_string())));
    assert!(g.edges.iter().any(|e| e.kind == EdgeKind::Imports && e.src == "_root.run" && e.dst == "solo.tune"));
    assert!(g.node("_root.solo").is_none(), "the module is not read as a script as well");
}

/// The parts side by side in one graph, every node in its part; checked part by part.
#[test]
fn the_parts_are_one_graph() {
    let g = shop();
    let parts: Vec<(&str, &str)> = g.parts.iter().map(|p| (p.id.as_str(), p.language.as_str())).collect();
    assert_eq!(parts, [("shop", "python"), ("web", "typescript")]);
    assert_eq!(g.language, "", "read in two languages");
    assert_eq!(g.node("shop.api.item").unwrap().part, "shop");
    assert_eq!(g.node("web.lib.client.item").unwrap().part, "web");
    let each: Vec<String> = analyze_parts(&[fixture()], &Options::default()).unwrap().into_iter().map(|g| g.language).collect();
    assert_eq!(each, ["python", "typescript"]);
}

/// A route is the path a client asks for: under the prefix its app is mounted at, and the
/// prefixes its router is declared and included with.
#[test]
fn routes_are_served_at_their_full_path() {
    let g = shop();
    assert_eq!(label(&g, "shop.api.item"), "GET /api/items/{item_id}", "`app.mount(\"/api\", items)`");
    assert_eq!(label(&g, "shop.serve.create_order"), "POST /v1/orders", "`include_router(router, prefix=\"/v1\")` and `APIRouter(prefix=\"/orders\")`");
    assert_eq!(label(&g, "shop.serve.health"), "GET /health");
    assert_eq!(g.node("web.routes.health").unwrap().entry.as_ref().unwrap().kind, EntryKind::Route);
}

/// A route rests on the web framework its decorator comes from, with the method it serves and
/// its path: by a decorator named for the method (FastAPI's `app.get`, Flask's `app.post`,
/// Litestar's `get`), or handed the methods (Flask's `methods=`, Litestar's `http_method=`, GET
/// when none), at `/` when handed no path. A `get` of the project's own registers nothing.
#[test]
fn each_web_framework_registers_its_routes() {
    let g = shop();
    let kind = |id: &str| g.node(id).and_then(|n| n.entry.as_ref()).map(|e| e.kind);
    assert_eq!(kind("shop.admin.notes"), Some(EntryKind::Route));
    assert_eq!(label(&g, "shop.admin.notes"), "GET|POST /notes", "Flask, handed its methods");
    assert_eq!(label(&g, "shop.admin.index"), "GET /", "Flask, handed none");
    assert_eq!(label(&g, "shop.admin.reset"), "POST /reset", "Flask, named for its method");
    assert_eq!(kind("shop.admin.cached"), None, "the project's own `cache.get`");
    assert_eq!(label(&g, "shop.stock.stock"), "GET /stock/{item_id:str}", "Litestar's `get`");
    assert_eq!(label(&g, "shop.stock.overview"), "GET /", "Litestar's `get()` serves `/`");
    assert_eq!(label(&g, "shop.stock.restock"), "POST|PUT /restock", "Litestar's `route`, handed its methods");
    // prefixes as each framework gives them
    assert_eq!(label(&g, "shop.board.pins"), "GET /board/pins", "a blueprint's own `url_prefix`");
    assert_eq!(label(&g, "shop.board.moved_notes"), "GET /new/notes", "`register_blueprint(.., url_prefix=..)` stands in for the blueprint's own");
    assert_eq!(label(&g, "shop.board.levels"), "GET /v2/levels", "a handler a Litestar `Router(path=..)` is handed");
    assert_eq!(label(&g, "shop.board.Shelves.shelf"), "GET /v2/shelves/{shelf:int}", "a controller's `path`, under its router's");
}

/// A request one part makes to a path another part's route serves is a request to its handler,
/// by method and path; nothing asks for `/health`, so neither of the parts' pages of that name
/// is requested.
#[test]
fn a_client_requests_what_a_server_serves() {
    let g = shop();
    let requests: Vec<(&str, &str, &str)> = g
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Requests)
        .map(|e| (e.src.as_str(), e.dst.as_str(), e.via.as_deref().unwrap_or("")))
        .collect();
    assert!(requests.contains(&("web.lib.client.item", "shop.api.item", "/api/items/{item_id}")), "{requests:?}");
    assert!(requests.contains(&("web.lib.client.order", "shop.serve.create_order", "/v1/orders")), "{requests:?}");
    assert!(!requests.iter().any(|(_, dst, _)| *dst == "shop.serve.health"), "`/health` is web's own page: {requests:?}");
}
