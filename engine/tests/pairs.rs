//! Pairs: the same program twice, once as good practice writes it and once with the shortcut
//! people typically take. The clean one must read as the algebra says; the shortcut must show as
//! a shortcut (a place the code does not name, an open question), never as if it were clean and
//! never not at all. What differs between the two pictures is what Atlas lays bare.

use std::path::PathBuf;

use atlas_engine::parts::analyze_repo;
use atlas_engine::{EdgeKind, Graph, Options, World};

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

/// A shaped array is a type of its own and stands in its rank family (`docs/reference/mathematics.md`, "Shapes and their
/// rank"). The clean components say the shape of what they take and give (`Float[Tensor, "batch dim"]`); the shortcut
/// says `Tensor`. Each shape of the clean side stays its own type, and stands in a family shared by exactly the shapes of
/// its dtype, array type and rank, where the graph holds two of them: the quotient is the rank, never the names (`batch
/// dim` and `batch ndim` are two types of one family), and never the dtype (`Int[Tensor, batch seq]` is rank 2 and
/// alone). A family is named by what it is (`Float Tensor · 2D`), never as a shape the code could have written. The
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
                assert!(!kin.is_empty(), "{a} is alone in its rank and stands in {k}");
                for u in &kin {
                    assert_eq!(types.kind.get(*u), Some(k), "{a} and {u} share a rank");
                }
                for (u, b) in shaped.iter().filter(|(u, _)| types.kind.get(u) == Some(k)) {
                    assert!(same(a, b), "{u} stands in {k} beside {a}");
                }
            }
            None => assert!(kin.is_empty(), "{a} shares its rank with {kin:?} and stands in no family"),
        }
    }
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
/// closure for each. The file read in each folder is one place, and the directory listed is one
/// place, whichever form and language the code takes.
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
    ] {
        assert_eq!(run_places(language, name, body), glob, "{language} {name}");
    }
}

/// The places `run` of a one-module program touches, each with its kind: a program in `language`
/// that keeps a directory `DATA` (`data`) and a table `KINDS` from a key to a folder below it
/// (`a` to `regelingen`, `b` to `ontwerp`), and runs `body` with a `kind` its caller hands.
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
                    "use std::collections::HashMap;\nuse std::fs;\nuse std::path::Path;\n\nconst DATA: &str = \"data\";\n\npub fn run(kind: &str) -> std::io::Result<()> {{\n    let kinds: HashMap<&str, &str> = HashMap::from([(\"a\", \"regelingen\"), (\"b\", \"ontwerp\")]);\n{body}\n    Ok(())\n}}\n"
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
