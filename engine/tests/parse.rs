use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::OnceLock;

use atlas_engine::view::{by_module, hide_private, reach_from, Compose, View};
use atlas_engine::{analyze, EdgeKind, EntryKind, Graph, Kind, NodeKind, Options};

/// The Atlas workspace, read once for the tests that read it.
fn atlas() -> &'static Graph {
    static G: OnceLock<Graph> = OnceLock::new();
    G.get_or_init(|| {
        analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".."), &Options::default()).expect("the Atlas workspace parses")
    })
}

fn fixture() -> Graph {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixture");
    analyze(&repo, &Options::default()).expect("fixture parses")
}

/// How many calls of a node cross to each kind of place, by the names the graph's JSON gives them.
fn boundaries(n: &atlas_engine::Node) -> std::collections::BTreeMap<&'static str, u32> {
    let mut out = std::collections::BTreeMap::new();
    for t in &n.body().touches {
        *out.entry(t.kind_name()).or_insert(0) += t.n;
    }
    out
}

fn edges(g: &Graph, kind: EdgeKind) -> HashSet<(String, String)> {
    g.edges.iter().filter(|e| e.kind == kind).map(|e| (e.src.clone(), e.dst.clone())).collect()
}

#[test]
fn modules_classes_and_functions_are_found() {
    let g = fixture();
    assert_eq!(g.root, "fixture");
    let kinds = |k: NodeKind| g.nodes.iter().filter(|n| n.kind == k).count();
    // `services` holds the places a library reaches by its table alone
    assert_eq!(kinds(NodeKind::Module), 28, "the package's twenty-five, a script and two notebooks");
    assert!(matches!(&g.node("fixture.sinks").unwrap().kind, Kind::Module(m) if m.is_pkg));
    assert_eq!(g.node("fixture.schema.Graph").unwrap().doc, "Nodes and edges.");
    assert_eq!(g.node("fixture.sinks.duck.DuckSink").unwrap().class().unwrap().bases, vec!["Sink", "Mixin"]);
    assert_eq!(g.node("fixture.build.build_graph").unwrap().returns().unwrap().value, "Graph");
}

/// A signature is data each reader writes the same way: a parameter by name and type, the value
/// or class a method is called on as its receiver, and what it gives out of the effect its
/// language wraps it in. `None`, `()`, `Result<()>` and `void` give no value; `Self` gives one of
/// the class it runs on.
#[test]
fn a_signature_is_data_in_every_language() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let read = |dir: &str| analyze(&manifest.join(dir), &Options::default()).unwrap();
    let (py, rs, ts) = (fixture(), read("tests/fixture-rs"), read("tests/fixture-ts"));
    let gives = |g: &Graph, id: &str| g.node(id).unwrap_or_else(|| panic!("{id}")).returns().cloned().map(|r| (r.value, r.own));
    let params = |g: &Graph, id: &str| g.node(id).unwrap().params().iter().map(|p| (p.to_string(), p.receiver)).collect::<Vec<_>>();

    assert_eq!(params(&py, "fixture.files.Report.from_file"), [("cls".into(), true), ("path: Path".into(), false)]);
    assert_eq!(gives(&py, "fixture.sinks.base.Sink.write"), Some((String::new(), false)));
    assert_eq!(gives(&py, "fixture.schema.Graph.first"), None, "a signature that does not say");

    assert_eq!(params(&rs, "handed.world.Log.append"), [("self".into(), true), ("line: &str".into(), false)]);
    assert_eq!(gives(&rs, "handed.world.Log.append"), Some((String::new(), false)), "std::io::Result<()>");
    assert_eq!(gives(&rs, "handed.shapes.Shape.square"), Some((String::new(), true)), "Self");

    assert_eq!(gives(&ts, "fixture-ts.lib.klant.vraag"), Some(("Response".into(), false)), "Promise<Response>");
    assert_eq!(gives(&ts, "fixture-ts.server.opslag.bewaar"), Some((String::new(), false)), "void");
}

#[test]
fn relative_imports_and_reexports_resolve() {
    let g = fixture();
    let imports = edges(&g, EdgeKind::Imports);
    assert!(imports.contains(&("fixture.sinks.base".into(), "fixture.schema.Graph".into())), "from ..schema import Graph");
    assert!(imports.contains(&("fixture.sinks".into(), "fixture.sinks.duck.DuckSink".into())), "from .duck import DuckSink");
    // `from fixture import Graph` goes through the package __init__ re-export
    let inst = edges(&g, EdgeKind::Instantiates);
    assert!(inst.contains(&("fixture.build.build_graph".into(), "fixture.schema.Graph".into())));
    // `from fixture.sinks import DuckSink` re-exported through sinks/__init__
    assert!(inst.contains(&("fixture.cli.build".into(), "fixture.sinks.duck.DuckSink".into())));
    // `Pipeline(DuckSink(..))`: what a call is handed is built first
    let order: Vec<&str> =
        g.edges.iter().filter(|e| e.src == "fixture.cli.build" && e.kind == EdgeKind::Instantiates).map(|e| e.dst.as_str()).collect();
    assert_eq!(order, ["fixture.sinks.duck.DuckSink", "fixture.build.Pipeline"], "calls run in evaluation order");
    // `from fixture import DuckSink`: through the root, then through sinks/__init__
    let takes = edges(&g, EdgeKind::Takes);
    assert!(takes.contains(&("fixture.build.Pipeline.__init__".into(), "fixture.sinks.duck.DuckSink".into())), "a name re-exported twice");
}

#[test]
fn inheritance_including_multiple_and_external() {
    let g = fixture();
    let inh = edges(&g, EdgeKind::Inherits);
    assert!(inh.contains(&("fixture.sinks.duck.DuckSink".into(), "fixture.sinks.base.Sink".into())));
    assert!(inh.contains(&("fixture.sinks.duck.DuckSink".into(), "fixture.sinks.base.Mixin".into())));
    assert!(inh.contains(&("fixture.sinks.base.Sink".into(), "ext:abc.ABC".into())));
    assert!(inh.contains(&("fixture.schema.Graph".into(), "ext:pydantic.BaseModel".into())));
    assert_eq!(g.node("ext:pydantic.BaseModel").unwrap().kind, NodeKind::External);
}

#[test]
fn self_attr_inference_and_base_method_lookup() {
    let g = fixture();
    let calls = edges(&g, EdgeKind::Calls);
    // self.sink = sink where sink: DuckSink, so self.sink.write resolves to DuckSink.write
    assert!(calls.contains(&("fixture.build.Pipeline.run".into(), "fixture.sinks.duck.DuckSink.write".into())));
    // self.close() is defined on the Mixin base
    assert!(calls.contains(&("fixture.sinks.duck.DuckSink.write".into(), "fixture.sinks.base.Mixin.close".into())));
    // g = Graph(); g.add(...) resolves through the local's constructor
    assert!(calls.contains(&("fixture.build.build_graph".into(), "fixture.schema.Graph.add".into())));
    let has = edges(&g, EdgeKind::Has);
    assert!(has.contains(&("fixture.build.Pipeline".into(), "fixture.sinks.duck.DuckSink".into())));
    assert!(has.contains(&("fixture.schema.Graph".into(), "fixture.schema.Node".into())), "annotated field");
}

/// The type of an expression is composed from its parts: a name has the type of its binding,
/// `a.b` the type of `b` in the type of `a`, `f()` what `f` makes or gives back, `a or b` the
/// type of either. A receiver typed this way is what a call reaches.
#[test]
fn types_compose_from_their_parts() {
    let g = fixture();
    let calls = edges(&g, EdgeKind::Calls);
    let call = |src: &str, dst: &str| calls.contains(&(format!("fixture.{src}"), format!("fixture.{dst}")));
    assert!(call("store.Store.write", "store.Connection.write"), "`self.conn = connection or Connection(uri)`, then `conn = self.conn`");
    assert!(call("store.Reader.nodes", "store.Connection.read"), "`self.store.conn.read(..)`: a chain of attributes");
    assert!(call("store.Copier.copy", "store.Store.write"), "`self.store`, set by the base's `__init__`");
    assert!(
        call("store.keep", "store.Store.write") && call("store.keep", "store.Reader.nodes"),
        "`store, reader = stores()` with `-> tuple[Store, Reader]`"
    );
    assert!(
        call("store.latest", "store.reader") && call("store.latest", "store.Reader.nodes"),
        "`current = reader`: a function under another name"
    );
    assert!(call("store.firsts", "schema.Graph.first"), "`[g.first() for g in graphs]` with `graphs: list[Graph]`");
    assert!(call("store.merged", "schema.Graph.first"), "`for g in graphs`");
    assert!(call("build.reread", "store.Reader.nodes"), "`reader()` is defined in a module read after this one");
    assert!(edges(&g, EdgeKind::Has).contains(&("fixture.store.Store".into(), "fixture.store.Connection".into())), "the class `or` makes");
    let b = |id: &str| boundaries(g.node(id).unwrap());
    assert_eq!(b("fixture.store.pull").get("net"), Some(&1), "`SESSION = _session()` with `-> requests.Session`, at module level");
    assert_eq!(b("fixture.store.unpack").get("disk_write"), Some(&1), "`with zipfile.ZipFile(..) as zf: zf.extractall(..)`");
    assert!(b("fixture.store.Connection.driver").contains_key("db_graph"), "`GraphDatabase.driver(..)`");
    let inst = edges(&g, EdgeKind::Instantiates);
    assert!(
        !inst.contains(&("fixture.schema.Graph.add".into(), "fixture.schema.Node".into())),
        "`self.nodes.append(..)` on a `list[Node]` builds no Node"
    );
}

/// Which way a graph database's data goes is what the Python code says to its driver: a managed
/// transaction, a routing, an access mode. What runs inside goes the same way; the query's text
/// is not read.
#[test]
fn a_driver_says_which_way_the_data_goes() {
    let g = fixture();
    let dirs = |id: &str| {
        g.node(id)
            .unwrap()
            .body()
            .touches
            .iter()
            .filter(|t| t.kind_name() == "db_graph")
            .map(|t| (t.op.as_str(), t.dir_name()))
            .collect::<HashSet<_>>()
    };
    let set = |xs: &[(&'static str, &'static str)]| xs.iter().copied().collect::<HashSet<_>>();
    assert_eq!(
        dirs("fixture.store.Connection.read"),
        set(&[("session", "read"), ("execute_read", "read"), ("run", "read")]),
        "`session.execute_read(lambda tx: tx.run(..))`"
    );
    assert_eq!(dirs("fixture.store.Connection.write"), set(&[("session", "write"), ("execute_write", "write")]));
    assert_eq!(dirs("fixture.store._write"), set(&[("run", "write")]), "the work `execute_write` is handed");
    assert_eq!(dirs("fixture.store.Connection.count"), set(&[("execute_query", "read")]), "`routing_=RoutingControl.READ`");
    let peek = g.node("fixture.store.Connection.peek").unwrap().body().touches.iter().find(|t| t.op == "run").unwrap();
    assert_eq!((peek.dir_name(), peek.artifact.as_str()), ("", ""), "`session.run(\"MATCH (n:Graph) ..\")` says nothing in Python");
}

/// An object holds what its constructor was handed: `Cache(Path("warm"))` sets `self.root` in
/// `__init__`, and `keep` writes under it, at the key its caller hands.
#[test]
fn an_object_holds_what_its_constructor_was_handed() {
    let g = fixture();
    let keep: Vec<(&str, Vec<String>)> =
        g.node("fixture.store.Cache.keep").unwrap().body().touches.iter().map(|t| (t.artifact.as_str(), t.by.clone())).collect();
    assert_eq!(keep, [("warm/a.txt", vec!["fixture.store.warm".to_string()])]);
    // a dataclass is built from its fields: `Bin(root=Path("tidy"))`
    let drop: Vec<&str> = g.node("fixture.store.Bin.drop").unwrap().body().touches.iter().map(|t| t.artifact.as_str()).collect();
    assert_eq!(drop, ["tidy/x.txt"]);
    // a Rust struct literal sets its fields: `Self { root }` in `Store::new(root)`
    let rs = analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixture-rs"), &Options::default()).unwrap();
    let keep: Vec<&str> = rs.node("handed.world.Store.keep").unwrap().body().touches.iter().map(|t| t.artifact.as_str()).collect();
    assert_eq!(keep, ["stock/kept.txt"], "`Store::new(PathBuf::from(\"stock\"))`");
    // the place is `stock`'s: a route that does not build the value sees the call as it was read
    let stock = atlas_engine::story::route(&rs, "handed.world.stock", 3).unwrap();
    assert!(stock.contains("writes stock/kept.txt"), "{stock}");
    let own = atlas_engine::story::route(&rs, "handed.world.Store.keep", 3).unwrap();
    assert!(own.contains("writes") && !own.contains("stock/kept.txt"), "{own}");
}

/// A script's command line, run as written, holds the defaults its parser declares: `args.out_file`
/// is `--out-file`'s `default=`. The option is the script's input, as a parameter of a command is,
/// so the code does not alone decide the place it names.
#[test]
fn a_script_runs_with_its_declared_defaults() {
    let g = fixture();
    let main: Vec<(&str, bool)> =
        g.node("scripts.report.main").unwrap().body().touches.iter().map(|t| (t.artifact.as_str(), t.fixed)).collect();
    assert_eq!(main, [("out/report.txt", false)]);
}

/// What changes in place is what the call does, as ty says where it goes: `setattr(graph, ..)` and
/// `del graph.nodes[0]` change the Graph; a method of the program called `update` that gives a new
/// value changes nothing. `type Part = Graph | Node` is a sum.
#[test]
fn a_change_is_what_the_call_does() {
    let g = fixture();
    let m = edges(&g, EdgeKind::Mutates);
    assert!(m.contains(&("fixture.edits.rename".into(), "fixture.schema.Graph".into())), "`setattr`: {m:?}");
    assert!(m.contains(&("fixture.edits.forget".into(), "fixture.schema.Graph".into())), "`del graph.nodes[0]`");
    assert!(!m.iter().any(|(s, _)| s == "fixture.edits.deeper"), "the program's own `update` that changes nothing");
    let variants = edges(&g, EdgeKind::Variant);
    assert!(
        variants.contains(&("fixture.edits.Part".into(), "fixture.schema.Graph".into()))
            && variants.contains(&("fixture.edits.Part".into(), "fixture.schema.Node".into())),
        "{variants:?}"
    );
}

/// The terminal is a place both ways, in every language: what the human types is read from the
/// screen (`input`, `getpass`, `sys.stdin`, a curses window's `getch`, Rust's `io::stdin()`,
/// `process.stdin`), what a window or a stream shows is written to it, and a print handed a file
/// writes the file.
#[test]
fn the_terminal_is_a_place_both_ways() {
    let t = |g: &atlas_engine::model::Graph, id: &str| {
        g.node(id)
            .unwrap()
            .body()
            .touches
            .iter()
            .map(|t| (t.kind_name().to_string(), t.op.clone(), t.dir_name().to_string()))
            .collect::<Vec<_>>()
    };
    let read = |op: &str| ("screen".to_string(), op.to_string(), "read".to_string());
    let py = fixture();
    assert_eq!(t(&py, "fixture.terminal.ask_name"), [read("input")]);
    assert_eq!(t(&py, "fixture.terminal.ask_secret"), [read("getpass")]);
    assert_eq!(t(&py, "fixture.terminal.first_line"), [read("readline")], "`sys.stdin.readline()`");
    assert!(!t(&py, "fixture.terminal.keep").iter().any(|x| x.0 == "screen"), "`print(.., file=f)` writes the file");
    assert_eq!(t(&py, "fixture.terminal.draw"), [("screen".into(), "addstr".into(), String::new()), read("getch")], "a curses window");
    let rs = analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixture-rs"), &Options::default()).unwrap();
    assert_eq!(t(&rs, "handed.world.answer"), [read("stdin")], "`io::stdin().read_line(..)`");
    let ts = analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixture-ts"), &Options::default()).unwrap();
    assert!(t(&ts, "fixture-ts.server.opslag.echo").contains(&read("on")), "`process.stdin.on(..)`");
}

/// SQL is read by its grammar: a statement writes the table it changes and reads the ones it
/// takes from, and a table named at run time is not named. A setting names no table, and like the
/// table's shape it is part of the statements beside it.
#[test]
fn sql_names_the_tables_it_reads_and_writes() {
    let g = fixture();
    let sides = g
        .node("fixture.store.Ledger.keep")
        .unwrap()
        .body()
        .touches
        .iter()
        .filter(|t| t.kind_name() == "db_sqlite")
        .map(|t| (t.op.as_str(), t.artifact.as_str(), t.dir_name()))
        .collect::<HashSet<_>>();
    let expected: HashSet<_> = [("execute", "log", "write"), ("execute", "nodes", "read"), ("execute", "", "write")].into_iter().collect();
    assert_eq!(sides, expected, "PRAGMA; CREATE TABLE IF NOT EXISTS log; INSERT INTO log SELECT .. FROM nodes; DELETE FROM {{table}}");
}

/// A statement is read where it is whole: from the constant a call is handed, and from what its
/// callers hand the parameter it is.
#[test]
fn a_statement_is_read_where_it_is_whole() {
    let g = fixture();
    let sides = |id: &str| {
        g.node(id)
            .unwrap()
            .body()
            .touches
            .iter()
            .filter(|t| t.kind_name() == "db_sqlite" && t.op != "connect")
            .map(|t| (t.op.as_str(), t.artifact.as_str(), t.dir_name()))
            .collect::<Vec<_>>()
    };
    assert_eq!(sides("fixture.store.seen"), [("schema", "seen", "write")], "`con.executescript(SCHEMA)` gives `seen` its shape");
    assert_eq!(sides("fixture.store.count"), [("execute", "seen", "read")], "`count(con, \"SELECT count(*) FROM seen\")`");
}

/// What a library's call reaches is a fact of its table: psycopg and asyncpg run statements on a
/// SQL server, read by their grammar, the connection opened for one part of it; redis reads and
/// writes the key it is handed, and building its client moves nothing; pandas reads a frame from a
/// database by a statement or a table's name and writes one to a table; `os.system` and asyncio's subprocesses run a program, nbclient
/// runs a notebook in a kernel, and transformers' `generate` runs a model.
#[test]
fn a_library_table_says_where_its_calls_go() {
    let g = fixture();
    let t = |id: &str| {
        g.node(&format!("fixture.services.{id}"))
            .unwrap()
            .body()
            .touches
            .iter()
            .map(|t| (t.kind_name(), t.op.as_str(), t.artifact.as_str(), t.dir_name()))
            .collect::<Vec<_>>()
    };
    assert_eq!(t("latest"), [("db_sql", "execute", "readings", "read")], "psycopg");
    assert_eq!(t("total"), [("db_sql", "fetchval", "readings", "read")], "asyncpg's query that hands back a value");
    assert_eq!(t("record"), [("db_sql", "execute", "readings", "write")], "`INSERT INTO readings .. ($1)`");
    assert!(t("cache").is_empty(), "`redis.Redis(..)` builds a client");
    assert_eq!(t("remember"), [("db_kv", "set", "session", "write")]);
    assert_eq!(t("recall"), [("db_kv", "get", "session", "read")]);
    assert_eq!(t("frame"), [("db_sql", "read_sql", "readings", "read")], "`pd.read_sql(\"SELECT * FROM readings\", con)`");
    // a table handed by its name is no statement: which way it goes is known, the table is what
    // the code names it
    assert_eq!(t("table"), [("db_sql", "read_sql_table", "", "read")]);
    assert_eq!(t("keep"), [("db_sql", "to_sql", "", "write")]);
    let what = |id: &str| {
        g.node(&format!("fixture.services.{id}"))
            .unwrap()
            .body()
            .touches
            .iter()
            .map(|t| (t.kind_name(), t.op.as_str(), t.what.as_str()))
            .collect::<Vec<_>>()
    };
    assert_eq!(what("table"), [("db_sql", "read_sql_table", "readings")]);
    assert_eq!(what("rebuild"), [("process", "system", "make report")]);
    assert_eq!(what("convert"), [("process", "create_subprocess_exec", "pandoc")], "`proc.wait()` runs nothing more");
    assert_eq!(what("shell"), [("process", "create_subprocess_shell", "command")]);
    assert_eq!(what("run_notebook"), [("process", "execute", "")], "`NotebookClient(nb).execute()`");
    assert_eq!(what("run_quick"), [("process", "execute", "nb")], "`nbclient.execute(nb)`");
    assert_eq!(what("complete"), [("model", "generate", "ids")], "`model.generate(ids, ..)`");
}

#[test]
fn signatures_become_takes_and_returns() {
    let g = fixture();
    let takes = edges(&g, EdgeKind::Takes);
    let returns = edges(&g, EdgeKind::Returns);
    assert!(takes.contains(&("fixture.sinks.base.Sink.write".into(), "fixture.schema.Graph".into())));
    assert!(returns.contains(&("fixture.build.build_graph".into(), "fixture.schema.Graph".into())));
}

#[test]
fn entry_points_from_decorators_scripts_and_makefile() {
    let g = fixture();
    let entry = |id: &str| g.node(id).unwrap().entry.clone().unwrap();
    assert_eq!(entry("fixture.cli.build").kind, EntryKind::Cli);
    assert_eq!(entry("fixture.cli.build").label, "build");
    assert_eq!(entry("fixture.cli.cmd_report").label, "report");
    assert_eq!(entry("fixture.cli.main").label, "fixture", "[project.scripts]");
    // a command rests on the framework its decorator comes from, and is named by that framework's
    // rule: click drops `_cmd`, typer keeps it; a `command` of the project's own is no command line
    assert_eq!(entry("fixture.cli.export_cmd").label, "export");
    assert_eq!(entry("fixture.cli.count_notes").label, "tally", "`name=`");
    assert!(g.node("fixture.cli.ping").unwrap().entry.is_none(), "`bot.command` is the project's own");
    assert_eq!(
        (entry("fixture.tasks.clean_cache_cmd").kind, entry("fixture.tasks.clean_cache_cmd").label),
        (EntryKind::Cli, "clean-cache-cmd".to_string())
    );
    assert_eq!(entry("fixture.tasks.synchronise").label, "sync");
    // what pyproject.toml declares beside `[project.scripts]`: a graphical program (its extras
    // left out), Poetry's program, and a plugin its host loads by group and name
    assert_eq!((entry("fixture.launch.window").kind, entry("fixture.launch.window").label), (EntryKind::Cli, "fixture-window".to_string()));
    assert_eq!(entry("fixture.launch.nightly").label, "fixture-nightly", "[tool.poetry.scripts]");
    assert_eq!(
        (entry("fixture.launch.Exporter").kind, entry("fixture.launch.Exporter").label),
        (EntryKind::Export, "fixture.exporters:plain".to_string())
    );
    // a tool server's tools, by the type of the value its decorators are on, not by its name
    assert_eq!((entry("fixture.cli.describe").kind, entry("fixture.cli.describe").label), (EntryKind::Mcp, "describe".to_string()));
    assert_eq!(entry("fixture.cli.summarise").label, "summary");
    assert!(g.node("fixture.cli.kept").unwrap().entry.is_none(), "`mcp_prompts` is no tool server");
    // what the package exports at its root (`__all__`) are routes into it as a library
    assert_eq!(entry("fixture.schema.Graph").kind, EntryKind::Export);
    assert_eq!(entry("fixture.sinks.base.Sink").label, "Sink", "exported by the root and by `sinks`: the root's label");
    // a public subpackage's `__all__` is a door too, named by its path
    assert_eq!(entry("fixture.sinks.duck.DuckSink").label, "sinks.DuckSink");
    assert!(g.node("fixture.build.build_graph").unwrap().entry.is_none(), "not exported");
    let build = g.make.iter().find(|t| t.label == "build").unwrap();
    assert_eq!(build.doc, "build the thing");
    assert_eq!(build.calls, vec!["fixture.cli.build".to_string(), "make:report".to_string()]);
    assert!(g.make.iter().all(|t| t.label != "DEPS"), "assignments are not targets");
    let all = g.make.iter().find(|t| t.label == "all").unwrap();
    assert_eq!(all.calls, vec!["make:build".to_string()], "deps count, echo lines do not");
    let window = g.make.iter().find(|t| t.label == "window").unwrap();
    assert_eq!(window.calls, vec!["fixture.launch.window".to_string()], "a declared graphical program is a program a recipe runs");
}

#[test]
fn boundaries_are_tagged() {
    let g = fixture();
    let b = |id: &str| boundaries(g.node(id).unwrap());
    assert_eq!(b("fixture.build.download").get("net"), Some(&1));
    let write = b("fixture.sinks.duck.DuckSink.write");
    assert_eq!(write.get("db_duckdb"), Some(&1), "con.execute with duckdb imported");
    assert_eq!(write.get("disk_write"), Some(&1), "Path(...).write_text");
    assert_eq!(b("fixture.sinks.duck.DuckSink.__init__").get("db_duckdb"), Some(&1), "duckdb.connect");
    assert_eq!(b("fixture.cli.cmd_report").get("screen"), Some(&1), "click.echo");
}

/// Changing an input in place is an arrow from its type to itself; building a new value is not.
#[test]
fn mutation_is_an_endomorphism() {
    let g = fixture();
    let m = edges(&g, EdgeKind::Mutates);
    assert!(m.contains(&("fixture.schema.Graph.add".into(), "fixture.schema.Graph".into())), "self.nodes.append");
    assert!(m.contains(&("fixture.build.enrich".into(), "fixture.schema.Graph".into())), "graph.add(..) on a parameter");
    let via = |src: &str| g.edges.iter().find(|e| e.kind == EdgeKind::Mutates && e.src == src).and_then(|e| e.via.clone());
    assert_eq!(via("fixture.build.enrich").as_deref(), Some("graph"), "the edge names the input that changes");
    assert_eq!(via("fixture.schema.Graph.add").as_deref(), Some("self"));
    assert!(!m.iter().any(|(s, _)| s == "fixture.build.build_graph"), "g = Graph(); g.add(..) builds, it does not change an input");
}

/// A notebook is a script whose code cells run in order; a line for IPython is not Python.
#[test]
fn a_notebook_is_a_script() {
    let g = fixture();
    let nb = g.node("notebooks.explore").expect("notebooks/explore.ipynb is read");
    assert_eq!(nb.entry.as_ref().map(|e| (e.kind, e.label.as_str())), Some((EntryKind::Notebook, "notebooks/explore.ipynb")));
    assert!(
        edges(&g, EdgeKind::Calls).contains(&("notebooks.explore".into(), "fixture.build.build_graph".into())),
        "a cell calls into the package"
    );
    // a notebook found at any depth, named by its path
    let deep = g.node("notebooks.les1.01_1-verken").expect("notebooks/les1/01.1-verken.ipynb is read");
    assert_eq!(deep.entry.as_ref().map(|e| e.label.as_str()), Some("notebooks/les1/01.1-verken.ipynb"));
    assert_eq!(boundaries(nb).get("screen"), Some(&1), "print in the last cell");
}

/// The Hugging Face Hub is a host on the web: a download reads it, an upload writes it, and
/// a client built from the library (`api = HfApi()`) is the library. An array annotated with
/// jaxtyping is one type with its shape.
#[test]
fn the_hub_is_a_host_and_a_shape_is_a_type() {
    let g = fixture();
    let t = |id: &str| {
        g.node(id)
            .unwrap()
            .body()
            .touches
            .iter()
            .map(|t| (t.kind_name(), t.artifact.as_str(), t.dir_name(), t.what.as_str()))
            .collect::<Vec<_>>()
    };
    assert_eq!(t("fixture.build.fetch"), [("net", "huggingface.co", "read", "repo_id")]);
    assert_eq!(t("fixture.build.publish"), [("net", "huggingface.co", "write", "repo_id")], "`api.upload_folder` through the client `api`");
    assert!(t("fixture.build.pool").is_empty(), "tensor arithmetic stays in memory");
    let e = |kind: EdgeKind| edges(&g, kind).into_iter().filter(|(s, _)| s == "fixture.build.pool").map(|(_, d)| d).collect::<Vec<_>>();
    assert_eq!(e(EdgeKind::Takes), ["ext:jaxtyping.Float[Tensor, batch seq dim]"]);
    assert_eq!(e(EdgeKind::Returns), ["ext:jaxtyping.Float[Tensor, batch dim]"]);
    assert_eq!(g.node("ext:jaxtyping.Float[Tensor, batch dim]").unwrap().name, "Float[Tensor, batch dim]");
}

/// A call knows the arms of the choices it sits in; calls in different arms of one choice are
/// alternatives, and a comparison knows which choice it decides.
#[test]
fn alternatives_carry_their_arms() {
    let g = fixture();
    let arms = |src: &str, dst: &str| g.edges.iter().find(|e| e.src == src && e.dst == dst).map(|e| e.arms.clone()).unwrap();
    assert_eq!(arms("fixture.build.prepare", "fixture.build.build_graph"), [(0, 0)], "`a if c else b`: a is arm 0");
    assert_eq!(arms("fixture.build.prepare", "fixture.build.empty"), [(0, 1)]);
    assert!(arms("fixture.build.prepare", "fixture.build.enrich").is_empty(), "outside every choice");
    let cmp = &g.node("fixture.build.prepare").unwrap().body().compares;
    assert_eq!(cmp.iter().map(|c| (c.subject.as_str(), c.choice)).collect::<Vec<_>>(), [("mode", Some(0)), ("mode", Some(1))]);

    let rs = analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixture-rs"), &Options::default()).unwrap();
    let arms = |dst: &str| rs.edges.iter().find(|e| e.src == "handed.choose.resolve" && e.dst == dst).map(|e| e.arms.clone()).unwrap();
    assert_eq!(arms("handed.choose.load"), [(0, 0)], "match arm `Some(p)`");
    assert_eq!(arms("handed.choose.find"), [(0, 1)], "`None`; the receiver of `unwrap_or_else` always runs");
    assert_eq!(arms("handed.choose.infer"), [(0, 1), (1, 1)], "`None`, then its argument");
}

/// Each arm of a choice says what decides it, as the code writes it: the first arm of an `if` or a
/// conditional expression its condition, the other `otherwise`; a `match` arm its pattern; of an
/// or-else the second arm runs otherwise. A choice of one arm is a choice: its step runs when the
/// condition holds, in the route as in its text.
#[test]
fn an_arm_says_what_decides_it() {
    let g = fixture();
    let whens = |g: &Graph, id: &str| g.node(id).unwrap().body().whens.clone();
    let w = |c: u32, a: u32, t: &str| (c, a, t.to_string());
    assert_eq!(
        whens(&g, "fixture.build.prepare"),
        [w(0, 0, "mode == \"fresh\""), w(0, 1, "otherwise"), w(1, 0, "mode == \"fresh\"")],
        "`a if mode == \"fresh\" else b`, then `if mode == \"fresh\": print(..)`"
    );
    let rs = analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixture-rs"), &Options::default()).unwrap();
    assert_eq!(whens(&rs, "handed.choose.resolve"), [w(0, 0, "Some(p)"), w(0, 1, "None"), w(1, 1, "otherwise")]);
    let text = atlas_engine::story::choices(&g, "fixture.build.prepare").unwrap();
    assert!(text.contains("  a  when mode == \"fresh\":  build_graph") && text.contains("  b  otherwise:  empty"), "{text}");
}

/// Where data rests is named as precisely as the code allows: a literal file, else its format;
/// a database statement reads or writes a table.
#[test]
fn files_and_tables_are_places() {
    let g = fixture();
    let t = |id: &str| {
        g.node(id).unwrap().body().touches.iter().map(|t| (t.kind_name(), t.artifact.as_str(), t.dir_name())).collect::<HashSet<_>>()
    };
    let write = t("fixture.sinks.duck.DuckSink.write");
    assert!(write.contains(&("db_duckdb", "nodes", "write")), "INSERT INTO nodes");
    assert!(write.contains(&("disk_write", "out.json", "")), "Path(\"out.json\").write_text");
    let save = t("fixture.build.save");
    assert!(save.contains(&("disk_write", "*.csv", "")), "to_csv(f\"{{name}}.csv\"): a name made of a value is known only by its format");
    let fixed = |id: &str| g.node(id).unwrap().body().touches.iter().filter(|t| t.fixed).map(|t| t.op.as_str()).collect::<Vec<_>>();
    assert_eq!(fixed("fixture.sinks.duck.DuckSink.write"), ["write_text"], "Path(\"out.json\") is decided by the code alone");
    assert!(fixed("fixture.build.save").is_empty(), "f\"{{name}}.csv\", path and name come from the caller");
    assert!(save.contains(&("disk_write", "*.parquet", "")), "to_parquet(path): only the format is known");
    assert!(save.contains(&("disk_write", "", "")), "open(name, \"w\") writes");
    assert!(save.contains(&("screen", "", "")), "print");
    assert!(t("fixture.build.load").contains(&("disk_read", "*.csv", "")), "pd.read_csv named in a module-level table");
    let returns = edges(&g, EdgeKind::Returns);
    assert!(
        returns.contains(&("fixture.build.load".into(), "fixture.build.Loaded".into())),
        "Loaded = Union[Graph, Node] is a type of its own"
    );
    let variants = edges(&g, EdgeKind::Variant);
    assert!(variants.contains(&("fixture.build.Loaded".into(), "fixture.schema.Graph".into())), "a sum and the types it may be");
    assert!(variants.contains(&("fixture.build.Loaded".into(), "fixture.schema.Node".into())));
    let takes = edges(&g, EdgeKind::Takes);
    assert!(
        takes.contains(&("fixture.build.register".into(), "fixture.sinks.base.Sink".into())),
        "Type[SinkT], SinkT = TypeVar(bound=\"Sink\")"
    );
    assert!(
        takes.contains(&("fixture.build.describe".into(), "fixture.schema.Graph".into())),
        "a quoted annotation is a forward reference"
    );
    // a generic class's parameters are types, a container's are the values it holds
    let of = |src: &str, kind: EdgeKind| edges(&g, kind).into_iter().filter(|(s, _)| s == src).map(|(_, d)| d).collect::<Vec<_>>();
    assert_eq!(of("fixture.build.hold", EdgeKind::Returns), ["fixture.build.Holder"], "`\"Holder[Node]\"` is one Holder");
    assert_eq!(of("fixture.build.unwrap", EdgeKind::Takes), ["fixture.build.Holder"], "`Holder[Node]` too");
    assert_eq!(of("fixture.build.unwrap", EdgeKind::Returns), ["fixture.schema.Node"]);
    assert!(returns.contains(&("fixture.build.describe".into(), "fixture.schema.Node".into())));
}

/// A Rust workspace parses; the sample is this workspace.
#[test]
fn rust_workspace_parses() {
    let g = atlas();
    let ids: HashSet<&str> = g.nodes.iter().map(|n| n.id.as_str()).collect();
    for id in ["atlas-engine", "atlas-engine.model", "atlas-engine.view", "atlas-engine.python", "atlas-engine.rust", "atlas"] {
        assert!(ids.contains(id), "module {id}");
    }
    assert!(ids.contains("atlas-engine.model.Graph") && ids.contains("atlas-engine.view.View"));
    assert_eq!(g.node("atlas.main").unwrap().entry.as_ref().unwrap().kind, EntryKind::Cli);
    // `atlas` is handed what clap parses its command line into: the global `--libraries` and
    // `--no-cache`; the subcommand is no option of its own, and `Cli::parse()` is the step that
    // reads it
    let main = g.node("atlas.main").unwrap();
    assert_eq!(main.entry.as_ref().unwrap().options, ["--libraries", "--no-cache"]);
    let parse = main.body().sites.iter().find(|s| s.command_line.is_some()).expect("main parses its command line");
    assert_eq!(
        (parse.to.as_str(), parse.command_line.clone().unwrap()),
        ("atlas.Cli", vec!["--libraries".to_string(), "--no-cache".to_string()])
    );
    assert_eq!(
        g.node("atlas-engine.analyze").unwrap().entry.as_ref().unwrap().kind,
        EntryKind::Export,
        "pub fn at the root of a library crate"
    );
    let hide = g.node("atlas-engine.view.hide_private").unwrap().entry.as_ref().unwrap();
    assert_eq!((hide.kind, hide.label.as_str()), (EntryKind::Export, "view::hide_private"), "pub in a `pub mod`, by its path");
    let inh = edges(&g, EdgeKind::Inherits);
    assert!(inh.contains(&("atlas-engine.view.Fiber".into(), "atlas-engine.view.View".into())), "impl View for Fiber");
    let calls = edges(&g, EdgeKind::Calls);
    assert!(calls.contains(&("atlas.main".into(), "atlas.view".into())));
    // the command reads the repository (one reading, or two as one), asks the principles of what it
    // read, draws it as one graph, takes its flow, and makes the page of those and writes it
    for step in
        ["atlas.page.reading", "atlas.page.Reading.findings", "atlas.page.Reading.drawn", "atlas-engine.flow.of", "atlas.page.Html.of"]
    {
        assert!(calls.contains(&("atlas.view".into(), step.into())), "the command runs {step}: Graph -> (Flow, Findings) -> Html");
    }
    assert!(calls.contains(&("atlas.view".into(), "atlas.page.Html.write".into())), "the command writes the page");
    assert!(
        calls.contains(&("atlas.page.reading".into(), "atlas-engine.parts.analyze_parts".into()))
            && calls.contains(&("atlas.page.Reading.drawn".into(), "atlas-engine.parts.merged".into())),
        "cli calls into the engine"
    );
    assert!(calls.contains(&("atlas.page.Reading.findings".into(), "atlas-engine.principles.check".into())), "each part is checked");
    assert!(
        calls.contains(&("atlas.page.Reading.drawn".into(), "atlas-engine.diff.union".into())),
        "two readings are one graph, their union"
    );
    assert!(
        calls.contains(&("atlas-engine.parts.analyze_parts".into(), "atlas-engine.parts.graph_of".into())),
        "each part is analyzed in its language"
    );
    assert!(calls.contains(&("atlas-engine.language.Rust.parse".into(), "atlas-engine.rust.parse".into())), "trait impl calls the module");
    let returns = edges(&g, EdgeKind::Returns);
    assert!(returns.contains(&("atlas-engine.analyze".into(), "atlas-engine.model.Graph".into())), "`pub use model::*` resolves Graph");
    // `let lang = language::detect(..)?; lang.parse(..)`: a local typed by the callee's return
    assert!(calls.contains(&("atlas-engine.parts.graph_of".into(), "atlas-engine.language.Language.parse".into())));
    // typed parameters give receivers a type: `store.add_edge(..)` with `store: &mut GraphBuilder`
    assert!(calls.contains(&("atlas-engine.python.passes.collect_symbols".into(), "atlas-engine.builder.GraphBuilder.add_edge".into())));
    // the repository is read in one place, and a reader reads what was read
    assert!(boundaries(g.node("atlas-engine.repo.Repo.read").unwrap()).get("disk_read").is_some(), "the listing");
    assert!(boundaries(g.node("atlas-engine.repo.Repo.bytes").unwrap()).get("disk_read").is_some(), "a file's text, once");
    for reader in [
        "atlas-engine.rust.parse",
        "atlas-engine.typescript.parse",
        "atlas-engine.entries.read_makefile",
        "atlas-engine.language.Python.detect",
        "atlas-engine.language.Python.parse",
    ] {
        assert!(!boundaries(g.node(reader).unwrap()).contains_key("disk_read"), "{reader} reads the Repo, not the disk");
    }
    let test = |id: &str| g.node(id).and_then(|n| n.entry.as_ref()).map_or(false, |e| e.kind == EntryKind::Test);
    let readers: Vec<&str> = g
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Calls && e.dst == "atlas-engine.repo.Repo.read" && !test(&e.src))
        .map(|e| e.src.as_str())
        .collect();
    assert_eq!(
        readers,
        ["atlas-engine.language.read"],
        "the repository is read in one place, and what it is depends on nothing read after it"
    );
    assert!(boundaries(g.node("atlas.view").unwrap()).contains_key("screen"), "eprintln!");
    let m = edges(&g, EdgeKind::Mutates);
    assert!(m.contains(&("atlas-engine.model.Graph.recount".into(), "atlas-engine.model.Graph".into())), "&mut self");
    // a method runs on the value the call before it gives: `Repo::read(&root, ..)?.asked_for(package)`; the
    // `find_map` whose result the `match` takes apart into `root` and `package` is a step of its own
    let read: Vec<&str> = g.node("atlas-engine.language.read").unwrap().body().sites.iter().map(|s| s.to.as_str()).collect();
    assert_eq!(
        read,
        [
            "atlas-engine.repo.Repo.read",
            "atlas-engine.language.languages",
            "",
            "atlas-engine.repo.Repo.read",
            "atlas-engine.repo.Repo.asked_for"
        ]
    );
    let places = |id: &str| {
        g.node(id).unwrap().body().touches.iter().map(|t| (t.kind_name().to_string(), t.op.clone(), t.artifact.clone())).collect::<Vec<_>>()
    };
    // a name bound by `if let`: `if let Some(d) = dest.parent()` is the directory, under the
    // revision's `env::temp_dir()`, of the file written there, so making it is part of writing it
    let revision = places("atlas-engine.changes.revision");
    assert!(
        revision.contains(&("disk_write".into(), "write".into(), "$TMPDIR/".into()))
            && !revision.iter().any(|(_, op, _)| op == "create_dir_all"),
        "{revision:?}"
    );
    // `atlas analyze` writes its graph where `-o` says, else where `made` puts it: in `.atlas/` at
    // the top of the git repository, a folder found on disk that the code cannot name, or for a
    // clone in the system's temporary folder; the command line decides, not the code alone
    let written = places("atlas.main");
    assert!(written.contains(&("disk_write".into(), "write".into(), "$TMPDIR/atlas/graph.json".into())), "{written:?}");
    assert!(
        written.contains(&("disk_write".into(), "write".into(), "*/.atlas/graph.json".into())),
        "the folder found on disk has no name, the file below it has: {written:?}"
    );
    let graph = g.node("atlas.main").unwrap().body().touches.iter().find(|t| t.artifact == "$TMPDIR/atlas/graph.json").unwrap();
    assert!(!graph.fixed && graph.by.is_empty(), "{graph:?}");
    // a `let` holds the type of the parameter it is handed to: `joined`'s graph, the one graph
    // read or the merge of several, is the `Graph` that `join(g: &mut Graph)` takes
    let joining = g.node("atlas-engine.parts.joined").unwrap().body().sites.iter().find(|s| s.to == "atlas-engine.parts.join").unwrap();
    assert_eq!(joining.hands.iter().map(|h| (h.name.as_str(), h.ty.as_str())).collect::<Vec<_>>(), [("g", "atlas-engine.model.Graph")]);
    // a generic's arguments are values where it holds them: `analyze_parts` gives the Graphs of a
    // `Result<Vec<Graph>>`, `merged` takes the Graphs of a `Vec<Graph>`, a Graph has the Nodes of
    // its `Vec<Node>`, and `Ix::out` gives the Edges of an `impl Iterator<Item = &Edge>`, whose
    // `Item` names what it gives, not flow's `Item`
    let takes = edges(&g, EdgeKind::Takes);
    let has = edges(&g, EdgeKind::Has);
    let pair = |a: &str, b: &str| (a.to_string(), b.to_string());
    assert!(returns.contains(&pair("atlas-engine.parts.analyze_parts", "atlas-engine.model.Graph")));
    assert!(takes.contains(&pair("atlas-engine.parts.merged", "atlas-engine.model.Graph")));
    assert!(has.contains(&pair("atlas-engine.model.Graph", "atlas-engine.model.Node")));
    assert!(returns.contains(&pair("atlas-engine.flow.Ix.out", "atlas-engine.model.Edge")));
    assert!(!returns.contains(&pair("atlas-engine.flow.Ix.out", "atlas-engine.flow.Item")));
    // a callback is handed no value: `initializer(node: impl Fn(&str) -> Option<&Node>)` takes no
    // Node, and the `fn(..) -> Part` a `ChoiceSyntax` keeps holds no Part
    assert!(!takes.contains(&pair("atlas-engine.model.Node.initializer", "atlas-engine.model.Node")));
    assert!(!has.contains(&pair("atlas-engine.tree.ChoiceSyntax", "atlas-engine.tree.Part")));
    // the crates Atlas calls are read where Cargo keeps them, as far as its calls reach: `Regex::new`
    // is a step into regex's own code, the library's and no part of Atlas; of ty's `types`, a module
    // of thousands of lines, only what Atlas calls is drawn; compiled code (tree-sitter) and the
    // crates the tables describe (walkdir, ignore) are not read
    let new = g.node("regex.regex.string.Regex.new").expect("a crate's function Atlas calls is read");
    assert!(new.library && g.parts.iter().all(|p| !p.id.starts_with("regex")));
    assert!(g.node("atlas-engine.entries.read_makefile").unwrap().body().sites.iter().any(|s| s.to == new.id));
    assert!(g.nodes.iter().filter(|n| n.module == "ty_python_semantic.types").count() < 10);
    assert!(g.nodes.iter().all(|n| !["tree_sitter", "walkdir", "ignore"].contains(&n.id.split('.').next().unwrap_or(""))));
}

/// Atlas obeys its own algebra: the steps of `analyze` move data between the repository as read
/// (`Repo`), the `Options` of the reading, a `Language`, the `Graph` and a `View` of it, and
/// nothing else.
/// A new type between parse and Graph is a box the algebra does not have, and the code changes,
/// not the picture.
#[test]
fn atlas_reads_as_its_algebra() {
    let g = atlas();
    let classes: HashSet<&str> = g.nodes.iter().filter(|n| n.kind == NodeKind::Class).map(|n| n.id.as_str()).collect();
    let name = |id: &str| id.rsplit('.').next().unwrap().to_string();
    let steps_of = |f: &str| g.edges.iter().filter(|e| e.src == f && e.kind == EdgeKind::Calls).map(|e| e.dst.as_str()).collect::<Vec<_>>();
    let short = |steps: &[&str]| steps.iter().map(|s| s.trim_start_matches("atlas-engine.").to_string()).collect::<Vec<_>>();
    assert_eq!(
        short(&steps_of("atlas-engine.analyze")),
        ["language.read", "parts.graph_of"],
        "analyze: Disk(repo) -> read -> Repo -> parse -> Graph"
    );
    let steps = steps_of("atlas-engine.parts.graph_of");
    assert_eq!(
        short(&steps),
        ["language.of", "language.Language.parse", "entries.read_makefile", "parts.assign", "view.reached", "view.View.apply"],
        "the graph of what was read, step by step"
    );
    for step in steps {
        for e in g.edges.iter().filter(|e| e.src == step && matches!(e.kind, EdgeKind::Takes | EdgeKind::Returns | EdgeKind::Mutates)) {
            if classes.contains(e.dst.as_str()) {
                assert!(
                    ["Repo", "Graph", "Language", "Options", "View"].contains(&name(&e.dst).as_str()),
                    "{step} moves a {}: the algebra has no such place",
                    e.dst
                );
            }
        }
    }
    let returns = edges(&g, EdgeKind::Returns);
    assert!(
        returns.contains(&("atlas-engine.language.Language.parse".into(), "atlas-engine.model.Graph".into())),
        "parse: Disk(repo) × Options -> Graph"
    );
    let takes = edges(&g, EdgeKind::Takes);
    assert!(
        takes.contains(&("atlas-engine.language.Language.parse".into(), "atlas-engine.options.Options".into())),
        "what a reading depends on is its argument, not a global"
    );
    let mutates = edges(&g, EdgeKind::Mutates);
    assert!(mutates.contains(&("atlas-engine.parts.assign".into(), "atlas-engine.model.Graph".into())), "assign: Graph -> Graph");
}

#[test]
fn views_are_fibers_and_quotients() {
    let g = fixture();
    let reach = reach_from(&g, "fixture.cli.build").apply(&g);
    let ids: HashSet<&str> = reach.nodes.iter().map(|n| n.id.as_str()).collect();
    assert!(ids.contains("fixture.build.Pipeline.run"));
    assert!(ids.contains("fixture.sinks.duck.DuckSink.write"));
    assert!(!ids.contains("fixture.cli.cmd_report"));

    let priv_free = hide_private().apply(&g);
    assert!(priv_free.node("fixture.store._session").is_none());
    // the initializer runs on every user's behalf, through a call to the class: no private name
    assert!(priv_free.node("fixture.sinks.duck.DuckSink.__init__").is_some());
    assert!(priv_free.node("fixture.sinks").is_some(), "packages stay");

    let q = Compose(vec![Box::new(hide_private()), Box::new(by_module())]).apply(&g);
    let ids: HashSet<&str> = q.nodes.iter().map(|n| n.id.as_str()).collect();
    assert!(ids.contains("fixture.cli") && ids.contains("fixture.sinks.duck"));
    let cli_to_sink = q.edges.iter().find(|e| e.src == "fixture.cli" && e.dst == "fixture.sinks.duck" && e.kind == EdgeKind::Instantiates);
    assert!(cli_to_sink.unwrap().weight.unwrap() >= 1);
    assert!(q.edges.iter().all(|e| e.src != e.dst), "edges inside a class vanish");
}

/// Python's types are ty's. Where ty does not know a value's type, neither does Atlas: a
/// method without a return annotation gives back a value of no type, and a database call on it
/// is not drawn.
#[test]
fn an_unknown_type_is_not_guessed() {
    let g = fixture();
    let kinds = |id: &str| g.node(id).unwrap().body().touches.iter().map(|t| t.kind_name()).collect::<HashSet<_>>();
    assert!(kinds("fixture.store.Ledger.keep").contains("db_sqlite"), "`self.con = sqlite3.connect(..)`");
    assert!(kinds("fixture.store.Journal.connection").contains("db_sqlite"), "`sqlite3.connect(..)` itself");
    assert!(!kinds("fixture.store.Journal.entries").contains("db_sqlite"), "`con = self.connection()`, which says nothing");
    let uses = |src: &str, dst: &str| g.edges.iter().any(|e| e.src == src && e.dst == dst && e.uses);
    assert!(uses("fixture.store.Journal.entries", "fixture.store.Journal.connection"), "its value is used");
    assert!(!uses("fixture.build.Pipeline.run", "fixture.sinks.duck.DuckSink.write"), "a call as a statement");
}

/// A library's type whose calls reach a place is a door to that place, not data: a requests
/// session reaches the network, a data frame stays in memory.
#[test]
fn a_client_is_a_door() {
    let g = fixture();
    assert_eq!(g.node("ext:requests.Session").map(|n| n.door().map_or("", |d| d.as_str())), Some("net"));
    assert_eq!(g.node("ext:neo4j.Driver").map(|n| n.door().map_or("", |d| d.as_str())), Some("db_graph"));
    assert_eq!(g.node("ext:pandas.DataFrame").map(|n| n.door().map_or("", |d| d.as_str())), Some(""));
}

/// Two routes meet on the disk where one writes the place the other reads. A path is evaluated
/// from the code: a constant (`DATA / "raw"`), what a parameter is handed (`unpack(.., out)`),
/// an entry's declared default (`--out`). An unknown part of a path is a directory: what is
/// made in `data/raw/{kind}/{archive}` and what lists `data/raw` meet at `data/raw/`; a file
/// named under an unknown directory is that name under any (`data/*/report.json`).
#[test]
fn routes_meet_on_the_disk() {
    let g = fixture();
    let places = |id: &str| g.node(id).unwrap().body().touches.iter().map(|t| (t.kind_name(), t.artifact.as_str())).collect::<HashSet<_>>();
    assert!(places("fixture.files.unpack").contains(&("disk_write", "data/raw/")), "{:?}", places("fixture.files.unpack"));
    assert!(places("fixture.files.kinds").contains(&("disk_read", "data/raw/")), "{:?}", places("fixture.files.kinds"));
    assert!(places("fixture.files.report").contains(&("disk_write", "data/*/report.json")), "{:?}", places("fixture.files.report"));
    assert!(
        places("fixture.files.read_report").contains(&("disk_read", "data/*/report.json")),
        "{:?}",
        places("fixture.files.read_report")
    );
}

/// A function of the package handed on as a value is called by whoever receives it: an
/// executor's work, a thread's target, an iterator's map, as in TypeScript.
#[test]
fn a_function_handed_on_is_called() {
    let g = fixture();
    let calls = edges(&g, EdgeKind::Calls);
    assert!(calls.contains(&("fixture.run.run_all".into(), "fixture.run.work".into())), "`pool.submit(work, n)`");
    assert!(calls.contains(&("fixture.run.run_all".into(), "fixture.run.done".into())), "`Thread(target=done)`");
    let rs = analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixture-rs"), &Options::default()).unwrap();
    let calls = edges(&rs, EdgeKind::Calls);
    assert!(calls.contains(&("handed.doubled".into(), "handed.double".into())), "`.map(double)`: {calls:?}");
    assert!(calls.contains(&("handed.Item.labels".into(), "handed.Item.label".into())), "`.map(Self::label)`: {calls:?}");
}

/// Rust is read by Rust's own rules: Cargo's targets (one lib, a bin per `main`), `Self` as the
/// impl's type, the crate's public paths (`pub mod`, `pub use`), an `enum` as a sum, `matches!`
/// as a comparison, and a library's value moving data where it is used, not where it is built.
#[test]
fn rust_is_read_by_rusts_semantics() {
    let g = analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixture-rs"), &Options::default()).unwrap();
    let entry = |id: &str| g.node(id).and_then(|n| n.entry.as_ref()).map(|e| (e.kind, e.label.clone()));
    assert_eq!(entry("handed.main.main"), Some((EntryKind::Cli, "handed".into())), "src/main.rs is the bin named after the crate");
    assert_eq!(entry("handed.bin.tool.main"), Some((EntryKind::Cli, "tool".into())), "src/bin/tool.rs");
    let make = |t: &str| g.make.iter().find(|m| m.label == t).map(|m| m.calls.clone()).unwrap_or_default();
    assert_eq!(
        (make("run"), make("tool")),
        (vec!["handed.main.main".to_string()], vec!["handed.bin.tool.main".to_string()]),
        "a recipe runs the crate's binaries by name"
    );
    assert_eq!(entry("handed.doubled"), Some((EntryKind::Export, "doubled".into())), "the lib keeps its exports beside a bin");
    assert_eq!(entry("handed.shapes.Shape"), Some((EntryKind::Export, "Shape".into())), "`pub use shapes::Shape` from a private module");
    assert_eq!(entry("handed.world.fetch"), Some((EntryKind::Export, "world::fetch".into())), "`pub mod world`");
    assert!(edges(&g, EdgeKind::Returns).contains(&("handed.shapes.Shape.square".into(), "handed.shapes.Shape".into())), "`-> Self`");
    assert!(!edges(&g, EdgeKind::Instantiates).iter().any(|(s, _)| s == "handed.describe"), "`item.to_string()` builds no Item");
    let variants = edges(&g, EdgeKind::Variant);
    assert!(
        variants.contains(&("handed.world.Line".into(), "handed.world.Page".into()))
            && variants.contains(&("handed.world.Line".into(), "handed.world.Log".into())),
        "{variants:?}"
    );
    assert!(!edges(&g, EdgeKind::Has).iter().any(|(s, _)| s == "handed.world.Line"), "an enum is one of its variants, not all of them");
    let weight = &g.node("handed.world.weight").unwrap().body().compares;
    assert_eq!(
        (weight[0].subject.as_str(), weight[0].cases.clone()),
        ("kind", vec!["page".to_string(), "log".to_string()]),
        "`matches!(kind, \"page\" | \"log\")`"
    );
    let t = |id: &str| {
        g.node(id).unwrap().body().touches.iter().map(|t| (t.kind_name().to_string(), t.op.clone(), t.artifact.clone())).collect::<Vec<_>>()
    };
    assert!(t("handed.world.client").is_empty(), "building a client moves nothing: {:?}", t("handed.world.client"));
    // `client.get(url).send()` on a `Client::new()`: at the address `main` hands it, and at the
    // one the export's users hand
    assert_eq!(t("handed.world.fetch"), [("net".into(), "get".into(), String::new()), ("net".into(), "get".into(), "example.org".into())]);
    assert!(t("handed.world.record").contains(&("db_sqlite".into(), "execute".into(), "log".into())), "{:?}", t("handed.world.record"));
    assert_eq!(t("handed.world.notes"), [("disk_read".into(), "open".into(), "notes.txt".into())], "the handle reads where it was opened");
    assert_eq!(
        t("handed.world.Log.append"),
        [("disk_write".into(), "open".into(), String::new())],
        "`OpenOptions::new().append(true)` writes"
    );
    assert_eq!(t("handed.world.clean"), [("process".into(), "new".into(), String::new())], "one program run");
    assert!(t("handed.world.quit").is_empty(), "`process::exit` runs no program");
}

/// Rust reads a path as built, and a statement where it is whole: through `join`, `format!`,
/// a `const`, a function of the program that gives the path, and what a caller hands. A place a
/// caller hands is that caller's; an entry the program also calls is run by its users too.
#[test]
fn rust_reads_paths_as_built() {
    let g = analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixture-rs"), &Options::default()).unwrap();
    let t = |id: &str| {
        let mut out: Vec<(String, String, Vec<String>, Vec<String>)> = g
            .node(&format!("handed.world.{id}"))
            .unwrap()
            .body()
            .touches
            .iter()
            .filter(|t| t.op != "open")
            .map(|t| (t.kind_name().to_string(), t.artifact.clone(), t.by.clone(), t.users_of.clone()))
            .collect();
        out.sort();
        out
    };
    let here = |kind: &str, artifact: &str| (kind.to_string(), artifact.to_string(), vec![], vec![]);
    let by = |kind: &str, artifact: &str, f: &str| (kind.to_string(), artifact.to_string(), vec![format!("handed.world.{f}")], vec![]);
    let users = |kind: &str, f: &str| (kind.to_string(), String::new(), vec![], vec![format!("handed.world.{f}")]);
    // a file whose name is known under a directory its users hand is that name under any directory
    let under = |kind: &str, file: &str, f: &str| (kind.to_string(), format!("*/{file}"), vec![], vec![format!("handed.world.{f}")]);
    assert_eq!(
        t("report"),
        [under("disk_write", "report.html", "report"), by("disk_write", "site/report.html", "publish")],
        "`dir.join(REPORT)`: `publish` hands `Path::new(\"site\")`, and `report` is exported"
    );
    assert_eq!(t("stamp"), [here("disk_write", "out/")], "`format!(\"out/{{name}}.txt\")`: the directory before what is not known");
    assert_eq!(
        t("keep_scratch"),
        [here("disk_write", "$TMPDIR/handed/scratch.log")],
        "the path `scratch()` gives, under `env::temp_dir()`"
    );
    assert_eq!(t("prepare"), [here("db_sqlite", "seen")], "`conn.execute(SCHEMA, [])`");
    assert_eq!(t("run"), [users("db_sqlite", "run"), by("db_sqlite", "seen", "forget")], "`run(path, \"DELETE FROM seen\")`");
    // a route shows what its own calls hand
    let publish = atlas_engine::story::route(&g, "handed.world.publish", 3).unwrap();
    assert!(publish.contains("writes site/report.html") && !publish.contains("writes dir"), "{publish}");
    let report = atlas_engine::story::route(&g, "handed.world.report", 3).unwrap();
    assert!(!report.contains("site/report.html"), "{report}");
}

/// `atlas check .` inside a crate reads the crate: the walk's root is read whatever it is called.
#[test]
fn a_crate_reads_from_inside() {
    let g = analyze(std::path::Path::new("."), &Options::default()).expect("the engine crate, from its own directory");
    assert!(g.nodes.iter().filter(|n| n.kind == NodeKind::Module).count() > 10);
}

/// A file call's file is what its library types as one: the path a method of a path is called
/// on (the mode of `p.open("w")` is no path), each of `shutil.copy`'s two paths in its
/// direction, the file a handle was opened at, and never a literal that is not the path
/// (`na_rep="NA"`, `encoding="utf-8"`): a path known only at run time is named by the code that
/// holds it.
#[test]
fn a_file_is_what_the_api_types_as_one() {
    let g = fixture();
    let t = |id: &str| {
        g.node(id)
            .unwrap()
            .body()
            .touches
            .iter()
            .map(|t| (t.kind_name().to_string(), t.op.clone(), t.artifact.clone(), t.fixed))
            .collect::<Vec<_>>()
    };
    let keep = t("fixture.files.keep");
    assert!(keep.iter().all(|(_, _, a, fixed)| !fixed && a != "w"), "`cache_file.open(\"w\")`: the path is the parameter: {keep:?}");
    assert!(
        keep.iter().any(|(k, op, _, _)| k == "disk_write" && op == "dump"),
        "`json.dump(.., f)` writes the file `f` was opened at: {keep:?}"
    );
    let named: Vec<&str> =
        g.node("fixture.files.keep").unwrap().body().touches.iter().filter(|t| t.op == "open").map(|t| t.what.as_str()).collect();
    assert_eq!(named, ["keep.cache_file"], "the file is the path it is opened at, not its encoding: `keep`'s `cache_file`");
    let install = t("fixture.files.install");
    assert!(install.iter().any(|(k, _, a, _)| k == "disk_write" && a == "data/config.toml"), "{install:?}");
    assert!(install.iter().any(|(k, _, _, _)| k == "disk_read"), "the source is read: {install:?}");
    let export = t("fixture.files.export");
    assert!(export.iter().any(|(k, _, a, _)| k == "disk_write" && a == "data/frame.csv"), "{export:?}");
    assert!(export.iter().all(|(_, _, a, _)| a != "NA"), "{export:?}");
}

/// A library call reaches a place when its types say it moves data: a network library's call
/// that is handed a URL, a model's answer. Building a client moves nothing, and reading a
/// response that has arrived (`response.headers.get(..)`) is no second request.
#[test]
fn building_a_client_moves_nothing() {
    let g = fixture();
    let n = |id: &str, kind: &str| {
        g.node(id).unwrap().body().touches.iter().filter(|t| t.kind_name() == kind).map(|t| t.op.clone()).collect::<Vec<_>>()
    };
    assert_eq!(n("fixture.clients.content_type", "net"), ["get"], "`requests.get(url)`, and not `headers.get`");
    assert_eq!(n("fixture.clients.ask", "llm"), ["create"], "`messages.create(..)`, and not `Anthropic()`");
}

/// A class is a door to a place when one of its operations reaches a database, the web, a model
/// or a program: a sink, a connection. A value that reads itself from a file (an alternative
/// constructor) or shows itself on the screen is data.
#[test]
fn a_door_reaches_a_service() {
    let g = fixture();
    let door = |id: &str| g.node(id).unwrap().door().map_or("", |d| d.as_str());
    assert_eq!(door("fixture.sinks.duck.DuckSink"), "db_duckdb");
    assert_eq!(door("fixture.store.Connection"), "db_graph");
    assert_eq!(door("fixture.files.Report"), "", "`from_file` and `show` make no door");
    assert_eq!(door("fixture.schema.Graph"), "");
}

/// A Rust body lists its calls in the order they run, each with what it is handed and the name
/// it is kept by: Atlas's own `analyze` hands the `Graph` one call gives to the next, and a
/// local takes its type from what the function it is kept from gives back, wherever that is
/// defined.
#[test]
fn a_rust_body_lists_its_calls() {
    let g = atlas();
    let analyze_fn = g.nodes.iter().find(|n| n.id == "atlas-engine.parts.graph_of").unwrap();
    let step =
        |to: &str| analyze_fn.body().sites.iter().find(|s| s.to == to).unwrap_or_else(|| panic!("{to} in {:#?}", analyze_fn.body().sites));
    let held = |ty: &str, name: &str| atlas_engine::Held::new(ty, name);
    assert_eq!(
        step("atlas-engine.language.Language.parse").gives,
        [held("atlas-engine.model.Graph", "g")],
        "a method gives what it returns, not its receiver's type"
    );
    let detect = step("atlas-engine.language.of");
    assert_eq!(
        detect.hands,
        [held("atlas-engine.repo.Repo", "repo"), held("atlas-engine.options.Options", "opts")],
        "the repository is a value: what was read"
    );
    let order: Vec<&str> = analyze_fn.body().sites.iter().map(|s| s.to.as_str()).collect();
    assert!(
        order.iter().position(|t| *t == "atlas-engine.entries.read_makefile")
            < order.iter().position(|t| *t == "atlas-engine.parts.assign"),
        "{order:?}"
    );
}

/// `parse` is a function: the same files read with the same options give the same graph, to the
/// order of its edges. Each `HashMap` and `HashSet` seeds its order afresh, so a reading that
/// iterates one into the graph differs from run to run.
#[test]
fn parse_is_a_function() {
    let json = |repo: &str| {
        serde_json::to_string(&analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(repo), &Options::default()).unwrap()).unwrap()
    };
    for repo in ["tests/fixture", "tests/fixture-rs", "tests/fixture-ts", ".."] {
        let first = if repo == ".." { serde_json::to_string(atlas()).unwrap() } else { json(repo) };
        for _ in 0..3 {
            assert!(json(repo) == first, "{repo} reads differently the second time");
        }
    }
}

/// A call written inside another is composition: `Pipeline(DuckSink(..)).run()` builds a
/// `DuckSink`, hands it to `Pipeline`, and runs what that gives back. The outer call is handed the
/// inner one's result, not its arguments, and a value no name holds is named by its step.
#[test]
fn a_call_inside_a_call_is_composition() {
    let g = fixture();
    let steps = &g.node("fixture.cli.build").unwrap().body().sites;
    let to: Vec<&str> = steps.iter().map(|s| s.to.as_str()).collect();
    assert_eq!(to, ["fixture.sinks.duck.DuckSink", "fixture.build.Pipeline", "fixture.build.Pipeline.run"]);
    let sink = &steps[0].gives[0];
    assert!(sink.name.starts_with('#') && sink.ty == "fixture.sinks.duck.DuckSink", "{sink:?}");
    assert!(steps[1].hands.contains(sink), "Pipeline is handed the sink: {:?}", steps[1].hands);
    assert!(steps[2].hands.contains(&steps[1].gives[0]), "run is called on what Pipeline gives");

    // `let lang = language::of(repo, opts)?`: the unwrapping is an identity
    let atlas = atlas();
    let analyze_steps: Vec<&str> = atlas
        .node("atlas-engine.parts.graph_of")
        .unwrap()
        .body()
        .sites
        .iter()
        .map(|s| if s.to.is_empty() { s.op.as_str() } else { s.to.as_str() })
        .collect();
    assert_eq!(
        analyze_steps,
        [
            "atlas-engine.language.of",
            "atlas-engine.language.Language.parse",
            "atlas-engine.entries.read_makefile",
            "atlas-engine.parts.assign",
            "atlas-engine.view.reached",
            "atlas-engine.view.View.apply"
        ]
    );
}

/// What a step runs is the same for the agent and the page: the engine's bodies. A method called
/// on a value whose class is known runs that class's method; called where the class is not known,
/// it runs any of the overrides.
#[test]
fn a_body_is_what_a_route_runs() {
    let g = fixture();
    let bodies = atlas_engine::flow::bodies(&g);
    let run: Vec<String> = bodies["fixture.build.Pipeline.run"].steps().iter().map(|s| s.to.clone()).collect();
    assert!(
        run.contains(&"fixture.sinks.duck.DuckSink.write".to_string()),
        "`self.sink.write(..)` on the sink the pipeline holds: {run:?}"
    );

    let atlas = atlas();
    let bodies = atlas_engine::flow::bodies(&atlas);
    let mut alts = bodies["atlas-engine.language.Language.parse"].alts.clone();
    alts.sort();
    assert_eq!(alts, ["atlas-engine.language.Python.parse", "atlas-engine.language.Rust.parse", "atlas-engine.language.TypeScript.parse"]);
    let route = atlas_engine::story::route(&atlas, "atlas-engine.analyze", 2).unwrap();
    assert!(route.contains("2.2  Language.parse(repo, opts) → Graph   ·   one of 3"), "the agent reads the same alternatives: {route}");
}

/// A subclass stands in the place of the base the program uses as a type, and a family whose
/// member reaches a database is a door to it, not data.
#[test]
fn types_fall_into_families() {
    let g = fixture();
    let t = atlas_engine::families::families(&g);
    assert_eq!(t.kind.get("fixture.sinks.duck.DuckSink").map(String::as_str), Some("fixture.sinks.base.Sink"), "Pipeline takes a Sink");
    assert!(t.members["fixture.sinks.base.Sink"].contains(&"fixture.sinks.duck.DuckSink".to_string()));
    assert!(t.doors.contains(&"fixture.sinks.base.Sink".to_string()), "DuckSink writes to DuckDB, so a Sink is a door");
    assert!(!t.doors.contains(&"fixture.schema.Graph".to_string()), "a Graph is data");
}

/// Making a value is one arrow in every language: into `Memory(T)` from what it takes, with no
/// value it runs on. Python's `Report(lines)` runs the initializer, `Report.from_file(path)`
/// constructs one on the class, and so does Rust's `Shape::square()`.
#[test]
fn a_constructor_is_an_arrow_into_its_type() {
    let rs = analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixture-rs"), &Options::default()).unwrap();
    let py = fixture();
    let (rs_arrows, py_arrows) = (atlas_engine::flow::arrows(&rs), atlas_engine::flow::arrows(&py));
    let into = |a: &atlas_engine::flow::Arrow| (a.receiver.clone(), a.builds.clone());
    assert_eq!(into(&rs_arrows["handed.shapes.Shape.square"]), (String::new(), "handed.shapes.Shape".into()));
    assert_eq!(into(&py_arrows["fixture.files.Report.__init__"]), (String::new(), "fixture.files.Report".into()));
    assert_eq!(into(&py_arrows["fixture.files.Report.from_file"]), (String::new(), "fixture.files.Report".into()));
    assert_eq!(py.node("fixture.files.Report").unwrap().initializer(|id| py.node(id)).as_deref(), Some("fixture.files.Report.__init__"));
}
