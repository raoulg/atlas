//! TypeScript and TSX read as the same algebra as Python and Rust: modules, types, calls,
//! routes and the places a body touches.

use std::collections::HashSet;
use std::path::PathBuf;

use atlas_engine::principles::check;
use atlas_engine::{analyze, EdgeKind, EntryKind, Graph, Lang, NodeKind, Options};

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
    assert_eq!((g.language.one(), g.root.as_str()), (Some(Lang::TypeScript), "fixture-ts"));
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
    assert!(
        has(&variants, "lib.model.Uitkomst", "lib.model.Treffer") && has(&variants, "lib.model.Uitkomst", "lib.model.Mis"),
        "a union alias is a sum"
    );
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
    assert_eq!(
        g.node("fixture-ts.lib.bron.BronLike.lees").map(|n| n.kind),
        Some(NodeKind::Method),
        "a method signature of a named object type"
    );
    let calls = edges(&g, EdgeKind::Calls);
    assert!(has(&calls, "lib.bron.haal", "lib.bron.BronLike.lees"), "`bron.lees()` through the value's declared type");
    let inherits = edges(&g, EdgeKind::Inherits);
    assert!(has(&inherits, "lib.echte-bron.EchteBron", "lib.bron.BronLike"), "`implements BronLike`");
    let arms = |dst: &str| {
        g.edges.iter().find(|e| e.src == "fixture-ts.lib.bron.kiesBron" && e.dst == dst).map(|e| e.arms.clone()).unwrap_or_default()
    };
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
    assert_eq!(
        entry("server.regels.regels.GET /:id"),
        Some((EntryKind::Route, "GET /api/regels/{id}".into())),
        "`app.route(\"/api/regels\", regels)`"
    );
    assert_eq!(entry("server.regels.regels.POST /"), Some((EntryKind::Route, "POST /api/regels".into())));
    assert!(g.node("fixture-ts.server.regels.regels").is_none(), "a router is no function");
    let (kind, scripts) = entry("server").expect("the scripts that run server/index.ts make it a command");
    assert_eq!(kind, EntryKind::Cli);
    assert!(
        scripts.contains("dev") && scripts.contains("serve") && !scripts.contains("build"),
        "the scripts that run server/index.ts, and not `build`: {scripts}"
    );
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
    let t = |id: &str| {
        g.node(&format!("fixture-ts.{id}"))
            .unwrap()
            .touches
            .iter()
            .map(|t| (t.kind_name().to_string(), t.op.clone(), t.artifact.clone(), t.dir_name().to_string()))
            .collect::<Vec<_>>()
    };
    let post = t("server.regels.regels.POST /");
    assert!(post.contains(&("db_sql".into(), "query".into(), "regels".into(), "write".into())), "{post:?}");
    // a statement is read where it is whole: from a constant, and from what a caller hands `tel`
    assert!(
        t("server.regels.regels.GET /:id").contains(&("db_sql".into(), "schema".into(), "bezoek".into(), "write".into())),
        "`pool.query(BEZOEK)` gives `bezoek` its shape"
    );
    assert_eq!(
        t("server.regels.tel"),
        [("db_sql".into(), "query".into(), "bezoek".into(), "read".into())],
        "`tel(\"SELECT count(*) FROM bezoek\")`"
    );
    assert!(post.iter().any(|x| x.0 == "process" && x.1 == "run"), "`promisify(execFile)`: {post:?}");
    assert_eq!(
        t("server.regels.vat"),
        [("llm".into(), "create".into(), String::new(), String::new())],
        "the client, not `zodOutputFormat`"
    );
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
    let t = |id: &str| {
        g.node(&format!("fixture-ts.{id}"))
            .unwrap()
            .touches
            .iter()
            .map(|t| (t.kind_name().to_string(), t.artifact.clone()))
            .collect::<Vec<_>>()
    };
    // the file `klanten` hands `lees`, in the routes that reach `klanten`: under `DATA`, what
    // `DATA_DIR` says or its default, one input drawn as its default
    let lees: Vec<(&str, &[String])> =
        g.node("fixture-ts.server.opslag.lees").unwrap().touches.iter().map(|t| (t.artifact.as_str(), t.by.as_slice())).collect();
    let klanten = &["fixture-ts.server.opslag.klanten".to_string()][..];
    assert_eq!(lees, [("data/klanten.csv", klanten)], "`lees(\"klanten.csv\")` under `DATA`");
    // an object holds what its constructor was handed: a parameter property, and `this.map = map`;
    // `new Archief("archief").bewaar("a.txt", ..)` hands the method the rest of the path
    assert_eq!(t("server.opslag.Archief.bewaar"), [("disk_write".into(), "archief/a.txt".into())], "`constructor(private map: string)`");
    assert_eq!(t("server.opslag.Logboek.noteer"), [("disk_write".into(), "logs/log.txt".into())], "`this.map = map`");
    assert_eq!(
        t("server.opslag.bewaar"),
        [("disk_write".into(), "uit/regels.pdf".into())],
        "`new URL(\"../../uit/regels.pdf\", import.meta.url)`"
    );
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
    for builds in ["lib.telling.telAlles", "lib.telling.Teller.constructor"] {
        assert!(g.node(&format!("fixture-ts.{builds}")).is_some(), "{builds} is in the graph");
        assert!(!m.iter().any(|(s, _)| s == builds), "{builds}: {m:?}");
    }
    let asked: Vec<_> = check(&g).into_iter().filter(|f| f.principle == "changes").collect();
    let verzamel = asked.iter().find(|f| f.node == "fixture-ts.lib.telling.verzamel").unwrap_or_else(|| panic!("{asked:#?}"));
    assert_eq!((verzamel.about("changes"), verzamel.about("type")), (vec!["uit"], vec!["fixture-ts.lib.model.Regel"]));
    assert!(asked.iter().all(|f| f.node != "fixture-ts.lib.telling.Teller.tel"), "{asked:#?}");
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

/// A small package written to a directory of its own, read, and removed.
fn written(name: &str, files: &[(&str, &str)]) -> Graph {
    read_with(name, files, &[Options::default()]).remove(0)
}

/// A small package written to a directory of its own, read once with each of `options`, and
/// removed.
fn read_with(name: &str, files: &[(&str, &str)], options: &[Options]) -> Vec<Graph> {
    let dir = std::env::temp_dir().join(format!("atlas-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (path, text) in files {
        let file = dir.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }
    let gs: Vec<_> = options.iter().map(|o| analyze(&dir, o)).collect();
    let _ = std::fs::remove_dir_all(&dir);
    gs.into_iter().map(|g| g.expect("the package parses")).collect()
}

/// A package the program calls is read as code as far as the program's calls go into it: the file
/// in `node_modules` that defines what the program calls, found from what `package.json` exports
/// as code (never its `.d.ts`), through the package's re-exports and `export *`. Its function is a
/// step on the program's route, and what it reaches of the world is what the program reaches
/// through it: no table names the package, yet the call reads the file the program hands it. Each
/// round of `--libraries` reaches one call further into the same package; `--libraries 0` reads
/// none. Not read: a package the tables describe, one with compiled code, and Node's own modules.
/// What the program does not reach of a package's file is no part of the picture, and `atlas
/// check` does not judge a library's code.
#[test]
fn a_package_is_read_as_far_as_the_program_calls_into_it() {
    let files: &[(&str, &str)] = &[
        ("package.json", r#"{ "name": "reads" }"#),
        ("tsconfig.json", "{}"),
        (
            "src/main.ts",
            r#"import { readConfig } from "filekit";
import * as fk from "filekit";
import { sum } from "fastsum";
import axios from "axios";

export function main(): string {
  const text = readConfig("settings.json");
  fk.listed("data");
  sum([1, 2]);
  axios.get("https://example.org/rows");
  return text;
}
"#,
        ),
        (
            "node_modules/filekit/package.json",
            r#"{ "name": "filekit", "exports": { ".": { "types": "./index.d.ts", "import": "./index.js" } } }"#,
        ),
        ("node_modules/filekit/index.d.ts", "export declare function readConfig(path: string): string;\n"),
        ("node_modules/filekit/index.js", "export { readConfig } from \"./lib/read.js\";\nexport * from \"./lib/list.js\";\n"),
        (
            "node_modules/filekit/lib/read.js",
            r#"import { readFileSync } from "node:fs";
import { clean } from "./clean.js";

export function readConfig(path) {
  return clean(readFileSync(path, "utf8"));
}

export function unused() {}
"#,
        ),
        ("node_modules/filekit/lib/clean.js", "export function clean(text) {\n  return text.trim();\n}\n"),
        (
            "node_modules/filekit/lib/list.js",
            "import { readdirSync } from \"node:fs\";\n\nexport function listed(dir) {\n  return readdirSync(dir);\n}\n",
        ),
        ("node_modules/fastsum/package.json", r#"{ "name": "fastsum", "main": "index.js" }"#),
        ("node_modules/fastsum/index.js", "export function sum(xs) {\n  return xs.reduce((a, b) => a + b, 0);\n}\n"),
        ("node_modules/fastsum/build/sum.node", ""),
        ("node_modules/axios/package.json", r#"{ "name": "axios", "main": "index.js" }"#),
        ("node_modules/axios/index.js", "export default { get(url) {} };\n"),
    ];
    let rounds = |n: usize| Options { libraries: n, ..Options::default() };
    let [none, one, two]: [Graph; 3] = read_with("libraries", files, &[rounds(0), rounds(1), rounds(2)]).try_into().ok().unwrap();
    let main = |g: &Graph| g.node(&format!("{}.main.main", g.root)).unwrap().clone();
    let read = one.node("filekit.lib.read.readConfig").expect("the function the program calls is read");
    assert!(read.library, "a library's function is marked as one");
    let file = read.touches.iter().find(|t| t.kind_name() == "disk_read").expect("the package's own code reads the disk");
    assert_eq!((file.op.as_str(), file.artifact.as_str()), ("readFileSync", "settings.json"), "at the path the program hands it: {file:?}");
    let to: Vec<String> = main(&one).sites.iter().map(|s| if s.to.is_empty() { s.op.clone() } else { s.to.clone() }).collect();
    assert_eq!(
        to,
        ["filekit.lib.read.readConfig", "filekit.lib.list.listed"],
        "the package's functions are steps, through `export *` and a namespace"
    );
    assert!(one.edges.iter().any(|e| e.src == main(&one).id && e.dst == "filekit.lib.read.readConfig" && e.kind == EdgeKind::Calls));
    assert!(one.node("filekit.lib.read.unused").is_none(), "what the program does not reach is no part of the picture");
    assert!(
        one.node("filekit.lib.clean.clean").is_none() && two.node("filekit.lib.clean.clean").is_some(),
        "each round reaches one call further"
    );
    assert!(main(&one).touches.iter().any(|t| t.kind_name() == "net"), "a package the tables describe stays a table's call");
    for g in [&one, &two] {
        assert!(
            g.nodes.iter().all(|n| !n.id.starts_with("fastsum") && !n.id.starts_with("axios")),
            "neither compiled code nor a package the tables describe is read"
        );
        assert!(check(g).iter().all(|f| !f.node.starts_with("filekit")), "a library's code is not judged");
    }
    assert!(none.nodes.iter().all(|n| !n.library), "no round reads no package");
    assert!(main(&none).sites.iter().all(|s| s.to.is_empty()), "a package not read is a call Atlas does not follow");
}

/// What lies beyond the rounds is glued into the package's function at the border: the program
/// calls `save`, whose file is written two calls deeper, in functions the graph does not hold.
/// `save` is one arrow that writes the file the program hands it, drawn at its own call that leads
/// there, so the program's route reaches the place. Where the rounds reach the function that
/// writes, the touch is that function's own, not `save`'s.
#[test]
fn what_lies_beyond_the_rounds_is_the_border_functions() {
    let files: &[(&str, &str)] = &[
        ("package.json", r#"{ "name": "deep" }"#),
        ("tsconfig.json", "{}"),
        ("src/main.ts", "import { save } from \"deepkit\";\n\nexport function main(): void {\n  save(\"out.txt\", \"rows\");\n}\n"),
        ("node_modules/deepkit/package.json", r#"{ "name": "deepkit", "main": "index.js" }"#),
        (
            "node_modules/deepkit/index.js",
            r#"import { writeFileSync } from "node:fs";

export function save(path, text) {
  prepare(path, text);
}

function prepare(path, text) {
  write(path, text.trim());
}

function write(path, text) {
  writeFileSync(path, text);
}
"#,
        ),
    ];
    let rounds = |n: usize| Options { libraries: n, ..Options::default() };
    let [one, three]: [Graph; 2] = read_with("beyond", files, &[rounds(1), rounds(3)]).try_into().ok().unwrap();
    assert!(one.node("deepkit.prepare").is_none() && one.node("deepkit.write").is_none(), "what lies beyond the rounds is not drawn");
    let save = one.node("deepkit.save").expect("the function the program calls");
    let wrote = save
        .touches
        .iter()
        .find(|t| t.kind_name() == "disk_write")
        .unwrap_or_else(|| panic!("the border writes what its code beyond writes: {:?}", save.touches));
    assert_eq!(wrote.artifact, "out.txt", "at the path the program hands it: {wrote:?}");
    let call = save.sites.iter().find(|s| s.to == "deepkit.prepare").expect("save's call into what lies beyond");
    assert_eq!(wrote.line, call.line, "drawn at the border's call that leads there");
    let route = atlas_engine::story::route(&one, &format!("{}.main.main", one.root), 2).unwrap();
    assert!(route.contains("out.txt"), "the program's route reaches the place:\n{route}");
    assert!(three.node("deepkit.write").unwrap().touches.iter().any(|t| t.kind_name() == "disk_write"));
    assert!(
        three.node("deepkit.save").unwrap().touches.iter().all(|t| t.kind_name() != "disk_write"),
        "a function's own arrow is not what it reaches"
    );
}

/// A value a module keeps between calls, which its functions change, is a place:
/// `Global(module.name)`, written by each function that changes it and read by each that uses
/// it, in its own module or where another imports it. A function changes it when it assigns the
/// name anew (`current = name`, `count++`) or changes its value in place: an item or a member
/// assigned or deleted, or a method of the language's own that changes its receiver
/// (`LATEST.set`, `QUEUE.push`). A name no function changes is a constant, a parameter of the same
/// name is the function's own, and a value a package made changes through the package's methods,
/// not the language's.
#[test]
fn a_value_a_module_keeps_is_a_global_place() {
    let g = written(
        "kept",
        &[
            ("package.json", r#"{ "name": "kept" }"#),
            ("tsconfig.json", "{}"),
            (
                "src/state.ts",
                r#"import { LRUCache } from "lru-cache";

export let current = "";
export const LATEST = new Map<string, number>();
export const SEEN: Record<string, boolean> = {};
export const QUEUE: string[] = [];
export const LIMITS = { max: 10 };
export const NAMES = ["a", "b"];
let count = 0;
const cache = new LRUCache<string, string>({ max: 10 });

export function choose(name: string): void {
  current = name;
}

export function remember(key: string, n: number): void {
  LATEST.set(key, n);
  count++;
}

export function mark(key: string): void {
  SEEN[key] = true;
}

export function forget(key: string): void {
  delete SEEN[key];
}

export function enqueue(item: string): void {
  QUEUE.push(item);
}

export function raise(max: number): void {
  LIMITS.max = max;
}

export function keep(key: string, value: string): void {
  cache.set(key, value);
}

export function echo(current: string): string {
  return current;
}

export function first(): string {
  return NAMES[0] ?? "";
}
"#,
            ),
            (
                "src/view.ts",
                r#"import { LATEST, choose, current } from "./state.js";

export function show(key: string): string {
  return `${current}: ${LATEST.get(key)}`;
}

export function pick(name: string): void {
  choose(name);
}
"#,
            ),
        ],
    );
    let root = format!("{}.", g.root);
    let globals = |f: &str| -> Vec<(String, String)> {
        let n = g.node(&format!("{root}{f}")).unwrap_or_else(|| panic!("{f}"));
        let mut out: Vec<(String, String)> = n
            .touches
            .iter()
            .filter(|t| t.kind_name() == "global")
            .map(|t| (t.artifact.trim_start_matches(&root).to_string(), t.dir_name().to_string()))
            .collect();
        out.sort();
        out
    };
    let place = |uses: &[(&str, &str)]| -> Vec<(String, String)> { uses.iter().map(|(k, d)| (k.to_string(), d.to_string())).collect() };
    assert_eq!(globals("state.choose"), place(&[("state.current", "write")]), "assigned anew, and not read");
    assert_eq!(
        globals("state.remember"),
        place(&[("state.LATEST", "read"), ("state.LATEST", "write"), ("state.count", "read"), ("state.count", "write")])
    );
    for (f, kept) in
        [("state.mark", "state.SEEN"), ("state.forget", "state.SEEN"), ("state.enqueue", "state.QUEUE"), ("state.raise", "state.LIMITS")]
    {
        assert_eq!(globals(f), place(&[(kept, "read"), (kept, "write")]), "{f}");
    }
    assert_eq!(
        globals("view.show"),
        place(&[("state.LATEST", "read"), ("state.current", "read")]),
        "read where another module imports them"
    );
    assert!(globals("state.keep").is_empty(), "a package's value changes through the package's methods");
    assert!(globals("state.echo").is_empty(), "a parameter is the function's own");
    assert!(globals("state.first").is_empty(), "a name no function changes is a constant");
    assert!(check(&g).iter().all(|f| f.principle != "nothing"), "a function that changes a global leaves it behind");
}

/// An option of the command line is the entry's input, as a parameter of a command is: what
/// `parseArgs` gives for it holds what the program runs with when the command line leaves the
/// option out, its `default`, else no value. `values.input ?? IN` is `IN`; an option with a
/// default is that default, from an options object written where it is handed or held by a
/// constant, a place the code does not alone decide; one with neither is a file the code does not
/// name. An object literal declared as a
/// type of the program's makes a value of it: a step to the type.
#[test]
fn an_option_holds_its_default_and_a_typed_literal_is_made() {
    let g = written(
        "options",
        &[
            ("package.json", r#"{ "name": "options", "bin": { "options": "./src/cli.ts" } }"#),
            ("tsconfig.json", "{}"),
            (
                "src/cli.ts",
                r#"import { readFile, writeFile } from "node:fs/promises";
import * as util from "node:util";
import { parseArgs } from "node:util";

const IN = "data/in.txt";
const OUT = "data/out.txt";
const OPTIONS = { log: { type: "string", default: "logs/run.log" } } as const;

interface Settings {
  size: number;
}

async function main(argv: string[]): Promise<void> {
  const { values } = parseArgs({
    args: argv,
    options: { input: { type: "string" }, output: { type: "string", default: OUT }, extra: { type: "string" } },
  });
  const settings: Settings = { size: 3 };
  const counts: Record<string, number> = { a: 1 };
  const text = await readFile(values.input ?? IN, "utf8");
  await writeFile(values.output, text.repeat(settings.size + (counts.a ?? 0)));
  await writeFile(values.extra, text);
  const parsed = util.parseArgs({ args: argv, options: OPTIONS, strict: false });
  await writeFile(parsed.values.log, text);
}

await main(process.argv.slice(2));
"#,
            ),
        ],
    );
    let main = g.node(&format!("{}.cli.main", g.root)).expect("main");
    let places = |kind: &str| -> Vec<&str> {
        let mut out: Vec<&str> = main.touches.iter().filter(|t| t.kind_name() == kind).map(|t| t.artifact.as_str()).collect();
        out.sort();
        out
    };
    assert_eq!(places("disk_read"), ["data/in.txt"], "`values.input ?? IN`");
    assert_eq!(places("disk_write"), ["", "data/out.txt", "logs/run.log"], "`default: OUT`, a constant's `default`, and `--extra`");
    // an option is the entry's input, as a parameter of a command is: the code does not alone
    // decide the place its default names
    let defaults: Vec<&atlas_engine::Touch> =
        main.touches.iter().filter(|t| matches!(t.artifact.as_str(), "data/out.txt" | "logs/run.log")).collect();
    assert!(defaults.len() == 2 && defaults.iter().all(|t| !t.fixed && t.by.is_empty()), "{defaults:#?}");
    let settings = format!("{}.cli.Settings", g.root);
    let made: Vec<(&str, Vec<(&str, &str)>)> = main
        .sites
        .iter()
        .filter(|s| !s.to.is_empty())
        .map(|s| (s.to.as_str(), s.gives.iter().map(|h| (h.name.as_str(), h.ty.as_str())).collect()))
        .collect();
    assert_eq!(
        made,
        [(settings.as_str(), vec![("settings", settings.as_str())])],
        "a literal of a type the program declares, and no other"
    );
    assert!(
        g.edges.iter().any(|e| e.kind == EdgeKind::Instantiates && e.src == main.id && e.dst == settings),
        "`const settings: Settings = {{..}}`"
    );
}

/// An entry's domain is what the human hands it on the command line: the command `bin` names takes
/// the options of the parses of the command line its route runs, in the order it runs them, each a
/// key of the options parseArgs is handed, written where it is handed or held by a constant
/// (`out` is `--out`, `"dry-run"` is `--dry-run`). The parse is a step that reads the command line,
/// whatever the body keeps of it: the names a pattern takes out (`{ values, positionals }`) or the
/// whole result, handed on to the steps that use them.
#[test]
fn parse_args_options_are_the_entrys_domain() {
    let g = written(
        "usage",
        &[
            ("package.json", r#"{ "name": "usage", "bin": { "usage": "./src/cli.ts" } }"#),
            ("tsconfig.json", "{}"),
            ("src/run.ts", "export function run(out: string, rest: string[]): void {}\n"),
            (
                "src/cli.ts",
                r#"import * as util from "node:util";
import { parseArgs } from "node:util";

import { run } from "./run.js";

const OPTIONS = { "dry-run": { type: "boolean" }, level: { type: "string", short: "l" } } as const;

function main(argv: string[]): void {
  const { values, positionals } = parseArgs({ args: argv, options: { out: { type: "string", short: "o" }, verbose: { type: "boolean" } }, allowPositionals: true });
  run(values.out ?? "out", positionals);
}

function extra(): void {
  const parsed = util.parseArgs({ options: OPTIONS });
  run(parsed.values.level ?? "info", []);
}

main(process.argv.slice(2));
extra();
"#,
            ),
        ],
    );
    let cli = g.node(&format!("{}.cli", g.root)).unwrap();
    assert_eq!(
        cli.entry.as_ref().map(|e| (e.kind, e.options.clone())),
        Some((EntryKind::Cli, ["--out", "--verbose", "--dry-run", "--level"].map(String::from).to_vec()))
    );
    let parse = |f: &str| {
        g.node(&format!("{}.cli.{f}", g.root))
            .unwrap()
            .sites
            .iter()
            .find(|s| s.command_line.is_some())
            .unwrap_or_else(|| panic!("{f} parses its command line"))
            .clone()
    };
    let names = |hs: &[atlas_engine::Held]| hs.iter().map(|h| h.name.clone()).collect::<Vec<_>>();
    let main = parse("main");
    assert_eq!(
        (main.op.as_str(), main.command_line.clone().unwrap(), names(&main.gives)),
        ("parseArgs", vec!["--out".to_string(), "--verbose".to_string()], vec!["values".to_string(), "positionals".to_string()])
    );
    let running = g.node(&format!("{}.cli.main", g.root)).unwrap().sites.iter().find(|s| s.to.ends_with(".run.run")).unwrap().clone();
    assert_eq!(names(&running.hands), ["values", "positionals"]);
    let extra = parse("extra");
    assert_eq!(
        (extra.command_line.unwrap(), names(&extra.gives)),
        (vec!["--dry-run".to_string(), "--level".to_string()], vec!["parsed".to_string()])
    );
}

/// Isomorphic is the same: a value read through a wrapper (parentheses, `!`, `as`, `satisfies`,
/// `await`) is the value inside it. Each of `Store`'s methods changes the rows it holds, however the
/// field is written, and a cast names the class a method runs on.
#[test]
fn a_value_through_its_wrappers_is_the_value_inside() {
    let g = written(
        "wrappers",
        &[
            ("package.json", "{\"name\": \"wrappers\", \"version\": \"0.0.0\", \"type\": \"module\"}\n"),
            (
                "src/store.ts",
                "export class Store {\n  rows: string[] = [];\n  plain(r: string): void {\n    this.rows.push(r);\n  }\n  bang(r: string): void {\n    (this.rows!).push(r);\n  }\n  cast(r: string): void {\n    (this.rows as string[]).push(r);\n  }\n  meets(r: string): void {\n    (this.rows satisfies string[]).push(r);\n  }\n}\n\nexport function fill(x: unknown): void {\n  (x as Store).plain(\"a\");\n}\n",
            ),
        ],
    );
    let r = &g.root;
    let mutates = edges(&g, EdgeKind::Mutates);
    for m in ["plain", "bang", "cast", "meets"] {
        assert!(has(&mutates, &format!("{r}.store.Store.{m}"), &format!("{r}.store.Store")), "{m} changes the Store: {mutates:?}");
    }
    assert!(has(&edges(&g, EdgeKind::Calls), &format!("{r}.store.fill"), &format!("{r}.store.Store.plain")), "a cast names the class");
}

/// An object literal the compiler checks against a type of the program's builds a value of it, as
/// `new` does, a struct expression in Rust and a call of the class in Python (#154): handed to a
/// parameter its function writes with the type, also of a method or a constructor and of a function
/// another module declares, written `satisfies` or `as` the type. The call it is handed to is handed
/// what it builds. A literal written for a type that is no plain name of the program's
/// (`Partial<Box>`) builds nothing, and neither does one handed to a rest parameter.
#[test]
fn an_object_literal_of_a_type_of_the_programs_builds_it() {
    let g = written(
        "literals",
        &[
            ("package.json", r#"{ "name": "literals" }"#),
            ("tsconfig.json", "{}"),
            (
                "src/model.ts",
                r#"export interface Box {
  size: number;
}

export function weigh(b: Box, scale: number): number {
  return b.size * scale;
}
"#,
            ),
            (
                "src/main.ts",
                r#"import { weigh } from "./model.js";
import type { Box } from "./model.js";

class Shelf {
  constructor(readonly first: Box) {}

  put(b: Box): number {
    return b.size;
  }
}

function part(b: Partial<Box>): number {
  return b.size ?? 0;
}

function all(...bs: Box[]): number {
  return bs.length;
}

export function main(n: number): number {
  const shelf = new Shelf({ size: n });
  const kept = { size: 2 } satisfies Box;
  const cast = { size: 3 } as Box;
  return weigh({ size: 1 }, 2) + shelf.put({ size: n }) + part({ size: 4 }) + all({ size: 5 }) + kept.size + cast.size;
}
"#,
            ),
        ],
    );
    let main = g.node(&format!("{}.main.main", g.root)).expect("main");
    let bx = format!("{}.model.Box", g.root);
    // Each literal that builds a Box, by where its value goes: the name that keeps it, or the call
    // that is handed it.
    let last = |id: &str| id.rsplit('.').next().unwrap_or(id).to_string();
    let built: Vec<String> = main
        .sites
        .iter()
        .filter(|s| s.to == bx)
        .flat_map(|s| s.gives.iter())
        .map(|h| match main.sites.iter().find(|c| h.name.starts_with('#') && c.hands.iter().any(|x| x.name == h.name)) {
            Some(c) => last(&c.to),
            None => h.name.clone(),
        })
        .collect();
    assert_eq!(
        built,
        ["Shelf", "kept", "cast", "weigh", "put"],
        "`new Shelf({{..}})`, `satisfies`, `as`, `weigh({{..}})`, `shelf.put({{..}})`: {:#?}",
        main.sites
    );
    let kept: Vec<&str> = main
        .sites
        .iter()
        .filter(|s| s.to == bx)
        .flat_map(|s| s.gives.iter().map(|h| h.name.as_str()))
        .filter(|n| !n.starts_with('#'))
        .collect();
    assert_eq!(kept, ["kept", "cast"], "a literal kept by a name gives it");
    let weigh = main.sites.iter().find(|s| s.to.ends_with(".weigh")).expect("weigh");
    assert!(weigh.hands.iter().any(|h| h.ty == bx && h.name.starts_with('#')), "weigh is handed the Box the literal builds: {weigh:#?}");
    let part = main.sites.iter().find(|s| s.to.ends_with(".part")).expect("part");
    assert!(part.hands.is_empty(), "`Partial<Box>` is no Box: {part:#?}");
}

/// A call gives anew each value it changes among what it is handed, under the name it was handed
/// (#129): the value it is called on where the method changes `this`, and each argument whose
/// parameter the function changes, also through a call of another that changes it (`refill` hands
/// its `xs` to `addAll`). A change through a projection changes its owner (`fill(box.items)` gives
/// `box`), a name of no type of the program's is of the type the change is drawn as (an Array), and
/// what the call makes anew (a spread) changes nothing of the body's. `this.items.push(..)` is the
/// language's `push` on a part of the value the method runs on: it is handed `this` and gives it
/// anew.
#[test]
fn a_call_gives_anew_what_it_changes_of_what_it_is_handed() {
    let g = written(
        "changed",
        &[
            ("package.json", r#"{ "name": "changed" }"#),
            ("tsconfig.json", "{}"),
            (
                "src/main.ts",
                r#"export class Box {
  items: string[] = [];

  add(x: string): void {
    this.items.push(x);
  }
}

function addAll(xs: string[], more: string[]): void {
  xs.push(...more);
}

function refill(xs: string[]): void {
  addAll(xs, ["a"]);
}

function fill(items: string[]): void {
  items.push("b");
}

export function main(): number {
  const box = new Box();
  const other = new Box();
  box.add("a");
  const lines: string[] = [];
  refill(lines);
  fill(box.items);
  fill([...other.items]);
  return lines.length + box.items.length;
}
"#,
            ),
        ],
    );
    let r = &g.root;
    let mut via: Vec<(String, String)> = g
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Mutates && e.src == format!("{r}.main.refill"))
        .map(|e| (e.dst.clone(), e.via.clone().unwrap_or_default()))
        .collect();
    via.sort();
    assert_eq!(via, [("ext:javascript.Array".to_string(), "xs".to_string())], "refill changes what it hands addAll");
    let main = g.node(&format!("{r}.main.main")).expect("main");
    let gives = |to: &str| -> Vec<Vec<(String, String)>> {
        main.sites
            .iter()
            .filter(|s| s.to.ends_with(to))
            .map(|s| s.gives.iter().map(|h| (h.ty.rsplit('.').next().unwrap_or("").to_string(), h.name.clone())).collect())
            .collect()
    };
    let bx = ("Box".to_string(), "box".to_string());
    assert_eq!(gives(".Box.add"), [vec![bx.clone()]], "`box.add(..)` changes the box");
    assert_eq!(gives(".refill"), [vec![("Array".to_string(), "lines".to_string())]], "through `addAll`");
    assert_eq!(gives(".fill"), [vec![bx], vec![]], "`fill(box.items)` changes the box; a spread is a new array");
    let add = g.node(&format!("{r}.main.Box.add")).expect("add");
    let push = add.sites.iter().find(|s| s.op == "push").unwrap_or_else(|| panic!("{:#?}", add.sites));
    let held = |hs: &[atlas_engine::Held]| -> Vec<(String, String)> { hs.iter().map(|h| (h.ty.clone(), h.name.clone())).collect() };
    let this = format!("{r}.main.Box");
    assert!(push.changes && !push.not_followed, "{push:#?}");
    assert!(held(&push.hands).contains(&(this.clone(), String::new())), "handed the box it runs on: {push:#?}");
    assert_eq!(held(&push.gives), [(this, "this".to_string())], "and gives it anew");
}
