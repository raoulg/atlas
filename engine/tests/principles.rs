use std::collections::BTreeMap;
use std::path::PathBuf;

use atlas_engine::principles::{check, Finding, PRINCIPLES};
use atlas_engine::{analyze, EdgeKind, Options};

fn findings(repo: &str) -> Vec<Finding> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(repo);
    check(&analyze(&root, &Options::default()).expect("parses"))
}

/// The names a finding's text holds: ids, types and paths, apart from the words and signs between them.
fn names(text: &str) -> Vec<&str> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.')).filter(|w| !w.is_empty()).collect()
}

fn by_principle(fs: &[Finding]) -> BTreeMap<&'static str, Vec<&str>> {
    let mut out: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for f in fs {
        out.entry(f.principle).or_default().push(f.node.as_str());
    }
    out
}

/// A finding ends in a question for the author, whatever its principle: read on the fixture, the
/// subject with cases of the most principles side by side.
#[test]
fn every_finding_ends_in_a_question() {
    let fs = findings("tests/fixture");
    assert!(!fs.is_empty());
    let unasked: Vec<String> = fs.iter().filter(|f| !f.question.ends_with('?')).map(|f| format!("{} {}", f.principle, f.node)).collect();
    assert!(unasked.is_empty(), "a finding ends in a question: {unasked:?}");
}

/// A prophecy is a todo, not a finding: the rules that ask about built code leave its bodiless
/// functions alone, and what it supersedes is still asked about, with the prophecy as the answer.
#[test]
fn a_prophecy_is_no_finding_and_answers_the_one_it_supersedes() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixture");
    let g = analyze(&root, &Options::default()).expect("parses");
    let fs = check(&g);
    assert!(
        fs.iter().all(|f| f.node != "fixture.plans.tidy"),
        "{:?}",
        fs.iter().filter(|f| f.node == "fixture.plans.tidy").collect::<Vec<_>>()
    );
    let said = |id: &str| g.node(id).unwrap().prophecy.clone().unwrap_or_else(|| panic!("{id} is a prophecy"));
    assert_eq!(said("fixture.plans.stash").replaces.as_deref(), Some("fixture.plans.keep"));
    let keep = fs.iter().find(|f| f.node == "fixture.plans.keep").expect("the function a prophecy supersedes is still asked about");
    assert!(keep.evidence.iter().any(|e| names(e).contains(&"stash")), "{:?}", keep.evidence);
    // `forgotten` supersedes a name the code does not hold; `report` calls the prophecy `tidy`
    let gone = said("fixture.plans.forgotten");
    assert!(gone.supersedes.as_deref() == Some("fixture.plans.gone") && gone.replaces.is_none() && gone.candidates.is_empty(), "{gone:?}");
    assert!(g.node("fixture.plans.gone").is_none());
    assert!(g.node("fixture.plans.tidy").unwrap().prophecy.is_some());
    assert!(g.edges.iter().any(|e| e.src == "fixture.plans.report" && e.dst == "fixture.plans.tidy" && e.kind == EdgeKind::Calls));
    assert_eq!(by_principle(&fs)["prophecy"], ["fixture.plans.forgotten", "fixture.plans.report"]);
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
    // `Ledger.from_row` is asked about the one override that breaks its promise, and `Box.fill` about its own
    let overrides: Vec<&str> =
        fs.iter().filter(|f| f.principle == "overrides" && f.file.ends_with("generic.py")).map(|f| f.node.as_str()).collect();
    assert_eq!(overrides, ["fixture.generic.Ledger.from_row", "fixture.generic.Box.fill"]);
    let breaking: Vec<&str> =
        g.nodes.iter().filter(|n| n.breaks.as_deref() == Some("fixture.generic.Ledger.from_row")).map(|n| n.id.as_str()).collect();
    assert_eq!(breaking, ["fixture.generic.CountLedger.from_row"]);
    let returns = |id: &str| g.node(id).unwrap().returns.as_ref().map(|r| r.written.clone());
    assert_eq!(returns("fixture.generic.Ledger.from_row").as_deref(), Some("Version[R]"));
    assert_eq!(returns("fixture.generic.CountLedger.from_row").as_deref(), Some("Version[int]"));
    let ledger = fs.iter().find(|f| f.node == "fixture.generic.Ledger.from_row").expect("Ledger.from_row is asked about");
    assert!(matches!(ledger.evidence.as_slice(), [e] if names(e).contains(&"CountLedger.from_row")), "{:?}", ledger.evidence);
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
    // its put is `Notes.put`, putting the Note into the file at `Notes.path`, and its get `Notes.get`, getting a dict from it
    let shown = |held: &[&str]| notes.evidence.iter().any(|e| held.iter().all(|n| names(e).contains(n)));
    assert_eq!(notes.evidence.len(), 2, "{:?}", notes.evidence);
    assert!(shown(&["Notes.put", "Note", "Notes.path"]) && shown(&["Notes.get", "dict", "Notes.path"]), "{:?}", notes.evidence);
}

/// The principles read Rust by what its types and entries mean: a trait's implementation has the
/// trait's signature, so `Disk.page`, which gives the `anyhow::Result<Page>` that `Source.page`
/// promises as `Result<Page>`, is asked nothing, and the test beside `ledger` names its own file
/// without making `ledger` one that may. `publish` hands `report(dir, ..)` the directory `site`,
/// and `stock` builds its `Store` on `stock`: each decides the place the function it calls writes.
#[test]
fn rust_is_read_by_meaning() {
    let fs = findings("tests/fixture-rs");
    let got = by_principle(&fs);
    assert!(!got.contains_key("overrides"), "{:?}", got.get("overrides"));
    assert_eq!(
        got["places"],
        ["handed.ledger.ledger", "handed.world.notes", "handed.world.publish", "handed.world.stamp", "handed.world.stock"]
    );
}

/// A command its package also exports is a door as well: `doors` asks of it as of any door, and
/// asks nothing of a command its package does not export. A graph of one function, a command
/// that does not say what its `n` is.
#[test]
fn a_command_its_package_exports_is_a_door() {
    use atlas_engine::{Entry, EntryKind, Graph, Node, NodeKind, Param};
    let graph = |exported: bool| Graph {
        root: "tool".into(),
        nodes: vec![Node {
            id: "tool.main".into(),
            kind: NodeKind::Function,
            name: "main".into(),
            module: "tool".into(),
            file: "tool.py".into(),
            entry: Some(Entry::new(EntryKind::Cli, "tool")),
            exported,
            params: vec![Param { name: "n".into(), ..Default::default() }],
            ..Default::default()
        }],
        ..Default::default()
    };
    let doors = |g: &Graph| check(g).into_iter().filter(|f| f.principle == "doors").collect::<Vec<_>>();
    let asked = doors(&graph(true));
    assert!(matches!(asked.as_slice(), [f] if f.node == "tool.main" && names(&f.what).contains(&"n")), "{asked:?}");
    assert!(doors(&graph(false)).is_empty());
}

/// `src/` holds packages; a package called `src` has no name of its own yet, and that comes first.
#[test]
fn a_package_called_src_is_asked_its_name() {
    let fs = findings("tests/srclayout");
    assert_eq!(by_principle(&fs)["doors"], ["src"]);
    // only the sentence tells the name asked apart from the root that exports nothing
    let asked = fs.iter().find(|f| f.principle == "doors").unwrap();
    assert!(asked.what.contains("called src"), "{}", asked.what);
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
