//! A repository of several parts: each manifest roots one, read in its own language, and a
//! route one part serves joins it to the part that names the route.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use atlas_engine::parts::{analyze_parts, analyze_repo, roots};
use atlas_engine::principles::check;
use atlas_engine::repo::Repo;
use atlas_engine::{analyze, EdgeKind, EntryKind, Graph, Kind, Options};

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
    let found: Vec<(String, &str)> = roots(&Repo::read(&fixture(), &[]).unwrap(), &Options::default())
        .into_iter()
        .map(|(d, l)| (d.file_name().unwrap().to_string_lossy().to_string(), l))
        .collect();
    assert_eq!(found, [("fixture-parts".to_string(), "python"), ("web".to_string(), "typescript")]);
    let atlas = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    assert_eq!(
        roots(&Repo::read(&atlas, &[]).unwrap(), &Options::default()).len(),
        1,
        "the workspace is one root; engine/tests holds data"
    );
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
    let found: Vec<(String, Vec<String>)> =
        graphs.iter().map(|g| (g.root.clone(), g.parts.iter().map(|p| p.id.clone()).collect())).collect();
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
    assert!(matches!(&solo.kind, Kind::Module(m) if m.is_pkg) && solo.entry.is_none(), "a package, not a script");
    assert_eq!(solo.part, "solo");
    let entry = |id: &str| g.node(id).and_then(|n| n.entry.as_ref()).map(|e| (e.kind, e.label.clone()));
    assert_eq!(entry("solo.tune"), Some((EntryKind::Export, "tune".to_string())));
    assert_eq!(entry("solo._octave"), None);
    assert_eq!(entry("_root.run"), Some((EntryKind::Script, "run.py".to_string())));
    assert!(g.edges.iter().any(|e| e.kind == EdgeKind::Imports && e.src == "_root.run" && e.dst == "solo.tune"));
    assert!(g.node("_root.solo").is_none(), "the module is not read as a script as well");
}

/// Every file the Python tools install or build a project from says as much as a `pyproject.toml`
/// does that its directory holds a Python project (issue #1): a repository of two packages under
/// `src` without an `__init__.py` of its own and a script under `scripts`, run as `python
/// scripts/main.py`, has that script as its entry, the same whichever of `pyproject.toml`,
/// `requirements.txt`, `setup.py`, `setup.cfg` and `Pipfile` it keeps at its root.
#[test]
fn every_project_file_makes_the_scripts_beside_the_packages_entries() {
    let files: [(&str, &str); 5] = [
        ("pyproject.toml", "[project]\nname = \"news\"\nversion = \"0.1.0\"\n"),
        ("requirements.txt", "requests==2.32.3\n"),
        ("setup.py", "from setuptools import setup\n\nsetup(name=\"news\")\n"),
        ("setup.cfg", "[metadata]\nname = news\n"),
        ("Pipfile", "[packages]\nrequests = \"*\"\n"),
    ];
    let entries = |manifest: &str, text: &str| -> Vec<(EntryKind, String)> {
        let dir = std::env::temp_dir().join(format!("atlas-parts-project-{}-{}", manifest.replace('.', "-"), std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let written = [
            (manifest, text),
            ("src/archiving/__init__.py", ""),
            ("src/archiving/store.py", "def archive(text: str) -> str:\n    return text\n"),
            ("src/enrichment/__init__.py", ""),
            ("src/enrichment/tag.py", "def tag(text: str) -> str:\n    return text.upper()\n"),
            (
                "scripts/main.py",
                "from archiving.store import archive\nfrom enrichment.tag import tag\n\nif __name__ == \"__main__\":\n    print(archive(tag(\"x\")))\n",
            ),
        ];
        for (path, body) in written {
            let file = dir.join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, body).unwrap();
        }
        let g = analyze_repo(&[dir.clone()], &Options::default());
        let _ = std::fs::remove_dir_all(&dir);
        let g = g.unwrap_or_else(|e| panic!("{manifest}: {e}"));
        let mut out: Vec<(EntryKind, String)> = g
            .nodes
            .iter()
            .filter_map(|n| n.entry.as_ref())
            .filter(|e| e.kind == EntryKind::Script)
            .map(|e| (e.kind, e.label.clone()))
            .collect();
        out.sort_by(|a, b| a.1.cmp(&b.1));
        out
    };
    for (manifest, text) in files {
        assert_eq!(entries(manifest, text), [(EntryKind::Script, "scripts/main.py".to_string())], "beside a {manifest}");
    }
}

/// A language names every file that roots a part in it (issue #161): a repository of scripts with no
/// package, beside a `requirements.txt` and nothing else, is a Python project, one part, and its scripts
/// are its entries; one below it beside a project file of its own is a part of its own.
#[test]
fn a_repository_of_scripts_beside_a_requirements_file_is_a_part() {
    let dir = std::env::temp_dir().join(format!("atlas-parts-requirements-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let written = [
        ("requirements.txt", "requests==2.32.3\n"),
        ("fetch.py", "import sys\n\n\ndef fetch(url: str) -> str:\n    return url\n\n\nif __name__ == \"__main__\":\n    print(fetch(sys.argv[1]))\n"),
        ("tidy.py", "def tidy(text: str) -> str:\n    return text.strip()\n\n\nif __name__ == \"__main__\":\n    print(tidy(\" x \"))\n"),
        ("tools/setup.cfg", "[metadata]\nname = tools\n"),
        ("tools/lint.py", "if __name__ == \"__main__\":\n    print(\"lint\")\n"),
    ];
    for (path, body) in written {
        let file = dir.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, body).unwrap();
    }
    let found: Vec<(String, &str)> = roots(&Repo::read(&dir, &[]).unwrap(), &Options::default())
        .into_iter()
        .map(|(d, l)| (d.strip_prefix(&dir).unwrap().to_string_lossy().replace('\\', "/"), l))
        .collect();
    let g = analyze_repo(&[dir.clone()], &Options::default());
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(found, [(String::new(), "python"), ("tools".to_string(), "python")], "each project file roots a part");
    let g = g.expect("a repository of scripts beside a requirements.txt is read");
    let mut scripts: Vec<(String, String)> = g
        .nodes
        .iter()
        .filter(|n| n.entry.as_ref().is_some_and(|e| e.kind == EntryKind::Script))
        .map(|n| (n.part.clone(), n.entry.as_ref().unwrap().label.clone()))
        .collect();
    scripts.sort();
    let part = |label: &str| scripts.iter().find(|(_, l)| l.ends_with(label)).map(|(p, _)| p.clone());
    assert_eq!(scripts.len(), 3, "every script is an entry: {scripts:?}");
    assert!(part("fetch.py").is_some() && part("fetch.py") == part("tidy.py"), "the scripts at the root are one part's: {scripts:?}");
    assert_ne!(part("lint.py"), part("fetch.py"), "the scripts below a project file of their own are another part's: {scripts:?}");
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

fn twins() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixture-twins")
}

fn makefiles() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixture-makefiles")
}

/// A part's Makefile is its own, a unit of its part with its targets. Two Makefiles are each held
/// under where their part lies (`orders/Makefile`), and so is a target both name (`run`); a target
/// one names keeps its id, and a target runs its own Makefile's. A directory with two manifests
/// (`shop/`: `pyproject.toml`, `package.json`) is two parts, each read in its manifest's language,
/// and its Makefile is one, the directory's.
#[test]
fn each_part_has_its_own_makefile() {
    let g = analyze_repo(&[makefiles()], &Options::default()).unwrap();
    let parts: Vec<String> = g.parts.iter().map(|p| format!("{} {}", p.id, p.language)).collect();
    assert_eq!(parts, ["billing python", "orders python", "shop/python/shop python", "shop/typescript/shop typescript"]);
    let mut make: Vec<(&str, &str, &str)> = g.make.iter().map(|t| (t.id.as_str(), t.module.as_str(), t.part.as_str())).collect();
    make.sort();
    assert_eq!(
        make,
        [
            ("billing/make:run", "billing/Makefile", "billing"),
            ("make:audit", "billing/Makefile", "billing"),
            ("make:test", "orders/Makefile", "orders"),
            ("orders/make:run", "orders/Makefile", "orders"),
            ("shop/make:run", "shop/Makefile", "shop/python/shop"),
        ]
    );
    assert_eq!(g.make.iter().find(|t| t.id == "make:audit").unwrap().calls, ["billing/make:run", "billing.main.main"]);
    let tops = atlas_engine::flow::units(&g).tops;
    let makefile_of = |part: &str| -> Vec<String> {
        tops.iter().find(|(p, _)| p == part).unwrap().1.iter().filter(|u| u.ends_with("Makefile")).cloned().collect()
    };
    assert_eq!(makefile_of("orders"), ["orders/Makefile"]);
    assert_eq!(makefile_of("billing"), ["billing/Makefile"]);
    assert_eq!(makefile_of("shop/python/shop"), ["shop/Makefile"]);
    assert!(makefile_of("shop/typescript/shop").is_empty());
    let alone = analyze_repo(&[makefiles().join("orders")], &Options::default()).unwrap();
    let make: Vec<(&str, &str)> = alone.make.iter().map(|t| (t.id.as_str(), t.module.as_str())).collect();
    assert_eq!(make, [("make:run", "Makefile"), ("make:test", "Makefile")], "one Makefile keeps its names");
}

/// Two parts whose package has one name (`orders/src/app`, `billing/src/app`) each hold it under
/// the directory the part lies in, with every name under it and the scripts beside it: both are
/// drawn, each with its own code, files and places, and no two nodes share an id. The project at
/// the root of the repository keeps its names. `atlas check` reads each part by the same names.
#[test]
fn two_parts_of_one_name_stay_two() {
    let g = analyze_repo(&[twins()], &Options::default()).unwrap();
    let parts: Vec<&str> = g.parts.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(parts, ["app", "billing/app", "orders/app"]);
    let ids: HashSet<&str> = g.nodes.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(ids.len(), g.nodes.len(), "no two nodes share an id");
    let edge = |kind: EdgeKind, src: &str, dst: &str| g.edges.iter().any(|e| e.kind == kind && e.src == src && e.dst == dst);
    assert!(edge(EdgeKind::Calls, "billing/app.main.main", "billing/app.main.bill"));
    assert!(edge(EdgeKind::Calls, "orders/app.main.main", "orders/app.main.take_orders"));
    assert!(edge(EdgeKind::Calls, "billing/_root.run", "billing/app.main.main"), "a script calls its own part's `main`");
    assert!(edge(EdgeKind::Calls, "orders/_root.run", "orders/app.main.main"));
    let node = |id: &str| g.node(id).unwrap_or_else(|| panic!("{id}"));
    assert_eq!(node("billing/app.main.main").part, "billing/app");
    assert!(Path::new(&node("billing/app.main.main").file).ends_with("billing/src/app/main.py"));
    let touched = |id: &str| node(id).body().touches.iter().map(|t| t.what.clone()).collect::<Vec<_>>();
    assert_eq!(touched("billing/app.main.main"), ["data/ledger.csv", "out/invoice.txt"]);
    assert_eq!(touched("orders/app.main.main"), ["out/orders.txt"]);
    assert_eq!(node("app.ship").part, "app", "the project at the root keeps its names");
    let checked: Vec<String> = analyze_parts(&[twins()], &Options::default()).unwrap().into_iter().map(|g| g.root).collect();
    assert_eq!(checked, ["app", "billing/app", "orders/app"]);
}

/// A part read alone keeps the names it has; two repositories read together, each a project whose
/// package is `app`, hold it under the repository's name.
#[test]
fn a_part_alone_keeps_its_names() {
    let alone = analyze_repo(&[twins().join("orders")], &Options::default()).unwrap();
    assert!(alone.node("app.main.main").is_some() && alone.node("_root.run").is_some());
    let both = analyze_repo(&[twins().join("orders"), twins().join("billing")], &Options::default()).unwrap();
    let parts: Vec<&str> = both.parts.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(parts, ["orders/app", "billing/app"]);
    assert!(both.node("billing/app.main.bill").is_some());
}

/// A part named apart from another is checked as the code it is: a package called `src`, held
/// under its directory (`billing/src`), is still asked its name.
#[test]
fn a_part_named_apart_is_checked_as_it_is() {
    let mut g = analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/srclayout"), &Options::default()).unwrap();
    g.rename(&|id| format!("billing/{id}"));
    let doors: Vec<(String, String)> = check(&g).into_iter().filter(|f| f.principle == "doors").map(|f| (f.node, f.what)).collect();
    assert!(matches!(doors.as_slice(), [(node, what)] if node == "billing/src" && what.contains("called src")), "{doors:?}");
}

/// A rename reaches every id a graph holds: afterwards no field that names a node names one by its
/// old id, in any language's graph. A label (a node's name, an entry's, a make recipe's command)
/// may read as an id, and is the author's.
#[test]
fn a_rename_reaches_every_id() {
    fn stale(v: &serde_json::Value, key: &str, old: &HashSet<String>, out: &mut Vec<String>) {
        match v {
            serde_json::Value::Object(m) => m.iter().for_each(|(k, v)| stale(v, k, old, out)),
            serde_json::Value::Array(a) => a.iter().for_each(|v| stale(v, key, old, out)),
            serde_json::Value::String(s) if !["name", "label", "recipe"].contains(&key) && s.split('&').any(|p| old.contains(p)) => {
                out.push(format!("{key}: {s}"))
            }
            _ => {}
        }
    }
    let tests = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests");
    for dir in ["fixture", "fixture-rs", "fixture-ts", "fixture-parts"] {
        let mut g = analyze_repo(&[tests.join(dir)], &Options::default()).unwrap();
        let old: HashSet<String> = g.nodes.iter().map(|n| n.id.clone()).chain(g.make.iter().map(|t| t.id.clone())).collect();
        g.rename(&|id| format!("~{id}"));
        let mut out = vec![];
        stale(&serde_json::to_value(&g).unwrap(), "", &old, &mut out);
        out.sort();
        out.dedup();
        assert!(out.is_empty(), "{dir}: ids the rename did not reach:\n{}", out.join("\n"));
    }
}

/// A repository named by a relative path, through `.` or `..`, is the same repository.
#[test]
fn a_relative_path_reads_the_same() {
    let ids = |g: Graph| g.nodes.into_iter().map(|n| n.id).collect::<Vec<_>>();
    let whole = ids(shop());
    for rel in ["./tests/fixture-parts", "tests/../tests/fixture-parts"] {
        let g = analyze_repo(&[PathBuf::from(rel)], &Options::default()).unwrap_or_else(|e| panic!("{rel}: {e}"));
        assert_eq!(ids(g), whole, "{rel}");
    }
}

/// A route is the path a client asks for: under the prefix its app is mounted at, and the
/// prefixes its router is declared and included with.
#[test]
fn routes_are_served_at_their_full_path() {
    let g = shop();
    assert_eq!(label(&g, "shop.api.item"), "GET /api/items/{item_id}", "`app.mount(\"/api\", items)`");
    assert_eq!(
        label(&g, "shop.serve.create_order"),
        "POST /v1/orders",
        "`include_router(router, prefix=\"/v1\")` and `APIRouter(prefix=\"/orders\")`"
    );
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
    assert_eq!(
        label(&g, "shop.board.moved_notes"),
        "GET /new/notes",
        "`register_blueprint(.., url_prefix=..)` stands in for the blueprint's own"
    );
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
