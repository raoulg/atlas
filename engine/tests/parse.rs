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
    // typed parameters give receivers a type: `store.add_edge(..)` with `store: &mut Store`
    assert!(calls.contains(&("atlas-engine.python.collect_symbols".into(), "atlas-engine.store.Store.add_edge".into())));
    assert!(g.node("atlas-engine.rust.parse").unwrap().boundaries.get("disk_read").is_some(), "walkdir and fs::read");
    // the declared picture: parse -> lens -> layout, model underneath
    assert_eq!(g.roles.order, vec!["Parse", "Lens", "Layout"]);
    assert_eq!(g.node("atlas-engine.lens.Quotient").unwrap().role, "Lens");
    assert_eq!(g.node("atlas-engine.model.Node").unwrap().role, "Model");
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
