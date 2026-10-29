//! The Python pair: one program, `harvest`, written twice. It reads its settings and a list of
//! stations from files, asks a weather service over HTTP for each station's latest reading,
//! keeps the readings in SQLite, writes reports through a family of exporters, summarises them
//! with pandas and shows them on the screen; a command line and an HTTP API start it.
//! `clean/` is written as a reviewer approves it, `shortcut/` with the shortcuts people take
//! under time pressure. Each side is read as its own repository, so a node has the same id on
//! both. Each test names the principle the shortcut breaks and states three things: the clean
//! side reads as the algebra says, the shortcut shows as a shortcut, and the shortcut is still
//! in the graph. A statement Atlas does not meet yet is a test of its own, ignored with what
//! Atlas gets wrong.

mod common;

use common::Pair;

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::OnceLock;

use atlas_engine::flow::{self, Side, Step};
use atlas_engine::principles::{check, Finding};
use atlas_engine::{analyze, EdgeKind, EntryKind, Graph, NodeKind, Options, Touch, World};

fn side(name: &str) -> Graph {
    analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/python").join(name), &Options::default()).unwrap()
}

fn clean() -> &'static Graph {
    static G: OnceLock<Graph> = OnceLock::new();
    G.get_or_init(|| side("clean"))
}

fn shortcut() -> &'static Graph {
    static G: OnceLock<Graph> = OnceLock::new();
    G.get_or_init(|| side("shortcut"))
}

/// The nodes `check` asks about under `principle`.
fn found(g: &Graph, principle: &str) -> Vec<String> {
    check(g).into_iter().filter(|f: &Finding| f.principle == principle).map(|f| f.node).collect()
}

/// The nodes `check` asks about under `principle` in the module `module` (`board.core`).
fn found_in(g: &Graph, principle: &str, module: &str) -> Vec<String> {
    found(g, principle).into_iter().filter(|n| n.strip_prefix(module).is_some_and(|rest| rest.starts_with('.'))).collect()
}

/// The touches of node `id` of the kind the graph's JSON names (`disk_read`, `net`, `db_sqlite`).
fn touches<'g>(g: &'g Graph, id: &str, kind: &str) -> Vec<&'g Touch> {
    g.node(id).unwrap_or_else(|| panic!("{id} is in the graph")).touches.iter().filter(|t| t.kind_name() == kind).collect()
}

fn edge(g: &Graph, src: &str, dst: &str, kind: EdgeKind) -> bool {
    g.edges.iter().any(|e| e.src == src && e.dst == dst && e.kind == kind)
}

/// The route from `entry` as text, its steps opened four levels deep.
fn route(g: &Graph, entry: &str) -> String {
    atlas_engine::story::route(g, entry, 4).unwrap_or_else(|| panic!("{entry} is a route"))
}

/// The steps the route from `entry` runs, as the route opens them, each as the key of the body it runs (the function, `@`
/// the class it runs on) with the bodies its call may run instead (`Step::alts`). Building a class runs its initializer,
/// and none of its methods.
fn runs(g: &Graph, entry: &str) -> BTreeSet<(String, Vec<String>)> {
    let (bodies, inits) = (flow::bodies(g), flow::initializers(g));
    let (mut out, mut opened) = (BTreeSet::new(), BTreeSet::new());
    let mut todo = vec![Step::of(entry)];
    while let Some(st) = todo.pop() {
        let built = g.node(&st.to).is_some_and(|n| n.kind == NodeKind::Class);
        let runs = if built { inits.get(&st.to).cloned() } else { Some(st.to.clone()) };
        let key = flow::key(runs.as_deref().unwrap_or(&st.to), &st.on);
        let alts = if built { vec![] } else { st.alts(&bodies).to_vec() };
        if !out.insert((key.clone(), alts)) {
            continue;
        }
        if !built {
            todo.extend(st.runs_instead(&bodies));
        }
        if let Some(b) = runs.and(bodies.get(&key)).filter(|_| opened.insert(key.clone())) {
            todo.extend(b.steps().into_iter().cloned());
        }
    }
    out
}

/// The functions the route from `entry` runs.
fn ran(g: &Graph, entry: &str) -> BTreeSet<String> {
    runs(g, entry).into_iter().map(|(k, _)| flow::split(&k).0.to_string()).collect()
}

/// The places the route from `entry` reaches for their content, each with the side of its step it stands on: what every
/// function it runs touches, by the place's id (`w:SQLite:readings`).
fn places(g: &Graph, entry: &str) -> Vec<(String, Side)> {
    let drawn = flow::of(g).drawn;
    ran(g, entry)
        .iter()
        .filter_map(|f| drawn.get(f))
        .flatten()
        .filter(|w| !w.shape && !w.test)
        .map(|w| (w.place.id.clone(), w.side))
        .collect()
}

/// Whether the route from `entry` reads (`Side::In`) or writes (`Side::Out`) the place `id`.
fn reaches(g: &Graph, entry: &str, id: &str, side: Side) -> bool {
    places(g, entry).iter().any(|(p, s)| p == id && *s == side)
}

/// Whether the body of `id` makes a call Atlas does not follow, as the code writes it (`op`).
fn not_followed(g: &Graph, id: &str, op: &str) -> bool {
    g.node(id).unwrap_or_else(|| panic!("{id} is in the graph")).sites.iter().any(|s| s.not_followed && s.op == op)
}

/// What the clean side says it does, it does: Atlas asks nothing about it.
#[test]
fn the_clean_side_follows_every_principle() {
    let fs = check(clean());
    assert!(fs.is_empty(), "{:#?}", fs.iter().map(|f| format!("{} {} {}", f.principle, f.node, f.what)).collect::<Vec<_>>());
}

/// A package names its doors. Python names them in `__all__` at the package's root; the shortcut
/// leaves `__init__.py` with only its docstring, so a user may import anything from anywhere.
#[test]
fn doors_a_package_names_its_doors() {
    let (c, s) = (clean(), shortcut());
    assert!(found(c, "doors").is_empty());
    for door in ["harvest.harvester.Harvester", "harvest.harvester.build_harvester", "harvest.store.ReadingStore"] {
        assert_eq!(c.node(door).unwrap().entry.as_ref().map(|e| e.kind), Some(EntryKind::Export), "{door} is a door");
    }
    assert_eq!(found(s, "doors"), ["harvest"]);
    assert!(s.nodes.iter().all(|n| n.entry.as_ref().map_or(true, |e| e.kind != EntryKind::Export)), "no door is named");
    // the package is all there, and its command line is still a way in
    assert!(s.node("harvest.harvester.Harvester").is_some());
    assert_eq!(s.node("harvest.cli.collect").unwrap().entry.as_ref().map(|e| e.kind), Some(EntryKind::Cli));
}

/// A sum is unwrapped where it arises. Python's clean form hands on both halves, each with its own
/// type: `Harvest(readings: list[Reading], outages: list[Outage])`, and the client raises for a
/// station it cannot reach. The shortcut names `Fetched = Reading | Outage`, returns it from the
/// client, and two consumers choose again between the two: the command line's `show` and the
/// report's `unreached`.
#[test]
fn sums_a_sum_is_unwrapped_where_it_arises() {
    let (c, s) = (clean(), shortcut());
    assert!(found(c, "sums").is_empty());
    assert!(c.edges.iter().all(|e| e.kind != EdgeKind::Variant), "the clean side has no sum");
    assert!(edge(c, "harvest.model.Harvest", "harvest.model.Reading", EdgeKind::Has));
    assert!(edge(c, "harvest.model.Harvest", "harvest.model.Outage", EdgeKind::Has));
    for consumer in ["harvest.cli.show", "harvest.report.unreached"] {
        assert!(edge(c, consumer, "harvest.model.Harvest", EdgeKind::Takes), "{consumer}");
        assert!(edge(s, consumer, "harvest.model.Fetched", EdgeKind::Takes), "{consumer}");
    }
    assert_eq!(found(s, "sums"), ["harvest.model.Fetched"]);
    assert!(edge(s, "harvest.model.Fetched", "harvest.model.Reading", EdgeKind::Variant));
    assert!(edge(s, "harvest.model.Fetched", "harvest.model.Outage", EdgeKind::Variant));
    assert!(edge(s, "harvest.client.WeatherClient.latest", "harvest.model.Fetched", EdgeKind::Returns));
}

/// An override keeps the promise of its base. Both sides have one family, `Exporter` with
/// `JsonExporter` and `CsvExporter`. The clean `Exporter[R]` writes a station's records, whatever
/// they are, and each format fills it in with `Reading`: its `write` takes the `list[Reading]` that
/// `Exporter[Reading]` makes of `list[R]`, under the name its base gives it. On the shortcut,
/// `CsvExporter.write` gives the number of rows where `Exporter.write` promises `None`, which ty
/// reports as an incompatible override.
#[test]
fn overrides_an_override_keeps_the_promise_of_its_base() {
    let (c, s) = (clean(), shortcut());
    assert!(found(c, "overrides").is_empty());
    let takes = |g: &Graph, id: &str| g.node(id).unwrap().params.iter().map(|p| p.to_string()).collect::<Vec<_>>();
    assert_eq!(takes(c, "harvest.export.Exporter.write"), ["self", "records: list[R]", "path: Path"]);
    for o in ["harvest.export.JsonExporter.write", "harvest.export.CsvExporter.write"] {
        assert_eq!(c.node(o).unwrap().keeps, Some(true), "{o}");
        assert_eq!(takes(c, o), ["self", "records: list[Reading]", "path: Path"]);
        assert!(edge(c, &o.replace(".write", ""), "harvest.export.Exporter", EdgeKind::Inherits));
    }
    assert_eq!(found(s, "overrides"), ["harvest.export.Exporter.write"]);
    assert_eq!(s.node("harvest.export.CsvExporter.write").unwrap().keeps, Some(false));
    // one override breaks the base's promise: it gives an int where the base gives None
    let breaking: Vec<&str> =
        s.nodes.iter().filter(|n| n.breaks.as_deref() == Some("harvest.export.Exporter.write")).map(|n| n.id.as_str()).collect();
    assert_eq!(breaking, ["harvest.export.CsvExporter.write"]);
    let gives = |id: &str| s.node(id).unwrap().returns.as_ref().map(|r| r.written.clone());
    assert_eq!(
        (gives("harvest.export.Exporter.write"), gives("harvest.export.CsvExporter.write")),
        (Some("None".into()), Some("int".into()))
    );
    // the override is still a member of the family, and still writes its file
    assert!(edge(s, "harvest.export.CsvExporter", "harvest.export.Exporter", EdgeKind::Inherits));
    assert_eq!(touches(s, "harvest.export.CsvExporter.write", "disk_write").len(), 1);
}

/// Library code does not decide where data lives. In Python the clean form takes a `Path`
/// (`load_settings(path)`) and the entry names it from a constant; the shortcut's
/// `load_settings()` opens `CONFIG_FILE`, a module constant of the library itself.
#[test]
fn places_where_the_program_starts_names_the_config_file() {
    let (c, s) = (clean(), shortcut());
    assert!(found(c, "places").is_empty());
    let read = touches(c, "harvest.settings.load_settings", "disk_read");
    assert!(read.iter().any(|t| t.artifact == "harvest.toml" && !t.fixed && t.by.contains(&"harvest.cli.collect".to_string())), "{read:?}");
    // the shortcut's read is in the graph, at the file it names
    let read = touches(s, "harvest.settings.load_settings", "disk_read");
    assert!(read.iter().any(|t| t.artifact == "harvest.json" && t.by.is_empty()), "{read:?}");
}

/// The shortcut's library code opens the file a module constant names: the code alone decides
/// where, and `places` asks.
#[test]
fn places_a_library_that_names_its_config_file_through_a_constant_is_asked() {
    let s = shortcut();
    let read = touches(s, "harvest.settings.load_settings", "disk_read");
    assert!(read.iter().any(|t| t.fixed), "{read:?}");
    assert!(found(s, "places").contains(&"harvest.settings.load_settings".to_string()), "{:?}", found(s, "places"));
}

/// A change to an input shows in the signature. Python's clean form keeps `Reading` frozen and
/// hands back a new one (`in_unit(reading, unit) -> Reading`, by `dataclasses.replace`); the
/// shortcut's `convert(reading, unit) -> None` sets the temperature of the reading it is handed.
#[test]
fn changes_a_change_to_an_input_shows_in_the_signature() {
    let (c, s) = (clean(), shortcut());
    assert!(found(c, "changes").is_empty());
    assert!(c.edges.iter().all(|e| e.kind != EdgeKind::Mutates), "nothing is changed in place");
    assert!(edge(c, "harvest.model.in_unit", "harvest.model.Reading", EdgeKind::Returns));
    assert_eq!(found(s, "changes"), ["harvest.model.convert"]);
    assert!(s.edges.iter().any(|e| e.kind == EdgeKind::Mutates
        && e.src == "harvest.model.convert"
        && e.dst == "harvest.model.Reading"
        && e.via.as_deref() == Some("reading")));
    assert!(edge(s, "harvest.harvester.Harvester.collect", "harvest.model.convert", EdgeKind::Calls));
}

/// A function whose value is used says what it gives back. In Python that is the return
/// annotation: `get_store() -> ReadingStore`. The shortcut's `get_store()` says nothing, so the
/// `history` its route calls on the store is a call on a value without a type; its
/// `Harvester.run(..) -> Any`, which the API uses, says no more: `Any` is the unchecked type.
#[test]
fn gives_a_function_whose_value_is_used_says_what_it_gives() {
    let (c, s) = (clean(), shortcut());
    assert!(found(c, "gives").is_empty());
    assert!(edge(c, "harvest.api.get_store", "harvest.store.ReadingStore", EdgeKind::Returns));
    assert!(edge(c, "harvest.api.readings", "harvest.store.ReadingStore.history", EdgeKind::Calls));
    assert!(reaches(c, "harvest.api.readings", "w:SQLite:readings", Side::In), "{:?}", places(c, "harvest.api.readings"));
    assert_eq!(found(s, "gives"), ["harvest.api.get_store", "harvest.harvester.Harvester.run"]);
    assert!(
        !edge(s, "harvest.api.readings", "harvest.store.ReadingStore.history", EdgeKind::Calls),
        "the call on the store is not followed"
    );
    assert!(not_followed(s, "harvest.api.readings", "get_store().history"), "{:#?}", s.node("harvest.api.readings").unwrap().sites);
    assert!(!reaches(s, "harvest.api.readings", "w:SQLite:readings", Side::In), "{:?}", places(s, "harvest.api.readings"));
    // the factory is still a step of the route, and the store still reads its table
    assert!(edge(s, "harvest.api.readings", "harvest.api.get_store", EdgeKind::Calls));
    assert!(edge(s, "harvest.api.get_store", "harvest.store.ReadingStore", EdgeKind::Instantiates));
    assert!(touches(s, "harvest.store.ReadingStore.history", "db_sqlite").iter().any(|t| t.artifact == "readings"));
}

/// A call on a value Atlas has no type for is still a step of its body: the route shows that
/// `readings` asks something of what `get_store` gave, and that Atlas cannot say what.
#[test]
fn gives_a_call_on_an_untyped_value_stays_a_step() {
    let readings = shortcut().node("harvest.api.readings").unwrap();
    assert_eq!(readings.sites.len(), 2, "get_store() and .history(station): {:#?}", readings.sites);
}

/// A choice is made once, in a place with a name. Python's clean form is an `Enum` whose methods
/// say what each unit means (`Unit.from_celsius`, `Unit.symbol`, each one `match` over the value
/// it runs on, each member once: the enumeration's own table); the shortcut passes the unit as a
/// string and `convert` tests `unit == "fahrenheit"` once to convert and again to label.
#[test]
fn choices_a_choice_is_made_once() {
    let (c, s) = (clean(), shortcut());
    assert!(found(c, "choices").is_empty());
    assert!(c.node("harvest.model.in_unit").unwrap().compares.is_empty(), "in_unit compares nothing");
    for f in ["harvest.model.Unit.from_celsius", "harvest.model.Unit.symbol"] {
        let compares = &c.node(f).unwrap().compares;
        assert!(
            compares.iter().all(|c| c.cases.is_empty() && c.receiver && c.members == ["Unit.CELSIUS", "Unit.FAHRENHEIT"]),
            "{f} matches the value it runs on, each member once: {compares:?}"
        );
    }
    assert_eq!(found(s, "choices"), ["harvest.model.convert"]);
    let compares = &s.node("harvest.model.convert").unwrap().compares;
    assert_eq!(compares.iter().filter(|c| c.subject == "unit" && c.cases == ["fahrenheit"]).count(), 2, "{compares:?}");
}

/// A call leaves something behind. On the clean side the latest reading of a station rests in a
/// place, the `readings` table, written by `ReadingStore.save`. The shortcut also keeps it in a
/// module-level `LATEST` dict that `remember(reading) -> None` fills: a global, the place
/// `Global(harvest.harvester.LATEST)`, which `remember` writes and the API's `latest` reads in
/// another module. Drawn, not asked: a global is sometimes the right choice.
#[test]
fn nothing_a_call_leaves_something_behind() {
    let (c, s) = (clean(), shortcut());
    assert!(found(c, "nothing").is_empty());
    assert!(edge(c, "harvest.harvester.Harvester.collect", "harvest.store.ReadingStore.save", EdgeKind::Calls));
    assert!(touches(c, "harvest.store.ReadingStore.save", "db_sqlite").iter().any(|t| t.artifact == "readings" && t.dir_name() == "write"));
    assert!(edge(s, "harvest.store.ReadingStore.save", "harvest.harvester.remember", EdgeKind::Calls));
    let global =
        |f: &str, dir: &str| touches(s, f, "global").iter().any(|t| t.artifact == "harvest.harvester.LATEST" && t.dir_name() == dir);
    assert!(global("harvest.harvester.remember", "write") && global("harvest.api.latest", "read"));
    assert!(found(s, "nothing").is_empty(), "a call that keeps its value in a global leaves it behind");
    assert!(touches(c, "harvest.harvester.Harvester.collect", "global").is_empty(), "the clean side keeps nothing in a global");
    // the shortcut's `serve` hands the API its database through the environment: a library's
    // value it changes, `os.environ`, not the module `os`, which is code
    let environ = |f: &str, dir: &str| {
        touches(s, f, "global").iter().any(|t| t.artifact == "os.environ" && t.what == "os.environ" && t.dir_name() == dir)
    };
    assert!(environ("harvest.cli.serve", "write") && environ("harvest.api.get_store", "read"));
    assert!(s.nodes.iter().flat_map(|n| n.touches.iter()).all(|t| t.kind != World::Global || t.artifact != "os"), "a module is no place");
    assert!(
        c.nodes.iter().flat_map(|n| n.touches.iter()).all(|t| t.kind != World::Global),
        "the clean side names its database where it runs"
    );
}

/// The packages of a part depend on one another in one direction. On the clean side the harvester
/// builds and calls the store and the store knows nothing of the harvester; the shortcut's
/// `ReadingStore.save` imports `remember` from the harvester inside the function, past the import
/// cycle, while the harvester builds the store.
#[test]
fn layers_the_packages_depend_in_one_direction() {
    let (c, s) = (clean(), shortcut());
    assert!(found(c, "layers").is_empty());
    assert!(edge(c, "harvest.harvester.build_harvester", "harvest.store.ReadingStore", EdgeKind::Instantiates));
    assert!(
        c.edges.iter().all(|e| !(e.src.starts_with("harvest.store.") && e.dst.starts_with("harvest.harvester."))),
        "the store calls nothing of the harvester"
    );
    assert_eq!(found(s, "layers"), ["harvest"]);
    assert!(edge(s, "harvest.harvester.build_harvester", "harvest.store.ReadingStore", EdgeKind::Instantiates));
    assert!(edge(s, "harvest.store.ReadingStore.save", "harvest.harvester.remember", EdgeKind::Calls));
}

/// What a lens puts, it gets back. Python's clean store puts a `Reading` and gets `list[Reading]`;
/// the shortcut's `history` gets the rows back as `list[dict]` (`sqlite3.Row`), since the API
/// returns JSON anyway.
#[test]
fn lens_what_a_lens_puts_it_gets_back() {
    let (c, s) = (clean(), shortcut());
    assert!(found(c, "lens").is_empty());
    assert!(edge(c, "harvest.store.ReadingStore.save", "harvest.model.Reading", EdgeKind::Takes));
    assert!(edge(c, "harvest.store.ReadingStore.history", "harvest.model.Reading", EdgeKind::Returns));
    assert_eq!(found(s, "lens"), ["harvest.store.ReadingStore"]);
    assert!(edge(s, "harvest.store.ReadingStore.save", "harvest.model.Reading", EdgeKind::Takes));
    assert_eq!(s.node("harvest.store.ReadingStore.history").unwrap().returns.as_ref().unwrap().written, "list[dict]");
    assert!(touches(s, "harvest.store.ReadingStore.history", "db_sqlite")
        .iter()
        .any(|t| t.artifact == "readings" && t.dir_name() == "read"));
}

/// What `lens` asks of the store: the types it puts and does not get back; none when it asks nothing.
fn lens_lost(g: &Graph) -> Option<Vec<String>> {
    let f = check(g).into_iter().find(|f| f.principle == "lens" && f.node == "harvest.store.ReadingStore")?;
    Some(f.about("lost").into_iter().map(String::from).collect())
}

/// What a lens puts, it gets back, also inside what a get gives. Both stores keep the outages of a
/// harvest. The clean store gives them back inside a station's `Harvest`, beside its readings; the
/// shortcut's `outages` gives the rows as `list[dict]`.
#[test]
fn lens_what_a_lens_puts_it_gets_back_inside_what_it_gives() {
    let (c, s) = (clean(), shortcut());
    assert!(edge(c, "harvest.store.ReadingStore.save_outage", "harvest.model.Outage", EdgeKind::Takes));
    assert!(edge(c, "harvest.store.ReadingStore.kept", "harvest.model.Harvest", EdgeKind::Returns));
    assert!(edge(c, "harvest.model.Harvest", "harvest.model.Outage", EdgeKind::Has));
    assert_eq!(lens_lost(c), None);
    assert!(edge(s, "harvest.store.ReadingStore.save_outage", "harvest.model.Outage", EdgeKind::Takes));
    assert_eq!(s.node("harvest.store.ReadingStore.outages").unwrap().returns.as_ref().unwrap().written, "list[dict]");
    assert!(lens_lost(s).unwrap_or_default().iter().any(|t| t == "harvest.model.Outage"), "{:?}", lens_lost(s));
}

/// What a lens puts is what reaches the place, not the key it looks a value up by. Both stores
/// summarise a station's day once and keep the summary. The clean `summary(day)` hands its `Day`
/// to `kept_summary`, which reads, and to `day_summary`, whose `Summary` it hands to
/// `keep_summary` to put: the Day is the key the summary is kept under, and what is put comes
/// back. The shortcut's `summary(day)` runs its own `INSERT` inside the lookup, and what a call
/// into the database is handed is not in the graph; it gives a `dict`, and is asked about the Day.
#[test]
fn lens_what_a_lens_puts_is_what_reaches_the_place() {
    let (c, s) = (clean(), shortcut());
    assert!(edge(c, "harvest.store.ReadingStore.summary", "harvest.model.Day", EdgeKind::Takes));
    assert!(edge(c, "harvest.store.ReadingStore.summary", "harvest.store.ReadingStore.keep_summary", EdgeKind::Calls));
    assert!(touches(c, "harvest.store.ReadingStore.keep_summary", "db_sqlite")
        .iter()
        .any(|t| t.artifact == "summaries" && t.dir_name() == "write"));
    assert_eq!(lens_lost(c), None);
    assert!(edge(s, "harvest.store.ReadingStore.summary", "harvest.model.Day", EdgeKind::Takes));
    assert!(touches(s, "harvest.store.ReadingStore.summary", "db_sqlite")
        .iter()
        .any(|t| t.artifact == "summaries" && t.dir_name() == "write"));
    assert!(lens_lost(s).unwrap_or_default().iter().any(|t| t == "harvest.model.Day"), "{:?}", lens_lost(s));
}

/// Every place carries a what: a request names the service it goes to. In Python the clean
/// client takes `base_url` and the entry hands it `API_URL`; the shortcut's client module reads
/// `os.environ.get("WEATHER_API_URL", ...)` at import, so where it asks is said at run time, in
/// library code: what the environment says, or the default the library holds, one input drawn as
/// its default. Whoever starts the program may set the variable, so the default is an option, as a
/// parameter's default is, and `places` does not ask; the shortcut shows in where the address is
/// whole: in the library, not where the program starts.
#[test]
fn a_place_carries_a_what_the_service_url() {
    let s = shortcut();
    let net = touches(s, "harvest.client.WeatherClient.latest", "net");
    assert!(!net.is_empty(), "the request is in the graph");
    assert!(net.iter().all(|t| t.artifact == "api.weather.example"), "one place, the variable its input: {net:?}");
    assert!(
        net.iter().any(|t| t.artifact == "api.weather.example" && !t.fixed && t.fixed_by.is_empty() && t.by.is_empty()),
        "the default, whole in the library: {net:?}"
    );
    assert!(!found(s, "places").contains(&"harvest.client.WeatherClient.latest".to_string()), "{:?}", found(s, "places"));
}

/// The clean client's request goes to the host the entry hands it, through the object it built.
#[test]
fn a_place_carries_a_what_the_url_the_entry_hands_the_client() {
    let net = touches(clean(), "harvest.client.WeatherClient.latest", "net");
    assert!(net.iter().any(|t| t.artifact == "api.weather.example" && t.by.iter().any(|b| b.contains("harvest.cli.collect"))), "{net:?}");
}

/// Every place carries a what, and settings have a type. Python's clean settings are a frozen
/// dataclass read from TOML, and the files are constants the entry hands in; the shortcut
/// `json.load`s its config into a dict that is passed around, and the stations file and the
/// report directory are keys of it (`config["stations_file"]`).
#[test]
fn a_place_carries_a_what_files_named_in_a_config_dict() {
    let (c, s) = (clean(), shortcut());
    assert!(edge(c, "harvest.settings.load_settings", "harvest.settings.Settings", EdgeKind::Returns));
    let stations = touches(c, "harvest.stations.read_stations", "disk_read");
    assert!(stations.iter().any(|t| t.artifact == "data/stations.csv"), "{stations:?}");
    // the shortcut's settings are a dict, and the file read from one of its keys has no name
    let settings = s.node("harvest.settings.load_settings").unwrap();
    assert_eq!(settings.returns.as_ref().unwrap().value, "dict");
    assert!(s.edges.iter().all(|e| !(e.src == settings.id && e.kind == EdgeKind::Returns)), "no type of the program");
    let stations = touches(s, "harvest.stations.read_stations", "disk_read");
    assert!(!stations.is_empty(), "the read is in the graph");
    assert!(stations.iter().all(|t| t.artifact.is_empty() && !t.fixed), "{stations:?}");
}

/// The clean report file is the directory the entry hands down to the base's `export`, which
/// writes through `self.write`, the override of the exporter it runs on.
#[test]
fn a_place_carries_a_what_the_report_file_through_the_exporter_base() {
    for o in ["harvest.export.JsonExporter.write", "harvest.export.CsvExporter.write"] {
        let w = touches(clean(), o, "disk_write");
        assert!(w.iter().any(|t| t.artifact == "reports/"), "{o}: {w:?}");
    }
}

/// A statement names its table whatever values it is built from. Both stores write the
/// `readings` table; the shortcut builds its `INSERT` as an f-string of the reading's fields.
/// The values are no place: in a route of its own, `save` writes `readings` on both sides.
#[test]
fn a_place_carries_a_what_the_table_of_a_statement_built_from_values() {
    // what `save` touches itself, not what the steps it runs touch
    for g in [clean(), shortcut()] {
        let own = flow::of(g).drawn.remove("harvest.store.ReadingStore.save").unwrap_or_default();
        assert!(own.iter().any(|w| w.place.id == "w:SQLite:readings" && w.side == Side::Out && !w.shape), "{own:#?}");
    }
}

/// A route starts at an entry and ends at a visible place. The clean API serves `POST /collect`,
/// which calls `Harvester.collect`; the shortcut serves `POST /tasks/{kind}`, a catch-all that
/// runs `Harvester.run(kind, **kw)`, which calls `getattr(self, kind)(**kw)`.
#[test]
fn a_route_ends_at_a_visible_place() {
    let (c, s) = (clean(), shortcut());
    let (net, table) = ("w:Web:api.weather.example", "w:SQLite:readings");
    let collect = "harvest.api.collect";
    assert!(reaches(c, collect, net, Side::In) && reaches(c, collect, table, Side::Out), "{:?}", places(c, collect));
    assert!(edge(c, "harvest.api.collect", "harvest.harvester.Harvester.collect", EdgeKind::Calls));
    let task = "harvest.api.run_task";
    assert!(!reaches(s, task, net, Side::In) && !reaches(s, task, table, Side::Out), "{:?}", places(s, task));
    assert!(
        s.edges.iter().all(|e| !(e.src == "harvest.harvester.Harvester.run" && e.kind == EdgeKind::Calls)),
        "run calls nothing Atlas follows"
    );
    // the catch-all is a step of the route, and what it may run is still in the graph
    assert!(edge(s, "harvest.api.run_task", "harvest.harvester.Harvester.run", EdgeKind::Calls));
    assert!(s.node("harvest.harvester.Harvester.collect").is_some() && s.node("harvest.harvester.Harvester.report").is_some());
}

/// The call a catch-all makes by name is a step of its body, named as the code writes it, and a
/// call not followed.
#[test]
fn a_route_ends_at_a_visible_place_a_call_by_name_stays_a_step() {
    let s = shortcut();
    let run = s.node("harvest.harvester.Harvester.run").unwrap();
    assert!(run.sites.iter().any(|x| x.op == "getattr(self, kind)"), "{:#?}", run.sites);
    assert!(not_followed(s, "harvest.harvester.Harvester.run", "getattr(self, kind)"), "{:#?}", run.sites);
}

/// Zoom is a quotient by base type and by composition: a class holds its parts by their types.
/// Python's clean `Harvester` is a dataclass of a `WeatherClient`, a `ReadingStore` and an
/// `Exporter`; the shortcut's fields are `Any`, so fakes can stand in without a base type.
#[test]
fn zoom_is_a_quotient_by_composition() {
    let (c, s) = (clean(), shortcut());
    for (part, via) in
        [("harvest.client.WeatherClient", "client"), ("harvest.store.ReadingStore", "store"), ("harvest.export.Exporter", "exporter")]
    {
        assert!(
            c.edges.iter().any(|e| e.kind == EdgeKind::Has
                && e.src == "harvest.harvester.Harvester"
                && e.dst == part
                && e.via.as_deref() == Some(via)),
            "{part}"
        );
    }
    assert!(edge(c, "harvest.harvester.Harvester.collect", "harvest.client.WeatherClient.latest", EdgeKind::Calls));
    let (net, table, collect) = ("w:Web:api.weather.example", "w:SQLite:readings", "harvest.cli.collect");
    assert!(reaches(c, collect, net, Side::In) && reaches(c, collect, table, Side::Out), "{:?}", places(c, collect));
    // the shortcut's harvester holds nothing Atlas can name, and its calls on its parts are not followed
    assert!(s.edges.iter().all(|e| !(e.kind == EdgeKind::Has && e.src == "harvest.harvester.Harvester")));
    assert!(!edge(s, "harvest.harvester.Harvester.collect", "harvest.client.WeatherClient.latest", EdgeKind::Calls));
    let harvester = "harvest.harvester.Harvester.collect";
    assert!(not_followed(s, harvester, "self.client.latest") && not_followed(s, harvester, "self.store.save"));
    assert!(!reaches(s, collect, net, Side::In) && !reaches(s, collect, table, Side::Out), "{:?}", places(s, collect));
    // the parts still touch their places
    assert!(!touches(s, "harvest.client.WeatherClient.latest", "net").is_empty());
    assert!(touches(s, "harvest.store.ReadingStore.save", "db_sqlite").iter().any(|t| t.artifact == "readings"));
}

/// The calls the shortcut's harvester makes on its `Any` parts are steps of its body, with no
/// target Atlas can name; beside them, `convert` and the `append` that gathers the results.
#[test]
fn zoom_is_a_quotient_by_composition_a_call_on_any_stays_a_step() {
    let collect = shortcut().node("harvest.harvester.Harvester.collect").unwrap();
    assert_eq!(
        collect.sites.len(),
        5,
        "self.client.latest, convert, self.store.save, self.store.save_outage, results.append: {:#?}",
        collect.sites
    );
}

/// One side of the collection pair (`tests/pairs/collections`): a store that reads its rows,
/// appends one and writes them back, in Python, Rust and TypeScript alike.
fn gathering(side: &str) -> Graph {
    analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/collections/python").join(side), &Options::default()).unwrap()
}

/// What a call changes in place, it gives: `rows.append(Row(reading))` is a step that makes `rows`
/// from what it is handed, so the Reading reaches what `write_rows(self.path, rows)` puts. The
/// helper's `path` and the store's `self.path` are one value, the path the store is made with, so
/// they are one place: `Disk(Store.path)`. Both stores read their
/// rows, append the new one and write them back. The clean `get` gives the Readings back; the
/// shortcut's gives the lines, and `lens` asks where the Reading went. `DroppedStore` appends to
/// another list and writes the rows as they were: it puts no Reading and is not asked, also where it
/// takes `readings: list[Reading]` and appends in a loop (`put_all`): the loop variable is an
/// element of the parameter, which is followed through it. A method of the program's own that
/// returns a new value (`Frozen.append`) and an observation whose result is dropped
/// (`path.exists()`) change nothing.
#[test]
fn lens_what_is_appended_to_what_is_put_is_put() {
    let (c, s) = (gathering("clean"), gathering("shortcut"));
    let fs = check(&c);
    assert!(fs.is_empty(), "{:#?}", fs.iter().map(|f| format!("{} {} {}", f.principle, f.node, f.what)).collect::<Vec<_>>());
    let put = s.node("gathering.store.Store.put").unwrap();
    assert!(
        put.sites.iter().any(|x| x.op == "append" && x.hands.iter().any(|h| h.name == "rows") && x.gives.iter().any(|g| g.name == "rows")),
        "{:?}",
        put.sites
    );
    assert_eq!(found(&s, "lens"), ["gathering.store.Store"]);
    for f in ["gathering.store.write_rows", "gathering.store.Store.get"] {
        assert!(s.node(f).unwrap().touches.iter().any(|t| t.what == "Store.path"), "{f}: {:?}", s.node(f).unwrap().touches);
    }
    let observe = s.node("gathering.store.observe").unwrap();
    assert!(observe.sites.iter().flat_map(|x| &x.gives).all(|g| g.name != "frozen" && g.name != "path"), "{:?}", observe.sites);
}

/// A call is handed the values the names in its arguments hold, `self` among them: `write_rows(self.path,
/// rows)` in `Store.put` is handed the store the method runs on with the rows, the projection `.path`
/// glued into the call. A method called on `self` itself runs on that store, which its step says, and is
/// handed nothing for it. A call in an argument that is a step of its own takes its names itself:
/// `Message(level.prefix() + job.name)` is handed what `prefix` gives and `job`, not `level`.
#[test]
fn a_call_is_handed_the_values_the_names_in_its_arguments_hold() {
    for side in ["clean", "shortcut"] {
        let g = gathering(side);
        let put = g.node("gathering.store.Store.put").unwrap();
        let hands = |to: &str| put.sites.iter().find(|x| x.to == to).map(|x| x.hands.clone()).unwrap_or_default();
        let store = "gathering.store.Store";
        assert!(hands("gathering.store.write_rows").iter().any(|h| h.ty == store), "{side}: {:?}", put.sites);
        assert!(hands("gathering.store.write_rows").iter().any(|h| h.name == "rows"), "{side}: {:?}", put.sites);
        let own = put.sites.iter().find(|x| x.to == "gathering.store.Store._rows").unwrap();
        assert!(own.on_receiver && own.hands.iter().all(|h| h.ty != store), "{side}: {:?}", own);
    }
    let c = choices_side("clean");
    let report = c.node("runs.core.report").unwrap();
    for message in report.sites.iter().filter(|x| x.to == "runs.core.Message") {
        assert!(message.hands.iter().all(|h| h.name != "level"), "{:?}", message.hands);
        assert!(message.hands.iter().any(|h| h.name == "job"), "{:?}", message.hands);
    }
    assert!(report.sites.iter().any(|x| x.to == "runs.core.Level.prefix" && x.hands.iter().any(|h| h.name == "level")));

    // a projection bound to a name first is still the projection, a lookup among them; a name a
    // `lambda` binds is no value of the body
    let g = program(
        "projection",
        "from dataclasses import dataclass\n\n\n@dataclass\nclass Settings:\n    width: int\n\n\n@dataclass\nclass Shape:\n    w: int\n\n\n\
         def draw(w: int) -> Shape:\n    return Shape(w)\n\n\n\
         def run(settings: Settings, table: dict[str, Settings]) -> list[Shape]:\n    width = settings.width\n    a = draw(width)\n    \
         row = table[\"x\"]\n    b = draw(row.width)\n    c = sorted([a, b], key=lambda s: s.w)\n    return c\n",
    );
    let run = g.node("demo.core.run").unwrap();
    let hands: Vec<Vec<String>> = run
        .sites
        .iter()
        .map(|x| x.hands.iter().map(|h| format!("{}·{}", h.ty.rsplit('.').next().unwrap_or(""), h.name)).collect())
        .collect();
    assert_eq!(hands, [vec!["Settings·settings"], vec!["Settings·table"], vec!["Shape·a", "Shape·b"]]);
}

/// A value a body binds without a call is made in the body from what it holds, never a value from
/// beyond: a name bound in the arms of an `if` holds what each arm binds it to, a conditional
/// expression either of its parts, an operator what it computes the value from (`a.width -
/// b.width`, as TypeScript's `height - margin`), and a name a `case` pattern binds is a part of what the `match`
/// takes apart, through the variant the case matches. A call handed such a name is handed what it
/// is a projection of: `describe`'s `Report(.., [s])` in `case Fit(score=s)` is handed the sum `r`.
#[test]
fn composition_a_name_bound_without_a_call_holds_what_it_is_bound_to() {
    let g = program(
        "bound",
        "from dataclasses import dataclass\n\n\n@dataclass\nclass Settings:\n    width: int\n\n\n@dataclass\nclass Shape:\n    w: int\n\n\n\
         def draw(w: int) -> Shape:\n    return Shape(w)\n\n\n\
         def pick(a: Settings, b: Settings, wide: bool) -> Shape:\n    if wide:\n        w = a.width\n    else:\n        w = b.width\n    \
         return draw(w)\n\n\n\
         def either(a: Settings, b: Settings, wide: bool) -> Shape:\n    w = a.width if wide else b.width\n    return draw(w)\n\n\n\
         def less(a: Settings, b: Settings) -> Shape:\n    w = a.width - b.width\n    return draw(w)\n",
    );
    for f in ["pick", "either", "less"] {
        let node = g.node(&format!("demo.core.{f}")).unwrap();
        let draw = node.sites.iter().find(|s| s.to == "demo.core.draw").unwrap();
        let mut names: Vec<&str> = draw.hands.iter().map(|h| h.name.as_str()).collect();
        names.sort();
        assert_eq!(names, ["a", "b"], "{f}: {:?}", draw.hands);
    }
    let describe = fits("clean").node("fits.report.describe").unwrap().clone();
    for report in describe.sites.iter().filter(|s| s.to == "fits.report.Report") {
        assert!(report.hands.iter().all(|h| h.name == "r") && !report.hands.is_empty(), "{:?}", report.hands);
    }
}

/// A program of one module, `demo/core.py` holding `code`, read as a repository of its own.
fn program(name: &str, code: &str) -> Graph {
    let root = std::env::temp_dir().join(format!("atlas-python-{name}-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src/demo")).expect("makes the package");
    std::fs::write(root.join("pyproject.toml"), "[project]\nname = \"demo\"\nversion = \"0\"\n").expect("writes the manifest");
    std::fs::write(root.join("src/demo/__init__.py"), "").expect("writes the package");
    std::fs::write(root.join("src/demo/core.py"), code).expect("writes the module");
    let g = analyze(&root, &Options::default()).expect("reads");
    let _ = std::fs::remove_dir_all(&root);
    g
}

/// One side of the sum pair (`tests/pairs/sums`): a knowledge base that adds a source and gives
/// back the graph of its nodes, in Python, Rust and TypeScript alike.
fn kennis(side: &str) -> Graph {
    analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/sums/python").join(side), &Options::default()).unwrap()
}

/// What a lens puts may come back as a variant of a sum a get gives: putting one node is the put of
/// a lens composed of the key that picks it, the list that holds it and the variant it is (a
/// prism). A named sum is the sum it names: `nodes: list[Node]` with `Node = Begrip | Bron` holds
/// a Bron as `list[Begrip | Bron]` does. The clean store gives the `Graaf` back and is asked
/// nothing; the shortcut gives rows, and `lens` asks where the Bron went.
#[test]
fn lens_what_is_put_comes_back_as_a_variant_of_a_named_sum() {
    let (c, s) = (kennis("clean"), kennis("shortcut"));
    let fs = check(&c);
    assert!(fs.is_empty(), "{:#?}", fs.iter().map(|f| format!("{} {} {}", f.principle, f.node, f.what)).collect::<Vec<_>>());
    assert!(edge(&c, "kennis.bank.Graaf", "kennis.bank.Node", EdgeKind::Has));
    assert!(edge(&c, "kennis.bank.Node", "kennis.bank.Bron", EdgeKind::Variant));
    assert_eq!(found(&s, "lens"), ["kennis.bank.Kennisbank"]);
}

/// Not a shortcut: both sides make their tables' shape each time they connect
/// (`CREATE TABLE IF NOT EXISTS`), which puts no content there, so what a store reads after it is
/// what it finds. Both keep a day's summary as a cache: the summary kept is read, and one made is
/// written where none was, so the route finds `summaries` and changes it.
#[test]
fn a_table_made_on_connecting_holds_what_was_there() {
    for (name, g) in [("clean", clean()), ("shortcut", shortcut())] {
        let tables: Vec<(&str, &str)> = touches(g, "harvest.store.ReadingStore.connect", "db_sqlite")
            .iter()
            .filter(|t| !t.artifact.is_empty())
            .map(|t| (t.op.as_str(), t.artifact.as_str()))
            .collect();
        assert_eq!(tables, [("schema", "readings"), ("schema", "outages"), ("schema", "summaries")], "{name}");
        let arrows = atlas_engine::flow::arrows(g);
        for f in ["harvest.store.ReadingStore.history", "harvest.store.ReadingStore.summary"] {
            assert!(arrows.get(f).map_or(true, |a| a.reads_back.is_empty()), "{name} {f}: {:?}", arrows.get(f));
        }
        // the summary kept is read before one is made and kept: the cache is used and changed
        let summary = arrows.get("harvest.store.ReadingStore.summary").map(|a| a.updates.clone()).unwrap_or_default();
        assert!(summary.iter().any(|p| p == "w:SQLite:summaries"), "{name}: {summary:?}");
    }
}

/// A place is the path it was built from. The clean archive reader is handed `data/archive` by
/// the entry and lists `*/station-*` below it: the place is that pattern below that directory.
/// The shortcut's directories come out of a config dict: the pattern is still written out, and
/// the root stays a hole in the term by the name the code gives it (`{d}/*/station-*`), not a
/// guess and not a wildcard; no directory decides the place.
#[test]
fn a_place_carries_a_what_a_pattern_below_a_directory_the_entry_hands_down() {
    let clean = touches(clean(), "harvest.stations.archived_stations", "disk_read");
    assert!(clean.iter().any(|t| t.artifact == "data/archive/*/station-*"), "{clean:?}");
    let shortcut = touches(shortcut(), "harvest.stations.archived_stations", "disk_read");
    assert!(!shortcut.is_empty(), "the listing is in the graph");
    assert!(shortcut.iter().all(|t| t.artifact == "{d}/*/station-*" && !t.fixed), "{shortcut:?}");
    assert!(shortcut.iter().all(|t| t.what == "{d}/*/station-*"), "{shortcut:?}");
}

/// The places `demo.core.run` of a one-module program touches, each with its kind, the program its
/// imports, the constants `DATA` and `KINDS`, and `run`'s body.
fn run_places(name: &str, body: &str) -> BTreeSet<(String, String)> {
    let code = format!(
        "import os\nfrom pathlib import Path\n\nDATA = Path(\"data\")\nKINDS = {{\"a\": \"regelingen\", \"b\": \"ontwerp\"}}\n\n\ndef run(kind: str) -> None:\n{body}\n"
    );
    let g = program(name, &code);
    let run = g.node("demo.core.run").unwrap_or_else(|| panic!("{name}: run is in the graph"));
    run.touches.iter().filter(|t| t.kind_name() != "screen").map(|t| (t.kind_name().to_string(), t.artifact.clone())).collect()
}

/// Each element of a listing of a directory is the pattern `*` below it (`docs/reference/mathematics.md`,
/// "Paths as terms"): `iterdir()` yields what `glob("*")` yields, and so do `os.listdir`, whose names
/// the code joins back below the directory, and `os.scandir`, whose entries are at their `path`. A
/// term built the four ways is one place: the directory listed, and the file read in each folder.
#[test]
fn a_place_carries_a_what_a_listing_is_a_pattern_below_its_directory() {
    let glob = run_places("listing-glob", "    for p in DATA.glob(\"*\"):\n        print((p / \"x.txt\").read_text())");
    assert!(glob.contains(&("disk_read".into(), "data/*/x.txt".into())), "{glob:?}");
    for (name, body) in [
        ("listing-iterdir", "    for p in DATA.iterdir():\n        print((p / \"x.txt\").read_text())"),
        ("listing-listdir", "    for n in os.listdir(DATA):\n        print(open(os.path.join(DATA, n, \"x.txt\")).read())"),
        ("listing-scandir", "    for e in os.scandir(DATA):\n        print(open(os.path.join(e.path, \"x.txt\")).read())"),
    ] {
        assert_eq!(run_places(name, body), glob, "{name}");
    }
}

/// A function a call runs on each element of a collection is handed the elements: the parameter of
/// a sort key (`sorted`, `max`, `list.sort`) and of `map` is one of the paths the collection holds,
/// as the name a loop over it binds is, and what `max`, `next` or `sorted(..)[-1]` picks of it is one
/// of them too. Each form reads at the place the loop reads.
#[test]
fn a_place_carries_a_what_a_sort_keys_parameter_is_an_element_of_what_it_orders() {
    let each = run_places("each-loop", "    for p in DATA.glob(\"*.csv\"):\n        print(p.stat().st_size, p.read_text())");
    assert_eq!(each, BTreeSet::from([("disk_read".to_string(), "data/*.csv".to_string())]));
    for (name, body) in [
        ("each-sorted", "    print(sorted(DATA.glob(\"*.csv\"), key=lambda p: p.stat().st_size)[-1].read_text())"),
        ("each-max", "    print(max(DATA.glob(\"*.csv\"), key=lambda p: p.stat().st_size).read_text())"),
        (
            "each-sort",
            "    found = list(DATA.glob(\"*.csv\"))\n    found.sort(key=lambda q: q.stat().st_size)\n    print(found[0].read_text())",
        ),
        ("each-map", "    print(list(map(lambda p: p.stat().st_size, DATA.glob(\"*.csv\"))), next(DATA.glob(\"*.csv\")).read_text())"),
    ] {
        assert_eq!(run_places(name, body), each, "{name}");
    }
}

/// `d[k]`, `d` a mapping the code writes out, is one of the values `d` holds where no code decides
/// `k`, as `d[k]` for each `k` of `d` is; a key the code writes picks its own value, the path written
/// with that value.
#[test]
fn a_place_carries_a_what_a_key_no_code_decides_is_one_of_a_mappings_values() {
    let every = run_places("mapping-every", "    for k in KINDS:\n        print((DATA / KINDS[k]).read_text())");
    assert_eq!(every.len(), 2, "{every:?}");
    assert_eq!(run_places("mapping-handed", "    print((DATA / KINDS[kind]).read_text())"), every);
    let written = run_places("mapping-written", "    print((DATA / \"regelingen\").read_text())");
    assert_eq!(run_places("mapping-known", "    print((DATA / KINDS[\"a\"]).read_text())"), written);
    assert!(written.is_subset(&every), "{written:?} in {every:?}");
}

// ---------- through: what a call runs is one answer ----------

/// The through pair (`tests/pairs/through/python`): `report` reads a heading from a file and shows it on a section, a
/// `Boxed` section among them. The clean side calls `section.show(heading)`, which on a boxed section runs `Boxed.show`;
/// the shortcut calls `Section.show(section, heading)`, a call through a class, which runs `Section.show` and no override
/// of it (`mathematics.md`, "Method resolution order"), so a boxed section is never shown boxed.
fn through() -> &'static Pair {
    static PAIR: OnceLock<Pair> = OnceLock::new();
    PAIR.get_or_init(|| common::pair("through/python"))
}

/// What a call runs is one answer (#263): the route's steps, what a parameter holds and what a call hands on all ask
/// which functions a call may run, and each follows the calls the route draws. On each side the test computes the three
/// and compares them: the call of `report` may run `Boxed.show` in its place exactly where `Boxed.show`'s `heading`
/// holds the heading `report` read (the body reads it, and tests it) and exactly where the call hands on the error
/// `Boxed.show` raises. The clean side's dispatch runs the override, the shortcut's call through the class does not, and
/// a parameter only that call fills holds no value.
#[test]
fn through_a_call_runs_the_same_functions_for_the_route_its_parameters_and_its_errors() {
    let boxed = "notes.sections.Boxed.show";
    for (side, g) in [("clean", &through().clean), ("shortcut", &through().shortcut)] {
        let bodies = flow::bodies(g);
        let call = bodies["notes.report.report"].steps().into_iter().find(|s| s.to == "notes.sections.Section.show").expect("report shows");
        let runs = call.alts(&bodies).iter().any(|k| flow::split(k).0 == boxed);
        let holds = g.node(boxed).unwrap().uses.iter().any(|u| u.name == "heading");
        let site = g.node("notes.report.report").unwrap().sites.iter().find(|s| s.to == "notes.sections.Section.show").unwrap();
        let hands_on = site.fails.iter().any(|e| e == "ext:builtins.ValueError");
        assert_eq!((holds, hands_on), (runs, runs), "{side}: the parameter and the error follow what the call runs");
        assert_eq!(runs, side == "clean", "{side}");
    }
}

// ---------- a library's facts hold where ty finds that library ----------

/// The files `id` touches, each by its kind and the term its path is (`~/data/{name}`).
fn terms(g: &Graph, id: &str) -> BTreeSet<(String, String)> {
    let n = g.node(id).unwrap_or_else(|| panic!("{id} is in the graph"));
    n.touches.iter().filter(|t| t.kind_name() != "screen").map(|t| (t.kind_name().to_string(), t.what.clone())).collect()
}

/// A subject of its own, `files` written below a fresh directory, read, and removed.
fn subject(name: &str, files: &[(&str, &str)]) -> Graph {
    let root = std::env::temp_dir().join(format!("atlas-python-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for (path, text) in files {
        let file = root.join(path);
        std::fs::create_dir_all(file.parent().expect("has a parent")).expect("makes the directory");
        std::fs::write(file, text).expect("writes the file");
    }
    let g = analyze(&root, &Options::default());
    let _ = std::fs::remove_dir_all(&root);
    g.unwrap_or_else(|e| panic!("{name} reads: {e}"))
}

/// A path method holds where ty finds the call goes into the path's library
/// (`knowledge::PYTHON_PATH_METHODS`), never by its name alone. The receivers pair
/// (`tests/pairs/receivers/python`) finds a data file below the directory `data_dir()` gives and
/// writes its count beside it. The clean side annotates `data_dir() -> Path`, so `joinpath` and
/// `with_suffix` are pathlib's and the places are the terms the code builds. The shortcut leaves the
/// annotations out: ty does not know what `data_dir()` gives, so nothing says `.joinpath(name)` is a
/// path's, and the place stays a hole by the name the code gives it rather than a guess. The calls
/// that open the files are in both graphs.
#[test]
fn places_a_path_method_holds_where_ty_finds_a_path() {
    let p = common::pair("receivers/python");
    let summarise = "shelf.summary.summarise";
    let clean = terms(&p.clean, summarise);
    for (kind, what) in [
        ("disk_read", "~/data/{name}"),
        ("disk_read", "{$SHELF_DATA}/{name}"),
        ("disk_write", "~/data/{name}.count"),
        ("disk_write", "{$SHELF_DATA}/{name}.count"),
    ] {
        assert!(clean.contains(&(kind.to_string(), what.to_string())), "{kind} {what} in {clean:?}");
    }
    let shortcut = terms(&p.shortcut, summarise);
    assert!(shortcut.iter().any(|(k, _)| k == "disk_read") && shortcut.iter().any(|(k, _)| k == "disk_write"), "{shortcut:?}");
    assert!(shortcut.iter().all(|(_, what)| !what.contains("data")), "no place is guessed from a method's name: {shortcut:?}");
}

/// A path's attribute holds where ty finds the attribute is the path library's
/// (`knowledge::PYTHON_PATH_ATTRIBUTES`), never by its name alone. In the receivers pair, `log`
/// appends to a log in the directory above the data file it is handed (`source.parent`), and
/// `shelved` opens the file an `Entry` names by the shelf it stands on and its name there
/// (`entry.parent`, `entry.name`): the project's own fields, named as pathlib's attributes are. The
/// clean side annotates `source: Path`, so its `parent` is the directory above the file; the shortcut
/// leaves it out, ty does not know what `source` is, and the directory stays a hole by the name the
/// code gives it rather than a guess. On both sides the entry's `parent` is its field, never the
/// directory above the entry.
#[test]
fn places_a_path_attribute_holds_where_ty_finds_a_path() {
    let p = common::pair("receivers/python");
    let clean = terms(&p.clean, "shelf.summary.log");
    assert!(clean.contains(&("disk_write".to_string(), "{source}/../shelf.log".to_string())), "{clean:?}");
    let shortcut = terms(&p.shortcut, "shelf.summary.log");
    assert!(shortcut.iter().any(|(k, _)| k == "disk_write"), "{shortcut:?}");
    assert!(
        shortcut.iter().all(|(_, what)| !what.contains("{source}/..")),
        "no directory is guessed from an attribute's name: {shortcut:?}"
    );
    for (side, g) in [("clean", &p.clean), ("shortcut", &p.shortcut)] {
        let shelved = terms(g, "shelf.summary.shelved");
        assert!(shelved.iter().any(|(k, _)| k == "disk_read"), "{side}: {shelved:?}");
        assert!(shelved.iter().all(|(_, what)| !what.contains("..")), "{side}: a field named `parent` is no directory up: {shelved:?}");
    }
}

/// A class and the attributes of the project's own are the project's, whatever their names: a
/// dataclass named as pathlib's class, with fields named as its attributes (`Path`, `parent`,
/// `name`, `parents`), reads what the same dataclass with other names reads, and nothing it reads is
/// a directory above another.
#[test]
fn places_a_project_attribute_named_as_a_librarys_is_the_projects() {
    let stop = |class: &str, before: &str, name: &str, ancestors: &str| {
        format!(
            "from dataclasses import dataclass\n\n\n@dataclass(frozen=True)\nclass {class}:\n    {before}: str\n    {name}: str\n    {ancestors}: list[str]\n\n\ndef run(stop: {class}) -> None:\n    print(open({class}(stop.{before}, \"index.html\", []).{name}).read())\n    print(open(stop.{before} + \"/\" + stop.{name}).read())\n    print(open(stop.{ancestors}[1] + \"/index.html\").read())\n"
        )
    };
    let named = terms(&program("own-path", &stop("Path", "parent", "name", "parents")), "demo.core.run");
    let other = terms(&program("own-stop", &stop("Stop", "before", "label", "earlier")), "demo.core.run");
    // the same terms, holes named by the fields the code writes
    let renamed: BTreeSet<(String, String)> = other
        .into_iter()
        .map(|(k, what)| (k, what.replace("{before}", "{parent}").replace("{earlier}", "{parents}").replace("{label}", "{name}")))
        .collect();
    assert_eq!(named, renamed);
    assert!(named.iter().all(|(_, what)| !what.contains("..")), "{named:?}");
}

/// A method of the project's own is the project's, whatever its name: a class whose methods are
/// named as pathlib's (`home`, `with_name`) reads what a class with other names for the same
/// methods reads, and neither is the user's home.
#[test]
fn places_a_project_method_named_as_a_librarys_is_the_projects() {
    let site = |home: &str, sibling: &str| {
        format!(
            "class Site:\n    def {home}(self) -> str:\n        return \"index.html\"\n\n    def {sibling}(self, name: str) -> str:\n        return \"pages/\" + name\n\n\ndef run(site: Site) -> None:\n    print(open(site.{home}()).read())\n    print(open(site.{sibling}(\"about.html\")).read())\n"
        )
    };
    let named = terms(&program("own-named", &site("home", "with_name")), "demo.core.run");
    let other = terms(&program("own-other", &site("index", "page")), "demo.core.run");
    // the same term, a hole named by the call the code writes
    let renamed: BTreeSet<(String, String)> =
        other.into_iter().map(|(k, what)| (k, what.replace(".index(", ".home(").replace(".page(", ".with_name("))).collect();
    assert_eq!(named, renamed);
    assert!(named.iter().all(|(_, what)| !what.starts_with('~')), "{named:?}");
}

/// A notebook reads as the script its code cells make, its IPython escape commands aside
/// (`%matplotlib`, `!pip`): only what ruff parses as an escape command is blanked. A line Black wraps
/// before a binary operator (`    % len(..)`, `    != ..`) is Python, and the notebook reads the files
/// the same code reads as a script.
#[test]
fn a_notebook_reads_as_the_script_its_cells_make() {
    let code = "report = (\n    \"rows: %d\"\n    % len(open(\"data/rows.csv\").read().splitlines())\n)\nchanged = (\n    open(\"data/old.csv\").read()\n    != open(\"data/new.csv\").read()\n)\nprint(report, changed)\n";
    let cell = |source: &str| {
        let lines: Vec<String> = source.split_inclusive('\n').map(|l| serde_json::to_string(l).expect("a string")).collect();
        format!(
            "{{\"cell_type\": \"code\", \"execution_count\": null, \"metadata\": {{}}, \"outputs\": [], \"source\": [{}]}}",
            lines.join(", ")
        )
    };
    let notebook = format!(
        "{{\"cells\": [{}, {}], \"metadata\": {{}}, \"nbformat\": 4, \"nbformat_minor\": 5}}",
        cell("%matplotlib inline\n!pip install rows\n"),
        cell(code)
    );
    let manifest = "[project]\nname = \"rows\"\nversion = \"0\"\n";
    let g = subject(
        "notebook-escapes",
        &[("pyproject.toml", manifest), ("src/rows/__init__.py", ""), ("notebooks/report.ipynb", &notebook), ("scripts/report.py", code)],
    );
    let (as_notebook, as_script) = (terms(&g, "notebooks.report"), terms(&g, "scripts.report"));
    assert_eq!(as_notebook, as_script);
    for file in ["data/rows.csv", "data/old.csv", "data/new.csv"] {
        assert!(as_notebook.contains(&("disk_read".to_string(), file.to_string())), "{file} in {as_notebook:?}");
    }
}

/// A script or a notebook that does not load is left out with a warning, and the rest of the
/// subject is read as it is without it: a notebook whose JSON does not parse fails nothing.
#[test]
fn a_notebook_that_does_not_load_is_left_out() {
    let files = [
        ("pyproject.toml", "[project]\nname = \"rows\"\nversion = \"0\"\n"),
        ("src/rows/__init__.py", "def count(path: str) -> int:\n    return len(open(path).read().splitlines())\n"),
        ("scripts/run.py", "from rows import count\n\nprint(count(\"data/rows.csv\"))\n"),
    ];
    let ids = |g: &Graph| g.nodes.iter().map(|n| n.id.clone()).collect::<BTreeSet<_>>();
    let without = subject("notebook-without", &files);
    let mut broken = files.to_vec();
    broken.push(("notebooks/broken.ipynb", "{\"cells\": [ not json"));
    let with = subject("notebook-broken", &broken);
    assert_eq!(ids(&with), ids(&without));
    assert_eq!(terms(&with, "scripts.run"), terms(&without, "scripts.run"));
}

// ---------- prophecy ----------

/// One side of the prophecy pair (`tests/pairs/prophecy`): a report `Sink` that only writes, and
/// the `Store` that will replace it, sketched without bodies.
fn archive_python(side: &str) -> Graph {
    analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/prophecy/python").join(side), &Options::default()).expect("reads")
}

/// Sketch code kept in the tree says it is not built: Atlas reads a class marked with the `@prophecy` decorator
/// as a prophecy that supersedes the sink, and the sink as built. The shortcut writes the same
/// sketch without the mark: Atlas cannot tell it from code that is finished, and does not pretend to.
#[test]
fn prophecy_a_sketch_in_the_tree_says_it_is_not_built() {
    let (c, s) = (archive_python("clean"), archive_python("unmarked"));
    let store = |g: &Graph| {
        g.nodes
            .iter()
            .find(|n| n.id.ends_with(".sink.Store") || n.id == "sink.Store")
            .unwrap_or_else(|| panic!("a Store in {:?}", g.nodes.iter().map(|n| &n.id).collect::<Vec<_>>()))
            .clone()
    };
    let sketch = store(&c).prophecy.expect("the marked sketch is a prophecy");
    assert_eq!(sketch.supersedes.as_deref(), Some("archive.sink.Sink"));
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
    let (c, s, u) = (archive_python("clean"), archive_python("shortcut"), archive_python("unmarked"));
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
    let (c, s, u) = (archive_python("clean"), archive_python("shortcut"), archive_python("unmarked"));
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
        "python",
        "src/archive/sink.py",
        "\n\nclass Vault:\n    \"\"\"Where reports wait.\"\"\"\n\n    def keep(self, report: str, /) -> None: ...\n\n    def size(self) -> int: ...\n",
        &[("archive.sink.Vault", "archive.sink.Sink")],
    );
    let text = decided.files.values().next().expect("one file is written");
    assert!(text.contains("@prophecy(supersedes=\"archive.sink.Sink\")"), "{text}");
    // ty refuses an empty body where a value is given: the sketch raises, and a body that gives nothing stays empty
    assert!(text.contains("def size(self) -> int:\n        raise NotImplementedError\n"), "{text}");
    assert!(text.contains("def keep(self, report: str, /) -> None: ..."), "{text}");
    assert_eq!(decided.marked, ["archive.sink.Vault"]);
    let vault = stands.iter().find(|s| s.id == "archive.sink.Vault").expect("the revelation lists what was decided");
    assert_eq!(vault.supersedes.as_deref(), Some("archive.sink.Sink"));
    assert!(vault.holds, "{vault:?}");
    assert_eq!(stands.len(), 3, "the two prophecies of the clean side, and the new one: {stands:?}");
}

/// A prophecy is the vision written into the tree, and draws the shape the vision drew. Python's
/// marker is a call, `@prophecy()` where nothing is superseded: the project's `prophecy` gives the
/// decorator, so a bare `@prophecy` would make the class what the call gives, and `Crate(Sink())`
/// would be typed as the `Sink` it is handed. The vision adds a `Crate` and a `run_crated` that
/// writes into it; written as prophecy, `run_crated` still calls `Crate.write`, as it did in the
/// vision, and not `Sink.write`.
#[test]
fn prophecy_draws_the_shape_of_its_vision() {
    use std::collections::{BTreeMap, BTreeSet};
    let copy = common::clean_copy("python", "shape");
    let file = PathBuf::from("src/archive/app.py");
    let now = std::fs::read_to_string(copy.join(&file)).expect("reads");
    let addition = "\n\nclass Crate:\n    \"\"\"Reports kept in a crate before they reach the sink.\"\"\"\n\n    def __init__(self, sink: Sink, /) -> None: ...\n\n    def write(self, report: str, /) -> None: ...\n\n\ndef run_crated(report: str) -> None:\n    Crate(Sink()).write(report)\n";
    let vision = BTreeMap::from([(file.clone(), Some(format!("{now}{addition}")))]);
    let supersedes = BTreeMap::from([("archive.app.run_crated".to_string(), "archive.app.run".to_string())]);
    let calls = |gs: &[Graph]| -> BTreeSet<String> {
        gs.iter()
            .flat_map(|g| g.edges.iter())
            .filter(|e| e.kind == EdgeKind::Calls && e.src == "archive.app.run_crated")
            .map(|e| e.dst.clone())
            .collect()
    };
    let envisioned = calls(&atlas_engine::changes::envisioned(&copy, &vision, &Options::default()).expect("reads the vision").parts);
    let decided = atlas_engine::decide::prophetize(&copy, &vision, &supersedes, &Options::default()).expect("decided");
    for (rel, t) in &decided.files {
        std::fs::write(copy.join(rel), t).expect("writes the file");
    }
    let written = calls(&[analyze(&copy, &Options::default()).expect("reads")]);
    let _ = std::fs::remove_dir_all(copy.parent().expect("has a parent"));
    assert!(envisioned.contains("archive.app.Crate.write"), "{envisioned:?}");
    assert_eq!(written, envisioned, "the prophecy calls what the vision called");
    let text = &decided.files[&file];
    assert!(text.contains("\n@prophecy()\nclass Crate:"), "{text}");
}

/// The import of `prophecy` that a decided vision adds can go in a circle: where `prophecy` is
/// defined in a module that imports the file it marks, neither module imports. Atlas cannot know
/// every safe place, and says so. The clean side keeps `prophecy` in an `__init__` that imports
/// nothing of the project: the import goes into the file's import block, and nothing is said. The
/// shortcut's `__init__` also imports `archive.sink`, the file the vision writes into.
#[test]
fn prophecy_says_when_its_import_goes_in_a_circle() {
    use std::collections::BTreeMap;
    let decide = |init_imports_sink: bool| {
        let copy = common::clean_copy("python", if init_imports_sink { "circle" } else { "line" });
        if init_imports_sink {
            let init = copy.join("src/archive/__init__.py");
            let text = std::fs::read_to_string(&init).expect("reads");
            std::fs::write(&init, format!("from archive.sink import Sink\n{text}")).expect("writes");
        }
        let file = PathBuf::from("src/archive/sink.py");
        // the vision is text with `\n` line ends, whatever line ends the checkout gives the file
        let now = std::fs::read_to_string(copy.join(&file))
            .expect("reads")
            .replace("\r\n", "\n")
            .replace("from archive import prophecy\n", "import json\n");
        let vision =
            BTreeMap::from([(file.clone(), Some(format!("{now}\n\nclass Crate:\n    def pack(self, report: str, /) -> None: ...\n")))]);
        let decided = atlas_engine::decide::prophetize(&copy, &vision, &BTreeMap::new(), &Options::default()).expect("decided");
        let _ = std::fs::remove_dir_all(copy.parent().expect("has a parent"));
        (decided.files[&file].clone(), decided.notes)
    };
    let (text, notes) = decide(false);
    assert!(text.contains("import json\n\nfrom archive import prophecy\n"), "{text}");
    assert!(notes.is_empty(), "{notes:?}");
    let (_, notes) = decide(true);
    // a note, which names the module the vision writes into
    assert!(notes.iter().any(|n| n.contains("archive.sink")), "{notes:?}");
}

/// A vision that changes what is built is no prophecy: writing it would replace the code with a
/// sketch. The sloppy vision rewrites `Sink.write` in place; the clean one gives the new thing a
/// name of its own, and is decided.
#[test]
fn prophecy_a_vision_that_changes_what_exists_is_refused() {
    use std::collections::BTreeMap;
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/prophecy/python/clean");
    let file = PathBuf::from("src/archive/sink.py");
    let now = std::fs::read_to_string(dir.join(&file)).expect("reads");
    let vision = BTreeMap::from([(file, Some(now.replace("print(report)", "...")))]);
    let refused =
        atlas_engine::decide::prophetize(&dir, &vision, &BTreeMap::new(), &Options::default()).expect_err("a changed body is refused");
    assert!(refused.to_string().contains("archive.sink.Sink.write"), "the refusal names what the vision changes: {refused}");
}

/// A vision's file is the file its path names from the directory the vision is asked for, and no
/// other: `archive/store/sink.py` holds a `Shelf` and is no part of a vision of `sink.py`, though
/// both are named `sink.py`. The vision puts a `Vault` above the `Sink` of `archive/sink.py`, and
/// is decided whether it is asked for the package on its own (its paths are shorter than the
/// project's) or for a directory the project is a part below (its paths are longer).
#[test]
fn prophecy_a_vision_takes_the_file_its_path_names() {
    use std::collections::BTreeMap;
    let decide = |name: &str, below: bool| {
        let copy = common::clean_copy("python", name);
        std::fs::create_dir_all(copy.join("src/archive/store")).expect("makes the directory");
        std::fs::write(copy.join("src/archive/store/__init__.py"), "").expect("writes");
        std::fs::write(
            copy.join("src/archive/store/sink.py"),
            "\"\"\"Reports kept on a shelf.\"\"\"\n\n\nclass Shelf:\n    \"\"\"Keeps what it is given.\"\"\"\n\n    def keep(self, report: str, /) -> None:\n        print(report)\n",
        )
        .expect("writes");
        let (asked, file) = match below {
            false => (copy.join("src/archive"), PathBuf::from("sink.py")),
            true => {
                let top = copy.parent().expect("has a parent").join("repository");
                std::fs::create_dir_all(&top).expect("makes the directory");
                std::fs::rename(&copy, top.join("py")).expect("moves the project");
                (top, PathBuf::from("py/src/archive/sink.py"))
            }
        };
        let now = std::fs::read_to_string(asked.join(&file)).expect("reads").replace("\r\n", "\n");
        let vault = "class Vault:\n    def keep(self, report: str, /) -> None: ...\n\n\n";
        let vision = BTreeMap::from([(file.clone(), Some(now.replacen("class Sink:", &format!("{vault}class Sink:"), 1)))]);
        let decided = atlas_engine::decide::prophetize(&asked, &vision, &BTreeMap::new(), &Options::default());
        let _ = std::fs::remove_dir_all(copy.parent().expect("has a parent"));
        let decided = decided.unwrap_or_else(|e| panic!("decided, asked for {}: {e}", asked.display()));
        (decided.files.keys().cloned().collect::<Vec<_>>(), decided.marked, file)
    };
    for (name, below) in [("package-alone", false), ("part-below", true)] {
        let (written, marked, file) = decide(name, below);
        assert_eq!(written, [file], "{name}");
        assert_eq!(marked, ["archive.sink.Vault"], "{name}");
    }
}

/// A vision is placed by the directory it is read from, so a vision that shares nothing with the
/// tree is placed as one that does: a project with only its manifest, and a vision that writes its
/// first module. The file is written at the path the vision names it by, and what it adds is marked.
#[test]
fn prophecy_a_vision_of_new_files_only_is_placed_where_it_names_them() {
    use std::collections::BTreeMap;
    let copy = common::clean_copy("python", "new-files");
    std::fs::remove_dir_all(copy.join("src")).expect("removes the code");
    let file = PathBuf::from("src/archive/vault.py");
    let vision = BTreeMap::from([(file.clone(), Some("class Vault:\n    def keep(self, report: str, /) -> None: ...\n".to_string()))]);
    let decided = atlas_engine::decide::prophetize(&copy, &vision, &BTreeMap::new(), &Options::default());
    let _ = std::fs::remove_dir_all(copy.parent().expect("has a parent"));
    let decided = decided.expect("a vision that only adds new files is decided");
    assert_eq!(decided.files.keys().collect::<Vec<_>>(), [&file]);
    assert_eq!(decided.marked, ["archive.vault.Vault"]);
}

/// A prophecy whose shape waits on an open question is sealed on its issue. The seals come back
/// as a list with what the host says of each issue (here: one closed, one open), and the one whose
/// issue is closed can be broken: `sealed=..` leaves its marker and nothing else changes. The
/// lookup is handed in, so the test asks no host.
#[test]
fn prophecy_a_seal_is_listed_and_broken_when_its_issue_is_closed() {
    use atlas_engine::revelation::{revelation, seals, IssueState};
    let (copy, g) = common::written("python", "src/archive/sink.py", "\n\n@prophecy(sealed=\"#7\", supersedes=\"archive.sink.Sink\")\nclass Vault:\n    def keep(self, report: str, /) -> None: ...\n\n\n@prophecy(sealed=\"#8\")\nclass Crate:\n    def pack(self, report: str, /) -> None: ...\n");
    let lookup = |issue: &str| Some((if issue == "#7" { IssueState::Closed } else { IssueState::Open }, format!("question {issue}")));
    let listed = seals(&revelation(&g), &lookup);
    assert_eq!(
        listed.iter().map(|s| (s.id.as_str(), s.can_be_broken())).collect::<Vec<_>>(),
        [("archive.sink.Vault", true), ("archive.sink.Crate", false)].map(|(i, b)| (i, b))
    );
    let broken = atlas_engine::decide::unseal(&g, &["archive.sink.Vault".to_string()]).expect("a closed seal is broken");
    let text = broken.values().next().expect("one file");
    assert!(!text.contains("#7") && text.contains("#8"), "only the broken seal leaves: {text}");
    for (rel, t) in &broken {
        std::fs::write(copy.join(rel), t).expect("writes the file");
    }
    let after = revelation(&atlas_engine::analyze(&copy, &atlas_engine::Options::default()).expect("reads"));
    let vault = after.iter().find(|s| s.id == "archive.sink.Vault").expect("still a prophecy");
    assert!(vault.sealed.is_none() && vault.supersedes.as_deref().is_some_and(|r| r.ends_with("Sink")), "{vault:?}");
    assert_eq!(after.iter().find(|s| s.id == "archive.sink.Crate").and_then(|s| s.sealed.as_deref()), Some("#8"));
    assert!(
        atlas_engine::decide::unseal(&g, &["archive.sink.Store".to_string()]).is_err(),
        "a prophecy that is not sealed has no seal to break"
    );
    let _ = std::fs::remove_dir_all(copy.parent().expect("has a parent"));
}

/// Breaking a seal changes the bytes of `sealed=..` and of the separator that joins it, and no
/// other: the file comes back as it was with exactly those bytes taken out, the seal first among
/// the decorator's arguments, last, or alone. The file is read when the seal is broken and the
/// graph may be older: a file emptied or shortened since is an error.
#[test]
fn prophecy_breaking_a_seal_changes_its_bytes_and_no_other() {
    let sealed = "\"\"\"Where reports wait.\"\"\"\n\nfrom archive import prophecy\n\n\n@prophecy(supersedes=\"archive.sink.Sink\", sealed=\"#7\")\nclass Vault:\n    def keep(self, report: str, /) -> None: ...\n\n\n@prophecy(sealed=\"#8\", supersedes=\"archive.sink.Sink\")\nclass Crate:\n    def pack(self, report: str, /) -> None: ...\n\n\n@prophecy(sealed=\"#9\")\nclass Shelf:\n    def rest(self, report: str, /) -> None: ...\n";
    let copy = common::clean_copy("python", "unseal");
    let file = copy.join("src/archive/vault.py");
    std::fs::write(&file, sealed).expect("writes");
    let g = analyze(&copy, &Options::default()).expect("reads");
    let ids = ["archive.vault.Vault", "archive.vault.Crate", "archive.vault.Shelf"].map(String::from);
    let broken = atlas_engine::decide::unseal(&g, &ids).expect("the seals are broken");
    let unsealed = sealed.replacen(", sealed=\"#7\"", "", 1).replacen("sealed=\"#8\", ", "", 1).replacen("sealed=\"#9\"", "", 1);
    let text = broken.get(std::path::Path::new("src/archive/vault.py")).cloned();
    let changed = ["", "\"\"\"Where reports wait.\"\"\"\n\n"].map(|cut| {
        std::fs::write(&file, cut).expect("writes");
        atlas_engine::decide::unseal(&g, &ids).is_err()
    });
    let _ = std::fs::remove_dir_all(copy.parent().expect("has a parent"));
    assert_eq!(text, Some(unsealed));
    assert_eq!(changed, [true, true], "an emptied file, a shortened one");
}

fn fits(side: &str) -> Graph {
    analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/sums/fits/python").join(side), &Options::default()).unwrap()
}

/// A sum is unwrapped where it arises, written in place or by a name: `fit` gives `Fit | Failed`
/// without naming it. On the clean side one function takes it, `describe`, the copairing that
/// gives the command line a line and the points to plot: a coproduct is taken apart somewhere,
/// and once is no repeated choice. The shortcut hands the sum unopened to `line` and to `points`,
/// which each choose again; `points` spells it `Optional[Fit | Failed]`, the same sum.
#[test]
fn sums_a_union_written_in_place_is_a_sum() {
    let (c, s) = (fits("clean"), fits("shortcut"));
    let sum = "(fits.model.Failed | fits.model.Fit)";
    for g in [&c, &s] {
        assert!(edge(g, sum, "fits.model.Fit", EdgeKind::Variant) && edge(g, sum, "fits.model.Failed", EdgeKind::Variant));
        assert!(edge(g, "fits.fitter.fit", sum, EdgeKind::Returns), "`fit` gives the sum, not a Fit and a Failed");
        assert!(!edge(g, "fits.fitter.fit", "fits.model.Fit", EdgeKind::Returns));
    }
    assert!(edge(&c, "fits.report.describe", sum, EdgeKind::Takes));
    assert!(found(&c, "sums").is_empty(), "one copairing is where the sum is taken apart");
    for consumer in ["fits.report.line", "fits.plot.points"] {
        assert!(edge(&s, consumer, sum, EdgeKind::Takes), "{consumer} takes the sum, however it spells it");
    }
    assert_eq!(found(&s, "sums"), [sum]);
    // the functions that take the sum unopened are these two, and no other
    let mut takers: Vec<&str> = s.edges.iter().filter(|e| e.kind == EdgeKind::Takes && e.dst == sum).map(|e| e.src.as_str()).collect();
    takers.sort();
    assert_eq!(takers, ["fits.plot.points", "fits.report.line"]);
}

/// The left of `x or y` always runs and what it gives flows on through it; the right runs only
/// where the left gives nothing: a choice of one arm, `f + 1`, as Rust's `unwrap_or_else` and
/// TypeScript's `??` are. The store's `summary` returns the summary kept, else keeps one made from
/// the day's readings: `kept_summary` is in no arm, `history`, `day_summary` and `keep_summary` in
/// the one that runs otherwise.
#[test]
fn choices_the_left_of_an_or_always_runs() {
    let summary = clean().node("harvest.store.ReadingStore.summary").unwrap();
    let arms = |to: &str| summary.sites.iter().find(|s| s.to == format!("harvest.{to}")).map(|s| s.arms.clone()).unwrap();
    assert_eq!(arms("store.ReadingStore.kept_summary"), []);
    for to in ["store.ReadingStore.history", "model.day_summary", "store.ReadingStore.keep_summary"] {
        assert_eq!(arms(to), [(0, 1)], "{to}");
    }
}

/// An entry run from the command line is handed the words the human types. `sys.argv` holds them
/// word by word, as Rust's `env::args()` gives them: the step that keeps them under a name parses
/// the command line, with no options since the code names none. In the fits pair that is the loop
/// of `[float(x) for x in sys.argv[1:]]`, which keeps each word as `x`, handed on to `fit`.
#[test]
fn places_the_command_line_is_the_entrys() {
    for side in ["clean", "shortcut"] {
        let g = fits(side);
        let main = g.node("fits.cli.main").unwrap();
        let parses: Vec<(String, Vec<String>, Vec<String>)> = main
            .sites
            .iter()
            .filter_map(|s| Some((s.op.clone(), s.command_line.clone()?, s.gives.iter().map(|h| h.name.clone()).collect())))
            .collect();
        assert_eq!(parses, [("for x in sys.argv[1:]".to_string(), vec![], vec!["x".to_string()])], "{side}");
        let fit = main.sites.iter().find(|s| s.to == "fits.fitter.fit").unwrap();
        assert!(fit.hands.iter().any(|h| h.name == "x"), "{side}: {:?}", fit.hands);
    }
}

/// A tuple is the values it pairs where it stands for one value, and a collection of tuples holds
/// pairs, of a type the code does not name, as Rust's `Vec<(NaiveDate, f64)>` holds no
/// `NaiveDate`. The tuples pair (`tests/pairs/tuples/python`) gives each station's mean: the clean
/// side as a `list[Mean]`, a record with a name, the shortcut as `list[tuple[Station, float]]`,
/// which gives no Station. Both give `tuple[Station, Store] | None` from `first`: a pair or nothing,
/// one value of each where there is one. A tuple of any length (`tuple[Station, ...]`) holds
/// Stations.
#[test]
fn a_place_carries_a_what_and_a_collection_of_pairs_names_none() {
    let p = common::pair("tuples/python");
    let edges = |g: &Graph, kind: EdgeKind, src: &str| -> Vec<String> {
        let mut out: Vec<String> =
            g.edges.iter().filter(|e| e.kind == kind && e.src == src).map(|e| e.dst.rsplit('.').next().unwrap_or("").to_string()).collect();
        out.sort();
        out
    };
    assert_eq!(edges(&p.clean, EdgeKind::Returns, "stations.means"), ["Mean"]);
    assert_eq!(
        edges(&p.shortcut, EdgeKind::Returns, "stations.means"),
        Vec::<String>::new(),
        "a pair of a Station and a number is no Station"
    );
    for g in [&p.clean, &p.shortcut] {
        assert_eq!(
            edges(g, EdgeKind::Returns, "stations.first"),
            ["Station", "Store"],
            "a tuple that stands for one value is the values it pairs"
        );
        assert_eq!(edges(g, EdgeKind::Has, "stations.Store"), ["Station"], "a tuple of any length holds its elements");
    }
}

/// What follows an early return runs only where the guard's arm does not: `fit`'s `if not xs:
/// return Failed(..)` and the `return Fit(..)` after it are the two arms of one choice, as an `if ..
/// else` writes them (isomorphic is the same), the second running otherwise. Each arm makes its
/// variant of the sum `fit` gives.
#[test]
fn choices_what_follows_an_early_return_is_the_other_arm() {
    for side in ["clean", "shortcut"] {
        let g = fits(side);
        let fit = g.node("fits.fitter.fit").unwrap();
        let arms = |to: &str| fit.sites.iter().find(|s| s.to == to).map(|s| s.arms.clone());
        assert_eq!(arms("fits.model.Failed"), Some(vec![(0, 0)]), "{side}");
        assert_eq!(arms("fits.model.Fit"), Some(vec![(0, 1)]), "{side}: what follows the guard is its other arm");
        assert!(fit.whens.contains(&(0, 1, "otherwise".to_string())), "{side}: {:?}", fit.whens);
    }
}

/// One side of the choices pair (`tests/pairs/choices/python`): a job run in one of two modes, and
/// a line that says how it went.
fn choices_side(side: &str) -> Graph {
    analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/choices/python").join(side), &Options::default()).unwrap()
}

/// A choice is made once, in a place with a name, and Fowler's repeated switch is between
/// functions as well as inside one. The clean side makes the mode an `Enum` with one table of what
/// each mode means (`PLANS`), gives the level methods that each `match` the value they run on once,
/// and names where the job comes from (`source`); `report` builds its Message by one constructor
/// in either arm, which is one step with a choice of what it is handed (`h ∘ [f, g] = [h ∘ f,
/// h ∘ g]`), so nothing is asked. The shortcut tests `self.mode` in two methods of `Runner`, `mode`
/// in two functions, `settings.mode` twice through a local `m`, `level` against the member
/// `Level.LOUD` in two functions (`is` and `case`), and picks where its Job comes from (`load |
/// infer`) between the steps of `run`: each is asked once, where the value is defined.
#[test]
fn choices_a_choice_is_made_once_across_bodies() {
    let (c, s) = (choices_side("clean"), choices_side("shortcut"));
    assert!(found(&c, "choices").is_empty(), "{:?}", found(&c, "choices"));
    let message: Vec<&[(u32, u32)]> =
        c.node("runs.core.report").unwrap().sites.iter().filter(|s| s.to == "runs.core.Message").map(|s| s.arms.as_slice()).collect();
    assert_eq!(message, [&[(0, 0)][..], &[(0, 1)][..]], "one constructor in each arm of one choice");
    let level = &c.node("runs.core.Level.prefix").unwrap().compares;
    assert!(level.len() == 1 && level[0].receiver && level[0].members == ["Level.QUIET", "Level.LOUD"], "{level:?}");

    let mut asked = found(&s, "choices");
    asked.sort();
    assert_eq!(asked, ["runs.core.Runner", "runs.core.prefix", "runs.core.retries", "runs.core.run", "runs.core.verbosity"]);
    // what each comes back to: the subject each body tests (as its comparison writes it), and the value it tests it against
    let tests = |id: &str| -> Vec<(String, String, Vec<String>)> {
        let compares = &s.node(id).unwrap().compares;
        compares.iter().map(|c| (c.subject.clone(), c.written.clone(), c.cases.iter().chain(&c.members).cloned().collect())).collect()
    };
    let against = |subject: &str, written: &str, value: &str| (subject.to_string(), written.to_string(), vec![value.to_string()]);
    for f in ["Runner.batch", "Runner.workers"] {
        assert_eq!(tests(&format!("runs.core.{f}")), [against("self.mode", "", "parallel")], "{f}");
    }
    for f in ["retries", "timeout"] {
        assert_eq!(tests(&format!("runs.core.{f}")), [against("mode", "", "parallel")], "{f}");
    }
    let m = against("settings.mode", "m", "parallel");
    assert_eq!(tests("runs.core.verbosity"), [m.clone(), m], "the local `m` is the value it is bound to");
    for f in ["prefix", "suffix"] {
        assert_eq!(tests(&format!("runs.core.{f}")), [against("level", "", "Level.LOUD")], "{f}");
    }
    // `run` takes its Job from `load` in one arm of a choice and from `infer` in the other, and hands it to `finished`
    // after the choice
    let run = s.node("runs.core.run").unwrap();
    let site = |to: &str| run.sites.iter().find(|x| x.to == to).unwrap_or_else(|| panic!("run calls {to}"));
    let (load, infer, finished) = (site("runs.core.load"), site("runs.core.infer"), site("runs.core.finished"));
    assert!(
        load.arms.len() == 1 && infer.arms.len() == 1 && load.arms[0].0 == infer.arms[0].0 && load.arms[0].1 != infer.arms[0].1,
        "{:?} {:?}",
        load.arms,
        infer.arms
    );
    assert!([load, infer].iter().all(|x| x.gives.iter().any(|h| h.name == "job")), "{:?} {:?}", load.gives, infer.gives);
    assert!(finished.arms.is_empty() && finished.hands.iter().any(|h| h.name == "job"), "{finished:?}");
    // the shortcut still runs the job and sends its line
    assert!(edge(&s, "runs.core.run", "runs.core.send", EdgeKind::Calls) && edge(&s, "runs.core.run", "runs.core.load", EdgeKind::Calls));
}

/// One side of the lens pair's club (`tests/pairs/lens/python/club`): a desk that keeps visits,
/// members and reports in SQLite, a table each, and the files a club keeps (a log, a roster, a
/// board, a tape), each class a lens or no lens on its table or file.
fn club() -> &'static Pair {
    static PAIR: OnceLock<Pair> = OnceLock::new();
    PAIR.get_or_init(|| common::pair("lens/python/club"))
}

/// What `lens` asks of class `club.{id}`: the types it puts and does not get back; none when it asks nothing.
fn lens_of(g: &Graph, id: &str) -> Option<Vec<String>> {
    let f = check(g).into_iter().find(|f| f.principle == "lens" && f.node == format!("club.{id}"))?;
    Some(f.about("lost").into_iter().map(String::from).collect())
}

/// Whether class `club.{cls}` takes a `club.model.{ty}` where it writes the place `at` (a table, or the value that holds a
/// file's path) and gets from `at` (reads it without writing it), while no get gives a `ty` back: as itself, inside what
/// it gives, or as a subclass of what it gives. That is what `lens` asks, short of telling a value put from a key the
/// statement finds rows by.
fn loses(g: &Graph, cls: &str, ty: &str, at: &str) -> bool {
    let (cls, ty) = (format!("club.{cls}"), format!("club.model.{ty}"));
    let methods: Vec<_> = g.nodes.iter().filter(|n| n.cls.as_deref() == Some(cls.as_str())).collect();
    let touches = |m: &atlas_engine::Node, side: Side| m.touches.iter().any(|t| (t.artifact == at || t.what == at) && t.side() == side);
    let holds = |t: &str| t == ty || edge(g, t, &ty, EdgeKind::Has) || edge(g, &ty, t, EdgeKind::Inherits);
    let gets: Vec<_> = methods.iter().filter(|m| touches(m, Side::In) && !touches(m, Side::Out)).collect();
    methods.iter().any(|m| edge(g, &m.id, &ty, EdgeKind::Takes) && touches(m, Side::Out))
        && !gets.is_empty()
        && !gets.iter().any(|m| g.edges.iter().any(|e| e.kind == EdgeKind::Returns && e.src == m.id && holds(&e.dst)))
}

/// What a lens puts, it gets back, at the place it puts to: a table, not the database it is in.
/// The clean `Desk` only writes `visits` and only reads `members`, and is no lens on either; its
/// `Register` puts a Member into `members` and gets it back from there, beside the Reports it reads
/// from `reports`. The shortcut's `Desk` reads `visits` back as rows, and is asked about the Visit;
/// its `Register` reads back only the names from `members`, and is asked about the Member: the
/// Member inside a Report comes from another table.
#[test]
fn lens_the_place_is_the_table() {
    let (c, s) = (&club().clean, &club().shortcut);
    let fs = check(c);
    assert!(fs.is_empty(), "{:#?}", fs.iter().map(|f| format!("{} {} {}", f.principle, f.node, f.what)).collect::<Vec<_>>());
    let at = |g: &Graph, f: &str| {
        touches(g, &format!("club.desk.{f}"), "db_sqlite").iter().map(|t| (t.artifact.clone(), t.dir_name())).collect::<Vec<_>>()
    };
    assert_eq!(at(c, "Desk.log"), [("visits".to_string(), "write")]);
    assert_eq!(at(c, "Desk.members"), [("members".to_string(), "read")]);
    assert_eq!(at(s, "Register.latest"), [("reports".to_string(), "read")]);
    assert!(edge(s, "club.model.Report", "club.model.Member", EdgeKind::Has));
    for (cls, ty, table) in [("desk.Desk", "Visit", "visits"), ("desk.Register", "Member", "members")] {
        assert_eq!(lens_of(s, cls), Some(vec![format!("club.model.{ty}")]), "{cls}");
        assert!(loses(s, cls, ty, table) && !loses(c, cls, ty, table), "{cls}");
    }
}

/// What a lens puts, it gets back, at the file the code names: by its path, or by the value that
/// holds it. The clean `Tape` writes a Visit to the stream it is handed and reads names from
/// another: each is known only as JSON, which names no file, and `lens` asks nothing. The
/// shortcut's `Tape` writes and reads the file it is made with, `Disk(Tape.path)`, and is asked
/// where the Visit went.
#[test]
fn lens_the_place_is_the_file_the_code_names() {
    let (c, s) = (&club().clean, &club().shortcut);
    let record = c.node("club.files.Tape.record").unwrap();
    assert!(record.touches.iter().any(|t| t.artifact == "*.json" && t.kind_name() == "disk_write"), "{:?}", record.touches);
    assert_eq!(lens_of(c, "files.Tape"), None);
    assert_eq!(lens_of(s, "files.Tape"), Some(vec!["club.model.Visit".to_string()]));
    assert!(loses(s, "files.Tape", "Visit", "Tape.path"));
    assert!(!loses(c, "files.Tape", "Visit", "Tape.path"));
}

/// A get gives a value back from the place; an operation that reads the place on its way to
/// writing it is a put. The clean `Log.add` reads the list in its file, appends a Visit and writes
/// the list back: an end product, no lens. The shortcut's `Log` also gives the list back as it is
/// kept, `list[dict]`, and is asked where the Visit went.
#[test]
fn lens_a_get_gives_a_value_back() {
    let (c, s) = (&club().clean, &club().shortcut);
    let add = c.node("club.files.Log.add").unwrap();
    let kinds: Vec<&str> = add.touches.iter().map(|t| t.kind_name()).collect();
    assert!(kinds.contains(&"disk_read") && kinds.contains(&"disk_write"), "{:?}", add.touches);
    assert_eq!(lens_of(c, "files.Log"), None);
    assert_eq!(lens_of(s, "files.Log"), Some(vec!["club.model.Visit".to_string()]));
    assert!(loses(s, "files.Log", "Visit", "Log.path"));
    assert!(!loses(c, "files.Log", "Visit", "Log.path"));
}

/// What a put puts is what its statement writes; a key its statement finds rows by is no value
/// put. The clean `Visits.forget` deletes a Member's visits (`DELETE .. WHERE member = ?`): the
/// Member is the key, and `Visits` gets back the Visits it puts. The shortcut's `come` inserts the
/// Member's name as a value, and `Visits` is asked where the Member went.
#[test]
fn lens_a_key_is_no_value_put() {
    let (c, s) = (&club().clean, &club().shortcut);
    assert!(edge(c, "club.desk.Visits.forget", "club.model.Member", EdgeKind::Takes));
    assert!(touches(c, "club.desk.Visits.forget", "db_sqlite").iter().any(|t| t.artifact == "visits" && t.dir_name() == "write"));
    assert_eq!(lens_of(c, "desk.Visits"), None);
    assert_eq!(lens_of(s, "desk.Visits"), Some(vec!["club.model.Member".to_string()]));
    assert!(loses(s, "desk.Visits", "Member", "visits"));
}

/// What a get gives back holds what was put: the class's own type, which a constructor gives, and
/// a subclass of what a get gives, a variant of a sum left open. The clean `Roster` gives a Roster
/// back (`load`, a constructor, and `reread`), which holds the Members it puts; its `Board` pins a
/// Notice and gives the Posts back, and a Notice is a Post. The shortcut's `Roster.load` gives a
/// Roster that holds rows, and its `Board` gives the rows back: each is asked.
#[test]
fn lens_a_get_gives_its_own_class_and_a_subclass_back() {
    let (c, s) = (&club().clean, &club().shortcut);
    assert!(c.node("club.files.Roster.load").unwrap().ctor);
    assert!(edge(c, "club.files.Roster", "club.model.Member", EdgeKind::Has));
    assert!(edge(c, "club.model.Notice", "club.model.Post", EdgeKind::Inherits));
    assert_eq!(lens_of(c, "files.Roster"), None);
    assert_eq!(lens_of(c, "files.Board"), None);
    for (cls, ty) in [("files.Roster", "Member"), ("files.Board", "Notice")] {
        let file = format!("{}.path", cls.trim_start_matches("files."));
        assert_eq!(lens_of(s, cls), Some(vec![format!("club.model.{ty}")]), "{cls}");
        assert!(loses(s, cls, ty, &file) && !loses(c, cls, ty, &file), "{cls}");
    }
}

/// What a get gives back holds its fields, a library's type among them. `Ledger` puts a `Fraction`
/// and gets it back as an `Entry`'s `amount: Fraction` (`tests/pairs/lens/python/ledger`); the
/// Python reader draws no `has` edge to a library's type, so `lens` does not see the Fraction
/// come back.
#[test]
#[ignore = "gap (a): the Python reader draws no `has` edge to a library's type"]
fn lens_a_library_type_comes_back_inside_what_a_get_gives() {
    let g = analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/lens/python/ledger/clean"), &Options::default()).unwrap();
    assert!(edge(&g, "books.ledger.Entry", "ext:fractions.Fraction", EdgeKind::Has));
    assert!(found(&g, "lens").is_empty(), "{:?}", found(&g, "lens"));
}

/// One side of the overrides pair (`tests/pairs/overrides`): readers of bytes, documents that parse
/// themselves, a count, a catalog of entries, ledgers and boxes over a type their subclasses fill
/// in, and encoders of arrays of a shape, each family once as its author keeps its bases' promises
/// and once as a hurried author writes it.
fn reading() -> &'static Pair {
    static PAIR: OnceLock<Pair> = OnceLock::new();
    PAIR.get_or_init(|| common::pair("overrides/python"))
}

/// An override keeps the promise of every method it overrides, since a caller written against any
/// of its class's ancestors may be handed it; the one it breaks is the first along the method
/// resolution order. `Feed(Buffered, Archive)` runs Feed, Buffered, Source, Archive, Store, as C3
/// orders them, so `Source.read` comes before `Archive.read`. On the clean side every `read` gives
/// bytes. On the shortcut `Archive.read` gives a str: `Feed.read` follows it and breaks the
/// `Source.read` its order reaches first, which the finding names, and `Tape.read` keeps
/// `Source.read` and breaks `Archive.read`.
#[test]
fn overrides_the_base_is_the_next_in_the_method_resolution_order() {
    let (c, s) = (&reading().clean, &reading().shortcut);
    assert!(found(c, "overrides").is_empty(), "{:?}", found(c, "overrides"));
    let bases = |g: &'static Graph| {
        move |cls: &'static str| -> Vec<&'static str> {
            g.edges.iter().filter(|e| e.kind == EdgeKind::Inherits && e.src == cls).map(|e| e.dst.as_str()).collect()
        }
    };
    let order = atlas_engine::linearization("reading.feeds.Feed", &bases(s), &mut Default::default());
    assert_eq!(order, ["Feed", "Buffered", "Source", "Archive", "Store"].map(|c| format!("reading.feeds.{c}")));
    let feeds: Vec<String> = found(s, "overrides").into_iter().filter(|n| n.starts_with("reading.feeds.")).collect();
    assert_eq!(feeds, ["reading.feeds.Source.read", "reading.feeds.Archive.read"]);
    assert_eq!(s.node("reading.feeds.Feed.read").unwrap().breaks.as_deref(), Some("reading.feeds.Source.read"));
    assert_eq!(s.node("reading.feeds.Tape.read").unwrap().breaks.as_deref(), Some("reading.feeds.Archive.read"));
    // `Source.read` promises bytes, and the one override that breaks it gives a str
    let breaking: Vec<&str> =
        s.nodes.iter().filter(|n| n.breaks.as_deref() == Some("reading.feeds.Source.read")).map(|n| n.id.as_str()).collect();
    assert_eq!(breaking, ["reading.feeds.Feed.read"]);
    let gives = |id: &str| s.node(id).unwrap().returns.as_ref().map(|r| r.written.clone());
    assert_eq!((gives("reading.feeds.Source.read"), gives("reading.feeds.Feed.read")), (Some("bytes".into()), Some("str".into())));
}

/// A constructor on the class is called through the class a caller names, and is handed what the
/// caller hands its base's: the clean `Page.parse` takes the `text: str` of `Doc.parse`, and a new
/// way in has a name of its own (`Page.decode(data: bytes)`). The shortcut's `Page.parse` takes
/// bytes, so a caller of `Doc.parse` handed the class `Page` breaks; ty says so, and Atlas asks. An
/// initializer answers to no base: each `Page.__init__` takes a page number `Doc.__init__` does not,
/// and neither side is asked about it.
#[test]
fn overrides_a_constructor_on_the_class_takes_what_its_base_takes() {
    let (c, s) = (&reading().clean, &reading().shortcut);
    let parse = c.node("reading.docs.Page.parse").unwrap();
    assert!(parse.ctor && parse.on_class && parse.keeps == Some(true), "{parse:?}");
    assert!(!found(c, "overrides").iter().any(|n| n.starts_with("reading.docs.")));
    let docs: Vec<String> = found(s, "overrides").into_iter().filter(|n| n.starts_with("reading.docs.")).collect();
    assert_eq!(docs, ["reading.docs.Doc.parse"]);
    assert_eq!(s.node("reading.docs.Page.parse").unwrap().keeps, Some(false));
    let init = s.node("reading.docs.Page.__init__").unwrap();
    assert!(init.ctor && !init.on_class);
}

/// `# type: ignore[override]` on an override is the code saying it knows, whatever its codes, as the
/// bare comment is: ty honours only the bare form and codes of its own (`ty:`), and Atlas reads the
/// comment on the override's line itself. The clean `Kennel.count` gives a str where `Shelter.count`
/// promises an int, and says so; the shortcut does the same without a word, and is asked.
#[test]
fn overrides_a_type_ignore_with_codes_is_the_code_saying_it_knows() {
    let (c, s) = (&reading().clean, &reading().shortcut);
    assert_eq!(c.node("reading.counts.Kennel.count").unwrap().keeps, Some(true));
    assert!(!found(c, "overrides").contains(&"reading.counts.Shelter.count".to_string()));
    assert_eq!(s.node("reading.counts.Kennel.count").unwrap().keeps, Some(false));
    assert!(found(s, "overrides").contains(&"reading.counts.Shelter.count".to_string()));
    // the override that says it knows is still in the graph, with what it gives
    assert_eq!(c.node("reading.counts.Kennel.count").unwrap().returns.as_ref().map(|r| r.written.as_str()), Some("str"));
}

/// An override keeps its base's promise when it says it in other words, and breaks it when it
/// gives one of what was promised a list of. On both sides `Catalog` says what `Index` promises in
/// other words (`List[Entry]` for `list[Entry]`, `Entry | None` for `Optional[Entry]`, an
/// `Annotated[str, ..]` for a `str`) or narrower (a list for a `Sequence`, a `Catalog` for an
/// `Index`), as ty decides, and none of that is asked. The clean `Catalog.head` gives the
/// `list[Entry]` `Index.head` promises; the shortcut's gives one `Entry`, and `Index.head` is asked.
#[test]
fn overrides_a_promise_said_in_other_words_is_kept_and_one_of_a_list_breaks_it() {
    let (c, s) = (&reading().clean, &reading().shortcut);
    assert!(found_in(c, "overrides", "reading.catalogs").is_empty(), "{:?}", found_in(c, "overrides", "reading.catalogs"));
    assert_eq!(found_in(s, "overrides", "reading.catalogs"), ["reading.catalogs.Index.head"]);
    for g in [c, s] {
        for m in ["entries", "roots", "find", "title", "copy"] {
            assert_eq!(g.node(&format!("reading.catalogs.Catalog.{m}")).unwrap().keeps, Some(true), "{m}");
        }
    }
    let head = |g: &Graph| g.node("reading.catalogs.Catalog.head").map(|n| (n.keeps, n.returns.as_ref().map(|r| r.written.clone())));
    assert_eq!(head(c), Some((Some(true), Some("list[Entry]".into()))));
    assert_eq!(head(s), Some((Some(false), Some("Entry".into()))));
}

/// An override of a generic base keeps the promise its subclass fills the base in with, in both
/// forms of a type parameter: `Ledger[R]` and `Box(ABC, Generic[T])`. The clean `CountLedger`
/// fills `Ledger` in with an int and gives a `Version[int]`, and `Numbers` fills `Box` in with an
/// int and takes `list[int]`. The shortcut's `CountLedger` fills `Ledger` in with an `Entry` and
/// still gives a `Version[int]`, and its `Numbers` takes strings: `Ledger.from_row` is asked about
/// what one override gives, `Box.fill` about what one takes. `EntryLedger` keeps its promise on both.
#[test]
fn overrides_an_override_of_a_generic_base_keeps_what_its_subclass_fills_in() {
    let (c, s) = (&reading().clean, &reading().shortcut);
    assert!(found_in(c, "overrides", "reading.ledgers").is_empty(), "{:?}", found_in(c, "overrides", "reading.ledgers"));
    assert_eq!(found_in(s, "overrides", "reading.ledgers"), ["reading.ledgers.Ledger.from_row", "reading.ledgers.Box.fill"]);
    let keeps = |g: &Graph, id: &str| g.node(&format!("reading.ledgers.{id}")).unwrap_or_else(|| panic!("{id} is in the graph")).keeps;
    for g in [c, s] {
        assert_eq!(keeps(g, "EntryLedger.from_row"), Some(true));
    }
    for m in ["CountLedger.from_row", "Numbers.fill"] {
        assert_eq!((keeps(c, m), keeps(s, m)), (Some(true), Some(false)), "{m}");
    }
    let breaks = |id: &str| s.node(&format!("reading.ledgers.{id}")).unwrap().breaks.clone();
    assert_eq!(breaks("CountLedger.from_row").as_deref(), Some("reading.ledgers.Ledger.from_row"));
    assert_eq!(breaks("Numbers.fill").as_deref(), Some("reading.ledgers.Box.fill"));
}

/// An array's shape is a promise no type checker reads, and an override keeps it as it keeps a
/// type: `MeanEncoder.encode` gives `batch dim` where `Encoder.encode` promises any shape (`...`).
/// The clean `PatchEncoder` keeps every dim and extends `Encoder`, whose `...` admits it; the
/// shortcut's extends `MeanEncoder`, whose `batch dim` it breaks with three dims, and
/// `MeanEncoder.encode` is asked.
#[test]
fn overrides_an_array_s_shape_is_a_promise() {
    let (c, s) = (&reading().clean, &reading().shortcut);
    assert!(found_in(c, "overrides", "reading.encoders").is_empty(), "{:?}", found_in(c, "overrides", "reading.encoders"));
    assert_eq!(found_in(s, "overrides", "reading.encoders"), ["reading.encoders.MeanEncoder.encode"]);
    assert!(edge(c, "reading.encoders.PatchEncoder", "reading.encoders.Encoder", EdgeKind::Inherits));
    assert!(edge(s, "reading.encoders.PatchEncoder", "reading.encoders.MeanEncoder", EdgeKind::Inherits));
    let gives = |g: &Graph| g.node("reading.encoders.PatchEncoder.encode").and_then(|n| n.returns.as_ref().map(|r| r.written.clone()));
    assert_eq!(gives(c), gives(s));
    assert_eq!(gives(s).as_deref(), Some("Float[Tensor, \"batch seq dim\"]"));
}

/// A library's operation is a step where the body names what it gives or where it changes the
/// value it runs on; what it gives without a name goes into the call around it. On the clean
/// board `renamed` names the copy it makes (`board = copy.copy(board)`, a value made anew, no
/// identity), `queued` the list it makes and the one `heapq.heappush` changes, `extended` the list
/// `append` changes, and `Board.reset` the board `self.cells.clear()` changes; a list is of no type
/// of the code's, a value all the same. `with self.connect() as con` names the connection as `con
/// = ..` does, `rows = con.execute(..).fetchall()` names what `fetchall` gives with `execute`
/// glued into it, and `uuid, soort, tekst = line.split(" ", 2)` names its three parts. A method
/// of pandas ty does not know (`frame["mean"].rolling(..).mean()`) is still pandas', called on a
/// DataFrame, so unnamed it is no step.
#[test]
fn a_library_operation_is_a_step_where_its_result_is_named_or_it_changes_a_value() {
    let ops = |g: &Graph, id: &str| -> Vec<(String, Vec<String>)> {
        g.node(id)
            .unwrap_or_else(|| panic!("{id}"))
            .sites
            .iter()
            .filter(|s| s.to.is_empty())
            .map(|s| (s.op.clone(), s.gives.iter().map(|h| h.name.clone()).collect()))
            .collect()
    };
    let named = |op: &str, names: &[&str]| (op.to_string(), names.iter().map(|n| n.to_string()).collect::<Vec<_>>());
    let c = board("clean");
    assert_eq!(ops(&c, "board.core.renamed"), [named("copy", &["board"])]);
    assert_eq!(ops(&c, "board.core.queued"), [named("list", &["jobs"]), named("heappush", &["jobs"])]);
    assert_eq!(ops(&c, "board.core.extended"), [named("append", &["xs"])]);
    assert_eq!(ops(&c, "board.core.Board.reset"), [named("clear", &["self"]), named("clear", &["self"])]);
    let s = board("shortcut");
    assert_eq!(ops(&s, "board.core.add_tag"), [named("append", &["tags"])]);

    let history = clean().node("harvest.store.ReadingStore.history").unwrap();
    let connect = history.sites.iter().find(|x| x.to == "harvest.store.ReadingStore.connect").unwrap();
    assert!(connect.gives.iter().any(|h| h.name == "con"), "{:?}", connect.gives);
    assert!(ops(clean(), "harvest.store.ReadingStore.history").contains(&named("fetchall", &["rows"])));
    assert!(ops(clean(), "harvest.store.ReadingStore.history").iter().all(|(op, _)| op != "execute"));

    let k = kennis("clean");
    assert_eq!(ops(&k, "kennis.bank.Kennisbank._node"), [named("split", &["uuid", "soort", "tekst"])]);

    let pipeline =
        analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/pipeline/python-clean"), &Options::default()).unwrap();
    assert!(ops(&pipeline, "sunspots.series.smooth").is_empty(), "{:?}", pipeline.node("sunspots.series.smooth").unwrap().sites);
}

/// One side of the change pair (`tests/pairs/changes`): a scoreboard of cells and scores, whose
/// functions change what they are handed, in Python, Rust and TypeScript alike; in Python also the
/// edits of `board.edits`, a tally among them.
fn board(side: &str) -> Graph {
    analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/changes/python").join(side), &Options::default()).unwrap()
}

/// A change to an input shows in the signature, whatever the input's type. The clean board changes
/// a value through a method of its own (`Board.reset` clears its cells, `Board.mark_all` marks each
/// cell it holds) or hands back a new one: `with_tag` and `counted` build it, `renamed` rebinds
/// `board` to a copy before it sets the name, `extended` and `queued` rebind their list to a new
/// one before they change it, and `emptied` resets a copy of the board. Nothing is asked. The
/// shortcut changes what it is handed and says nothing: a `list[str]` it appends to, a `dict` it
/// sets an item of, a list it extends with `+=`, a heap `heapq` keeps in a list, the cells `clear`
/// empties and the scores `update` fills (methods ty finds on typing's `MutableSequence` and
/// `MutableMapping`), each cell through the loop variable that goes over the board's cells, the
/// board through a second name for it, both boards a tuple assignment swaps, and the board `wipe`
/// hands to `reset`, a function that changes it. `Board.take_from` and `Board.take_victim` have one
/// body: each empties the board it is handed as well as its own, two changes, whatever the
/// parameter is called. Each is asked, and none of them is a call that leaves nothing behind.
#[test]
fn changes_a_change_to_an_input_of_any_type_shows_in_the_signature() {
    let (c, s) = (board("clean"), board("shortcut"));
    let fs = check(&c);
    assert!(fs.is_empty(), "{:#?}", fs.iter().map(|f| format!("{} {} {}", f.principle, f.node, f.what)).collect::<Vec<_>>());
    assert!(c.edges.iter().filter(|e| e.kind == EdgeKind::Mutates).all(|e| e.via.as_deref() == Some("self")), "only methods change");
    let mut asked = found_in(&s, "changes", "board.core");
    asked.sort();
    let want = [
        "Board.take_from",
        "Board.take_victim",
        "add_tag",
        "count",
        "extend",
        "mark_all",
        "merge_scores",
        "queue",
        "rename",
        "reset",
        "swap",
        "swap",
        "wipe",
    ];
    assert_eq!(asked, want.map(|f| format!("board.core.{f}")));
    assert!(found(&s, "nothing").is_empty(), "{:?}", found(&s, "nothing"));
    let inputs = |f: &str| {
        let mut via: Vec<&str> =
            s.edges.iter().filter(|e| e.kind == EdgeKind::Mutates && e.src == f).filter_map(|e| e.via.as_deref()).collect();
        via.sort();
        via
    };
    assert_eq!(inputs("board.core.Board.take_from"), ["other", "self"]);
    assert_eq!(inputs("board.core.Board.take_victim"), ["self", "victim"]);
    assert!(edge(&s, "board.core.add_tag", "ext:builtins.list", EdgeKind::Mutates), "a list is what changes");
    // what changes is the `list[str]` it is handed as `tags`
    assert!(s.edges.iter().any(|e| e.kind == EdgeKind::Mutates && e.src == "board.core.add_tag" && e.via.as_deref() == Some("tags")));
    let tags = s.node("board.core.add_tag").unwrap().params.iter().find(|p| p.name == "tags").map(|p| p.ty.clone());
    assert_eq!(tags.as_deref(), Some("list[str]"));
    assert_eq!(inputs("board.core.wipe"), ["board"], "what it hands a function that changes it, it changes");
}

/// A call that changes what it is handed makes each such value anew, under the name the caller
/// handed it by and of its type (`T × A → T`): the shortcut's `other.take_from(board)` and
/// `other.take_victim(board)` give main's other and main's board, `swap(board, other)` both boards,
/// and `add_tag(tags, ..)`, `count(counts, ..)`, `queue(jobs, ..)` and `extend(xs, ..)` the list or
/// dict they change, of the type the change is to (the literal the name holds is no value of the
/// plan, so it is not handed). On the clean board only a method that changes its receiver gives it
/// (`board.mark_all()`, `board.reset()`); `renamed(board, ..)` and `extended([1], ..)` change
/// nothing they are handed and give none of it.
#[test]
fn changes_a_call_gives_what_it_changes_under_the_name_it_was_handed() {
    let (c, s) = (board("clean"), board("shortcut"));
    let gives = |g: &Graph, to: &str| -> Vec<(String, String)> {
        let main = g.node("board.core.main").expect("main");
        let site = main.sites.iter().find(|x| x.to == to).unwrap_or_else(|| panic!("main calls {to}"));
        site.gives.iter().map(|h| (h.ty.clone(), h.name.clone())).collect()
    };
    let held = |vs: &[(&str, &str)]| vs.iter().map(|(t, n)| (t.to_string(), n.to_string())).collect::<Vec<_>>();
    let board_ = "board.core.Board";
    assert_eq!(gives(&s, "board.core.Board.take_from"), held(&[(board_, "other"), (board_, "board")]));
    assert_eq!(gives(&s, "board.core.Board.take_victim"), held(&[(board_, "other"), (board_, "board")]));
    assert_eq!(gives(&s, "board.core.swap"), held(&[(board_, "board"), (board_, "other")]));
    assert_eq!(gives(&s, "board.core.reset"), held(&[(board_, "board")]));
    assert_eq!(gives(&s, "board.core.add_tag"), held(&[("ext:builtins.list", "tags")]));
    assert_eq!(gives(&s, "board.core.count"), held(&[("ext:builtins.dict", "counts")]));
    assert_eq!(gives(&s, "board.core.queue"), held(&[("ext:builtins.list", "jobs")]));
    assert_eq!(gives(&s, "board.core.extend"), held(&[("ext:builtins.list", "xs")]));
    let main = s.node("board.core.main").unwrap();
    let add_tag = main.sites.iter().find(|x| x.to == "board.core.add_tag").unwrap();
    assert!(add_tag.hands.is_empty(), "a literal under a name is no value handed: {:?}", add_tag.hands);

    assert_eq!(gives(&c, "board.core.Board.mark_all"), held(&[(board_, "board")]));
    assert_eq!(gives(&c, "board.core.Board.reset"), held(&[(board_, "board")]));
    assert!(gives(&c, "board.core.renamed").iter().all(|(_, n)| n != "board"), "{:?}", gives(&c, "board.core.renamed"));
    assert!(gives(&c, "board.core.extended").is_empty(), "{:?}", gives(&c, "board.core.extended"));
}

/// A function changes what it is handed by whatever changes it: a method of the value's own that
/// changes it (`tally.add(n)`), `setattr`, `del`. The clean edits hand back a new value
/// (`dataclasses.replace`, a new `Tally`) and leave the change to the method itself; on both sides
/// `deeper` calls a method named `update` that hands back a new value and changes nothing. The
/// shortcut's `tally_up`, `rename` and `forget` change the value they are handed, and each is asked.
#[test]
fn changes_a_method_setattr_and_del_change_what_a_function_is_handed() {
    let (c, s) = (board("clean"), board("shortcut"));
    assert!(found_in(&c, "changes", "board.edits").is_empty(), "{:?}", found_in(&c, "changes", "board.edits"));
    assert_eq!(found_in(&s, "changes", "board.edits"), ["board.edits.tally_up", "board.edits.rename", "board.edits.forget"]);
    let changes = |g: &Graph, f: &str, ty: &str, via: &str| {
        g.edges.iter().any(|e| e.kind == EdgeKind::Mutates && e.src == f && e.dst == ty && e.via.as_deref() == Some(via))
    };
    assert!(changes(&s, "board.edits.tally_up", "board.edits.Tally", "tally"));
    assert!(changes(&s, "board.edits.rename", "board.core.Board", "board"));
    assert!(changes(&s, "board.edits.forget", "board.core.Board", "board"));
    for g in [&c, &s] {
        assert!(changes(g, "board.edits.Tally.add", "board.edits.Tally", "self"), "a method changes its own value");
        assert!(edge(g, "board.edits.deeper", "board.edits.Settings.update", EdgeKind::Calls));
        assert!(g.edges.iter().all(|e| !(e.kind == EdgeKind::Mutates && e.src == "board.edits.deeper")), "`update` changes nothing");
    }
}

// ---------- layers: the package quotient the picture draws ----------

/// One side of the layers pair (`tests/pairs/layers/python`): a shop whose orders price their
/// lines, a kitchen beside it that cooks from the shop's orders, and tests inside the shop that
/// cook too.
fn shop_python(side: &str) -> Graph {
    analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/layers/python").join(side), &Options::default()).expect("reads")
}

/// The packages of a part depend on one another in one direction, at every level the picture
/// draws: a package's own module (`orders/__init__.py`) is a block beside the packages below it,
/// and the top-level packages of a part are blocks beside one another. The clean side keeps what
/// both need below both: `lines` takes `tax` from a module of its own, and the shop asks nothing of
/// the kitchen. The shortcut writes `tax` in `orders/__init__.py` and imports it from there inside
/// `price`, past the import cycle, and lets the shop's `menu` cook from the kitchen that cooks from
/// the shop.
#[test]
fn layers_a_package_and_its_own_module_and_the_top_packages_depend_in_one_direction() {
    let (c, s) = (shop_python("clean"), shop_python("shortcut"));
    assert!(found(&c, "layers").is_empty(), "{:?}", check(&c).iter().map(|f| &f.what).collect::<Vec<_>>());
    assert!(edge(&c, "shop.orders.total", "shop.orders.lines.price", EdgeKind::Calls));
    assert!(edge(&c, "kitchen.cook.boil", "shop.orders.total", EdgeKind::Calls));
    assert_eq!(found(&s, "layers"), ["kitchen", "shop.orders"]);
    // kitchen and shop call each other
    assert!(
        edge(&s, "kitchen.cook.boil", "shop.orders.total", EdgeKind::Calls)
            && edge(&s, "shop.menu.dish", "kitchen.cook.boil", EdgeKind::Calls)
    );
    // `orders` itself, the code in `orders/__init__.py`, and `lines` below it call each other
    assert!(edge(&s, "shop.orders.total", "shop.orders.lines.price", EdgeKind::Calls));
    assert!(edge(&s, "shop.orders.lines.price", "shop.orders.tax", EdgeKind::Calls), "the shortcut's call back is in the graph");
    let own = |id: &str| s.node(id).is_some_and(|n| PathBuf::from(&n.file).ends_with("src/shop/orders/__init__.py"));
    assert!(own("shop.orders.total") && own("shop.orders.tax"), "the package's own module");
}

/// A test is a way in from outside, not a layer: a pytest file (`test_*.py`, `*_test.py`,
/// `conftest.py`) is test code wherever it stands. On both sides a test inside `shop.orders` and the
/// shop's `conftest.py` cook from the kitchen, which cooks from the shop; counted as the shop's
/// code, they would close a cycle. They are in the graph, and no cycle comes of them.
#[test]
fn layers_a_test_inside_a_package_is_no_layer() {
    for side in ["clean", "shortcut"] {
        let g = shop_python(side);
        for test in ["shop.orders.test_lines", "shop.conftest"] {
            assert!(g.node(test).is_some_and(|m| m.test), "{side}: {test} is test code");
        }
        assert!(edge(&g, "shop.orders.test_lines.boiled", "kitchen.cook.boil", EdgeKind::Calls), "{side}");
        assert!(edge(&g, "shop.conftest.cooked", "kitchen.cook.boil", EdgeKind::Calls), "{side}");
        assert!(g.node("shop.orders.lines").is_some_and(|m| !m.test), "{side}: a module beside a test is not one");
    }
    // on the clean side the tests cook from the kitchen that cooks from the shop, and no cycle comes of it; the shortcut's
    // cycle between kitchen and shop is the one its `menu` closes, and names no test
    assert!(found(&shop_python("clean"), "layers").is_empty());
    let s = shop_python("shortcut");
    let kitchen = check(&s).into_iter().find(|f| f.principle == "layers" && f.node == "kitchen").expect("kitchen and shop are a cycle");
    let links: Vec<&str> = kitchen.about("from").into_iter().chain(kitchen.about("to")).collect();
    assert!(links.contains(&"shop.menu.dish"), "{links:?}");
    let test = |id: &str| s.node(id).is_some_and(|n| n.test || s.node(&n.module).is_some_and(|m| m.test));
    assert!(test("shop.orders.test_lines.boiled") && test("shop.conftest.cooked"));
    assert!(links.iter().all(|id| !test(id)), "{links:?}");
}

// ---------- prophecy: what leans, and what a name names ----------

fn prophecy_python(topic: &str, side: &str) -> Graph {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/prophecy").join(topic).join("python").join(side);
    analyze(&dir, &Options::default()).expect("reads")
}

/// Test code may lean on the code it is written for before it is built. On the clean side a pytest
/// file inside the package (`test_seal.py`), and the `conftest.py` and `plan_test.py` of a `tests`
/// package inside it, call the prophecy `seal`: Atlas asks nothing. The shortcut writes the same
/// helper in the package's own `check.py`: built code that would run on a stub, and asked.
#[test]
fn prophecy_a_test_leans_on_a_prophecy_freely() {
    let (c, s) = (prophecy_python("tested", "clean"), prophecy_python("tested", "shortcut"));
    assert!(check(&c).is_empty(), "{:?}", check(&c).iter().map(|f| format!("{} {}", f.principle, f.what)).collect::<Vec<_>>());
    for (helper, test) in [
        ("archive.test_seal.sealed_twice", "archive.test_seal"),
        ("archive.tests.conftest.sealed", "archive.tests.conftest"),
        ("archive.tests.plan_test.test_a_sealed_report_keeps_its_text", "archive.tests.plan_test"),
    ] {
        assert!(edge(&c, helper, "archive.plan.seal", EdgeKind::Calls), "{helper} calls the prophecy");
        assert!(c.node(test).is_some_and(|m| m.test), "{test} is test code");
    }
    assert_eq!(found(&s, "prophecy"), ["archive.check.sealed_twice"]);
}

/// A name the prophecy's `supersedes` writes names one node, by its whole id or by the one id that
/// ends so. The clean side names the ledger's `save` by its id and the shelf's by `shelf.save`; both
/// hold. The shortcut writes `save`, which ends the ids of both: the prophecy does not say which it
/// replaces, and Atlas names both rather than call the name one the code does not hold.
#[test]
fn prophecy_a_name_that_names_several_nodes_is_ambiguous() {
    let (c, s) = (prophecy_python("names", "clean"), prophecy_python("names", "shortcut"));
    assert!(found(&c, "prophecy").is_empty(), "{:?}", check(&c).iter().map(|f| &f.what).collect::<Vec<_>>());
    let replaces = |g: &Graph, id: &str| g.node(id).and_then(|n| n.prophecy.as_ref()).and_then(|p| p.replaces.clone());
    assert_eq!(replaces(&c, "archive.plan.save_posted").as_deref(), Some("archive.ledger.save"));
    assert_eq!(replaces(&c, "archive.plan.save_kept").as_deref(), Some("archive.shelf.save"));
    assert_eq!(found_in(&s, "prophecy", "archive.plan"), ["archive.plan.save_posted"]);
    // `save` names both, and the prophecy replaces neither: it names both rather than call the name one the code does not hold
    let posted = s.node("archive.plan.save_posted").and_then(|n| n.prophecy.clone()).expect("a prophecy");
    assert_eq!(posted.supersedes.as_deref(), Some("save"));
    assert_eq!((posted.replaces, posted.candidates), (None, vec!["archive.ledger.save".to_string(), "archive.shelf.save".to_string()]));
}

/// A name the prophecy's `supersedes` writes names a node the code holds. The clean `total_posted`
/// supersedes `archive.totals.total`, and replaces it; the shortcut's names `archive.totals.sum_all`,
/// which the code does not hold, and Atlas asks whether it is gone already or written otherwise.
#[test]
fn prophecy_a_name_that_names_no_node_is_asked() {
    let (c, s) = (prophecy_python("names", "clean"), prophecy_python("names", "shortcut"));
    let said = |g: &Graph| g.node("archive.totals.total_posted").and_then(|n| n.prophecy.clone()).expect("a prophecy");
    assert!(found_in(&c, "prophecy", "archive.totals").is_empty(), "{:?}", found_in(&c, "prophecy", "archive.totals"));
    assert_eq!(said(&c).replaces.as_deref(), Some("archive.totals.total"));
    assert_eq!(found_in(&s, "prophecy", "archive.totals"), ["archive.totals.total_posted"]);
    let gone = said(&s);
    assert_eq!((gone.supersedes.as_deref(), gone.replaces, gone.candidates), (Some("archive.totals.sum_all"), None, vec![]));
    assert!(s.node("archive.totals.sum_all").is_none() && s.node("archive.totals.total").is_some());
}

/// A request to a route leans on its handler as a call does. In each side a client part asks the
/// shop part for an item; the shop serves it, and sketches a second version of the route as a
/// prophecy that supersedes the first. The clean client asks the route that is built; the shortcut's
/// asks the prophesied one, which would answer with a stub. Read as one graph the parts are joined
/// by the request, and Atlas asks about the client's `item`. `atlas check` reads each part alone,
/// where no request crosses to another part.
#[test]
fn prophecy_a_request_to_a_prophesied_route_leans_on_it() {
    let read = |side: &str| {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/prophecy/requests/python").join(side);
        atlas_engine::parts::analyze_repo(&[dir], &Options::default()).expect("reads")
    };
    let (c, s) = (read("clean"), read("shortcut"));
    assert!(edge(&c, "buyer.client.item", "shop.api.item", EdgeKind::Requests));
    assert!(found(&c, "prophecy").is_empty(), "{:?}", check(&c).iter().map(|f| &f.what).collect::<Vec<_>>());
    assert!(edge(&s, "buyer.client.item", "shop.api.item_v2", EdgeKind::Requests));
    assert_eq!(found(&s, "prophecy"), ["buyer.client.item"]);
    // what it requests is a prophecy, and the route it served before is built
    assert!(
        s.node("shop.api.item_v2").is_some_and(|n| n.prophecy.is_some()) && s.node("shop.api.item").is_some_and(|n| n.prophecy.is_none())
    );
}

// ---------- doors: every declaration a door is made of ----------

/// One side of the doors pair (`tests/pairs/doors`): lengths in metres, read from text, scaled,
/// added up and logged, and the squares they measure, the package's doors brought to its root with
/// `from .. import *`.
fn units(side: &str) -> Graph {
    analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/doors/python").join(side), &Options::default()).unwrap()
}

/// A door says what goes in and what comes out in each form Python declares one. The clean root
/// names its doors with `from units.core import *` (the module's `__all__`) and `from
/// units.convert import *` (its public names). `scale` says what it takes and gives through typed
/// `@overload`s, which its callers are checked against, before an implementation without types;
/// `Length.meters` is its getter, typed, before its setter; the dunders a user runs through an
/// operator (`__add__`, `__eq__`) are typed, and so are `log`'s `*parts` and `**fields`. It is
/// asked nothing, and the route of `Length` runs its initializer once, then each name a user may
/// run on it, the dunders among them. The shortcut is the same package with each door left open:
/// an overload without types, an untyped getter behind a typed setter, an untyped `__add__`,
/// untyped `*parts` and `**fields`, and `parse(text: Any) -> Any`, which says no more than no
/// annotation. Each is asked where it is left open.
#[test]
fn doors_every_declaration_of_a_door_says_what_goes_in_and_out() {
    let (c, s) = (units("clean"), units("shortcut"));
    let fs = check(&c);
    assert!(fs.is_empty(), "{:#?}", fs.iter().map(|f| format!("{} {} {}", f.principle, f.node, f.what)).collect::<Vec<_>>());
    for door in ["units.core.Length", "units.core.scale", "units.core.log", "units.convert.parse"] {
        assert_eq!(c.node(door).unwrap().entry.as_ref().map(|e| e.kind), Some(EntryKind::Export), "{door} is a door a star brings");
    }
    // one node per name: the implementation with the overloads its callers see, the property's getter
    let scale = c.node("units.core.scale").unwrap();
    assert_eq!(scale.overloads.len(), 2, "{:?}", scale.overloads);
    assert!(scale.overloads.iter().all(|o| o.params.iter().all(|p| !p.ty.is_empty()) && o.returns.is_some()), "typed overloads");
    let after = scale.span.is_some_and(|(start, _)| scale.overloads.iter().all(|o| o.line < start));
    assert!(scale.params.iter().all(|p| p.ty.is_empty()) && after, "the implementation, as written, after its overloads");
    let meters = c.node("units.core.Length.meters").unwrap();
    assert_eq!(meters.decorators, ["property"]);
    assert_eq!(meters.returns.as_ref().map(|r| r.written.as_str()), Some("float"));
    assert_eq!(c.node("units.core.Length").unwrap().methods, ["__init__", "meters", "__add__", "__eq__"]);
    // the class's route runs its initializer once, then what a user may run on it, the dunders among it
    let steps: Vec<String> = atlas_engine::flow::bodies(&c)["units.core.Length"].steps().iter().map(|s| s.to.clone()).collect();
    assert_eq!(steps, ["__init__", "meters", "__add__", "__eq__"].map(|m| format!("units.core.Length.{m}")));
    // the shortcut: each door left open is asked, at the declaration that leaves it open
    let asked: Vec<(String, u32)> = check(&s)
        .into_iter()
        .filter(|f| f.principle == "doors" && !f.node.starts_with("units.shapes."))
        .map(|f| (f.node, f.line))
        .collect();
    let declared = |id: &str| s.node(id).and_then(|n| n.span).map(|(start, _)| start).unwrap();
    // of `scale`, the declaration left open is its overload without types
    let open_overload = s.node("units.core.scale").unwrap().overloads.iter().find(|o| o.params.iter().any(|p| p.ty.is_empty())).unwrap();
    let want = [
        ("units.convert.parse", declared("units.convert.parse")),
        ("units.core.Length.meters", declared("units.core.Length.meters")),
        ("units.core.Length.__add__", declared("units.core.Length.__add__")),
        ("units.core.scale", open_overload.line),
        ("units.core.log", declared("units.core.log")),
    ];
    assert_eq!(asked, want.map(|(n, l)| (n.to_string(), l)));
    assert_eq!(s.node("units.core.Length.meters").unwrap().returns, None, "the node is the getter, left untyped");
    // `Any` is drawn as written, and says nothing
    let parse = s.node("units.convert.parse").unwrap();
    assert_eq!(parse.params.iter().map(|p| (p.ty.as_str(), p.unchecked)).collect::<Vec<_>>(), [("Any", true)]);
    assert!(parse.returns.as_ref().is_some_and(|r| r.written == "Any" && r.unchecked));
}

/// A class's doors are the names a user may run on it, its own and those it inherits. Both
/// sides export `Square`, which inherits `show` from the mixin `Shown` that the package does not
/// export. The clean `Square.area` and `Shown.show` say what they give; the shortcut leaves both
/// unsaid, and each is asked where it is declared, the inherited one in its mixin. `label` uses
/// what `area` gives, and a door is asked by `doors`, not again by `gives`.
#[test]
fn doors_a_method_of_a_door_says_what_it_gives_and_so_does_what_it_inherits() {
    let (c, s) = (units("clean"), units("shortcut"));
    assert!(found_in(&c, "doors", "units.shapes").is_empty(), "{:?}", found_in(&c, "doors", "units.shapes"));
    assert_eq!(found_in(&s, "doors", "units.shapes"), ["units.shapes.Shown.show", "units.shapes.Square.area"]);
    for g in [&c, &s] {
        assert_eq!(g.node("units.shapes.Square").unwrap().entry.as_ref().map(|e| e.kind), Some(EntryKind::Export));
        assert!(g.node("units.shapes.Shown").unwrap().entry.is_none(), "the mixin is no door of its own");
        assert!(edge(g, "units.shapes.Square", "units.shapes.Shown", EdgeKind::Inherits));
    }
    // the methods left open are in the graph, and what `area` gives is used
    for m in ["units.shapes.Shown.show", "units.shapes.Square.area"] {
        assert_eq!(s.node(m).unwrap_or_else(|| panic!("{m} is in the graph")).returns, None, "{m}");
    }
    assert!(s
        .edges
        .iter()
        .any(|e| e.kind == EdgeKind::Calls && e.uses && e.src == "units.shapes.label" && e.dst == "units.shapes.Square.area"));
    assert!(found_in(&s, "gives", "units.shapes").is_empty(), "{:?}", found_in(&s, "gives", "units.shapes"));
}

/// One side of the nothing pair (`tests/pairs/nothing`): a job runner that runs a task, says when a
/// job is done, reports, counts its runs, settles its log and stops the run, in Python, Rust and
/// TypeScript alike; in Python also a worker that trims names and closes when its work is done.
fn jobs(side: &str) -> Graph {
    analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/nothing/python").join(side), &Options::default()).unwrap()
}

/// A call leaves something behind, and where Atlas cannot see what a call leaves, the question says
/// so. The clean runner leaves each step where a reader sees it: `Echo` subclasses the `Task` it
/// implements, `finish` is handed its callback, `report` writes to the screen, `count` changes a
/// module's global, `settle` writes its log and `stop` ends the run with `sys.exit`, as `raise
/// SystemExit` does: nothing is asked. The shortcut leaves them out of sight: `Echo` fills in a
/// `Protocol` without naming it, so `Task.run` is only declared; `Job` keeps its callback in a
/// field; `report` logs through `logging`, which no table describes; `count` changes a value its
/// class keeps. Atlas asks what each of those leaves behind and names what it cannot see. `settle`,
/// a door that waits and does nothing, is asked whether it is dead, as any function is.
#[test]
fn nothing_says_what_atlas_cannot_see() {
    let (c, s) = (jobs("clean"), jobs("shortcut"));
    assert!(found(&c, "nothing").is_empty(), "{:?}", found(&c, "nothing"));
    for g in [&c, &s] {
        assert!(g.node("jobs.runner.stop").unwrap().raises, "ending the run is what `stop` leaves behind");
    }
    let want = ["run_task", "Job.finish", "report", "count", "settle"].map(|f| format!("jobs.runner.{f}"));
    assert_eq!(found_in(&s, "nothing", "jobs.runner"), want);
    // what each runs that Atlas cannot see into, as the code writes it: a method only declared, a call on a field, a call
    // into a library no table describes, a value its class keeps; `settle` runs nothing at all
    assert!(edge(&s, "jobs.runner.run_task", "jobs.runner.Task.run", EdgeKind::Calls));
    for (f, code) in [("Job.finish", "self.on_done"), ("report", "logger.info"), ("count", "Counter.runs")] {
        let unseen = &s.node(&format!("jobs.runner.{f}")).unwrap().unseen;
        assert!(unseen.len() == 1 && unseen[0].1.contains(code), "{f}: {unseen:?}");
    }
    let settle = s.node("jobs.runner.settle").unwrap();
    assert!(settle.unseen.is_empty() && settle.sites.is_empty() && settle.touches.is_empty(), "{settle:?}");
    assert!(s.edges.iter().all(|e| !(e.src == "jobs.runner.settle" && e.kind == EdgeKind::Mutates)));
    // what is only declared has no body to ask about, and what implements it is still drawn
    assert!(s.node("jobs.runner.Task.run").unwrap().declared && c.node("jobs.runner.Task.run").unwrap().declared);
    assert_eq!(touches(&s, "jobs.runner.Echo.run", "screen").len(), 1);
    assert_eq!(s.node("jobs.runner.settle").unwrap().entry.as_ref().map(|e| e.kind), Some(EntryKind::Export), "settle is a door");
}

/// A call leaves something behind, also a call through `self` that a subclass makes to what it
/// inherits, and a call whose body computes a value only to drop it. On both sides `Worker.work`
/// trims each name and closes, `close` inherited from `Closing`. The clean `close` writes its log
/// and `trim` gives the trimmed name; the shortcut's `close` is `pass` and its `trim` drops what
/// `strip` gives. Atlas sees all that each runs, and asks whether each is dead.
#[test]
fn nothing_an_inherited_method_that_does_nothing_and_a_body_that_drops_its_value() {
    let (c, s) = (jobs("clean"), jobs("shortcut"));
    assert!(found_in(&c, "nothing", "jobs.closing").is_empty(), "{:?}", found_in(&c, "nothing", "jobs.closing"));
    assert_eq!(found_in(&s, "nothing", "jobs.closing"), ["jobs.closing.Closing.close", "jobs.closing.trim"]);
    for g in [&c, &s] {
        assert!(
            edge(g, "jobs.closing.Worker.work", "jobs.closing.Closing.close", EdgeKind::Calls),
            "`self.close()` runs the inherited close"
        );
        assert!(edge(g, "jobs.closing.Worker.work", "jobs.closing.trim", EdgeKind::Calls));
    }
    assert!(touches(&c, "jobs.closing.Closing.close", "disk_write").len() == 1, "the clean close writes its log");
    for f in ["jobs.closing.Closing.close", "jobs.closing.trim"] {
        let n = s.node(f).unwrap_or_else(|| panic!("{f} is in the graph"));
        assert!(n.touches.is_empty() && n.unseen.is_empty(), "{f}: Atlas sees all it runs: {n:?}");
    }
}

/// One side of the gives pair (`tests/pairs/gives`): readings kept in a file, how many there are,
/// and the readings ranked, in Python and TypeScript alike; in Python also a journal kept in SQLite.
fn readings(side: &str) -> Graph {
    analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/gives/python").join(side), &Options::default()).unwrap()
}

/// A function whose value is used says what it gives back, also where it is handed on to be used:
/// `sorted(readings, key=score)` orders the readings by what `score` gives. The clean store says
/// what each of its functions gives; the shortcut leaves `get_store` and `score` unsaid, and
/// `gives` asks about both. The route loses `Store.history` at the store `get_store` makes, and
/// the key `ranked` sorts by has no type.
#[test]
fn gives_a_key_handed_on_is_a_value_used() {
    let (c, s) = (readings("clean"), readings("shortcut"));
    assert!(found(&c, "gives").is_empty(), "{:?}", found(&c, "gives"));
    assert_eq!(found_in(&s, "gives", "readings.store"), ["readings.store.get_store", "readings.store.score"]);
    for g in [&c, &s] {
        assert!(
            g.edges
                .iter()
                .any(|e| e.kind == EdgeKind::Calls && e.uses && e.src == "readings.store.ranked" && e.dst == "readings.store.score"),
            "a key handed on is used"
        );
    }
    assert!(edge(&c, "readings.store.main", "readings.store.Store.history", EdgeKind::Calls));
    assert!(!edge(&s, "readings.store.main", "readings.store.Store.history", EdgeKind::Calls), "the route loses the store's history");
}

/// A property is a function its readers run: an untyped one gives what they use without saying
/// (`store.size`). A property read is no call in the graph, so `gives` does not ask.
#[test]
#[ignore = "gap (b): a property read is no call in the graph, so an untyped property is not asked"]
fn gives_a_property_read_is_a_value_used() {
    assert!(found(&readings("shortcut"), "gives").contains(&"readings.store.Store.size".to_string()));
}

/// A method whose value its own class uses says what it gives, as a function does. On both sides
/// `Journal.entries` runs its statement on the connection `self.connection()` gives. The clean
/// `connection` says it is a `sqlite3.Connection`; the shortcut's says nothing, and is asked.
#[test]
fn gives_a_method_its_own_class_uses_says_what_it_gives() {
    let (c, s) = (readings("clean"), readings("shortcut"));
    assert!(found_in(&c, "gives", "readings.journal").is_empty(), "{:?}", found_in(&c, "gives", "readings.journal"));
    assert_eq!(found_in(&s, "gives", "readings.journal"), ["readings.journal.Journal.connection"]);
    for g in [&c, &s] {
        assert!(g.edges.iter().any(|e| e.kind == EdgeKind::Calls
            && e.uses
            && e.src == "readings.journal.Journal.entries"
            && e.dst == "readings.journal.Journal.connection"));
    }
    assert_eq!(
        c.node("readings.journal.Journal.connection").unwrap().returns.as_ref().map(|r| r.written.as_str()),
        Some("sqlite3.Connection")
    );
    assert_eq!(s.node("readings.journal.Journal.connection").unwrap().returns, None);
}

/// Where the program starts may name places, and a program `pyproject.toml` declares is where it
/// starts, whatever layout holds it. `places/python/flat` is one module, `tally.py`, declared as
/// `tally = "tally:main"`, so its package exports `main` with every other public name;
/// `places/python/package` is the same code in `src/tally/cli.py`, declared as
/// `tally = "tally.cli:main"`, its package exporting `count` and `keep`. Both draw one route: the
/// command `tally --source --out`, whose parse of the command line is the entry's, with the same
/// steps and places, and `places` asks neither. The flat `main` is a door as well.
#[test]
fn places_a_declared_program_is_a_command_flat_or_in_a_package() {
    let read = |side: &str| {
        analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/places/python").join(side), &Options::default()).unwrap()
    };
    let (flat, package) = (read("flat"), read("package"));
    let entries = |g: &Graph| -> Vec<(EntryKind, String, Vec<String>)> {
        g.nodes.iter().filter_map(|n| n.entry.as_ref()).map(|e| (e.kind, e.label.clone(), e.options.clone())).collect()
    };
    assert_eq!(entries(&flat), entries(&package));
    assert!(entries(&flat).contains(&(EntryKind::Cli, "tally".into(), vec!["--source".into(), "--out".into()])), "{:?}", entries(&flat));
    // the same route, its steps named alike once the module the code is in is left out
    let steps = |g: &Graph, main: &str, module: &str| route(g, main).replace(module, "");
    assert_eq!(steps(&flat, "tally.main", "tally."), steps(&package, "tally.cli.main", "tally.cli."));
    assert!(!route(&flat, "tally.main").contains("parse_args"), "{}", route(&flat, "tally.main"));
    assert!(found(&flat, "places").is_empty() && found(&package, "places").is_empty(), "{:?}", found(&flat, "places"));
    assert!(flat.node("tally.main").is_some_and(|n| n.exported), "the flat package exports its command");
    assert!(package.node("tally.cli.main").is_some_and(|n| !n.exported), "{:?}", package.node("tally.cli.main").map(|n| &n.entry));
}

/// The places pair, `places/python`: rows read and kept, pages made from a template the package
/// ships, a report a command writes, rows kept by kind and a cache, the same on both sides but for
/// who decides where each lives.
fn stock(side: &str) -> Graph {
    analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/places/python").join(side), &Options::default()).unwrap()
}

/// Library code does not decide where data lives, and a function that hands a path decides it as
/// much as one that opens it. Both sides read rows with `read_rows(path)`. The clean command hands
/// it `Path("data/rows.csv")`, as where the program starts may; on the shortcut the library's
/// `default_rows()` hands it, one call away from the file. `places` asks `default_rows`, whose own
/// code writes the path, and not `read_rows`, which takes it.
#[test]
fn places_the_function_that_writes_the_path_decides_it() {
    let (c, s) = (stock("clean"), stock("shortcut"));
    assert!(found(&c, "places").is_empty(), "{:?}", found(&c, "places"));
    assert_eq!(found_in(&s, "places", "stock.rows"), ["stock.rows.default_rows"]);
    let read = |g: &Graph, by: &str| {
        touches(g, "stock.rows.read_rows", "disk_read").iter().any(|t| t.artifact == "data/rows.csv" && !t.fixed && t.fixed_by == [by])
    };
    assert!(read(&c, "stock.cli.main") && read(&s, "stock.rows.default_rows"));
    // the file `default_rows` decides is read in `read_rows`, which it calls
    assert!(edge(&s, "stock.rows.default_rows", "stock.rows.read_rows", EdgeKind::Calls));
}

/// A default whoever calls the function or starts the program can override is an option, not a
/// decision: a parameter's (`path: Path = Path("out/rows.csv")`, `path or Path("out/rows.csv")`)
/// and an environment variable's (`os.environ.get("STOCK_PATH", "out/rows.csv")`,
/// `os.getenv("STOCK_PATH") or "out/rows.csv"`). The forms say the same thing and draw alike: one
/// place, the file the code takes where nobody says otherwise, the caller's to choose, as an option
/// of the command line is. `places` asks none of them.
#[test]
fn places_a_default_the_caller_can_override_is_an_option() {
    let c = stock("clean");
    assert!(found(&c, "places").is_empty(), "{:?}", found(&c, "places"));
    let drawn = |f: &str| -> Vec<(String, String, bool)> {
        let mut out: Vec<_> =
            touches(&c, f, "disk_write").iter().map(|t| (t.artifact.clone(), t.what.clone(), t.fixed || !t.fixed_by.is_empty())).collect();
        out.sort();
        out
    };
    assert_eq!(drawn("stock.rows.save_where_told"), drawn("stock.rows.save_or_default"));
    let told = drawn("stock.rows.save_where_told");
    assert!(told.iter().any(|(file, _, decided)| file == "out/rows.csv" && !decided), "{told:?}");
    assert!(told.iter().all(|(file, _, _)| file == "out/rows.csv"), "one place, the variable its input: {told:?}");
    for f in ["stock.rows.save", "stock.rows.save_or_given"] {
        assert!(drawn(f).iter().any(|(file, _, decided)| file == "out/rows.csv" && !decided), "{f}: {:?}", drawn(f));
    }
}

/// A file that ships with the package, found from where its code lies, is part of the package and
/// no place its user chooses: `Path(__file__).parent / "template.html"` and
/// `importlib.resources.files("stock").joinpath("template.html")` read the same file, and `places`
/// asks neither.
#[test]
fn places_a_file_shipped_with_the_package_is_part_of_it() {
    let c = stock("clean");
    for f in ["stock.pages.page", "stock.pages.resource_page"] {
        let read = touches(&c, f, "disk_read");
        assert!(read.iter().any(|t| t.artifact == "src/stock/template.html" && !t.fixed && t.fixed_by.is_empty()), "{f}: {read:?}");
    }
    assert!(found(&c, "places").is_empty(), "{:?}", found(&c, "places"));
}

/// Where the program starts is the entry itself; a function beside it is library code. On both
/// sides the command `report` names the file it writes, as an entry may, and is asked nothing.
/// The clean command hands `summary` the file it writes; the shortcut's `summary` names its own,
/// and is asked.
#[test]
fn places_a_function_beside_a_command_is_library_code() {
    let (c, s) = (stock("clean"), stock("shortcut"));
    assert!(found_in(&c, "places", "stock.report").is_empty(), "{:?}", found_in(&c, "places", "stock.report"));
    assert_eq!(found_in(&s, "places", "stock.report"), ["stock.report.summary"]);
    for g in [&c, &s] {
        assert_eq!(g.node("stock.report.report").unwrap().entry.as_ref().map(|e| e.kind), Some(EntryKind::Cli));
        assert!(touches(g, "stock.report.report", "disk_write").iter().any(|t| t.artifact == "out/report.txt" && t.fixed));
        assert!(edge(g, "stock.report.report", "stock.report.summary", EdgeKind::Calls));
    }
    let written =
        |g: &Graph| touches(g, "stock.report.summary", "disk_write").iter().map(|t| (t.artifact.clone(), t.fixed)).collect::<Vec<_>>();
    assert_eq!(written(&s), [("out/summary.txt".to_string(), true)]);
    assert!(written(&c).iter().all(|(_, fixed)| !fixed), "{:?}", written(&c));
}

/// A module's constant is a place the library decides itself. The shortcut's `kept` keeps its
/// rows under `DATA = Path("data")`, and each function that reaches a file through it is asked:
/// one that lists `data/raw/`, one that writes and one that reads `data/{kind}/report.json`, one
/// that copies to `data/config.toml` with `shutil`, one that writes `data/frame.csv` with pandas;
/// and so is `Sink.write`, a method that names its own file. The clean `kept` takes the folder,
/// and its `Sink` the file, from whoever calls.
#[test]
fn places_a_module_s_constant_decides_where_data_lives() {
    let (c, s) = (stock("clean"), stock("shortcut"));
    assert!(found_in(&c, "places", "stock.kept").is_empty(), "{:?}", found_in(&c, "places", "stock.kept"));
    let want = ["kinds", "report", "read_report", "install", "export", "Sink.write"].map(|f| format!("stock.kept.{f}"));
    assert_eq!(found_in(&s, "places", "stock.kept"), want);
    // each still reaches its file, in the direction its call goes
    for (f, kind, file) in [
        ("kinds", "disk_read", "data/raw/"),
        ("report", "disk_write", "data/{kind}/report.json"),
        ("read_report", "disk_read", "data/{kind}/report.json"),
        ("install", "disk_write", "data/config.toml"),
        ("export", "disk_write", "data/frame.csv"),
        ("Sink.write", "disk_write", "out.json"),
    ] {
        let at = touches(&s, &format!("stock.kept.{f}"), kind);
        assert!(at.iter().any(|t| t.artifact == file && t.fixed), "{f}: {at:?}");
    }
}

/// A function that builds a value on a folder decides the place the value's method writes under,
/// as it would had it written the file itself. On both sides a `Cache` keeps entries under the
/// root its initializer is handed, and a `Bin`, a dataclass, under its field `root`; each default
/// is an option and asked nothing. The clean `warm` and `tidy` hand on the root they are handed;
/// the shortcut's build theirs on `Path("warm")` and `Path("tidy")`, and are asked.
#[test]
fn places_a_folder_handed_to_a_constructor_is_decided_by_the_function_that_hands_it() {
    let (c, s) = (stock("clean"), stock("shortcut"));
    assert!(found_in(&c, "places", "stock.cache").is_empty(), "{:?}", found_in(&c, "places", "stock.cache"));
    assert_eq!(found_in(&s, "places", "stock.cache"), ["stock.cache.warm", "stock.cache.tidy"]);
    for (method, by, file) in [("Cache.keep", "warm", "warm/a.txt"), ("Bin.drop", "tidy", "tidy/x.txt")] {
        let at = touches(&s, &format!("stock.cache.{method}"), "disk_write");
        assert!(at.iter().any(|t| t.artifact == file && t.fixed_by == [format!("stock.cache.{by}")]), "{method}: {at:?}");
        assert!(!touches(&c, &format!("stock.cache.{method}"), "disk_write").iter().any(|t| t.fixed || !t.fixed_by.is_empty()), "{method}");
    }
}

/// One side of the diamond pair (`tests/pairs/diamond`): a feed that is a buffered and a cached
/// source at once, `Feed(Buffered, Cached)` over `Source`, once as its author says which `read`
/// and which initializer a feed runs and once as the order of its bases decides it.
fn news() -> &'static Pair {
    static PAIR: OnceLock<Pair> = OnceLock::new();
    PAIR.get_or_init(|| common::pair("diamond/python"))
}

/// A value runs, for a name its class does not define, the first definition along its class's
/// method resolution order, the order the rules walk a class's ancestors in (`linearization`).
/// `Feed(Buffered, Cached)` over `Source` runs Feed, Buffered, Cached, Source: a feed's `read` is
/// `Cached.read`, though `Buffered`, its first base, inherits `Source.read`, and building a feed
/// takes what `Cached.__init__` takes. The clean `Feed` says which it runs, its own `read` and
/// initializer calling `Cached`'s. The shortcut leaves it to the order of its bases: its route runs
/// `Cached.read` from the code of `Buffered`, whose call names `Source.read`, and reads the file
/// the cache keeps, not the network.
#[test]
fn diamond_a_value_runs_the_method_its_method_resolution_order_names() {
    let class = |c: &str| format!("news.sources.{c}");
    for (side, g, read, init) in [("clean", &news().clean, "Feed", "Feed"), ("shortcut", &news().shortcut, "Cached", "Cached")] {
        let bases = |cls: &'static str| -> Vec<&'static str> {
            g.edges.iter().filter(|e| e.kind == EdgeKind::Inherits && e.src == cls).map(|e| e.dst.as_str()).collect()
        };
        let order = atlas_engine::linearization("news.sources.Feed", &bases, &mut Default::default());
        assert_eq!(order, ["Feed", "Buffered", "Cached", "Source"].map(class), "{side}");
        let first = |name: &str| order.iter().map(|c| format!("{c}.{name}")).find(|m| g.node(m).is_some()).unwrap();
        assert_eq!(first("read"), format!("{}.read", class(read)), "{side}");
        assert_eq!(first("__init__"), format!("{}.__init__", class(init)), "{side}");
        let bodies = atlas_engine::flow::bodies(g);
        let lines = &bodies[&atlas_engine::flow::key("news.sources.Buffered.lines", &class("Feed"))];
        let ran: Vec<&str> = lines.steps().iter().map(|s| s.to.as_str()).collect();
        assert_eq!(ran, [first("read")], "{side}");
        assert_eq!(atlas_engine::flow::arrows(g)[&class("Feed")].takes, ["news.store.Store"], "{side}");
        assert!(edge(g, "news.sources.Buffered.lines", "news.sources.Source.read", EdgeKind::Calls), "{side}");
        // the route reads the store, and never runs the read that asks the network
        let route = crate::ran(g, "news.read.headlines");
        assert!(route.contains("news.store.Store.get") && !route.contains("news.sources.Source.read"), "{side}: {route:?}");
    }
    let asked: Vec<String> = check(&news().clean).into_iter().map(|f| format!("{} {}", f.principle, f.node)).collect();
    assert!(asked.is_empty(), "{asked:?}");
}

/// A call through a class (`Cached.read(self)`) runs that class's method and no override of it:
/// the clean `Feed.read` runs `Cached.read`, which runs nothing in its place, on the feed it is
/// handed first. `Cached.__init__(self, url, store)` hands `self` as the receiver, so the file the
/// store reads is named by the feed's `url`.
#[test]
fn diamond_a_call_through_a_class_runs_its_method() {
    let (g, headlines) = (&news().clean, "news.read.headlines");
    let steps = runs(g, headlines);
    assert!(steps.iter().all(|(_, alts)| alts.is_empty()), "no step may run another in its place: {steps:?}");
    assert!(ran(g, headlines).contains("news.sources.Cached.read") && ran(g, headlines).contains("news.store.Store.get"));
    let read = touches(g, "news.store.Store.get", "disk_read");
    assert!(read.iter().any(|t| t.what == "{cache}/{url}" && t.by.iter().any(|b| b == headlines)), "{read:?}");
    let bodies = atlas_engine::flow::bodies(&news().clean);
    let read = &bodies[&atlas_engine::flow::key("news.sources.Feed.read", "news.sources.Feed")];
    let ran: Vec<(&str, &str, bool)> = read.steps().iter().map(|s| (s.to.as_str(), s.on.as_str(), s.named)).collect();
    assert_eq!(ran, [("news.sources.Cached.read", "news.sources.Feed", true)]);
}

/// One side of the cooperative pair (`tests/pairs/cooperative`): a feed that is a trimmed and a
/// cached source at once, `Feed(Trimmed, Cached)` over `Source`, whose every `read` hands on with
/// `super().read()`; the shortcut's `Trimmed.read` names the `read` it hands on to.
fn cooperative() -> &'static Pair {
    static PAIR: OnceLock<Pair> = OnceLock::new();
    PAIR.get_or_init(|| common::pair("cooperative/python"))
}

/// `super().read()` written in a class runs the next `read` after that class along the order of
/// the value's class: on a feed (Feed, Trimmed, Cached, Source) the clean `Trimmed.read` runs
/// `Cached.read`, which reads the store before it asks the network, though ty resolves the call
/// along `Trimmed`'s own order, to `Source.read`. Where the value's class is not known, as in
/// `lines(source: Trimmed)`, a trimmed source may be a feed, and the call runs `Source.read` or
/// `Cached.read`. The shortcut's `Trimmed.read` calls `Source.read(self)`, a call through a class:
/// it runs that method and nothing in its place, so a feed never reads its cache and the route asks
/// the network alone. Neither side draws a call as one of the overrides below the method it runs.
#[test]
fn cooperative_super_runs_the_next_method_along_the_value_s_order() {
    let class = |c: &str| format!("news.sources.{c}");
    let key = |f: &str, on: &str| atlas_engine::flow::key(&format!("{}.read", class(f)), &class(on));
    for (side, g, next) in [("clean", &cooperative().clean, "Cached"), ("shortcut", &cooperative().shortcut, "Source")] {
        let bodies = atlas_engine::flow::bodies(g);
        let trimmed: Vec<(&str, bool)> = bodies[&key("Trimmed", "Feed")].steps().iter().map(|s| (s.to.as_str(), s.named)).collect();
        assert_eq!(trimmed, [(format!("{}.read", class(next)).as_str(), true)], "{side}");
        assert!(edge(g, &format!("{}.read", class("Trimmed")), &format!("{}.read", class("Source")), EdgeKind::Calls), "{side}");
        let steps = runs(g, "news.read.headlines");
        assert!(steps.iter().all(|(_, alts)| alts.is_empty()), "{side}: {steps:?}");
        assert_eq!(ran(g, "news.read.headlines").contains("news.store.Store.get"), side == "clean", "{side}: {steps:?}");
    }
    let bodies = atlas_engine::flow::bodies(&cooperative().clean);
    let cached: Vec<&str> = bodies[&key("Cached", "Feed")].steps().iter().map(|s| s.to.as_str()).collect();
    assert_eq!(cached, ["news.store.Store.has", "news.store.Store.get", "news.sources.Source.read", "news.store.Store.put"]);
    let unknown = &bodies["news.sources.Trimmed.read"].steps()[0];
    assert_eq!(
        (unknown.to.as_str(), unknown.instead.as_slice()),
        ("news.sources.Source.read", [atlas_engine::flow::key("news.sources.Cached.read", "news.sources.Feed")].as_slice())
    );
    // the route of `lines` runs that call, which downloads, or in its place the cached read on a feed
    let (c, s) = (&cooperative().clean, &cooperative().shortcut);
    let cached = atlas_engine::flow::key("news.sources.Cached.read", "news.sources.Feed");
    let steps = runs(c, "news.read.lines");
    assert!(steps.contains(&("news.sources.Source.read".to_string(), vec![cached])), "{steps:?}");
    assert!(!touches(c, "news.sources.Source.read", "net").is_empty());
    let steps = runs(s, "news.read.lines");
    assert!(steps.iter().all(|(_, alts)| alts.is_empty()) && !ran(s, "news.read.lines").contains("news.sources.Cached.read"), "{steps:?}");
}

/// A call by name on a value whose class is not known may run, for each class the value may be,
/// the method that class runs for the name along its order (#140). In `cached_lines(source:
/// Cached)` a cached source may be a feed, `Feed(Trimmed, Cached)`, which runs `Trimmed.read`
/// first, though `Trimmed` is no class below `Cached`: the call is `Cached.read` or, on a feed,
/// `Trimmed.read`, drawn as the method of the class that runs it (`Feed.read`, #155), on either
/// side. What the pair lays bare is what the sibling does then: the clean `Trimmed.read`
/// hands on with `super().read()`, which on a feed runs `Cached.read`, so a cached source still
/// reads its store; the shortcut's `Source.read(self)` asks the network, and a cached source that
/// is a feed never reads its cache.
#[test]
fn cooperative_a_call_by_name_may_run_a_sibling_s_method() {
    for (side, g) in [("clean", &cooperative().clean), ("shortcut", &cooperative().shortcut)] {
        let bodies = atlas_engine::flow::bodies(g);
        let lines = &bodies["news.read.cached_lines"].steps()[0];
        assert_eq!(lines.to, "news.sources.Cached.read", "{side}");
        assert_eq!(lines.alts(&bodies), [atlas_engine::flow::key("news.sources.Trimmed.read", "news.sources.Feed")], "{side}");
        // the route runs the call, or in its place the sibling's method on a feed, whose steps `trimmed` below names
        let sibling = atlas_engine::flow::key("news.sources.Trimmed.read", "news.sources.Feed");
        let steps = runs(g, "news.read.cached_lines");
        assert!(steps.contains(&("news.sources.Cached.read".to_string(), vec![sibling])), "{side}: {steps:?}");
    }
    let trimmed = |g: &Graph| -> Vec<String> {
        let bodies = atlas_engine::flow::bodies(g);
        bodies[&atlas_engine::flow::key("news.sources.Trimmed.read", "news.sources.Feed")].steps().iter().map(|s| s.to.clone()).collect()
    };
    assert_eq!(trimmed(&cooperative().clean), ["news.sources.Cached.read"]);
    assert_eq!(trimmed(&cooperative().shortcut), ["news.sources.Source.read"]);
}

/// The file a feed reads is named in the terms of the route that builds it, on either side: the
/// cache directory `headlines` is handed, then the name of the feed in it. A class's call hands its
/// arguments to the initializer it runs, its own or the first along its bases: the shortcut's
/// `Feed(url, Store(cache))` hands them to `Cached.__init__`, which `Feed` and `Buffered` leave to
/// it.
#[test]
fn diamond_the_file_a_feed_reads_is_named_where_it_is_built() {
    for (side, g) in [("clean", &news().clean), ("shortcut", &news().shortcut)] {
        assert!(ran(g, "news.read.headlines").contains("news.store.Store.get"), "{side}");
        let read = touches(g, "news.store.Store.get", "disk_read");
        assert!(read.iter().any(|t| t.what.starts_with("{cache}/") && t.by.iter().any(|b| b == "news.read.headlines")), "{side}: {read:?}");
    }
}

/// A request goes the way its method says, and urllib says the method by what a request is handed
/// (#160): `urlopen(url)` without data is a `GET`, which takes what the feed holds; handed data it is
/// a `POST`, which leaves something there; a `Request(..)` it opens goes by the `method` it is built
/// with. The clean client says the way of each request. The shortcut's `call` is handed data that may
/// be `None` and its `remove` builds its request with a method that is a value: the code does not say
/// which way either goes, so the feed is in neither domain nor codomain. Its `latest` still asks.
#[test]
fn ways_a_request_goes_the_way_its_data_says() {
    use atlas_engine::flow::Side;
    let ways = common::pair("ways/python");
    let side = |g: &Graph, f: &str| -> Vec<Side> { touches(g, f, "net").into_iter().map(|t| t.side()).collect() };
    assert_eq!(side(&ways.clean, "feeds.latest"), [Side::In], "a GET reads");
    assert_eq!(side(&ways.clean, "feeds.post"), [Side::Out], "a request handed data writes");
    assert_eq!(side(&ways.clean, "feeds.remove"), [Side::Out], "a Request built with DELETE writes");
    assert_eq!(side(&ways.shortcut, "feeds.latest"), [Side::In], "a GET reads");
    assert_eq!(side(&ways.shortcut, "feeds.call"), [Side::Open], "data that may be None says no way");
    assert_eq!(side(&ways.shortcut, "feeds.remove"), [Side::Open], "a method that is a value says no way");
}

/// A method called on what a library's call gave is that library's operation, also where its types do not list it
/// (#125): `with urlopen(self.url) as response: return response.read()` names what `read` gives nowhere and changes
/// nothing it runs on, so it is no step; `urlopen`, whose value the body keeps, is.
#[test]
fn cooperative_a_method_on_a_library_s_value_is_the_library_s() {
    for (side, g) in [("clean", &cooperative().clean), ("shortcut", &cooperative().shortcut)] {
        let read = g.node("news.sources.Source.read").unwrap();
        let ops: Vec<&str> = read.sites.iter().map(|s| s.op.as_str()).collect();
        assert_eq!(ops, ["urlopen"], "{side}: {:#?}", read.sites);
        assert!(read.uses.iter().any(|u| u.name == "response"), "{side}: the body reads what urlopen gave");
    }
}

/// A body that sets a part of a value through the name that holds it says what it sets it to (#11, `Node::sets`):
/// the value it changes, and what the assigned expression reads, a lens's put made in place. The clean board sets
/// only its own pen, to the pen it is handed (`take`), and lends by handing its pen to the other's `take`; the
/// shortcut's `lend` reaches into the other board and sets its pen there to what the board it runs on holds
/// (`other.pen = self.pen`). What the right side reads is a use of the body's own code.
#[test]
fn sets_a_body_says_what_it_sets_a_part_of_a_value_to() {
    let p = common::pair("sets/python");
    let sets = |g: &Graph, id: &str| -> Vec<(String, String, Vec<(String, String)>)> {
        let held = |h: &atlas_engine::Held| (h.ty.clone(), h.name.clone());
        g.node(id).unwrap().sets.iter().map(|s| (s.on.ty.clone(), s.on.name.clone(), s.from.iter().map(held).collect())).collect()
    };
    let board = || "boards.Board".to_string();
    let pen = vec![("boards.Pen".to_string(), "pen".to_string())];
    assert_eq!(sets(&p.clean, "boards.Board.take"), [(board(), String::new(), pen.clone())], "its own pen, to the pen it is handed");
    assert!(sets(&p.clean, "boards.Board.lend").is_empty(), "the other board takes it itself");
    let take = p.clean.node("boards.Board.lend").unwrap().sites.iter().find(|s| s.to == "boards.Board.take").expect("lend hands it on");
    assert_eq!(
        take.hands.iter().map(|h| (h.name.as_str(), h.fills.clone())).collect::<Vec<_>>(),
        [("other", vec![atlas_engine::Slot::Receiver]), ("", vec![atlas_engine::Slot::At(0)])]
    );
    assert_eq!(
        sets(&p.shortcut, "boards.Board.lend"),
        [(board(), "other".to_string(), vec![(board(), String::new())])],
        "the other's pen, to its own"
    );
    let uses = &p.shortcut.node("boards.Board.lend").unwrap().uses;
    assert!(uses.iter().any(|u| u.name.is_empty() && u.ty == "boards.Board"), "`self.pen` is read: {uses:#?}");
}

/// A parameter of no type of the code's is a name like any other (#128, #157), in Python as in Rust and TypeScript: inside
/// its function it holds what its call fills it with, so a step written with it is handed it by its name, and the body's
/// own code reads it there; where every call fills it with a literal, it holds none (`builder::settle_parameters`). The
/// choices pair tells the two forms apart: the shortcut's `finished(job, mode: str)` hands the string it is filled with on
/// to `Runner(mode)` and `timeout(mode)`, the clean's `mode: Mode` is a value of the code's own type.
#[test]
fn a_parameter_of_no_type_is_handed_by_its_name_where_a_call_fills_it() {
    let p = common::pair("choices/python");
    let hands = |g: &Graph, id: &str, to: &str| -> Vec<(String, String)> {
        let site = g.node(id).unwrap().sites.iter().find(|s| s.to.ends_with(to)).unwrap_or_else(|| panic!("{id} calls {to}"));
        site.hands.iter().map(|h| (h.ty.clone(), h.name.clone())).collect()
    };
    let mode = vec![(String::new(), "mode".to_string())];
    assert_eq!(hands(&p.shortcut, "runs.core.finished", ".Runner"), mode);
    assert_eq!(hands(&p.shortcut, "runs.core.finished", ".timeout"), mode);
    assert_eq!(hands(&p.clean, "runs.core.finished", ".Runner"), [("runs.core.Mode".to_string(), "mode".to_string())]);
    // `print(title, ..)` reads the parameter `show(sheet.title, ..)` fills
    let sheets =
        analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/plans/unbound-parameter"), &Options::default()).expect("reads");
    let uses = &sheets.node("sheets.show").unwrap().uses;
    assert!(uses.iter().any(|u| u.name == "title" && u.ty.is_empty()), "{uses:#?}");
}

/// The tables pair (`tests/pairs/tables/python`): what a mode means, looked up in one table, `plans: dict[Mode, Plan]`, against
/// two lists at the same places, `modes: list[Mode]` and `plans: list[Plan]`. A map holds its values by its keys (#167): the clean table is one
/// value, of Plans, which the body's own lookup reads as one; the shortcut takes the modes as a value of their own.
#[test]
fn a_table_is_one_value_and_the_lists_it_is_cut_into_two() {
    common::a_table_is_one_value("python", "tables.plans.timeout");
}

/// The choices pair (`tests/pairs/choices/python`): `run` looks its mode up in the module's `PLANS: dict[Mode, Plan]`, a value
/// from beyond of the Plans it holds, as the TypeScript pair's `run` reads its `Record<Mode, Plan>` (#171); the shortcut tests
/// a string in each function and keeps no table.
#[test]
fn a_module_s_table_is_a_value_the_body_reads() {
    common::a_module_s_table_is_a_value_the_body_reads("python", "runs.core.run");
}

/// The choices pair: `report(job, settings.level, ..)` is handed the settings through their `level`, and the graph says
/// so beside the value it is handed (#172), as the Rust and TypeScript readers do.
#[test]
fn a_call_names_the_field_a_projection_reads() {
    common::a_call_names_the_field_a_projection_reads("python", "runs.core");
}

/// The choices pair: a condition names the field it tests (#184), as the Rust and TypeScript readers read it.
#[test]
fn a_condition_names_the_field_it_tests() {
    common::a_condition_names_the_field_it_tests("python", "runs.core");
}

/// The projections pair: a projection bound to a name first is still the projection (#174), as the Rust and TypeScript
/// readers read it.
#[test]
fn a_projection_bound_to_a_name_is_still_the_projection() {
    common::a_projection_bound_to_a_name_is_still_the_projection("python", "charts.chart");
}

/// The recursion pair: a call into a function the route is already inside takes what its call hands it, and no value a body
/// makes comes from beyond the route (#237); the shortcut's `FLOOR` alone does, as in Rust and TypeScript.
#[test]
fn a_recursive_call_takes_what_its_call_hands_it() {
    common::a_recursive_call_takes_what_its_call_hands_it("python");
}

/// The literals pair: `[]` and `list()` make the same list, the body's (#237), as `vec![]` and `Vec::new()` do in Rust.
#[test]
fn a_list_made_by_a_literal_is_made_by_the_body() {
    common::a_list_made_by_a_literal_is_made_by_the_body("python", "by_literal", "by_call", "by_constant");
}

/// The workbench pair, `workbench/python`: what the program makes, kept in a folder of the project's
/// own. The clean side gives the folder a class whose method names each file in it
/// (`Workbench.of(project).file("report.txt")`); the shortcut hands the folder around as a bare path (`made_dir(project) / "report.txt"`).
/// Isomorphic is the same: a path a class's method builds from the field its constructor sets is the
/// path written out with bare paths, so the two sides name the same files, at the same places, fixed
/// by the same function. The method is followed on the value another call gives back and on a
/// binding of it, and `cls(..)` in the classmethod builds the class.
#[test]
fn places_a_path_a_class_builds_is_the_path_written_out() {
    let read = |side: &str| {
        analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/workbench/python").join(side), &Options::default()).unwrap()
    };
    let files = |g: &Graph| -> Vec<(String, String, Vec<String>)> {
        let mut out: Vec<(String, String, Vec<String>)> = ["bench.keep.save_report", "bench.keep.save_log"]
            .iter()
            .flat_map(|f| touches(g, f, "disk_write").into_iter().map(move |t| (f.to_string(), t.artifact.clone(), t.fixed_by.clone())))
            .collect();
        out.sort();
        out
    };
    let (c, s) = (files(&read("clean")), files(&read("shortcut")));
    assert_eq!(c, s);
    assert!(c.iter().any(|(f, a, by)| f == "bench.keep.save_report" && a == ".made/report.txt" && by == &["bench.cli.main"]), "{c:?}");
}
