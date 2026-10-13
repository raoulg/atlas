//! TypeScript and TSX read as the same algebra as Python and Rust: modules, types, calls,
//! routes and the places a body touches.

use std::collections::HashSet;
use std::path::PathBuf;

use atlas_engine::principles::check;
use atlas_engine::{analyze, EdgeKind, EntryKind, Graph, NodeKind, Options};

fn fixture() -> Graph {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixture-ts");
    analyze(&repo, &Options::default()).expect("the TypeScript fixture parses")
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
    let ctor = g.node("fixture-ts.lib.model.Laag.constructor").unwrap();
    assert!(ctor.ctor && !ctor.on_class, "the constructor initializes what `new Laag()` makes");
    let laag = g.node("fixture-ts.lib.model.Laag").unwrap();
    assert_eq!(laag.initializer(|id| g.node(id)).as_deref(), Some("fixture-ts.lib.model.Laag.constructor"));
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
    let touched: Vec<(&str, &str)> = laad.touches.iter().map(|t| (t.kind_name(), t.artifact.as_str())).collect();
    assert!(touched.contains(&("net", "api.example.org")), "{touched:?}");
    assert!(touched.contains(&("screen", "")));
    let kaart = g.node("fixture-ts.components.Kaart.Kaart").unwrap();
    assert!(kaart.touches.iter().any(|t| t.kind_name() == "screen" && t.op == "render"), "a component renders to the screen");
}

/// The principles read TypeScript as they read Python: `kies` tests `modus` twice.
#[test]
fn principles_read_typescript() {
    let fs = check(&fixture());
    assert!(fs.iter().any(|f| f.principle == "choices" && f.node == "fixture-ts.lib.api.kies"), "{fs:#?}");
}

/// A server's routes are its registration calls, at the full path of the routers they are
/// mounted on; a script that runs a file of the package is a command; what the package's
/// entry exports is its library.
#[test]
fn a_server_registers_its_routes() {
    let g = fixture();
    let entry = |id: &str| g.node(&format!("fixture-ts.{id}")).and_then(|n| n.entry.clone()).map(|e| (e.kind, e.label));
    assert_eq!(entry("server.app.GET /api/health"), Some((EntryKind::Route, "GET /api/health".into())));
    assert_eq!(entry("server.regels.regels.GET /:id"), Some((EntryKind::Route, "GET /api/regels/{id}".into())), "`app.route(\"/api/regels\", regels)`");
    assert_eq!(entry("server.regels.regels.POST /"), Some((EntryKind::Route, "POST /api/regels".into())));
    assert!(g.node("fixture-ts.server.regels.regels").is_none(), "a router is no function");
    assert_eq!(entry("server"), Some((EntryKind::Cli, "dev · serve".into())), "the scripts that run server/index.ts");
    assert_eq!(entry("lib.api.laad"), Some((EntryKind::Export, "laad".into())), "`exports` → lib/index.ts → `export *`");
    assert_eq!(entry("lib.model.Laag"), Some((EntryKind::Export, "Kaartlaag".into())), "`export {{ Laag as Kaartlaag }}`");
    assert_eq!(entry("lib.thema.thema"), None, "not exported by the entry");
}

/// A client a package makes moves data where it is used: a pool's query reaches the database
/// by its statement, an SDK's client asks the model (its helpers do not), a promisified
/// `execFile` runs a program, the browser's storage keeps a key, a socket connects.
#[test]
fn a_client_moves_data_where_it_is_used() {
    let g = fixture();
    let t = |id: &str| g.node(&format!("fixture-ts.{id}")).unwrap().touches.iter().map(|t| (t.kind_name().to_string(), t.op.clone(), t.artifact.clone(), t.dir_name().to_string())).collect::<Vec<_>>();
    let post = t("server.regels.regels.POST /");
    assert!(post.contains(&("db_sql".into(), "query".into(), "regels".into(), "write".into())), "{post:?}");
    // a statement is read where it is whole: from a constant, and from what a caller hands `tel`
    assert!(t("server.regels.regels.GET /:id").contains(&("db_sql".into(), "query".into(), "bezoek".into(), "write".into())), "`pool.query(BEZOEK)`");
    assert_eq!(t("server.regels.tel"), [("db_sql".into(), "query".into(), "bezoek".into(), "read".into())], "`tel(\"SELECT count(*) FROM bezoek\")`");
    assert!(post.iter().any(|x| x.0 == "process" && x.1 == "run"), "`promisify(execFile)`: {post:?}");
    assert_eq!(t("server.regels.vat"), [("llm".into(), "create".into(), String::new(), String::new())], "the client, not `zodOutputFormat`");
    assert_eq!(t("lib.thema.thema"), [("db_kv".into(), "getItem".into(), "thema".into(), "read".into())]);
    assert!(t("lib.thema.bewaar").contains(&("screen".into(), "write".into(), String::new(), String::new())), "process.stdout");
    assert_eq!(t("lib.thema.luister"), [("net".into(), "WebSocket".into(), "live.example.org".into(), String::new())]);
}

/// A file's place is its path evaluated: from the module's own file (`import.meta.url`),
/// through `path.join`, `path.resolve` and `new URL(..)`, across the constants it names, with a
/// function's parameters bound to what its callers hand it. A request is its method and URL
/// built the same way, and a URL the package's own routes serve is a request to the handler
/// from the function that names it.
#[test]
fn paths_and_requests_are_evaluated() {
    let g = fixture();
    let t = |id: &str| g.node(&format!("fixture-ts.{id}")).unwrap().touches.iter().map(|t| (t.kind_name().to_string(), t.artifact.clone())).collect::<Vec<_>>();
    // the file `klanten` hands `lees`, in the routes that reach `klanten`: under `DATA`'s default,
    // or wherever `DATA_DIR` says, a place the code does not name
    let lees: Vec<(&str, &[String])> = g.node("fixture-ts.server.opslag.lees").unwrap().touches.iter().map(|t| (t.artifact.as_str(), t.by.as_slice())).collect();
    let klanten = &["fixture-ts.server.opslag.klanten".to_string()][..];
    assert_eq!(lees, [("data/klanten.csv", klanten), ("", klanten)], "`lees(\"klanten.csv\")` under `DATA`");
    // an object holds what its constructor was handed: a parameter property, and `this.map = map`
    assert_eq!(t("server.opslag.Archief.bewaar"), [("disk_write".into(), "archief/".into())], "`constructor(private map: string)`");
    assert_eq!(t("server.opslag.Logboek.noteer"), [("disk_write".into(), "logs/log.txt".into())], "`this.map = map`");
    assert_eq!(t("server.opslag.bewaar"), [("disk_write".into(), "uit/regels.pdf".into())], "`new URL(\"../../uit/regels.pdf\", import.meta.url)`");
    let requests = edges(&g, EdgeKind::Requests);
    assert!(has(&requests, "lib.klant.regel", "server.regels.regels.GET /:id"), "{requests:?}");
    assert!(has(&requests, "lib.klant.nieuw", "server.regels.regels.POST /"), "{requests:?}");
    assert!(!has(&requests, "lib.klant.nieuw", "server.regels.regels.GET /:id"), "by its method");
    assert!(t("lib.klant.vraag").is_empty(), "a request to the package's own route is no place on the web");
}

/// A body that changes what it is handed (a member assigned, an item pushed) changes its input:
/// `T -> T`, and `changes` asks about it; a constructor builds its object.
#[test]
fn a_change_to_an_input_is_a_change() {
    let g = fixture();
    let m = edges(&g, EdgeKind::Mutates);
    assert!(has(&m, "lib.telling.verzamel", "lib.model.Regel"), "`uit.push(r)`: {m:?}");
    assert!(has(&m, "lib.telling.hernoem", "lib.model.Regel"), "`regel.id = id`");
    assert!(has(&m, "lib.telling.Teller.tel", "lib.telling.Teller"), "`this.aantal++`");
    assert!(!m.iter().any(|(s, _)| s == "lib.telling.telAlles" || s == "lib.telling.Teller.__init__"), "{m:?}");
    let asked: Vec<String> = check(&g).into_iter().filter(|f| f.principle == "changes").map(|f| f.what).collect();
    assert!(asked.iter().any(|w| w.contains("verzamel")) && !asked.iter().any(|w| w.contains("Teller.tel")), "{asked:?}");
}

/// A TypeScript body lists its calls, each with what it is handed and the name it is kept by:
/// the `Laag` that `new Laag(..)` gives is the `laag` the next call is made on.
#[test]
fn a_typescript_body_lists_its_calls() {
    let g = fixture();
    let scherm = g.nodes.iter().find(|n| n.id.ends_with("routes.kaart.KaartScherm")).expect("KaartScherm");
    let laag = scherm.sites.iter().find(|s| s.to.ends_with("model.Laag")).expect("new Laag");
    assert!(laag.gives.iter().any(|h| h.name == "laag" && h.ty.ends_with("model.Laag")), "{:#?}", scherm.sites);
    let toon = scherm.sites.iter().find(|s| s.to.ends_with("Laag.toon")).expect("laag.toon()");
    assert!(toon.hands.iter().any(|h| h.name == "laag"), "{:#?}", toon);
    assert!(scherm.sites.iter().position(|s| s.to.ends_with("model.Laag")) < scherm.sites.iter().position(|s| s.to.ends_with("Laag.toon")));
}
