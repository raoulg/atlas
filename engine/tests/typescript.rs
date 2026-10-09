//! TypeScript and TSX read as the same algebra as Python and Rust: modules, types, calls,
//! routes and the places a body touches.

use std::collections::HashSet;
use std::path::PathBuf;

use atlas_engine::principles::check;
use atlas_engine::{analyze, EdgeKind, EntryKind, Graph, NodeKind};

fn fixture() -> Graph {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixture-ts");
    analyze(&repo, None, None).expect("the TypeScript fixture parses")
}

fn edges(g: &Graph, kind: EdgeKind) -> HashSet<(String, String)> {
    g.edges.iter().filter(|e| e.kind == kind).map(|e| (e.src.replace("fixture-ts.", ""), e.dst.replace("fixture-ts.", ""))).collect()
}

fn has(set: &HashSet<(String, String)>, src: &str, dst: &str) -> bool {
    set.contains(&(src.to_string(), dst.to_string()))
}

/// A file is a module under the package (`src/` is a layout folder), a directory a package
/// module, an `index` file its directory; interfaces, classes and named shapes are types.
#[test]
fn files_are_modules_and_types_are_types() {
    let g = fixture();
    assert_eq!((g.language.as_str(), g.root.as_str()), ("typescript", "fixture-ts"));
    let kind = |id: &str| g.node(&format!("fixture-ts.{id}")).map(|n| n.kind);
    assert_eq!(kind("lib.model"), Some(NodeKind::Module));
    assert_eq!(kind("lib"), Some(NodeKind::Module), "lib/index.ts stands for lib");
    assert!(g.node("fixture-ts.lib").unwrap().is_pkg);
    assert_eq!(kind("lib.model.Regel"), Some(NodeKind::Class), "an interface is a type");
    let ctor = g.node("fixture-ts.lib.model.Laag.__init__").expect("a constructor is the class's __init__");
    assert_eq!(ctor.name, "constructor");
    let fields = edges(&g, EdgeKind::Has);
    assert!(has(&fields, "lib.model.Regel", "lib.model.Laag"), "an interface's typed property");
    assert!(has(&fields, "lib.model.Laag", "lib.model.Bron"), "`constructor(private bron: Bron)` declares a field");
    let variants = edges(&g, EdgeKind::Variant);
    assert!(has(&variants, "lib.model.Uitkomst", "lib.model.Treffer") && has(&variants, "lib.model.Uitkomst", "lib.model.Mis"), "a union alias is a sum");
}

/// Types in signatures are takes and returns; `Promise<Regel[]>` hands on Regels.
#[test]
fn signatures_name_their_types() {
    let g = fixture();
    let returns = edges(&g, EdgeKind::Returns);
    assert!(has(&returns, "lib.api.laad", "lib.model.Regel"), "Promise and arrays are containers");
    assert!(has(&returns, "lib.api.zoek", "lib.model.Uitkomst"), "a server function's handler is its body");
    assert!(has(&edges(&g, EdgeKind::Takes), "lib.api.kies", "lib.model.Uitkomst"));
}

/// A route starts at its file route; JSX renders a component; a function handed on is called
/// by whoever receives it; imports resolve through tsconfig paths, `index` files and `export *`.
#[test]
fn routes_render_components_and_hand_on_functions() {
    let g = fixture();
    let route = g.node("fixture-ts.routes.kaart").unwrap().entry.clone().unwrap();
    assert_eq!((route.kind, route.label.as_str()), (EntryKind::Route, "/kaart"));
    let calls = edges(&g, EdgeKind::Calls);
    assert!(has(&calls, "routes.kaart", "routes.kaart.KaartScherm"), "`component: KaartScherm`");
    assert!(has(&calls, "routes.kaart.KaartScherm", "components.Kaart.Kaart"), "`<Kaart/>` through `@/components/Kaart`");
    assert!(has(&calls, "routes.kaart.KaartScherm", "lib.api.zoek"), "`useServerFn(zoek)` hands the server function on");
    assert!(has(&calls, "routes.kaart.KaartScherm", "lib.model.Laag.toon"), "a method on a local built with `new`");
    assert!(has(&edges(&g, EdgeKind::Instantiates), "routes.kaart.KaartScherm", "lib.model.Laag"));
    assert!(has(&calls, "lib.model.Laag.toon", "lib.model.Bron.lees"), "`this.bron.lees()` through the field's type");
    assert!(has(&calls, "components.Kaart.Kaart", "components.Kaart.Legenda"));
    assert!(has(&calls, "cli", "lib.api.laad"), "through lib/index.ts and its `export *`");
    let cli = g.node("fixture-ts.cli").unwrap().entry.clone().unwrap();
    assert_eq!((cli.kind, cli.label.as_str()), (EntryKind::Cli, "fixture"), "package.json bin");
    assert_eq!(g.node("fixture-ts.lib.api_test").unwrap().entry.as_ref().map(|e| e.kind), Some(EntryKind::Test));
}

/// A type with method signatures promises what its implementations do; a value of that type
/// calls the promise; a class that says `implements` keeps it; a body that imports when it
/// runs (`await import(..)`) and picks one is a choice.
#[test]
fn a_promise_is_kept_by_who_implements_it() {
    let g = fixture();
    assert_eq!(g.node("fixture-ts.lib.bron.BronLike.lees").map(|n| n.kind), Some(NodeKind::Method), "a method signature of a named object type");
    let calls = edges(&g, EdgeKind::Calls);
    assert!(has(&calls, "lib.bron.haal", "lib.bron.BronLike.lees"), "`bron.lees()` through the value's declared type");
    let inherits = edges(&g, EdgeKind::Inherits);
    assert!(has(&inherits, "lib.echte-bron.EchteBron", "lib.bron.BronLike"), "`implements BronLike`");
    let arms = |dst: &str| g.edges.iter().find(|e| e.src == "fixture-ts.lib.bron.kiesBron" && e.dst == dst).map(|e| e.arms.clone()).unwrap_or_default();
    let (nep, echt) = (arms("fixture-ts.lib.nep-bron.NepBron"), arms("fixture-ts.lib.echte-bron.EchteBron"));
    assert!(!nep.is_empty() && !echt.is_empty() && nep != echt, "two branches of one choice: {nep:?} | {echt:?}");
}

/// `fetch` reaches the web, named by its host; `console` and a rendered component the screen.
#[test]
fn fetch_is_the_web_and_console_the_screen() {
    let g = fixture();
    let laad = g.node("fixture-ts.lib.api.laad").unwrap();
    let touched: Vec<(&str, &str)> = laad.touches.iter().map(|t| (t.kind.as_str(), t.artifact.as_str())).collect();
    assert!(touched.contains(&("net", "api.example.org")), "{touched:?}");
    assert!(touched.contains(&("screen", "")));
    let kaart = g.node("fixture-ts.components.Kaart.Kaart").unwrap();
    assert!(kaart.touches.iter().any(|t| t.kind == "screen" && t.op == "render"), "a component renders to the screen");
}

/// The principles read TypeScript as they read Python: `kies` tests `modus` twice.
#[test]
fn principles_read_typescript() {
    let fs = check(&fixture());
    assert!(fs.iter().any(|f| f.principle == "choices" && f.node == "fixture-ts.lib.api.kies"), "{fs:#?}");
}
