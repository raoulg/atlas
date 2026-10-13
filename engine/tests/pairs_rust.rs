//! The Rust pair: one small program, `spend`, written twice. `clean` is what a reviewer approves
//! on its own merits (clap's derived subcommands, serde-typed settings and responses, a trait for
//! the report's formats, parameterised SQL, places named once in `main`, a typed error); `shortcut`
//! is the same program as it is written under time pressure. Each side is read as a repository of
//! its own, so a node has the same id on both sides where the shortcut keeps the shape.
//!
//! Each test is named after a general principle (`docs/principles.md`, or the shared language in
//! `CLAUDE.md`) and states three things: the clean side reads as the algebra says, the shortcut
//! shows as a shortcut, and the shortcut is still in the graph. A statement Atlas does not meet
//! yet is a test of its own, ignored with what Atlas gets wrong: `cargo test --test pairs_rust --
//! --ignored` lists them.

use std::path::PathBuf;
use std::sync::OnceLock;

use atlas_engine::principles::check;
use atlas_engine::{analyze, EdgeKind, Graph, Node, Options, Site, Touch};

fn read(side: &str) -> Graph {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/rust").join(side);
    analyze(&dir, &Options::default()).expect("reads")
}

fn clean() -> &'static Graph {
    static G: OnceLock<Graph> = OnceLock::new();
    G.get_or_init(|| read("clean"))
}

fn shortcut() -> &'static Graph {
    static G: OnceLock<Graph> = OnceLock::new();
    G.get_or_init(|| read("shortcut"))
}

fn node<'g>(g: &'g Graph, id: &str) -> &'g Node {
    g.node(id).unwrap_or_else(|| panic!("no node {id}"))
}

/// The nodes a principle asks about.
fn found(g: &Graph, principle: &str) -> Vec<String> {
    check(g).into_iter().filter(|f| f.principle == principle).map(|f| f.node).collect()
}

fn edge(g: &Graph, kind: EdgeKind, src: &str, dst: &str) -> bool {
    g.edges.iter().any(|e| e.kind == kind && e.src == src && e.dst == dst)
}

fn targets<'g>(g: &'g Graph, kind: EdgeKind, src: &str) -> Vec<&'g str> {
    g.edges.iter().filter(|e| e.kind == kind && e.src == src).map(|e| e.dst.as_str()).collect()
}

/// The step of `from` that calls or builds `to`.
fn step<'g>(g: &'g Graph, from: &str, to: &str) -> &'g Site {
    node(g, from).sites.iter().find(|s| s.to == to).unwrap_or_else(|| panic!("{from} has no step to {to}"))
}

/// The types of the values a step gives back.
fn gives(s: &Site) -> Vec<&str> {
    s.gives.iter().map(|h| h.ty.as_str()).collect()
}

/// A function's touches of one kind (`disk_read`, `net`, `db_sqlite`, `process`).
fn touches<'g>(g: &'g Graph, f: &str, kind: &str) -> Vec<&'g Touch> {
    node(g, f).touches.iter().filter(|t| t.kind_name() == kind).collect()
}

/// Whether an id is one of the program's own types.
fn own_type(g: &Graph, id: &str) -> bool {
    g.node(id).map_or(false, |n| n.kind == atlas_engine::NodeKind::Class)
}

/// Every principle holds on the code a reviewer approves: `atlas check` has nothing to ask.
#[test]
fn every_principle_holds_on_the_clean_side() {
    let fs = check(clean());
    assert!(fs.is_empty(), "{:#?}", fs.iter().map(|f| format!("{} {}:{} {}", f.principle, f.file, f.line, f.what)).collect::<Vec<_>>());
}

// ---------- choices ----------

/// A choice is made once, in a place with a name. Rust's clean form is an `enum` with one `match`:
/// clap derives the subcommands as `Command`, and `main` matches it once. The sloppy form takes the
/// command line as strings and dispatches in a catch-all `run(kind: &str, args: &[String])` that
/// tests `kind` again further down.
#[test]
fn choices_a_command_is_chosen_once() {
    let c = clean();
    assert_eq!(found(c, "choices"), Vec::<String>::new());
    // one choice in `main`, one arm per command, and no name tested against a string
    let arms: Vec<(u32, u32)> = ["spend.expense.read_csv", "spend.rates.RatesClient.latest", "spend.report.write"].iter().map(|t| step(c, "spend.main", t).arms[0]).collect();
    assert!(arms.iter().all(|a| a.0 == arms[0].0), "{arms:?}");
    assert_eq!(arms.iter().map(|a| a.1).collect::<Vec<_>>(), [0, 1, 2]);
    assert!(node(c, "spend.main").compares.is_empty(), "{:?}", node(c, "spend.main").compares);

    let s = shortcut();
    let asked: Vec<_> = check(s).into_iter().filter(|f| f.principle == "choices" && f.node == "spend.run").collect();
    assert_eq!(asked.len(), 1, "{asked:#?}");
    assert!(asked[0].what.contains("`kind`") && asked[0].what.contains("\"report\""), "{}", asked[0].what);
    // the commands are still run
    for t in ["spend.expense.read_csv", "spend.rates.fetch", "spend.report.write"] {
        step(s, "spend.run", t);
    }
}

// ---------- zoom by base type ----------

/// Zoom is a quotient by base type: a family is a base type with its kinds, and closing it shows
/// the base alone. Rust's clean form is a trait with an implementation per kind (`Render` for
/// `Markdown` and `Csv`), handed on as `&dyn Render`. The sloppy form has no family: one function
/// takes the format's name and tests it wherever the kinds differ.
#[test]
fn zoom_a_family_is_a_base_type_with_its_kinds() {
    let c = clean();
    for kind in ["spend.report.Markdown", "spend.report.Csv"] {
        assert!(edge(c, EdgeKind::Inherits, kind, "spend.report.Render"), "{kind} is a Render");
    }
    assert!(edge(c, EdgeKind::Takes, "spend.report.write", "spend.report.Render"));
    assert!(edge(c, EdgeKind::Calls, "spend.report.write", "spend.report.Render.render"));
    assert!(!found(c, "choices").contains(&"spend.report.write".to_string()));

    let s = shortcut();
    let families: Vec<_> = s.edges.iter().filter(|e| e.kind == EdgeKind::Inherits).map(|e| (&e.src, &e.dst)).collect();
    assert_eq!(families, [], "the formats are no family");
    let asked: Vec<_> = check(s).into_iter().filter(|f| f.principle == "choices" && f.node == "spend.report.write").collect();
    assert!(asked.len() == 1 && asked[0].what.contains("`format`"), "{asked:#?}");
    // the report is still written
    assert!(!touches(s, "spend.report.write", "disk_write").is_empty());
}

/// Composition is call order: a method called on a value is a step of the route. In Rust, the
/// values a `match` arm binds (`Command::Report { format, open }`) have the types the variant
/// declares, so `format.renderer()` is a call of `Format::renderer`.
#[test]
fn composition_a_method_on_a_matched_value_is_a_step() {
    let c = clean();
    assert!(edge(c, EdgeKind::Calls, "spend.main", "spend.Format.renderer"), "{:?}", targets(c, EdgeKind::Calls, "spend.main"));
}

/// Zoom is a quotient by base type, and a route opens the family at the kinds it builds. In Rust a
/// unit struct is built by naming it: `Box::new(Markdown)` makes a `Markdown`.
#[test]
fn zoom_a_unit_struct_named_as_a_value_is_built() {
    let c = clean();
    let built = targets(c, EdgeKind::Instantiates, "spend.Format.renderer");
    assert_eq!(built, ["spend.report.Markdown", "spend.report.Csv"]);
}

// ---------- a place carries a what ----------

/// A place carries a what: a value in memory is a value of a type the program names. Rust's clean
/// form deserializes the settings into a struct (`Config`, serde); the sloppy form keeps them as a
/// `HashMap<String, String>` and looks each one up by its key where it is used.
#[test]
fn a_place_carries_a_what_settings_in_memory() {
    let c = clean();
    assert!(edge(c, EdgeKind::Returns, "spend.config.Config.load", "spend.config.Config"));
    assert_eq!(gives(step(c, "spend.main", "spend.config.Config.load")), ["spend.config.Config"]);

    let s = shortcut();
    assert_eq!(targets(s, EdgeKind::Returns, "spend.config.load"), Vec::<&str>::new(), "a map of strings is no type of the program");
    let load = step(s, "spend.run", "spend.config.load");
    assert_eq!(load.gives.iter().map(|h| (h.name.as_str(), h.ty.as_str())).collect::<Vec<_>>(), [("config", "")]);
    let handed = &step(s, "spend.run", "spend.rates.fetch").hands;
    assert!(handed.iter().any(|h| h.name == "config" && h.ty.is_empty()), "{handed:?}");
    // the settings are still read
    assert!(!touches(s, "spend.config.load", "disk_read").is_empty());
}

/// A place carries a what: data from the world becomes a value of the program's own type where it
/// comes in. Rust's clean form deserializes the rates API's answer into `Rates` (serde); the
/// sloppy form keeps the `serde_json::Value` and hands it on.
#[test]
fn a_place_carries_a_what_a_response_in_memory() {
    let c = clean();
    assert!(edge(c, EdgeKind::Returns, "spend.rates.RatesClient.latest", "spend.rates.Rates"));
    assert_eq!(gives(step(c, "spend.main", "spend.rates.RatesClient.latest")), ["spend.rates.Rates"]);

    let s = shortcut();
    let given = targets(s, EdgeKind::Returns, "spend.rates.fetch");
    assert_eq!(given, ["ext:serde_json.Value"]);
    assert!(given.iter().all(|t| !own_type(s, t)));
    assert_eq!(gives(step(s, "spend.run", "spend.rates.fetch")), ["ext:serde_json.Value"]);
    assert!(edge(s, EdgeKind::Takes, "spend.store.Store.save_rates", "ext:serde_json.Value"));
}

/// A place carries a what: a program a route runs is Process(command). Rust's clean form names the
/// program once, as a constant (`const VIEWER: &str = "open"`), and runs `Command::new(VIEWER)`.
#[test]
#[ignore = "Atlas (a): a program named by a constant is drawn as the constant's name, not the program"]
fn a_place_carries_a_what_a_program_named_by_a_constant() {
    let run = touches(clean(), "spend.main", "process");
    assert_eq!(run.iter().map(|t| t.what.as_str()).collect::<Vec<_>>(), ["open"]);
}

// ---------- gives ----------

/// A function says what it gives back, as a type a reader can follow. Rust's compiler asks every
/// function for its return type, so the sloppy form is a type that names no what: `split_amount`
/// gives `(String, String)` where the clean form's `Money::parse` gives a `Money`.
#[test]
fn gives_what_a_function_gives_has_a_type() {
    let c = clean();
    assert!(edge(c, EdgeKind::Returns, "spend.expense.Money.parse", "spend.expense.Money"));
    assert_eq!(gives(step(c, "spend.expense.Expense.parse", "spend.expense.Money.parse")), ["spend.expense.Money"]);

    let s = shortcut();
    assert_eq!(targets(s, EdgeKind::Returns, "spend.expense.split_amount"), Vec::<&str>::new());
    let split = step(s, "spend.expense.Expense.parse", "spend.expense.split_amount");
    assert!(split.gives.iter().all(|h| h.ty.is_empty()), "{:?}", split.gives);
    // the call is still a step, and its signature still says what it gives
    assert_eq!(node(s, "spend.expense.split_amount").returns.as_ref().map(|r| r.written.as_str()), Some("(String, String)"));
}

/// A function gives back what its signature says. In Rust, `Result<PathBuf, Error>` gives a
/// `PathBuf` (the path the report is written at) and fails with an `Error`.
#[test]
#[ignore = "Atlas (a): a step whose function gives `Result<T, E>` with T no type of the program gives E"]
fn gives_a_result_gives_its_value_not_its_error() {
    let write = step(clean(), "spend.main", "spend.report.write");
    assert!(write.gives.iter().all(|h| h.ty != "spend.error.Error"), "{:?}", write.gives);
}

// ---------- places ----------

/// Library code does not decide where data lives: the place is named where the program starts and
/// handed in. Rust's clean form names its places as constants in `main` (`CONFIG`, `REPORTS`) and
/// hands them on as `&Path`. The sloppy form reads an environment variable in each library function
/// and falls back to a default it names itself
/// (`env::var("SPEND_CONFIG").unwrap_or_else(|_| "spend.toml".to_string())`).
#[test]
fn places_where_data_lives_is_decided_where_the_program_starts() {
    let c = clean();
    assert_eq!(found(c, "places"), Vec::<String>::new());
    let settings = touches(c, "spend.config.Config.load", "disk_read");
    assert!(settings.iter().any(|t| t.artifact == "spend.toml" && !t.fixed && t.by == ["spend.main"]), "{settings:#?}");
    let report = touches(c, "spend.report.write", "disk_write");
    assert!(report.iter().all(|t| t.artifact == "reports/" && !t.fixed && t.by == ["spend.main"]), "{report:#?}");

    // the shortcut's settings are still read, at a place made whole in the library itself
    let s = shortcut();
    let settings = touches(s, "spend.config.load", "disk_read");
    assert!(!settings.is_empty() && settings.iter().all(|t| t.by.is_empty() && t.users_of.is_empty()), "{settings:#?}");
}

/// Library code does not decide where data lives. A library function that falls back to a path of
/// its own when an environment variable is not set decides where its data lives: the default is
/// its choice, and the variable a place the code does not name.
#[test]
fn places_a_library_that_picks_its_own_place_is_asked() {
    let asked = found(shortcut(), "places");
    for f in ["spend.config.load", "spend.report.write"] {
        assert!(asked.contains(&f.to_string()), "{f} picks its own place: {asked:?}");
    }
}

// ---------- a route ends at a visible place ----------

/// A route starts at an entry and ends at a visible place. Rust's clean form names the rates API
/// once in `main` (`RATES_API`) and builds the client with it; the sloppy form reads the address
/// from the settings map (`config["rates_url"]`), so the code does not say where the request goes.
#[test]
fn a_route_ends_at_a_visible_place() {
    let s = shortcut();
    let asked = touches(s, "spend.rates.fetch", "net");
    assert_eq!(asked.len(), 1, "{asked:#?}");
    assert!(asked[0].artifact.is_empty() && !asked[0].fixed, "an address the code does not say: {asked:#?}");
    step(s, "spend.run", "spend.rates.fetch");
}

/// A route starts at an entry and ends at a visible place: `main → RatesClient::latest` ends at the
/// rates API. In Rust the client and its address are fields of `RatesClient`, set by `new` from
/// what `main` hands it, and `latest` sends the request on `self.http`.
#[test]
fn a_route_ends_at_a_visible_place_through_a_client() {
    let asked = touches(clean(), "spend.rates.RatesClient.latest", "net");
    assert!(asked.iter().any(|t| t.artifact == "api.frankfurter.app"), "{asked:#?}");
}

// ---------- nothing ----------

/// A call leaves something behind: what it gives, what it changes in the open, or a place it
/// reaches. Rust's clean form hands the rates on as a value (`report::lines(&expenses, &rates)`);
/// the sloppy form keeps them in a global (`static LATEST: Mutex<..>`) that `remember` fills and
/// `to_base` reads.
#[test]
fn nothing_a_call_leaves_something_behind() {
    let c = clean();
    assert_eq!(found(c, "nothing"), Vec::<String>::new());
    assert!(step(c, "spend.main", "spend.report.lines").hands.iter().any(|h| h.ty == "spend.rates.Rates"));

    let s = shortcut();
    assert!(found(s, "nothing").contains(&"spend.rates.remember".to_string()), "{:?}", found(s, "nothing"));
    step(s, "spend.run", "spend.rates.remember");
    assert_eq!(node(s, "spend.rates.to_base").params.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["currency"], "the rates come from the global");
}

// ---------- layers ----------

/// The packages of a part depend on one another in one direction. Rust's clean form keeps `report`
/// and `store` side by side: `main` takes the expenses from the store and hands them to the
/// report. In the sloppy form the report reads the store itself, and the store borrows a rounding
/// helper that was first written in the report.
#[test]
fn layers_modules_depend_in_one_direction() {
    assert_eq!(found(clean(), "layers"), Vec::<String>::new());

    let s = shortcut();
    let asked: Vec<_> = check(s).into_iter().filter(|f| f.principle == "layers").collect();
    assert_eq!(asked.len(), 1, "{asked:#?}");
    assert!(asked[0].what.starts_with("report and store"), "{}", asked[0].what);
    assert!(edge(s, EdgeKind::Calls, "spend.report.write", "spend.store.Store.rows"));
}

/// Composition is call order, and a call is a link between the modules it joins. In Rust a call
/// written as a macro's argument (`params![.., round2(e.amount), ..]`, `format!("{}", f(x))`) runs
/// like any other.
#[test]
#[ignore = "Atlas (b): a call inside a macro's arguments is no step, so the store's call into the report is dropped"]
fn composition_a_call_in_a_macro_argument_is_a_step() {
    assert!(edge(shortcut(), EdgeKind::Calls, "spend.store.Store.save", "spend.report.round2"));
}

// ---------- lens ----------

/// What a lens puts, it gets back. Rust's clean form saves an `Expense` and reads back an
/// `Expense` (`Store::save(&[Expense])`, `Store::expenses() -> Vec<Expense>`); the sloppy form
/// saves expenses and reads the rows back as tuples (`Store::rows() -> Vec<(String, String, f64,
/// String)>`).
#[test]
fn lens_what_is_put_is_got_back() {
    let c = clean();
    assert!(edge(c, EdgeKind::Takes, "spend.store.Store.save", "spend.expense.Expense"));
    assert!(edge(c, EdgeKind::Returns, "spend.store.Store.expenses", "spend.expense.Expense"));

    let s = shortcut();
    assert!(edge(s, EdgeKind::Takes, "spend.store.Store.save", "spend.expense.Expense"));
    assert_eq!(targets(s, EdgeKind::Returns, "spend.store.Store.rows"), Vec::<&str>::new());
}

/// What a lens puts, it gets back, and both meet at the place: the store writes the `expenses`
/// table and reads it again. In Rust the store holds its `rusqlite::Connection` in a field and runs
/// each statement on `self.conn`.
#[test]
fn lens_a_store_touches_the_tables_it_runs_statements_on() {
    let c = clean();
    let at = |f: &str| touches(c, f, "db_sqlite").iter().map(|t| (t.artifact.clone(), t.dir_name())).collect::<Vec<_>>();
    assert!(at("spend.store.Store.save").contains(&("expenses".into(), "write")), "{:?}", at("spend.store.Store.save"));
    assert!(at("spend.store.Store.expenses").contains(&("expenses".into(), "read")), "{:?}", at("spend.store.Store.expenses"));
    assert!(at("spend.store.Store.save_rates").contains(&("rates".into(), "write")));
    assert!(at("spend.store.Store.rates").contains(&("rates".into(), "read")));
}

/// What a lens puts, it gets back: a store that saves an `Expense` and reads back tuples is asked
/// where its `Expense` went. The shortcut's store holds its connection as the clean one does.
#[test]
fn lens_a_store_that_gets_back_rows_is_asked() {
    let s = shortcut();
    assert!(!touches(s, "spend.store.Store.save", "db_sqlite").is_empty(), "the shortcut's insert is in the graph");
    assert!(found(s, "lens").contains(&"spend.store.Store".to_string()), "{:?}", found(s, "lens"));
}

// ---------- sums ----------

/// A sum type is unwrapped where it arises. Rust's clean form opens a `Result` with `?` where the
/// call returns it: `main` takes the `Rates` out of `RatesClient::latest`. The sloppy form hands
/// the `Result` of the request on to the store, whose `save_rates` matches on it.
#[test]
fn sums_a_choice_is_made_where_it_arises() {
    let c = clean();
    let takes_result: Vec<String> = c.nodes.iter().flat_map(|n| n.params.iter().filter(|p| p.ty.starts_with("Result")).map(move |p| format!("{}({})", n.id, p.name))).collect();
    assert_eq!(takes_result, Vec::<String>::new());

    let s = shortcut();
    let save = node(s, "spend.store.Store.save_rates");
    assert!(save.params.iter().any(|p| p.name == "fetched" && p.ty.starts_with("Result<")), "{:?}", save.params);
    let fetched = gives(step(s, "spend.run", "spend.rates.fetch"));
    let handed = &step(s, "spend.run", "spend.store.Store.save_rates").hands;
    assert!(handed.iter().any(|h| fetched.contains(&h.ty.as_str())), "the store is handed what the request gives: {handed:?}");
}

/// A sum type is unwrapped where it arises: a function that takes a `Result` has received a
/// choice its caller should have made (docs/principles.md, Rust).
#[test]
#[ignore = "Atlas (b): a `Result` taken as an argument is no sum to the `sums` check, which reads only the program's own enums"]
fn sums_a_result_handed_on_is_asked() {
    assert!(!found(shortcut(), "sums").is_empty());
}
