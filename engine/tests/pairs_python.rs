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
/// client, and the command line's `show` chooses again between the two.
#[test]
fn sums_a_sum_is_unwrapped_where_it_arises() {
    let (c, s) = (clean(), shortcut());
    assert!(found(c, "sums").is_empty());
    assert!(c.edges.iter().all(|e| e.kind != EdgeKind::Variant), "the clean side has no sum");
    assert!(edge(c, "harvest.model.Harvest", "harvest.model.Reading", EdgeKind::Has));
    assert!(edge(c, "harvest.model.Harvest", "harvest.model.Outage", EdgeKind::Has));
    assert!(edge(c, "harvest.cli.show", "harvest.model.Harvest", EdgeKind::Takes));
    assert_eq!(found(s, "sums"), ["harvest.model.Fetched"]);
    assert!(edge(s, "harvest.model.Fetched", "harvest.model.Reading", EdgeKind::Variant));
    assert!(edge(s, "harvest.model.Fetched", "harvest.model.Outage", EdgeKind::Variant));
    assert!(edge(s, "harvest.client.WeatherClient.latest", "harvest.model.Fetched", EdgeKind::Returns));
    assert!(edge(s, "harvest.cli.show", "harvest.model.Fetched", EdgeKind::Takes));
}

/// An override keeps the promise of its base. Both sides have one family, `Exporter` with
/// `JsonExporter` and `CsvExporter`; on the shortcut, `CsvExporter.write` gives the number of rows
/// where `Exporter.write` promises `None`, which ty reports as an incompatible override.
#[test]
fn overrides_an_override_keeps_the_promise_of_its_base() {
    let (c, s) = (clean(), shortcut());
    assert!(found(c, "overrides").is_empty());
    for o in ["harvest.export.JsonExporter.write", "harvest.export.CsvExporter.write"] {
        assert_eq!(c.node(o).unwrap().keeps, Some(true), "{o}");
        assert!(edge(c, &o.replace(".write", ""), "harvest.export.Exporter", EdgeKind::Inherits));
    }
    assert_eq!(found(s, "overrides"), ["harvest.export.Exporter.write"]);
    assert_eq!(s.node("harvest.export.CsvExporter.write").unwrap().keeps, Some(false));
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
    assert!(s.edges.iter().any(|e| e.kind == EdgeKind::Mutates && e.src == "harvest.model.convert" && e.dst == "harvest.model.Reading" && e.via.as_deref() == Some("reading")));
    assert!(edge(s, "harvest.harvester.Harvester.collect", "harvest.model.convert", EdgeKind::Calls));
}

/// A function whose value is used says what it gives back. In Python that is the return
/// annotation: `get_store() -> ReadingStore`. The shortcut's `get_store()` says nothing, so the
/// `history` its route calls on the store is a call on a value without a type.
#[test]
fn gives_a_function_whose_value_is_used_says_what_it_gives() {
    let (c, s) = (clean(), shortcut());
    assert!(found(c, "gives").is_empty());
    assert!(edge(c, "harvest.api.get_store", "harvest.store.ReadingStore", EdgeKind::Returns));
    assert!(edge(c, "harvest.api.readings", "harvest.store.ReadingStore.history", EdgeKind::Calls));
    assert!(route(c, "harvest.api.readings").contains("reads readings"), "{}", route(c, "harvest.api.readings"));
    assert_eq!(found(s, "gives"), ["harvest.api.get_store"]);
    assert!(!edge(s, "harvest.api.readings", "harvest.store.ReadingStore.history", EdgeKind::Calls), "the call on the store is not followed");
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
/// say what each unit means (`Unit.from_celsius`, `Unit.symbol`, each one `match`); the shortcut
/// passes the unit as a string and `convert` tests `unit == "fahrenheit"` once to convert and
/// again to label.
#[test]
fn choices_a_choice_is_made_once() {
    let (c, s) = (clean(), shortcut());
    assert!(found(c, "choices").is_empty());
    for f in ["harvest.model.in_unit", "harvest.model.Unit.from_celsius", "harvest.model.Unit.symbol"] {
        assert!(c.node(f).unwrap().compares.is_empty(), "{f} compares no string");
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
    let global = |f: &str, dir: &str| touches(s, f, "global").iter().any(|t| t.artifact == "harvest.harvester.LATEST" && t.dir_name() == dir);
    assert!(global("harvest.harvester.remember", "write") && global("harvest.api.latest", "read"));
    assert!(found(s, "nothing").is_empty(), "a call that keeps its value in a global leaves it behind");
    assert!(touches(c, "harvest.harvester.Harvester.collect", "global").is_empty(), "the clean side keeps nothing in a global");
    // the shortcut's `serve` hands the API its database through the environment: a library's
    // value it changes, `os.environ`, not the module `os`, which is code
    let environ = |f: &str, dir: &str| touches(s, f, "global").iter().any(|t| t.artifact == "os.environ" && t.what == "os.environ" && t.dir_name() == dir);
    assert!(environ("harvest.cli.serve", "write") && environ("harvest.api.get_store", "read"));
    assert!(s.nodes.iter().flat_map(|n| n.touches.iter()).all(|t| t.kind != World::Global || t.artifact != "os"), "a module is no place");
    assert!(c.nodes.iter().flat_map(|n| n.touches.iter()).all(|t| t.kind != World::Global), "the clean side names its database where it runs");
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
    assert!(c.edges.iter().all(|e| !(e.src.starts_with("harvest.store.") && e.dst.starts_with("harvest.harvester."))), "the store calls nothing of the harvester");
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
    assert!(touches(s, "harvest.store.ReadingStore.history", "db_sqlite").iter().any(|t| t.artifact == "readings" && t.dir_name() == "read"));
}

/// Every place carries a what: a request names the service it goes to. In Python the clean
/// client takes `base_url` and the entry hands it `API_URL`; the shortcut's client module reads
/// `os.environ.get("WEATHER_API_URL", ...)` at import, so where it asks is decided at run time,
/// in library code: what the environment says, a place the code does not name, or the default the
/// library holds, which `places` asks about.
#[test]
fn a_place_carries_a_what_the_service_url() {
    let s = shortcut();
    let net = touches(s, "harvest.client.WeatherClient.latest", "net");
    assert!(!net.is_empty(), "the request is in the graph");
    assert!(net.iter().any(|t| t.artifact.is_empty() && !t.fixed), "what the environment says is no address the code says: {net:?}");
    assert!(found(s, "places").contains(&"harvest.client.WeatherClient.latest".to_string()), "the default is the library's own choice: {:?}", found(s, "places"));
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
    assert!(s.edges.iter().all(|e| !(e.src == "harvest.harvester.Harvester.run" && e.kind == EdgeKind::Calls)), "run calls nothing Atlas follows");
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
    for (part, via) in [("harvest.client.WeatherClient", "client"), ("harvest.store.ReadingStore", "store"), ("harvest.export.Exporter", "exporter")] {
        assert!(c.edges.iter().any(|e| e.kind == EdgeKind::Has && e.src == "harvest.harvester.Harvester" && e.dst == part && e.via.as_deref() == Some(via)), "{part}");
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
/// target Atlas can name.
#[test]
fn zoom_is_a_quotient_by_composition_a_call_on_any_stays_a_step() {
    let collect = shortcut().node("harvest.harvester.Harvester.collect").unwrap();
    assert_eq!(collect.sites.len(), 3, "self.client.latest, convert, self.store.save: {:#?}", collect.sites);
}
