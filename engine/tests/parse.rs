use std::collections::HashSet;
use std::path::PathBuf;

use atlas_engine::view::{by_module, hide_private, reach_from, Compose, View};
use atlas_engine::{analyze, EdgeKind, EntryKind, Graph, NodeKind, Options};

fn fixture() -> Graph {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixture");
    analyze(&repo, &Options::default()).expect("fixture parses")
}

/// How many calls of a node cross to each kind of place, by the names the graph's JSON gives them.
fn boundaries(n: &atlas_engine::Node) -> std::collections::BTreeMap<&'static str, u32> {
    let mut out = std::collections::BTreeMap::new();
    for t in &n.touches {
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
    assert_eq!(kinds(NodeKind::Module), 13, "the package's ten, a root script and two notebooks");
    assert!(g.node("fixture.sinks").unwrap().is_pkg);
    assert_eq!(g.node("fixture.schema.Graph").unwrap().doc, "Nodes and edges.");
    assert_eq!(g.node("fixture.sinks.duck.DuckSink").unwrap().bases, vec!["Sink", "Mixin"]);
    assert_eq!(g.node("fixture.build.build_graph").unwrap().returns, "Graph");
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
    let order: Vec<&str> = g.edges.iter().filter(|e| e.src == "fixture.cli.build" && e.kind == EdgeKind::Instantiates).map(|e| e.dst.as_str()).collect();
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
    assert!(call("store.keep", "store.Store.write") && call("store.keep", "store.Reader.nodes"), "`store, reader = stores()` with `-> tuple[Store, Reader]`");
    assert!(call("store.latest", "store.reader") && call("store.latest", "store.Reader.nodes"), "`current = reader`: a function under another name");
    assert!(call("store.firsts", "schema.Graph.first"), "`[g.first() for g in graphs]` with `graphs: list[Graph]`");
    assert!(call("store.merged", "schema.Graph.first"), "`for g in graphs`");
    assert!(call("build.reread", "store.Reader.nodes"), "`reader()` is defined in a module read after this one");
    assert!(edges(&g, EdgeKind::Has).contains(&("fixture.store.Store".into(), "fixture.store.Connection".into())), "the class `or` makes");
    let b = |id: &str| boundaries(g.node(id).unwrap());
    assert_eq!(b("fixture.store.pull").get("net"), Some(&1), "`SESSION = _session()` with `-> requests.Session`, at module level");
    assert_eq!(b("fixture.store.unpack").get("disk_write"), Some(&1), "`with zipfile.ZipFile(..) as zf: zf.extractall(..)`");
    assert!(b("fixture.store.Connection.driver").contains_key("db_graph"), "`GraphDatabase.driver(..)`");
    let inst = edges(&g, EdgeKind::Instantiates);
    assert!(!inst.contains(&("fixture.schema.Graph.add".into(), "fixture.schema.Node".into())), "`self.nodes.append(..)` on a `list[Node]` builds no Node");
}

/// Which way a graph database's data goes is what the Python code says to its driver: a managed
/// transaction, a routing, an access mode. What runs inside goes the same way; the query's text
/// is not read.
#[test]
fn a_driver_says_which_way_the_data_goes() {
    let g = fixture();
    let dirs = |id: &str| g.node(id).unwrap().touches.iter().filter(|t| t.kind_name() == "db_graph").map(|t| (t.op.as_str(), t.dir_name())).collect::<HashSet<_>>();
    let set = |xs: &[(&'static str, &'static str)]| xs.iter().copied().collect::<HashSet<_>>();
    assert_eq!(dirs("fixture.store.Connection.read"), set(&[("session", "read"), ("execute_read", "read"), ("run", "read")]), "`session.execute_read(lambda tx: tx.run(..))`");
    assert_eq!(dirs("fixture.store.Connection.write"), set(&[("session", "write"), ("execute_write", "write")]));
    assert_eq!(dirs("fixture.store._write"), set(&[("run", "write")]), "the work `execute_write` is handed");
    assert_eq!(dirs("fixture.store.Connection.count"), set(&[("execute_query", "read")]), "`routing_=RoutingControl.READ`");
    let peek = g.node("fixture.store.Connection.peek").unwrap().touches.iter().find(|t| t.op == "run").unwrap();
    assert_eq!((peek.dir_name(), peek.artifact.as_str()), ("", ""), "`session.run(\"MATCH (n:Graph) ..\")` says nothing in Python");
}

/// SQL is read by its grammar: a statement writes the table it changes and reads the ones it
/// takes from; a setting names no table, and a table named at run time is not named.
#[test]
fn sql_names_the_tables_it_reads_and_writes() {
    let g = fixture();
    let sides = g.node("fixture.store.Ledger.keep").unwrap().touches.iter().filter(|t| t.kind_name() == "db_sqlite").map(|t| (t.op.as_str(), t.artifact.as_str(), t.dir_name())).collect::<HashSet<_>>();
    let expected: HashSet<_> = [("execute", "", ""), ("execute", "log", "write"), ("execute", "nodes", "read"), ("execute", "", "write")].into_iter().collect();
    assert_eq!(sides, expected, "PRAGMA; CREATE TABLE IF NOT EXISTS log; INSERT INTO log SELECT .. FROM nodes; DELETE FROM {{table}}");
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
    assert!(edges(&g, EdgeKind::Calls).contains(&("notebooks.explore".into(), "fixture.build.build_graph".into())), "a cell calls into the package");
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
    let t = |id: &str| g.node(id).unwrap().touches.iter().map(|t| (t.kind_name(), t.artifact.as_str(), t.dir_name(), t.what.as_str())).collect::<Vec<_>>();
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
    let cmp = &g.node("fixture.build.prepare").unwrap().compares;
    assert_eq!(cmp.iter().map(|c| (c.subject.as_str(), c.choice)).collect::<Vec<_>>(), [("mode", Some(0)), ("mode", Some(1))]);

    let rs = analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixture-rs"), &Options::default()).unwrap();
    let arms = |dst: &str| rs.edges.iter().find(|e| e.src == "handed.choose.resolve" && e.dst == dst).map(|e| e.arms.clone()).unwrap();
    assert_eq!(arms("handed.choose.load"), [(0, 0)], "match arm `Some(p)`");
    assert_eq!(arms("handed.choose.find"), [(0, 1), (1, 0)], "`None`, then the receiver of `unwrap_or_else`");
    assert_eq!(arms("handed.choose.infer"), [(0, 1), (1, 1)], "`None`, then its argument");
}

/// Where data rests is named as precisely as the code allows: a literal file, else its format;
/// a database statement reads or writes a table.
#[test]
fn files_and_tables_are_places() {
    let g = fixture();
    let t = |id: &str| g.node(id).unwrap().touches.iter().map(|t| (t.kind_name(), t.artifact.as_str(), t.dir_name())).collect::<HashSet<_>>();
    let write = t("fixture.sinks.duck.DuckSink.write");
    assert!(write.contains(&("db_duckdb", "nodes", "write")), "INSERT INTO nodes");
    assert!(write.contains(&("disk_write", "out.json", "")), "Path(\"out.json\").write_text");
    let save = t("fixture.build.save");
    assert!(save.contains(&("disk_write", "*.csv", "")), "to_csv(f\"{{name}}.csv\"): a name made of a value is known only by its format");
    let fixed = |id: &str| g.node(id).unwrap().touches.iter().filter(|t| t.fixed).map(|t| t.op.as_str()).collect::<Vec<_>>();
    assert_eq!(fixed("fixture.sinks.duck.DuckSink.write"), ["write_text"], "Path(\"out.json\") is decided by the code alone");
    assert!(fixed("fixture.build.save").is_empty(), "f\"{{name}}.csv\", path and name come from the caller");
    assert!(save.contains(&("disk_write", "*.parquet", "")), "to_parquet(path): only the format is known");
    assert!(save.contains(&("disk_write", "", "")), "open(name, \"w\") writes");
    assert!(save.contains(&("screen", "", "")), "print");
    assert!(t("fixture.build.load").contains(&("disk_read", "*.csv", "")), "pd.read_csv named in a module-level table");
    let returns = edges(&g, EdgeKind::Returns);
    assert!(returns.contains(&("fixture.build.load".into(), "fixture.build.Loaded".into())), "Loaded = Union[Graph, Node] is a type of its own");
    let variants = edges(&g, EdgeKind::Variant);
    assert!(variants.contains(&("fixture.build.Loaded".into(), "fixture.schema.Graph".into())), "a sum and the types it may be");
    assert!(variants.contains(&("fixture.build.Loaded".into(), "fixture.schema.Node".into())));
    let takes = edges(&g, EdgeKind::Takes);
    assert!(takes.contains(&("fixture.build.register".into(), "fixture.sinks.base.Sink".into())), "Type[SinkT], SinkT = TypeVar(bound=\"Sink\")");
    assert!(takes.contains(&("fixture.build.describe".into(), "fixture.schema.Graph".into())), "a quoted annotation is a forward reference");
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
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let g = analyze(&repo, &Options::default()).expect("the Atlas workspace parses");
    let ids: HashSet<&str> = g.nodes.iter().map(|n| n.id.as_str()).collect();
    for id in ["atlas-engine", "atlas-engine.model", "atlas-engine.view", "atlas-engine.python", "atlas-engine.rust", "atlas"] {
        assert!(ids.contains(id), "module {id}");
    }
    assert!(ids.contains("atlas-engine.model.Graph") && ids.contains("atlas-engine.view.View"));
    assert_eq!(g.node("atlas.main").unwrap().entry.as_ref().unwrap().kind, EntryKind::Cli);
    assert_eq!(g.node("atlas-engine.analyze").unwrap().entry.as_ref().unwrap().kind, EntryKind::Export, "pub fn at the root of a library crate");
    let hide = g.node("atlas-engine.view.hide_private").unwrap().entry.as_ref().unwrap();
    assert_eq!((hide.kind, hide.label.as_str()), (EntryKind::Export, "view::hide_private"), "pub in a `pub mod`, by its path");
    let inh = edges(&g, EdgeKind::Inherits);
    assert!(inh.contains(&("atlas-engine.view.Fiber".into(), "atlas-engine.view.View".into())), "impl View for Fiber");
    let calls = edges(&g, EdgeKind::Calls);
    assert!(calls.contains(&("atlas.main".into(), "atlas.view".into())));
    assert!(calls.contains(&("atlas.view".into(), "atlas-engine.parts.analyze_repo".into())), "cli calls into the engine");
    assert!(calls.contains(&("atlas-engine.parts.analyze_parts".into(), "atlas-engine.analyze".into())), "each part is analyzed in its language");
    assert!(calls.contains(&("atlas-engine.language.Rust.parse".into(), "atlas-engine.rust.parse".into())), "trait impl calls the module");
    let returns = edges(&g, EdgeKind::Returns);
    assert!(returns.contains(&("atlas-engine.analyze".into(), "atlas-engine.model.Graph".into())), "`pub use model::*` resolves Graph");
    // `let lang = language::detect(..)?; lang.parse(..)`: a local typed by the callee's return
    assert!(calls.contains(&("atlas-engine.analyze".into(), "atlas-engine.language.Language.parse".into())));
    // typed parameters give receivers a type: `store.add_edge(..)` with `store: &mut GraphBuilder`
    assert!(calls.contains(&("atlas-engine.python.collect_symbols".into(), "atlas-engine.builder.GraphBuilder.add_edge".into())));
    assert!(boundaries(g.node("atlas-engine.rust.parse").unwrap()).get("disk_read").is_some(), "walkdir and fs::read");
    let arts = |id: &str| g.node(id).unwrap().touches.iter().map(|t| (t.kind_name().to_string(), t.artifact.clone())).collect::<HashSet<_>>();
    assert!(arts("atlas-engine.entries.read_makefile").contains(&("disk_read".into(), "Makefile".into())));
    assert!(arts("atlas-engine.language.Python.detect").contains(&("disk_read".into(), "pyproject.toml".into())), "repo.join(..).exists()");
    assert!(boundaries(g.node("atlas.view").unwrap()).contains_key("screen"), "eprintln!");
    let m = edges(&g, EdgeKind::Mutates);
    assert!(m.contains(&("atlas-engine.model.Graph.recount".into(), "atlas-engine.model.Graph".into())), "&mut self");
}

/// Atlas obeys its own algebra: the steps of `analyze` move data between the repo on disk, the
/// `Options` of the reading, a `Language` and the `Graph`, and nothing else.
/// A new type between parse and Graph is a box the algebra does not have, and the code changes,
/// not the picture.
#[test]
fn atlas_reads_as_its_algebra() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let g = analyze(&repo, &Options::default()).expect("the Atlas workspace parses");
    let classes: HashSet<&str> = g.nodes.iter().filter(|n| n.kind == NodeKind::Class).map(|n| n.id.as_str()).collect();
    let name = |id: &str| id.rsplit('.').next().unwrap().to_string();
    let steps: Vec<&str> = g.edges.iter().filter(|e| e.src == "atlas-engine.analyze" && e.kind == EdgeKind::Calls).map(|e| e.dst.as_str()).collect();
    let short: Vec<&str> = steps.iter().map(|s| s.trim_start_matches("atlas-engine.")).collect();
    assert_eq!(short, ["language.detect", "language.Language.parse", "entries.read_makefile", "parts.assign"], "analyze is its algebra, step by step");
    for step in steps {
        for e in g.edges.iter().filter(|e| e.src == step && matches!(e.kind, EdgeKind::Takes | EdgeKind::Returns | EdgeKind::Mutates)) {
            if classes.contains(e.dst.as_str()) {
                assert!(["Graph", "Language", "Options"].contains(&name(&e.dst).as_str()), "{step} moves a {}: the algebra has no such place", e.dst);
            }
        }
    }
    let returns = edges(&g, EdgeKind::Returns);
    assert!(returns.contains(&("atlas-engine.language.Language.parse".into(), "atlas-engine.model.Graph".into())), "parse: Disk(repo) × Options -> Graph");
    let takes = edges(&g, EdgeKind::Takes);
    assert!(takes.contains(&("atlas-engine.language.Language.parse".into(), "atlas-engine.Options".into())), "what a reading depends on is its argument, not a global");
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
    assert!(priv_free.node("fixture.sinks.duck.DuckSink.__init__").is_none());
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
    let kinds = |id: &str| g.node(id).unwrap().touches.iter().map(|t| t.kind_name()).collect::<HashSet<_>>();
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
    assert_eq!(g.node("ext:requests.Session").map(|n| n.door.map_or("", |d| d.as_str())), Some("net"));
    assert_eq!(g.node("ext:neo4j.Driver").map(|n| n.door.map_or("", |d| d.as_str())), Some("db_graph"));
    assert_eq!(g.node("ext:pandas.DataFrame").map(|n| n.door.map_or("", |d| d.as_str())), Some(""));
}

/// Two routes meet on the disk where one writes the place the other reads. A path is evaluated
/// from the code: a constant (`DATA / "raw"`), what a parameter is handed (`unpack(.., out)`),
/// an entry's declared default (`--out`). An unknown part of a path is a directory: what is
/// made in `data/raw/{kind}/{archive}` and what lists `data/raw` meet at `data/raw/`; a file
/// named under an unknown directory is that name under any (`data/*/report.json`).
#[test]
fn routes_meet_on_the_disk() {
    let g = fixture();
    let places = |id: &str| g.node(id).unwrap().touches.iter().map(|t| (t.kind_name(), t.artifact.as_str())).collect::<HashSet<_>>();
    assert!(places("fixture.files.unpack").contains(&("disk_write", "data/raw/")), "{:?}", places("fixture.files.unpack"));
    assert!(places("fixture.files.kinds").contains(&("disk_read", "data/raw/")), "{:?}", places("fixture.files.kinds"));
    assert!(places("fixture.files.report").contains(&("disk_write", "data/*/report.json")), "{:?}", places("fixture.files.report"));
    assert!(places("fixture.files.read_report").contains(&("disk_read", "data/*/report.json")), "{:?}", places("fixture.files.read_report"));
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
    assert_eq!(entry("handed.doubled"), Some((EntryKind::Export, "doubled".into())), "the lib keeps its exports beside a bin");
    assert_eq!(entry("handed.shapes.Shape"), Some((EntryKind::Export, "Shape".into())), "`pub use shapes::Shape` from a private module");
    assert_eq!(entry("handed.world.fetch"), Some((EntryKind::Export, "world::fetch".into())), "`pub mod world`");
    assert!(edges(&g, EdgeKind::Returns).contains(&("handed.shapes.Shape.square".into(), "handed.shapes.Shape".into())), "`-> Self`");
    assert!(!edges(&g, EdgeKind::Instantiates).iter().any(|(s, _)| s == "handed.describe"), "`item.to_string()` builds no Item");
    let variants = edges(&g, EdgeKind::Variant);
    assert!(variants.contains(&("handed.world.Line".into(), "handed.world.Page".into())) && variants.contains(&("handed.world.Line".into(), "handed.world.Log".into())), "{variants:?}");
    assert!(!edges(&g, EdgeKind::Has).iter().any(|(s, _)| s == "handed.world.Line"), "an enum is one of its variants, not all of them");
    let weight = &g.node("handed.world.weight").unwrap().compares;
    assert_eq!((weight[0].subject.as_str(), weight[0].cases.clone()), ("kind", vec!["page".to_string(), "log".to_string()]), "`matches!(kind, \"page\" | \"log\")`");
    let t = |id: &str| g.node(id).unwrap().touches.iter().map(|t| (t.kind_name().to_string(), t.op.clone(), t.artifact.clone())).collect::<Vec<_>>();
    assert!(t("handed.world.client").is_empty(), "building a client moves nothing: {:?}", t("handed.world.client"));
    assert_eq!(t("handed.world.fetch"), [("net".into(), "get".into(), String::new())], "`client.get(url).send()` on a `Client::new()`");
    assert!(t("handed.world.record").contains(&("db_sqlite".into(), "execute".into(), "log".into())), "{:?}", t("handed.world.record"));
    assert_eq!(t("handed.world.notes"), [("disk_read".into(), "open".into(), "notes.txt".into())], "the handle reads where it was opened");
    assert_eq!(t("handed.world.Log.append"), [("disk_write".into(), "open".into(), String::new())], "`OpenOptions::new().append(true)` writes");
    assert_eq!(t("handed.world.clean"), [("process".into(), "new".into(), String::new())], "one program run");
    assert!(t("handed.world.quit").is_empty(), "`process::exit` runs no program");
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
    let t = |id: &str| g.node(id).unwrap().touches.iter().map(|t| (t.kind_name().to_string(), t.op.clone(), t.artifact.clone(), t.fixed)).collect::<Vec<_>>();
    let keep = t("fixture.files.keep");
    assert!(keep.iter().all(|(_, _, a, fixed)| !fixed && a != "w"), "`cache_file.open(\"w\")`: the path is the parameter: {keep:?}");
    assert!(keep.iter().any(|(k, op, _, _)| k == "disk_write" && op == "dump"), "`json.dump(.., f)` writes the file `f` was opened at: {keep:?}");
    let named: Vec<&str> = g.node("fixture.files.keep").unwrap().touches.iter().filter(|t| t.op == "open").map(|t| t.what.as_str()).collect();
    assert_eq!(named, ["cache_file"], "the file is the path it is opened at, not its encoding");
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
    let n = |id: &str, kind: &str| g.node(id).unwrap().touches.iter().filter(|t| t.kind_name() == kind).map(|t| t.op.clone()).collect::<Vec<_>>();
    assert_eq!(n("fixture.clients.content_type", "net"), ["get"], "`requests.get(url)`, and not `headers.get`");
    assert_eq!(n("fixture.clients.ask", "llm"), ["create"], "`messages.create(..)`, and not `Anthropic()`");
}

/// A class is a door to a place when one of its operations reaches a database, the web, a model
/// or a program: a sink, a connection. A value that reads itself from a file (an alternative
/// constructor) or shows itself on the screen is data.
#[test]
fn a_door_reaches_a_service() {
    let g = fixture();
    let door = |id: &str| g.node(id).unwrap().door.map_or("", |d| d.as_str());
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
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let g = analyze(&repo, &Options::default()).expect("the Atlas workspace parses");
    let analyze_fn = g.nodes.iter().find(|n| n.id == "atlas-engine.analyze").unwrap();
    let step = |to: &str| analyze_fn.sites.iter().find(|s| s.to == to).unwrap_or_else(|| panic!("{to} in {:#?}", analyze_fn.sites));
    let held = |ty: &str, name: &str| atlas_engine::Held::new(ty, name);
    assert_eq!(step("atlas-engine.language.Language.parse").gives, [held("atlas-engine.model.Graph", "g")], "a method gives what it returns, not its receiver's type");
    let detect = step("atlas-engine.language.detect");
    assert_eq!(detect.hands, [held("atlas-engine.Options", "opts")], "the repository is a path, not a value of the package");
    let order: Vec<&str> = analyze_fn.sites.iter().map(|s| s.to.as_str()).collect();
    assert!(order.iter().position(|t| *t == "atlas-engine.entries.read_makefile") < order.iter().position(|t| *t == "atlas-engine.parts.assign"), "{order:?}");
}

/// `parse` is a function: the same files read with the same options give the same graph, to the
/// order of its edges. Each `HashMap` and `HashSet` seeds its order afresh, so a reading that
/// iterates one into the graph differs from run to run.
#[test]
fn parse_is_a_function() {
    let json = |repo: &str| serde_json::to_string(&analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(repo), &Options::default()).unwrap()).unwrap();
    for repo in ["tests/fixture", "tests/fixture-rs", "tests/fixture-ts", ".."] {
        let first = json(repo);
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
    let steps = &g.node("fixture.cli.build").unwrap().sites;
    let to: Vec<&str> = steps.iter().map(|s| s.to.as_str()).collect();
    assert_eq!(to, ["fixture.sinks.duck.DuckSink", "fixture.build.Pipeline", "fixture.build.Pipeline.run"]);
    let sink = &steps[0].gives[0];
    assert!(sink.name.starts_with('#') && sink.ty == "fixture.sinks.duck.DuckSink", "{sink:?}");
    assert!(steps[1].hands.contains(sink), "Pipeline is handed the sink: {:?}", steps[1].hands);
    assert!(steps[2].hands.contains(&steps[1].gives[0]), "run is called on what Pipeline gives");

    // `let lang = language::detect(repo, opts).ok_or_else(..)?`: the unwrapping is an identity
    let atlas = analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".."), &Options::default()).unwrap();
    let analyze_steps: Vec<&str> = atlas.node("atlas-engine.analyze").unwrap().sites.iter().map(|s| if s.to.is_empty() { s.op.as_str() } else { s.to.as_str() }).collect();
    assert_eq!(analyze_steps, ["atlas-engine.language.detect", "atlas-engine.language.Language.parse", "atlas-engine.entries.read_makefile", "atlas-engine.parts.assign"]);
}

/// What a step runs is the same for the agent and the page: the engine's bodies. A method called
/// on a value whose class is known runs that class's method; called where the class is not known,
/// it runs any of the overrides.
#[test]
fn a_body_is_what_a_route_runs() {
    let g = fixture();
    let bodies = atlas_engine::flow::bodies(&g);
    let run: Vec<String> = bodies["fixture.build.Pipeline.run"].steps().iter().map(|s| s.to.clone()).collect();
    assert!(run.contains(&"fixture.sinks.duck.DuckSink.write".to_string()), "`self.sink.write(..)` on the sink the pipeline holds: {run:?}");

    let atlas = analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".."), &Options::default()).unwrap();
    let bodies = atlas_engine::flow::bodies(&atlas);
    let mut alts = bodies["atlas-engine.language.Language.parse"].alts.clone();
    alts.sort();
    assert_eq!(alts, ["atlas-engine.language.Python.parse", "atlas-engine.language.Rust.parse", "atlas-engine.language.TypeScript.parse"]);
    let route = atlas_engine::story::route(&atlas, "atlas-engine.analyze", 2).unwrap();
    assert!(route.contains("2  Language.parse(repo, opts) → Graph   ·   one of 3"), "the agent reads the same alternatives: {route}");
}
