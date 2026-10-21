use std::collections::BTreeMap;
use std::path::PathBuf;

use atlas_engine::principles::{check, Finding, PRINCIPLES};
use atlas_engine::{analyze, EdgeKind, Options};

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
    // gives one Node where a list was promised. `CountLedger` fills `Ledger[R]` in with a Node
    // and gives a `Version[int]`; `Numbers` fills `Box[T]` in with an int and takes strings
    assert_eq!(
        got["overrides"],
        [
            "fixture.build.MeanEncoder.encode",
            "fixture.catalog.Catalog.head",
            "fixture.generic.Ledger.from_row",
            "fixture.generic.Box.fill",
            "fixture.sinks.base.Sink.write"
        ]
    );
    // `Path("out.json")` in a sink, `summary` beside the command `export`, which names its own
    // file as an entry may, and the functions of `files` that keep their data under the module's
    // constant `DATA`; `save` writes where its caller says
    assert_eq!(
        got["places"],
        [
            "fixture.export.summary",
            "fixture.files.kinds",
            "fixture.files.report",
            "fixture.files.read_report",
            "fixture.files.install",
            "fixture.files.export",
            "fixture.sinks.duck.DuckSink.write"
        ]
    );
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
    // `Mixin.close` does nothing; `remember` writes a global, a place (`Global(fixture.build.SEEN)`),
    // `check_name` raises, `each` calls the function it is handed, and `register_names` runs both
    assert_eq!(got["nothing"], ["fixture.plans.keep", "fixture.sinks.base.Mixin.close"]);
    // `_rank.low.peek` calls back up into `_rank.high`, which builds on `low`
    assert_eq!(got["layers"], ["fixture._rank"]);
    // `Notes` puts a Note into its file and gets a dict back, `Shelf` gets the Note, and `Diary`
    // inherits the put of `Notes`, which is asked there; `Copier` puts a Graph into the graph
    // database and gets only a list of nodes. `Register` gets its Stamp back inside an Entry, and
    // `Tiles` looks its tiles up by a Point it puts nowhere
    assert_eq!(got["lens"], ["fixture.notes.Notes", "fixture.store.Copier"]);
    // `forgotten` supersedes a function the code does not hold; `report` is built and calls
    // `tidy`, a prophecy. `tidy` itself, bodiless, is a todo and no finding
    assert_eq!(got["prophecy"], ["fixture.plans.forgotten", "fixture.plans.report"]);
    assert!(fs.iter().all(|f| f.question.ends_with('?')), "a finding ends in a question");
}

/// A prophecy is a todo, not a finding: the rules that ask about built code leave its bodiless
/// functions alone, and what it supersedes is still asked about, with the prophecy as the answer.
#[test]
fn a_prophecy_is_no_finding_and_answers_the_one_it_supersedes() {
    let fs = findings("tests/fixture");
    assert!(
        fs.iter().all(|f| f.node != "fixture.plans.tidy"),
        "{:?}",
        fs.iter().filter(|f| f.node == "fixture.plans.tidy").collect::<Vec<_>>()
    );
    let keep = fs.iter().find(|f| f.node == "fixture.plans.keep").expect("the function a prophecy supersedes is still asked about");
    assert!(keep.evidence.iter().any(|e| e.contains("answered by the prophecy `stash`")), "{:?}", keep.evidence);
    let unmarked = fs.iter().filter(|f| f.principle == "prophecy").map(|f| f.what.as_str()).collect::<Vec<_>>();
    assert!(unmarked[0].contains("`fixture.plans.gone`") && unmarked[1].contains("leans on `tidy`"), "{unmarked:?}");
}

/// An override of a generic base keeps the promise its subclass fills in: `NodeLedger(Ledger[Node])`
/// gives the `Version[Node]` that `Ledger[R]` promises as `Version[R]`, `Numbers(Box[int])` the
/// `list[int]` of `Box[T]`'s `list[T]` (`Generic[T]`), and `NodeHistory` the `Version[list[Node]]`
/// that `Ledger` promises through `History[S](Ledger[list[S]])`; each takes what its base offers,
/// filled in alike. `CountLedger` gives a `Version[int]` where its `Ledger[Node]` promises a
/// `Version[Node]`, and `Numbers.fill` takes `list[str]` where its `Box[int]` hands a `list[int]`.
#[test]
fn an_override_of_a_generic_base_keeps_what_its_subclass_fills_in() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixture");
    let g = analyze(&root, &Options::default()).expect("parses");
    let keeps = |id: &str| g.node(id).unwrap_or_else(|| panic!("{id} is in the graph")).keeps;
    for kept in ["NodeLedger.from_row", "NodeLedger.put", "NodeHistory.from_row", "NodeHistory.put", "CountLedger.put", "Numbers.take"] {
        assert_eq!(keeps(&format!("fixture.generic.{kept}")), Some(true), "{kept}");
    }
    for broken in ["CountLedger.from_row", "Numbers.fill"] {
        assert_eq!(keeps(&format!("fixture.generic.{broken}")), Some(false), "{broken}");
    }
    let fs = check(&g);
    let ledger = fs.iter().find(|f| f.node == "fixture.generic.Ledger.from_row").expect("Ledger.from_row is asked about");
    assert_eq!(ledger.what, "Ledger.from_row promises (row: tuple[str, int]) -> Version[R]; 1 override breaks it");
    assert_eq!(ledger.evidence, ["CountLedger.from_row(row: tuple[str, int]) -> Version[int] (src/fixture/generic.py:49)"]);
}

/// What a lens puts, it gets back, also inside what a get gives: a lens composed with the
/// projection to a field, an element or a variant is a lens on that part. `Register.check` puts an
/// Entry and the Stamp that checks it; `Register.entry` gives the Entry, whose `state` may be the
/// variant `Checked`, which holds the Stamp.
#[test]
fn a_lens_gets_back_what_it_puts_inside_what_it_gives() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixture");
    let g = analyze(&root, &Options::default()).expect("parses");
    let edge = |src: &str, dst: &str, kind: EdgeKind| g.edges.iter().any(|e| e.src == src && e.dst == dst && e.kind == kind);
    assert!(edge("fixture.notes.Register.check", "fixture.notes.Stamp", EdgeKind::Takes));
    assert!(edge("fixture.notes.Register.entry", "fixture.notes.Entry", EdgeKind::Returns));
    assert!(edge("fixture.notes.Entry", "fixture.notes.Checked", EdgeKind::Has));
    assert!(edge("fixture.notes.Checked", "fixture.notes.Stamp", EdgeKind::Has));
    let lens: Vec<Finding> = check(&g).into_iter().filter(|f| f.principle == "lens").collect();
    assert!(lens.iter().all(|f| f.node != "fixture.notes.Register"), "{:#?}", lens);
}

/// What a lens puts is what reaches the place. `Tiles.tile` hands its Point to `find`, which only
/// reads, and to `Area.around`, whose Area it hands to `fetch`: the Point is a key the tile is
/// looked up by, and the Area is `fetch`'s to put, asked about there and got back as the tile's
/// `area`. A put whose own call into the place may be handed anything still puts every parameter:
/// `Notes.put` writes its file itself, and is asked.
#[test]
fn a_lens_puts_what_reaches_the_place_not_the_key_it_looks_up_by() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixture");
    let g = analyze(&root, &Options::default()).expect("parses");
    let edge = |src: &str, dst: &str, kind: EdgeKind| g.edges.iter().any(|e| e.src == src && e.dst == dst && e.kind == kind);
    assert!(edge("fixture.tiles.Tiles.tile", "fixture.tiles.Point", EdgeKind::Takes));
    assert!(edge("fixture.tiles.Tiles.tile", "fixture.tiles.Tiles.fetch", EdgeKind::Calls), "`tile` reaches the write");
    assert!(edge("fixture.tiles.Tile", "fixture.tiles.Area", EdgeKind::Has));
    let lens: Vec<Finding> = check(&g).into_iter().filter(|f| f.principle == "lens").collect();
    assert!(lens.iter().all(|f| f.node != "fixture.tiles.Tiles"), "{:#?}", lens);
    let notes = lens.iter().find(|f| f.node == "fixture.notes.Notes").expect("Notes is asked");
    assert_eq!(notes.evidence, ["put Notes.put: Note × Disk(Notes.path) → Disk(Notes.path)", "get Notes.get: Disk(Notes.path) → dict"]);
}

/// The principles read Rust by what its types and entries mean: a trait's implementation has the
/// trait's signature, so `Disk.page`, which gives the `anyhow::Result<Page>` that `Source.page`
/// promises as `Result<Page>`, is asked nothing, and the test beside `ledger` names its own file
/// without making `ledger` one that may.
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

/// What `atlas check` asks of Atlas, each finding read: its principle, the node it is about, and
/// why it stands (a fault of Atlas's code, tracked in an issue, or a fault of the rule).
const REVIEWED: &[(&str, &str, &str)] = &[];

/// Atlas is its own first subject, and `atlas check` is a signal on it as on any code: never an
/// oracle, never a target. What it asks of Atlas is the reviewed list above; the test fails when
/// the findings change, so that each new one is read and each one gone is seen to go, not so that
/// the list stays short.
#[test]
fn atlas_findings_are_the_reviewed_ones() {
    let fs = findings("..");
    let mut got: Vec<(&str, &str)> = fs.iter().map(|f| (f.principle, f.node.as_str())).collect();
    let mut want: Vec<(&str, &str)> = REVIEWED.iter().map(|(p, n, _)| (*p, *n)).collect();
    got.sort();
    want.sort();
    let new: Vec<String> = fs
        .iter()
        .filter(|f| !want.contains(&(f.principle, f.node.as_str())))
        .map(|f| format!("{} {} ({}:{}) {}", f.principle, f.node, f.file, f.line, f.what))
        .collect();
    let gone: Vec<&(&str, &str)> = want.iter().filter(|w| !got.contains(w)).collect();
    assert!(
        got == want,
        "Atlas's findings are not the reviewed ones; read each and say in REVIEWED why it stands\nnew: {new:#?}\ngone: {gone:#?}"
    );
}

/// The docs explain every principle the engine checks.
#[test]
fn every_principle_is_explained() {
    let docs = std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../docs/reference/principles.md"))
        .expect("docs/reference/principles.md");
    for p in PRINCIPLES {
        assert!(docs.contains(&format!("## {}", p.asks)), "docs/reference/principles.md has a section `## {}`", p.asks);
        assert!(docs.contains(&format!("`{}`", p.id)), "docs/reference/principles.md names the id `{}`", p.id);
    }
}
