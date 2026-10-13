use std::collections::BTreeMap;
use std::path::PathBuf;

use atlas_engine::{analyze, Options};
use atlas_engine::principles::{check, Finding, PRINCIPLES};

fn findings(repo: &str) -> Vec<Finding> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(repo);
    check(&analyze(&root, &Options::default()).expect("parses"))
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
    // `Graph.add` and `Sink.write` are typed; `first` says nothing, nor does the `flush`
    // that the exported DuckSink inherits from its mixin
    assert_eq!(got["doors"], ["fixture.schema.Graph.first", "fixture.sinks.base.Mixin.flush"]);
    // `load` makes a Loaded, `cli.show` takes it from another module
    assert_eq!(got["sums"], ["fixture.build.Loaded"]);
    // `CountingSink.write` gives an int where `Sink.write` promises None; `clone` narrows to a
    // subclass. `MeanEncoder` keeps `...` with `batch dim`, `PatchEncoder` breaks `batch dim`
    // with three dims; `GraphReader` gives one variant of the `Loaded` that `Reader` promises.
    // `Archive` says what `Catalog` promises in other words (`List[Node]`, `Node | None`, an
    // `Annotated[str, ..]`) or narrower (a list for a `Sequence`), as ty decides; its `head`
    // gives one Node where a list was promised
    assert_eq!(got["overrides"], ["fixture.build.MeanEncoder.encode", "fixture.catalog.Catalog.head", "fixture.sinks.base.Sink.write"]);
    // `Path("out.json")` in a sink, and `summary` beside the command `export`, which names its own
    // file as an entry may; `save` writes where its caller says
    assert_eq!(got["places"], ["fixture.export.summary", "fixture.sinks.duck.DuckSink.write"]);
    // `enrich` changes the Graph it is handed, so do `rename` (`setattr`) and `forget` (`del`);
    // `Graph.add` changes itself, and `deeper` calls an `update` that changes nothing
    assert_eq!(got["changes"], ["fixture.build.enrich", "fixture.edits.rename", "fixture.edits.forget"]);
    // `Journal.connection` gives back a connection `entries` uses, without saying so; the
    // door `Graph.first` is asked about by `doors`
    assert_eq!(got["gives"], ["fixture.store.Journal.connection"]);
    // `prepare` tests `mode` twice and picks where its Graph comes from between other steps;
    // `fresh_or_empty` is nothing but that choice, so it has a name. `seeded` builds its Graph
    // with one constructor in both arms, between other steps; `seeded_by_name` leaves that
    // choice to `seed_graph`
    assert_eq!(got["choices"], ["fixture.build.prepare", "fixture.build.prepare", "fixture.build.seeded"]);
    // `remember` only writes a global and `Mixin.close` does nothing; `check_name` raises, `each`
    // calls the function it is handed, and `register_names` runs both
    assert_eq!(got["nothing"], ["fixture.build.remember", "fixture.sinks.base.Mixin.close"]);
    // `_rank.low.peek` calls back up into `_rank.high`, which builds on `low`
    assert_eq!(got["layers"], ["fixture._rank"]);
    // `Notes` puts a Note into its file and gets a dict back, `Shelf` gets the Note, and `Diary`
    // inherits the put of `Notes`, which is asked there; `Copier` puts a Graph into the graph
    // database and gets only a list of nodes
    assert_eq!(got["lens"], ["fixture.notes.Notes", "fixture.store.Copier"]);
    assert!(fs.iter().all(|f| f.question.ends_with('?')), "a finding ends in a question");
}

/// The principles read Rust by what its types and entries mean: `Disk.page` gives the
/// `anyhow::Result<Page>` that `Source.page` promises as `Result<Page>`, and the test beside
/// `ledger` names its own file without making `ledger` one that may.
#[test]
fn rust_is_read_by_meaning() {
    let fs = findings("tests/fixture-rs");
    let got = by_principle(&fs);
    assert!(!got.contains_key("overrides"), "{:?}", got.get("overrides"));
    assert_eq!(got["places"], ["handed.ledger.ledger", "handed.world.notes", "handed.world.stamp"]);
}

/// `src/` holds packages; a package called `src` has no name of its own yet, and that comes first.
#[test]
fn a_package_called_src_is_asked_its_name() {
    let fs = findings("tests/srclayout");
    assert_eq!(by_principle(&fs)["doors"], ["src"]);
    assert!(fs[0].what.contains("called src"), "{}", fs[0].what);
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
