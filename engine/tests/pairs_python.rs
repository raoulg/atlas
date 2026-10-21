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
/// `history` its route calls on the store is a call on a value without a type.
#[test]
fn gives_a_function_whose_value_is_used_says_what_it_gives() {
    let (c, s) = (clean(), shortcut());
    assert!(found(c, "gives").is_empty());
    assert!(edge(c, "harvest.api.get_store", "harvest.store.ReadingStore", EdgeKind::Returns));
    assert!(edge(c, "harvest.api.readings", "harvest.store.ReadingStore.history", EdgeKind::Calls));
    assert!(route(c, "harvest.api.readings").contains("reads readings"), "{}", route(c, "harvest.api.readings"));
    assert_eq!(found(s, "gives"), ["harvest.api.get_store"]);
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
/// `os.environ.get("WEATHER_API_URL", ...)` at import, so where it asks is decided at run time,
/// in library code: what the environment says, a place the code does not name, or the default the
/// library holds, which `places` asks about.
#[test]
fn a_place_carries_a_what_the_service_url() {
    let s = shortcut();
    let net = touches(s, "harvest.client.WeatherClient.latest", "net");
    assert!(!net.is_empty(), "the request is in the graph");
    assert!(net.iter().any(|t| t.artifact.is_empty() && !t.fixed), "what the environment says is no address the code says: {net:?}");
    assert!(
        found(s, "places").contains(&"harvest.client.WeatherClient.latest".to_string()),
        "the default is the library's own choice: {:?}",
        found(s, "places")
    );
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
/// the root stays a hole the term names (`{d}`), not a guess; no directory decides the place.
#[test]
fn a_place_carries_a_what_a_pattern_below_a_directory_the_entry_hands_down() {
    let clean = touches(clean(), "harvest.stations.archived_stations", "disk_read");
    assert!(clean.iter().any(|t| t.artifact == "data/archive/*/station-*"), "{clean:?}");
    let shortcut = touches(shortcut(), "harvest.stations.archived_stations", "disk_read");
    assert!(!shortcut.is_empty(), "the listing is in the graph");
    assert!(shortcut.iter().all(|t| t.artifact == "*/station-*" && !t.fixed), "{shortcut:?}");
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
