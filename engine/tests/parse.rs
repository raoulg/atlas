use std::collections::HashSet;
use std::path::PathBuf;

use atlas_engine::lens::{by_role, hide_private, reach_from, Compose, Lens};
use atlas_engine::{analyze, EdgeKind, EntryKind, Graph, NodeKind};

fn fixture() -> Graph {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixture");
    analyze(&repo, None, None).expect("fixture parses")
}

fn edges(g: &Graph, kind: EdgeKind) -> HashSet<(String, String)> {
    g.edges.iter().filter(|e| e.kind == kind).map(|e| (e.src.clone(), e.dst.clone())).collect()
}

#[test]
fn modules_classes_and_functions_are_found() {
    let g = fixture();
    assert_eq!(g.root, "fixture");
    let kinds = |k: NodeKind| g.nodes.iter().filter(|n| n.kind == k).count();
    assert_eq!(kinds(NodeKind::Module), 7);
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
    let b = |id: &str| g.node(id).unwrap().boundaries.clone();
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

    let atlas = analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".."), None, None).unwrap();
    let arms = |dst: &str| atlas.edges.iter().find(|e| e.src == "atlas-engine.roles.resolve" && e.dst == dst).map(|e| e.arms.clone()).unwrap();
    assert_eq!(arms("atlas-engine.roles.load"), [(0, 0)], "match arm `Some(p)`");
    assert_eq!(arms("atlas-engine.roles.find"), [(0, 1), (1, 0)], "`None`, then the receiver of `unwrap_or_else`");
    assert_eq!(arms("atlas-engine.roles.infer"), [(0, 1), (1, 1)], "`None`, then its argument");
}

/// Where data rests is named as precisely as the code allows: a literal file, else its format;
/// a database statement reads or writes a table.
#[test]
fn files_and_tables_are_places() {
    let g = fixture();
    let t = |id: &str| g.node(id).unwrap().touches.iter().map(|t| (t.kind.as_str(), t.artifact.as_str(), t.dir.as_str())).collect::<HashSet<_>>();
    let write = t("fixture.sinks.duck.DuckSink.write");
    assert!(write.contains(&("db_duckdb", "nodes", "write")), "INSERT INTO nodes");
    assert!(write.contains(&("disk_write", "out.json", "")), "Path(\"out.json\").write_text");
    let save = t("fixture.build.save");
    assert!(save.contains(&("disk_write", "{name}.csv", "")), "to_csv with an f-string path");
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
    assert!(returns.contains(&("fixture.build.describe".into(), "fixture.schema.Node".into())));
}

#[test]
fn roles_are_inferred_when_nothing_is_declared() {
    let g = fixture();
    assert!(g.roles.inferred);
    assert!(g.roles.rules.iter().any(|(p, r)| p == "fixture.sinks" && r == "Sinks"));
    assert_eq!(g.node("fixture.sinks.duck.DuckSink").unwrap().role, "Sinks");
    assert_eq!(g.node("fixture.cli.build").unwrap().role, "Root");
}

/// A Rust workspace parses; the sample is this workspace, which also declares its own picture.
#[test]
fn rust_workspace_parses() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let g = analyze(&repo, None, None).expect("the Atlas workspace parses");
    let ids: HashSet<&str> = g.nodes.iter().map(|n| n.id.as_str()).collect();
    for id in ["atlas-engine", "atlas-engine.model", "atlas-engine.lens", "atlas-engine.python", "atlas-engine.rust", "atlas"] {
        assert!(ids.contains(id), "module {id}");
    }
    assert!(ids.contains("atlas-engine.model.Graph") && ids.contains("atlas-engine.lens.Lens"));
    assert_eq!(g.node("atlas.main").unwrap().entry.as_ref().unwrap().kind, EntryKind::Cli);
    assert_eq!(g.node("atlas-engine.analyze").unwrap().entry.as_ref().unwrap().kind, EntryKind::Export, "pub fn at the root of a library crate");
    assert!(g.node("atlas-engine.lens.hide_private").unwrap().entry.is_none(), "pub in a module is not the crate's root");
    let inh = edges(&g, EdgeKind::Inherits);
    assert!(inh.contains(&("atlas-engine.lens.Restrict".into(), "atlas-engine.lens.Lens".into())), "impl Lens for Restrict");
    let calls = edges(&g, EdgeKind::Calls);
    assert!(calls.contains(&("atlas.main".into(), "atlas.view".into())));
    assert!(calls.contains(&("atlas.view".into(), "atlas-engine.analyze".into())), "cli calls into the engine");
    assert!(calls.contains(&("atlas-engine.language.Rust.parse".into(), "atlas-engine.rust.parse".into())), "trait impl calls the module");
    let returns = edges(&g, EdgeKind::Returns);
    assert!(returns.contains(&("atlas-engine.analyze".into(), "atlas-engine.model.Graph".into())), "`pub use model::*` resolves Graph");
    // `let lang = language::detect(..)?; lang.parse(..)`: a local typed by the callee's return
    assert!(calls.contains(&("atlas-engine.analyze".into(), "atlas-engine.language.Language.parse".into())));
    // typed parameters give receivers a type: `store.add_edge(..)` with `store: &mut GraphBuilder`
    assert!(calls.contains(&("atlas-engine.python.collect_symbols".into(), "atlas-engine.builder.GraphBuilder.add_edge".into())));
    assert!(g.node("atlas-engine.rust.parse").unwrap().boundaries.get("disk_read").is_some(), "walkdir and fs::read");
    let arts = |id: &str| g.node(id).unwrap().touches.iter().map(|t| (t.kind.clone(), t.artifact.clone())).collect::<HashSet<_>>();
    assert!(arts("atlas-engine.entries.read_makefile").contains(&("disk_read".into(), "Makefile".into())));
    assert!(arts("atlas-engine.language.Python.detect").contains(&("disk_read".into(), "pyproject.toml".into())), "repo.join(..).exists()");
    assert!(g.node("atlas.view").unwrap().boundaries.contains_key("screen"), "eprintln!");
    let m = edges(&g, EdgeKind::Mutates);
    assert!(m.contains(&("atlas-engine.model.Graph.recount".into(), "atlas-engine.model.Graph".into())), "&mut self");
    // the declared picture: parse -> lens -> layout, model underneath
    assert_eq!(g.roles.order, vec!["Parse", "Lens", "Layout"]);
    assert_eq!(g.node("atlas-engine.lens.Quotient").unwrap().role, "Lens");
    assert_eq!(g.node("atlas-engine.model.Node").unwrap().role, "Model");
}

/// Atlas obeys its own algebra: the steps of `analyze` move data between the repo on disk, a
/// `Language`, the `Graph` and its `Architecture`, and nothing else. A new type between parse
/// and Graph is a box the algebra does not have, and the code changes, not the picture.
#[test]
fn atlas_reads_as_its_algebra() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let g = analyze(&repo, None, None).expect("the Atlas workspace parses");
    let classes: HashSet<&str> = g.nodes.iter().filter(|n| n.kind == NodeKind::Class).map(|n| n.id.as_str()).collect();
    let name = |id: &str| id.rsplit('.').next().unwrap().to_string();
    let steps: Vec<&str> = g.edges.iter().filter(|e| e.src == "atlas-engine.analyze" && e.kind == EdgeKind::Calls).map(|e| e.dst.as_str()).collect();
    let short: Vec<&str> = steps.iter().map(|s| s.trim_start_matches("atlas-engine.")).collect();
    assert_eq!(short, ["language.detect", "language.Language.parse", "entries.read_makefile", "roles.resolve", "roles.assign"], "analyze is its algebra, step by step");
    for step in steps {
        for e in g.edges.iter().filter(|e| e.src == step && matches!(e.kind, EdgeKind::Takes | EdgeKind::Returns | EdgeKind::Mutates)) {
            if classes.contains(e.dst.as_str()) {
                assert!(["Graph", "Language", "Architecture"].contains(&name(&e.dst).as_str()), "{step} moves a {}: the algebra has no such place", e.dst);
            }
        }
    }
    let returns = edges(&g, EdgeKind::Returns);
    assert!(returns.contains(&("atlas-engine.language.Language.parse".into(), "atlas-engine.model.Graph".into())), "parse: Disk(repo) -> Graph");
    let mutates = edges(&g, EdgeKind::Mutates);
    assert!(mutates.contains(&("atlas-engine.roles.assign".into(), "atlas-engine.model.Graph".into())), "assign: Graph -> Graph");
}

#[test]
fn lenses_restrict_and_quotient() {
    let g = fixture();
    let reach = reach_from(&g, "fixture.cli.build").apply(&g);
    let ids: HashSet<&str> = reach.nodes.iter().map(|n| n.id.as_str()).collect();
    assert!(ids.contains("fixture.build.Pipeline.run"));
    assert!(ids.contains("fixture.sinks.duck.DuckSink.write"));
    assert!(!ids.contains("fixture.cli.cmd_report"));

    let priv_free = hide_private().apply(&g);
    assert!(priv_free.node("fixture.sinks.duck.DuckSink.__init__").is_none());
    assert!(priv_free.node("fixture.sinks").is_some(), "packages stay");

    let q = Compose(vec![Box::new(hide_private()), Box::new(by_role())]).apply(&g);
    let names: HashSet<&str> = q.nodes.iter().map(|n| n.name.as_str()).collect();
    assert!(names.contains("Sinks") && names.contains("Root"));
    let root_to_sinks = q.edges.iter().find(|e| e.src == "role:Root" && e.dst == "role:Sinks" && e.kind == EdgeKind::Instantiates);
    assert!(root_to_sinks.unwrap().weight.unwrap() >= 1);
    assert!(q.edges.iter().all(|e| e.src != e.dst), "edges inside a class vanish");
}
