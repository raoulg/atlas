//! The TypeScript pair: one small program, an expense ledger, written twice. `clean` follows
//! the design principles; `shortcut` is the same program with the shortcuts people take under
//! time pressure. Both read config, input and output files, keep expenses in SQLite, ask a
//! rates service over HTTP, serve routes through Hono, run as a command and print to the
//! terminal. Each side is read as a repository of its own, and both are the package `expenses`, so a
//! node has the same id on both sides (`expenses.ledger.Ledger`), as in every pair.
//!
//! Each test is one principle, in three statements: the clean side reads as the algebra says,
//! the shortcut shows as a shortcut (a finding, a place the code does not name, a call not
//! followed, a missing type), and the shortcut is still in the graph. A statement Atlas does not
//! hold yet is a test of its own, ignored with what Atlas gets wrong.

mod common;

use common::Pair;

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::OnceLock;

use atlas_engine::principles::{check, Finding};
use atlas_engine::{analyze, flow, EdgeKind, Graph, Node, NodeKind, Options, Touch};

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

/// The nodes `principle` asks about, in the order it asks.
fn asked_nodes(g: &Graph, principle: &str) -> Vec<String> {
    findings(g).into_iter().filter(|f| f.principle == principle).map(|f| f.node).collect()
}

/// What a body does that Atlas does not see, as the body writes it (`Node::unseen`).
fn unseen<'g>(g: &'g Graph, id: &str) -> Vec<&'g str> {
    node(g, id).unseen.iter().map(|(_, what)| what.as_str()).collect()
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
    // the shortcut: the door takes the unchecked type, which the picture shows as written
    let params = &node(s, "ledger.Ledger.record").params;
    assert_eq!(params.iter().map(|p| (p.ty.as_str(), p.unchecked)).collect::<Vec<_>>(), [("any", true)], "`any` says nothing");
    assert!(!s.edges.iter().any(|e| e.kind == EdgeKind::Takes && e.src == "expenses.ledger.Ledger.record"), "`any` names no type");
    assert!(asked(s, "doors", "ledger.Ledger.record"), "{:?}", asked_anywhere(s, "doors"));
    // and it is still the door the route goes through
    assert!(edge(s, EdgeKind::Calls, "server.app.POST /api/expenses", "ledger.Ledger.record"));
    assert!(node(s, "ledger.Ledger").entry.is_some(), "Ledger is still exported");
}

/// A door says what goes in, and `any` says nothing: it is a parameter without a type. `unknown`
/// is a type: the one for what is still to be checked, which TypeScript writes where data from the
/// world comes in (`parseConfig(raw: unknown)`, `parseExpense`, `parseRates`). The shortcut's
/// `record(input: any)` is asked what it takes.
#[test]
fn doors_any_says_nothing() {
    let (c, s) = (clean(), shortcut());
    for f in ["config.parseConfig", "expense.parseExpense", "rates.source.parseRates"] {
        assert_eq!(node(c, f).params.iter().map(|p| p.ty.as_str()).collect::<Vec<_>>(), ["unknown"], "{f}");
        assert!(node(c, f).entry.is_some() && !asked(c, "doors", f), "{f} is a door that says what it takes");
    }
    let record = findings(s).into_iter().find(|f| f.principle == "doors" && f.node == "expenses.ledger.Ledger.record");
    assert_eq!(record.as_ref().map(|f| f.about("takes")), Some(vec!["input"]), "{record:?}");
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
/// `res.json()` gives (`any`) travels on as the RateTable `latest` promises, and `gives` asks.
#[test]
fn gives_a_used_value_has_a_type() {
    let (c, s) = (clean(), shortcut());
    assert_eq!(node(c, "rates.http.getJson").returns.as_ref().map(|r| r.written.as_str()), Some("Promise<unknown>"));
    assert!(edge(c, EdgeKind::Calls, "rates.http.HttpRates.latest", "rates.source.parseRates"), "what the service says is checked");
    assert!(edge(c, EdgeKind::Returns, "rates.source.parseRates", "model.RateTable"));
    assert!(asked_anywhere(c, "gives").is_empty(), "{:?}", asked_anywhere(c, "gives"));
    // the shortcut: a missing type
    assert!(node(s, "rates.http.getJson").returns.is_none());
    assert_eq!(asked_nodes(s, "gives"), ["expenses.rates.http.getJson"], "{:?}", asked_anywhere(s, "gives"));
    assert!(!s.edges.iter().any(|e| e.kind == EdgeKind::Returns && e.src == "expenses.rates.http.getJson"));
    // still the step that asks the service
    assert!(edge(s, EdgeKind::Calls, "rates.http.HttpRates.latest", "rates.http.getJson"));
    assert_eq!(touches(s, "rates.http.getJson", "net").len(), 1);
}

/// A function whose value is used says what it gives back, and a call says whether its value is
/// used: `parseRates(await getJson(..))` and `return getJson(..)` use it, `this.store.add(filed)`
/// standing as a statement does not. Every value the clean side uses comes from a function that
/// says what it is; the shortcut uses what `getJson` gives without saying.
#[test]
fn gives_a_call_says_whether_its_value_is_used() {
    let (c, s) = (clean(), shortcut());
    let uses = |g: &Graph, src: &str, dst: &str| {
        g.edges
            .iter()
            .any(|e| e.kind == EdgeKind::Calls && e.uses && e.src == format!("{}.{src}", g.root) && e.dst == format!("{}.{dst}", g.root))
    };
    for g in [c, s] {
        assert!(uses(g, "rates.http.HttpRates.latest", "rates.http.getJson"), "{}", g.root);
        assert!(
            edge(g, EdgeKind::Calls, "ledger.Ledger.record", "store.ExpenseStore.add")
                && !uses(g, "ledger.Ledger.record", "store.ExpenseStore.add"),
            "{}: a call standing as a statement",
            g.root
        );
    }
    let unsaid = |g: &Graph| -> Vec<String> {
        g.edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Calls && e.uses && g.node(&e.dst).map_or(false, |f| f.returns.is_none() && !f.ctor))
            .map(|e| format!("{} -> {}", e.src, e.dst))
            .collect()
    };
    assert!(unsaid(c).is_empty(), "{:?}", unsaid(c));
    assert!(unsaid(s).contains(&"expenses.rates.http.HttpRates.latest -> expenses.rates.http.getJson".to_string()), "{:?}", unsaid(s));
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
    assert!(!touches(s, "rates.file.FileRates.latest", "disk_read").is_empty());
}

/// An object literal the compiler checks against a type of the program's makes a value of it, as
/// `new` does: the clean `parseConfig(raw: unknown): Config` gives back `{ currency, categories }`,
/// a step to `Config` handed what it checked, as `const s: Settings = { .. }` is. The shortcut's
/// `loadConfig` asserts what `JSON.parse` gives to be a Config and makes none.
#[test]
fn morphisms_a_returned_literal_is_a_value_of_its_type() {
    let (c, s) = (clean(), shortcut());
    let config = format!("{}.config.Config", c.root);
    let made = node(c, "config.parseConfig").sites.iter().find(|site| site.to == config).expect("parseConfig makes a Config");
    let hands: Vec<&str> = made.hands.iter().map(|h| h.name.as_str()).collect();
    assert_eq!(hands, ["currency", "categories"]);
    assert!(edge(c, EdgeKind::Instantiates, "config.parseConfig", "config.Config"));
    assert!(!node(s, "config.loadConfig").sites.iter().any(|site| site.to == config), "{:#?}", node(s, "config.loadConfig").sites);
}

/// Morphisms are functions: a type no arrow gives is no type of the route. The shortcut's asserted
/// types show as what they are: the value `JSON.parse` gives is a step that gives no type.
#[test]
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
    for (class, writer) in [
        ("exporters.csv.CsvExporter", "exporters.csv.CsvExporter.write"),
        ("exporters.json.JsonExporter", "exporters.json.JsonExporter.write"),
    ] {
        assert!(edge(c, EdgeKind::Inherits, class, "exporters.exporter.ReportExporter"), "{class} is a ReportExporter");
        assert_eq!(touches(c, writer, "disk_write").len(), 1, "{writer} writes the report");
    }
    // the shortcut: the route from `exportReport` reaches no writer
    for writer in ["exporters.csv.writeCsv", "exporters.json.writeJson"] {
        assert!(!edge(s, EdgeKind::Calls, "ledger.Ledger.exportReport", writer));
        // and the writers and their files are still there
        assert!(!touches(s, writer, "disk_write").is_empty(), "{writer} writes the report");
    }
    assert!(edge(s, EdgeKind::Calls, "cli.run", "ledger.Ledger.exportReport"));
}

/// Composition is call order: a call whose callee the code does not say is a step all the same,
/// one Atlas does not follow. The shortcut's call through the table shows as such a call.
#[test]
fn composition_a_call_not_followed_shows() {
    let s = shortcut();
    let exported = node(s, "ledger.Ledger.exportReport");
    let step = exported.sites.iter().any(|site| site.to.is_empty());
    let counted = s.stats.unresolved_calls_top.iter().any(|(name, _)| name.starts_with("EXPORTERS"));
    assert!(step || counted, "sites {:#?}, not followed {:?}", exported.sites, s.stats.unresolved_calls_top);
}

/// A guard of the language's narrows what it tests where it holds: after `!Array.isArray(categories)
/// ||`, `categories` is an array, and `categories.every(..)` is the language's own operation, which
/// Atlas reads, not a call it cannot follow. So is the lookup that gives `categories`: `fields` is
/// what `fieldsOf` gives, a `ReadonlyMap` as its signature writes it, and `fields.get(..)` is the
/// Map's operation, a step where the body names what it gives.
#[test]
fn composition_a_method_on_what_a_guard_narrowed_is_the_language_s() {
    let parse = node(clean(), "config.parseConfig");
    assert!(parse.sites.iter().all(|s| !s.not_followed), "{:#?}", parse.sites);
    let gets: Vec<&str> = parse.sites.iter().filter(|s| s.op == "get").flat_map(|s| s.gives.iter().map(|h| h.name.as_str())).collect();
    assert_eq!(gets, ["currency", "categories"], "{:#?}", parse.sites);
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
    assert_eq!(
        places(c, "rates.file.FileRates.latest", "disk_read"),
        [("data/rates.json".to_string(), vec!["expenses.cli.main".to_string()])]
    );
    assert_eq!(
        places(c, "config.loadConfig", "disk_read").iter().find(|(a, _)| !a.is_empty()).map(|(_, by)| by.clone()),
        Some(vec!["expenses.server".to_string(), "expenses.cli.main".to_string()])
    );
    assert!(asked_anywhere(c, "places").is_empty(), "{:?}", asked_anywhere(c, "places"));
    // the shortcut's reads and writes are still there, made whole in the library itself
    for (f, kind) in
        [("rates.file.FileRates.latest", "disk_read"), ("exporters.csv.writeCsv", "disk_write"), ("exporters.json.writeJson", "disk_write")]
    {
        let found = places(s, f, kind);
        assert!(!found.is_empty() && found.iter().all(|(_, by)| by.is_empty()), "{f} makes its own path whole: {found:?}");
    }
}

/// Library code does not decide where data lives. A path the library takes from the environment
/// with a default (`process.env.RATES_FILE ?? "data/rates.json"`) is one input, an option whoever
/// starts the program may set, as a parameter's default is: one place, drawn as its default and
/// decided by no code, and `places` does not ask.
#[test]
fn places_decided_in_library_code_show() {
    let s = shortcut();
    for (f, kind, default) in [
        ("rates.file.FileRates.latest", "disk_read", "data/rates.json"),
        ("exporters.csv.writeCsv", "disk_write", "out/report.csv"),
        ("exporters.json.writeJson", "disk_write", "out/report.json"),
    ] {
        let ts = touches(s, f, kind);
        let one = !ts.is_empty() && ts.iter().all(|t| t.artifact == default && !t.fixed && t.fixed_by.is_empty());
        assert!(one && !asked(s, "places", f), "{f}: {:?}", places(s, f, kind));
    }
}

/// Library code does not decide where data lives, and where the program starts names it. The
/// clean exporters write where the command says: the file is handed through the base type's
/// method, `ReportExporter.write(report, file)`, to the class the command built.
#[test]
fn places_handed_through_a_base_type_are_named() {
    let c = clean();
    assert_eq!(
        places(c, "exporters.csv.CsvExporter.write", "disk_write").iter().map(|(a, _)| a.as_str()).collect::<Vec<_>>(),
        ["out/report.csv"]
    );
    assert_eq!(
        places(c, "exporters.json.JsonExporter.write", "disk_write").iter().map(|(a, _)| a.as_str()).collect::<Vec<_>>(),
        ["out/report.json"]
    );
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
    assert!(!c.edges.iter().any(|e| e.kind == EdgeKind::Mutates && e.src == "expenses.expense.normalize"));
    assert!(asked_anywhere(c, "changes").is_empty(), "{:?}", asked_anywhere(c, "changes"));
    assert!(asked(s, "changes", "expense.normalize"), "{:?}", asked_anywhere(s, "changes"));
    assert!(edge(s, EdgeKind::Calls, "ledger.Ledger.record", "expense.normalize"), "still a step of record");
}

/// A call changes the value it is made on when the method it reaches changes its own; a method of
/// the program's is read, not looked up among the language's methods that change their receiver.
/// TypeScript keeps the expenses in the database, so `this.store.add(filed)` writes a row and the
/// Ledger that holds the store stays as it was. The shortcut also keeps the rows it read in the
/// store (`this.rows ??= ..`): reading changes the store, and with it the Ledger that asks.
#[test]
fn changes_are_what_the_method_reached_changes() {
    let (c, s) = (clean(), shortcut());
    let changes = |g: &Graph, f: &str| {
        g.edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Mutates && e.src == format!("{}.{f}", g.root))
            .map(|e| e.dst.clone())
            .collect::<Vec<_>>()
    };
    for g in [c, s] {
        assert!(edge(g, EdgeKind::Calls, "ledger.Ledger.record", "store.ExpenseStore.add"), "{}", g.root);
        assert!(
            changes(g, "store.ExpenseStore.add").is_empty() && changes(g, "ledger.Ledger.record").is_empty(),
            "{}: `add` keeps a row and changes no value",
            g.root
        );
    }
    for f in ["store.ExpenseStore.all", "ledger.Ledger.report", "ledger.Ledger.expenses"] {
        assert!(changes(c, f).is_empty(), "{f}");
    }
    // the shortcut: a read that changes what it reads through
    assert_eq!(changes(s, "store.ExpenseStore.all"), ["expenses.store.ExpenseStore"]);
    assert_eq!(changes(s, "ledger.Ledger.report"), ["expenses.ledger.Ledger"], "through `this.store.all()`");
    assert_eq!(changes(s, "ledger.Ledger.expenses"), ["expenses.ledger.Ledger"]);
}

// ---------- nothing ----------

/// A call leaves something behind. TypeScript writes the configuration as a value handed to the
/// object that uses it (`new Ledger(config, ..)`, a `Config` the Ledger has). The shortcut keeps it
/// in a module-level `let`, set by `configure(config): void` and read back through `settings()`:
/// a global, the place `Global(config.current)`, which `configure` writes and its users read.
/// Drawn, not asked: a global is sometimes the right choice. The shortcut's `exportReport` hands
/// its report to an exporter it looks up by the format's name, `EXPORTERS[format](report)`, a call
/// Atlas cannot follow, and gives nothing back (its `Promise<void>` says that it waits, not that it
/// fails): Atlas asks what that call leaves behind.
#[test]
fn nothing_a_call_leaves_something_behind() {
    let (c, s) = (clean(), shortcut());
    assert!(c.edges.iter().any(|e| e.kind == EdgeKind::Has && e.src == "expenses.ledger.Ledger" && e.dst == "expenses.config.Config"));
    assert!(asked_anywhere(c, "nothing").is_empty(), "{:?}", asked_anywhere(c, "nothing"));
    let global =
        |f: &str, dir: &str| touches(s, f, "global").iter().any(|t| t.artifact == "expenses.config.current" && t.dir_name() == dir);
    assert!(global("config.configure", "write") && global("config.settings", "read"));
    assert_eq!(
        asked_nodes(s, "nothing"),
        ["expenses.ledger.Ledger.exportReport"],
        "a call that keeps its value in a global leaves it behind: {:?}",
        asked_anywhere(s, "nothing")
    );
    let unseen = unseen(s, "ledger.Ledger.exportReport");
    assert!(
        unseen.len() == 1 && unseen[0].contains("EXPORTERS[format]"),
        "the exporter looked up by name is what Atlas cannot see: {unseen:?}"
    );
    assert!(
        edge(s, EdgeKind::Calls, "cli.run", "config.configure") && edge(s, EdgeKind::Calls, "server", "config.configure"),
        "still called where the program starts"
    );
    assert!(edge(s, EdgeKind::Calls, "ledger.Ledger.record", "config.settings"));
    assert!(c.nodes.iter().all(|n| n.touches.iter().all(|t| t.kind_name() != "global")), "the clean side keeps nothing in a global");
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
    let arms =
        |to: &str| node(c, "cli.main").sites.iter().find(|x| x.to == format!("expenses.{to}")).map(|x| x.arms.clone()).unwrap_or_default();
    let (file, http) = (arms("rates.file.FileRates"), arms("rates.http.HttpRates"));
    assert!(!file.is_empty() && !http.is_empty() && file != http, "the command picks one: {file:?} | {http:?}");
    let layers: Vec<Finding> = findings(s).into_iter().filter(|f| f.principle == "layers").collect();
    assert_eq!(layers.len(), 1, "{layers:?}");
    // the cycle runs through the three modules: asked at one of them, its links named by the code at their ends, each
    // implementation's class and the interface it implements, and the pick that builds both
    let cycle = ["expenses.rates.file", "expenses.rates.http", "expenses.rates.source"];
    assert!(cycle.contains(&layers[0].node.as_str()), "{}", layers[0].node);
    let mut blocks = layers[0].about("block");
    blocks.sort();
    assert_eq!(blocks, cycle, "{:?}", layers[0].about);
    let links: Vec<&str> = layers[0].about("from").into_iter().chain(layers[0].about("to")).collect();
    for end in ["rates.file.FileRates", "rates.http.HttpRates", "rates.source.RateSource", "rates.source.rateSource"] {
        assert!(links.contains(&format!("expenses.{end}").as_str()), "{end}: {links:?}");
    }
    assert!(
        edge(s, EdgeKind::Instantiates, "rates.source.rateSource", "rates.file.FileRates")
            && edge(s, EdgeKind::Instantiates, "rates.source.rateSource", "rates.http.HttpRates")
    );
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
    assert!(!s.edges.iter().any(|e| e.kind == EdgeKind::Returns && e.src == "expenses.store.ExpenseStore.all"));
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

/// One side of the collection pair (`tests/pairs/collections`): a store that reads its rows,
/// pushes one and writes them back, in Python, Rust and TypeScript alike.
fn gathering(side: &str) -> Graph {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/collections/typescript").join(side);
    analyze(&dir, &Options::default()).expect("reads")
}

/// What a call changes in place, it gives: `rows.push(new Row(reading))` on a name the body
/// declares an array is the language's own step, which makes `rows` from what it is handed, so the
/// Reading reaches what `writeRows(this.path, rows)` puts, at the place `get` reads: the path the
/// store is made with, `Disk(Store.path)`. It is no call Atlas cannot follow, which might put
/// what it is handed anywhere. The clean `get` gives the Readings back; the shortcut's gives the
/// lines, and `lens` asks where the Reading went. `DroppedStore` pushes onto another array: it puts
/// no Reading and is not asked, also where it takes `readings: Reading[]` and pushes in a loop
/// (`putAll`): `reading` is an element of the array, and the parameter is followed through it. The
/// program's own `Frozen.push` returns a new value and changes nothing.
#[test]
fn lens_what_is_pushed_onto_what_is_put_is_put() {
    let (c, s) = (gathering("clean"), gathering("shortcut"));
    assert!(findings(&c).is_empty(), "{:?}", findings(&c).iter().map(|f| format!("{} {}", f.node, f.what)).collect::<Vec<_>>());
    let put = s.node(&format!("{}.gathering.Store.put", s.root)).unwrap();
    assert!(
        put.sites.iter().any(|x| x.op == "push"
            && !x.not_followed
            && x.hands.iter().any(|h| h.name == "rows")
            && x.gives.iter().any(|g| g.name == "rows")),
        "{:?}",
        put.sites
    );
    let all = s.node(&format!("{}.gathering.Store.putAll", s.root)).unwrap();
    let element = |h: &atlas_engine::Held| h.name == "reading" && h.ty == format!("{}.gathering.Reading", s.root);
    assert!(all.sites.iter().any(|x| x.hands.iter().any(element)), "{:?}", all.sites);
    for f in ["gathering.writeRows", "gathering.Store.get"] {
        let n = s.node(&format!("{}.{f}", s.root)).unwrap();
        assert!(n.touches.iter().any(|t| t.what == "Store.path"), "{f}: {:?}", n.touches);
    }
    assert!(asked(&s, "lens", "gathering.Store"), "{:?}", asked_anywhere(&s, "lens"));
    assert!(!asked(&s, "lens", "gathering.DroppedStore"), "{:?}", asked_anywhere(&s, "lens"));
    let observe = s.node(&format!("{}.gathering.observe", s.root)).unwrap();
    assert!(observe.sites.iter().flat_map(|x| &x.gives).all(|g| g.name != "frozen"), "{:?}", observe.sites);
}

/// One side of the sum pair (`tests/pairs/sums`): a knowledge base that adds a source and gives
/// back the graph of its nodes, in Python, Rust and TypeScript alike.
fn kennis(side: &str) -> Graph {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/sums/typescript").join(side);
    analyze(&dir, &Options::default()).expect("reads")
}

/// What a lens puts may come back as a variant of a sum a get gives. A named sum is the sum it
/// names: `nodes: Node[]` with `type Node = Begrip | Bron` holds a Bron. A constructor's
/// `readonly` parameter is a field of its class, as an access modifier makes one. The clean store
/// gives the `Graaf` back and is asked nothing; the shortcut gives rows, and `lens` asks.
#[test]
fn lens_what_is_put_comes_back_as_a_variant_of_a_named_sum() {
    let (c, s) = (kennis("clean"), kennis("shortcut"));
    assert!(findings(&c).is_empty(), "{:?}", findings(&c).iter().map(|f| format!("{} {}", f.node, f.what)).collect::<Vec<_>>());
    let has = |src: &str, dst: &str, kind: EdgeKind| {
        c.edges.iter().any(|e| e.kind == kind && e.src == format!("{}.{src}", c.root) && e.dst == format!("{}.{dst}", c.root))
    };
    assert!(has("bank.Graaf", "bank.Node", EdgeKind::Has));
    assert!(has("bank.Node", "bank.Bron", EdgeKind::Variant));
    assert!(asked(&s, "lens", "bank.Kennisbank"), "{:?}", asked_anywhere(&s, "lens"));
}

/// One side of the lens pair (`tests/pairs/lens`): the Python pair's store of readings, outages
/// and summaries, as Rust and TypeScript write it.
fn harvest(side: &str) -> Graph {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/lens/typescript").join(side);
    analyze(&dir, &Options::default()).expect("reads")
}

/// What a lens puts, it gets back, also inside what a get gives; and what it puts is what reaches
/// the place, not the key it looks a value up by. The clean store gives the outages back inside a
/// station's `Harvest`, and its `summary(day)` hands the Day to `keptSummary`, which reads, and to
/// `daySummary`, whose Summary it hands to `keepSummary` to put. It is asked nothing. The
/// shortcut's `outages` gives lines, and its `summary` writes the file itself: it is asked about
/// the Outage and the Day.
#[test]
fn lens_what_is_put_comes_back_inside_and_a_key_is_no_put() {
    let (c, s) = (harvest("clean"), harvest("shortcut"));
    assert!(findings(&c).is_empty(), "{:?}", findings(&c).iter().map(|f| format!("{} {}", f.node, f.what)).collect::<Vec<_>>());
    let edge = |src: &str, dst: &str, kind: EdgeKind| {
        c.edges.iter().any(|e| e.kind == kind && e.src == format!("{}.{src}", c.root) && e.dst == format!("{}.{dst}", c.root))
    };
    assert!(edge("store.Harvest", "store.Outage", EdgeKind::Has));
    assert!(edge("store.ReadingStore.summary", "store.ReadingStore.keepSummary", EdgeKind::Calls));
    let lens: Vec<Finding> = findings(&s).into_iter().filter(|f| f.principle == "lens").collect();
    assert_eq!(lens.len(), 1, "{lens:?}");
    let mut lost = lens[0].about("lost");
    lost.sort();
    assert_eq!(lost, ["harvest.store.Day", "harvest.store.Outage"], "the Outage and the Day, not the Reading");
}

// ---------- prophecy ----------

/// One side of the prophecy pair (`tests/pairs/prophecy`): a report `Sink` that only writes, and
/// the `Store` that will replace it, sketched without bodies.
fn archive_typescript(side: &str) -> Graph {
    analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/prophecy/typescript").join(side), &Options::default())
        .expect("reads")
}

/// Sketch code kept in the tree says it is not built: Atlas reads a class marked with the `@prophecy` JSDoc tag
/// as a prophecy that supersedes the sink, and the sink as built. The shortcut writes the same
/// sketch without the mark: Atlas cannot tell it from code that is finished, and does not pretend to.
#[test]
fn prophecy_a_sketch_in_the_tree_says_it_is_not_built() {
    let (c, s) = (archive_typescript("clean"), archive_typescript("unmarked"));
    let store = |g: &Graph| {
        g.nodes
            .iter()
            .find(|n| n.id.ends_with(".Store") || n.id == "Store")
            .unwrap_or_else(|| panic!("a Store in {:?}", g.nodes.iter().map(|n| &n.id).collect::<Vec<_>>()))
            .clone()
    };
    let sketch = store(&c).prophecy.expect("the marked sketch is a prophecy");
    assert_eq!(sketch.supersedes.as_deref(), Some("Sink"));
    assert!(!sketch.veiled && sketch.sealed.is_none());
    assert!(store(&s).prophecy.is_none(), "an unmarked sketch is read as built");
    assert!(c.nodes.iter().filter(|n| n.prophecy.is_some()).count() == 2, "only the marked items are prophecies");
    assert!(s.nodes.iter().any(|n| n.name == "Store"), "the shortcut's sketch is still in the graph");
}

/// Built code does not lean on a prophecy. The clean side keeps the sketch apart: the built `run`
/// still writes to the `Sink`, and `run_stored` is a prophecy that supersedes it and is the only
/// one to call the `Store`; the sketch's empty bodies are no finding, and the prophecies answer
/// the code they supersede. The shortcut lets `run` call the `Store` itself: production would run
/// on a stub, and Atlas asks about `run`. The side with no mark has nothing to lean on.
#[test]
fn prophecy_built_code_does_not_lean_on_a_stub() {
    let (c, s, u) = (archive_typescript("clean"), archive_typescript("shortcut"), archive_typescript("unmarked"));
    let leaning = |g: &Graph| -> Vec<String> { check(g).into_iter().filter(|f| f.principle == "prophecy").map(|f| f.node).collect() };
    assert!(check(&c).is_empty(), "{:?}", check(&c).iter().map(|f| format!("{} {}", f.principle, f.what)).collect::<Vec<_>>());
    assert!(leaning(&u).is_empty());
    let run = leaning(&s);
    assert_eq!(run.len(), 1, "{run:?}");
    assert!(run[0].ends_with(".run") || run[0] == "run", "{run:?}");
    assert!(s.nodes.iter().any(|n| n.name == "Store" && n.prophecy.is_some()), "the shortcut's sketch is marked, and still leaned on");
}

/// The revelation is what the sketch leaves to do: each prophecy, what it supersedes (found in the
/// code), the bodies still to write, and the built code that leans on it. The clean side lists
/// the `Store` and `run_stored`, each superseding what it replaces, leaned on by nothing; its
/// operations are routes into what is planned. The shortcut's `Store` is leaned on by `run`. The
/// side with no mark has no revelation.
#[test]
fn prophecy_the_revelation_says_what_is_left() {
    use atlas_engine::revelation::revelation;
    let (c, s, u) = (archive_typescript("clean"), archive_typescript("shortcut"), archive_typescript("unmarked"));
    let stands = revelation(&c);
    assert_eq!(stands.len(), 2, "{stands:?}");
    assert!(stands.iter().all(|p| p.holds && p.supersedes.is_some() && p.leaned_on_by.is_empty() && p.bodies > 0), "{stands:?}");
    let store = stands.iter().find(|p| p.id.ends_with("Store")).expect("the Store is a prophecy");
    assert!(store.supersedes.as_deref().is_some_and(|r| r.ends_with("Sink")), "{store:?}");
    let routes: Vec<_> = c.nodes.iter().filter(|n| n.entry.as_ref().is_some_and(|e| e.kind == atlas_engine::EntryKind::Prophecy)).collect();
    assert!(!routes.is_empty() && routes.iter().all(|n| n.prophecy.is_some() || n.cls.is_some()), "a prophecy's operations are routes");
    let leaning = revelation(&s);
    assert_eq!(leaning.len(), 1);
    assert!(leaning[0].leaned_on_by.iter().any(|l| l.ends_with("run")), "{leaning:?}");
    assert!(revelation(&u).is_empty());
}

/// A vision that is decided is written into the tree as prophecy: what it adds is marked in the
/// language of its file, with the item it replaces, and the revelation then lists it. What exists
/// is not touched, and a vision that changes what exists is refused.
#[test]
fn prophecy_a_decided_vision_is_marked_where_it_is_written() {
    let (decided, stands) = common::decided(
        "typescript",
        "src/index.ts",
        "\n/** Where reports wait. */\nexport declare class Vault {\n  keep(report: string): void;\n}\n",
        &[("archive.Vault", "archive.Sink")],
    );
    let text = decided.files.values().next().expect("one file is written");
    assert!(text.contains("@prophecy supersedes=archive.Sink"), "{text}");
    assert_eq!(decided.marked, ["archive.Vault"]);
    let vault = stands.iter().find(|s| s.id == "archive.Vault").expect("the revelation lists what was decided");
    assert_eq!(vault.supersedes.as_deref(), Some("archive.Sink"));
    assert!(vault.holds, "{vault:?}");
    assert_eq!(stands.len(), 3, "the two prophecies of the clean side, and the new one: {stands:?}");
}

/// A prophecy whose shape waits on an open question is sealed on its issue. The seals come back
/// as a list with what the host says of each issue (here: one closed, one open), and the one whose
/// issue is closed can be broken: `sealed=..` leaves its marker and nothing else changes. The
/// lookup is handed in, so the test asks no host.
#[test]
fn prophecy_a_seal_is_listed_and_broken_when_its_issue_is_closed() {
    use atlas_engine::revelation::{revelation, seals, IssueState};
    let (copy, g) = common::written("typescript", "src/index.ts", "\n/** @prophecy supersedes=archive.Sink sealed=#7 */\nexport declare class Vault {\n  keep(report: string): void;\n}\n\n/** @prophecy sealed=#8 */\nexport declare class Crate {\n  pack(report: string): void;\n}\n");
    let lookup = |issue: &str| Some((if issue == "#7" { IssueState::Closed } else { IssueState::Open }, format!("question {issue}")));
    let listed = seals(&revelation(&g), &lookup);
    assert_eq!(
        listed.iter().map(|s| (s.id.as_str(), s.can_be_broken())).collect::<Vec<_>>(),
        [("archive.Vault", true), ("archive.Crate", false)].map(|(i, b)| (i, b))
    );
    let broken = atlas_engine::decide::unseal(&g, &["archive.Vault".to_string()]).expect("a closed seal is broken");
    let text = broken.values().next().expect("one file");
    assert!(!text.contains("#7") && text.contains("#8"), "only the broken seal leaves: {text}");
    for (rel, t) in &broken {
        std::fs::write(copy.join(rel), t).expect("writes the file");
    }
    let after = revelation(&atlas_engine::analyze(&copy, &atlas_engine::Options::default()).expect("reads"));
    let vault = after.iter().find(|s| s.id == "archive.Vault").expect("still a prophecy");
    assert!(vault.sealed.is_none() && vault.supersedes.as_deref().is_some_and(|r| r.ends_with("Sink")), "{vault:?}");
    assert_eq!(after.iter().find(|s| s.id == "archive.Crate").and_then(|s| s.sealed.as_deref()), Some("#8"));
    assert!(
        atlas_engine::decide::unseal(&g, &["archive.Store".to_string()]).is_err(),
        "a prophecy that is not sealed has no seal to break"
    );
    let _ = std::fs::remove_dir_all(copy.parent().expect("has a parent"));
}

/// Breaking a seal changes the bytes of `sealed=..` and of the separator that joins it, and no
/// other: the file comes back as it was with exactly those bytes taken out, the seal first among
/// the tag's arguments, last, or alone, in a comment of one line or of several. The file is read
/// when the seal is broken and the graph may be older: a file emptied or shortened since is an error.
#[test]
fn prophecy_breaking_a_seal_changes_its_bytes_and_no_other() {
    let sealed = "/** Where reports wait. */\n\n/** @prophecy supersedes=archive.Sink sealed=#7 */\nexport declare class Vault {\n  keep(report: string): void;\n}\n\n/** @prophecy sealed=#8 supersedes=archive.Sink */\nexport declare class Crate {\n  pack(report: string): void;\n}\n\n/**\n * Where reports rest.\n * @prophecy sealed=#9\n */\nexport declare class Shelf {\n  rest(report: string): void;\n}\n";
    let copy = common::clean_copy("typescript", "unseal");
    let file = copy.join("src/vault.ts");
    std::fs::write(&file, sealed).expect("writes");
    let g = analyze(&copy, &Options::default()).expect("reads");
    let ids = ["archive.vault.Vault", "archive.vault.Crate", "archive.vault.Shelf"].map(String::from);
    let broken = atlas_engine::decide::unseal(&g, &ids)
        .unwrap_or_else(|e| panic!("{e}: {:?}", g.nodes.iter().filter(|n| n.prophecy.is_some()).map(|n| &n.id).collect::<Vec<_>>()));
    let unsealed = sealed.replacen(" sealed=#7", "", 1).replacen("sealed=#8 ", "", 1).replacen(" sealed=#9", "", 1);
    let text = broken.get(std::path::Path::new("src/vault.ts")).cloned();
    let changed = ["", "/** Where reports wait. */\n\n"].map(|cut| {
        std::fs::write(&file, cut).expect("writes");
        atlas_engine::decide::unseal(&g, &ids).is_err()
    });
    let _ = std::fs::remove_dir_all(copy.parent().expect("has a parent"));
    assert_eq!(text, Some(unsealed));
    assert_eq!(changed, [true, true], "an emptied file, a shortened one");
}

fn fits(side: &str) -> Graph {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/sums/fits/typescript").join(side);
    analyze(&dir, &Options::default()).expect("reads")
}

/// A sum is unwrapped where it arises, written in place or by a name: `fit(): Fit | Failed`. On
/// the clean side one function takes it, `describe`, the copairing that gives a line and the
/// points to plot; the shortcut hands it to `line` and to `points`, which each choose again;
/// `points` takes `Fit | Failed | undefined`, the same sum, which may also be left out.
#[test]
fn sums_a_union_written_in_place_is_a_sum() {
    let (c, s) = (fits("clean"), fits("shortcut"));
    for g in [&c, &s] {
        let sum = format!("({root}.model.Failed | {root}.model.Fit)", root = g.root);
        let variants: Vec<&str> = g.edges.iter().filter(|e| e.kind == EdgeKind::Variant && e.src == sum).map(|e| e.dst.as_str()).collect();
        assert_eq!(variants.len(), 2, "{sum}: {variants:?}");
        assert!(g.edges.iter().any(|e| e.kind == EdgeKind::Returns && e.src.ends_with(".fitter.fit") && e.dst == sum));
    }
    let takers = |g: &Graph| -> Vec<String> {
        let sum = format!("({root}.model.Failed | {root}.model.Fit)", root = g.root);
        g.edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Takes && e.dst == sum)
            .map(|e| e.src.replacen(&format!("{}.", g.root), "", 1))
            .collect()
    };
    assert_eq!(takers(&c), ["report.describe"], "one copairing is where the sum is taken apart");
    assert!(asked_anywhere(&c, "sums").is_empty(), "{:?}", asked_anywhere(&c, "sums"));
    assert_eq!(takers(&s), ["plot.points", "report.line"], "each chooses again");
    assert_eq!(asked_nodes(&s, "sums"), [format!("({r}.model.Failed | {r}.model.Fit)", r = s.root)], "{:?}", asked_anywhere(&s, "sums"));
}

/// One side of the choices pair (`tests/pairs/choices/typescript`): a job run in one of two modes,
/// and a line that says how it went.
fn choices_side(side: &str) -> Graph {
    analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/choices/typescript").join(side), &Options::default()).unwrap()
}

/// A choice is made once, in a place with a name, and Fowler's repeated switch is between
/// functions as well as inside one. TypeScript's clean form is an `enum` with one table of what
/// each case means (`PLANS`, `MARKS`), and a function whose body is where the job comes from
/// (`source`); `report` builds its Message by one constructor in either arm, one step with a
/// choice of what it is handed, and nothing is asked. The shortcut tests `this.mode` in two methods
/// of `Runner`, `this.level` twice in `Report.line` through `const l = this.level` (`this` is the
/// value a method runs on, handed as Python's `self` is), `mode` in two functions, `level` against
/// the member `Level.Loud` in two functions (`===` and `case`), and picks where its Job comes from
/// (`load | infer`) between the steps of `run`: each is asked once, where the value is defined.
#[test]
fn choices_a_choice_is_made_once_across_bodies() {
    let (c, s) = (choices_side("clean"), choices_side("shortcut"));
    assert!(asked_anywhere(&c, "choices").is_empty(), "{:?}", asked_anywhere(&c, "choices"));
    let made = format!("{}.runs.Message", c.root);
    let message: Vec<&[(u32, u32)]> = node(&c, "runs.report").sites.iter().filter(|s| s.to == made).map(|s| s.arms.as_slice()).collect();
    // two arms of one choice, one constructor in each
    let one_choice = |a: &[(u32, u32)], b: &[(u32, u32)]| a.len() == 1 && b.len() == 1 && a[0].0 == b[0].0 && a[0].1 != b[0].1;
    assert!(message.len() == 2 && one_choice(message[0], message[1]), "one constructor in each arm of one choice: {message:?}");

    // each is asked once, where the value is defined
    let mut asked: Vec<String> = asked_nodes(&s, "choices").into_iter().map(|n| n.replacen(&format!("{}.", s.root), "", 1)).collect();
    asked.sort();
    assert_eq!(
        asked,
        ["runs.Report.line", "runs.Runner", "runs.prefix", "runs.retries", "runs.run"],
        "{:#?}",
        asked_anywhere(&s, "choices")
    );
    // what each place tests, as it writes it, and what against: the value a method runs on, a parameter, a literal, a member
    let tests = |id: &str| -> Vec<(String, String, Vec<String>)> {
        node(&s, id)
            .compares
            .iter()
            .map(|c| (c.subject.clone(), c.written.clone(), [c.cases.clone(), c.members.clone()].concat()))
            .collect()
    };
    let tested = |subject: &str, written: &str, against: &str| (subject.to_string(), written.to_string(), vec![against.to_string()]);
    for (places, subject, against) in [
        (["runs.Runner.batch", "runs.Runner.workers"], "this.mode", "parallel"),
        (["runs.retries", "runs.timeout"], "mode", "parallel"),
        (["runs.prefix", "runs.suffix"], "level", "Level.Loud"),
    ] {
        for f in places {
            assert_eq!(tests(f), [tested(subject, "", against)], "{f}");
        }
    }
    let line = &node(&s, "runs.Report.line").compares;
    assert_eq!(tests("runs.Report.line"), vec![tested("this.level", "l", "loud"); 2], "`this.level` twice, through `l`");
    assert_ne!(line[0].choice, line[1].choice, "in two choices");
    let arms = |to: &str| node(&s, "runs.run").sites.iter().find(|x| x.to == format!("{}.runs.{to}", s.root)).map(|x| x.arms.clone());
    let (load, infer) = (arms("load").unwrap_or_default(), arms("infer").unwrap_or_default());
    assert!(one_choice(&load, &infer), "where the Job comes from is one choice between the steps of run: {load:?} | {infer:?}");
    // the shortcut still runs the job and sends its line
    assert!(edge(&s, EdgeKind::Calls, "runs.run", "runs.send") && edge(&s, EdgeKind::Calls, "runs.run", "runs.load"));
}

/// One side of the lens pair's club (`tests/pairs/lens/typescript/club`): the Python club's desk,
/// which keeps visits, members and reports in SQLite, a table each, and its log, roster and board
/// in files, as TypeScript writes them.
fn club() -> &'static Pair {
    static PAIR: OnceLock<Pair> = OnceLock::new();
    PAIR.get_or_init(|| common::pair("lens/typescript/club"))
}

/// What the methods of class `id` of the club write and read, as the graph holds it: each type of the program a method that
/// writes takes, with the place it writes; and what each method that only reads gives back, with the place it reads. A place
/// is the table a statement names, or what names the file. Which written type a statement puts as a value and which it finds
/// rows by, a key, `lens` reads from the statement.
#[allow(clippy::type_complexity)]
fn written_and_read(g: &Graph, id: &str) -> (Vec<(String, String)>, Vec<(String, String)>) {
    let place = |t: &Touch| if t.artifact.is_empty() { t.what.clone() } else { t.artifact.clone() };
    let writes = |t: &&Touch| t.dir_name() == "write" || t.kind_name() == "disk_write";
    let (mut written, mut read) = (vec![], vec![]);
    for m in &node(g, id).methods {
        let n = node(g, &format!("{id}.{m}"));
        let at: Vec<&Touch> = n.touches.iter().filter(writes).collect();
        let takes = g.edges.iter().filter(|e| e.kind == EdgeKind::Takes && e.src == n.id);
        let takes: Vec<&str> = takes.filter_map(|e| e.dst.strip_prefix(&format!("{}.", g.root))).collect();
        if !at.is_empty() {
            written.extend(takes.iter().flat_map(|ty| at.iter().map(|t| (ty.to_string(), place(t)))));
        } else if let Some(r) = &n.returns {
            read.extend(n.touches.iter().map(|t| (place(t), r.written.clone())));
        }
    }
    (written, read)
}

/// A list of pairs of names, as `written_and_read` gives them.
fn pairs(names: &[(&str, &str)]) -> Vec<(String, String)> {
    names.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
}

/// What a lens puts, it gets back, at the table it puts to; and a key its statement finds rows by
/// is no value put. The clean `Desk` only writes `visits` and only reads `members`: no lens on
/// either. Its `Register` gets the Member back from `members`, and its `Visits.forget` deletes a
/// Member's visits by the Member, a key. The shortcut's `Desk` reads `visits` back as records, its
/// `Register` reads back only names (the Member inside a Report comes from `reports`), and its
/// `Visits.come` inserts a Member's name as a value: each is asked.
#[test]
fn lens_the_place_is_the_table_and_a_key_is_no_put() {
    let (c, s) = (&club().clean, &club().shortcut);
    assert!(findings(c).is_empty(), "{:?}", findings(c).iter().map(|f| format!("{} {}", f.node, f.what)).collect::<Vec<_>>());
    let at = |f: &str| touches(c, f, "db_sqlite").iter().map(|t| (t.artifact.clone(), t.dir_name())).collect::<Vec<_>>();
    assert_eq!(at("desk.Desk.members"), [("members".to_string(), "read")]);
    assert_eq!(at("desk.Visits.forget"), [("visits".to_string(), "write")]);
    assert!(edge(c, EdgeKind::Takes, "desk.Visits.forget", "model.Member"));
    // the shortcut's Desk puts a Visit into `visits` and reads back records; its Register puts a Member into `members` and
    // reads back names, a Member only inside the Report `reports` holds
    assert!(asked(s, "lens", "desk.Desk"), "{:?}", asked_anywhere(s, "lens"));
    assert_eq!(written_and_read(s, "desk.Desk"), (pairs(&[("model.Visit", "visits")]), pairs(&[("visits", "Record<string, unknown>[]")])));
    assert!(asked(s, "lens", "desk.Register"), "{:?}", asked_anywhere(s, "lens"));
    assert_eq!(
        written_and_read(s, "desk.Register"),
        (pairs(&[("model.Member", "members")]), pairs(&[("members", "string[]"), ("reports", "Report")]))
    );
    // Visits writes a Member into `visits` on both sides and reads back Visits: by the Member, a key, on the clean side, as a
    // value on the shortcut's
    let visits = (pairs(&[("model.Visit", "visits"), ("model.Member", "visits")]), pairs(&[("visits", "Visit[]")]));
    assert_eq!(written_and_read(c, "desk.Visits"), visits);
    assert_eq!(written_and_read(s, "desk.Visits"), visits);
    assert!(asked(s, "lens", "desk.Visits"), "{:?}", asked_anywhere(s, "lens"));
}

/// A get gives a value back from the place; an operation that reads the place on its way to
/// writing it is a put. The clean `Log.add` reads the file and writes it back with a line more,
/// `void`: an end product, no lens. The shortcut's `Log` also gives the lines back, and is asked
/// where the Visit went.
#[test]
fn lens_a_get_gives_a_value_back() {
    let (c, s) = (&club().clean, &club().shortcut);
    let kinds: Vec<&str> = node(c, "files.Log.add").touches.iter().map(|t| t.kind_name()).collect();
    assert!(kinds.contains(&"disk_read") && kinds.contains(&"disk_write"), "{kinds:?}");
    assert_eq!(written_and_read(c, "files.Log"), (pairs(&[("model.Visit", "Log.path")]), vec![]), "a put, and no get");
    assert!(!asked(c, "lens", "files.Log"));
    assert_eq!(written_and_read(s, "files.Log"), (pairs(&[("model.Visit", "Log.path")]), pairs(&[("Log.path", "string[]")])));
    assert!(asked(s, "lens", "files.Log"), "{:?}", asked_anywhere(s, "lens"));
}

/// What a get gives back holds what was put: the class's own type, which a constructor gives, and
/// a subclass of what a get gives, a variant of a sum left open. The clean `Roster.load` (static,
/// a constructor) and `reread` give a Roster, which holds the Members `add` puts; its `Board` pins
/// a Notice and gives the Posts back, and a Notice `extends` Post. The shortcut's `Roster.load`
/// gives a Roster that holds the lines, and its `Board` gives records back: each is asked.
#[test]
fn lens_a_get_gives_its_own_class_and_a_subclass_back() {
    let (c, s) = (&club().clean, &club().shortcut);
    assert!(node(c, "files.Roster.load").ctor);
    assert!(edge(c, EdgeKind::Has, "files.Roster", "model.Member"));
    assert!(edge(c, EdgeKind::Inherits, "model.Notice", "model.Post"));
    assert!(!asked(c, "lens", "files.Roster") && !asked(c, "lens", "files.Board"), "{:?}", asked_anywhere(c, "lens"));
    // the shortcut's Roster gives a Roster back that holds no Member, and its Board records
    let roster = (pairs(&[("model.Member", "Roster.path")]), pairs(&[("Roster.path", "Roster")]));
    assert_eq!(written_and_read(s, "files.Roster"), roster);
    assert!(!edge(s, EdgeKind::Has, "files.Roster", "model.Member"));
    assert!(asked(s, "lens", "files.Roster"), "{:?}", asked_anywhere(s, "lens"));
    let board = (pairs(&[("model.Notice", "Board.path")]), pairs(&[("Board.path", "Record<string, unknown>[]")]));
    assert_eq!(written_and_read(s, "files.Board"), board);
    assert!(asked(s, "lens", "files.Board"), "{:?}", asked_anywhere(s, "lens"));
}

/// Building a value runs the constructor of its class, and `super(..)` in it runs the one its base
/// runs, as Python's `super().__init__(..)` does: an occurrence of its own, which names the
/// constructor it runs. A Notice is built by its own constructor, which runs Post's.
#[test]
fn lens_super_in_a_constructor_runs_the_base_s() {
    let g = &club().clean;
    let bodies = atlas_engine::flow::bodies(g);
    let built = &bodies[&format!("{}.model.Notice.constructor", g.root)];
    let ran: Vec<(&str, bool)> = built.steps().iter().map(|s| (s.to.as_str(), s.named)).collect();
    assert_eq!(ran, [(format!("{}.model.Post.constructor", g.root).as_str(), true)]);
}

/// One side of the overrides pair (`tests/pairs/overrides`): event handlers and the factories of
/// shapes.
fn events(side: &str) -> Graph {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/overrides/typescript").join(side);
    analyze(&dir, &Options::default()).expect("reads")
}

/// An override keeps the promise of its base in what it gives and in what it takes, and TypeScript
/// compares types by their members. `CircleFactory.make` gives a `Circle`, which has the `area` a
/// `Shape` declares without saying it implements one, and keeps the promise of `Factory.make`. The
/// clean `ClickHandler.log` takes any `Named`, wider than the `Click` its base takes, since a Click
/// has the `name` a Named declares. The compiler checks a method's parameters both ways, so the
/// shortcut's `ClickHandler.handle(e: Click)` compiles where its base takes any `Event`: a caller
/// written against a Handler may hand it an Event that is no Click, and Atlas asks.
#[test]
fn overrides_an_override_takes_what_its_base_takes_and_a_type_is_its_members() {
    let (c, s) = (events("clean"), events("shortcut"));
    assert!(findings(&c).is_empty(), "{:?}", findings(&c).iter().map(|f| format!("{} {}", f.node, f.what)).collect::<Vec<_>>());
    assert!(c.parts.iter().all(|p| p.structural && p.leaves.overrides), "{:?}", c.parts);
    assert!(!edge(&c, EdgeKind::Inherits, "shapes.Circle", "shapes.Shape"), "a Circle is a Shape by its members alone");
    assert!(edge(&c, EdgeKind::Returns, "shapes.CircleFactory.make", "shapes.Circle"));
    assert_eq!(node(&c, "handlers.Click").fields, ["x"]);
    assert_eq!(node(&c, "handlers.Event").fields, ["name"]);
    assert!(edge(&c, EdgeKind::Takes, "handlers.ClickHandler.log", "handlers.Named"));
    // the base is asked, and its one override that breaks the promise is named
    let asked: Vec<Finding> = findings(&s).into_iter().filter(|f| f.principle == "overrides").collect();
    assert_eq!(asked.iter().map(|f| f.node.as_str()).collect::<Vec<_>>(), ["events.handlers.Handler.handle"], "{asked:#?}");
    assert_eq!(asked[0].about("breaks"), ["events.handlers.ClickHandler.handle"], "{:?}", asked[0].about);
    assert!(edge(&s, EdgeKind::Takes, "handlers.ClickHandler.handle", "handlers.Click"));
    assert!(edge(&s, EdgeKind::Inherits, "handlers.Click", "handlers.Event"));
}

/// One side of the change pair (`tests/pairs/changes`): a ledger of expenses whose functions
/// change what they are handed, in Python, Rust and TypeScript alike.
fn ledger(side: &str) -> Graph {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/changes/typescript").join(side);
    analyze(&dir, &Options::default()).expect("reads")
}

/// A change to an input shows in the signature, whatever the input's type. The clean ledger
/// changes a value through a method of its own (`Ledger.add` pushes onto its rows) or hands back a
/// new one: `withLine` and `merged` build it, `counted` sets a key of a `Map` it makes, `refiled`
/// rebinds `e` to a copy before it files it. `remove` calls `delete` on a library's client, which
/// is no array, `Map` or `Set`: a method's name changes nothing. Nothing is asked. The shortcut
/// changes what it is handed and says nothing: a `string[]` it pushes onto, a `Map` it sets a key
/// of, each expense through the loop variable that goes over the array, and an expense through a
/// second name for it. `Ledger.takeFrom` and `Ledger.takeVictim` have one body: each empties the
/// ledger it is handed as well as its own, two changes, whatever the parameter is called. Each is
/// asked, and none of them is a call that leaves nothing behind.
#[test]
fn changes_a_change_to_an_input_of_any_type_shows_in_the_signature() {
    let (c, s) = (ledger("clean"), ledger("shortcut"));
    let fs = findings(&c);
    assert!(fs.is_empty(), "{:#?}", fs.iter().map(|f| format!("{} {} {}", f.principle, f.node, f.what)).collect::<Vec<_>>());
    assert!(c.edges.iter().filter(|e| e.kind == EdgeKind::Mutates).all(|e| e.via.as_deref() == Some("this")), "only methods change");
    let mut changes: Vec<String> = findings(&s).into_iter().filter(|f| f.principle == "changes").map(|f| f.node).collect();
    changes.sort();
    let want = ["Ledger.takeFrom", "Ledger.takeVictim", "addLine", "count", "fileAll", "rename"];
    assert_eq!(changes, want.map(|f| format!("ledger.ledger.{f}")));
    assert!(asked_anywhere(&s, "nothing").is_empty(), "{:?}", asked_anywhere(&s, "nothing"));
    let inputs = |f: &str| {
        let mut via: Vec<&str> = s
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Mutates && e.src == format!("ledger.ledger.{f}"))
            .filter_map(|e| e.via.as_deref())
            .collect();
        via.sort();
        via
    };
    assert_eq!(inputs("Ledger.takeFrom"), ["other", "this"]);
    assert_eq!(inputs("Ledger.takeVictim"), ["this", "victim"]);
    assert!(
        s.edges.iter().any(|e| e.kind == EdgeKind::Mutates && e.src == "ledger.ledger.addLine" && e.dst == "ext:javascript.Array"),
        "an array is what changes"
    );
}

/// A change to a value is the caller's value at a new moment (#129): a call gives anew each value
/// it changes among what it is handed, under the name it was handed. The clean `main` hands values
/// on and gets new ones back; only `ledger.add(..)` changes what it runs on, and gives the ledger.
/// The shortcut's calls change what they are handed: `ledger.takeFrom(other)` both ledgers, the
/// Array `lines` and the Map `m` that `addLine` and `count` are handed, and the ledger whose rows
/// `fileAll` files (a change through a projection changes its owner). Inside `Ledger.add`,
/// `this.rows.push(e)` is the language's `push` on a part of the ledger: handed the ledger (#128),
/// it gives it anew. The object literal each side hands to a parameter written as an Expense is a
/// step that builds one, which the call is handed (#154).
#[test]
fn changes_a_call_gives_anew_what_it_changes_of_what_it_is_handed() {
    let (c, s) = (ledger("clean"), ledger("shortcut"));
    let changed = |g: &Graph| -> Vec<(String, Vec<String>)> {
        let mut out: Vec<(String, Vec<String>)> = node(g, "ledger.main")
            .sites
            .iter()
            .filter(|x| !x.to.is_empty() && x.hands.iter().any(|h| x.gives.iter().any(|v| v.name == h.name && !h.name.is_empty())))
            .map(|x| (x.to.rsplit('.').next().unwrap_or("").to_string(), x.gives.iter().map(|h| h.name.clone()).collect()))
            .collect();
        out.sort();
        out
    };
    let names = |xs: &[&str]| xs.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    assert_eq!(changed(&c), [("add".to_string(), names(&["ledger"]))], "only the ledger's own method changes it");
    assert_eq!(
        changed(&s),
        [
            ("count".to_string(), names(&["m"])),
            ("fileAll".to_string(), names(&["ledger"])),
            ("takeFrom".to_string(), names(&["ledger", "other"])),
            ("takeVictim".to_string(), names(&["ledger", "other"])),
        ]
    );
    let lines = node(&s, "ledger.main").sites.iter().find(|x| x.to.ends_with(".addLine")).expect("addLine");
    assert_eq!(lines.gives.iter().map(|h| (h.ty.as_str(), h.name.as_str())).collect::<Vec<_>>(), [("ext:javascript.Array", "lines")]);
    let push = node(&c, "ledger.Ledger.add").sites.iter().find(|x| x.op == "push").expect("push is a step");
    let held = |hs: &[atlas_engine::Held]| hs.iter().map(|h| (h.ty.clone(), h.name.clone())).collect::<Vec<_>>();
    assert!(push.changes && held(&push.hands).contains(&("ledger.ledger.Ledger".to_string(), String::new())), "{push:#?}");
    assert_eq!(held(&push.gives), [("ledger.ledger.Ledger".to_string(), "this".to_string())]);
    for (g, to) in [(&c, ".refiled"), (&s, ".rename")] {
        let main = &node(g, "ledger.main").sites;
        assert!(main.iter().any(|x| x.to == "ledger.ledger.Expense"), "the literal builds an Expense: {main:#?}");
        let call = main.iter().find(|x| x.to.ends_with(to)).expect("the call");
        assert!(call.hands.iter().any(|h| h.ty == "ledger.ledger.Expense"), "{to} is handed what it builds: {call:#?}");
    }
}

/// A field that holds what a package made is that package's, on any value of the class that
/// declares it, as on the body's own receiver: `api.http.delete(..)`, with `http: AxiosInstance` a
/// field of `Api`, is a request through axios, as `this.http.delete(..)` in `Api` would be. Its
/// URL waits for what the caller hands `remove`.
#[test]
fn changes_a_request_through_a_client_a_field_holds_reaches_the_web() {
    let c = ledger("clean");
    let sent: Vec<(&str, &str, &[String])> =
        node(&c, "ledger.remove").touches.iter().map(|t| (t.kind_name(), t.op.as_str(), t.by.as_slice())).collect();
    assert_eq!(sent, [("net", "delete", ["ledger.ledger.main".to_string()].as_slice())]);
}

// ---------- layers: the package quotient the picture draws ----------

/// One side of the layers pair (`tests/pairs/layers/typescript`): orders priced by their lines, and
/// a ledger that posts in money's rounding, with tests inside money that post.
fn cyc_typescript(side: &str) -> Graph {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/layers/typescript").join(side);
    analyze(&dir, &Options::default()).unwrap_or_else(|e| panic!("the {side} side parses: {e}"))
}

/// The directories of a package depend on one another in one direction, a directory's own
/// `index.ts` a block beside the files below it. The clean side keeps `tax` in a module of its own
/// below the lines and the orders. The shortcut writes it in `orders/index.ts` and lets `lines.ts`
/// import it from there, while the index prices the order by its lines.
#[test]
fn layers_a_directorys_own_index_depends_in_one_direction() {
    let (c, s) = (cyc_typescript("clean"), cyc_typescript("shortcut"));
    assert!(asked_anywhere(&c, "layers").is_empty(), "{:?}", asked_anywhere(&c, "layers"));
    // the index is a block of its own, asked for the cycle it makes with `lines` below it
    assert_eq!(asked_nodes(&s, "layers"), ["cyc.orders"], "{:?}", asked_anywhere(&s, "layers"));
    assert!(edge(&s, EdgeKind::Calls, "orders.total", "orders.lines.price"));
    assert!(edge(&s, EdgeKind::Calls, "orders.lines.price", "orders.tax"));
}

/// Test code is a way in from outside, not a layer: a `.test.ts` file and a file under `__tests__`
/// are test code. On both sides a helper in `money/round.test.ts` and one in `money/__tests__` post
/// through the ledger, which posts in money's rounding: counted as money's code they would close a
/// cycle. They are in the graph, and no cycle comes of them.
#[test]
fn layers_a_test_file_is_no_layer() {
    for side in ["clean", "shortcut"] {
        let g = cyc_typescript(side);
        assert!(node(&g, "money.round_test").test && node(&g, "money.__tests__.posting").test, "{side}: test files are test code");
        assert!(!node(&g, "money.round").test, "{side}");
        assert!(edge(&g, EdgeKind::Calls, "money.round_test.posted", "ledger.post.post"), "{side}");
        assert!(edge(&g, EdgeKind::Calls, "money.__tests__.posting.postedTwice", "ledger.post.post"), "{side}");
        assert!(edge(&g, EdgeKind::Calls, "ledger.post.post", "money.round.round"), "{side}");
        // no cycle has money as a block, or a link from or to it
        let money = format!("{}.money", g.root);
        let in_money = |id: &str| id.strip_prefix(money.as_str()).is_some_and(|rest| rest.is_empty() || rest.starts_with(['.', '$']));
        let layers: Vec<Finding> = findings(&g).into_iter().filter(|f| f.principle == "layers").collect();
        assert!(!asked(&g, "layers", "money") && layers.iter().all(|f| f.about.iter().all(|a| !in_money(&a.id))), "{side}: {layers:?}");
    }
}

// ---------- prophecy: what leans, and what a name names ----------

fn prophecy_typescript(topic: &str, side: &str) -> Graph {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/prophecy").join(topic).join("typescript").join(side);
    analyze(&dir, &Options::default()).unwrap_or_else(|e| panic!("the {side} side parses: {e}"))
}

/// Test code may lean on the code it is written for before it is built. On the clean side a helper
/// in `index.test.ts` and one under `__tests__` call the prophecy `seal`: Atlas asks nothing. The
/// shortcut writes the same helper in `check.ts`, beside the code it checks: built code that would
/// run on a stub, and asked.
#[test]
fn prophecy_a_test_leans_on_a_prophecy_freely() {
    let (c, s) = (prophecy_typescript("tested", "clean"), prophecy_typescript("tested", "shortcut"));
    assert!(findings(&c).is_empty(), "{:?}", findings(&c).iter().map(|f| format!("{} {}", f.principle, f.what)).collect::<Vec<_>>());
    assert!(edge(&c, EdgeKind::Calls, "index_test.sealedTwice", "seal"));
    assert!(edge(&c, EdgeKind::Calls, "__tests__.sealed.sealed", "seal"));
    assert!(node(&s, "seal").prophecy.is_some() && edge(&s, EdgeKind::Calls, "check.sealedTwice", "seal"), "built code calls the prophecy");
    assert_eq!(asked_nodes(&s, "prophecy"), ["archive.check.sealedTwice"], "{:?}", asked_anywhere(&s, "prophecy"));
}

/// A name the prophecy's `supersedes` writes names one node, by its whole id or by the one id that
/// ends so. The clean side names the ledger's `save` as `ledger.save` and the shelf's as `shelf.save`;
/// both hold. The shortcut writes `save`, which ends the ids of both: Atlas names both rather than
/// call the name one the code does not hold.
#[test]
fn prophecy_a_name_that_names_several_nodes_is_ambiguous() {
    let (c, s) = (prophecy_typescript("names", "clean"), prophecy_typescript("names", "shortcut"));
    assert!(asked_anywhere(&c, "prophecy").is_empty(), "{:?}", asked_anywhere(&c, "prophecy"));
    let replaces = |g: &Graph, id: &str| node(g, id).prophecy.as_ref().and_then(|p| p.replaces.clone());
    assert_eq!(replaces(&c, "savePosted").as_deref(), Some("archive.ledger.save"));
    assert_eq!(replaces(&c, "saveKept").as_deref(), Some("archive.shelf.save"));
    let said = node(&s, "savePosted").prophecy.as_ref().expect("savePosted is a prophecy");
    assert_eq!((said.supersedes.as_deref(), said.replaces.as_deref()), (Some("save"), None), "the name replaces none");
    assert_eq!(said.candidates, ["archive.ledger.save", "archive.shelf.save"], "it names both");
    assert_eq!(asked_nodes(&s, "prophecy"), ["archive.savePosted"], "{:?}", asked_anywhere(&s, "prophecy"));
}

// ---------- doors: every declaration a door is made of ----------

/// One side of the doors pair (`tests/pairs/doors`): the lengths of the Python pair in TypeScript.
fn units(side: &str) -> Graph {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/doors/typescript").join(side);
    analyze(&dir, &Options::default()).unwrap_or_else(|e| panic!("the {side} side parses: {e}"))
}

/// A door says what goes in and what comes out in each declaration its callers meet. The clean
/// `scale` says it through two typed overload signatures, before an implementation typed `any`
/// that no caller is checked against; `Length.meters` is its getter, typed, and its setter takes
/// the getter's type, as TypeScript gives it; `log(...parts: string[])` types what is left over;
/// `parse` takes `unknown`. It is asked nothing, and the route of `Length` runs its constructor
/// once, then each method a user may run on it. The shortcut leaves one overload without types,
/// the getter without a return type and `...parts` without a type, and gives `any`, which says no
/// more than no type: each is asked where it is left open.
#[test]
fn doors_every_declaration_of_a_door_says_what_goes_in_and_out() {
    let (c, s) = (units("clean"), units("shortcut"));
    let fs = findings(&c);
    assert!(fs.is_empty(), "{:#?}", fs.iter().map(|f| format!("{} {}: {}", f.principle, f.node, f.what)).collect::<Vec<_>>());
    // one node per name: the implementation with the overloads its callers see, the property's getter
    let scale = node(&c, "core.scale");
    let typed: Vec<Vec<&str>> = scale.overloads.iter().map(|o| o.params.iter().map(|p| p.ty.as_str()).collect()).collect();
    assert_eq!(typed, [["number", "number"], ["bigint", "bigint"]], "two typed overloads");
    let body = scale.span.expect("the implementation has a body").0;
    assert!(scale.overloads.iter().all(|o| o.line < body), "written before the implementation");
    assert_eq!(scale.params.iter().map(|p| (p.ty.as_str(), p.unchecked)).collect::<Vec<_>>(), [("any", true), ("any", true)]);
    assert_eq!(node(&c, "core.Length.meters").returns.as_ref().map(|r| r.written.as_str()), Some("number"));
    assert_eq!(node(&c, "core.Length").methods, ["constructor", "meters", "plus"]);
    // the class's route runs its constructor once, then what a user may run on it
    let steps: Vec<String> = atlas_engine::flow::bodies(&c)["units.core.Length"].steps().iter().map(|s| s.to.clone()).collect();
    assert_eq!(steps, ["constructor", "meters", "plus"].map(|m| format!("units.core.Length.{m}")));
    // the shortcut: each door left open is asked, at the declaration that leaves it open
    let asked: Vec<(String, u32)> = findings(&s)
        .into_iter()
        .filter(|f| f.principle == "doors")
        .map(|f| (f.node.trim_start_matches("units.").to_string(), f.line))
        .collect();
    // the getter, log and parse at their own declarations; scale at its overload without types, the second
    let declared = |id: &str| node(&s, id).span.expect("a declaration has a span").0;
    let open = &node(&s, "core.scale").overloads;
    assert!(open.len() == 2 && !open[0].params[0].ty.is_empty() && open[1].params[0].ty.is_empty(), "{open:?}");
    let want = [
        ("core.Length.meters", declared("core.Length.meters")),
        ("core.scale", open[1].line),
        ("core.log", declared("core.log")),
        ("core.parse", declared("core.parse")),
    ];
    assert_eq!(asked, want.map(|(n, l)| (n.to_string(), l)));
    assert!(node(&s, "core.parse").returns.as_ref().is_some_and(|r| r.written == "any" && r.unchecked), "`any` is drawn as written");
}

/// One side of the nothing pair (`tests/pairs/nothing`): a job runner that runs a task, says when a
/// job is done, counts its runs, settles its log and stops the run, in Python, Rust and TypeScript
/// alike.
fn jobs(side: &str) -> Graph {
    analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/nothing/typescript").join(side), &Options::default()).unwrap()
}

/// A call leaves something behind, and where Atlas cannot see what a call leaves, the question says
/// so. The clean runner leaves each step where a reader sees it: `Echo` implements the `Task` it
/// names, `finish` is handed its callback, `count` changes a module's `let`, `settle` writes its log
/// and `stop` ends the run with `process.exit`: nothing is asked. The shortcut leaves them out of
/// sight: `Echo` has `Task`'s method without naming it, so `Task.run` is only declared; `Job` keeps
/// its callback in a field; `count` changes a static its class keeps. Atlas asks what each of those
/// leaves behind and names what it cannot see. `settle`, a door that waits and does nothing, is
/// asked whether it is dead as Python's `async def` is: a `Promise<void>` says it waits, not that
/// it fails.
#[test]
fn nothing_says_what_atlas_cannot_see() {
    let (c, s) = (jobs("clean"), jobs("shortcut"));
    let asked =
        |g: &Graph| -> Vec<String> { asked_nodes(g, "nothing").into_iter().map(|n| n.replacen(&format!("{}.", g.root), "", 1)).collect() };
    assert!(asked(&c).is_empty(), "{:#?}", asked_anywhere(&c, "nothing"));
    assert!(node(&c, "runner.stop").raises, "ending the run is what `stop` leaves behind");
    assert_eq!(asked(&s), ["runner.runTask", "runner.Job.finish", "runner.count", "runner.settle"], "{:#?}", asked_anywhere(&s, "nothing"));
    // what each cannot be seen through: a method only declared, the callback a field keeps, the static a class keeps; and
    // `settle`, which runs nothing Atlas cannot see
    assert!(edge(&s, EdgeKind::Calls, "runner.runTask", "runner.Task.run") && unseen(&s, "runner.runTask").is_empty());
    let one = |id: &str, what: &str| {
        let unseen = unseen(&s, id);
        assert!(unseen.len() == 1 && unseen[0].contains(what), "{id}: {unseen:?}");
    };
    one("runner.Job.finish", "this.onDone");
    one("runner.count", "Counter.runs");
    let settle = node(&s, "runner.settle");
    assert!(settle.unseen.is_empty() && settle.sites.is_empty() && settle.touches.is_empty(), "{settle:?}");
    // what is only declared has no body to ask about, and what implements it is still drawn
    assert!(node(&s, "runner.Task.run").declared && node(&c, "runner.Task.run").declared);
    assert_eq!(touches(&s, "runner.Echo.run", "screen").len(), 1);
    assert!(node(&s, "runner.settle").returns.as_ref().is_some_and(|r| !r.fails), "a Promise says it waits");
}

/// One side of the gives pair (`tests/pairs/gives`): readings kept in a file and a label, in
/// Python and TypeScript alike.
fn readings(side: &str) -> Graph {
    analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/gives/typescript").join(side), &Options::default()).unwrap()
}

/// A function whose value is used says what it gives back. TypeScript leaves the return type to the
/// author as Python does: the compiler works it out where a reader of the declaration does not see
/// it, and Atlas reads the declaration. The clean store says what `getStore` and `label` give; the
/// shortcut leaves both to the compiler, and `gives` asks about both, on the condition `doors` asks
/// a door on. The route loses `Store.history` at the store `getStore` makes.
#[test]
fn gives_is_asked_where_the_return_type_is_left_out() {
    let (c, s) = (readings("clean"), readings("shortcut"));
    let asked = |g: &Graph| -> Vec<String> {
        check(g).into_iter().filter(|f| f.principle == "gives").map(|f| f.node.replacen(&format!("{}.", g.root), "", 1)).collect()
    };
    assert!(asked(&c).is_empty(), "{:?}", asked(&c));
    assert_eq!(asked(&s), ["store.getStore", "store.label"]);
    assert!(edge(&c, EdgeKind::Calls, "store.main", "store.Store.history"));
    assert!(!edge(&s, EdgeKind::Calls, "store.main", "store.Store.history"), "the route loses the store's history");
}

/// The places pair, `places/typescript`: rows read and kept, and pages made from a template the
/// package ships, the same on both sides but for where the rows come from.
fn stock(side: &str) -> Graph {
    analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/places/typescript").join(side), &Options::default()).unwrap()
}

/// Library code does not decide where data lives, and a function that hands a path decides it as
/// much as one that opens it. Both sides read rows with `readRows(path)`. The clean command's
/// `main`, which the `bin` runs, hands it `"data/rows.csv"`, as where the program starts may; on
/// the shortcut the library's `defaultRows()` hands it. `places` asks `defaultRows`, whose own code
/// writes the path, and not `readRows`, which takes it.
#[test]
fn places_the_function_that_writes_the_path_decides_it() {
    let (c, s) = (stock("clean"), stock("shortcut"));
    assert!(asked_anywhere(&c, "places").is_empty(), "{:?}", asked_anywhere(&c, "places"));
    assert!(asked(&s, "places", "rows.defaultRows") && !asked(&s, "places", "rows.readRows"), "{:?}", asked_anywhere(&s, "places"));
    let read = |g: &Graph, by: &str| {
        touches(g, "rows.readRows", "disk_read").iter().any(|t| t.artifact == "data/rows.csv" && t.fixed_by == [format!("{}.{by}", g.root)])
    };
    assert!(read(&c, "cli.main") && read(&s, "rows.defaultRows"));
}

/// A default whoever calls the function or starts the program can override is an option, not a
/// decision: a parameter's (`path: string = "out/rows.csv"`) and an environment variable's
/// (`process.env.STOCK_PATH ?? "out/rows.csv"`). The two forms say the same thing and draw alike:
/// one place, the file the code takes where nobody says otherwise; `places` asks neither form.
#[test]
fn places_a_default_the_caller_can_override_is_an_option() {
    let c = stock("clean");
    assert!(!asked(&c, "places", "rows.save") && !asked(&c, "places", "rows.saveWhereTold"), "{:?}", asked_anywhere(&c, "places"));
    let told = touches(&c, "rows.saveWhereTold", "disk_write");
    assert!(told.iter().any(|t| t.artifact == "out/rows.csv" && !t.fixed && t.fixed_by.is_empty()), "{told:?}");
    assert!(told.iter().all(|t| t.artifact == "out/rows.csv"), "one place, the variable its input: {told:?}");
}

/// A file that ships with the package, found from where its code lies
/// (`new URL("./template.html", import.meta.url)`), is part of the package and no place its user
/// chooses: `page` reads `src/template.html`, and `places` does not ask.
#[test]
fn places_a_file_shipped_with_the_package_is_part_of_it() {
    let c = stock("clean");
    let read = touches(&c, "pages.page", "disk_read");
    assert!(read.iter().any(|t| t.artifact == "src/template.html" && !t.fixed && t.fixed_by.is_empty()), "{read:?}");
    assert!(!asked(&c, "places", "pages.page"), "{:?}", asked_anywhere(&c, "places"));
}

/// One side of the diamond pair (`tests/pairs/diamond`) in TypeScript, whose classes have one base
/// each: a feed that is a cached and a buffered source is a chain, `Feed extends Cached extends
/// Buffered extends Source`. The clean `Feed` says which constructor and which `read` it runs, its
/// own calling `Cached`'s through `super`; the shortcut leaves both to its bases.
fn news(side: &str) -> Graph {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/diamond/typescript").join(side);
    analyze(&dir, &Options::default()).expect("reads")
}

/// Building a value hands its arguments to the constructor it runs, its own or the first along its
/// bases, and `super(..)` in a constructor hands them to the one its base runs. `new Feed(url,
/// cache)` runs the clean feed's own constructor, which hands them on through `super(url, cache)`,
/// and the shortcut's `Cached` constructor, which `Feed` leaves to it; `Cached`'s hands the url on
/// to `Source`'s. So the address a feed fetches and the directory of the file it reads are named
/// where `headlines` builds it, on either side, and Atlas asks nothing of either.
#[test]
fn diamond_a_feed_hands_its_arguments_to_the_constructor_it_runs() {
    for side in ["clean", "shortcut"] {
        let g = &news(side);
        let headlines = format!("{}.read.headlines", g.root);
        let made = |f: &str, kind: &str| -> Vec<(String, Vec<String>)> {
            touches(g, f, kind).iter().map(|t| (t.what.clone(), t.by.clone())).collect()
        };
        let fetched = made("sources.Source.read", "net");
        assert!(fetched.iter().any(|(_, by)| by.contains(&headlines)), "{side}: {fetched:?}");
        let read = made("store.Store.get", "disk_read");
        let in_headlines = |by: &[String]| by.iter().any(|b| b.split('&').any(|f| f == headlines));
        assert!(read.iter().any(|(what, by)| what.starts_with("{cache}/") && in_headlines(by)), "{side}: {read:?}");
        assert!(edge(g, EdgeKind::Calls, "sources.Cached.constructor", "sources.Source.constructor"), "{side}");
        assert!(findings(g).is_empty(), "{side}: {:?}", findings(g).iter().map(|f| format!("{} {}", f.node, f.what)).collect::<Vec<_>>());
    }
    assert!(edge(&news("clean"), EdgeKind::Calls, "sources.Feed.constructor", "sources.Cached.constructor"));
}

/// What a route runs, through the bodies its text and its picture are drawn from (`flow::bodies`): each function a step
/// reaches and each library operation, by its id, and the steps that may run an override in their place. Building a class
/// runs its initializer.
fn route_runs(g: &Graph, entry: &str) -> (BTreeSet<String>, usize) {
    let (bodies, inits) = (flow::bodies(g), flow::initializers(g));
    let (mut runs, mut seen, mut alternatives) = (BTreeSet::new(), BTreeSet::new(), 0);
    let mut todo = vec![flow::Step::of(entry)];
    while let Some(st) = todo.pop() {
        let built = g.node(&st.to).is_some_and(|n| n.kind == NodeKind::Class);
        let Some(to) = (if built { inits.get(&st.to).cloned() } else { Some(st.to.clone()) }) else { continue };
        if !seen.insert(flow::key(&to, &st.on)) {
            continue;
        }
        if !built {
            let instead = st.runs_instead(&bodies);
            alternatives += usize::from(!instead.is_empty());
            todo.extend(instead);
        }
        todo.extend(bodies.get(&flow::key(&to, &st.on)).into_iter().flat_map(|b| b.steps()).cloned());
        runs.insert(to);
    }
    (runs, alternatives)
}

/// A feed runs the `read` its bases name, `Cached`'s, from the code of `Buffered`, whose call
/// names `Source.read`: the route reads the file the cache keeps, in the terms of the route that
/// builds the feed, and fetches nothing.
#[test]
fn diamond_a_feed_runs_the_read_its_bases_name() {
    for side in ["clean", "shortcut"] {
        let g = news(side);
        let (runs, _) = route_runs(&g, &format!("{}.read.headlines", g.root));
        let id = |f: &str| format!("{}.{f}", g.root);
        assert!(runs.contains(&id("sources.Cached.read")) && runs.contains(&id("store.Store.get")), "{side}: {runs:?}");
        assert!(!runs.contains(&id("sources.Source.read")) && !runs.contains("op:fetch"), "{side}: it fetches nothing: {runs:?}");
        let kinds: BTreeSet<&str> = runs.iter().filter_map(|f| g.node(f)).flat_map(|n| n.touches.iter().map(|t| t.kind_name())).collect();
        assert_eq!(kinds, BTreeSet::from(["disk_read"]), "{side}: the route only reads the file the cache keeps");
    }
}

/// The name a feed's file has in the cache is the url the feed is built with, which `Source`'s
/// constructor keeps: `this.url` in `Cached.read` is that field, named where `headlines` builds the
/// feed.
#[test]
fn diamond_the_name_of_a_feed_s_file_is_its_url() {
    for side in ["clean", "shortcut"] {
        let g = news(side);
        let headlines = format!("{}.read.headlines", g.root);
        let read = touches(&g, "store.Store.get", "disk_read");
        assert!(read.iter().any(|t| t.what == "{cache}/{url}" && t.by.contains(&headlines)), "{side}: {read:?}");
    }
}

/// A constructor that keeps what it builds in a field puts it there by its own code, a lens's put (#176), as Python's
/// initializer does in `tests/pairs/figures/built`: `this.store = new Store(cache)` in `Cached`'s constructor is set from the
/// store its step builds, and its own code reads that store, on either side.
#[test]
fn diamond_a_constructor_reads_the_store_it_keeps_in_a_field() {
    for side in ["clean", "shortcut"] {
        let g = news(side);
        let ctor = node(&g, "sources.Cached.constructor");
        let made: Vec<&str> =
            ctor.sites.iter().flat_map(|s| &s.gives).filter(|h| h.ty.ends_with(".store.Store")).map(|h| h.name.as_str()).collect();
        assert_eq!(made.len(), 1, "{side}: the store it builds is named by its step");
        let set = ctor.sets.iter().find(|s| s.field == "store").expect("it sets its field `store`");
        assert_eq!(set.from.iter().map(|h| h.name.as_str()).collect::<Vec<_>>(), made, "{side}: set from the store it builds");
        assert!(ctor.uses.iter().any(|u| u.name == made[0]), "{side}: its own code reads the store it keeps");
    }
}

/// One side of the cooperative pair (`tests/pairs/cooperative/typescript`): a feed that extends a
/// cached source, whose every `read` hands on to the one above it with `super.read()`; the
/// shortcut's feed hands on with `Source.prototype.read.call(this)`.
fn cooperative(side: &str) -> Graph {
    analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/cooperative/typescript").join(side), &Options::default())
        .expect("reads")
}

/// `super.read()` runs, on the body's own receiver, the next `read` above the class it is written
/// in, and a call through a class's prototype (`Source.prototype.read.call(this)`) runs that class's
/// method: neither may run an override in its place, as Python's `super().read()` and
/// `Source.read(self)` may not. The clean feed's `read` runs `Cached.read`, which reads the store
/// before it asks the network; the shortcut's runs `Source.read` and asks the network alone.
#[test]
fn cooperative_super_and_a_call_through_a_prototype_run_one_method() {
    for (side, next) in [("clean", "Cached"), ("shortcut", "Source")] {
        let g = cooperative(side);
        let bodies = atlas_engine::flow::bodies(&g);
        let read = &bodies[&format!("{}.sources.Feed.read", g.root)];
        let ran: Vec<(&str, bool)> = read.steps().iter().filter(|s| !s.to.starts_with("op:")).map(|s| (s.to.as_str(), s.named)).collect();
        assert_eq!(ran, [(format!("{}.sources.{next}.read", g.root).as_str(), true)], "{side}");
        // the route runs no step that may run an override in its place, no operation but the fetch, and reads the store
        // on the clean side alone
        let (runs, alternatives) = route_runs(&g, &format!("{}.read.headlines", g.root));
        assert_eq!(alternatives, 0, "{side}: {runs:?}");
        assert_eq!(runs.iter().filter(|f| f.starts_with("op:")).collect::<Vec<_>>(), ["op:fetch"], "{side}");
        assert_eq!(runs.contains(&format!("{}.store.Store.get", g.root)), side == "clean", "{side}: {runs:?}");
    }
}

// ---------- through: what a call runs is one answer ----------

/// What a call runs is one answer (#263), as in Python: the route's steps, what a parameter holds and what a call hands
/// on follow the calls the route draws. The through pair (`tests/pairs/through/typescript`) shows a heading read from a
/// file on a section: the clean `section.show(heading)` may run `Boxed.show` on a boxed section, the shortcut's
/// `Section.prototype.show.call(section, heading)` runs `Section.show` and no override of it. On each side the call may
/// run `Boxed.show` exactly where `Boxed.show`'s `heading` holds the heading `report` read and exactly where the call
/// hands on the error `Boxed.show` throws.
#[test]
fn through_a_call_runs_the_same_functions_for_the_route_its_parameters_and_its_errors() {
    let p = common::pair("through/typescript");
    for (side, g) in [("clean", &p.clean), ("shortcut", &p.shortcut)] {
        let [report, show, boxed] = [".report.report", ".sections.Section.show", ".sections.Boxed.show"].map(|f| format!("{}{f}", g.root));
        let bodies = flow::bodies(g);
        let call = bodies[&report].steps().into_iter().find(|s| s.to == show).expect("report shows");
        let runs = call.alts(&bodies).iter().any(|k| flow::split(k).0 == boxed);
        let holds = node(g, "sections.Boxed.show").uses.iter().any(|u| u.name == "heading");
        let site = node(g, "report.report").sites.iter().find(|s| s.to == show).unwrap();
        let hands_on = site.fails.iter().any(|e| e == "ext:javascript.Error");
        assert_eq!((holds, hands_on), (runs, runs), "{side}: the parameter and the error follow what the call runs");
        assert_eq!(runs, side == "clean", "{side}");
    }
}

/// An entry run from the command line is handed the words the human types (#152). Node holds them
/// raw in `process.argv`, as Rust's `env::args()` gives them: the shortcut keeps them under names,
/// `const [kind = "", ...flags] = process.argv.slice(2)`, a step that takes the entry's words under
/// `kind` and `flags` and declares no option, since the code names none. The clean side hands the
/// words on to `main` without a name; they go into the call around them, and no step of the
/// module's own code parses them.
#[test]
fn places_the_command_line_is_the_entrys() {
    let parses = |g: &Graph| -> Vec<(String, String, Vec<String>, Vec<String>)> {
        node(g, "cli")
            .sites
            .iter()
            .filter_map(|s| Some((s.to.clone(), s.op.clone(), s.command_line.clone()?, s.gives.iter().map(|h| h.name.clone()).collect())))
            .collect()
    };
    assert_eq!(parses(clean()), []);
    let words = vec!["kind".to_string(), "flags".to_string()];
    assert_eq!(parses(shortcut()), [(String::new(), "slice".to_string(), vec![], words)]);
}

/// A tuple is the values it pairs where it stands for one value, and a collection of tuples holds
/// pairs, of a type the code does not name (#152), as Rust's `Vec<(NaiveDate, f64)>` holds no
/// `NaiveDate`. The tuples pair (`tests/pairs/tuples/typescript`) gives each station's mean: the clean
/// side as a `Mean[]`, a record with a name, the shortcut as `[Station, number][]`, which gives no
/// Station. Both give `[Station, Store] | undefined` from `first`: a pair or nothing, one value of
/// each where there is one, and no union of a Station and a Store.
#[test]
fn a_place_carries_a_what_and_a_collection_of_pairs_names_none() {
    let p = common::pair("tuples/typescript");
    let returns = |g: &Graph, f: &str| -> Vec<String> {
        let id = format!("{}.{f}", g.root);
        let mut out: Vec<String> = g
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Returns && e.src == id)
            .map(|e| e.dst.rsplit('.').next().unwrap_or("").to_string())
            .collect();
        out.sort();
        out
    };
    assert_eq!(returns(&p.clean, "means"), ["Mean"]);
    assert_eq!(returns(&p.shortcut, "means"), Vec::<String>::new(), "a pair of a Station and a number is no Station");
    for g in [&p.clean, &p.shortcut] {
        assert_eq!(returns(g, "first"), ["Station", "Store"], "a tuple that stands for one value is the values it pairs");
    }
}

/// Which forms are choices (#130). What follows an early return is the other arm of the `if` that
/// returns, as an `else` would write it: the clean `describe` builds its Report in one arm and the
/// other of one choice. In `x ?? y` the left always runs and is no arm; the right is the one arm:
/// the clean `summary` runs `keptSummary` whenever it runs, and `keepSummary` only sometimes.
#[test]
fn choices_an_early_return_and_the_left_of_a_logical_operator() {
    let describe = node(&fits("clean"), "report.describe").sites.clone();
    let reports: Vec<&[(u32, u32)]> = describe.iter().filter(|s| s.to.ends_with(".Report")).map(|s| s.arms.as_slice()).collect();
    assert_eq!(reports.len(), 2, "{describe:#?}");
    assert!(reports[0].len() == 1 && reports[1].len() == 1, "{reports:?}");
    assert_eq!(reports[0][0].0, reports[1][0].0, "one choice: {reports:?}");
    assert_ne!(reports[0][0].1, reports[1][0].1, "two arms: {reports:?}");
    let g = harvest("clean");
    let arms = |to: &str| -> Vec<Vec<(u32, u32)>> {
        node(&g, "store.ReadingStore.summary").sites.iter().filter(|s| s.to.ends_with(to)).map(|s| s.arms.clone()).collect()
    };
    assert_eq!(arms(".keptSummary"), [vec![]], "the left of `??` always runs");
    assert!(arms(".keepSummary").iter().all(|a| a.len() == 1), "the right is the arm: {:?}", arms(".keepSummary"));
}

/// Which operations of the language's and a library's are steps (#125): one whose result the body
/// names, by a name or the names a pattern takes out of it, that changes the value it runs on, or
/// that builds the error it throws (#138). What one gives without a name goes into the call around it. `const db = new Database(this.path)`
/// is a step that gives `db` (`readback`); `text.split("\n").filter(..).map(parseLine).filter(..)`
/// in the pipeline's `parse`, returned unnamed, is none, and neither is `x(month.date)`, a call of
/// the scale d3 gave `trend`.
#[test]
fn composition_an_operation_is_a_step_where_its_result_is_named() {
    let readback = common::pair("readback/typescript");
    for g in [&readback.clean, &readback.shortcut] {
        let keep = node(g, "cli.Ledger.keep");
        let db = keep.sites.iter().find(|s| s.op == "Database()").unwrap_or_else(|| panic!("{:#?}", keep.sites));
        assert_eq!(db.gives.iter().map(|h| h.name.as_str()).collect::<Vec<_>>(), ["db"]);
    }
    let pipeline =
        analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/pipeline/typescript"), &Options::default()).expect("reads");
    assert!(node(&pipeline, "series.parse").sites.iter().all(|s| !s.to.is_empty()), "{:#?}", node(&pipeline, "series.parse").sites);
    let trend: Vec<&str> = node(&pipeline, "plot.trend").sites.iter().filter(|s| s.to.is_empty()).map(|s| s.op.as_str()).collect();
    assert_eq!(trend, ["at", "Error()", "range", "range", "line", "map", "map"], "each named, by its operation");
}

/// A call is handed the values the names in its arguments hold (#128), also through a name the body
/// binds to a value without a step (#103): a projection or an element bound first, a pattern that
/// takes a value apart, a value computed by operators, `this` in an argument. In the pipeline's
/// `trend`, the scale `range` gives `x` is handed the months (`first = series[0]`) and the settings
/// (`const { width, height } = settings`); `parseLine` builds its Month of what `const [year, ..] =
/// fields` took apart; `new Database(this.path)` is handed the ledger it runs on (`readback`), and so
/// is the step a chain on `readFileSync(this.outages, ..)` ends in, since a library's call that is no
/// step is glued into the step around it (`kept` in `lens`). A method called on `this` itself runs
/// on it and is handed nothing more.
#[test]
fn composition_a_call_is_handed_what_its_names_hold() {
    let pipeline =
        analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/pipeline/typescript"), &Options::default()).expect("reads");
    let handed = |id: &str, op: &str| -> Vec<String> {
        let sites = &node(&pipeline, id).sites;
        let site = sites.iter().find(|s| s.op == op || s.to.ends_with(op)).unwrap_or_else(|| panic!("{id} {op}: {sites:#?}"));
        let mut names: Vec<String> = site.hands.iter().map(|h| h.name.clone()).collect();
        names.sort();
        names
    };
    assert_eq!(handed("plot.trend", "range"), ["last", "series", "settings"], "`first.date`, `width - MARGIN.right` and what `at` gave");
    assert_eq!(handed("series.parseLine", ".Month"), ["fields"]);
    let readback = common::pair("readback/typescript");
    let keep = node(&readback.clean, "cli.Ledger.keep");
    let db = keep.sites.iter().find(|s| s.op == "Database()").expect("a step");
    assert_eq!(db.hands.iter().map(|h| (h.ty.as_str(), h.name.as_str())).collect::<Vec<_>>(), [("ledger.cli.Ledger", "")]);
    let g = harvest("clean");
    let kept = &node(&g, "store.ReadingStore.kept").sites;
    let map = kept.iter().find(|s| s.op == "map").unwrap_or_else(|| panic!("{kept:#?}"));
    assert!(map.hands.iter().any(|h| h.ty == "harvest.store.ReadingStore" && h.name.is_empty()), "{map:#?}");
    let history = kept.iter().find(|s| s.to.ends_with(".history")).expect("history");
    assert!(history.on_receiver && history.hands.iter().all(|h| h.ty != "harvest.store.ReadingStore"), "{history:#?}");
}

/// A value a body binds without a call comes from the values it is made of, not from beyond (#103).
/// The bound pair (`tests/pairs/bound/typescript`) shows a reading in its unit: the clean side
/// chooses the unit in a function of its own, `const unit = unitOf(kind)`; the shortcut declares
/// `let unit` and sets it in each arm of an `if`. The Reading is built of a Unit on both sides: the
/// one `unitOf` gives, or the module's `KELVIN` or `CELSIUS` the arms set.
#[test]
fn composition_a_value_bound_without_a_call_is_made_of_its_values() {
    let p = common::pair("bound/typescript");
    let built = |g: &Graph| -> Vec<(String, String)> {
        let show = node(g, "show");
        let reading = show.sites.iter().find(|s| s.to.ends_with(".Reading")).unwrap_or_else(|| panic!("{:#?}", show.sites));
        let mut held: Vec<(String, String)> =
            reading.hands.iter().map(|h| (h.ty.rsplit('.').next().unwrap_or("").to_string(), h.name.clone())).collect();
        held.sort();
        held
    };
    assert_eq!(built(&p.clean), [("Unit".to_string(), "unit".to_string())], "what `unitOf` gave, by its name");
    assert_eq!(built(&p.shortcut), [("Unit".to_string(), "CELSIUS".to_string()), ("Unit".to_string(), "KELVIN".to_string())]);
}

/// A name bound twice holds two values, one for each binding where it is in scope (#148). The
/// rebinds pair (`tests/pairs/rebinds/typescript`) resolves each base of a module in a loop, then
/// builds its `Body`: the clean side names the loop's value `base` and the body `body`, the shortcut
/// names both `b`. A use takes the binding in scope where it stands, so `resolve(b)` in the loop is
/// handed the loop's base, a string, never the `Body` the later `const b` holds; and the shortcut
/// reads as the clean side does.
#[test]
fn composition_a_name_bound_twice_is_two_values() {
    let p = common::pair("rebinds/typescript");
    let link =
        |g: &Graph| -> Node { g.nodes.iter().find(|n| n.name == "link").cloned().unwrap_or_else(|| panic!("link is in {}", g.root)) };
    let hands = |g: &Graph| -> Vec<Vec<String>> {
        link(g).sites.iter().filter(|s| s.to.ends_with(".resolve")).map(|s| s.hands.iter().map(|h| h.ty.clone()).collect()).collect()
    };
    let (clean, shortcut) = (hands(&p.clean), hands(&p.shortcut));
    assert_eq!(clean.len(), 1, "link resolves each base: {clean:?}");
    assert!(shortcut.iter().flatten().all(|t| !t.ends_with(".Body")), "the loop's `b` is no Body: {shortcut:?}");
    assert_eq!(clean, shortcut, "the shortcut reads as the clean side");
    let body = |g: &Graph| -> Vec<String> {
        link(g).sites.iter().flat_map(|s| s.gives.clone()).filter(|h| h.ty.ends_with(".Body")).map(|h| h.name).collect()
    };
    assert_eq!(body(&p.clean), ["body"]);
    assert_eq!(body(&p.shortcut), ["b"], "the later `b` holds the Body its constructor builds");
}

/// A parameter of no type of the code's is a name like any other (#128, #157): inside its function it holds what its call
/// fills it with, so a step written with it is handed it by its name, and where every call fills it with a literal, or none
/// of the program's calls it, it holds none and the step is handed nothing of it (`builder::settle_parameters`). The
/// choices pair tells the two forms apart: the shortcut's `finished(job, mode: string)` hands the string it is filled with
/// on to `new Runner(mode)` and `timeout(mode)`, the clean's `mode: Mode` is a value of the code's own type.
#[test]
fn a_parameter_of_no_type_is_handed_by_its_name_where_a_call_fills_it() {
    let pipeline =
        analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/pipeline/typescript"), &Options::default()).expect("reads");
    let hands = |g: &Graph, id: &str, op: &str| -> Vec<(String, String)> {
        let sites = &node(g, id).sites;
        let site = sites.iter().find(|s| s.op == op || s.to.ends_with(op)).unwrap_or_else(|| panic!("{id} {op}: {sites:#?}"));
        site.hands.iter().map(|h| (h.ty.clone(), h.name.clone())).collect()
    };
    let bare = |n: &str| vec![(String::new(), n.to_string())];
    assert_eq!(hands(&pipeline, "cli.monthsOf", "Number"), bare("option"), "`Number(option)`, filled by `values.months`");
    assert_eq!(hands(&pipeline, "series.smooth", "floor"), bare("months"), "`Math.floor(months / 2)`, filled by `settings.months`");
    let choices = common::pair("choices/typescript");
    assert_eq!(hands(&choices.shortcut, "runs.finished", ".Runner"), bare("mode"));
    assert_eq!(hands(&choices.shortcut, "runs.finished", ".timeout"), bare("mode"));
    assert_eq!(hands(&choices.clean, "runs.finished", ".Runner"), [("runs.runs.Mode".to_string(), "mode".to_string())]);
    // a literal fills a parameter with no value: `count(m, "a")`'s `k`, and the notes a ledger keeps
    let changes = common::pair("changes/typescript");
    assert_eq!(hands(&changes.shortcut, "ledger.count", "set"), bare("m"), "the Map `count` is handed, not the literal `k`");
    let readback = common::pair("readback/typescript");
    let keep = node(&readback.clean, "cli.Ledger.keep");
    assert!(keep.sites.iter().all(|s| !s.op.starts_with("for note")), "a loop over the literal notes makes no value: {:#?}", keep.sites);
}

/// The tables pair (`tests/pairs/tables/typescript`): what a mode means, looked up in one table, `plans: Record<Mode, Plan>`, against
/// two lists at the same places, `modes: Mode[]` and `plans: Plan[]`. A map holds its values by its keys (#167): the clean table is one
/// value, of Plans, which the body's own lookup reads as one; the shortcut takes the modes as a value of their own.
#[test]
fn a_table_is_one_value_and_the_lists_it_is_cut_into_two() {
    common::a_table_is_one_value("typescript", "tables.tables.timeout");
}

/// The choices pair (`tests/pairs/choices/typescript`): `run` looks its mode up in the module's `PLANS: Record<Mode, Plan>`, a
/// value from beyond of the Plans it holds, as the Python pair's `run` reads its `dict[Mode, Plan]` (#171); the shortcut tests
/// a string in each function and keeps no table.
#[test]
fn a_module_s_table_is_a_value_the_body_reads() {
    common::a_module_s_table_is_a_value_the_body_reads("typescript", "runs.runs.run");
}

/// The choices pair: `report(job, settings.level, ..)` is handed the settings through their `level`, and the graph says
/// so beside the value it is handed (#172), as the Python and Rust readers do.
#[test]
fn a_call_names_the_field_a_projection_reads() {
    common::a_call_names_the_field_a_projection_reads("typescript", "runs.runs");
}

/// The choices pair: a condition names the field it tests (#184), as the Python and Rust readers read it.
#[test]
fn a_condition_names_the_field_it_tests() {
    common::a_condition_names_the_field_it_tests("typescript", "runs.runs");
}

/// A body's own code that sets a field of a value names the field (#168): the changes pair's `rename` sets the `category`
/// of the expense it is handed (`x.category = name`, through the name `x` that holds it), and a parameter the signature
/// keeps (`constructor(readonly level: Level)`) sets the field of its name of the value the constructor builds.
#[test]
fn a_set_names_the_field_it_sets() {
    let named =
        |g: &Graph, id: &str| -> Vec<(String, String)> { node(g, id).sets.iter().map(|s| (s.on.name.clone(), s.field.clone())).collect() };
    let changes = common::pair("changes/typescript");
    assert_eq!(named(&changes.shortcut, "ledger.rename"), [("e".to_string(), "category".to_string())]);
    let choices = common::pair("choices/typescript");
    assert_eq!(named(&choices.clean, "runs.Report.constructor"), [(String::new(), "level".to_string())]);
}

/// The projections pair: a projection bound to a name first is still the projection (#174), as the Python and Rust readers
/// read it.
#[test]
fn a_projection_bound_to_a_name_is_still_the_projection() {
    common::a_projection_bound_to_a_name_is_still_the_projection("typescript", "charts");
}

/// The each pair (`tests/pairs/each/typescript`): a node's places, its touches drawn by a function written in place handed
/// to `map`, which gives back what `Touch.drawn` makes as an expression (`(t) => t.drawn()`) or returns it from a block
/// (`(t) => { return t.drawn(); }`). What a function written in place gives back goes where its result goes (#177): into
/// what `map` gives, which the body's own code puts in the pairs it returns. The two forms say the same thing, and read alike:
/// the body's own code reads the Where `drawn` gives, by the name its step gives it.
#[test]
fn what_a_function_written_in_place_returns_it_gives_back() {
    let each = common::pair("each/typescript");
    for (side, g) in [("clean", &each.clean), ("shortcut", &each.shortcut)] {
        let f = node(g, "drawn");
        let given: Vec<&str> =
            f.sites.iter().filter(|s| s.to.ends_with("Touch.drawn")).flat_map(|s| &s.gives).map(|h| h.name.as_str()).collect();
        assert_eq!(given.len(), 1, "{side}: the Where is named by its step: {given:?}");
        assert!(
            f.uses.iter().any(|u| u.name == given[0] && u.ty.ends_with(".Where")),
            "{side}: its own code reads the Where: {:?}",
            f.uses
        );
    }
}

/// The value each call of `Touch.drawn` in `drawing.drawn` is handed, by its name.
fn drawn_on(g: &Graph) -> Vec<String> {
    let f = node(g, "drawn");
    f.sites.iter().filter(|s| s.to.ends_with("Touch.drawn")).flat_map(|s| s.hands.iter().map(|h| h.name.clone())).collect()
}

/// The parameter of a function written in place that an array's method hands each element is that element (#180), as
/// Rust's `.map(|t| t.drawn())` is (#170): in the each pair (`tests/pairs/each/typescript`), `t` of `touches.map((t) =>
/// t.drawn())` is a part of the `touches` the route is handed, so `Touch.drawn` is handed `touches`, on both sides. The
/// method pair (`tests/pairs/each/typescript-method`) is Rust's: the clean side maps each node's touches inside the function
/// `flatMap` hands each node, so every element is a part of the graph and the step is handed `g`, and a field declared as
/// an array is one, so neither `map` nor `flatMap` is a step; the shortcut pushes each place onto a list in two loops, a step
/// each, whose `t` the step is handed, and the push changes the list (`out: Where[]` holds Wheres).
#[test]
fn a_function_s_parameter_is_each_element_of_the_array_it_maps() {
    let each = common::pair("each/typescript");
    assert_eq!(drawn_on(&each.clean), ["touches"], "each touch is an element of the array the route is handed");
    assert_eq!(drawn_on(&each.shortcut), ["touches"], "as from a block");
    let method = common::pair("each/typescript-method");
    let ops = |g: &Graph| -> Vec<String> { node(g, "drawn").sites.iter().filter(|s| !s.op.is_empty()).map(|s| s.op.clone()).collect() };
    assert_eq!(drawn_on(&method.clean), ["g"], "each touch is a part of the graph the function is handed");
    assert!(ops(&method.clean).is_empty(), "`flatMap` and `map` on arrays are no steps: {:?}", ops(&method.clean));
    assert_eq!(drawn_on(&method.shortcut), ["t"], "the loop's touch, a value the body makes");
    // a step for each loop, each handed what it goes over and giving its element by the loop's name, then the push
    let named = |hs: &[atlas_engine::Held]| -> Vec<String> { hs.iter().map(|h| h.name.clone()).filter(|n| !n.starts_with('#')).collect() };
    let sites = node(&method.shortcut, "drawn").sites.iter().filter(|s| !s.op.is_empty());
    let steps: Vec<(Vec<String>, Vec<String>)> = sites.map(|s| (named(&s.hands), named(&s.gives))).collect();
    let step = |over: &str, each: &str| (vec![over.to_string()], vec![each.to_string()]);
    assert_eq!(steps, [step("g", "n"), step("n", "t"), step("out", "out")], "{:?}", ops(&method.shortcut));
    let push = node(&method.shortcut, "drawn").sites.iter().find(|s| s.op == "push").expect("the shortcut pushes");
    assert!(
        push.changes && push.gives.iter().any(|g| g.name == "out" && g.ty.ends_with(".Where")),
        "the push makes the list of Wheres anew"
    );
}

/// The recursion pair: a call into a function the route is already inside takes what its call hands it, and no value a body
/// makes comes from beyond the route (#237); the shortcut's `const FLOOR` alone does, as in Python and Rust.
#[test]
fn a_recursive_call_takes_what_its_call_hands_it() {
    common::a_recursive_call_takes_what_its_call_hands_it("typescript");
}

/// The literals pair: `[]` and `new Array<Item>()` make the same list, the body's (#237), as `vec![]` and `Vec::new()` do in
/// Rust.
#[test]
fn a_list_made_by_a_literal_is_made_by_the_body() {
    common::a_list_made_by_a_literal_is_made_by_the_body("typescript", "byLiteral", "byCall", "byConstant");
}

/// The workbench pair, `workbench/typescript`: what the program makes, kept in a folder of the project's
/// own. The clean side gives the folder a class whose method names each file in it
/// (`Workbench.of(project).file("report.txt")`); the shortcut hands the folder around as a bare path (`join(madeDir(project), "report.txt")`).
/// Isomorphic is the same: a path a class's method builds from the field its constructor sets is the
/// path written out with bare paths, so the two sides name the same files, at the same places, fixed
/// by the same function. The method is followed on the value another call gives back and on a
/// binding of it, and a static method called on its class is followed as a function is.
#[test]
fn places_a_path_a_class_builds_is_the_path_written_out() {
    let read = |side: &str| {
        analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/workbench/typescript").join(side), &Options::default())
            .unwrap()
    };
    let files = |g: &Graph| -> Vec<(String, String, Vec<String>)> {
        let mut out: Vec<(String, String, Vec<String>)> = ["keep.saveReport", "keep.saveLog"]
            .iter()
            .flat_map(|f| touches(g, f, "disk_write").into_iter().map(move |t| (f.to_string(), t.artifact.clone(), t.fixed_by.clone())))
            .collect();
        out.sort();
        out
    };
    let (c, s) = (files(&read("clean")), files(&read("shortcut")));
    assert_eq!(c, s);
    assert!(c.iter().any(|(f, a, by)| f == "keep.saveReport" && a == ".made/report.txt" && by == &["bench.cli.main"]), "{c:?}");
}
