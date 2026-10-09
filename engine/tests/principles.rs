use std::collections::BTreeMap;
use std::path::PathBuf;

use atlas_engine::analyze;
use atlas_engine::principles::{check, Finding, PRINCIPLES};

fn findings(repo: &str) -> Vec<Finding> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(repo);
    check(&analyze(&root, None, None).expect("parses"))
}

fn by_principle(fs: &[Finding]) -> BTreeMap<&'static str, Vec<&str>> {
    let mut out: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for f in fs {
        out.entry(f.principle).or_default().push(f.node.as_str());
    }
    out
}

/// Each principle finds its one case in the fixture, and leaves the cases next to it alone.
#[test]
fn each_principle_finds_its_case() {
    let fs = findings("tests/fixture");
    let got = by_principle(&fs);
    // `Graph.add` and `Sink.write` are typed; `first` says nothing
    assert_eq!(got["doors"], ["fixture.schema.Graph.first"]);
    // `load` makes a Loaded, `cli.show` takes it from another module
    assert_eq!(got["sums"], ["fixture.build.Loaded"]);
    // `CountingSink.write` gives an int where `Sink.write` promises None; `clone` narrows to a subclass
    assert_eq!(got["overrides"], ["fixture.sinks.base.Sink.write"]);
    // `Path("out.json")` in a sink; `save` writes where its caller says
    assert_eq!(got["places"], ["fixture.sinks.duck.DuckSink.write"]);
    // `enrich` changes the Graph it is handed; `Graph.add` changes itself
    assert_eq!(got["changes"], ["fixture.build.enrich"]);
    assert!(fs.iter().all(|f| f.question.ends_with('?')), "a finding ends in a question");
}

/// Atlas is its own first subject: it follows the principles it checks.
#[test]
fn atlas_obeys_its_principles() {
    let fs = findings("..");
    assert!(fs.is_empty(), "{:#?}", fs.iter().map(|f| format!("{} {}:{} {}", f.principle, f.file, f.line, f.what)).collect::<Vec<_>>());
}

/// The docs explain every principle the engine checks.
#[test]
fn every_principle_is_explained() {
    let docs = std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../docs/principles.md")).expect("docs/principles.md");
    for p in PRINCIPLES {
        assert!(docs.contains(&format!("## {}", p.asks)), "docs/principles.md has a section `## {}`", p.asks);
        assert!(docs.contains(&format!("`{}`", p.id)), "docs/principles.md names the id `{}`", p.id);
    }
}
