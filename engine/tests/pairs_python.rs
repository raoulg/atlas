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

use std::path::PathBuf;
use std::sync::OnceLock;

use atlas_engine::principles::{check, Finding};
use atlas_engine::{analyze, EdgeKind, EntryKind, Graph, Options, Touch, World};

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

/// Whether the calls Atlas does not follow include one that starts from `name`.
fn not_followed(g: &Graph, name: &str) -> bool {
    g.stats.unresolved_calls_top.iter().any(|(n, _)| n == name)
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
    let broken = check(s).into_iter().find(|f| f.principle == "overrides").unwrap();
    assert_eq!(broken.what, "Exporter.write promises (rows: list[dict], path: Path) -> None; 1 override breaks it");
    assert_eq!(broken.evidence, ["CsvExporter.write(rows: list[dict], path: Path) -> int (src/harvest/export.py:35)"]);
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
    assert!(route(c, "harvest.api.readings").contains("reads readings"), "{}", route(c, "harvest.api.readings"));
    assert_eq!(found(s, "gives"), ["harvest.api.get_store", "harvest.harvester.Harvester.run"]);
    assert!(
        !edge(s, "harvest.api.readings", "harvest.store.ReadingStore.history", EdgeKind::Calls),
        "the call on the store is not followed"
    );
    assert!(not_followed(s, "get_store.history"));
    assert!(!route(s, "harvest.api.readings").contains("reads readings"), "{}", route(s, "harvest.api.readings"));
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

/// What `lens` says of the store: what it puts and does not get back; empty when it asks nothing.
fn lens_says(g: &Graph) -> String {
    check(g).into_iter().find(|f| f.principle == "lens" && f.node == "harvest.store.ReadingStore").map(|f| f.what).unwrap_or_default()
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
    assert_eq!(lens_says(c), "");
    assert!(edge(s, "harvest.store.ReadingStore.save_outage", "harvest.model.Outage", EdgeKind::Takes));
    assert_eq!(s.node("harvest.store.ReadingStore.outages").unwrap().returns.as_ref().unwrap().written, "list[dict]");
    assert!(lens_says(s).contains("Outage"), "{}", lens_says(s));
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
    assert_eq!(lens_says(c), "");
    assert!(edge(s, "harvest.store.ReadingStore.summary", "harvest.model.Day", EdgeKind::Takes));
    assert!(touches(s, "harvest.store.ReadingStore.summary", "db_sqlite")
        .iter()
        .any(|t| t.artifact == "summaries" && t.dir_name() == "write"));
    assert!(lens_says(s).contains("Day"), "{}", lens_says(s));
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
    // the second line of a route is its start, with that function's own effects
    let own = |g: &Graph| route(g, "harvest.store.ReadingStore.save").lines().nth(1).unwrap_or_default().to_string();
    assert!(own(clean()).contains("writes readings"), "{}", own(clean()));
    assert!(own(shortcut()).contains("writes readings"), "{}", own(shortcut()));
}

/// A route starts at an entry and ends at a visible place. The clean API serves `POST /collect`,
/// which calls `Harvester.collect`; the shortcut serves `POST /tasks/{kind}`, a catch-all that
/// runs `Harvester.run(kind, **kw)`, which calls `getattr(self, kind)(**kw)`.
#[test]
fn a_route_ends_at_a_visible_place() {
    let (c, s) = (clean(), shortcut());
    let collect = route(c, "harvest.api.collect");
    assert!(collect.contains("GETs") && collect.contains("writes readings"), "{collect}");
    assert!(edge(c, "harvest.api.collect", "harvest.harvester.Harvester.collect", EdgeKind::Calls));
    let task = route(s, "harvest.api.run_task");
    assert!(!task.contains("GETs") && !task.contains("writes readings"), "{task}");
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
    assert!(s.stats.unresolved_calls_top.iter().any(|(n, _)| n.contains("getattr")), "{:?}", s.stats.unresolved_calls_top);
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
    let collect = route(c, "harvest.cli.collect");
    assert!(collect.contains("GETs") && collect.contains("writes readings"), "{collect}");
    // the shortcut's harvester holds nothing Atlas can name, and its calls on its parts are not followed
    assert!(s.edges.iter().all(|e| !(e.kind == EdgeKind::Has && e.src == "harvest.harvester.Harvester")));
    assert!(!edge(s, "harvest.harvester.Harvester.collect", "harvest.client.WeatherClient.latest", EdgeKind::Calls));
    assert!(not_followed(s, "self.client") && not_followed(s, "self.store"));
    let collect = route(s, "harvest.cli.collect");
    assert!(!collect.contains("GETs") && !collect.contains("writes readings"), "{collect}");
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
/// the root stays a hole in the term (`{d}`, any directory: `*/*/station-*`), not a guess; no
/// directory decides the place.
#[test]
fn a_place_carries_a_what_a_pattern_below_a_directory_the_entry_hands_down() {
    let clean = touches(clean(), "harvest.stations.archived_stations", "disk_read");
    assert!(clean.iter().any(|t| t.artifact == "data/archive/*/station-*"), "{clean:?}");
    let shortcut = touches(shortcut(), "harvest.stations.archived_stations", "disk_read");
    assert!(!shortcut.is_empty(), "the listing is in the graph");
    assert!(shortcut.iter().all(|t| t.artifact == "*/*/station-*" && !t.fixed), "{shortcut:?}");
    assert!(shortcut.iter().all(|t| t.what == "{d}/*/station-*"), "{shortcut:?}");
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
    let envisioned = calls(&atlas_engine::changes::envisioned(&copy, &vision, &Options::default()).expect("reads the vision"));
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
    assert!(notes.iter().any(|n| n.contains("goes in a circle") && n.contains("archive.sink")), "{notes:?}");
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
    assert!(refused.to_string().contains("changes `archive.sink.Sink.write`") && refused.to_string().contains("supersedes"), "{refused}");
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
    let asked = check(&s).into_iter().find(|f| f.principle == "sums").unwrap();
    assert_eq!(asked.what, "Fit | Failed is handed on unopened: points and line take it");
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

    let asked: Vec<Finding> = check(&s).into_iter().filter(|f| f.principle == "choices").collect();
    let at = |id: &str| asked.iter().find(|f| f.node == id).unwrap_or_else(|| panic!("{id} is asked: {asked:#?}"));
    assert_eq!(
        asked.iter().map(|f| f.node.as_str()).collect::<Vec<_>>(),
        ["runs.core.Runner", "runs.core.retries", "runs.core.verbosity", "runs.core.prefix", "runs.core.run"]
    );
    let runner = at("runs.core.Runner");
    assert_eq!(runner.what, "Runner.batch and Runner.workers test `self.mode` in 2 places; \"parallel\" (2×) come back");
    assert_eq!(
        runner.evidence,
        ["line 34 in Runner.batch: self.mode against \"parallel\"", "line 37 in Runner.workers: self.mode against \"parallel\""]
    );
    assert_eq!(at("runs.core.retries").what, "retries and timeout test `mode` in 2 places; \"parallel\" (2×) come back");
    assert_eq!(
        at("runs.core.verbosity").evidence,
        ["line 50: m (settings.mode) against \"parallel\"", "line 51: m (settings.mode) against \"parallel\""],
        "the local `m` is the value it is bound to"
    );
    assert_eq!(at("runs.core.prefix").what, "prefix and suffix test `level` in 2 places; Level.LOUD (2×) come back");
    assert!(at("runs.core.run").what.contains("(load | infer)"), "{}", at("runs.core.run").what);
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

/// What `lens` says of class `id` of the club: empty when it asks nothing.
fn lens_of(g: &Graph, id: &str) -> String {
    check(g).into_iter().find(|f| f.principle == "lens" && f.node == format!("club.{id}")).map(|f| f.what).unwrap_or_default()
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
    assert_eq!(lens_of(s, "desk.Desk"), "Desk puts Visit into SQLite(visits) and gets no Visit back");
    assert_eq!(lens_of(s, "desk.Register"), "Register puts Member into SQLite(members) and gets no Member back");
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
    assert_eq!(lens_of(c, "files.Tape"), "");
    assert_eq!(lens_of(s, "files.Tape"), "Tape puts Visit into Disk(Tape.path) and gets no Visit back");
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
    assert_eq!(lens_of(c, "files.Log"), "");
    assert_eq!(lens_of(s, "files.Log"), "Log puts Visit into Disk(Log.path) and gets no Visit back");
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
    assert_eq!(lens_of(c, "desk.Visits"), "");
    assert_eq!(lens_of(s, "desk.Visits"), "Visits puts Member into SQLite(visits) and gets no Member back");
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
    assert_eq!(lens_of(c, "files.Roster"), "");
    assert_eq!(lens_of(c, "files.Board"), "");
    assert_eq!(lens_of(s, "files.Roster"), "Roster puts Member into Disk(Roster.path) and gets no Member back");
    assert_eq!(lens_of(s, "files.Board"), "Board puts Notice into Disk(Board.path) and gets no Notice back");
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
/// themselves and a count, each family once as its author keeps its bases' promises and once as a
/// hurried author writes it.
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
    let source = check(s).into_iter().find(|f| f.node == "reading.feeds.Source.read").unwrap();
    assert_eq!(source.what, "Source.read promises () -> bytes; 1 override breaks it");
    assert_eq!(source.evidence, ["Feed.read() -> str (src/reading/feeds.py:20)"]);
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

/// One side of the change pair (`tests/pairs/changes`): a scoreboard of cells and scores, whose
/// functions change what they are handed, in Python, Rust and TypeScript alike.
fn board(side: &str) -> Graph {
    analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/changes/python").join(side), &Options::default()).unwrap()
}

/// A change to an input shows in the signature, whatever the input's type. The clean board changes
/// a value through a method of its own (`Board.reset` clears its cells, `Board.mark_all` marks each
/// cell it holds) or hands back a new one: `with_tag` and `counted` build it, `renamed` rebinds
/// `board` to a copy before it sets the name, `extended` and `queued` rebind their list to a new
/// one before they change it. Nothing is asked. The shortcut changes what it is handed and says
/// nothing: a `list[str]` it appends to, a `dict` it sets an item of, a list it extends with `+=`,
/// a heap `heapq` keeps in a list, the cells `clear` empties and the scores `update` fills (methods
/// ty finds on typing's `MutableSequence` and `MutableMapping`), each cell through the loop
/// variable that goes over the board's cells, the board through a second name for it, and both
/// boards a tuple assignment swaps. `Board.take_from` and `Board.take_victim` have one body: each
/// empties the board it is handed as well as its own, two changes, whatever the parameter is
/// called. Each is asked, and none of them is a call that leaves nothing behind.
#[test]
fn changes_a_change_to_an_input_of_any_type_shows_in_the_signature() {
    let (c, s) = (board("clean"), board("shortcut"));
    let fs = check(&c);
    assert!(fs.is_empty(), "{:#?}", fs.iter().map(|f| format!("{} {} {}", f.principle, f.node, f.what)).collect::<Vec<_>>());
    assert!(c.edges.iter().filter(|e| e.kind == EdgeKind::Mutates).all(|e| e.via.as_deref() == Some("self")), "only methods change");
    let mut asked = found(&s, "changes");
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
    let add_tag = check(&s).into_iter().find(|f| f.node == "board.core.add_tag").expect("asked");
    assert!(add_tag.what.contains("the list[str] it is handed as `tags`"), "{}", add_tag.what);
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
    let layers: Vec<Finding> = check(&s).into_iter().filter(|f| f.principle == "layers").collect();
    let said: Vec<(&str, &str)> = layers.iter().map(|f| (f.node.as_str(), f.what.as_str())).collect();
    assert_eq!(
        said,
        [("kitchen", "kitchen and shop depend on one another"), ("shop.orders", "orders itself and lines depend on one another")]
    );
    assert!(layers[1].evidence.iter().any(|e| e.starts_with("orders itself: the code of `shop.orders`")), "{:?}", layers[1].evidence);
    assert!(edge(&s, "shop.orders.lines.price", "shop.orders.tax", EdgeKind::Calls), "the shortcut's call back is in the graph");
    assert!(edge(&s, "shop.menu.dish", "kitchen.cook.boil", EdgeKind::Calls));
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
        let cycles: Vec<Finding> = check(&g).into_iter().filter(|f| f.principle == "layers").collect();
        assert!(cycles.iter().all(|f| f.evidence.iter().all(|e| !e.contains("boiled") && !e.contains("cooked"))), "{side}: {cycles:#?}");
    }
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
    let asked: Vec<Finding> = check(&s).into_iter().filter(|f| f.principle == "prophecy").collect();
    assert_eq!(asked.len(), 1, "{asked:#?}");
    assert_eq!(asked[0].what, "save_posted supersedes `save`, which names 2 nodes: `archive.ledger.save` and `archive.shelf.save`");
    assert!(!asked[0].what.contains("does not hold"));
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
    let asked: Vec<Finding> = check(&s).into_iter().filter(|f| f.principle == "prophecy").collect();
    assert_eq!(asked.iter().map(|f| f.node.as_str()).collect::<Vec<_>>(), ["buyer.client.item"]);
    assert_eq!(asked[0].evidence, ["Requests `item_v2`, which is not built"]);
}

// ---------- doors: every declaration a door is made of ----------

/// One side of the doors pair (`tests/pairs/doors`): lengths in metres, read from text, scaled,
/// added up and logged, the package's doors brought to its root with `from .. import *`.
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
    assert_eq!(scale.overloads.iter().map(|o| o.line).collect::<Vec<_>>(), [26, 28]);
    assert!(scale.params.iter().all(|p| p.ty.is_empty()) && scale.span.map(|s| s.0) == Some(29), "the implementation, as written");
    let meters = c.node("units.core.Length.meters").unwrap();
    assert_eq!(meters.decorators, ["property"]);
    assert_eq!(meters.returns.as_ref().map(|r| r.written.as_str()), Some("float"));
    assert_eq!(c.node("units.core.Length").unwrap().methods, ["__init__", "meters", "__add__", "__eq__"]);
    // the class's route runs its initializer once, then what a user may run on it, the dunders among it
    let steps: Vec<String> = atlas_engine::flow::bodies(&c)["units.core.Length"].steps().iter().map(|s| s.to.clone()).collect();
    assert_eq!(steps, ["__init__", "meters", "__add__", "__eq__"].map(|m| format!("units.core.Length.{m}")));
    // the shortcut: each door left open is asked, at the declaration that leaves it open
    let asked: Vec<(String, u32)> = check(&s).into_iter().filter(|f| f.principle == "doors").map(|f| (f.node, f.line)).collect();
    let want = [
        ("units.convert.parse", 6),
        ("units.core.Length.meters", 11),
        ("units.core.Length.__add__", 18),
        ("units.core.scale", 28),
        ("units.core.log", 33),
    ];
    assert_eq!(asked, want.map(|(n, l)| (n.to_string(), l)));
    // `Any` is drawn as written, and says nothing
    let parse = s.node("units.convert.parse").unwrap();
    assert_eq!(parse.params.iter().map(|p| (p.ty.as_str(), p.unchecked)).collect::<Vec<_>>(), [("Any", true)]);
    assert!(parse.returns.as_ref().is_some_and(|r| r.written == "Any" && r.unchecked));
}

/// One side of the nothing pair (`tests/pairs/nothing`): a job runner that runs a task, says when a
/// job is done, reports, counts its runs, settles its log and stops the run, in Python, Rust and
/// TypeScript alike.
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
    let asked = |g: &Graph| -> Vec<(String, String)> {
        check(g).into_iter().filter(|f| f.principle == "nothing").map(|f| (f.node, f.what)).collect()
    };
    assert!(asked(&c).is_empty(), "{:#?}", asked(&c));
    for g in [&c, &s] {
        assert!(g.node("jobs.runner.stop").unwrap().raises, "ending the run is what `stop` leaves behind");
    }
    let want = [
        ("run_task", "run_task gives nothing back, and Atlas cannot see what `Task.run` does"),
        ("Job.finish", "Job.finish gives nothing back, and Atlas cannot see what `self.on_done(..)` does"),
        ("report", "report gives nothing back, and Atlas cannot see what `logger.info(..)` does"),
        ("count", "count gives nothing back, and Atlas cannot see what `Counter.runs += 1` does"),
        ("settle", "settle gives nothing back, and Atlas sees no change and no place it reaches"),
    ]
    .map(|(f, what)| (format!("jobs.runner.{f}"), what.to_string()));
    assert_eq!(asked(&s), want);
    // what is only declared has no body to ask about, and what implements it is still drawn
    assert!(s.node("jobs.runner.Task.run").unwrap().declared && c.node("jobs.runner.Task.run").unwrap().declared);
    assert_eq!(touches(&s, "jobs.runner.Echo.run", "screen").len(), 1);
    assert_eq!(s.node("jobs.runner.settle").unwrap().entry.as_ref().map(|e| e.kind), Some(EntryKind::Export), "settle is a door");
}

/// One side of the gives pair (`tests/pairs/gives`): readings kept in a file, how many there are,
/// and the readings ranked, in Python and TypeScript alike.
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
    assert_eq!(found(&s, "gives"), ["readings.store.get_store", "readings.store.score"]);
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

/// The places pair, `places/python`: rows read and kept, and pages made from a template the
/// package ships, the same on both sides but for where the rows come from.
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
    assert_eq!(found(&s, "places"), ["stock.rows.default_rows"]);
    let read = |g: &Graph, by: &str| {
        touches(g, "stock.rows.read_rows", "disk_read").iter().any(|t| t.artifact == "data/rows.csv" && !t.fixed && t.fixed_by == [by])
    };
    assert!(read(&c, "stock.cli.main") && read(&s, "stock.rows.default_rows"));
    let asked = check(&s).into_iter().find(|f| f.principle == "places").expect("default_rows is asked");
    assert_eq!(asked.evidence, ["read_text(data/rows.csv) in read_rows"]);
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
        let r = route(g, "news.read.headlines");
        assert!(r.contains("Store.get(key)") && !r.contains("response.read()"), "{side}: {r}");
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
    let r = route(&news().clean, "news.read.headlines");
    assert!(!r.contains("one of") && r.contains("Cached.read()"), "{r}");
    assert!(r.contains("reads {cache}/{url}"), "{r}");
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
        let r = route(g, "news.read.headlines");
        assert!(!r.contains("one of"), "{side}: {r}");
        assert_eq!(r.contains("Store.get(key)"), side == "clean", "{side}: {r}");
    }
    let bodies = atlas_engine::flow::bodies(&cooperative().clean);
    let cached: Vec<&str> = bodies[&key("Cached", "Feed")].steps().iter().map(|s| s.to.as_str()).collect();
    assert_eq!(cached, ["news.store.Store.has", "news.store.Store.get", "news.sources.Source.read", "news.store.Store.put"]);
    let unknown = &bodies["news.sources.Trimmed.read"].steps()[0];
    assert_eq!(
        (unknown.to.as_str(), unknown.instead.as_slice()),
        ("news.sources.Source.read", ["news.sources.Cached.read".to_string()].as_slice())
    );
    let r = route(&cooperative().clean, "news.read.lines");
    assert!(r.contains("Source.read() → bytes   ·   calls self.url   ·   one of 1") && r.contains("1.1a  Cached.read()"), "{r}");
    let r = route(&cooperative().shortcut, "news.read.lines");
    assert!(!r.contains("one of") && !r.contains("Cached.read"), "{r}");
}

/// The file a feed reads is named in the terms of the route that builds it, on either side: the
/// cache directory `headlines` is handed, then the name of the feed in it. A class's call hands its
/// arguments to the initializer it runs, its own or the first along its bases: the shortcut's
/// `Feed(url, Store(cache))` hands them to `Cached.__init__`, which `Feed` and `Buffered` leave to
/// it.
#[test]
fn diamond_the_file_a_feed_reads_is_named_where_it_is_built() {
    for (side, g) in [("clean", &news().clean), ("shortcut", &news().shortcut)] {
        let r = route(g, "news.read.headlines");
        assert!(r.contains("reads {cache}/"), "{side}: {r}");
    }
}
