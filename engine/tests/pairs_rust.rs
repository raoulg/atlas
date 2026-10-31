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

use common::{by, Pair};

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::OnceLock;

use atlas_engine::principles::{check, Finding};
use atlas_engine::{analyze, Compare, EdgeKind, Graph, Node, Options, Site, Touch};

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

/// The ids a `layers` finding's links run from and to.
fn links(f: &Finding) -> Vec<&str> {
    f.about("from").into_iter().chain(f.about("to")).collect()
}

/// Whether `id` is block `block` or stands inside it.
fn within(id: &str, block: &str) -> bool {
    id == block || id.strip_prefix(block).is_some_and(|rest| rest.starts_with('.'))
}

/// The comparisons of `f` that test `subject` against `case`, a literal or a member of a type.
fn tests_of<'g>(g: &'g Graph, f: &str, subject: &str, case: &str) -> Vec<&'g Compare> {
    node(g, f).compares.iter().filter(|c| c.subject == subject && (c.cases.iter().chain(&c.members).any(|x| x == case))).collect()
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
    // one choice in `main`, one arm per command, and no name tested against a string: the one
    // `match` names each variant once (`Command::Rates`, the variant without fields, as a case)
    let arms: Vec<(u32, u32)> = ["spend.expense.read_csv", "spend.rates.RatesClient.latest", "spend.report.write"]
        .iter()
        .map(|t| step(c, "spend.main", t).arms[0])
        .collect();
    assert!(arms.iter().all(|a| a.0 == arms[0].0), "{arms:?}");
    assert_eq!(arms.iter().map(|a| a.1).collect::<Vec<_>>(), [0, 1, 2]);
    let compares = &node(c, "spend.main").compares;
    assert!(compares.len() == 1 && compares[0].cases.is_empty() && compares[0].members == ["Command::Rates"], "{compares:?}");

    let s = shortcut();
    let asked: Vec<_> = check(s).into_iter().filter(|f| f.principle == "choices" && f.node == "spend.run").collect();
    assert_eq!(asked.len(), 1, "{asked:#?}");
    // `kind` is tested against "report" in two places
    assert_eq!((asked[0].about("subject"), asked[0].about("case")), (vec!["kind"], vec!["\"report\""]), "{asked:#?}");
    assert_eq!(tests_of(s, "spend.run", "kind", "report").len(), 2, "{:?}", node(s, "spend.run").compares);
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
    assert!(asked.len() == 1 && asked[0].about("subject") == ["format"], "{asked:#?}");
    assert_eq!(tests_of(s, "spend.report.write", "format", "csv").len(), 2, "{:?}", node(s, "spend.report.write").compares);
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
    assert!(settings.iter().any(|t| t.artifact == "spend.toml" && !t.fixed && t.by == by(["spend.main"])), "{settings:#?}");
    let report = touches(c, "spend.report.write", "disk_write");
    assert!(report.iter().all(|t| t.artifact == "reports/" && !t.fixed && t.by == by(["spend.main"])), "{report:#?}");

    // the shortcut's settings are still read, at a place made whole in the library itself
    let s = shortcut();
    let settings = touches(s, "spend.config.load", "disk_read");
    assert!(!settings.is_empty() && settings.iter().all(|t| t.by.is_empty()), "{settings:#?}");
}

/// Library code does not decide where data lives. A library function that falls back to a path of
/// its own when an environment variable is not set leaves the place to whoever starts the program:
/// the variable with its default is one input, an option, as a parameter's default is, and one
/// place, drawn as its default. `places` does not ask; the shortcut shows in where the place is whole: in the
/// library itself, while the clean side's is whole in `main`.
#[test]
fn places_an_environment_variable_with_a_default_is_an_option() {
    let s = shortcut();
    let asked = found(s, "places");
    for (f, kind, default, variable) in [
        ("spend.config.load", "disk_read", "spend.toml", "{$SPEND_CONFIG}"),
        ("spend.report.write", "disk_write", "reports/", "{$SPEND_REPORTS}"),
    ] {
        assert!(!asked.contains(&f.to_string()), "{f} takes what the environment says: {asked:?}");
        let ts = touches(s, f, kind);
        assert!(ts.iter().any(|t| t.artifact == default && !t.fixed && t.fixed_by.is_empty()), "{f}: {ts:#?}");
        let defaults = ts.iter().all(|t| t.artifact.starts_with(default.trim_end_matches('/')) && !t.fixed);
        assert!(defaults, "{f} draws its default, {variable} its input, and no place beside it: {ts:#?}");
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
    // the cycle is report and store: each link in it joins the two
    let cycle = ["spend.report", "spend.store"];
    assert_eq!(asked[0].about("block"), cycle, "{asked:#?}");
    assert!(links(&asked[0]).iter().all(|id| cycle.iter().any(|b| within(id, b))), "{asked:#?}");
    for block in cycle {
        assert!(asked[0].about("from").iter().any(|id| within(id, block)), "a link runs from {block}: {asked:#?}");
    }
    assert!(edge(s, EdgeKind::Calls, "spend.report.write", "spend.store.Store.rows"));
    assert!(edge(s, EdgeKind::Calls, "spend.store.Store.save", "spend.report.round2"));
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

/// A sum taken apart once is no repeated choice: the shortcut's `Store.save_rates` takes the
/// `Result` the request gives and opens it, the one copairing of that sum, so `sums` does not ask.
/// Two takers that each choose again are asked (`sums_a_variant_with_fields_is_a_variant`).
#[test]
fn sums_a_result_taken_apart_once_is_not_asked() {
    let takers: Vec<&str> =
        shortcut().edges.iter().filter(|e| e.kind == EdgeKind::Takes && e.dst.starts_with("ext:Result<")).map(|e| e.src.as_str()).collect();
    assert_eq!(takers, ["spend.store.Store.save_rates"]);
    assert!(found(shortcut(), "sums").is_empty());
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
    assert_eq!(found(&s, "lens"), ["harvest.ReadingStore"]);
    let (types, _) = lens_of(&s, "harvest.ReadingStore").unwrap();
    assert_eq!(types, ["harvest.Day", "harvest.Outage"], "the Outage and the Day, not the Reading");
}

/// What follows an early return runs only where the guard's arm does not. The clean `summary`
/// writes its choice as a `match` whose `None` arm reads the history; the shortcut returns the
/// kept line from an `if let` and reads the history after it. Isomorphic is the same: on both
/// sides the history is read in the second arm of the one choice `summary` makes, and the
/// shortcut's says it runs otherwise.
#[test]
fn choices_what_follows_an_early_return_is_the_other_arm() {
    for side in ["clean", "shortcut"] {
        let g = harvest(side);
        let summary = node(&g, "harvest.ReadingStore.summary");
        assert_eq!(step(&g, "harvest.ReadingStore.summary", "harvest.ReadingStore.history").arms, [(0, 1)], "{side}");
        if side == "shortcut" {
            assert!(summary.whens.contains(&(0, 1, "otherwise".to_string())), "{:?}", summary.whens);
        }
    }
}

/// A library's operation is a step where the body names what it gives or changes the value it runs
/// on, and of a chain only the last: `fs::read_to_string(p)?` names its text, `text.push_str(..)`
/// changes the String a `let mut` holds, and `con.prepare(..)?.query_map(..)?.collect::<..>()?`
/// is one step, `collect`, whatever its turbofish says. The calls before it in the chain are no
/// steps: what they give goes into it, and the instance the body runs on (`&self.path`) is handed
/// to the step that takes it.
#[test]
fn composition_a_named_result_or_a_change_is_a_library_step() {
    let s = harvest("shortcut");
    let ops = |f: &str| node(&s, f).sites.iter().filter(|x| !x.op.is_empty()).map(|x| (x.op.clone(), x.changes)).collect::<Vec<_>>();
    assert_eq!(ops("harvest.ReadingStore.save"), [("read_to_string".to_string(), false), ("push_str".to_string(), true)]);
    let read = node(&s, "harvest.ReadingStore.save").sites.iter().find(|x| x.op == "read_to_string").unwrap();
    assert!(read.hands.iter().any(|h| h.ty == "harvest.ReadingStore" && h.name.is_empty()), "{:?}", read.hands);
    let g = analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/readback/rust/shortcut"), &Options::default()).unwrap();
    let keep: Vec<(String, Vec<String>)> = node(&g, "ledger.Ledger.keep")
        .sites
        .iter()
        .filter(|x| !x.op.is_empty())
        .map(|x| (x.op.clone(), x.gives.iter().map(|h| h.name.clone()).collect()))
        .collect();
    assert!(keep.contains(&("collect".to_string(), vec!["stored".to_string()])), "{keep:?}");
    assert!(!keep.iter().any(|(op, _)| op == "prepare" || op == "query_map"), "{keep:?}");
}

/// A call is handed the values the names in its arguments hold. A name a pattern binds of a part
/// of another's value is that value (`Command::Report { format, .. }` of `cli.command` hands
/// `format.renderer()` the `Cli`; `Outcome::Failed { reason }` hands the Report built of it the
/// `Outcome`), also where a format string writes it (`format!("failed: {reason}")`); a call written
/// inside the arguments that is a step of its own takes its names itself (`level.prefix()` in the
/// Message `report` builds).
#[test]
fn composition_a_call_is_handed_what_the_names_in_its_arguments_hold() {
    let hands = |g: &Graph, f: &str, to: &str| -> Vec<String> {
        node(g, f).sites.iter().filter(|s| s.to == to).flat_map(|s| s.hands.iter().map(|h| format!("{}·{}", h.ty, h.name))).collect()
    };
    assert!(hands(clean(), "spend.main", "spend.Format.renderer").contains(&"spend.Cli·cli".to_string()));
    let f = fits("clean");
    let report = hands(&f, "fits.report.describe", "fits.report.Report");
    assert!(report.iter().filter(|x| *x == "fits.model.Outcome·o").count() >= 2, "both arms build of `o`: {report:?}");
    // what a closure is handed of what an iterator goes over is a part of it: `run` of
    // `series.windows(months).map(|run| Smoothed { .. })` is a window of the series
    let smooth = hands(&sunspots("rust-clean"), "sunspots.series.smooth", "sunspots.series.Smoothed");
    assert!(smooth.contains(&"sunspots.series.Month·series".to_string()), "{smooth:?}");
    let c = choices_side("clean");
    let message = hands(&c, "runs.report", "runs.Message");
    assert!(!message.iter().any(|h| h.starts_with("runs.Level")), "the level goes to `prefix`, not to Message: {message:?}");
    // a name written twice, once in a step written inside and once outside it, is handed by the one
    // outside: `self.history(&day.station)?.iter().filter(|r| r.date == day.date)..collect()`
    let summary = node(&harvest("shortcut"), "harvest.ReadingStore.summary").sites.iter().find(|s| s.op == "collect").unwrap().clone();
    assert!(handed(&summary).contains(&"harvest.Day·day".to_string()), "{:?}", summary.hands);
    // a parameter typed as the value a library's sum holds is that value: `fetched: Result<Value, reqwest::Error>`
    let rates = node(shortcut(), "spend.store.Store.save_rates").sites.iter().find(|s| s.op == "unwrap_or_default").unwrap();
    assert_eq!(handed(rates), ["ext:serde_json.Value·fetched"], "`body` of `Ok(body)` is a part of `fetched`");
}

/// What a step is handed, each value as `type·name`.
fn handed(s: &Site) -> Vec<String> {
    s.hands.iter().map(|h| format!("{}·{}", h.ty, h.name)).collect()
}

/// A parameter is a name, and holds what its call fills it with, of whatever type: a step handed
/// it is handed that. Where no call fills it with a value of the plan (a literal, or a function
/// no code of the program calls) it holds none. Both sides import the file the command line
/// names: `read_csv(&file)` and `read_csv(&args[0])` fill `path`, so the read inside is handed it.
/// The clean side names its settings' file once in `main`, `Config::load(Path::new(CONFIG))`: the
/// read of `path` there is handed nothing. The choices pair tells the same of a mode: the clean
/// `finished(&job, settings.mode)` hands `Mode::plan` a `Mode`, the shortcut's
/// `finished(&job, &settings.mode)` hands `Runner::new` and `timeout` the String `mode` holds,
/// and the route's own `name: &str`, which nothing in the program fills, is handed to no step.
#[test]
fn composition_a_parameter_holds_what_its_call_fills_it_with() {
    for g in [clean(), shortcut()] {
        let read = node(g, "spend.expense.read_csv").sites.iter().find(|s| !s.op.is_empty()).unwrap();
        assert!(handed(read).contains(&"·path".to_string()), "{:?}", read.hands);
    }
    let load = node(clean(), "spend.config.Config.load").sites.iter().find(|s| s.op == "read_to_string").unwrap();
    assert!(load.hands.is_empty(), "a literal fills `path`: {:?}", load.hands);
    let (c, s) = (choices_side("clean"), choices_side("shortcut"));
    assert!(node(&c, "runs.finished").sites.iter().any(|x| x.to == "runs.Mode.plan" && handed(x).contains(&"runs.Mode·mode".to_string())));
    for to in ["runs.Runner.new", "runs.timeout"] {
        assert_eq!(handed(step(&s, "runs.finished", to)), ["·mode"], "{to}");
    }
    for to in ["runs.load", "runs.infer"] {
        assert!(step(&s, "runs.run", to).hands.is_empty(), "the route's own `name` holds no value of the plan: {to}");
    }
}

/// A function a method hands each element of what it runs on is called there, as the closure
/// `|line| Expense::parse(line)` would call it: a step written where the function is named,
/// handed the element, a part of what the method runs on (`text` of `text.lines()..map(Expense::parse)`),
/// and what it gives goes into the step around it (isomorphic is the same). The clean knowledge
/// base builds its Graaf of the Nodes `map(node)` gives; the shortcut maps the library's
/// `str::to_string`, no step, and builds no Node.
#[test]
fn composition_a_function_handed_each_element_is_a_step_where_it_is_named() {
    for g in [clean(), shortcut()] {
        assert_eq!(handed(step(g, "spend.expense.read_csv", "spend.expense.Expense.parse")), ["·text"]);
    }
    let (c, s) = (kennis("clean"), kennis("shortcut"));
    let made = gives(step(&c, "kennis.Kennisbank.graaf", "kennis.node"));
    assert_eq!(made, ["kennis.Node"]);
    let graaf = step(&c, "kennis.Kennisbank.graaf", "kennis.Graaf");
    assert!(graaf.hands.iter().any(|h| h.ty == "kennis.Node"), "the Nodes go into `Graaf {{ nodes }}`: {:?}", graaf.hands);
    assert!(node(&s, "kennis.Kennisbank.graaf").sites.iter().all(|x| x.to.is_empty()), "{:?}", node(&s, "kennis.Kennisbank.graaf").sites);
}

/// A closure bound to a name makes its value wherever it is called or handed to a call that calls
/// it: `let wrong = || Error::Amount(..)` in the clean `Money::parse` hands the Error it builds to
/// `ok_or_else(wrong)` and to `map_err(|_| wrong())`. The shortcut's `split_amount` falls back on a
/// pair, `unwrap_or((text, "EUR"))`, and builds no error.
#[test]
fn composition_a_closure_bound_to_a_name_makes_its_value_where_it_is_handed() {
    let parse = node(clean(), "spend.expense.Money.parse");
    for op in ["ok_or_else", "map_err"] {
        let s = parse.sites.iter().find(|s| s.op == op).unwrap_or_else(|| panic!("no step {op}"));
        assert!(handed(s).contains(&"spend.error.Error·".to_string()), "{op}: {:?}", s.hands);
    }
    let split = node(shortcut(), "spend.expense.split_amount");
    assert!(split.sites.iter().all(|s| s.hands.iter().all(|h| !h.ty.ends_with("Error"))), "{:?}", split.sites);
}

/// A loop is a step that takes what it goes over: written over a step of the program's own
/// (`for (date, ..) in store.rows()` in the shortcut's `report::write`), it takes what that step
/// gives, the rows, not the store the step runs on. The clean report is handed its lines and goes
/// over them (`for l in lines`).
#[test]
fn composition_a_loop_over_a_step_takes_what_the_step_gives() {
    let write = node(shortcut(), "spend.report.write");
    let rows = step(shortcut(), "spend.report.write", "spend.store.Store.rows");
    let each = write.sites.iter().find(|s| s.op.starts_with("for ")).unwrap();
    assert_eq!(rows.gives.len(), 1, "{:?}", rows.gives);
    assert_eq!(each.hands, rows.gives, "the loop takes the rows `Store.rows` gives");
    assert!(each.hands.iter().all(|h| h.ty != "spend.store.Store"));
    let render = node(clean(), "spend.report.Csv.render").sites.iter().find(|s| s.op.starts_with("for ")).unwrap();
    assert_eq!(handed(render), ["spend.report.Line·lines"]);
}

/// A `let .. else` is a choice: its `else` runs when the pattern does not match and must leave,
/// so what follows it runs otherwise. `Expense::parse` builds the line's error in the `else` and
/// the expense after it: two arms of one choice.
#[test]
fn choices_a_let_else_is_a_guard() {
    let parse = "spend.expense.Expense.parse";
    let arms = |to: &str| step(clean(), parse, to).arms.clone();
    assert_eq!(arms("spend.expense.Money.parse"), [(0, 1)]);
    assert_eq!(arms("spend.expense.Expense"), [(0, 1)]);
    assert!(node(clean(), parse).sites.iter().any(|s| s.arms == [(0, 0)] && s.to.ends_with("Error")), "{:?}", node(clean(), parse).sites);
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

/// A vision is placed by the directory it is read from, so a vision that shares nothing with the
/// tree is placed as one that does: a crate with only its manifest, and a vision that writes its
/// first file. The file is written at the path the vision names it by, and what it adds is marked.
#[test]
fn prophecy_a_vision_of_new_files_only_is_placed_where_it_names_them() {
    use std::collections::BTreeMap;
    let copy = common::clean_copy("rust", "new-files");
    std::fs::remove_dir_all(copy.join("src")).expect("removes the code");
    let file = PathBuf::from("src/lib.rs");
    let vision = BTreeMap::from([(
        file.clone(),
        Some("pub struct Vault;\n\nimpl Vault {\n    pub fn keep(&self, _report: &str) {\n        todo!()\n    }\n}\n".to_string()),
    )]);
    let decided = atlas_engine::decide::prophetize(&copy, &vision, &BTreeMap::new(), &Options::default());
    let _ = std::fs::remove_dir_all(copy.parent().expect("has a parent"));
    let decided = decided.expect("a vision that only adds new files is decided");
    assert_eq!(decided.files.keys().collect::<Vec<_>>(), [&file]);
    assert_eq!(decided.marked, ["archive.Vault"]);
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

/// Breaking a seal changes the bytes of `sealed=..` and of the separator that joins it, and no
/// other: the file comes back as it was with exactly those bytes taken out, the seal first among
/// the attribute's arguments, last, or alone. The file is read when the seal is broken and the
/// graph may be older: a file emptied or shortened since is an error.
#[test]
fn prophecy_breaking_a_seal_changes_its_bytes_and_no_other() {
    let sealed = "//! Where reports wait.\n\n#[doc = \"prophecy: supersedes=archive.Sink sealed=#7\"]\npub struct Vault;\n\n#[doc = \"prophecy: sealed=#8 supersedes=archive.Sink\"]\npub struct Crate;\n\n#[doc = \"prophecy: sealed=#9\"]\npub struct Shelf;\n";
    let copy = common::clean_copy("rust", "unseal");
    let lib = std::fs::read_to_string(copy.join("src/lib.rs")).expect("reads");
    std::fs::write(copy.join("src/lib.rs"), format!("{lib}\npub mod vault;\n")).expect("writes");
    let file = copy.join("src/vault.rs");
    std::fs::write(&file, sealed).expect("writes");
    let g = analyze(&copy, &Options::default()).expect("reads");
    let ids = ["archive.vault.Vault", "archive.vault.Crate", "archive.vault.Shelf"].map(String::from);
    let broken = atlas_engine::decide::unseal(&g, &ids).expect("the seals are broken");
    let unsealed = sealed.replacen(" sealed=#7", "", 1).replacen("sealed=#8 ", "", 1).replacen(" sealed=#9", "", 1);
    let text = broken.get(std::path::Path::new("src/vault.rs")).cloned();
    let changed = ["", "//! Where reports wait.\n\n"].map(|cut| {
        std::fs::write(&file, cut).expect("writes");
        atlas_engine::decide::unseal(&g, &ids).is_err()
    });
    let _ = std::fs::remove_dir_all(copy.parent().expect("has a parent"));
    assert_eq!(text, Some(unsealed));
    assert_eq!(changed, [true, true], "an emptied file, a shortened one");
}

fn fits(side: &str) -> Graph {
    analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/sums/fits/rust").join(side), &Options::default()).expect("reads")
}

/// A sum is unwrapped where it arises, whatever its variants carry: `enum Outcome { Fit { score:
/// f64 }, Failed { reason: String } }` is a sum of two variants, each a type of its own, though
/// neither holds a type of the program. On the clean side one function takes it apart,
/// `describe`, and gives a line and the points to plot; the shortcut hands it to `line` and to
/// `points`, which each choose again.
#[test]
fn sums_a_variant_with_fields_is_a_variant() {
    let (c, s) = (fits("clean"), fits("shortcut"));
    for g in [&c, &s] {
        assert_eq!(targets(g, EdgeKind::Variant, "fits.model.Outcome"), ["fits.model.Outcome::Fit", "fits.model.Outcome::Failed"]);
        assert_eq!(node(g, "fits.model.Outcome::Fit").name, "Outcome::Fit");
        assert!(edge(g, EdgeKind::Returns, "fits.fitter.fit", "fits.model.Outcome"));
    }
    assert!(found(&c, "sums").is_empty(), "one copairing is where the sum is taken apart");
    assert!(edge(&s, EdgeKind::Takes, "fits.report.line", "fits.model.Outcome"));
    assert!(edge(&s, EdgeKind::Takes, "fits.plot.points", "fits.model.Outcome"));
    assert_eq!(found(&s, "sums"), ["fits.model.Outcome"]);
}

/// One side of the choices pair (`tests/pairs/choices/rust`): a job run in one of two modes, and a
/// line that says how it went.
fn choices_side(side: &str) -> Graph {
    analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/choices/rust").join(side), &Options::default()).expect("reads")
}

/// A choice is made once, in a place with a name, and Fowler's repeated switch is between
/// functions as well as inside one. Rust's clean form is an `enum` with methods that each `match`
/// the value they run on once (`Mode::plan`, `Level::prefix`, `Level::suffix`), and a function
/// whose body is where the job comes from (`source`); `report` builds its Message by one struct
/// expression in either arm, one step with a choice of what it is handed, and nothing is asked.
/// The shortcut tests `self.mode` in two methods of `Runner`, `mode` in two functions,
/// `settings.mode` twice through `let m = &settings.mode`, `level` against the variant
/// `Level::Loud` in two functions (`==` and `match`), and picks where its Job comes from (`load |
/// infer`) between the steps of `run`: each is asked once, where the value is defined.
#[test]
fn choices_a_choice_is_made_once_across_bodies() {
    let (c, s) = (choices_side("clean"), choices_side("shortcut"));
    assert_eq!(found(&c, "choices"), Vec::<String>::new());
    let message: Vec<&[(u32, u32)]> =
        node(&c, "runs.report").sites.iter().filter(|s| s.to == "runs.Message").map(|s| s.arms.as_slice()).collect();
    assert_eq!(message, [&[(0, 0)][..], &[(0, 1)][..]], "one struct expression in each arm of one choice");
    let level = &node(&c, "runs.Level.prefix").compares;
    assert!(level.len() == 1 && level[0].receiver && level[0].members == ["Level::Quiet", "Level::Loud"], "{level:?}");

    let asked: Vec<_> = check(&s).into_iter().filter(|f| f.principle == "choices").collect();
    let at = |id: &str| asked.iter().find(|f| f.node == id).unwrap_or_else(|| panic!("{id} is asked: {asked:#?}"));
    assert_eq!(
        asked.iter().map(|f| f.node.as_str()).collect::<Vec<_>>(),
        ["runs.Runner", "runs.retries", "runs.verbosity", "runs.prefix", "runs.run"]
    );
    // each finding is about the value tested and the bodies that test it against one case, and each of them tests it
    for (asked, value, case, bodies) in [
        ("runs.Runner", "self.mode", "parallel", &["runs.Runner.batch", "runs.Runner.workers"][..]),
        ("runs.retries", "mode", "parallel", &["runs.retries", "runs.timeout"]),
        ("runs.prefix", "level", "Level::Loud", &["runs.prefix", "runs.suffix"]),
    ] {
        assert_eq!((at(asked).about("subject"), at(asked).about("body")), (vec![value], bodies.to_vec()), "{asked}: {:#?}", at(asked));
        for body in bodies {
            assert_eq!(tests_of(&s, body, value, case).len(), 1, "{body} tests {value} against {case}");
        }
    }
    assert!(tests_of(&s, "runs.Runner.batch", "self.mode", "parallel")[0].receiver);
    // the local `m` is the value it is bound to: `settings.mode`, tested at two choices of one body
    let m = tests_of(&s, "runs.verbosity", "settings.mode", "parallel");
    assert!(m.len() == 2 && m.iter().all(|c| c.written == "m") && m[0].choice != m[1].choice, "{m:?}");
    // `run` picks where its Job comes from: `load` and `infer` are the arms of one choice
    let (load, infer) = (&step(&s, "runs.run", "runs.load").arms, &step(&s, "runs.run", "runs.infer").arms);
    assert!(load.len() == 1 && infer.len() == 1 && load[0].0 == infer[0].0 && load[0].1 != infer[0].1, "{load:?} {infer:?}");
    assert_eq!(at("runs.run").about("step"), ["runs.load", "runs.infer"], "{:#?}", at("runs.run"));
    // the shortcut still runs the job and sends its line
    assert!(edge(&s, EdgeKind::Calls, "runs.run", "runs.send") && edge(&s, EdgeKind::Calls, "runs.run", "runs.load"));
}

/// One side of the lens pair's club (`tests/pairs/lens/rust/club`): the Python club's desk, which
/// keeps visits, members and reports in SQLite, a table each, and its log and roster in files, as
/// Rust writes them.
fn club() -> &'static Pair {
    static PAIR: OnceLock<Pair> = OnceLock::new();
    PAIR.get_or_init(|| common::pair("lens/rust/club"))
}

/// What `lens` asks of class `id`: the types it loses and the places it loses them at, each sorted;
/// none when it asks nothing.
fn lens_of(g: &Graph, id: &str) -> Option<(Vec<String>, Vec<String>)> {
    let f = check(g).into_iter().find(|f| f.principle == "lens" && f.node == id)?;
    let sorted = |role: &str| -> Vec<String> {
        let mut ids: Vec<String> = f.about(role).into_iter().map(String::from).collect();
        ids.sort();
        ids
    };
    Some((sorted("lost"), sorted("place")))
}

/// What a lens puts, it gets back, at the table it puts to; and a key its statement finds rows by
/// is no value put. The clean `Desk` only writes `visits` and only reads `members`: no lens on
/// either. Its `Register` gets the Member back from `members`, and its `Visits.forget` deletes a
/// Member's visits by the Member, a key. The shortcut's `Desk` reads `visits` back as tuples, its
/// `Register` reads back only names (the Member inside a Report comes from `reports`), and its
/// `Visits.come` inserts a Member's name as a value: each is asked.
#[test]
fn lens_the_place_is_the_table_and_a_key_is_no_put() {
    let (c, s) = (&club().clean, &club().shortcut);
    let fs = check(c);
    assert!(fs.is_empty(), "{:#?}", fs.iter().map(|f| format!("{} {} {}", f.principle, f.node, f.what)).collect::<Vec<_>>());
    let at = |f: &str| touches(c, f, "db_sqlite").iter().map(|t| (t.artifact.clone(), t.dir_name())).collect::<Vec<_>>();
    assert_eq!(at("club.desk.Desk.members"), [("members".to_string(), "read")]);
    assert_eq!(at("club.desk.Visits.forget"), [("visits".to_string(), "write")]);
    assert!(edge(c, EdgeKind::Takes, "club.desk.Visits.forget", "club.Member"));
    let lens = |id: &str, ty: &str, table: &str| assert_eq!(lens_of(s, id), Some((vec![ty.into()], vec![table.into()])), "{id}");
    lens("club.desk.Desk", "club.Visit", "w:SQLite:visits");
    lens("club.desk.Register", "club.Member", "w:SQLite:members");
    lens("club.desk.Visits", "club.Member", "w:SQLite:visits");
}

/// A get gives a value back from the place; an operation that reads the place on its way to
/// writing it is a put. The clean `Log::add` reads the file, pushes a line and writes it back,
/// `io::Result<()>`: an end product, no lens. The shortcut's `Log` also gives the lines back, and
/// is asked where the Visit went.
#[test]
fn lens_a_get_gives_a_value_back() {
    let (c, s) = (&club().clean, &club().shortcut);
    let kinds: Vec<&str> = node(c, "club.files.Log.add").touches.iter().map(|t| t.kind_name()).collect();
    assert!(kinds.contains(&"disk_read") && kinds.contains(&"disk_write"), "{kinds:?}");
    assert_eq!(lens_of(c, "club.files.Log"), None);
    assert_eq!(lens_of(s, "club.files.Log").map(|(types, _)| types), Some(vec!["club.Visit".to_string()]));
}

/// What a get gives back may be the class's own type, which a constructor gives. The clean
/// `Roster::load` and `Roster::reread` give a Roster, which holds the Members `add` puts. The
/// shortcut's `Roster::load` gives a Roster that holds the lines, and `Roster` is asked.
#[test]
fn lens_a_get_gives_its_own_class_back() {
    let (c, s) = (&club().clean, &club().shortcut);
    assert!(node(c, "club.files.Roster.load").ctor);
    assert!(edge(c, EdgeKind::Has, "club.files.Roster", "club.Member"));
    assert_eq!(lens_of(c, "club.files.Roster"), None);
    assert_eq!(lens_of(s, "club.files.Roster").map(|(types, _)| types), Some(vec!["club.Member".to_string()]));
}

/// What a get gives back holds its fields, a library's type among them. `Ledger` puts a `Decimal`
/// and gets it back as an `Entry`'s `amount: Decimal` (`tests/pairs/lens/rust/ledger`); the Rust
/// reader keeps a field of a library's type beside the struct, not as a `has` edge, so `lens` does
/// not see the Decimal come back.
#[test]
#[ignore = "gap (a): the Rust reader draws no `has` edge to a library's type"]
fn lens_a_library_type_comes_back_inside_what_a_get_gives() {
    let g =
        analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/lens/rust/ledger/clean"), &Options::default()).expect("reads");
    assert!(edge(&g, EdgeKind::Has, "books.Entry", "ext:rust_decimal.Decimal"));
    assert!(found(&g, "lens").is_empty(), "{:?}", found(&g, "lens"));
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

/// An associated type is the type each implementation fixes it to. `Shelf.get` gives a `Book`
/// whether it writes `-> Book` (clean) or `-> Self::Item` with `type Item = Book` (shortcut), and
/// the trait's `Store.get`, whose `Self::Item` each implementation names, gives what they fix: a
/// call into the family hands on the `Book` its override gives.
#[test]
fn gives_an_associated_type_is_what_its_implementations_fix() {
    for side in ["clean", "shortcut"] {
        let g = books(side);
        for f in ["books.Shelf.get", "books.Store.get"] {
            assert_eq!(targets(&g, EdgeKind::Returns, f), ["books.Book"], "{side}: {f}");
        }
    }
}

/// One side of the change pair (`tests/pairs/changes`): a log a line is noted in.
fn journal(side: &str) -> Graph {
    analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/changes/rust").join(side), &Options::default()).expect("reads")
}

/// A change to an input shows in the signature. Rust says it with `&mut Log`, and with a type that
/// changes through a shared reference, `&RefCell<Log>`: on the clean side both put the Log in the
/// function's codomain, `changes` has nothing to ask, and neither is a call that leaves nothing
/// behind. The step that makes the change makes the Log anew, also through the guard the cell
/// gives on what it holds (`log.borrow_mut().lines.push(..)`), as `log.lines.push(..)` does. The
/// shortcut borrows the log to read and pushes the line onto a copy of its lines: the line is
/// lost, the signature says the log stays as it was, the push makes the copy anew, and `nothing`
/// asks.
#[test]
fn changes_a_cell_in_the_signature_says_the_value_changes() {
    let (c, s) = (journal("clean"), journal("shortcut"));
    let fs = check(&c);
    assert!(fs.is_empty(), "{:#?}", fs.iter().map(|f| format!("{} {} {}", f.principle, f.node, f.what)).collect::<Vec<_>>());
    for f in ["journal.note", "journal.note_shared"] {
        assert!(node(&c, f).params.iter().any(|p| p.name == "log" && p.changes), "{f}: {:?}", node(&c, f).params);
        assert_eq!(targets(&c, EdgeKind::Mutates, f), ["journal.Log"], "{f}");
        let push = node(&c, f).sites.iter().find(|x| x.op == "push").unwrap_or_else(|| panic!("{f}: no push"));
        assert!(push.changes && gives(push) == ["journal.Log"], "{f}: {:?}", push.gives);
    }
    assert_eq!(found(&s, "nothing"), ["journal.note"]);
    assert!(node(&s, "journal.note").params.iter().all(|p| !p.changes));
    assert!(targets(&s, EdgeKind::Mutates, "journal.note").is_empty());
    let push = node(&s, "journal.note").sites.iter().find(|x| x.op == "push").unwrap();
    assert_eq!(handed(push), ["·lines"], "the copy, not the log");
}

// ---------- layers: the package quotient the picture draws ----------

/// One side of the layers pair (`tests/pairs/layers/rust`): a crate whose root audits through a
/// stamp, and a ledger of books held in money's coins, with tests inside money that post.
fn cyc_rust(side: &str) -> Graph {
    analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/layers/rust").join(side), &Options::default()).expect("reads")
}

/// Whether a node is test code: marked so, or in a module that is.
fn tested(g: &Graph, id: &str) -> bool {
    let n = node(g, id);
    n.test || g.node(&n.module).is_some_and(|m| m.test)
}

/// The modules of a crate depend on one another in one direction, its root's own code a block
/// beside them, and a type is a dependence as a call is. The clean side keeps the stamp's rate in
/// a module of its own below the root, and money knows nothing of the ledger that holds its coins.
/// The shortcut writes `rate` in `lib.rs`, which the stamp calls back up into while `audit` calls the
/// stamp, and lets money's `worth` take a `Book` of the ledger that holds a `Coin`: a cycle through
/// types alone, with no call from money's built code to the ledger.
#[test]
fn layers_a_crates_root_and_its_types_depend_in_one_direction() {
    let (c, s) = (cyc_rust("clean"), cyc_rust("shortcut"));
    assert!(found(&c, "layers").is_empty(), "{:?}", check(&c).iter().map(|f| &f.what).collect::<Vec<_>>());
    // two cycles: the root's `audit` calls the stamp, which calls `rate` back in the root; money's
    // `worth` takes the ledger's `Book`, which holds money's `Coin`
    let layers: Vec<_> = check(&s).into_iter().filter(|f| f.principle == "layers").collect();
    assert_eq!(layers.len(), 2, "{layers:#?}");
    for through in [&["cyc.audit", "cyc.stamp.stamp", "cyc.rate"][..], &["cyc.money.worth", "cyc.ledger.Book", "cyc.money.Coin"]] {
        let by: Vec<_> = layers.iter().filter(|f| through.iter().all(|l| links(f).contains(l))).collect();
        assert_eq!(by.len(), 1, "one cycle through {through:?}: {layers:#?}");
    }
    assert!(edge(&s, EdgeKind::Calls, "cyc.audit", "cyc.stamp.stamp"));
    assert!(edge(&s, EdgeKind::Calls, "cyc.stamp.stamp", "cyc.rate"));
    assert!(edge(&s, EdgeKind::Takes, "cyc.money.worth", "cyc.ledger.Book"));
    assert!(edge(&s, EdgeKind::Has, "cyc.ledger.Book", "cyc.money.Coin"));
    let money_calls_ledger =
        s.edges.iter().filter(|e| e.kind == EdgeKind::Calls && e.src.starts_with("cyc.money.") && e.dst.starts_with("cyc.ledger."));
    assert!(money_calls_ledger.into_iter().all(|e| tested(&s, &e.src)), "money calls the ledger only from its tests");
}

/// Test code is a way in from outside, not a layer. Inside `money` both sides keep an inline
/// `#[cfg(test)] mod tests`, whose `posted` calls the ledger, and a file module declared under
/// `#[cfg(test)]` (`money/checks.rs`) that builds a `Book`: counted as money's code they would close a
/// cycle with the ledger that holds money's coins. The inline module's items stand in `money` in
/// the graph, marked as test code; its `use super::*` is the tests' import, not `money`'s.
#[test]
fn layers_a_test_module_is_no_layer() {
    for side in ["clean", "shortcut"] {
        let g = cyc_rust(side);
        assert!(node(&g, "cyc.money.posted").test, "{side}: an item of an inline test module is test code");
        assert!(node(&g, "cyc.money.checks").test, "{side}: a file module declared under cfg(test) is test code");
        assert!(!node(&g, "cyc.money.round").test, "{side}: what stands beside a test module is not");
        assert!(edge(&g, EdgeKind::Calls, "cyc.money.posted", "cyc.ledger.post"), "{side}");
        assert!(edge(&g, EdgeKind::Calls, "cyc.money.checks.posted_book", "cyc.ledger.post"), "{side}");
        assert!(!edge(&g, EdgeKind::Imports, "cyc.money", "cyc"), "{side}: the tests' `use super::*` is no import of money's");
        // no link of a cycle the layers find runs from or to test code; the clean side, whose money
        // only takes the ledger from its tests, has none
        let layers: Vec<_> = check(&g).into_iter().filter(|f| f.principle == "layers").collect();
        let test_code: Vec<&str> =
            g.nodes.iter().filter(|n| n.module.starts_with("cyc.money") && tested(&g, &n.id)).map(|n| n.id.as_str()).collect();
        assert!(test_code.contains(&"cyc.money.posted") && test_code.contains(&"cyc.money.checks.posted_book"), "{side}: {test_code:?}");
        assert!(layers.iter().all(|f| links(f).iter().all(|id| !test_code.contains(id))), "{side}: {layers:#?}");
        assert_eq!(layers.is_empty(), side == "clean", "{side}: {layers:#?}");
    }
}

// ---------- prophecy: what leans, and what a name names ----------

fn prophecy_rust(topic: &str, side: &str) -> Graph {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/prophecy").join(topic).join("rust").join(side);
    analyze(&dir, &Options::default()).expect("reads")
}

/// Test code may lean on the code it is written for before it is built. On the clean side a helper
/// in an inline `#[cfg(test)] mod tests`, a file module declared under `#[cfg(test)]` and an
/// integration test in `tests/` call the prophecy `seal`: Atlas asks nothing. The shortcut writes
/// the inline module's helper beside the code it checks, outside the test module: built code that
/// would run on a stub, and asked.
#[test]
fn prophecy_a_test_leans_on_a_prophecy_freely() {
    let (c, s) = (prophecy_rust("tested", "clean"), prophecy_rust("tested", "shortcut"));
    assert!(check(&c).is_empty(), "{:?}", check(&c).iter().map(|f| format!("{} {}", f.principle, f.what)).collect::<Vec<_>>());
    for helper in ["archive.sealed_twice", "archive.checks.sealed", "archive.tests.seal.sealed"] {
        assert!(edge(&c, EdgeKind::Calls, helper, "archive.seal"), "{helper} calls the prophecy");
        assert!(tested(&c, helper), "{helper} is test code");
    }
    assert_eq!(found(&s, "prophecy"), ["archive.sealed_twice"]);
    assert!(!tested(&s, "archive.sealed_twice"));
}

/// A name the prophecy's `supersedes` writes is read as Rust reads a path: `crate::` and the crate's
/// own name start at the crate, `self::` at the prophecy's module and `super::` at the one above it;
/// a name that ends one node's id names it. On the clean side every form holds. The shortcut writes
/// `save`, which ends the ids of the ledger's `save` and the shelf's: Atlas says it names both, not
/// that the code does not hold it. And `crate::ledger::store` names nothing: a false prophecy.
#[test]
fn prophecy_a_name_is_read_as_the_language_reads_a_path() {
    let (c, s) = (prophecy_rust("names", "clean"), prophecy_rust("names", "shortcut"));
    assert!(found(&c, "prophecy").is_empty(), "{:?}", check(&c).iter().map(|f| &f.what).collect::<Vec<_>>());
    let replaces = |g: &Graph, id: &str| node(g, id).prophecy.as_ref().and_then(|p| p.replaces.clone());
    for (prophecy, replaced) in [
        ("archive.post_v2", "archive.ledger.post"),
        ("archive.post_v3", "archive.ledger.post"),
        ("archive.save_v2", "archive.shelf.save"),
        ("archive.ledger.save_v3", "archive.ledger.save"),
        ("archive.ledger.keep_v2", "archive.shelf.keep"),
    ] {
        assert_eq!(replaces(&c, prophecy).as_deref(), Some(replaced), "{prophecy}");
    }
    assert_eq!(found(&s, "prophecy"), ["archive.save_v2", "archive.store_v2"]);
    // `save` names two nodes and replaces neither; `crate::ledger::store` names none
    let both = ["archive.ledger.save", "archive.shelf.save"].map(String::from).to_vec();
    let want = [("archive.save_v2", "save", both), ("archive.store_v2", "crate::ledger::store", vec![])];
    for (id, name, candidates) in &want {
        let p = node(&s, id).prophecy.as_ref().unwrap();
        assert_eq!((p.supersedes.as_deref(), p.replaces.as_deref(), &p.candidates), (Some(*name), None, candidates), "{id}");
    }
    // the revelation says the same of each: the name, not held, and what it names
    let stands = atlas_engine::revelation::revelation(&s);
    let said: Vec<_> = stands.iter().filter(|st| !st.holds).map(|st| (st.id.as_str(), st.supersedes.as_deref(), &st.candidates)).collect();
    assert_eq!(said, want.iter().map(|(id, name, candidates)| (*id, Some(*name), candidates)).collect::<Vec<_>>());
}

/// One side of the nothing pair (`tests/pairs/nothing`): a job runner that runs a task, says when a
/// job is done, settles its log and stops the run, in Python, Rust and TypeScript alike.
fn jobs(side: &str) -> Graph {
    analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/nothing/rust").join(side), &Options::default()).unwrap()
}

/// A call leaves something behind, and where Atlas cannot see what a call leaves, the question says
/// so. The clean runner leaves each step where a reader sees it: `finish` is handed its callback,
/// `count` changes an atomic `static`, `settle` writes its log and `stop` ends the run with
/// `std::process::exit`: nothing is asked. The shortcut's `Job` keeps its callback in a field and
/// calls it there, `(self.on_done)()`, a call Atlas cannot follow: Atlas asks what it leaves behind.
/// `settle`, a door that waits and does nothing, is asked whether it is dead, as any function is.
/// An implementation names the trait it fills in, so `run_task` reads alike on both sides.
#[test]
fn nothing_says_what_atlas_cannot_see() {
    let (c, s) = (jobs("clean"), jobs("shortcut"));
    assert_eq!(found(&c, "nothing"), Vec::<String>::new());
    assert!(node(&c, "jobs.stop").raises, "ending the run is what `stop` leaves behind");
    assert_eq!(found(&s, "nothing"), ["jobs.Job.finish", "jobs.settle"]);
    // `finish` calls the callback it keeps in a field, which Atlas does not see into; all that
    // `settle` runs, Atlas sees
    let unseen: Vec<&str> = node(&s, "jobs.Job.finish").unseen.iter().map(|(_, what)| what.as_str()).collect();
    assert!(unseen.len() == 1 && unseen[0].contains("on_done"), "{unseen:?}");
    let settle = s.reach("jobs.settle");
    assert!(settle.iter().filter_map(|id| s.node(id)).all(|n| n.unseen.is_empty() && !n.declared), "{settle:?}");
    for g in [&c, &s] {
        assert!(node(g, "jobs.Task.run").declared, "a trait's method without a body is what its implementations fill in");
        assert!(edge(g, EdgeKind::Calls, "jobs.run_task", "jobs.Task.run"));
    }
}

/// The places pair, `places/rust`: rows read and kept, and a page made from a template, the same
/// on both sides but for where the rows come from and where the template is found.
fn stock(side: &str) -> Graph {
    analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/places/rust").join(side), &Options::default()).unwrap()
}

/// Library code does not decide where data lives, and a function that hands a path decides it as
/// much as one that opens it. Both sides read rows with `read_rows(path: &Path)`. The clean `main`
/// hands it `Path::new("data/rows.csv")`, as where the program starts may; on the shortcut the
/// library's `default_rows()` hands it. `places` asks `default_rows`, whose own code writes the
/// path, and not `read_rows`, which takes it. Both sides fall back to `out/rows.csv` where
/// `STOCK_PATH` is not set, an option of whoever starts the program, which neither side is asked.
#[test]
fn places_the_function_that_writes_the_path_decides_it() {
    let (c, s) = (stock("clean"), stock("shortcut"));
    assert!(found(&c, "places").is_empty(), "{:?}", found(&c, "places"));
    assert!(found(&s, "places").contains(&"stock.default_rows".to_string()), "{:?}", found(&s, "places"));
    assert!(
        !found(&s, "places").contains(&"stock.read_rows".to_string())
            && !found(&s, "places").contains(&"stock.save_where_told".to_string())
    );
    let read = |g: &Graph, by: &str| {
        touches(g, "stock.read_rows", "disk_read").iter().any(|t| t.artifact == "data/rows.csv" && t.fixed_by == [by])
    };
    assert!(read(&c, "stock.main.main") && read(&s, "stock.default_rows"));
}

/// A file that ships with the program is part of it. Rust's clean form compiles the template in
/// (`include_str!("template.html")`): no place at run time. The shortcut reads it at run time from
/// `env!("CARGO_MANIFEST_DIR")`, the source directory of the machine the program was built on, a
/// place the code fixes, and `places` asks.
#[test]
fn places_a_file_compiled_in_is_no_place_and_one_found_where_the_crate_was_built_is_fixed() {
    let (c, s) = (stock("clean"), stock("shortcut"));
    assert!(node(&c, "stock.page").touches.is_empty(), "{:?}", node(&c, "stock.page").touches);
    let read = touches(&s, "stock.page", "disk_read");
    assert!(read.iter().any(|t| t.artifact == "template.html" && t.fixed), "{read:?}");
    assert!(found(&s, "places").contains(&"stock.page".to_string()), "{:?}", found(&s, "places"));
}

/// The workbench pair, `workbench/rust`: what the program makes, kept in a folder of the project's
/// own. The clean side gives the folder a type whose method names each file in it
/// (`Workbench::of(project).file("report.txt")`); the shortcut hands the folder around as a bare
/// path (`made_dir(project).join("report.txt")`).
fn bench(side: &str) -> Graph {
    analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/workbench/rust").join(side), &Options::default()).unwrap()
}

/// Isomorphic is the same: a path a type's method builds from the field its constructor sets is
/// the path written out with bare paths, so the two sides name the same files, at the same places,
/// fixed by the same function. The method is followed on the value another call gives back
/// (`Workbench::of(project).file(..)`) and on a binding of it (`bench.file(..)`). What `expect` is
/// handed is a message said when the value is missing: the folder `clear` removes is `.made/`.
#[test]
fn places_a_path_a_type_builds_is_the_path_written_out() {
    let files = |g: &Graph| -> Vec<(String, String, Vec<String>)> {
        let mut out: Vec<(String, String, Vec<String>)> = ["bench.save_report", "bench.save_log", "bench.clear"]
            .iter()
            .flat_map(|f| touches(g, f, "disk_write").into_iter().map(move |t| (f.to_string(), t.artifact.clone(), t.fixed_by.clone())))
            .collect();
        out.sort();
        out
    };
    let (c, s) = (files(&bench("clean")), files(&bench("shortcut")));
    assert_eq!(c, s);
    assert!(c.iter().any(|(f, a, by)| f == "bench.save_report" && a == ".made/report.txt" && by == &["bench.main.main"]), "{c:?}");
    assert!(c.iter().any(|(f, a, _)| f == "bench.save_log" && a == "{project}/.made/log.txt"), "{c:?}");
    assert!(c.iter().any(|(f, a, _)| f == "bench.clear" && a == ".made/"), "{c:?}");
}

/// The spelled pair, `spelled/rust`: a note kept in a folder the caller names however it likes
/// (`./data/../notes`), and shown again from `notes/note.txt`. The clean side writes the folder once
/// with the libraries that say what they do to a path (`std::path::absolute`, `path_clean::clean`);
/// the shortcut takes its `.` and `..` out in a loop of its own.
fn spelled(side: &str) -> Graph {
    analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/spelled/rust").join(side), &Options::default()).unwrap()
}

/// A term written out in full is the place itself, however the code spells it (`docs/reference/mathematics.md`,
/// "Paths as terms"): a path a library makes absolute or writes without its `.` and `..` is the place its term names,
/// so the note `save` writes is the note `show` reads, one place. A loop of the program's own is not evaluated: what
/// it gives is not said, the gap the code leaves, and no place `save` writes is the one `show` reads.
#[test]
fn places_a_path_written_another_way_is_the_same_place() {
    let places = |g: &Graph, f: &str, kind: &str| -> Vec<String> { touches(g, f, kind).iter().map(|t| t.place().id).collect() };
    let (c, s) = (spelled("clean"), spelled("shortcut"));
    assert!(!places(&c, "notes.save", "disk_write").is_empty());
    assert_eq!(places(&c, "notes.save", "disk_write"), places(&c, "notes.show", "disk_read"));
    let (saved, shown) = (places(&s, "notes.save", "disk_write"), places(&s, "notes.show", "disk_read"));
    assert!(!saved.is_empty() && saved.iter().all(|p| !shown.contains(p)), "{saved:?} {shown:?}");
}

/// The kept pair, `kept/rust`: a note kept in a file in the folder `where.txt` names, read, and
/// taken once. The clean side writes it whole beside its file and renames it in, and names the
/// folder (`let folder = kept::notes_folder()?`); the shortcut writes it straight in and names
/// no folder (`note_file(&kept::notes_folder()?)`).
fn kept(side: &str) -> Graph {
    analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/kept/rust").join(side), &Options::default()).unwrap()
}

/// One file is one place, whatever operation touches it (`docs/reference/mathematics.md`, "Paths
/// as terms"): the file a note is written to, renamed into, read from and removed is one place,
/// and two files are two. On each side the place a touch draws is a function of the file it names,
/// and one to one; none is the disk beside its file. What `read` reads and `take` removes is a
/// place `keep` writes; on the clean side the partial file `keep` writes and moves out of is a
/// place of its own, the same for the write and for the rename.
#[test]
fn places_one_file_is_one_place_whatever_touches_it() {
    use atlas_engine::flow::Side;
    use atlas_engine::World;
    use std::collections::{BTreeMap, BTreeSet};
    for side in ["clean", "shortcut"] {
        let g = kept(side);
        let named: Vec<(&str, &Touch)> = g
            .nodes
            .iter()
            .flat_map(|n| n.touches.iter().map(move |t| (n.id.as_str(), t)))
            .filter(|(_, t)| t.kind == World::Disk && !t.artifact.is_empty())
            .collect();
        let mut by_file: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
        for (_, t) in &named {
            by_file.entry(t.artifact.as_str()).or_default().insert(t.place().id);
        }
        assert!(by_file.values().all(|p| p.len() == 1), "{side}: one file, one place: {by_file:?}");
        let places: BTreeSet<&String> = by_file.values().flatten().collect();
        assert_eq!(places.len(), by_file.len(), "{side}: two files, two places: {by_file:?}");
        assert!(places.iter().all(|p| !p.starts_with("w:Disk")), "{side}: {places:?}");
        let at = |f: &str, s: Option<Side>| -> BTreeSet<String> {
            named.iter().filter(|(n, t)| *n == f && s.map_or(true, |s| t.side() == s)).map(|(_, t)| t.place().id).collect()
        };
        let (read, taken, kept) = (at("kept.read", None), at("kept.take", None), at("kept.keep", Some(Side::Out)));
        assert!(!read.is_empty() && read == taken && read.is_subset(&kept), "{side}: {read:?} {taken:?} {kept:?}");
        if side == "clean" {
            let partial: BTreeSet<String> = at("kept.keep", Some(Side::In));
            assert!(partial.len() == 1 && partial.is_subset(&kept) && partial.is_disjoint(&read), "{partial:?} {kept:?} {read:?}");
        }
    }
}

/// A hole keeps the name the code gives the value, in the function that gives it
/// (`docs/reference/mathematics.md`, "Paths as terms"): the folder `where.txt` names is no value the
/// code decides, and the clean side's `main` binds it (`let folder = ..`), so the note's file is
/// `{main.folder}/note.json`, through every name it is handed on by (`note_file(folder: &Path)`,
/// `keep(path: &Path)`). The shortcut binds it to no name: its hole is `{}`, never the name of the
/// function that gives it (`notes_folder`) or of a method (`trim`).
#[test]
fn places_a_value_the_code_names_is_a_hole_by_that_name() {
    use std::collections::BTreeSet;
    let files = |g: &Graph| -> BTreeSet<String> {
        g.nodes.iter().flat_map(|n| n.touches.iter()).filter(|t| t.artifact.ends_with("note.json")).map(|t| t.artifact.clone()).collect()
    };
    assert_eq!(files(&kept("clean")), BTreeSet::from(["{main.folder}/note.json".to_string()]));
    assert_eq!(files(&kept("shortcut")), BTreeSet::from(["{}/note.json".to_string()]));
}

/// One side of the shadows pair (`tests/pairs/shadows/rust`): letters sorted onto a pile, by a
/// body whose locals are named apart from the module's functions on the clean side and like them
/// on the shortcut.
fn inbox(side: &str) -> Graph {
    analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/shadows/rust").join(side), &Options::default()).expect("reads")
}

/// A name a body binds shadows the item of that name where the binding is in scope: a local
/// handed on (`pile.letters.extend(addressed)`, a tuple pattern's `returned`, `push(size)`) is no
/// function handed on, and a closure called by its name (`tidy(&l.to)`) is no call of the module's
/// `tidy`. Naming a local like a function changes nothing of what the code does, so `sort` draws
/// alike on both sides: it calls none of the program's functions. Where no local binds the name,
/// the function is still the item: `total` hands `size` on to `map`, and `addressed` calls `tidy`.
#[test]
fn composition_a_local_shadows_the_function_it_is_named_like() {
    for side in ["clean", "shortcut"] {
        let g = inbox(side);
        assert_eq!(targets(&g, EdgeKind::Calls, "inbox.sort"), Vec::<&str>::new(), "{side}");
        let sites = &node(&g, "inbox.sort").sites;
        assert!(sites.iter().all(|s| !s.to.starts_with("inbox.")), "{side}: {sites:?}");
        assert_eq!(targets(&g, EdgeKind::Calls, "inbox.total"), ["inbox.size"], "{side}");
        assert_eq!(targets(&g, EdgeKind::Calls, "inbox.addressed"), ["inbox.tidy"], "{side}");
    }
}

/// One side of the dispatch pair (`tests/pairs/dispatch/rust`): text read from a source, by the
/// trait's own `read` or the one a kind overrides it with, each method called on the value it
/// runs on on the clean side and by its path on the shortcut.
fn sources(side: &str) -> Graph {
    analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/dispatch/rust").join(side), &Options::default()).expect("reads")
}

/// A method called by its path runs on the value it is handed first, as the same call in method
/// syntax does: `Source::read(p)`, `<Plain as Source>::read(p)` and `Self::raw(self)` dispatch on
/// what the body knows of their first argument, as `p.read()` and `self.raw()` do. Rust has no form
/// that runs a method in place of the one dispatch finds, so the two sides draw each step alike:
/// `plain` runs the trait's `read` on a `Plain`, `trimmed` the override `Trimmed.read`, and `any`,
/// on a `dyn Source`, a call into the family.
#[test]
fn composition_a_method_called_by_its_path_runs_on_its_first_argument() {
    let (c, s) = (sources("clean"), sources("shortcut"));
    let steps = |g: &Graph, f: &str| -> Vec<(String, String, bool, Vec<String>)> {
        node(g, f)
            .sites
            .iter()
            .filter(|s| !s.to.is_empty())
            .map(|s| (s.to.clone(), s.on.clone(), s.on_receiver, s.hands.iter().map(|h| format!("{}·{}", h.ty, h.name)).collect()))
            .collect()
    };
    for f in ["sources.plain", "sources.trimmed", "sources.any", "sources.both", "sources.Source.read", "sources.Trimmed.read"] {
        assert_eq!(steps(&s, f), steps(&c, f), "{f}");
        assert_eq!(targets(&s, EdgeKind::Calls, f), targets(&c, EdgeKind::Calls, f), "{f}");
    }
    let to = |f: &str| steps(&c, f).into_iter().map(|(t, on, ..)| (t, on)).collect::<Vec<_>>();
    assert_eq!(to("sources.plain"), [("sources.Source.read".to_string(), "sources.Plain".to_string())]);
    assert_eq!(to("sources.trimmed"), [("sources.Trimmed.read".to_string(), String::new())]);
    assert_eq!(to("sources.any"), [("sources.Source.read".to_string(), String::new())]);
}

/// A call by name runs, for each type its value may be, the method that type runs (#268), as in Python and TypeScript:
/// a value of a struct is of that struct, built at the call or declared, since no type is below a struct; one the code
/// declares of a trait (`&dyn Block`) may be of any type that implements it. The built pair (`tests/pairs/built/rust`)
/// shows a heading on a block under the block's name: the clean side is handed a `&dyn Block`, so `block.show(..)` may
/// run `Boxed.show`; the shortcut builds a `Plain` where it shows it, a unit struct named as the value (`Plain.show(..)`),
/// which runs the trait's own `show` alone. `Boxed.show`'s `heading` holds the heading exactly where the call may run it
/// (#263). The name is a supertrait's method (`trait Block: Named`, #285): on the `&dyn Block` it may run the `name` of
/// each type below `Block`, on the `Plain` that type's alone.
#[test]
fn built_a_value_of_a_struct_runs_its_type_and_one_of_a_trait_any_type_below() {
    for side in ["clean", "shortcut"] {
        let g = analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/built/rust").join(side), &Options::default())
            .expect("reads");
        let bodies = atlas_engine::flow::bodies(&g);
        // the bodies a call may run: a trait's method without a body of its own runs none
        let may = |method: &str| -> BTreeSet<String> {
            let call = bodies["notes.report"].steps().into_iter().find(|s| s.to.ends_with(method)).expect("report calls it");
            std::iter::once(call.to.clone())
                .chain(call.alts(&bodies).iter().map(|k| atlas_engine::flow::split(k).0.to_string()))
                .filter(|f| !node(&g, f).declared)
                .collect()
        };
        let runs = may(".show").contains("notes.Boxed.show");
        assert_eq!(runs, side == "clean", "{side}");
        assert_eq!(
            node(&g, "notes.Boxed.show").uses.iter().any(|u| u.name == "heading"),
            runs,
            "{side}: the parameter follows what the call runs"
        );
        let named: BTreeSet<String> = match side {
            "clean" => ["notes.Plain.name", "notes.Boxed.name"].map(String::from).into(),
            _ => ["notes.Plain.name"].map(String::from).into(),
        };
        assert_eq!(may(".name"), named, "{side}");
    }
}

/// A version of the sunspots pipeline (`tests/pairs/pipeline`).
fn sunspots(version: &str) -> Graph {
    analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/pipeline").join(version), &Options::default()).expect("reads")
}

/// A place carries a what, and a collection of pairs carries none the code names: the quick
/// pipeline's `load` gives a `Vec<(NaiveDate, f64)>` and `smooth` takes a `&[(NaiveDate, f64)]`,
/// pairs of a date and a number, so neither gives nor takes a `NaiveDate`. The clean pipeline
/// names its record: `load` gives `Vec<Month>`, Months, and `smooth` takes them.
#[test]
fn a_place_carries_a_what_a_collection_of_pairs_names_none() {
    let (c, s) = (sunspots("rust-clean"), sunspots("rust"));
    for (f, kind) in [("sunspots.load", EdgeKind::Returns), ("sunspots.smooth", EdgeKind::Takes), ("sunspots.smooth", EdgeKind::Returns)] {
        assert!(targets(&s, kind, f).is_empty(), "{f} {kind:?}: {:?}", targets(&s, kind, f));
    }
    assert_eq!(targets(&c, EdgeKind::Returns, "sunspots.series.load"), ["sunspots.series.Month"]);
    assert!(targets(&c, EdgeKind::Takes, "sunspots.series.smooth").contains(&"sunspots.series.Month"));
}

/// An entry run from the command line is handed the words the human types. The clean side parses
/// them into `Cli` (clap's derive, its subcommands the arms of `main`); the shortcut reads them raw,
/// `env::args().skip(1).collect()`, a step that takes the entry's words under the name `args`
/// and declares no option: the code names none, so the usage says none.
#[test]
fn places_the_command_line_is_the_entrys() {
    let parses = |g: &Graph| -> Vec<(String, String, Vec<String>)> {
        node(g, "spend.main").sites.iter().filter_map(|s| Some((s.to.clone(), s.op.clone(), s.command_line.clone()?))).collect()
    };
    let clean = parses(clean());
    assert!(clean.len() == 1 && clean[0].0 == "spend.Cli", "{clean:?}");
    assert_eq!(parses(shortcut()), [(String::new(), "collect".to_string(), vec![])]);
    let args = node(shortcut(), "spend.main").sites.iter().find(|s| s.command_line.is_some()).map(|s| s.gives.clone());
    assert!(args.is_some_and(|g| g.iter().any(|h| h.name == "args")));
}

/// One side of the rebinds pair (`tests/pairs/rebinds/rust`): a module linked, its bases resolved
/// in a loop and its body read after it, each value named apart on the clean side and both named
/// `b` on the shortcut.
fn linker(side: &str) -> Graph {
    analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/rebinds/rust").join(side), &Options::default()).expect("reads")
}

/// A name holds the value of the binding visible where it is used: inside the loop `b` is the
/// base, after the `let` it is the body. Shadowing is ordinary code and reads as the code with
/// names of their own: `resolve` is handed a `Base` and `check` a `Body` on both sides, and no
/// step is handed a value of the other type.
#[test]
fn composition_a_name_bound_twice_holds_each_binding_where_it_is_visible() {
    for side in ["clean", "shortcut"] {
        let g = linker(side);
        let hands = |to: &str| step(&g, "linker.link", to).hands.iter().map(|h| h.ty.clone()).collect::<Vec<_>>();
        assert_eq!(hands("linker.resolve"), ["linker.Base"], "{side}");
        assert_eq!(hands("linker.check"), ["linker.Body"], "{side}");
        assert_eq!(gives(step(&g, "linker.link", "linker.read")), ["linker.Body"], "{side}");
    }
}

/// An error leaves a body the ways `?` hands it on and as what the body gives back (#138): a call whose `Result` the
/// body gives back as it is (`self.write(..)` last in `voeg_toe`) fails with what the signature names, `io::Error`, no
/// type of the code's; a `?` written in a call that is no step is handed on by the step it is glued into (the clean
/// `Graaf { nodes: fs::read_to_string(&self.path)?.lines().. }`), or, written in no step, by the body's own code (the
/// shortcut's `graaf`, which builds no type of its own).
#[test]
fn sums_an_error_leaves_by_what_the_body_gives_back_and_by_a_glued_question_mark() {
    let p = common::pair("sums/rust");
    for (side, g) in [("clean", &p.clean), ("shortcut", &p.shortcut)] {
        let write = g.node("kennis.Kennisbank.voeg_toe").unwrap().sites.iter().find(|s| s.to == "kennis.Kennisbank.write").unwrap();
        assert_eq!(write.fails, [""], "{side}: what voeg_toe gives back");
    }
    let graaf = p.clean.node("kennis.Kennisbank.graaf").unwrap();
    let built = graaf.sites.iter().find(|s| s.to == "kennis.Graaf").expect("the clean graaf builds a Graaf");
    assert_eq!(built.fails, [""], "the read's `?` is the step's");
    assert_eq!(graaf.fails, None, "and not the body's own code's");
    let graaf = p.shortcut.node("kennis.Kennisbank.graaf").unwrap();
    assert_eq!((graaf.sites.len(), graaf.fails.as_deref()), (0, Some("")), "the shortcut's `?` is its own code's");
}

/// A `match` that ends a body without a `;` is what the body gives back (#138): `summary`'s `match self.kept_summary(day)?
/// { Some(summary) => Ok(summary), None => self.keep_summary(..) }` gives back what `keep_summary` gives, its error among it,
/// so that call fails with the signature's `io::Error`, no type of the code's.
#[test]
fn an_arm_of_a_match_that_ends_the_body_gives_back_its_call_s_error() {
    let g = analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/lens/rust/clean"), &Options::default()).unwrap();
    let keep = g.node("harvest.ReadingStore.summary").unwrap().sites.iter().find(|s| s.to == "harvest.ReadingStore.keep_summary").unwrap();
    assert_eq!(keep.fails, [""], "what the match's arm gives back");
}

/// What a closure written in a call's arguments binds in its own body is the closure's (#128): `query_map([], |row| {
/// let amount = Money { .. }; Ok(Expense { .., amount }) })` is handed what the closure gives back, not the `amount` it
/// binds.
#[test]
fn rust_a_call_is_handed_no_name_a_closure_in_it_binds() {
    let g = analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/rust/clean"), &Options::default()).unwrap();
    let query = g.node("spend.store.Store.expenses").unwrap().sites.iter().find(|s| s.op == "query_map").unwrap();
    let names: Vec<&str> = query.hands.iter().map(|h| h.name.as_str()).collect();
    assert!(!names.contains(&"amount"), "{names:?}");
    assert!(query.hands.iter().any(|h| h.ty == "spend.expense.Expense"), "what the closure gives back: {:#?}", query.hands);
}

/// The tables pair (`tests/pairs/tables/rust`): what a mode means, looked up in one table, `plans: &HashMap<Mode, Plan>`, against
/// two lists at the same places, `modes: &[Mode]` and `plans: &[Plan]`. A map holds its values by its keys (#167): the clean table is one
/// value, of Plans, which the body's own lookup reads as one; the shortcut takes the modes as a value of their own.
#[test]
fn a_table_is_one_value_and_the_lists_it_is_cut_into_two() {
    common::a_table_is_one_value("rust", "tables.timeout");
}

/// A body's own code that sets a field of a value names the field (#168): the store's `rates` sets the `date` of the Rates
/// it builds (`rates.date = date`), and no other field of them; what a figure or a lens follows through a field, it follows
/// through that one.
#[test]
fn a_set_names_the_field_it_sets() {
    let sets = &node(clean(), "spend.store.Store.rates").sets;
    let named: Vec<(&str, &str)> = sets.iter().map(|s| (s.on.name.as_str(), s.field.as_str())).collect();
    assert_eq!(named, [("rates", "date")], "{sets:#?}");
}

/// The each pair (`tests/pairs/each/rust`): what a graph draws, each node's touches and what it does instead. The clean side
/// hands `Touch::drawn`, by its path, each element of `n.touches.iter().chain(&n.instead)`, inside the closure `map` hands
/// each node: every element is a part of the graph the function is handed, through what `chain` goes on with, so the step
/// takes the graph (#166). The shortcut gathers them into a list first (`let all = ..collect()`), a value of its own, and
/// the step takes that list.
#[test]
fn an_element_of_a_closure_s_parameter_is_a_part_of_what_the_function_is_handed() {
    let each = common::pair("each/rust");
    let drawn = |g: &Graph| -> Vec<String> {
        let f = node(g, "drawing.drawn");
        f.sites.iter().filter(|s| s.to == "drawing.Touch.drawn").flat_map(|s| s.hands.iter().map(|h| h.name.clone())).collect()
    };
    assert_eq!(drawn(&each.clean), ["g"], "each touch is a part of the graph the function is handed");
    assert_eq!(drawn(&each.shortcut), ["all"], "the shortcut draws from the list it gathered");
}

/// The method pair (`tests/pairs/each/rust-method`): what a graph draws, every node's touches and what it does instead. The
/// clean side calls `drawn` on each element a closure is handed, `n.touches.iter().chain(&n.instead).map(|t| t.drawn())`,
/// inside the closure `flat_map` hands each node: the closure's `t` is an element of the node, as a loop binds it, so the
/// call is a step to `Touch::drawn`, handed the graph each element is a part of, as `.map(Touch::drawn)` is in the each pair
/// (#170). The shortcut pushes each place onto a list in two loops: the step is handed the loop's `t`, and the push changes
/// the list.
#[test]
fn a_method_called_on_a_closure_s_parameter_is_a_step_on_its_element() {
    let method = common::pair("each/rust-method");
    let drawn = |g: &Graph| -> Vec<String> {
        let f = node(g, "drawing.drawn");
        f.sites.iter().filter(|s| s.to == "drawing.Touch.drawn").flat_map(|s| s.hands.iter().map(|h| h.name.clone())).collect()
    };
    let pushes = |g: &Graph| node(g, "drawing.drawn").sites.iter().filter(|s| s.op == "push" && s.changes).count();
    assert_eq!(drawn(&method.clean), ["g"], "each touch is a part of the graph the function is handed");
    assert_eq!(pushes(&method.clean), 0, "the clean side changes no list");
    assert_eq!(drawn(&method.shortcut), ["t"], "the loop's touch, a value the body makes");
    assert_eq!(pushes(&method.shortcut), 1, "the shortcut changes a list");
}

/// The pairs pair (`tests/pairs/each/rust-pairs`): each node's places, gathered node by node with `out.push(..)`. The clean
/// side names what it gathers, `Vec<Drawn>`; the shortcut gathers pairs, `Vec<(String, Vec<Where>)>`, a type the code does
/// not name. A collection of pairs holds pairs and none of what they pair (#177): the shortcut's return holds no Where, so
/// what its body gives back is made of no Where its steps make, and the route gives back the list alone (the `.plan`
/// files). The clean route gives back the Drawns its push gathered, made of the Drawn its step builds.
#[test]
fn a_collection_of_pairs_gives_back_none_of_what_they_pair() {
    let pairs = common::pair("each/rust-pairs");
    let holds = |g: &Graph| node(g, "drawing.drawn").returns.as_ref().map(|r| r.holds.clone()).unwrap_or_default();
    let made = |g: &Graph| atlas_engine::flow::of(g).bodies["drawing.drawn"].made_from.clone();
    assert_eq!(holds(&pairs.clean), ["Vec", "Drawn"]);
    assert_eq!(made(&pairs.clean), ["drawing.Drawn"], "made of the Drawn its step builds");
    assert_eq!(holds(&pairs.shortcut), ["Vec"], "a collection of pairs, of a type the code does not name");
    assert!(made(&pairs.shortcut).is_empty(), "made of no Where: {:?}", made(&pairs.shortcut));
}

/// The choices pair: `report(&job, settings.level, ..)` is handed the settings through their `level`, and the graph says
/// so beside the value it is handed (#172), as the Python and TypeScript readers do.
#[test]
fn a_call_names_the_field_a_projection_reads() {
    common::a_call_names_the_field_a_projection_reads("rust", "runs");
}

/// The choices pair: a condition names the field it tests (#184), as the Python and TypeScript readers read it.
#[test]
fn a_condition_names_the_field_it_tests() {
    common::a_condition_names_the_field_it_tests("rust", "runs");
}

/// The projections pair: a projection bound to a name first is still the projection (#174), as the Python and TypeScript
/// readers read it.
#[test]
fn a_projection_bound_to_a_name_is_still_the_projection() {
    common::a_projection_bound_to_a_name_is_still_the_projection("rust", "charts");
}

/// The recursion pair: a call into a function the route is already inside takes what its call hands it, and no value a body
/// makes comes from beyond the route (#237); the shortcut's `const FLOOR` alone does, as in Python and TypeScript.
#[test]
fn a_recursive_call_takes_what_its_call_hands_it() {
    common::a_recursive_call_takes_what_its_call_hands_it("rust");
}

/// The literals pair: `vec![]` and `Vec::new()` make the same list, the body's (#237), as `[]` and `list()` do in Python.
#[test]
fn a_list_made_by_a_literal_is_made_by_the_body() {
    common::a_list_made_by_a_literal_is_made_by_the_body("rust", "by_literal", "by_call", "by_constant");
}

/// The locals pair: an array the body writes of what it is handed is the body's own, never a module's name (#318), as
/// a list is in Python and an array in TypeScript.
#[test]
fn a_local_is_bound_by_the_bodys_own_code() {
    common::a_local_is_bound_by_the_bodys_own_code("rust", "pair");
}

/// The locals pair: a `vec![..]` is the array it holds, so `listed` binds its vector by its own code as `pair` binds its
/// array, and what the macro is written of is read as the array's elements are: the module's constant in the shortcut's
/// `vec![first, QUIET]` comes from beyond, as it does in `[first, QUIET]` (#327).
#[test]
fn a_vec_is_the_array_it_holds() {
    let p = common::pair("locals/rust");
    for (side, g) in [("clean", &p.clean), ("shortcut", &p.shortcut)] {
        let read = |f: &str| {
            let n = g.node(f).unwrap_or_else(|| panic!("{side}: no node {f}"));
            let names = |xs: &[atlas_engine::Use]| xs.iter().map(|u| u.name.clone()).collect::<Vec<_>>();
            (names(&n.uses), names(&n.binds))
        };
        assert_eq!(read("signal.listed"), read("signal.pair"), "{side}: what the body reads and binds");
        for budget in common::BUDGETS {
            let plans = common::planned(g, &format!("vec-{side}"), budget);
            let beyond = |route: &str| common::from_beyond(&plans[route], &common::parameters(g, route));
            assert_eq!(beyond("signal.shown"), beyond("signal.paired"), "{side}, budget {budget:?}: from beyond");
        }
    }
}

/// One side of the sockets pair (`tests/pairs/sockets/rust`): a server that greets each client by
/// the name it sends and lets the client read the greeting before the connection closes.
fn greeter(side: &str) -> Graph {
    analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/sockets/rust").join(side), &Options::default()).expect("reads")
}

/// A call leaves something behind, and a socket is a place: what a function reads from or writes to
/// std's `TcpStream` reaches the network, as what Python's `socket` receives and sends does. The
/// clean server lingers as TCP asks: it shuts its writing half and reads what the client still sends,
/// so `linger` reaches the network and nothing is asked. The shortcut sleeps a moment and drops the
/// connection: its `linger` reaches no place, and Atlas asks what the wait leaves behind. Binding an
/// address, shutting a half and setting a timeout move no data, on both sides.
#[test]
fn nothing_a_socket_is_a_place() {
    let (c, s) = (greeter("clean"), greeter("shortcut"));
    assert_eq!(found(&c, "nothing"), Vec::<String>::new());
    assert_eq!(found(&s, "nothing"), ["greeter.linger"]);
    let ops = |g: &Graph, f: &str| touches(g, f, "net").iter().map(|t| t.op.clone()).collect::<Vec<_>>();
    assert_eq!(ops(&c, "greeter.linger"), ["read"], "what the client still sends is read from the network");
    assert!(ops(&s, "greeter.linger").is_empty(), "a wait reaches no place");
    for g in [&c, &s] {
        assert_eq!(ops(g, "greeter.answer"), ["read", "write_all"], "the name is received and the greeting sent");
        assert!(node(g, "greeter.main").touches.is_empty(), "binding an address and accepting a connection move no data");
    }
}

/// The parents pair: `source.parent()` of `~/data/{name}` is `~/data`, as `Path.parent` and `path.dirname` are (#284).
#[test]
fn places_a_parent_drops_the_last_part_of_a_path() {
    common::a_parent_drops_the_last_part_of_a_path("rust", "shelf.summarise", "shelf.log");
}

/// The closures pair: a closure a `let` keeps and the body calls takes what each call hands it (#239), as a function of
/// the module does, and as a lambda or a `def` inside a `def` does in Python.
#[test]
fn a_closures_parameter_is_what_its_calls_hand_it() {
    common::a_closures_parameter_is_what_its_calls_hand_it("rust", &["knot.settings", "knot.lock"]);
}

/// A value built at the call is built where it is named, among a macro's tokens as outside them (#299): a unit struct's
/// name is a value of it (`docs/reference/mathematics.md`, "Method resolution order"), so `println!("{}:", Plain.name())`
/// builds a `Plain` and runs `name` on it, as `Plain.show(..)` written outside the macro builds one and runs `show` on it.
/// The shortcut of the built pair (`tests/pairs/built/rust`) writes both forms: each method it runs on a `Plain` is handed,
/// as the value it runs on, the `Plain` the step right before it builds. The clean side is handed its block and builds none.
#[test]
fn built_a_unit_struct_named_in_a_macro_is_built_where_it_is_named() {
    for side in ["clean", "shortcut"] {
        let g = analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/built/rust").join(side), &Options::default())
            .expect("reads");
        let sites = &node(&g, "notes.report").sites;
        // a method of `Plain`'s own, or a trait's run on a `Plain` (`Block.show`)
        let on_plain = |s: &Site| s.to.starts_with("notes.Plain.") || s.on == "notes.Plain";
        let runs: Vec<usize> = (0..sites.len()).filter(|&i| on_plain(&sites[i])).collect();
        for &i in &runs {
            let built = i.checked_sub(1).map(|b| &sites[b]).filter(|b| b.to == "notes.Plain");
            let built = built.unwrap_or_else(|| panic!("{side}: no Plain is built before {}", sites[i].to));
            assert!(
                sites[i].hands.iter().any(|h| built.gives.contains(h) && h.fills == [atlas_engine::Slot::Receiver]),
                "{side}: {} runs on the Plain built for it: {:?}",
                sites[i].to,
                sites[i].hands
            );
        }
        let builds = sites.iter().filter(|s| s.to == "notes.Plain").count();
        assert_eq!(builds, runs.len(), "{side}: one Plain built for each method run on one");
        assert_eq!(runs.len(), if side == "shortcut" { 2 } else { 0 }, "{side}: `Plain.name()` in the macro and `Plain.show(..)`");
    }
}

/// The sockets pair (#302): the client `ask` connects to the greeter's address and hands the connection to `hear`; the
/// server binds the address and hands each connection it accepts to `answer`, which hands it to `linger`. Each keeps the
/// place it is opened at (`common::a_connection_keeps_its_place`).
#[test]
fn a_door_keeps_its_place() {
    common::a_connection_keeps_its_place("rust", ["greeter.answer", "greeter.linger"], "greeter.main", "greeter.bin.ask");
}

/// The handles pair (#323): a `File` handed on is read and written at the path it was opened at, never at the buffer
/// an operation is handed (`f.read_to_string(&mut text)` reads the file `f` is), as a `TcpStream` handed on is at its
/// address (`common::a_file_handed_on_keeps_its_place`).
#[test]
fn handles_a_file_handed_on_keeps_its_place() {
    common::a_file_handed_on_keeps_its_place("rust");
}

/// The locals pair: a name bound to what an operator or an array computes of names stands for those names, so a step
/// handed it is handed what the expression written in its place hands (#328), and the route's own arrow takes what it
/// takes with the expression written in place (#343), as in Python and TypeScript.
#[test]
fn a_name_computed_of_names_stands_for_them() {
    common::a_name_computed_of_names_stands_for_them("rust");
}

/// The twice pair: `kind` in `root.join(kind).join(format!("{kind}.csv"))` is one value, so the loop writes two places,
/// never four (#324).
#[test]
fn places_a_name_bound_once_is_one_value() {
    common::a_name_bound_once_is_one_value("rust", "export.export");
}

/// The trees pair: a recursion over `read_dir` is one place at every depth (#325).
#[test]
fn places_a_walk_of_a_tree_is_one_place_at_every_depth() {
    common::a_walk_of_a_tree_is_one_place_at_every_depth("rust", "backup.copy_tree", "backup.main");
}

/// The locals pair: a vector a step changes in place (`both.push(second)`) holds a value of its own and stands for nothing,
/// where the vector written out once stands for the levels it holds (#355), as in Python and TypeScript.
#[test]
fn a_name_changed_in_place_holds_a_value_of_its_own() {
    common::a_name_changed_in_place_holds_a_value_of_its_own("rust");
}

/// The locals pair's shortcut: a call handed a name as `&mut` changes it in place as a method that changes what it runs on
/// does (#355): `add(&mut both, second)` is `both.push(second)`, as a function that changes what it is handed is in Python
/// and TypeScript.
#[test]
fn a_name_handed_as_mut_is_changed_in_place() {
    common::a_name_a_function_changes_holds_a_value_of_its_own("rust", "add", "push");
}

/// The locals pair: a vector pushed onto in one arm is the choice's value, which the step after the choice takes, where
/// the vector the choice gives, written out in each arm, is the body's own (#366), as in Python and TypeScript.
#[test]
fn a_name_changed_in_one_arm_is_the_choices_value() {
    common::a_name_changed_in_one_arm_is_the_choices_value("rust");
}

/// The lookups pair: `levels.first()?` and `levels.get(0)?` are the projection `&levels[0]` is and `levels.get(name)?` the
/// one `&levels[name]` is, no step, also of a vector changed in place first and of a map a `LazyLock` keeps (#346, #355),
/// as in Python and TypeScript; a tuple a tuple pattern takes apart binds each name as a `let` of its element would, so
/// `let (first, second) = (levels.first()?, levels.get(1)?)` is `(&levels[0], &levels[1])` (`paired`). The twins `added`
/// and `added_looked_up` are not one route yet: a `push` to the row `rows.first_mut()?` picks changes the rows, one to the
/// row `&mut rows[0]` borrows is no step (known #374).
#[test]
fn a_lookup_is_the_projection_its_subscript_is() {
    common::a_lookup_is_the_projection_its_subscript_is(
        "rust",
        &[
            ("indexed", "looked_up"),
            ("indexed", "got_at"),
            ("paired", "paired_looked_up"),
            ("keyed", "got"),
            ("pushed", "pushed_looked_up"),
        ],
        &[],
    );
}
