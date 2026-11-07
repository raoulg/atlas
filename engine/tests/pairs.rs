//! Pairs: the same program twice, once as good practice writes it and once with the shortcut
//! people typically take. The clean one must read as the algebra says; the shortcut must show as
//! a shortcut (a place the code does not name, an open question), never as if it were clean and
//! never not at all. What differs between the two pictures is what Atlas lays bare.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use atlas_engine::parts::analyze_repo;
use atlas_engine::{Atom, EdgeKind, Graph, Options, World};

fn pair(name: &str) -> Graph {
    analyze_repo(&[PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs").join(name)], &Options::default()).unwrap()
}

/// Services that name their package after themselves (`orders`, `billing`) are parts by those
/// names, and every id is the code's own. Services that each call their package `app` are named by
/// what the code does not say, the directory each lies in (`orders/app`, `billing/app`), and both
/// stay in the picture: the shortcut is the clean program with a directory where the code gives no
/// name, its code, calls and places alike. So in Python, Rust and TypeScript, whose package is named
/// by its `package.json`, as a Python package by its manifest and a Rust crate by its `Cargo.toml`.
#[test]
fn two_parts_are_named_apart() {
    for language in ["python", "rust", "typescript"] {
        let (clean, shortcut) = (pair(&format!("parts/{language}/clean")), pair(&format!("parts/{language}/shortcut")));
        let parts = |g: &Graph| g.parts.iter().map(|p| p.id.clone()).collect::<Vec<_>>();
        assert_eq!(parts(&clean), ["billing", "orders"], "{language}");
        assert_eq!(parts(&shortcut), ["billing/app", "orders/app"], "{language}");
        // the shortcut's ids with the directory as the name the code does not give
        let named = |g: &Graph, name: &dyn Fn(&str) -> String| {
            let mut out: Vec<String> = g.nodes.iter().filter(|n| !n.library).map(|n| format!("{} {:?}", name(&n.id), n.touches)).collect();
            out.extend(g.edges.iter().map(|e| format!("{} {:?} {}", name(&e.src), e.kind, name(&e.dst))));
            out.sort();
            out
        };
        assert_eq!(named(&shortcut, &|id| id.replacen("/app", "", 1)), named(&clean, &|id| id.to_string()), "{language}");
    }
}

/// Two parts that depend on one version of a library read one library: one directory is one code,
/// under the library's own name, and both parts call it. Two parts whose dependencies drift to two
/// versions read two libraries, each named after its version, and each part calls the one it runs,
/// which does what that version's code does: 0.1.0 writes its file whole, 0.2.0 writes beside it and
/// renames it in. Neither side holds an id twice (`mathematics.md`, Coproducts; #372). So in every
/// language (`versions/{python,rust,typescript}`), with the vendored package installed as its tool
/// installs it (Cargo by path, uv editable from its source, npm by a link into `node_modules`), and
/// in one Cargo workspace whose two members are one part (`versions/rust/workspace`, #378): its crates'
/// calls go into the version each crate's manifest and the lock give it.
#[test]
fn two_versions_of_a_library_are_two_libraries() {
    let versions = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/versions");
    let parts = ["clean/orders", "clean/billing", "shortcut/orders", "shortcut/billing"];
    let (python, typescript) = (installed("versions/python", &parts, uv_sync), installed("versions/typescript", &parts, npm_install));
    let subjects = [
        ("rust", versions.join("rust")),
        ("rust workspace", versions.join("rust/workspace")),
        ("python", python),
        ("typescript", typescript),
    ];
    let read = |dir: &Path, side: &str| analyze_repo(&[dir.join(side)], &Options::default()).unwrap();
    let graphs: Vec<(&str, Graph, Graph)> = subjects.iter().map(|(l, dir)| (*l, read(dir, "clean"), read(dir, "shortcut"))).collect();
    for (_, dir) in &subjects[2..] {
        let _ = std::fs::remove_dir_all(dir);
    }
    for (language, clean, shortcut) in &graphs {
        one_directory_is_one_library(language, clean, shortcut);
    }
}

/// A library two parts read from one directory is read over what both of them reach: the graph keeps
/// of a library the fiber over what the project reaches, and the project is both parts
/// (`mathematics.md`, Coproducts; #377). The shared program (`shared/{python,rust,typescript}`) has no
/// shortcut, since the two sides would be one code: `orders` reaches `notekit`'s `save` alone,
/// `billing` reaches `save` and what it calls, `write` beside it and `sync` in a module of its own.
/// Read alone, `orders` keeps `save` as the border of its fiber, with what `write` touches glued into
/// it, and does not read the module `sync` is in. Read with `billing`, in either order, `save` is one
/// reading in both parts and in the merge: it calls `write` and `sync` and touches nothing itself.
#[test]
fn a_library_two_parts_read_is_read_over_both_their_reach() {
    let shared = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/shared");
    let parts = ["shop/orders", "shop/billing"];
    let (python, typescript) = (installed("shared/python", &parts, uv_sync), installed("shared/typescript", &parts, npm_install));
    let subjects = [("rust", shared.join("rust")), ("python", python), ("typescript", typescript)];
    for (language, dir) in &subjects {
        read_over_both_parts(language, &dir.join("shop"));
    }
    for (_, dir) in &subjects[1..] {
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// The shared pair's law, on the repository `shop` as `language` reads it.
fn read_over_both_parts(language: &str, shop: &Path) {
    let (orders, billing) = (shop.join("orders"), shop.join("billing"));
    let function = |g: &Graph, name: &str| {
        let f = g.nodes.iter().find(|n| n.library && n.name == name && n.kind == atlas_engine::NodeKind::Function);
        f.map(|n| n.id.clone())
    };
    // what a graph reads `save` as: its steps and touches, and the lines out of it
    let reading = |g: &Graph| {
        let save = function(g, "save").unwrap_or_else(|| panic!("{language}: no save"));
        let n = g.node(&save).unwrap();
        let mut out: Vec<String> = g.edges.iter().filter(|e| e.src == save).map(|e| format!("{:?} {}", e.kind, e.dst)).collect();
        out.sort();
        (serde_json::to_string(&n.sites).unwrap(), serde_json::to_string(&n.touches).unwrap(), out)
    };
    let calls = |g: &Graph, to: &str| match (function(g, "save"), function(g, to)) {
        (Some(save), Some(to)) => g.edges.iter().any(|e| e.src == save && e.dst == to && e.kind == EdgeKind::Calls),
        _ => false,
    };
    let touches = |g: &Graph| g.node(&function(g, "save").unwrap()).unwrap().touches.len();
    let alone = analyze_repo(&[orders.clone()], &Options::default()).unwrap();
    assert!(!calls(&alone, "write") && !calls(&alone, "sync"), "{language}: read alone, orders reaches no callee of save");
    assert!(touches(&alone) > 0, "{language}: read alone, orders glues what write touches into save");
    let each: Vec<_> =
        atlas_engine::parts::analyze_parts(&[shop.to_path_buf()], &Options::default()).unwrap().iter().map(|g| reading(g)).collect();
    assert_eq!(each.len(), 2, "{language}");
    assert_eq!(each[0], each[1], "{language}: the two parts read save as one reading");
    let forward = analyze_repo(&[orders.clone(), billing.clone()], &Options::default()).unwrap();
    let backward = analyze_repo(&[billing, orders], &Options::default()).unwrap();
    for g in [&forward, &backward] {
        assert!(calls(g, "write") && calls(g, "sync") && touches(g) == 0, "{language}: {}", reading(g).0);
    }
    assert_eq!(reading(&forward), reading(&backward), "{language}: the merge is the same in both orders");
    assert_eq!(reading(&forward), each[0], "{language}: the merge holds the parts' one reading");
}

/// The versions pair's law, on its two sides as `language` reads them.
fn one_directory_is_one_library(language: &str, clean: &Graph, shortcut: &Graph) {
    // the roots of the libraries whose code the graph holds
    let roots = |g: &Graph| -> BTreeSet<String> {
        let holds = |r: &str| g.nodes.iter().any(|n| n.library && (n.id == r || n.id.starts_with(&format!("{r}."))));
        g.libraries.keys().filter(|r| holds(r)).cloned().collect()
    };
    // what the part's `main` calls to save
    let save = |g: &Graph, part: &str| -> String {
        let main = g.nodes.iter().find(|n| !n.library && n.part == part && n.name == "main" && n.kind == atlas_engine::NodeKind::Function);
        let saves: Vec<String> = main.unwrap().sites.iter().map(|s| s.to.clone()).filter(|t| t.ends_with(".save")).collect();
        assert_eq!(saves.len(), 1, "{language}: {part} saves once: {saves:?}");
        saves[0].clone()
    };
    let moves = |t: &atlas_engine::Touch| atlas_engine::effects::fs_op(&t.op) == atlas_engine::effects::FsOp::Moves;
    let renames = |g: &Graph, id: &str| g.node(id).unwrap_or_else(|| panic!("{language}: no {id}")).touches.iter().any(moves);
    assert_eq!(roots(clean), BTreeSet::from(["notekit".to_string()]), "{language}");
    let (orders, billing) = (save(clean, "orders"), save(clean, "billing"));
    assert!(orders == billing && orders.starts_with("notekit.") && renames(clean, &orders), "{language}: {orders} {billing}");
    assert_eq!(roots(shortcut), BTreeSet::from(["notekit-0_1_0".to_string(), "notekit-0_2_0".to_string()]), "{language}");
    let (orders, billing) = (save(shortcut, "orders"), save(shortcut, "billing"));
    assert!(orders.starts_with("notekit-0_1_0.") && billing.starts_with("notekit-0_2_0."), "{language}: {orders} {billing}");
    assert_eq!(orders.strip_prefix("notekit-0_1_0"), billing.strip_prefix("notekit-0_2_0"), "{language}: one name below each version");
    assert!(!renames(shortcut, &orders) && renames(shortcut, &billing), "{language}");
    for g in [clean, shortcut] {
        let ids: Vec<&str> = g.nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(ids.len(), ids.iter().collect::<BTreeSet<_>>().len(), "{language}: {ids:?}");
    }
}

/// The pair `name` with the vendored package each of its `parts` depends on installed as the
/// language's tool installs it (`install`), in a directory of its own. Nothing is fetched.
fn installed(name: &str, parts: &[&str], install: fn(&Path)) -> PathBuf {
    let from = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs").join(name);
    let to = std::env::temp_dir().join(format!("atlas-pairs-{}-{}", name.replace('/', "-"), std::process::id()));
    let _ = std::fs::remove_dir_all(&to);
    copy(&from, &to);
    for part in parts {
        install(&to.join(part));
    }
    to
}

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let target = to.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy(&e.path(), &target);
        } else {
            std::fs::copy(e.path(), target).unwrap();
        }
    }
}

/// What `uv sync` makes of the project at `dir` whose one dependency is an editable path source: a
/// `.venv` whose site-packages holds the distribution's metadata and a `.pth` that puts its source
/// on the path.
fn uv_sync(dir: &Path) {
    let manifest: toml::Table = std::fs::read_to_string(dir.join("pyproject.toml")).unwrap().parse().unwrap();
    let source = manifest["tool"]["uv"]["sources"]["notekit"]["path"].as_str().unwrap();
    let source = path_clean::clean(dir.join(source));
    let theirs: toml::Table = std::fs::read_to_string(source.join("pyproject.toml")).unwrap().parse().unwrap();
    let version = theirs["project"]["version"].as_str().unwrap();
    let venv = dir.join(".venv");
    let site = if cfg!(windows) { venv.join("Lib/site-packages") } else { venv.join("lib/python3.13/site-packages") };
    let info = site.join(format!("notekit-{version}.dist-info"));
    std::fs::create_dir_all(&info).unwrap();
    let home = format!("home = {}\nversion_info = 3.13.0\ninclude-system-site-packages = false\n", dir.display());
    std::fs::write(venv.join("pyvenv.cfg"), home).unwrap();
    std::fs::write(info.join("METADATA"), format!("Metadata-Version: 2.4\nName: notekit\nVersion: {version}\n")).unwrap();
    std::fs::write(site.join("_notekit.pth"), format!("{}\n", source.join("src").display())).unwrap();
}

/// What `npm install` makes of the package at `dir` whose one dependency is a `file:` directory: a
/// link to it in its `node_modules` (a junction on Windows).
fn npm_install(dir: &Path) {
    let manifest: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("package.json")).unwrap()).unwrap();
    let source = path_clean::clean(dir.join(manifest["dependencies"]["notekit"].as_str().unwrap().strip_prefix("file:").unwrap()));
    let link = dir.join("node_modules/notekit");
    std::fs::create_dir_all(link.parent().unwrap()).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&source, &link).unwrap();
    #[cfg(windows)]
    assert!(std::process::Command::new("cmd").arg("/C").arg("mklink").arg("/J").arg(&link).arg(&source).status().unwrap().success());
}

/// Isomorphic is the same (`CLAUDE.md`): one program draws one set of places in every language.
/// The kept pair (`kept/{python,rust,typescript}`) keeps a note in a file in the folder `where.txt`
/// names, reads it and takes it; the clean side writes it whole beside its file and renames it in
/// and binds the folder to a name, the shortcut writes it straight in and binds it to none. A value
/// no code Atlas follows decides is a hole by the name the code binds it to, else `{}`, in each
/// language alike; a move into a file, its removal and its reading rest at that one file. So the
/// places each reading draws, with what each touch does there, are the same in all three.
#[test]
fn one_program_draws_one_set_of_places_in_every_language() {
    use std::collections::BTreeSet;
    for side in ["clean", "shortcut"] {
        let places = |language: &str| -> BTreeSet<(String, String)> {
            let g = pair(&format!("kept/{language}/{side}"));
            g.nodes
                .iter()
                .filter(|n| !n.library)
                .flat_map(|n| n.touches.iter())
                .filter(|t| t.kind == World::Disk && !t.artifact.is_empty())
                .map(|t| {
                    let w = t.drawn();
                    (w.place.label, w.verb)
                })
                .collect()
        };
        let rust = places("rust");
        assert!(!rust.is_empty(), "{side}");
        assert_eq!(places("python"), rust, "{side}: Python and Rust");
        assert_eq!(places("typescript"), rust, "{side}: TypeScript and Rust");
    }
}

/// A name a function gives a value is the function's (`docs/reference/mathematics.md`, "Paths as
/// terms"), as its parameters are: the owned pair (`owned/{python,rust,typescript}`) keeps a report
/// and a log where whoever runs the program says, which no code decides. The clean side names each
/// value for what it holds (`report`, `log`), the shortcut names both `file`. A name decides no place,
/// so each side draws two places, one per function, in every language: the shortcut's two `file`s
/// are not one file.
#[test]
fn a_name_two_functions_share_is_two_places() {
    use std::collections::BTreeSet;
    for language in ["python", "rust", "typescript"] {
        for side in ["clean", "shortcut"] {
            let g = pair(&format!("owned/{language}/{side}"));
            let places: BTreeSet<(String, String)> = g
                .nodes
                .iter()
                .filter(|n| !n.library)
                .flat_map(|n| n.touches.iter().map(move |t| (n.id.clone(), t)))
                .filter(|(_, t)| t.kind == World::Disk)
                .map(|(f, t)| (f, t.place().id))
                .collect();
            let ids: BTreeSet<&String> = places.iter().map(|(_, p)| p).collect();
            assert_eq!(places.len(), 2, "{language} {side}: {places:?}");
            assert_eq!(ids.len(), 2, "{language} {side}: one place for each function: {places:?}");
        }
    }
}

/// Places by the type that holds them (`docs/reference/mathematics.md`): the workbench pair
/// (`workbench/{python,rust,typescript}`) keeps a report and a log in a folder of the project's own.
/// The clean side gives the folder a type, whose method names each file in it; the shortcut hands
/// the folder around as a bare path. Both keep the same files at the same places, and only the clean
/// side's code says they belong together: there each place is the `Workbench`'s, a group the
/// picture folds, and on the shortcut none is any type's.
#[test]
fn the_places_a_type_builds_are_its_group() {
    use std::collections::BTreeSet;
    for language in ["python", "rust", "typescript"] {
        let read = |side: &str| -> Vec<(String, String, String)> {
            let g = pair(&format!("workbench/{language}/{side}"));
            let mut out: Vec<(String, String, String)> = g
                .nodes
                .iter()
                .filter(|n| !n.library)
                .flat_map(|n| n.touches.iter())
                .filter(|t| t.kind == World::Disk && !t.artifact.is_empty())
                .map(|t| (t.artifact.clone(), t.holder.clone(), t.place().group))
                .collect();
            out.sort();
            out.dedup();
            out
        };
        let (clean, shortcut) = (read("clean"), read("shortcut"));
        let files = |side: &[(String, String, String)]| -> BTreeSet<String> { side.iter().map(|(a, _, _)| a.clone()).collect() };
        assert_eq!(files(&clean), files(&shortcut), "{language}: the same files");
        assert!(clean.iter().all(|(_, h, g)| h.ends_with(".Workbench") && *g == format!("w:Type:{h}")), "{language}: {clean:?}");
        assert!(shortcut.iter().all(|(_, h, g)| h.is_empty() && !g.starts_with("w:Type:")), "{language}: {shortcut:?}");
    }
}

/// A client that names the paths it asks for is joined to the handlers that serve them, by
/// method and path: its POST to `/api/orders` is not the GET of that path. One that looks its
/// paths and methods up in tables by a key, with an environment variable that may stand in for
/// each path, asks an address the code does not say, and meets at most the routes at the paths its
/// tables hold. Each reader reads a table the code writes, looked up by a key no code decides, as
/// one of the values the table holds (Python's `PATHS[key]`, TypeScript's `DEFAULT_PATHS[found!.env]`,
/// Rust's `paths[key]` of a `HashMap::from([..])`), the variable standing in being an input with that
/// default, so `item` meets the one route at its path. The method is looked up where the code does
/// not say it as a method (`METHODS["order"]`, `method("order")`), so `order` meets each route at
/// `/api/orders`. Isomorphic is the same: the three shortcuts meet the same routes. So in TypeScript
/// (`clean`, `shortcut`), Python (`pyclean`, `pyshortcut`) and Rust (`rsclean`, `rsshortcut`).
#[test]
fn a_request_meets_the_route_it_names() {
    let g = pair("requests");
    let requests = |side: &str| -> Vec<(String, String)> {
        g.edges.iter().filter(|e| e.kind == EdgeKind::Requests && e.src.starts_with(side)).map(|e| (e.src.clone(), e.dst.clone())).collect()
    };
    // the routes at the paths the shortcut's tables hold, `/api/items/{..}` and `/api/orders`
    let tabled = ["shop.api.item", "shop.api.order", "shop.api.orders"];
    // what each shortcut's requests meet, by the function that asks
    let mut met = vec![];
    for (clean, shortcut) in [("clean", "shortcut"), ("pyclean", "pyshortcut"), ("rsclean", "rsshortcut")] {
        // a Rust crate's functions are its root module's
        let at = |side: &str, f: &str| if side.starts_with("rs") { format!("{side}.{f}") } else { format!("{side}.client.{f}") };
        let want = [(at(clean, "item"), "shop.api.item".to_string()), (at(clean, "order"), "shop.api.order".to_string())];
        assert_eq!(requests(&format!("{clean}.")), want);
        let asked = requests(&format!("{shortcut}."));
        assert!(
            asked.iter().all(|(_, route)| tabled.contains(&route.as_str())),
            "{shortcut}: a request meets a route its tables hold: {asked:?}"
        );
        let by_function: Vec<(String, String)> =
            asked.iter().map(|(f, route)| (f.rsplit('.').next().unwrap_or_default().to_string(), route.clone())).collect();
        met.push((shortcut, by_function));
        for f in ["item", "order"].map(|f| at(shortcut, f)) {
            let net = g.node(&f).unwrap().touches.iter().find(|t| t.kind == World::Net).cloned();
            assert!(
                net.as_ref().map_or(false, |t| t.artifact.is_empty() && !t.fixed),
                "{f} asks an address the code does not say: {net:?}"
            );
        }
    }
    let picked = [("item", "shop.api.item"), ("order", "shop.api.order"), ("order", "shop.api.orders")]
        .map(|(f, route)| (f.to_string(), route.to_string()))
        .to_vec();
    for (shortcut, by_function) in &met {
        assert_eq!(by_function, &picked, "{shortcut}: the table holds the path; a method the code does not say meets every route at it");
    }
}

/// A request's method says which way it goes, as a statement's verb does: a GET takes what the
/// place holds, a POST leaves something there. The clean clients say the method of each request
/// (`requests.post`, `fetch(url, { method: "POST" })`, `client.post(..)`); the shortcut looks the
/// method of `order` up by a key at run time (`requests.request(METHODS["order"], ..)`,
/// `{ method: method("order") }`, `client.request(method("order"), ..)`), so the code does not say
/// which way it goes and the place is in neither domain nor codomain. Its `item` still says GET.
#[test]
fn a_request_goes_the_way_its_method_says() {
    use atlas_engine::flow::Side;
    let g = pair("requests");
    let side =
        |f: &str| -> Vec<Side> { g.node(f).unwrap().touches.iter().filter(|t| t.kind == World::Net).map(|t| t.side()).collect::<Vec<_>>() };
    for (clean, shortcut) in [("clean", "shortcut"), ("pyclean", "pyshortcut"), ("rsclean", "rsshortcut")] {
        let at = |side: &str, f: &str| if side.starts_with("rs") { format!("{side}.{f}") } else { format!("{side}.client.{f}") };
        assert_eq!(side(&at(clean, "item")), [Side::In], "{clean}: a GET reads");
        assert_eq!(side(&at(clean, "order")), [Side::Out], "{clean}: a POST writes");
        assert_eq!(side(&at(shortcut, "item")), [Side::In], "{shortcut}: a GET reads");
        assert_eq!(side(&at(shortcut, "order")), [Side::Open], "{shortcut}: a method looked up at run time says no way");
    }
}

/// A package's doors are what its root names, in every language: a crate's `pub fn` in `lib.rs`, what the module
/// `package.json` points to exports, the names the root `__init__.py` binds. The six clients of the requests pair are
/// one client written three ways, clean and with the shortcut, so each has the same doors, the functions `item` and
/// `order`, and a part reaches what its doors reach alike: isomorphic is the same.
#[test]
fn one_client_has_the_same_doors_in_every_language() {
    use atlas_engine::{EntryKind, NodeKind};
    let g = pair("requests");
    for side in ["clean", "shortcut", "pyclean", "pyshortcut", "rsclean", "rsshortcut"] {
        let doors: Vec<&str> = g
            .nodes
            .iter()
            .filter(|n| n.id.starts_with(&format!("{side}.")) && n.kind == NodeKind::Function)
            .filter(|n| n.entry.as_ref().is_some_and(|e| e.kind == EntryKind::Export))
            .map(|n| n.name.as_str())
            .collect();
        assert_eq!(doors, ["item", "order"], "{side}: the functions its root names");
    }
}

/// A package meant to be imported is met at its doors, in every language: a crate with a lib target, a `package.json`
/// that is not private, a Python package with a build system. Its doors are what its root names (`pub fn` in `lib.rs`,
/// what the module `package.json` points to exports, `__all__`). The clean side names `scale` and `metres`; the shortcut
/// is the same package with the root naming nothing (private functions, no `exports`, an `__init__.py` with only a
/// docstring), and each of the three is asked what a user may import.
#[test]
fn a_library_that_exports_nothing_has_no_door_in_every_language() {
    use atlas_engine::principles::check;
    use atlas_engine::{EntryKind, NodeKind};
    let read = |lang: &str, side: &str| {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/exports").join(lang).join(side);
        atlas_engine::analyze(&dir, &Options::default()).unwrap_or_else(|e| panic!("{lang}/{side} parses: {e}"))
    };
    let doors = |g: &Graph| -> Vec<String> {
        let mut d: Vec<String> = g
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Function && n.entry.as_ref().is_some_and(|e| e.kind == EntryKind::Export))
            .map(|n| n.name.clone())
            .collect();
        d.sort();
        d
    };
    let asked = |g: &Graph| check(g).into_iter().map(|f| (f.principle.to_string(), f.what)).collect::<Vec<_>>();
    for lang in ["python", "typescript", "rust"] {
        let (clean, shortcut) = (read(lang, "clean"), read(lang, "shortcut"));
        assert!(clean.parts.iter().all(|p| p.library), "{lang}: the clean side is a library");
        assert!(shortcut.parts.iter().all(|p| p.library), "{lang}: the shortcut is a library as well");
        assert_eq!(doors(&clean), ["metres", "scale"], "{lang}: what its root names");
        assert_eq!(asked(&clean), [], "{lang}: a package with doors is asked nothing");
        assert!(doors(&shortcut).is_empty(), "{lang}: the shortcut names no door");
        assert_eq!(asked(&shortcut), [("doors".to_string(), "units exports nothing at its root".to_string())], "{lang}");
    }
}

/// An app has routes, not doors: a crate with only a `main`, a private `package.json` and a `package.json` with only a
/// `bin` are no library, so exporting nothing is no finding. The manifest decides, not the code.
#[test]
fn an_app_exports_nothing_and_is_asked_no_door() {
    use atlas_engine::principles::check;
    for app in ["rust", "typescript-private", "typescript-bin"] {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/exports/apps").join(app);
        let g = atlas_engine::analyze(&dir, &Options::default()).unwrap_or_else(|e| panic!("{app} parses: {e}"));
        assert!(g.parts.iter().all(|p| !p.library), "{app}: an app is no library");
        let asked: Vec<_> = check(&g).into_iter().filter(|f| f.principle == "doors").collect();
        assert!(asked.is_empty(), "{app}: {asked:?}");
    }
}

/// A shaped array is a type of its own and stands in its rank family (`docs/reference/mathematics.md`, "Shapes and their
/// rank"). The clean components say the shape of what they take and give (`Float[Tensor, "batch dim"]`); the shortcut
/// says `Tensor`. Each shape of the clean side stays its own type, and stands in a family shared by exactly the shapes of
/// its dtype, array type and rank, however few: the quotient is the rank, a map on each shape alone, never the names
/// (`batch dim` and `batch ndim` are two types of one family), and never the dtype (`Int[Tensor, batch seq]` is alone in
/// its rank, and in a family of its own). A run alone (`...`) is named as its family and stands in one only beside
/// another shape. A family is named by what it is (`Float Tensor · 2D`), never as a shape the code could have written. The
/// shortcut's steps all take and give the one `Tensor`, so the picture cannot tell a reducer from a projection: what the
/// code does not say, Atlas does not draw.
#[test]
fn a_shape_stands_in_its_rank_family_and_stays_a_type_of_its_own() {
    use atlas_engine::shapes::Array;
    use std::collections::BTreeSet;
    let (clean, shortcut) = (pair("shapes/python/clean"), pair("shapes/python/shortcut"));
    let types = atlas_engine::families::families(&clean);
    let shaped: Vec<(String, Array)> = clean.nodes.iter().filter_map(|n| Some((n.id.clone(), Array::of_id(&n.id)?))).collect();
    let names: BTreeSet<String> = shaped.iter().map(|(_, a)| a.to_string()).collect();
    assert!(names.contains("Float[Tensor, batch dim]") && names.contains("Float[Tensor, batch ndim]"), "{names:?}");
    let same = |a: &Array, b: &Array| a.dtype == b.dtype && a.array == b.array && a.rank() == b.rank();
    for (t, a) in &shaped {
        let kin: Vec<&String> = shaped.iter().filter(|(u, b)| u != t && same(a, b)).map(|(u, _)| u).collect();
        match types.kind.get(t) {
            Some(k) => {
                assert!(Array::of_id(k).is_none(), "{a} stands in {k}, which reads as a shape the code does not write");
                assert!(types.ranks.contains(k), "{k} is not said to be a rank family: {:?}", types.ranks);
                for u in &kin {
                    assert_eq!(types.kind.get(*u), Some(k), "{a} and {u} share a rank");
                }
                for (u, b) in shaped.iter().filter(|(u, _)| types.kind.get(u) == Some(k)) {
                    assert!(same(a, b), "{u} stands in {k} beside {a}");
                }
            }
            None => {
                assert!(atlas_engine::shapes::name(t).is_some(), "{a} stands in no rank family");
                assert!(kin.is_empty(), "{a} shares its rank with {kin:?} and stands in no family");
            }
        }
    }
    let alone = shaped.iter().find(|(_, a)| a.to_string() == "Int[Tensor, batch seq]").map(|(t, _)| t).expect("the clean side's Int shape");
    assert!(types.kind.contains_key(alone), "a shape alone in its rank stands in its family");
    assert!(types.kind.values().collect::<BTreeSet<_>>().len() >= 2, "the clean side has families of two ranks: {:?}", types.kind);
    // the shortcut: one type for every tensor, in no family
    let tensors = |f: &str, kind: EdgeKind| -> Vec<String> {
        shortcut.edges.iter().filter(|e| e.src == f && e.kind == kind).map(|e| e.dst.clone()).collect()
    };
    let steps = ["Lookup", "MeanPool", "Projection", "Concatenate"].map(|c| format!("mesh.components.{c}.forward"));
    let all: BTreeSet<String> = steps.iter().flat_map(|f| [tensors(f, EdgeKind::Takes), tensors(f, EdgeKind::Returns)].concat()).collect();
    assert_eq!(all, BTreeSet::from(["ext:torch.Tensor".to_string()]));
    assert!(atlas_engine::families::families(&shortcut).kind.is_empty());
}

/// `d[k]`, `d` a table the code writes out, is one of the values `d` holds where no code decides
/// `k`, and a key the code writes picks its own value (`docs/reference/mathematics.md`, "Paths as
/// terms"). Isomorphic is the same: Python's `KINDS[kind]`, TypeScript's `KINDS[kind]` of an object
/// literal and Rust's `kinds[kind]` and `kinds.get(kind)` of a `HashMap::from([..])`. In each
/// language the test computes the place a key no code decides reads, and the places the code reads
/// where it writes each value out instead, and compares them; then a key the code writes, and the
/// value it picks written out.
#[test]
fn a_key_no_code_decides_is_one_of_a_tables_values_in_every_language() {
    let forms = [
        (
            "python",
            [
                "    print((DATA / KINDS[kind]).read_text())",
                "    print((DATA / {v}).read_text())",
                "    print((DATA / KINDS[\"a\"]).read_text())",
            ],
        ),
        (
            "typescript",
            [
                "  readFileSync(path.join(DATA, KINDS[kind]));",
                "  readFileSync(path.join(DATA, {v}));",
                "  readFileSync(path.join(DATA, KINDS.a));",
            ],
        ),
        (
            "rust",
            [
                "    fs::read_to_string(Path::new(DATA).join(kinds[kind]))?;",
                "    fs::read_to_string(Path::new(DATA).join({v}))?;",
                "    fs::read_to_string(Path::new(DATA).join(kinds.get(\"a\").unwrap()))?;",
            ],
        ),
    ];
    let mut read = vec![];
    for (language, [handed, written, known]) in forms {
        let each: Vec<_> = ["regelingen", "ontwerp"]
            .iter()
            .map(|v| run_places(language, &format!("table-{v}"), &written.replace("{v}", &format!("\"{v}\""))))
            .collect();
        let every: std::collections::BTreeSet<_> = each.iter().flatten().cloned().collect();
        assert_eq!(every.len(), 2, "{language}: {every:?}");
        assert_eq!(run_places(language, "table-handed", handed), every, "{language}: a key no code decides");
        assert_eq!(run_places(language, "table-known", known), each[0], "{language}: a key the code writes");
        read.push(every);
    }
    assert!(read.windows(2).all(|w| w[0] == w[1]), "one table reads the same places in every language: {read:?}");
}

/// Each element of a listing of a directory is the pattern `*` below it (`docs/reference/mathematics.md`,
/// "Paths as terms"), in every language: Python's `D.glob("*")` and `os.listdir(D)`, TypeScript's
/// `readdirSync(D)`, whose names the code joins back below the directory, looped over or mapped, and
/// Rust's `fs::read_dir(D)`, whose entries are at their `path()`, looped over, mapped or handed to a
/// closure for each, as the path's own method `D.read_dir()`, which std documents as `fs::read_dir(D)`, and
/// tokio's `read_dir(D)`, whose entries `next_entry` gives one by one. An entry's name joined back below the
/// directory is the entry's path: Python's `e.name` of `os.scandir(D)` as its `e.path`, TypeScript's `e.name`
/// of `readdirSync(D, { withFileTypes: true })` and Rust's `e.file_name()`. The file read in each folder is one
/// place, and the directory listed is one place, whichever form and language the code takes.
#[test]
fn a_listing_is_a_pattern_below_its_directory_in_every_language() {
    let glob = run_places("python", "listing-glob", "    for p in DATA.glob(\"*\"):\n        print((p / \"x.txt\").read_text())");
    assert!(glob.contains(&("disk_read".into(), "data/*/x.txt".into())), "{glob:?}");
    for (language, name, body) in [
        ("python", "listing-listdir", "    for n in os.listdir(DATA):\n        print(open(os.path.join(DATA, n, \"x.txt\")).read())"),
        ("typescript", "listing-loop", "  for (const n of readdirSync(DATA)) {\n    readFileSync(path.join(DATA, n, \"x.txt\"));\n  }"),
        (
            "typescript",
            "listing-map",
            "  readdirSync(DATA)\n    .map((n) => path.join(DATA, n, \"x.txt\"))\n    .forEach((p) => readFileSync(p));",
        ),
        ("rust", "listing-loop", "    for e in fs::read_dir(DATA)? {\n        fs::read_to_string(e?.path().join(\"x.txt\"))?;\n    }"),
        (
            "rust",
            "listing-map",
            "    for p in fs::read_dir(DATA)?.flatten().map(|e| e.path()) {\n        fs::read_to_string(p.join(\"x.txt\"))?;\n    }",
        ),
        (
            "rust",
            "listing-each",
            "    fs::read_dir(DATA)?.flatten().for_each(|e| {\n        let _ = fs::read_to_string(e.path().join(\"x.txt\"));\n    });",
        ),
        ("rust", "listing-method", "    for e in Path::new(DATA).read_dir()? {\n        fs::read_to_string(e?.path().join(\"x.txt\"))?;\n    }"),
        (
            "rust",
            "listing-tokio",
            "    let mut entries = tokio::fs::read_dir(DATA).await?;\n    while let Some(e) = entries.next_entry().await? {\n        tokio::fs::read_to_string(e.path().join(\"x.txt\")).await?;\n    }",
        ),
        ("python", "listing-entry-path", "    for e in os.scandir(DATA):\n        print(open(os.path.join(e.path, \"x.txt\")).read())"),
        (
            "python",
            "listing-entry-name",
            "    for e in os.scandir(DATA):\n        print(open(os.path.join(DATA, e.name, \"x.txt\")).read())",
        ),
        (
            "typescript",
            "listing-entry-name",
            "  for (const e of readdirSync(DATA, { withFileTypes: true })) {\n    readFileSync(path.join(DATA, e.name, \"x.txt\"));\n  }",
        ),
        (
            "rust",
            "listing-entry-name",
            "    for e in fs::read_dir(DATA)? {\n        fs::read_to_string(Path::new(DATA).join(e?.file_name()).join(\"x.txt\"))?;\n    }",
        ),
    ] {
        assert_eq!(run_places(language, name, body), glob, "{language} {name}");
    }
}

/// A path is one term whichever form builds it (`docs/reference/mathematics.md`, "Paths as terms"): a join is the join
/// whether the code writes it as a method or as the function it is (`Path::join(p, x)` is `p.join(x)`), and a path the
/// code changes in place is what it holds after each change, as a name bound anew is (`p.push(x)` is `p = p.join(x)`,
/// `p.pop()` is `p = p.parent()`, `p.set_extension(e)` and `p.set_file_name(n)` are the path with another extension or
/// name). So Python's `DATA / kind / "x.txt"`, TypeScript's `path.join(DATA, kind, "x.txt")` and each Rust form read
/// one place. A change the body may skip (in an `if`) leaves the path as it was or changed: the union of both; a join it
/// repeats (in a loop) as often as the code does not say leaves it where it was or below it, however the join is written.
#[test]
fn a_path_is_one_term_whichever_form_builds_it() {
    let one = run_places("python", "term-slash", "    print((DATA / kind / \"x.txt\").read_text())");
    assert_eq!(one.len(), 1, "{one:?}");
    assert_eq!(run_places("typescript", "term-join", "  readFileSync(path.join(DATA, kind, \"x.txt\"));"), one, "typescript");
    for (name, body) in [
        ("term-method", "    fs::read_to_string(Path::new(DATA).join(kind).join(\"x.txt\"))?;"),
        ("term-function", "    fs::read_to_string(Path::join(&Path::join(Path::new(DATA), kind), \"x.txt\"))?;"),
        ("term-push", "    let mut p = std::path::PathBuf::from(DATA);\n    p.push(kind);\n    p.push(\"x.txt\");\n    fs::read_to_string(&p)?;"),
        (
            "term-push-new",
            "    let mut p = std::path::PathBuf::new();\n    p.push(DATA);\n    p.push(kind);\n    p.push(\"x.txt\");\n    fs::read_to_string(p)?;",
        ),
        ("term-push-function", "    let mut p = Path::new(DATA).join(kind);\n    std::path::PathBuf::push(&mut p, \"x.txt\");\n    fs::read_to_string(p)?;"),
        ("term-rebound", "    let mut p = Path::new(DATA).to_path_buf();\n    p = p.join(kind).join(\"x.txt\");\n    fs::read_to_string(p)?;"),
        ("term-pop", "    let mut p = Path::new(DATA).join(kind).join(\"y\");\n    p.pop();\n    p.push(\"x.txt\");\n    fs::read_to_string(p)?;"),
        ("term-extension", "    let mut p = Path::new(DATA).join(kind).join(\"x\");\n    p.set_extension(\"txt\");\n    fs::read_to_string(p)?;"),
        ("term-file-name", "    let mut p = Path::new(DATA).join(kind).join(\"y.txt\");\n    p.set_file_name(\"x.txt\");\n    fs::read_to_string(p)?;"),
    ] {
        assert_eq!(run_places("rust", name, body), one, "rust {name}");
    }
    let kept = run_places("rust", "term-kept", "    fs::read_to_string(Path::new(DATA).join(kind))?;");
    let both: std::collections::BTreeSet<_> = kept.union(&one).cloned().collect();
    assert_eq!(both.len(), 2, "{both:?}");
    let skipped = "    let mut p = Path::new(DATA).join(kind);\n    if kind.len() > 1 {\n        p.push(\"x.txt\");\n    }\n    fs::read_to_string(p)?;";
    assert_eq!(run_places("rust", "term-skipped", skipped), both, "a change the body may skip");
    // a join the body repeats as often as the code does not say leaves the path where it was or below it
    let before = run_places("rust", "term-before", "    fs::read_to_string(Path::new(DATA).join(\"x\"))?;");
    let loop_over = |change: &str| {
        format!("    let mut p = Path::new(DATA).join(\"x\");\n    for part in kind.split('/') {{\n        {change}\n    }}\n    fs::read_to_string(p)?;")
    };
    let repeated = run_places("rust", "term-repeated", &loop_over("p.push(part);"));
    assert_eq!(run_places("rust", "term-repeated-rebound", &loop_over("p = p.join(part);")), repeated, "`p = p.join(x)` is `p.push(x)`");
    let below = |(kind, place): &(String, String)| before.iter().any(|(k, q)| k == kind && place.starts_with(&format!("{q}/")));
    assert!(before.is_subset(&repeated) && repeated.len() > before.len(), "{repeated:?} holds {before:?}");
    assert!(repeated.difference(&before).all(below), "{repeated:?} where it was or below {before:?}");
}

/// The places `run` of a one-module program touches, each with its kind: a program in `language`
/// that keeps a directory `DATA` (`data`) and a table `KINDS` from a key to a folder below it
/// (`a` to `regelingen`, `b` to `ontwerp`), and runs `body` with a `kind` its caller hands. A Rust `run` whose body
/// awaits is an `async fn`.
fn run_places(language: &str, name: &str, body: &str) -> std::collections::BTreeSet<(String, String)> {
    let root = std::env::temp_dir().join(format!("atlas-{language}-{name}-{}", std::process::id()));
    let write = |file: &str, text: &str| {
        let p = root.join(file);
        std::fs::create_dir_all(p.parent().unwrap()).expect("makes the directory");
        std::fs::write(p, text).expect("writes the file");
    };
    let id = match language {
        "python" => {
            write("pyproject.toml", "[project]\nname = \"demo\"\nversion = \"0\"\n");
            write("src/demo/__init__.py", "");
            write(
                "src/demo/core.py",
                &format!(
                    "import os\nfrom pathlib import Path\n\nDATA = Path(\"data\")\nKINDS = {{\"a\": \"regelingen\", \"b\": \"ontwerp\"}}\n\n\ndef run(kind: str) -> None:\n{body}\n"
                ),
            );
            "demo.core.run"
        }
        "typescript" => {
            write("package.json", "{\"name\": \"demo\", \"version\": \"0.0.0\"}\n");
            write(
                "src/core.ts",
                &format!(
                    "import {{ readdirSync, readFileSync }} from \"fs\";\nimport * as path from \"path\";\n\nconst DATA = \"data\";\nconst KINDS: Record<string, string> = {{ a: \"regelingen\", b: \"ontwerp\" }};\n\nexport function run(kind: string): void {{\n{body}\n}}\n"
                ),
            );
            "demo.core.run"
        }
        _ => {
            write("Cargo.toml", "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n");
            write(
                "src/lib.rs",
                &format!(
                    "use std::collections::HashMap;\nuse std::fs;\nuse std::path::Path;\n\nconst DATA: &str = \"data\";\n\npub {}fn run(kind: &str) -> std::io::Result<()> {{\n    let kinds: HashMap<&str, &str> = HashMap::from([(\"a\", \"regelingen\"), (\"b\", \"ontwerp\")]);\n{body}\n    Ok(())\n}}\n",
                    if body.contains(".await") { "async " } else { "" }
                ),
            );
            "demo.run"
        }
    };
    let g = analyze_repo(&[root.clone()], &Options::default()).expect("reads");
    let _ = std::fs::remove_dir_all(&root);
    let run = g.node(id).unwrap_or_else(|| panic!("{language} {name}: run is in the graph"));
    run.touches.iter().filter(|t| t.kind_name() != "screen").map(|t| (t.kind_name().to_string(), t.artifact.clone())).collect()
}

/// A loop over a table the code writes out goes over its keys (`docs/reference/mathematics.md`,
/// "Paths as terms"), in every language that reads one: Python's `for k in KINDS` and
/// `KINDS.keys()` of a dict, TypeScript's `for (const k in KINDS)` of an object literal. The test
/// computes the places a loop over the table reads, and those a loop over its keys written out
/// reads, and compares them; the values the table holds are no part of either.
#[test]
fn a_loop_over_a_table_goes_over_its_keys_in_every_language() {
    let python = "    for k in [\"a\", \"b\"]:\n        print((DATA / k).read_text())";
    let typescript = "  for (const k of [\"a\", \"b\"]) {\n    readFileSync(path.join(DATA, k));\n  }";
    let mut read = vec![];
    for (language, name, table, written) in [
        ("python", "keys-in", "    for k in KINDS:\n        print((DATA / k).read_text())", python),
        ("python", "keys-of", "    for k in KINDS.keys():\n        print((DATA / k).read_text())", python),
        ("typescript", "keys-in", "  for (const k in KINDS) {\n    readFileSync(path.join(DATA, k));\n  }", typescript),
    ] {
        let keys = run_places(language, &format!("{name}-written"), written);
        assert_eq!(keys.len(), 2, "{language}: {keys:?}");
        assert_eq!(run_places(language, name, table), keys, "{language}: {table}");
        read.push(keys);
    }
    assert!(read.windows(2).all(|w| w[0] == w[1]), "one table's keys read the same places in every language: {read:?}");
}

/// A folder is placed as the files in it are (`docs/reference/mathematics.md`, "Paths as terms"):
/// in the workbench pair (`workbench/{python,rust,typescript}`) the folder `clear` removes, the one
/// `report.txt` is in, is the folder of the file `save_report` writes, under the same callers, on
/// both sides and in every language, also where no caller says the project (`{project}/.made/`):
/// there each is the place the users of the library's entries hand it.
#[test]
fn a_folder_is_placed_as_the_files_in_it_are() {
    for language in ["python", "rust", "typescript"] {
        for side in ["clean", "shortcut"] {
            let g = pair(&format!("workbench/{language}/{side}"));
            // a touch's place and the callers that make it whole, the users of whichever entry as
            // one condition
            let disk = |names: &[&str]| -> Vec<(String, std::collections::BTreeSet<Vec<String>>)> {
                let users = |a: &Atom| match a {
                    Atom::Starts(_) => "the users of an entry".to_string(),
                    a => format!("{a:?}"),
                };
                g.nodes
                    .iter()
                    .filter(|n| !n.library && names.contains(&n.name.as_str()))
                    .flat_map(|n| &n.touches)
                    .filter(|t| t.kind == World::Disk)
                    .map(|t| (t.artifact.clone(), t.by.iter().map(|b| b.0.iter().map(users).collect()).collect()))
                    .collect()
            };
            let (cleared, written) = (disk(&["clear"]), disk(&["save_report", "saveReport"]));
            assert!(!cleared.is_empty(), "{language} {side}");
            let folder = |file: &str| file.rsplit_once('/').map(|(d, _)| format!("{d}/"));
            for (dir, by) in &cleared {
                assert!(
                    written.iter().any(|(file, callers)| callers == by && folder(file).as_deref() == Some(dir.as_str())),
                    "{language} {side}: {:?} is no folder of {written:?}",
                    (dir, by)
                );
            }
        }
    }
}

/// Places on the disk are ordered by their terms (`docs/reference/mathematics.md`, "Paths as terms"): a listing of a
/// folder reads the fiber below it, so two routes meet where a place one writes or makes lies in a place the other
/// reads (#381). The meets pair (`meets/{python,rust,typescript}`) keeps downloads per kind in one folder: `fetch`
/// writes a file below it, `unpack` makes a folder below it, `kinds` lists it. The clean side names the folder by one
/// constant, and in every language the file and the folder each lie in the folder `kinds` lists, keeping a place of
/// their own (the order is a quotient, no fact dropped). The shortcut takes the folder from settings Atlas does not
/// follow for the writes and spells it out for the listing: nothing in its code says the three are one folder, so
/// nothing it writes or makes lies in what it lists.
#[test]
fn a_place_written_below_a_listed_folder_lies_in_it() {
    for language in ["python", "rust", "typescript"] {
        for side in ["clean", "shortcut"] {
            let g = pair(&format!("meets/{language}/{side}"));
            let flow = atlas_engine::flow::of(&g);
            let at = |name: &str, verb: &str| -> Vec<String> {
                g.nodes
                    .iter()
                    .filter(|n| !n.library && n.name == name)
                    .flat_map(|n| flow.drawn.get(&n.id).into_iter().flatten())
                    .filter(|w| w.verb == verb && !w.place.file.is_empty())
                    .map(|w| w.place.id.clone())
                    .collect()
            };
            let listed = at("kinds", "lists");
            assert_eq!(listed.len(), 1, "{language} {side}: the folder `kinds` lists, {listed:?}");
            for (name, verb) in [("fetch", "writes"), ("unpack", "creates")] {
                let placed = at(name, verb);
                assert!(!placed.is_empty() && !placed.contains(&listed[0]), "{language} {side}: `{name}` keeps its own place, {placed:?}");
                let meets = placed.iter().all(|p| flow.lies_in.get(p).is_some_and(|above| above.contains(&listed[0])));
                let none = placed.iter().all(|p| flow.lies_in.get(p).is_none_or(|above| !above.contains(&listed[0])));
                match side {
                    "clean" => assert!(meets, "{language}: `{name}`'s {placed:?} lies in {listed:?}: {:?}", flow.lies_in),
                    _ => assert!(none, "{language}: `{name}`'s {placed:?} lies in nothing listed: {:?}", flow.lies_in),
                }
            }
        }
    }
}

/// An empty default decides no place (`docs/reference/mathematics.md`, "Paths as terms"): the empty
/// path names nothing, so an input that falls back to it is named as the input alone. The unset
/// pair (`unset/{python,rust,typescript}`) keeps a report and a log in the files the environment
/// names, and reads the report back; the clean side asks for each variable, the shortcut falls back
/// to the empty path, for the name a caller hands (`os.environ.get(name, "")`,
/// `unwrap_or(String::new())`, `?? ""`) and for the one its lookup writes
/// (`os.environ.get("REPORT", "")`, `unwrap_or_default()`). Each side draws two places, one per
/// variable, the same on both sides and in every language: the shortcut's two empty defaults are
/// not one file.
#[test]
fn an_empty_default_decides_no_place() {
    use std::collections::BTreeSet;
    let mut drawn = vec![];
    for language in ["python", "rust", "typescript"] {
        for side in ["clean", "shortcut"] {
            let g = pair(&format!("unset/{language}/{side}"));
            let places: BTreeSet<String> = g
                .nodes
                .iter()
                .filter(|n| !n.library)
                .flat_map(|n| &n.touches)
                .filter(|t| t.kind == World::Disk)
                .map(|t| t.place().id)
                .collect();
            assert_eq!(places.len(), 2, "{language} {side}: {places:?}");
            drawn.push(places);
        }
    }
    assert!(drawn.windows(2).all(|w| w[0] == w[1]), "{drawn:?}");
}

/// The file a database is opened on is named as a file is, however the code builds its path
/// (`docs/reference/mathematics.md`, "Paths as terms"): the opened pair (`opened/{python,rust}`)
/// keeps counts per kind, each in a database file of its own; the clean side joins the path part by
/// part, the shortcut glues it together as text. Isomorphic is the same: both sides and both
/// languages open the same files by the same names, and two files a function opens are two.
#[test]
fn a_database_is_named_by_its_file_however_its_path_is_built() {
    let mut drawn = vec![];
    for language in ["python", "rust"] {
        for side in ["clean", "shortcut"] {
            let g = pair(&format!("opened/{language}/{side}"));
            let mut opened: Vec<(String, String, String)> = g
                .nodes
                .iter()
                .filter(|n| !n.library)
                .flat_map(|n| n.touches.iter().map(move |t| (n.name.clone(), t)))
                .filter(|(_, t)| matches!(t.kind, World::Db(_)))
                .map(|(f, t)| (f, t.what.clone(), t.file.clone()))
                .collect();
            opened.sort();
            assert_eq!(opened.iter().filter(|(f, _, _)| f == "both").count(), 2, "{language} {side}: {opened:?}");
            drawn.push(opened);
        }
    }
    assert!(drawn.windows(2).all(|w| w[0] == w[1]), "{drawn:?}");
}

/// A directory made for a file the same function writes is part of what is done at that file,
/// however coarse a place the file's own unknown parts leave it: the folder `data/{kind}/cache/` is
/// made for `data/{kind}/cache/{kind}.json`, which rests in `data/`. The test computes the places a
/// function that makes the folder and writes the file touches, and those of one that only writes
/// the file, and compares them, in each language.
#[test]
fn a_folder_made_for_a_file_is_that_files_in_every_language() {
    for (language, made, written) in [
        (
            "python",
            "    folder = DATA / kind / \"cache\"\n    folder.mkdir(parents=True, exist_ok=True)\n    (folder / (kind + \".json\")).write_text(\"{}\")",
            "    folder = DATA / kind / \"cache\"\n    (folder / (kind + \".json\")).write_text(\"{}\")",
        ),
        (
            "rust",
            "    let folder = Path::new(DATA).join(kind).join(\"cache\");\n    fs::create_dir_all(&folder)?;\n    fs::write(folder.join(format!(\"{kind}.json\")), \"{}\")?;",
            "    let folder = Path::new(DATA).join(kind).join(\"cache\");\n    fs::write(folder.join(format!(\"{kind}.json\")), \"{}\")?;",
        ),
    ] {
        let file = run_places(language, "folder-written", written);
        assert_eq!(file.len(), 1, "{language}: {file:?}");
        assert_eq!(run_places(language, "folder-made", made), file, "{language}");
    }
}

/// Test code is one predicate and a test is an entry of kind `test`, in every language (`mathematics.md`, "A filter: the
/// program without its test code"; #403, #404). The tests pair (`tests/{python,rust,typescript}`) is one shelf with one
/// door, `load`, and one test of it, handed a sample by a helper. The clean side keeps the test where the runner's layout
/// puts it beside the package (pytest's `tests/`, a crate's `tests/`, a `tests/` beside `src/`); the shortcut keeps it in
/// the package beside the store it tests (a `test_*.py` in the package, a `#[test]` in `store.rs`, a `*.test.ts` beside
/// `disk.ts`), where the runner collects it and the package ships it. On both sides, in the three languages alike, the
/// fiber without the test code has the same functions and the test code the same, every test the runner collects is an
/// entry of kind `test` by the same label, and its route calls the door. On the shortcut side the test code lies in a
/// module below the package's root that holds the program's code as well, the store, so the overview's fiber cuts it in
/// two; on the clean side no module below a root holds both.
#[test]
fn a_test_is_an_entry_of_kind_test_in_every_language() {
    use atlas_engine::model::{EntryKind, Node, NodeKind};
    use std::collections::BTreeSet;
    type Read = (BTreeSet<String>, BTreeSet<String>, BTreeSet<String>, BTreeSet<String>);
    for side in ["clean", "shortcut"] {
        let read = |language: &str| -> Read {
            let g = pair(&format!("tests/{language}/{side}"));
            let test = |n: &Node| n.entry.as_ref().is_some_and(|e| e.kind == EntryKind::Test);
            let test_code = |n: &Node| n.test || test(n) || g.node(&n.module).is_some_and(|m| m.test);
            let functions: Vec<&Node> =
                g.nodes.iter().filter(|n| !n.library && matches!(n.kind, NodeKind::Function | NodeKind::Method)).collect();
            let names = |keep: &dyn Fn(&Node) -> bool| functions.iter().filter(|n| keep(n)).map(|n| n.name.clone()).collect();
            let labels = g.nodes.iter().filter(|n| test(n)).map(|n| n.entry.as_ref().unwrap().label.clone()).collect();
            for t in g.nodes.iter().filter(|n| test(n)) {
                let calls: Vec<&str> = g
                    .edges
                    .iter()
                    .filter(|e| e.src == t.id && e.kind == EdgeKind::Calls)
                    .filter_map(|e| g.node(&e.dst))
                    .map(|d| d.name.as_str())
                    .collect();
                assert!(calls.contains(&"load"), "{language} {side}: {} calls {calls:?}, not the door", t.id);
            }
            // a module holds what lies in it or in a module below it; a root lies below none
            let modules: Vec<&Node> = g.nodes.iter().filter(|n| !n.library && n.kind == NodeKind::Module).collect();
            let within = |id: &str, m: &str| id == m || id.starts_with(&format!("{m}."));
            let below_root = |m: &&&Node| modules.iter().any(|a| a.id != m.id && within(&m.id, &a.id));
            let holds = |m: &Node, code: bool| functions.iter().any(|f| within(&f.module, &m.id) && test_code(f) == code);
            let cut = modules.iter().filter(below_root).filter(|m| holds(m, true) && holds(m, false)).map(|m| m.id.clone()).collect();
            (names(&|n| !test_code(n)), names(&|n| test_code(n)), labels, cut)
        };
        let python = read("python");
        assert_eq!(python.0, BTreeSet::from(["load".to_string()]), "{side}: the program's functions");
        assert_eq!(python.1, BTreeSet::from(["sample".to_string(), "test_loads".to_string()]), "{side}: the test code");
        assert_eq!(python.2, BTreeSet::from(["test_loads".to_string()]), "{side}: the tests");
        let cut = if side == "shortcut" { BTreeSet::from(["shelf.store".to_string()]) } else { BTreeSet::new() };
        assert_eq!(python.3, cut, "{side}: the modules the test code cuts");
        for language in ["rust", "typescript"] {
            assert_eq!(read(language), python, "{language} {side}");
        }
    }
}

/// The places the finest plan of each route of `g` (`common::planned`) reads under an occurrence whose path ends in `at`:
/// those a line goes from into it on its own channel.
fn read_under(g: &Graph, name: &str, at: &str) -> BTreeSet<String> {
    let read = |f: &String| -> Option<String> {
        let (from, to) = f.strip_prefix("line ")?.strip_suffix(" own")?.split_once(" > ")?;
        (to.ends_with(at) && from.starts_with("File(")).then(|| from.to_string())
    };
    common::planned(g, name, None).values().flatten().filter_map(read).collect()
}

/// A set-up that ran before holds for a method only where the value the method runs on may be one that set-up built
/// (`docs/reference/mathematics.md`, "Who makes a path whole"; #368). The order pair (#359), in Python, Rust and
/// TypeScript: `review` keeps its report on the shelf `open_shelf` built, then sets the drafts up. The clean side reads
/// the report back from the shelf before it sets the drafts up; the shortcut reads it from the drafts, where it never kept
/// one. Both set-ups ran before the shortcut's read, but the read runs on `drafts`, which only `open_drafts` made: under
/// it the route reads `drafts/report.txt` alone, and `review` reads back nothing it wrote, as the engine's arrow and the
/// page's plan both say. The clean `review` reads back the report it kept.
#[test]
fn a_set_up_holds_for_a_method_only_on_the_value_it_built() {
    let out = "w:File:out/report.txt".to_string();
    for (language, review) in [("python", "shelf.cli.review"), ("rust", "shelf.review"), ("typescript", "shelf.cli.review")] {
        let (clean, shortcut) = (pair(&format!("order/{language}/clean")), pair(&format!("order/{language}/shortcut")));
        let reads_back = |g: &Graph| atlas_engine::flow::arrows(g).remove(review).unwrap_or_default().reads_back;
        assert_eq!(reads_back(&clean), [out.clone()], "{language}: the clean `review` reads back the report it kept");
        assert_eq!(reads_back(&shortcut), Vec::<String>::new(), "{language}: the shortcut reads the drafts, where it kept none");
        let under = read_under(&shortcut, &format!("order-{language}"), "/review/Shelf.report/read");
        assert_eq!(under, BTreeSet::from(["File(drafts/report.txt)".to_string()]), "{language}: the read runs on the drafts alone");
    }
}

/// In a loop, a name rebound after a call holds, in the next round, what the rebinding made: the call is handed every
/// binding of the name that ran before it in the loop's order (`docs/reference/mathematics.md`, "Who makes a path whole";
/// #354). `plans/setup-in-a-loop`, in Python, Rust and TypeScript: `main` sets the drafts up, and each round keeps its
/// report on the shelf and then opens the shelf on `out`. The value the report is kept on is made by both steps, as the
/// engine reads the body (`flow::receiver_makers`) and as the page's plan hands it: `write` under it takes both shelves.
#[test]
fn a_call_in_a_loop_is_handed_what_the_round_before_bound() {
    for (language, keep, makers) in [
        ("python", "keep_report", ["open_drafts", "open_shelf"]),
        ("rust", "keep_report", ["open_drafts", "open_shelf"]),
        ("typescript", "keepReport", ["openDrafts", "openShelf"]),
    ] {
        let at = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/plans/setup-in-a-loop").join(language);
        let g = analyze_repo(&[at], &Options::default()).unwrap();
        let n = g.node("rounds.main").expect("the route");
        let site = n.sites.iter().position(|s| s.to.ends_with(keep)).expect("keeps the report");
        let made: Vec<&str> = atlas_engine::flow::receiver_makers(n, site)
            .expect("the body says which values the shelf is")
            .into_iter()
            .map(|j| n.sites[j].to.rsplit('.').next().unwrap_or_default())
            .collect();
        assert_eq!(made, makers, "{language}: the drafts set up before the loop, and the shelf the round before opened");
        let facts = common::planned(&g, &format!("rounds-{language}"), None).remove("rounds.main").unwrap_or_default();
        let into = format!(" > main/Shelf.{keep}/write value");
        let handed: BTreeSet<String> =
            facts.iter().filter_map(|f| f.strip_prefix("line Shelf·shelf@")?.strip_suffix(into.as_str()).map(String::from)).collect();
        assert_eq!(handed, makers.iter().map(|m| format!("main/{m}")).collect(), "{language}: `write` takes both shelves");
    }
}

/// An input the caller can override is its default where no caller overrides it (`docs/reference/mathematics.md`,
/// "Paths as terms"; #428, #427, #383, #350). The defaults pair (`defaults/{python,rust,typescript}`) keeps a report
/// where its caller says, `out/report.txt` unless told; a download of each kind below the folder `RAW` names; a note
/// where its caller says, else in the workbench of the home it is handed. The command keeps the report twice, handing
/// nothing and handing the default, and the note twice: once through `keep`'s fallback, once straight (`jot`). The
/// clean side writes each language's own default (a parameter's default, an `Option` it unwraps) and asks for `RAW`;
/// the shortcut falls back in the body (`path or …`, `path ?? …`) and to the empty path where `RAW` is not set. In
/// every language and on both sides: a caller that hands nothing writes where the one that hands the default writes,
/// one place and none unnamed (#383); the fallback is a term of the parameters, so the callers complete it to the
/// place the same path written in place names (#350); and the empty default joined keeps the root the variable gives,
/// the clean side's, never the literal below it (#428). The facts are the same in every reading.
#[test]
fn an_input_is_its_default_where_no_caller_overrides_it() {
    use std::collections::BTreeSet;
    // the Disk places each function of the program writes, by its name with the language's spelling taken away
    let places = |g: &Graph, function: &str| -> BTreeSet<String> {
        g.nodes
            .iter()
            .filter(|n| !n.library && n.id.rsplit('.').next().map(|s| s.replace('_', "").to_lowercase()).as_deref() == Some(function))
            .flat_map(|n| &n.touches)
            .filter(|t| t.kind == World::Disk)
            .map(|t| t.artifact.clone())
            .collect()
    };
    let root = |p: &String| p.split('/').next().unwrap_or_default().to_string();
    let mut facts = vec![];
    for language in ["python", "rust", "typescript"] {
        let clean = pair(&format!("defaults/{language}/clean"));
        let fetched: BTreeSet<String> = places(&clean, "fetch").iter().map(root).collect();
        assert_eq!(fetched.len(), 1, "{language}: {fetched:?}");
        for side in ["clean", "shortcut"] {
            let g = pair(&format!("defaults/{language}/{side}"));
            let report = places(&g, "savereport");
            assert_eq!(report, BTreeSet::from(["out/report.txt".to_string()]), "{language} {side}: handing nothing is handing the default");
            let jotted = places(&g, "jot");
            assert_eq!(jotted.len(), 1, "{language} {side}: {jotted:?}");
            let kept = places(&g, "keep");
            assert!(jotted.is_subset(&kept), "{language} {side}: the fallback completed is the path in place: {kept:?} holds {jotted:?}");
            let roots: BTreeSet<String> = places(&g, "fetch").iter().map(root).collect();
            assert_eq!(roots, fetched, "{language} {side}: the empty default joined keeps its root");
            assert!(roots.iter().all(|r| r.starts_with('{')), "{language} {side}: {roots:?} is a value no code decides");
            facts.push((report, jotted, roots));
        }
    }
    assert!(facts.windows(2).all(|w| w[0] == w[1]), "{facts:?}");
}

/// A file its caller names is the term the code names it by, whatever format the operation that touches it reads or
/// writes (`docs/reference/mathematics.md`, "Paths as terms"; #449). The formats pair (`formats/{python,rust,typescript}`)
/// is a table kept as CSV behind two doors: `save` writes its text with an operation that says no format, `load` reads it
/// with one that does where the language's reader knows it (pandas' `read_csv` says CSV). On the shortcut each door takes
/// its file from its caller and names it by nothing else, so each is the place its parameter names, `Disk(save.to)` and
/// `Disk(load.path)`: two forms of one construct, drawn alike, never the format (`File(*.csv)`), and two places, since
/// nothing in the code says they are one. On the clean side both default to one constant, `TABLE`, so the two doors meet
/// at the one file it names. The facts are the same in every reading.
#[test]
fn a_file_the_caller_names_is_its_term_whatever_its_format() {
    // the places on the disk a door touches as its own route sees it: each touch, and each read where no caller runs
    let places = |g: &Graph, door: &str| -> BTreeSet<String> {
        g.nodes
            .iter()
            .filter(|n| !n.library && n.id.rsplit('.').next() == Some(door))
            .flat_map(|n| n.touches.iter().chain(&n.instead))
            .filter(|t| t.kind == World::Disk && !t.shape())
            .map(|t| t.place().label)
            .collect()
    };
    let mut facts = vec![];
    for language in ["python", "rust", "typescript"] {
        let shortcut = pair(&format!("formats/{language}/shortcut"));
        let (saved, loaded) = (places(&shortcut, "save"), places(&shortcut, "load"));
        assert_eq!(saved, BTreeSet::from(["Disk(save.to)".to_string()]), "{language}: the file the caller names, by its parameter");
        assert_eq!(loaded, BTreeSet::from(["Disk(load.path)".to_string()]), "{language}: the file the caller names, by its parameter");
        let clean = pair(&format!("formats/{language}/clean"));
        let (kept, read) = (places(&clean, "save"), places(&clean, "load"));
        assert_eq!(kept, read, "{language}: the doors meet at the file their one default names");
        assert!(kept.iter().all(|p| p.starts_with("File(") && !p.contains('*')), "{language}: {kept:?}");
        facts.push((saved, loaded, kept));
    }
    assert!(facts.windows(2).all(|w| w[0] == w[1]), "{facts:?}");
}

/// The calls to `write` from the functions `inplace` and `bound` of a program written as `files` (path, text) under a
/// fresh directory.
fn calls_to_write(name: &str, files: &[(&str, String)]) -> BTreeSet<(String, String)> {
    let root = std::env::temp_dir().join(format!("atlas-{name}-{}", std::process::id()));
    for (file, text) in files {
        let p = root.join(file);
        std::fs::create_dir_all(p.parent().unwrap()).expect("makes the directory");
        std::fs::write(p, text).expect("writes the file");
    }
    let g = analyze_repo(&[root.clone()], &Options::default()).expect("reads");
    let _ = std::fs::remove_dir_all(&root);
    let short = |id: &str| g.node(id).filter(|n| !n.library).map(|n| n.name.clone());
    g.edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Calls)
        .filter_map(|e| Some((short(&e.src)?, short(&e.dst)?)))
        .filter(|(src, dst)| (src == "inplace" || src == "bound") && dst == "write")
        .collect()
}

/// A method called on a value built where it is called has the type it was built as, whatever the language's form: it is
/// linked to the type's method exactly as it is on a value bound to a name first. `inplace` calls `write` on a value built
/// in place, `bound` on one bound to a name; Python's `Html(text)`, TypeScript's `new Html(text)` and Rust's tuple-struct
/// call `Html(text)` and constructor `Html::new(text)` each read the same two calls.
#[test]
fn a_method_on_a_value_built_in_place_is_linked_in_every_language() {
    let python = |built: &str| {
        vec![
            ("pyproject.toml", "[project]\nname = \"demo\"\nversion = \"0\"\n".to_string()),
            ("src/demo/__init__.py", String::new()),
            (
                "src/demo/core.py",
                format!(
                    "class Html:\n    def __init__(self, text: str) -> None:\n        self.text = text\n\n    def write(self, out: str) -> None:\n        print(out, self.text)\n\n\ndef inplace(text: str, out: str) -> None:\n    {built}.write(out)\n\n\ndef bound(text: str, out: str) -> None:\n    page = Html(text)\n    page.write(out)\n"
                ),
            ),
        ]
    };
    let typescript = |built: &str| {
        vec![
            ("package.json", "{\"name\": \"demo\", \"version\": \"0.0.0\"}\n".to_string()),
            (
                "src/core.ts",
                format!(
                    "export class Html {{\n  constructor(private text: string) {{}}\n\n  write(out: string): void {{\n    console.log(out, this.text);\n  }}\n}}\n\nexport function inplace(text: string, out: string): void {{\n  {built}.write(out);\n}}\n\nexport function bound(text: string, out: string): void {{\n  const page = new Html(text);\n  page.write(out);\n}}\n"
                ),
            ),
        ]
    };
    let rust = |built: &str| {
        vec![
            ("Cargo.toml", "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n".to_string()),
            (
                "src/lib.rs",
                format!(
                    "pub struct Html(pub String);\n\nimpl Html {{\n    pub fn new(text: String) -> Self {{\n        Html(text)\n    }}\n\n    pub fn write(&self, out: &str) {{\n        println!(\"{{}} {{}}\", out, self.0);\n    }}\n}}\n\npub fn inplace(text: String, out: &str) {{\n    {built}.write(out);\n}}\n\npub fn bound(text: String, out: &str) {{\n    let page = Html(text);\n    page.write(out);\n}}\n"
                ),
            ),
        ]
    };
    let law = BTreeSet::from([("inplace".to_string(), "write".to_string()), ("bound".to_string(), "write".to_string())]);
    for (name, files) in [
        ("inplace-python", python("Html(text)")),
        ("inplace-typescript", typescript("new Html(text)")),
        ("inplace-rust-tuple", rust("Html(text)")),
        ("inplace-rust-new", rust("Html::new(text)")),
    ] {
        assert_eq!(calls_to_write(name, &files), law, "{name}");
    }
}

/// A name the body binds is the body's value, never the module's function of that name (`docs/reference/mathematics.md`,
/// "Paths as terms": a binding is a name a function gives a value; #375). The shadows pair (`shadows/{python,rust,
/// typescript}`) sorts letters onto a pile. `sort` binds a function that tidies a name, the letters it keeps, those it sends
/// back and how many came, calls the function it binds and hands the others on; the clean side names them apart from the
/// module's functions, the shortcut by the module's own names (`tidy`, `addressed`, `size`). Naming a local like a function
/// changes nothing the code does, so on the shortcut each route runs the program's functions it runs on the clean side, and
/// each reading runs those of every other language. A name no binding takes is the module's function: `total` hands `size`
/// itself on to `map`, which runs it on each pile, and `addressed` calls `tidy`, on both sides and in every language.
#[test]
fn a_name_the_body_binds_is_never_the_modules_function() {
    use atlas_engine::NodeKind;
    use std::collections::BTreeMap;
    // the program's functions a route's steps run and its lines call, by their names
    let runs = |g: &Graph, route: &str| -> BTreeSet<String> {
        let id = format!("inbox.{route}");
        let n = g.node(&id).unwrap_or_else(|| panic!("no {id}"));
        let own = |to: &str| g.node(to).is_some_and(|x| !x.library && x.kind == NodeKind::Function);
        let steps = n.sites.iter().map(|s| s.to.as_str());
        let calls = g.edges.iter().filter(|e| e.src == id && e.kind == EdgeKind::Calls).map(|e| e.dst.as_str());
        steps.chain(calls).filter(|t| own(t)).map(|t| t.rsplit('.').next().unwrap_or_default().to_string()).collect()
    };
    let (languages, routes) = (["python", "rust", "typescript"], ["sort", "total", "addressed"]);
    let mut read = BTreeMap::new();
    for language in languages {
        for side in ["clean", "shortcut"] {
            let g = pair(&format!("shadows/{language}/{side}"));
            read.insert((language, side), routes.map(|r| (r, runs(&g, r))));
        }
    }
    let mut broken = vec![];
    for ((language, side), facts) in &read {
        let clean = &read[&(*language, "clean")];
        if facts != clean {
            broken.push(format!("{language} {side} runs {facts:?}, its clean side {clean:?}"));
        }
        let named = |r: &str, f: &str| facts.iter().any(|(route, runs)| *route == r && runs.contains(f));
        if !named("total", "size") || !named("addressed", "tidy") {
            broken.push(format!("{language} {side}: a name no binding takes is the module's function: {facts:?}"));
        }
    }
    for side in ["clean", "shortcut"] {
        for (i, a) in languages.iter().enumerate() {
            for b in &languages[i + 1..] {
                if read[&(*a, side)] != read[&(*b, side)] {
                    broken.push(format!("{side}: {a} runs {:?}, {b} {:?}", read[&(*a, side)], read[&(*b, side)]));
                }
            }
        }
    }
    assert!(broken.is_empty(), "{}", broken.join("\n"));
}

/// A value a change gives anew is handed on with the type it was given (`docs/reference/mathematics.md`, "Paths as terms": a
/// binding is a name a function gives a value; #394). In the lookups pair (`lookups/{python,rust,typescript}`) the routes
/// `pushed`, `added` and `grown`, each by a subscript and by a lookup, change a list in place and then hand it to `draw` or
/// `show`: the step after the change takes the value the change gave, by the name it gave it, so with the type it gave it.
/// What the change gives is one value in every language, so each reading gives and hands the types every other does.
#[test]
fn a_value_a_change_gives_is_handed_on_with_its_type() {
    use std::collections::BTreeMap;
    let languages = ["python", "rust", "typescript"];
    let routes = ["pushed", "pushedlookedup", "added", "addedlookedup", "grown", "grownlookedup"];
    // a route by its name with the language's spelling taken away: `pushedlookedup` of `pushed_looked_up` and `pushedLookedUp`
    let flat = |id: &str| id.rsplit('.').next().unwrap_or_default().replace('_', "").to_lowercase();
    let mut read: BTreeMap<(&str, &str), BTreeMap<String, Vec<(String, String)>>> = BTreeMap::new();
    let mut broken = vec![];
    for language in languages {
        for side in ["clean", "shortcut"] {
            let g = pair(&format!("lookups/{language}/{side}"));
            let mut facts = BTreeMap::new();
            for n in g.nodes.iter().filter(|n| !n.library && routes.contains(&flat(&n.id).as_str())) {
                let at = format!("{language} {side} {}", n.id);
                let changing: Vec<usize> = (0..n.sites.len()).filter(|i| n.sites[*i].changes).collect();
                let [c] = changing[..] else { panic!("{at}: one step changes the list: {:?}", n.sites) };
                let after = n.sites.get(c + 1).unwrap_or_else(|| panic!("{at}: a step takes the list after the change"));
                // of each value the change gives, the type it gives it and the types the step after is handed it by, by its name
                let mut given: Vec<(String, String)> = n.sites[c]
                    .gives
                    .iter()
                    .map(|g| {
                        let handed: Vec<&str> = after.hands.iter().filter(|h| h.name == g.name).map(|h| h.ty.as_str()).collect();
                        (g.ty.clone(), handed.join(" | "))
                    })
                    .collect();
                given.sort();
                for (gives, hands) in given.iter().filter(|(g, h)| g != h) {
                    broken.push(format!("{at}: `{}` gives `{gives}`, `{}` is handed `{hands}`", n.sites[c].op, after.to));
                }
                facts.insert(flat(&n.id), given);
            }
            assert_eq!(facts.len(), routes.len(), "{language} {side}: {:?}", facts.keys());
            read.insert((language, side), facts);
        }
    }
    for side in ["clean", "shortcut"] {
        for r in routes {
            let types: Vec<(&str, &Vec<(String, String)>)> = languages.iter().map(|l| (*l, &read[&(*l, side)][r])).collect();
            // a reading is named where another that differs from it says as many types or more: a type no reading says is
            // missing in none, two that are said and differ are wrong in both
            let typed =
                |v: &Vec<(String, String)>| v.iter().map(|(g, h)| usize::from(!g.is_empty()) + usize::from(!h.is_empty())).sum::<usize>();
            for (language, t) in &types {
                if types.iter().any(|(l, o)| l != language && o != t && typed(o) >= typed(t)) {
                    broken.push(format!("{side} {r}: {language} gives and hands {t:?}, the readings {types:?}"));
                }
            }
        }
    }
    assert!(broken.is_empty(), "{}", broken.join("\n"));
}

/// A field holds what the constructor that built the value set (`docs/reference/mathematics.md`, "A function's return is
/// its summary"; #407, #395). The fields pair (`fields/{python,rust,typescript}`): a shelf holds its folder, its methods
/// name a file in it; `open_archive` builds the archive and hands it on to the method that writes the note, `read_index`
/// builds the drafts where it reads their index. The clean shelf is handed its folder, the shortcut's constructor puts
/// it under one it names itself. In every language and on both sides: the index read on the shelf built in place is that
/// shelf's alone, never the archive's (#407); the note written on the shelf handed in is made whole by the builder and
/// the method together (the conjunction), and its place is decided where its start is written: by the builder on the
/// clean side, by the constructor on the shortcut's (#395). The facts are the same in every reading.
#[test]
fn a_field_holds_what_the_constructor_that_built_the_value_set() {
    // a function by its name with the language's spelling taken away, a type's constructor by one name
    let name = |id: &str| -> String {
        let last = id.rsplit(['.', ':']).next().unwrap_or_default().replace('_', "").to_lowercase();
        if matches!(last.as_str(), "new" | "init" | "constructor") {
            "constructor".to_string()
        } else {
            last
        }
    };
    let touches = |g: &Graph, function: &str| -> BTreeSet<(String, Vec<String>, BTreeSet<String>)> {
        g.nodes
            .iter()
            .filter(|n| !n.library && name(&n.id) == function)
            .flat_map(|n| &n.touches)
            .filter(|t| t.kind == World::Disk)
            .map(|t| {
                let by = t
                    .by
                    .iter()
                    .map(|b| b.0.iter().map(|a| name(&a.to_string())).collect::<BTreeSet<_>>().into_iter().collect::<Vec<_>>().join("&"));
                let by = by.collect();
                (t.artifact.clone(), t.fixed_by.iter().map(|f| name(f)).collect(), by)
            })
            .collect()
    };
    let mut facts = vec![];
    for language in ["python", "rust", "typescript"] {
        for (side, below, decided) in [("clean", "", "openarchive"), ("shortcut", "shelves/", "constructor")] {
            let g = pair(&format!("fields/{language}/{side}"));
            let read: BTreeSet<String> = touches(&g, "readindex").into_iter().map(|(at, ..)| at).collect();
            assert_eq!(read, BTreeSet::from([format!("{below}drafts/index.txt")]), "{language} {side}: the shelf built in place");
            let written = touches(&g, "write");
            let archive = written.iter().find(|(at, ..)| *at == format!("{below}archive/note.txt"));
            let (_, fixed_by, by) = archive.unwrap_or_else(|| panic!("{language} {side}: {written:?}"));
            assert_eq!(by, &BTreeSet::from(["keep&openarchive".to_string()]), "{language} {side}: the builder and the method");
            assert_eq!(fixed_by, &[decided.to_string()], "{language} {side}: decided where the start is written");
            facts.push((side, read, written));
        }
    }
    for side in ["clean", "shortcut"] {
        let same: Vec<_> = facts.iter().filter(|(s, ..)| *s == side).map(|(_, read, written)| (read, written)).collect();
        assert!(same.windows(2).all(|w| w[0] == w[1]), "{side}: {same:?}");
    }
}

/// A variant's field taken out by the pattern that matches it is the value built in, as a struct's field read back is
/// (`docs/reference/mathematics.md`, "A function's return is its summary"; #349). The kept pair of sums
/// (`sums/kept/{python,rust}`): an attachment is kept in a file of its own or in a folder with an index. The clean side
/// keeps where as a variant built with its path (`Plek::Bestand(path)`, `Bestand(path)`) and takes it apart in a `match`
/// (`Plek::Bestand(pad) =>`, `case Bestand(pad):`), so the reads under its arms are the places built in, decided where
/// they are built; the shortcut writes the kind before the path into one string and splits it again, so no place it
/// reads is one the code decides. The facts are the same in both readings. TypeScript's discriminated union is not read
/// this way yet.
#[test]
fn a_variant_s_field_taken_out_by_a_match_is_the_value_built_in() {
    let places = |g: &Graph| -> BTreeSet<(String, Vec<String>)> {
        g.nodes
            .iter()
            .filter(|n| !n.library && n.id.ends_with("lees"))
            .flat_map(|n| &n.touches)
            .filter(|t| t.kind == World::Disk && !t.artifact.is_empty())
            .map(|t| (t.artifact.clone(), t.fixed_by.iter().map(|f| f.rsplit('.').next().unwrap_or_default().to_string()).collect()))
            .collect()
    };
    let built = BTreeSet::from([
        ("bijlagen/bron.txt".to_string(), vec!["main".to_string()]),
        ("bijlagen/los/index.txt".to_string(), vec!["main".to_string()]),
    ]);
    for language in ["python", "rust"] {
        assert_eq!(places(&pair(&format!("sums/kept/{language}/clean"))), built, "{language}: the places built into the variants");
        let split = places(&pair(&format!("sums/kept/{language}/shortcut")));
        assert!(split.iter().all(|(at, by)| at.starts_with('{') && by.is_empty()), "{language}: no place the code decides: {split:?}");
    }
}

/// The pair `abstract` (`tests/pairs/abstract/{python,rust,typescript}`): two families whose members each write a report
/// somewhere of their own, and an entry for each that runs the family's method on a value of the family's type, which a
/// factory picks. On the clean side the family only declares the method, in both of each language's forms: an abstract
/// method (`@abstractmethod`, TypeScript's `abstract`) and a protocol's or an interface's (`Protocol`, `interface`); in
/// Rust both are a trait's method without a default body. On the shortcut the base writes a report of its own, a default
/// its members override (`out/report.txt`, `store/report.txt`). Each entry, the method it calls, and the members that
/// fill it in, as the code names them.
const DECLARED: [(&str, &str, [&str; 2]); 2] = [
    ("publish", "Exporter.export", ["CsvExporter.export", "JsonExporter.export"]),
    ("archive", "Store.keep", ["Shelf.keep", "Vault.keep"]),
];

/// What the shortcut's bases write of their own.
const DEFAULTS: [(&str, &str); 2] = [("Exporter.export", "out/report.txt"), ("Store.keep", "store/report.txt")];

const LANGUAGES: [&str; 3] = ["python", "rust", "typescript"];

fn declared_pair(language: &str) -> &'static common::Pair {
    static PAIRS: [std::sync::OnceLock<common::Pair>; 3] = [const { std::sync::OnceLock::new() }; 3];
    let i = LANGUAGES.iter().position(|l| *l == language).expect("a language of the pair");
    PAIRS[i].get_or_init(|| common::pair(&format!("abstract/{language}")))
}

/// The program's function whose id ends in `short` (`Exporter.export`): the class and the name the code gives it.
fn function<'g>(g: &'g Graph, short: &str) -> &'g atlas_engine::Node {
    let callable = |n: &atlas_engine::Node| matches!(n.kind, atlas_engine::NodeKind::Method | atlas_engine::NodeKind::Function);
    g.nodes
        .iter()
        .find(|n| !n.library && callable(n) && n.id.ends_with(&format!(".{short}")))
        .unwrap_or_else(|| panic!("no function {short}"))
}

/// The id of the program's entry `name` (`publish`).
fn entry_named(g: &Graph, name: &str) -> String {
    g.nodes.iter().find(|n| !n.library && n.entry.is_some() && n.name == name).unwrap_or_else(|| panic!("no entry {name}")).id.clone()
}

/// A function's own arrow as the engine forms it (sentence 2): the types of the code's its arrow takes, and those it
/// gives (gives back, builds, changes), each as the place the viewer's plan names it (`t:` and the type), and the places of
/// the world its own body reads (`takes`) and writes (`gives`). What it reaches through its calls is no part of it
/// (sentence 6).
fn own_in_engine(g: &Graph, id: &str) -> BTreeSet<String> {
    let arrows = atlas_engine::flow::arrows(g);
    let a = arrows.get(id).cloned().unwrap_or_default();
    let own = |t: &String| g.node(t).is_some_and(|n| !n.library && n.kind == atlas_engine::NodeKind::Class);
    let mut out: BTreeSet<String> = a.takes.iter().filter(|t| own(t)).map(|t| format!("takes t:{t}")).collect();
    out.extend(a.gives.iter().chain(&a.changes).filter(|t| own(t)).map(|t| format!("gives t:{t}")));
    if !a.builds.is_empty() {
        out.insert(format!("gives t:{}", a.builds));
    }
    for t in &g.node(id).expect("the function is a node").touches {
        let way = if t.dir == Some(atlas_engine::Dir::Read) { "takes" } else { "gives" };
        out.insert(format!("{way} w:File:{}", t.artifact));
    }
    out
}

/// The own arrow of every function in the overview's fine plan `F₀`, as the viewer's model builds it (`plans.mjs --own`),
/// by the function: each place its occurrence takes and gives.
fn own_in_overview(g: &Graph, name: &str) -> BTreeMap<String, BTreeSet<String>> {
    let plans = common::planned_routes(g, name, None, &["--own"]);
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for f in plans.get("@own").into_iter().flatten() {
        let Some((function, line)) = f.strip_prefix("own ").and_then(|f| f.split_once(' ')) else { continue };
        out.entry(function.to_string()).or_default().insert(line.to_string());
    }
    out
}

/// The facts of the finest plan of the route of `entry` (`publish`), as `plans.mjs` writes them.
fn route_plan(g: &Graph, name: &str, entry: &str) -> BTreeSet<String> {
    let id = entry_named(g, entry);
    common::planned_routes(g, name, None, &[&id]).remove(&id).unwrap_or_default()
}

/// The lines of a plan, each as `(from, to, channel)`.
fn plan_lines(facts: &BTreeSet<String>) -> Vec<(String, String, String)> {
    facts
        .iter()
        .filter_map(|f| f.strip_prefix("line "))
        .filter_map(|l| {
            let (from, rest) = l.split_once(" > ")?;
            let (to, channel) = rest.split_once(' ')?;
            Some((from.to_string(), to.to_string(), channel.to_string()))
        })
        .collect()
}

/// The type label of a value of a plan (`Receipt@publish/Exporter.export` is a `Receipt`).
fn type_of(value: &str) -> &str {
    value.split(['@', '·']).next().unwrap_or(value)
}

/// A function with no body has an empty own arrow (sentence 2: its domain is its parameters plus what its body consumes,
/// its codomain what it returns plus what it writes; a declared method runs no body, so it consumes, returns and writes
/// nothing). Its signature is the type of the arrow each of its members must be, never an arrow of its own. So in every
/// language and in both of each language's forms (an abstract method, a protocol's or an interface's method, a trait's
/// method without a default body): the method is declared, its own arrow as the engine forms it is empty, and the types
/// the overview reads off who makes and takes them (`Families::makes`, `Families::takes`) do not name it (#430).
#[test]
fn a_declared_method_has_an_empty_own_arrow_in_every_language() {
    let mut faults = vec![];
    for language in LANGUAGES {
        let g = &declared_pair(language).clean;
        let types = atlas_engine::families::families(g);
        for (_, base, _) in DECLARED {
            let n = function(g, base);
            if !n.declared {
                faults.push(format!("{language}: {base} is not read as declared"));
            }
            let own = own_in_engine(g, &n.id);
            if !own.is_empty() {
                faults.push(format!("{language}: {base} has the own arrow {own:?}"));
            }
            for (way, uses) in [("makes", &types.makes), ("takes", &types.takes)] {
                for (t, _) in uses.iter().filter(|(_, by)| by.contains(&n.id)) {
                    faults.push(format!("{language}: the overview's types say {base} {way} {t}"));
                }
            }
        }
    }
    assert!(faults.is_empty(), "{}", faults.join("\n"));
}

/// A default body is a body: a method a base defines and its members override has the own arrow its body forms, as any
/// function does. The shortcut's bases write a report of their own, so their own arrow takes the Report, gives the
/// Receipt and writes their file, in every language, where the clean side's declared method has none (#430).
#[test]
fn a_default_body_has_its_own_arrow_in_every_language() {
    for language in LANGUAGES {
        let g = &declared_pair(language).shortcut;
        for (base, file) in DEFAULTS {
            let n = function(g, base);
            assert!(!n.declared, "{language}: {base} has a body");
            let own = own_in_engine(g, &n.id);
            let named = |way: &str, end: &str| own.iter().any(|o| o.starts_with(way) && o.ends_with(end));
            assert!(named("takes t:", ".Report") && named("gives t:", ".Receipt"), "{language}: {base} {own:?}");
            assert!(own.contains(&format!("gives w:File:{file}")), "{language}: {base} {own:?}");
        }
    }
}

/// A function's own arrow is one quantity, whoever forms it: the overview's fine plan `F₀` gives each function's
/// occurrence its own arrow (`docs/reference/mathematics.md`, "One fine plan, every picture its image"), and that is the
/// arrow the engine forms. So for the declared methods, their members and the bases with a default body, on both sides of
/// the pair and in every language, the two agree: neither changes without the other.
#[test]
fn the_own_arrow_is_one_in_the_engine_and_the_overview_in_every_language() {
    for language in LANGUAGES {
        let p = declared_pair(language);
        for (side, g) in [("clean", &p.clean), ("shortcut", &p.shortcut)] {
            let overview = own_in_overview(g, &format!("abstract-own-one-{language}-{side}"));
            for short in DECLARED.iter().flat_map(|(_, base, members)| std::iter::once(*base).chain(members.iter().copied())) {
                let id = &function(g, short).id;
                assert_eq!(overview.get(id).cloned().unwrap_or_default(), own_in_engine(g, id), "{language} {side}: {short}");
            }
        }
    }
}

/// A declared method draws no line of its own (#430): in the overview's fine plan `F₀` the occurrence of a declared
/// method takes and gives nothing, and in the plan of a route that calls it, the step of the call, a choice of the members
/// that fill it in, takes nothing its own body would read and writes nothing: its only lines are what its members hand
/// back to it (`T@step/?`) and what it hands on of that (`T@step`). The shortcut's default body draws lines of its own
/// there, the Report it reads and the file it writes, as any body does.
#[test]
fn a_declared_method_draws_no_line_of_its_own_in_every_language() {
    let mut faults = vec![];
    for language in LANGUAGES {
        let p = declared_pair(language);
        for (side, g) in [("clean", &p.clean), ("shortcut", &p.shortcut)] {
            let overview = own_in_overview(g, &format!("abstract-own-lines-{language}-{side}"));
            for (entry, base, _) in DECLARED {
                let in_overview = overview.get(&function(g, base).id).cloned().unwrap_or_default();
                let step = format!("{entry}/{base}");
                let facts = route_plan(g, &format!("abstract-lines-{language}-{side}-{entry}"), entry);
                let own: Vec<_> = plan_lines(&facts)
                    .into_iter()
                    .filter(|(from, to, _)| *from == step || *to == step)
                    .filter(|(from, to, channel)| {
                        let back = *to == step && from.ends_with(&format!("@{step}/?")) && channel == "value";
                        let on = *from == step && to.ends_with(&format!("@{step}")) && channel == "value";
                        !back && !on
                    })
                    .collect();
                if side == "shortcut" {
                    assert!(!in_overview.is_empty(), "{language} shortcut: in the overview, {base}'s default body draws its lines");
                    assert!(own.iter().any(|(_, _, c)| c == "own"), "{language} shortcut: {step} writes its file: {own:?}");
                    continue;
                }
                if !in_overview.is_empty() {
                    faults.push(format!("{language}: in the overview, {base} draws {in_overview:?}"));
                }
                if !own.is_empty() {
                    faults.push(format!("{language}: in the route {entry}, {step} draws {own:?}"));
                }
            }
        }
    }
    assert!(faults.is_empty(), "{}", faults.join("\n"));
}

/// A call of a declared method runs one of the members that fill it in: where the code does not say which, it may run
/// each of them, and its codomain is the coproduct of theirs (`docs/reference/mathematics.md`, Coproducts: a call into a
/// family). So in every language, on both sides, the step of the call in its route is a choice of each member (and of the
/// base's own body where it has one), and what the route writes, as the engine reads it along the route (`flow::along`)
/// and as its plan draws it, is the union of what those bodies write; and the step hands on what its members hand back.
#[test]
fn a_call_of_a_declared_method_runs_each_member_in_every_language() {
    for language in LANGUAGES {
        let p = declared_pair(language);
        for (side, g) in [("clean", &p.clean), ("shortcut", &p.shortcut)] {
            for (entry, base, members) in DECLARED {
                let at = format!("{language} {side} {entry}");
                let n = function(g, base);
                let runs: Vec<&atlas_engine::Node> = members.iter().map(|m| function(g, m)).chain((!n.declared).then_some(n)).collect();
                let written: BTreeSet<String> = runs.iter().flat_map(|f| f.touches.iter().map(|t| t.artifact.clone())).collect();
                let along: BTreeSet<String> = atlas_engine::flow::along(g, &entry_named(g, entry))
                    .into_values()
                    .flatten()
                    .filter_map(|k| k.strip_prefix("w:File:").map(String::from))
                    .collect();
                assert_eq!(along, written, "{at}: along the route");
                let facts = route_plan(g, &format!("abstract-runs-{language}-{side}-{entry}"), entry);
                let drawn: BTreeSet<String> =
                    facts.iter().filter_map(|f| f.strip_prefix("side File(")?.split_once(')').map(|(p, _)| p.to_string())).collect();
                assert_eq!(drawn, written, "{at}: in the plan");
                let step = format!("{entry}/{base}");
                assert!(facts.contains(&format!("step {step} one of {}", runs.len())), "{at}: {step} runs one of {}", runs.len());
                // the steps below the call that run the method on a class, apart from the steps of the base's own body
                let method = base.rsplit('.').next().unwrap_or(base);
                let alternatives: BTreeSet<String> = facts
                    .iter()
                    .filter_map(|f| f.strip_prefix(&format!("step {step}/")))
                    .filter(|s| !s.contains('/') && s.ends_with(&format!(".{method}")))
                    .map(String::from)
                    .collect();
                assert_eq!(alternatives, members.iter().map(|m| m.to_string()).collect(), "{at}: the members {step} runs");
                let lines = plan_lines(&facts);
                let back: BTreeSet<&str> = lines
                    .iter()
                    .filter(|(from, to, _)| members.iter().any(|m| *from == format!("{step}/{m}")) && to.ends_with(&format!("@{step}/?")))
                    .map(|(_, to, _)| type_of(to))
                    .collect();
                let on: BTreeSet<&str> = lines
                    .iter()
                    .filter(|(from, to, _)| *from == step && to.ends_with(&format!("@{step}")))
                    .map(|(_, to, _)| type_of(to))
                    .collect();
                assert!(!back.is_empty() && back == on, "{at}: {step} hands on {on:?}, its members hand back {back:?}");
            }
        }
    }
}

/// Isomorphic is the same: an abstract method, a protocol's, an interface's and a trait's method without a default body
/// say one thing, a signature the members fill in, and a default body says another in every language. So what Atlas
/// reads of each side of the pair is one reading in every language, by the names the code gives: whether each method is
/// declared, its own arrow in the engine and in the overview, and the plan of each route through it.
#[test]
fn a_declared_method_reads_alike_in_every_language() {
    // a type by the name the code gives it
    let short = |s: &str| -> String {
        match s.split_once(" t:") {
            Some((way, t)) => format!("{way} t:{}", t.rsplit('.').next().unwrap_or(t)),
            None => s.to_string(),
        }
    };
    for side in ["clean", "shortcut"] {
        let reading = |language: &str| {
            let p = declared_pair(language);
            let g = if side == "clean" { &p.clean } else { &p.shortcut };
            let overview = own_in_overview(g, &format!("abstract-own-alike-{language}-{side}"));
            let mut out = BTreeSet::new();
            for (entry, base, _) in DECLARED {
                let n = function(g, base);
                out.insert(format!("{base} declared {}", n.declared));
                out.extend(own_in_engine(g, &n.id).iter().map(|o| format!("{base} engine {}", short(o))));
                out.extend(overview.get(&n.id).into_iter().flatten().map(|o| format!("{base} overview {}", short(o))));
                let step = format!("{entry}/{base}");
                let facts = route_plan(g, &format!("abstract-alike-{language}-{side}-{entry}"), entry);
                // the call and what stands directly in it, a member or the choice of one: how each language writes a
                // body is its own
                let method = format!(".{}", base.rsplit('.').next().unwrap_or(base));
                let at_call = |token: &str| {
                    let path = token.rsplit('@').next().unwrap_or(token);
                    match path.strip_prefix(&format!("{step}/")) {
                        Some(rest) => rest == "?" || (!rest.contains('/') && rest.ends_with(&method)),
                        None => path == step,
                    }
                };
                let kept = |f: &&String| {
                    let tokens: Vec<&str> = f.split(' ').filter(|t| t.contains(&step)).collect();
                    f.starts_with("side ") || (!tokens.is_empty() && tokens.iter().all(|t| at_call(t)))
                };
                // each language spells a function's name its own way (`exporter_for`, `exporterFor`)
                out.extend(facts.iter().filter(kept).map(|f| format!("{entry} {}", f.to_lowercase().replace('_', ""))));
            }
            out
        };
        let python = reading("python");
        for language in &LANGUAGES[1..] {
            let theirs = reading(language);
            let only = |a: &BTreeSet<String>, b: &BTreeSet<String>| a.difference(b).cloned().collect::<Vec<_>>();
            assert!(
                python == theirs,
                "{side}: python and {language} read apart\nonly python: {:?}\nonly {language}: {:?}",
                only(&python, &theirs),
                only(&theirs, &python)
            );
        }
    }
}
