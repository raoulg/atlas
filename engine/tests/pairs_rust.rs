//! The Rust pair: one small program, `spend`, written twice. `clean` is what a reviewer approves
//! on its own merits (clap's derived subcommands, serde-typed settings and responses, a trait for
//! the report's formats, parameterised SQL, places named once in `main`, a typed error); `shortcut`
//! is the same program as it is written under time pressure. Each side is read as a repository of
//! its own, so a node has the same id on both sides where the shortcut keeps the shape.
//!
//! Each test is named after a general principle (`docs/reference/principles.md`, or the shared language in
//! `CLAUDE.md`) and states three things: the clean side reads as the algebra says, the shortcut
//! shows as a shortcut, and the shortcut is still in the graph. A statement Atlas does not meet
//! yet is a test of its own, ignored with what Atlas gets wrong: `cargo test --test pairs_rust --
//! --ignored` lists them.

mod common;

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
    let arms: Vec<(u32, u32)> = ["spend.expense.read_csv", "spend.rates.RatesClient.latest", "spend.report.write"]
        .iter()
        .map(|t| step(c, "spend.main", t).arms[0])
        .collect();
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
/// the sloppy form keeps them in a global (`static LATEST: Mutex<..>`) that `remember` fills: the
/// place `Global(spend.rates.LATEST)`, which `remember` writes and `to_base` reads. Drawn, not
/// asked: a global is sometimes the right choice.
#[test]
fn nothing_a_call_leaves_something_behind() {
    let c = clean();
    assert_eq!(found(c, "nothing"), Vec::<String>::new());
    assert!(step(c, "spend.main", "spend.report.lines").hands.iter().any(|h| h.ty == "spend.rates.Rates"));
    assert!(c.nodes.iter().all(|n| n.touches.iter().all(|t| t.kind_name() != "global")), "the clean side keeps nothing in a global");

    let s = shortcut();
    step(s, "spend.run", "spend.rates.remember");
    let global =
        |f: &str| touches(s, f, "global").iter().filter(|t| t.artifact == "spend.rates.LATEST").map(|t| t.dir_name()).collect::<Vec<_>>();
    assert_eq!(global("spend.rates.remember"), ["write"]);
    assert_eq!(global("spend.rates.to_base"), ["read"]);
    assert!(found(s, "nothing").is_empty(), "a call that keeps its value in a global leaves it behind: {:?}", found(s, "nothing"));
    assert_eq!(
        node(s, "spend.rates.to_base").params.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
        ["currency"],
        "the rates come from the global"
    );
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
    let takes_result: Vec<String> = c
        .nodes
        .iter()
        .flat_map(|n| n.params.iter().filter(|p| p.ty.starts_with("Result")).map(move |p| format!("{}({})", n.id, p.name)))
        .collect();
    assert_eq!(takes_result, Vec::<String>::new());

    let s = shortcut();
    let save = node(s, "spend.store.Store.save_rates");
    assert!(save.params.iter().any(|p| p.name == "fetched" && p.ty.starts_with("Result<")), "{:?}", save.params);
    let fetched = gives(step(s, "spend.run", "spend.rates.fetch"));
    let handed = &step(s, "spend.run", "spend.store.Store.save_rates").hands;
    assert!(handed.iter().any(|h| fetched.contains(&h.ty.as_str())), "the store is handed what the request gives: {handed:?}");
}

/// A sum type is unwrapped where it arises: a function that takes a `Result` has received a
/// choice its caller should have made (docs/reference/principles.md, Rust).
#[test]
fn sums_a_result_handed_on_is_asked() {
    assert!(!found(shortcut(), "sums").is_empty());
}

/// One side of the collection pair (`tests/pairs/collections`): a store that reads its rows,
/// pushes one and writes them back, in Python, Rust and TypeScript alike.
fn gathering(side: &str) -> Graph {
    analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/collections/rust").join(side), &Options::default()).expect("reads")
}

/// What a call changes in place, it gives: `rows.push(Row { .. })` on a `Vec` is a step that makes
/// `rows` from what it is handed (std's row of the mutators), so the Reading reaches what
/// `write_rows(&self.path, &rows)` puts, at the place `get` reads: the path the store is made with,
/// `Disk(Store.path)`. The clean `get` gives the Readings back; the shortcut's gives the lines, and
/// `lens` asks where the Reading went. `DroppedStore` pushes onto another `Vec`: it puts no
/// Reading and is not asked, also where it takes `readings: &[Reading]` and pushes in a loop
/// (`put_all`): `reading` is an element of the slice, and the parameter is followed through it. The program's own `Frozen::push` returns a new value, and
/// `path.exists()` only looks: neither changes what it is called on. The file a write names is
/// its path (`&self.path`), not the `"\n"` it joins the lines with.
#[test]
fn lens_what_is_pushed_onto_what_is_put_is_put() {
    let (c, s) = (gathering("clean"), gathering("shortcut"));
    let fs = check(&c);
    assert!(fs.is_empty(), "{:#?}", fs.iter().map(|f| format!("{} {} {}", f.principle, f.node, f.what)).collect::<Vec<_>>());
    let put = node(&s, "gathering.Store.put");
    assert!(
        put.sites.iter().any(|x| x.op == "push" && x.hands.iter().any(|h| h.name == "rows") && x.gives.iter().any(|g| g.name == "rows")),
        "{:?}",
        put.sites
    );
    assert!(
        node(&s, "gathering.write_rows").touches.iter().any(|t| t.what == "Store.path"),
        "{:?}",
        node(&s, "gathering.write_rows").touches
    );
    assert!(node(&s, "gathering.Store.get").touches.iter().any(|t| t.what == "Store.path"));
    let element = |h: &atlas_engine::Held| h.name == "reading" && h.ty == "gathering.Reading";
    assert!(
        node(&s, "gathering.Store.put_all").sites.iter().any(|x| x.hands.iter().any(element)),
        "{:?}",
        node(&s, "gathering.Store.put_all").sites
    );
    assert_eq!(found(&s, "lens"), ["gathering.Store"]);
    let observe = node(&s, "gathering.observe");
    assert!(observe.sites.iter().flat_map(|x| &x.gives).all(|g| g.name != "frozen" && g.name != "path"), "{:?}", observe.sites);
}

/// One side of the sum pair (`tests/pairs/sums`): a knowledge base that adds a source and gives
/// back the graph of its nodes, in Python, Rust and TypeScript alike.
fn kennis(side: &str) -> Graph {
    analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/sums/rust").join(side), &Options::default()).expect("reads")
}

/// What a lens puts may come back as a variant of a sum a get gives. Rust names its sums: `enum
/// Node { Begrip(Begrip), Bron(Bron) }` in `Graaf.nodes: Vec<Node>` holds a Bron. The clean store
/// gives the `Graaf` back and is asked nothing; the shortcut gives lines, and `lens` asks.
#[test]
fn lens_what_is_put_comes_back_as_a_variant_of_a_named_sum() {
    let (c, s) = (kennis("clean"), kennis("shortcut"));
    let fs = check(&c);
    assert!(fs.is_empty(), "{:#?}", fs.iter().map(|f| format!("{} {} {}", f.principle, f.node, f.what)).collect::<Vec<_>>());
    assert!(edge(&c, EdgeKind::Has, "kennis.Graaf", "kennis.Node"));
    assert!(edge(&c, EdgeKind::Variant, "kennis.Node", "kennis.Bron"));
    assert_eq!(found(&s, "lens"), ["kennis.Kennisbank"]);
}

/// One side of the lens pair (`tests/pairs/lens`): the Python pair's store of readings, outages
/// and summaries, as Rust and TypeScript write it.
fn harvest(side: &str) -> Graph {
    analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/lens/rust").join(side), &Options::default()).expect("reads")
}

/// What a lens puts, it gets back, also inside what a get gives; and what it puts is what reaches
/// the place, not the key it looks a value up by. Both stores keep the outages of a harvest and a
/// summary per day. The clean store gives the outages back inside a station's `Harvest`, and its
/// `summary(day)` hands the Day to `kept_summary`, which reads, and to `day_summary`, whose Summary
/// it hands to `keep_summary` to put: the Day is the key the summary is kept under. It is asked
/// nothing. The shortcut's `outages` gives lines, and its `summary` writes the file itself: it is
/// asked about the Outage and the Day.
#[test]
fn lens_what_is_put_comes_back_inside_and_a_key_is_no_put() {
    let (c, s) = (harvest("clean"), harvest("shortcut"));
    let fs = check(&c);
    assert!(fs.is_empty(), "{:#?}", fs.iter().map(|f| format!("{} {} {}", f.principle, f.node, f.what)).collect::<Vec<_>>());
    assert!(edge(&c, EdgeKind::Has, "harvest.Harvest", "harvest.Outage"));
    assert!(edge(&c, EdgeKind::Calls, "harvest.ReadingStore.summary", "harvest.ReadingStore.keep_summary"));
    let lens: Vec<String> = check(&s).into_iter().filter(|f| f.principle == "lens").map(|f| f.what).collect();
    assert_eq!(lens.len(), 1, "{lens:?}");
    assert!(lens[0].contains("Outage") && lens[0].contains("Day") && !lens[0].contains("Reading "), "{}", lens[0]);
}

// ---------- prophecy ----------

/// One side of the prophecy pair (`tests/pairs/prophecy`): a report `Sink` that only writes, and
/// the `Store` that will replace it, sketched without bodies.
fn archive_rust(side: &str) -> Graph {
    analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/prophecy/rust").join(side), &Options::default()).expect("reads")
}

/// Sketch code kept in the tree says it is not built: Atlas reads a struct marked with the `#[doc = "prophecy: .."]` attribute
/// as a prophecy that supersedes the sink, and the sink as built. The shortcut writes the same
/// sketch without the mark: Atlas cannot tell it from code that is finished, and does not pretend to.
#[test]
fn prophecy_a_sketch_in_the_tree_says_it_is_not_built() {
    let (c, s) = (archive_rust("clean"), archive_rust("unmarked"));
    let store = |g: &Graph| {
        g.nodes
            .iter()
            .find(|n| n.id.ends_with(".Store") || n.id == "Store")
            .unwrap_or_else(|| panic!("a Store in {:?}", g.nodes.iter().map(|n| &n.id).collect::<Vec<_>>()))
            .clone()
    };
    let sketch = store(&c).prophecy.expect("the marked sketch is a prophecy");
    assert_eq!(sketch.supersedes.as_deref(), Some("archive::Sink"));
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
    let (c, s, u) = (archive_rust("clean"), archive_rust("shortcut"), archive_rust("unmarked"));
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
    let (c, s, u) = (archive_rust("clean"), archive_rust("shortcut"), archive_rust("unmarked"));
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
    let (decided, stands) = common::decided("rust", "src/lib.rs", "\n/// Where reports wait.\npub struct Vault;\n\nimpl Vault {\n    pub fn keep(&self, _report: &str) {\n        todo!()\n    }\n}\n", &[("archive.Vault", "archive.Sink")]);
    let text = decided.files.values().next().expect("one file is written");
    assert!(text.contains("#[doc = \"prophecy: supersedes=archive.Sink\"]"), "{text}");
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
    let (copy, g) = common::written("rust", "src/lib.rs", "\n#[doc = \"prophecy: supersedes=archive.Sink sealed=#7\"]\npub struct Vault;\n\n#[doc = \"prophecy: sealed=#8\"]\npub struct Crate;\n");
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

/// One side of the overrides pair (`tests/pairs/overrides`): books on a shelf, named and keyed.
fn books(side: &str) -> Graph {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/overrides/rust").join(side);
    analyze(&dir, &Options::default()).expect("reads")
}

/// An override keeps the promise of its base, and Rust cannot write the careless form: the compiler
/// holds a trait's implementation to the trait's signature, and the language says so
/// (`Leaves::overrides`). Neither side is asked: not the `Shelf.get` that gives the `Book` its
/// `Store::Item` names, written out on the clean side and by name on the other, nor the one `Book.id`
/// the graph keeps of the two that `Named` and `Keyed` declare, whichever trait is implemented
/// first.
#[test]
fn overrides_a_trait_implementation_has_its_traits_signature() {
    for side in ["clean", "shortcut"] {
        let g = books(side);
        assert!(found(&g, "overrides").is_empty(), "{side}: {:?}", found(&g, "overrides"));
        assert!(g.parts.iter().all(|p| !p.leaves.overrides && !p.structural), "{:?}", g.parts);
        assert!(edge(&g, EdgeKind::Inherits, "books.Shelf", "books.Store"));
        assert!(edge(&g, EdgeKind::Inherits, "books.Book", "books.Named") && edge(&g, EdgeKind::Inherits, "books.Book", "books.Keyed"));
    }
}
