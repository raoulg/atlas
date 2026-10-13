//! The TypeScript pair: one small program, an expense ledger, written twice. `clean` follows
//! the design principles; `shortcut` is the same program with the shortcuts people take under
//! time pressure. Both read config, input and output files, keep expenses in SQLite, ask a
//! rates service over HTTP, serve routes through Hono, run as a command and print to the
//! terminal. Each side is read as a repository of its own, so a node is named the same on both
//! sides after its root (`clean.ledger.Ledger`, `shortcut.ledger.Ledger`).
//!
//! Each test is one principle, in three statements: the clean side reads as the algebra says,
//! the shortcut shows as a shortcut (a finding, a place the code does not name, a call not
//! followed, a missing type), and the shortcut is still in the graph. A statement Atlas does not
//! hold yet is a test of its own, ignored with what Atlas gets wrong.

use std::path::PathBuf;
use std::sync::OnceLock;

use atlas_engine::principles::{check, Finding};
use atlas_engine::{analyze, EdgeKind, Graph, Node, Options, Touch};

fn side(name: &str) -> Graph {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/typescript").join(name);
    analyze(&dir, &Options::default()).unwrap_or_else(|e| panic!("the {name} side parses: {e}"))
}

fn clean() -> &'static Graph {
    static G: OnceLock<Graph> = OnceLock::new();
    G.get_or_init(|| side("clean"))
}

fn shortcut() -> &'static Graph {
    static G: OnceLock<Graph> = OnceLock::new();
    G.get_or_init(|| side("shortcut"))
}

fn findings(g: &Graph) -> Vec<Finding> {
    check(g)
}

/// A node by its id under the side's root: `ledger.Ledger.record`.
fn node<'g>(g: &'g Graph, id: &str) -> &'g Node {
    g.node(&format!("{}.{id}", g.root)).unwrap_or_else(|| panic!("{}.{id} is in the graph", g.root))
}

fn touches<'g>(g: &'g Graph, id: &str, kind: &str) -> Vec<&'g Touch> {
    node(g, id).touches.iter().filter(|t| t.kind_name() == kind).collect()
}

fn edge(g: &Graph, kind: EdgeKind, src: &str, dst: &str) -> bool {
    let (src, dst) = (format!("{}.{src}", g.root), format!("{}.{dst}", g.root));
    g.edges.iter().any(|e| e.kind == kind && e.src == src && e.dst == dst)
}

fn asked(g: &Graph, principle: &str, id: &str) -> bool {
    let id = format!("{}.{id}", g.root);
    findings(g).iter().any(|f| f.principle == principle && f.node == id)
}

fn asked_anywhere(g: &Graph, principle: &str) -> Vec<String> {
    findings(g).into_iter().filter(|f| f.principle == principle).map(|f| format!("{} {}", f.node, f.what)).collect()
}

/// The places a function's touches of `kind` name, with the functions that make them whole.
fn places(g: &Graph, id: &str, kind: &str) -> Vec<(String, Vec<String>)> {
    touches(g, id, kind).into_iter().map(|t| (t.artifact.clone(), t.by.clone())).collect()
}

// ---------- the clean side as a whole ----------

/// The clean side follows every principle `atlas check` asks about.
#[test]
fn the_clean_side_follows_every_principle() {
    let fs = findings(clean());
    assert!(fs.is_empty(), "{:#?}", fs.iter().map(|f| format!("{} {}: {}", f.principle, f.node, f.what)).collect::<Vec<_>>());
}

// ---------- doors ----------

/// A door says what goes in. TypeScript writes it as a parameter type: `Ledger.record(expense:
/// Expense)`, with the request body checked into an Expense before it reaches the door. The
/// shortcut lets the door take the body as it comes, `record(input: any)`.
#[test]
fn doors_say_what_goes_in() {
    let (c, s) = (clean(), shortcut());
    assert!(edge(c, EdgeKind::Takes, "ledger.Ledger.record", "model.Expense"), "the door takes an Expense");
    assert!(edge(c, EdgeKind::Calls, "server.app.POST /api/expenses", "expense.parseExpense"), "the route checks the body into one");
    assert!(!asked(c, "doors", "ledger.Ledger.record"));
    // the shortcut: the door takes no type
    assert_eq!(node(s, "ledger.Ledger.record").params.iter().map(|p| p.ty.as_str()).collect::<Vec<_>>(), ["any"]);
    assert!(!s.edges.iter().any(|e| e.kind == EdgeKind::Takes && e.src == "shortcut.ledger.Ledger.record"), "`any` names no type");
    // and it is still the door the route goes through
    assert!(edge(s, EdgeKind::Calls, "server.app.POST /api/expenses", "ledger.Ledger.record"));
    assert!(node(s, "ledger.Ledger").entry.is_some(), "Ledger is still exported");
}

/// A door says what comes out. TypeScript writes it as a return type on what the package exports
/// (`isolatedDeclarations` asks for it): `summarize(..): Report`. The shortcut leaves it to the
/// compiler to work out from the object literal it returns.
#[test]
fn doors_say_what_comes_out() {
    let (c, s) = (clean(), shortcut());
    assert!(edge(c, EdgeKind::Returns, "report.summarize", "model.Report"));
    assert!(!asked(c, "doors", "report.summarize"));
    assert!(asked(s, "doors", "report.summarize"), "{:?}", asked_anywhere(s, "doors"));
    assert!(node(s, "report.summarize").returns.is_none());
    assert!(edge(s, EdgeKind::Calls, "ledger.Ledger.report", "report.summarize"), "still a step of the report");
}

// ---------- gives ----------

/// A function whose value is used says what it gives back. TypeScript writes it as a return type,
/// also where the compiler could work it out: `getJson(url): Promise<unknown>`, which the caller
/// turns into a RateTable through `parseRates`. The shortcut leaves `getJson` unannotated, so what
/// `res.json()` gives (`any`) travels on as the RateTable `latest` promises.
#[test]
fn gives_a_used_value_has_a_type() {
    let (c, s) = (clean(), shortcut());
    assert_eq!(node(c, "rates.http.getJson").returns.as_ref().map(|r| r.written.as_str()), Some("Promise<unknown>"));
    assert!(edge(c, EdgeKind::Calls, "rates.http.HttpRates.latest", "rates.source.parseRates"), "what the service says is checked");
    assert!(edge(c, EdgeKind::Returns, "rates.source.parseRates", "model.RateTable"));
    // the shortcut: a missing type
    assert!(node(s, "rates.http.getJson").returns.is_none());
    assert!(!s.edges.iter().any(|e| e.kind == EdgeKind::Returns && e.src == "shortcut.rates.http.getJson"));
    // still the step that asks the service
    assert!(edge(s, EdgeKind::Calls, "rates.http.HttpRates.latest", "rates.http.getJson"));
    assert_eq!(touches(s, "rates.http.getJson", "net").len(), 1);
}

// ---------- morphisms: a type is made by an arrow ----------

/// Morphisms are functions: what a value is, is what the arrow that makes it gives. Data from the
/// world comes in as the world has it, and a step turns it into the program's own type, once.
/// TypeScript writes that step as a function from `unknown` (`parseConfig(raw: unknown): Config`,
/// `parseRates`). The shortcut asserts the type instead: `JSON.parse(..) as Config`, or
/// `const table: RateTable = JSON.parse(..)`.
#[test]
fn morphisms_a_type_comes_from_a_step() {
    let (c, s) = (clean(), shortcut());
    assert!(edge(c, EdgeKind::Calls, "config.loadConfig", "config.parseConfig"));
    assert!(edge(c, EdgeKind::Returns, "config.parseConfig", "config.Config"));
    assert!(edge(c, EdgeKind::Calls, "rates.file.FileRates.latest", "rates.source.parseRates"));
    // the shortcut still reads the files
    assert_eq!(places(s, "config.loadConfig", "disk_read").iter().filter(|(a, _)| a == "config/expenses.json").count(), 1);
    assert_eq!(touches(s, "rates.file.FileRates.latest", "disk_read").len(), 1);
}

/// Morphisms are functions: a type no arrow gives is no type of the route. The shortcut's asserted
/// types show as what they are: the value `JSON.parse` gives is a step that gives no type.
#[test]
#[ignore = "Atlas: a value whose type is only asserted (`JSON.parse(..) as Config`, `const t: RateTable = JSON.parse(..)`) leaves no trace: no step gives it, so the route shows a Config made by nothing"]
fn morphisms_an_asserted_type_shows() {
    let s = shortcut();
    for f in ["config.loadConfig", "rates.file.FileRates.latest"] {
        let untyped = node(s, f).sites.iter().any(|site| site.to.is_empty() && site.gives.iter().any(|h| h.ty.is_empty()));
        assert!(untyped, "{f}: the parsed JSON is a step that gives no type: {:#?}", node(s, f).sites);
    }
}

// ---------- composition: a route reaches its place ----------

/// Composition is call order: a route starts at an entry and ends at a place, and both are
/// visible. TypeScript writes a choice of how to write a report as an interface with a class per
/// format (`ReportExporter` with `CsvExporter`, `JsonExporter`), so `exportReport` calls
/// `exporter.write(..)` and the family is a base type to zoom by. The shortcut keeps the writers
/// in an object indexed by the format's name, `EXPORTERS[format](report)`.
#[test]
fn composition_a_route_reaches_its_place() {
    let (c, s) = (clean(), shortcut());
    assert!(edge(c, EdgeKind::Calls, "ledger.Ledger.exportReport", "exporters.exporter.ReportExporter.write"));
    for (class, writer) in [("exporters.csv.CsvExporter", "exporters.csv.CsvExporter.write"), ("exporters.json.JsonExporter", "exporters.json.JsonExporter.write")] {
        assert!(edge(c, EdgeKind::Inherits, class, "exporters.exporter.ReportExporter"), "{class} is a ReportExporter");
        assert_eq!(touches(c, writer, "disk_write").len(), 1, "{writer} writes the report");
    }
    // the shortcut: the route from `exportReport` reaches no writer
    for writer in ["exporters.csv.writeCsv", "exporters.json.writeJson"] {
        assert!(!edge(s, EdgeKind::Calls, "ledger.Ledger.exportReport", writer));
        // and the writers and their files are still there
        assert_eq!(touches(s, writer, "disk_write").len(), 1, "{writer} writes the report");
    }
    assert!(edge(s, EdgeKind::Calls, "cli.run", "ledger.Ledger.exportReport"));
}

/// Composition is call order: a call whose callee the code does not say is a step all the same,
/// one Atlas does not follow. The shortcut's call through the table shows as such a call.
#[test]
#[ignore = "Atlas: a call through a value looked up by a key (`EXPORTERS[format](report)`) is dropped: no step, and not counted among the calls not followed"]
fn composition_a_call_not_followed_shows() {
    let s = shortcut();
    let exported = node(s, "ledger.Ledger.exportReport");
    let step = exported.sites.iter().any(|site| site.to.is_empty());
    let counted = s.stats.unresolved_calls_top.iter().any(|(name, _)| name.starts_with("EXPORTERS"));
    assert!(step || counted, "sites {:#?}, not followed {:?}", exported.sites, s.stats.unresolved_calls_top);
}

// ---------- places ----------

/// Library code does not decide where data lives. TypeScript writes it as a constructor or a
/// parameter that takes the path, named where the program starts: `new FileRates(RATES_FILE)` in
/// the command. The shortcut reads the path in the library from an environment variable with a
/// literal fallback, each module its own: `process.env.RATES_FILE ?? "data/rates.json"`,
/// `path.join(process.env.OUT_DIR ?? "out", "report.csv")`.
#[test]
fn places_are_decided_where_the_program_starts() {
    let (c, s) = (clean(), shortcut());
    assert_eq!(places(c, "rates.file.FileRates.latest", "disk_read"), [("data/rates.json".to_string(), vec!["clean.cli.main".to_string()])]);
    assert_eq!(places(c, "config.loadConfig", "disk_read").iter().find(|(a, _)| !a.is_empty()).map(|(_, by)| by.clone()), Some(vec!["clean.server".to_string(), "clean.cli.main".to_string()]));
    assert!(asked_anywhere(c, "places").is_empty(), "{:?}", asked_anywhere(c, "places"));
    // the shortcut's reads and writes are still there, decided in the library itself
    for (f, kind) in [("rates.file.FileRates.latest", "disk_read"), ("exporters.csv.writeCsv", "disk_write"), ("exporters.json.writeJson", "disk_write")] {
        let found = places(s, f, kind);
        assert_eq!(found.len(), 1, "{f}: {found:?}");
        assert!(found[0].1.is_empty(), "{f} makes its own path whole: {found:?}");
    }
}

/// Library code does not decide where data lives. A path the library takes from the environment
/// shows as what it is: a place the code does not name, or one the library decides itself, which
/// `places` asks about.
#[test]
#[ignore = "Atlas: a path read from an environment variable with a literal fallback (`process.env.X ?? \"data/rates.json\"`) is drawn as the fallback alone, and `places` does not ask about library code that decides it"]
fn places_decided_in_library_code_show() {
    let s = shortcut();
    for (f, kind) in [("rates.file.FileRates.latest", "disk_read"), ("exporters.csv.writeCsv", "disk_write"), ("exporters.json.writeJson", "disk_write")] {
        let unnamed = touches(s, f, kind).iter().any(|t| t.artifact.is_empty() && !t.fixed);
        assert!(unnamed || asked(s, "places", f), "{f}: {:?}", places(s, f, kind));
    }
}

/// Library code does not decide where data lives, and where the program starts names it. The
/// clean exporters write where the command says: the file is handed through the base type's
/// method, `ReportExporter.write(report, file)`, to the class the command built.
#[test]
fn places_handed_through_a_base_type_are_named() {
    let c = clean();
    assert_eq!(places(c, "exporters.csv.CsvExporter.write", "disk_write").iter().map(|(a, _)| a.as_str()).collect::<Vec<_>>(), ["out/report.csv"]);
    assert_eq!(places(c, "exporters.json.JsonExporter.write", "disk_write").iter().map(|(a, _)| a.as_str()).collect::<Vec<_>>(), ["out/report.json"]);
}

// ---------- a place carries a what ----------

/// Every place carries a what: a request names the service it goes to. TypeScript writes the URL
/// as a constant where the program starts, handed to the client (`new HttpRates(RATES_URL)`). The
/// shortcut builds it in the client from an environment variable that must be set:
/// `` `${process.env.RATES_URL}/latest` ``.
#[test]
fn a_place_carries_a_what() {
    let (c, s) = (clean(), shortcut());
    assert_eq!(places(c, "rates.http.getJson", "net").iter().map(|(a, _)| a.as_str()).collect::<Vec<_>>(), ["rates.example.org"]);
    let net = touches(s, "rates.http.getJson", "net");
    assert_eq!(net.len(), 1, "the request is still there");
    assert!(net.iter().all(|t| t.artifact.is_empty() && !t.fixed), "a service the code does not name: {net:?}");
}

// ---------- changes ----------

/// A change to an input shows in the signature. TypeScript cannot say a parameter changes, so the
/// clean `normalize(expense: Expense): Expense` hands back a new expense. The shortcut files the
/// category in place: `normalize(expense: Expense): void` sets `expense.category`.
#[test]
fn changes_show_in_the_signature() {
    let (c, s) = (clean(), shortcut());
    assert!(edge(c, EdgeKind::Returns, "expense.normalize", "model.Expense"));
    assert!(!c.edges.iter().any(|e| e.kind == EdgeKind::Mutates && e.src == "clean.expense.normalize"));
    assert!(asked_anywhere(c, "changes").is_empty(), "{:?}", asked_anywhere(c, "changes"));
    assert!(asked(s, "changes", "expense.normalize"), "{:?}", asked_anywhere(s, "changes"));
    assert!(edge(s, EdgeKind::Calls, "ledger.Ledger.record", "expense.normalize"), "still a step of record");
}

// ---------- nothing ----------

/// A call leaves something behind. TypeScript writes the configuration as a value handed to the
/// object that uses it (`new Ledger(config, ..)`, a `Config` the Ledger has). The shortcut keeps it
/// in a module-level `let`, set by `configure(config): void` and read back through `settings()`.
#[test]
fn nothing_a_call_leaves_something_behind() {
    let (c, s) = (clean(), shortcut());
    assert!(c.edges.iter().any(|e| e.kind == EdgeKind::Has && e.src == "clean.ledger.Ledger" && e.dst == "clean.config.Config"));
    assert!(!findings(c).iter().any(|f| f.principle == "nothing" && f.node.starts_with("clean.config")));
    assert!(asked(s, "nothing", "config.configure"), "{:?}", asked_anywhere(s, "nothing"));
    assert!(edge(s, EdgeKind::Calls, "cli.run", "config.configure") && edge(s, EdgeKind::Calls, "server", "config.configure"), "still called where the program starts");
    assert!(edge(s, EdgeKind::Calls, "ledger.Ledger.record", "config.settings"));
}

// ---------- choices ----------

/// A choice is made once, in a place with a name. TypeScript writes a command line as one
/// `switch` on the command, each case once. The shortcut is a catch-all `run(kind: string, opts:
/// any)` that tests `kind` again where the cases share work.
#[test]
fn choices_are_made_once() {
    let (c, s) = (clean(), shortcut());
    assert!(asked_anywhere(c, "choices").is_empty(), "{:?}", asked_anywhere(c, "choices"));
    assert!(!node(c, "cli.main").compares.is_empty(), "the command is a choice");
    assert!(asked(s, "choices", "cli.run"), "{:?}", asked_anywhere(s, "choices"));
    for step in ["ledger.Ledger.importFile", "ledger.Ledger.report", "ledger.Ledger.exportReport"] {
        assert!(edge(s, EdgeKind::Calls, "cli.run", step), "{step} is still a step of the command");
    }
}

// ---------- layers ----------

/// The packages of a part depend on one another in one direction. TypeScript keeps an interface
/// in a module of its own and picks the implementation where the program starts (the command
/// builds `FileRates` or `HttpRates`). The shortcut puts the pick beside the interface
/// (`rateSource()` in `rates/source.ts`), which imports the classes that implement it.
#[test]
fn layers_run_one_way() {
    let (c, s) = (clean(), shortcut());
    assert!(asked_anywhere(c, "layers").is_empty(), "{:?}", asked_anywhere(c, "layers"));
    let arms = |to: &str| node(c, "cli.main").sites.iter().find(|x| x.to == format!("clean.{to}")).map(|x| x.arms.clone()).unwrap_or_default();
    let (file, http) = (arms("rates.file.FileRates"), arms("rates.http.HttpRates"));
    assert!(!file.is_empty() && !http.is_empty() && file != http, "the command picks one: {file:?} | {http:?}");
    let layers: Vec<Finding> = findings(s).into_iter().filter(|f| f.principle == "layers").collect();
    assert_eq!(layers.len(), 1, "{layers:?}");
    assert!(layers[0].what.contains("file, http and source"), "{}", layers[0].what);
    assert!(edge(s, EdgeKind::Instantiates, "rates.source.rateSource", "rates.file.FileRates") && edge(s, EdgeKind::Instantiates, "rates.source.rateSource", "rates.http.HttpRates"));
}

// ---------- lens ----------

/// What a lens puts, it gets back. TypeScript writes a store that saves an `Expense` and reads
/// rows back through a check into an `Expense` (`storedExpense`). The shortcut gives the rows back
/// as they come from the driver, `Record<string, unknown>[]`.
#[test]
fn lens_gives_back_what_it_puts() {
    let (c, s) = (clean(), shortcut());
    assert!(edge(c, EdgeKind::Takes, "store.ExpenseStore.add", "model.Expense"));
    assert!(edge(c, EdgeKind::Returns, "store.ExpenseStore.all", "model.Expense"));
    // the shortcut: what comes back has no type
    assert!(edge(s, EdgeKind::Takes, "store.ExpenseStore.add", "model.Expense"));
    assert!(!s.edges.iter().any(|e| e.kind == EdgeKind::Returns && e.src == "shortcut.store.ExpenseStore.all"));
    assert!(edge(s, EdgeKind::Calls, "ledger.Ledger.report", "store.ExpenseStore.all"), "still read by the report");
}

/// What a lens puts, it gets back, at one place. The store's table is that place: the clean
/// store puts and gets an Expense there, and `lens` asks the shortcut what the table holds.
#[test]
fn lens_puts_and_gets_at_the_table() {
    let (c, s) = (clean(), shortcut());
    for g in [c, s] {
        let put: Vec<_> = touches(g, "store.ExpenseStore.add", "db_sqlite").iter().map(|t| (t.artifact.clone(), t.dir_name())).collect();
        let got: Vec<_> = touches(g, "store.ExpenseStore.all", "db_sqlite").iter().map(|t| (t.artifact.clone(), t.dir_name())).collect();
        assert_eq!(put, [("expenses".to_string(), "write")], "{}", g.root);
        assert_eq!(got, [("expenses".to_string(), "read")], "{}", g.root);
    }
    assert!(!asked(c, "lens", "store.ExpenseStore"));
    assert!(asked(s, "lens", "store.ExpenseStore"), "{:?}", asked_anywhere(s, "lens"));
}
