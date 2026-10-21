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
    let record = asked_anywhere(s, "doors").into_iter().find(|f| f.starts_with("expenses.ledger.Ledger.record ")).unwrap_or_default();
    assert!(record.contains("`input`"), "{record}");
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
    assert_eq!(
        asked_anywhere(s, "gives"),
        ["expenses.rates.http.getJson getJson gives back a value its callers use, and does not say what it is"]
    );
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
/// shows as what it is: a place the code does not name, beside the default the library takes where
/// nobody sets the variable. Whoever starts the program may set it, so the default is an option,
/// as a parameter's default is, and `places` does not ask.
#[test]
fn places_decided_in_library_code_show() {
    let s = shortcut();
    for (f, kind) in
        [("rates.file.FileRates.latest", "disk_read"), ("exporters.csv.writeCsv", "disk_write"), ("exporters.json.writeJson", "disk_write")]
    {
        let unnamed = touches(s, f, kind).iter().any(|t| t.artifact.is_empty() && !t.fixed);
        assert!(unnamed && !asked(s, "places", f), "{f}: {:?}", places(s, f, kind));
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
        asked_anywhere(s, "nothing"),
        ["expenses.ledger.Ledger.exportReport Ledger.exportReport gives nothing back, and Atlas cannot see what `EXPORTERS[format](..)` does"],
        "a call that keeps its value in a global leaves it behind"
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
    assert!(layers[0].what.contains("file, http and source"), "{}", layers[0].what);
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
    let lens = asked_anywhere(&s, "lens");
    assert_eq!(lens.len(), 1, "{lens:?}");
    assert!(lens[0].contains("Outage") && lens[0].contains("Day") && !lens[0].contains("Reading "), "{}", lens[0]);
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
    assert!(asked_anywhere(&c, "sums").is_empty(), "one copairing is where the sum is taken apart");
    assert_eq!(
        asked_anywhere(&s, "sums"),
        [format!("({r}.model.Failed | {r}.model.Fit) Fit | Failed is handed on unopened: points and line take it", r = s.root)]
    );
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
    assert_eq!(message, [&[(0, 0)][..], &[(0, 1)][..]], "one constructor in each arm of one choice");

    let asked: Vec<Finding> = findings(&s).into_iter().filter(|f| f.principle == "choices").collect();
    let at = |id: &str| {
        let id = format!("{}.{id}", s.root);
        asked.iter().find(|f| f.node == id).unwrap_or_else(|| panic!("{id} is asked: {asked:#?}"))
    };
    assert_eq!(asked.len(), 5, "{asked:#?}");
    assert_eq!(at("runs.Runner").what, "Runner.batch and Runner.workers test `this.mode` in 2 places; \"parallel\" (2×) come back");
    assert_eq!(at("runs.Report.line").evidence, ["line 41: l (this.level) against \"loud\"", "line 41: l (this.level) against \"loud\""]);
    assert_eq!(at("runs.retries").what, "retries and timeout test `mode` in 2 places; \"parallel\" (2×) come back");
    assert_eq!(at("runs.prefix").what, "prefix and suffix test `level` in 2 places; Level.Loud (2×) come back");
    assert!(at("runs.run").what.contains("(load | infer)"), "{}", at("runs.run").what);
    // the shortcut still runs the job and sends its line
    assert!(edge(&s, EdgeKind::Calls, "runs.run", "runs.send") && edge(&s, EdgeKind::Calls, "runs.run", "runs.load"));
}

/// One side of the lens pair's club (`tests/pairs/lens/typescript/club`): the Python club's desk,
/// which keeps visits, members and reports in SQLite, a table each, and its log, roster and board
/// in files, as TypeScript writes them.
fn club(side: &str) -> &'static Graph {
    static CLEAN: OnceLock<Graph> = OnceLock::new();
    static SHORTCUT: OnceLock<Graph> = OnceLock::new();
    let read = || {
        analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/lens/typescript/club").join(side), &Options::default())
            .expect("reads")
    };
    if side == "clean" {
        CLEAN.get_or_init(read)
    } else {
        SHORTCUT.get_or_init(read)
    }
}

/// What `lens` says of class `id` of the club: empty when it asks nothing.
fn lens_of(g: &Graph, id: &str) -> String {
    findings(g).into_iter().find(|f| f.principle == "lens" && f.node == format!("{}.{id}", g.root)).map(|f| f.what).unwrap_or_default()
}

/// What a lens puts, it gets back, at the table it puts to; and a key its statement finds rows by
/// is no value put. The clean `Desk` only writes `visits` and only reads `members`: no lens on
/// either. Its `Register` gets the Member back from `members`, and its `Visits.forget` deletes a
/// Member's visits by the Member, a key. The shortcut's `Desk` reads `visits` back as records, its
/// `Register` reads back only names (the Member inside a Report comes from `reports`), and its
/// `Visits.come` inserts a Member's name as a value: each is asked.
#[test]
fn lens_the_place_is_the_table_and_a_key_is_no_put() {
    let (c, s) = (club("clean"), club("shortcut"));
    assert!(findings(c).is_empty(), "{:?}", findings(c).iter().map(|f| format!("{} {}", f.node, f.what)).collect::<Vec<_>>());
    let at = |f: &str| touches(c, f, "db_sqlite").iter().map(|t| (t.artifact.clone(), t.dir_name())).collect::<Vec<_>>();
    assert_eq!(at("desk.Desk.members"), [("members".to_string(), "read")]);
    assert_eq!(at("desk.Visits.forget"), [("visits".to_string(), "write")]);
    assert!(edge(c, EdgeKind::Takes, "desk.Visits.forget", "model.Member"));
    assert_eq!(lens_of(s, "desk.Desk"), "Desk puts Visit into SQLite(visits) and gets no Visit back");
    assert_eq!(lens_of(s, "desk.Register"), "Register puts Member into SQLite(members) and gets no Member back");
    assert_eq!(lens_of(s, "desk.Visits"), "Visits puts Member into SQLite(visits) and gets no Member back");
}

/// A get gives a value back from the place; an operation that reads the place on its way to
/// writing it is a put. The clean `Log.add` reads the file and writes it back with a line more,
/// `void`: an end product, no lens. The shortcut's `Log` also gives the lines back, and is asked
/// where the Visit went.
#[test]
fn lens_a_get_gives_a_value_back() {
    let (c, s) = (club("clean"), club("shortcut"));
    let kinds: Vec<&str> = node(c, "files.Log.add").touches.iter().map(|t| t.kind_name()).collect();
    assert!(kinds.contains(&"disk_read") && kinds.contains(&"disk_write"), "{kinds:?}");
    assert_eq!(lens_of(c, "files.Log"), "");
    assert_eq!(lens_of(s, "files.Log"), "Log puts Visit into Disk(Log.path) and gets no Visit back");
}

/// What a get gives back holds what was put: the class's own type, which a constructor gives, and
/// a subclass of what a get gives, a variant of a sum left open. The clean `Roster.load` (static,
/// a constructor) and `reread` give a Roster, which holds the Members `add` puts; its `Board` pins
/// a Notice and gives the Posts back, and a Notice `extends` Post. The shortcut's `Roster.load`
/// gives a Roster that holds the lines, and its `Board` gives records back: each is asked.
#[test]
fn lens_a_get_gives_its_own_class_and_a_subclass_back() {
    let (c, s) = (club("clean"), club("shortcut"));
    assert!(node(c, "files.Roster.load").ctor);
    assert!(edge(c, EdgeKind::Has, "files.Roster", "model.Member"));
    assert!(edge(c, EdgeKind::Inherits, "model.Notice", "model.Post"));
    assert_eq!(lens_of(c, "files.Roster"), "");
    assert_eq!(lens_of(c, "files.Board"), "");
    assert_eq!(lens_of(s, "files.Roster"), "Roster puts Member into Disk(Roster.path) and gets no Member back");
    assert_eq!(lens_of(s, "files.Board"), "Board puts Notice into Disk(Board.path) and gets no Notice back");
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
    assert_eq!(
        asked_anywhere(&s, "overrides"),
        ["events.handlers.Handler.handle Handler.handle promises (e: Event) -> string; 1 override breaks it"]
    );
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
    assert_eq!(asked_anywhere(&s, "layers"), ["cyc.orders orders itself and lines depend on one another"]);
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
        assert!(!asked(&g, "layers", "money") && asked_anywhere(&g, "layers").iter().all(|f| !f.contains("money")), "{side}");
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
    assert_eq!(asked_anywhere(&s, "prophecy"), ["archive.check.sealedTwice sealedTwice leans on `seal`, which is not built"]);
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
    assert_eq!(
        asked_anywhere(&s, "prophecy"),
        ["archive.savePosted savePosted supersedes `save`, which names 2 nodes: `archive.ledger.save` and `archive.shelf.save`"]
    );
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
    assert_eq!(scale.overloads.iter().map(|o| o.line).collect::<Vec<_>>(), [22, 23]);
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
    let want = [("core.Length.meters", 9), ("core.scale", 23), ("core.log", 28), ("core.parse", 32)];
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
    let asked = |g: &Graph| -> Vec<(String, String)> {
        check(g)
            .into_iter()
            .filter(|f| f.principle == "nothing")
            .map(|f| (f.node.replacen(&format!("{}.", g.root), "", 1), f.what))
            .collect()
    };
    assert!(asked(&c).is_empty(), "{:#?}", asked(&c));
    assert!(node(&c, "runner.stop").raises, "ending the run is what `stop` leaves behind");
    let want = [
        ("runner.runTask", "runTask gives nothing back, and Atlas cannot see what `Task.run` does"),
        ("runner.Job.finish", "Job.finish gives nothing back, and Atlas cannot see what `this.onDone(..)` does"),
        ("runner.count", "count gives nothing back, and Atlas cannot see what `Counter.runs += 1` does"),
        ("runner.settle", "settle gives nothing back, and Atlas sees no change and no place it reaches"),
    ]
    .map(|(f, what)| (f.to_string(), what.to_string()));
    assert_eq!(asked(&s), want);
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
/// (`process.env.STOCK_PATH ?? "out/rows.csv"`). The variable draws as a place the code does not
/// name, beside the file the code takes where nobody sets it; `places` asks neither form.
#[test]
fn places_a_default_the_caller_can_override_is_an_option() {
    let c = stock("clean");
    assert!(!asked(&c, "places", "rows.save") && !asked(&c, "places", "rows.saveWhereTold"), "{:?}", asked_anywhere(&c, "places"));
    let told = touches(&c, "rows.saveWhereTold", "disk_write");
    assert!(told.iter().any(|t| t.artifact == "out/rows.csv" && !t.fixed && t.fixed_by.is_empty()), "{told:?}");
    assert!(told.iter().any(|t| t.artifact.is_empty() && t.what == "{$STOCK_PATH}"), "{told:?}");
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
        let r = atlas_engine::story::route(&g, "headlines", 4).expect("a route");
        assert!(!r.contains("one of") && !r.contains("prototype"), "{side}: {r}");
        assert_eq!(r.contains("Store.get(key)"), side == "clean", "{side}: {r}");
    }
}
