//! What Atlas asks of its own code. Atlas is its own first subject (CLAUDE.md), and these are reviews of it, not laws:
//! Atlas's own design rules (the repository is read in one place, a route reads as its algebra), seen through Atlas's
//! reading of itself, and the findings `atlas check` asks of Atlas, each read. A failure here has two causes that one
//! assertion cannot tell apart, Atlas's code and a reader, so it is read before anything changes: never a reason to bend
//! Atlas's code to keep it quiet, nor to drop a rule it holds. The laws are elsewhere, on subjects made for them.

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::sync::OnceLock;

use atlas_engine::principles::check;
use atlas_engine::{analyze, EdgeKind, EntryKind, Graph, Options};

/// The Atlas workspace, read once for the reviews that read it.
fn atlas() -> &'static Graph {
    static G: OnceLock<Graph> = OnceLock::new();
    G.get_or_init(|| {
        analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".."), &Options::default()).expect("the Atlas workspace parses")
    })
}

/// How many calls of a node cross to each kind of place, by the names the graph's JSON gives them.
fn boundaries(n: &atlas_engine::Node) -> BTreeMap<&'static str, u32> {
    let mut out = BTreeMap::new();
    for t in &n.touches {
        *out.entry(t.kind_name()).or_insert(0) += t.n;
    }
    out
}

fn edges(g: &Graph, kind: EdgeKind) -> HashSet<(String, String)> {
    g.edges.iter().filter(|e| e.kind == kind).map(|e| (e.src.clone(), e.dst.clone())).collect()
}

/// The call positions from one function to the first occurrence of another, through its bodies.
fn first_call_path(g: &Graph, from: &str, target: &str) -> Option<Vec<usize>> {
    fn visit(g: &Graph, id: &str, target: &str, active: &mut HashSet<String>, path: &mut Vec<usize>) -> Option<Vec<usize>> {
        if !active.insert(id.to_string()) {
            return None;
        }
        if let Some(node) = g.node(id) {
            for (at, site) in node.sites.iter().enumerate() {
                path.push(at);
                let found = if site.to == target { Some(path.clone()) } else { visit(g, &site.to, target, active, path) };
                if found.is_some() {
                    return found;
                }
                path.pop();
            }
        }
        active.remove(id);
        None
    }
    visit(g, from, target, &mut HashSet::new(), &mut Vec::new())
}

/// The repository is read in one place, and a reader reads what was read (`engine/src/repo.rs`, CLAUDE.md).
#[test]
fn atlas_reads_its_repository_in_one_place() {
    let g = atlas();
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
}

/// `analyze` reads a repository before parsing it; the parse returns a graph and takes its
/// options as an argument. Functions that arrange the reading contribute their own calls.
#[test]
fn atlas_reads_as_its_algebra() {
    let g = atlas();
    let read = first_call_path(g, "atlas-engine.analyze", "atlas-engine.repo.Repo.read").expect("the route reads a repository");
    let parse =
        first_call_path(g, "atlas-engine.analyze", "atlas-engine.language.Language.parse").expect("the route parses the repository");
    assert!(read < parse, "the repository is read before it is parsed: {read:?}, {parse:?}");
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

/// The view reads the repository and computes findings and flow before making and writing its
/// page. An opened branch may contain more calls without changing those dependencies.
#[test]
fn atlas_view_reads_as_its_algebra() {
    let g = atlas();
    let call = |target| first_call_path(g, "atlas-code.view", target).unwrap_or_else(|| panic!("atlas-code.view reaches {target}"));
    let read = call("atlas-engine.repo.Repo.read");
    let parse = call("atlas-engine.language.Language.parse");
    let findings = call("atlas-engine.principles.check");
    let flow = call("atlas-engine.flow.of");
    let page = call("atlas-code.page.Html.of");
    let write = call("atlas-code.page.Html.write");
    assert!(read < parse, "read before parse: {read:?}, {parse:?}");
    assert!(parse < findings && parse < flow, "graph before its findings and flow: {parse:?}, {findings:?}, {flow:?}");
    assert!(findings < page && flow < page && page < write, "the page is made from both before it is written");
    let returns = edges(g, EdgeKind::Returns);
    assert!(returns.contains(&("atlas-code.page.Html.of".into(), "atlas-code.page.Html".into())));
}

/// What `atlas check` asks of Atlas, each finding read: its principle, the node it is about, and
/// why it stands (a fault of Atlas's code, tracked in an issue, or a fault of the rule).
const REVIEWED: &[(&str, &str, &str)] = &[
    (
        "nothing",
        "atlas-code.serve.Server.progress",
        "`progress` writes the server's shared Progress value through a Mutex; Atlas does not trace the lock's interior mutation",
    ),
    (
        "choices",
        "atlas-engine.builder.GraphBuilder.declare",
        "`Declared` is a type already: the first test asks whether a name's first declaration, which has no node yet, starts its overloads, the `match` what a later one does, in one function",
    ),
    (
        "choices",
        "atlas-code.history.Server.respond",
        "`respond` is shared with `atlas serve`, which lets a POST through to its own routes, while history's own server answers it 405: the two tests mean two things",
    ),
    (
        "choices",
        "atlas-engine.flow.arrow_of",
        "`arrow_of` and `body_of` each read `NodeKind`, the type that says what a node is, for a question of their own (a class's arrow, an entry class's body); that a `Node` is one struct for every kind is #13",
    ),
    (
        "choices",
        "atlas-engine.rust.body.root",
        "`root`, `value_class` and `declared` read a name three ways: the name a value is a part of, the class of a value, the type a declaration gives it",
    ),
    (
        "choices",
        "atlas-engine.rust.body.Body.call",
        "a call among a macro's tokens (`token_tree`) has a shape of its own: `call` dispatches on it, `call_hands` reads its arguments; one shape, two questions",
    ),
    (
        "choices",
        "atlas-engine.rust.paths.Eval.of",
        "`of` and `parsed` are two evaluations over different domains (the values an expression can have, the type a command line is parsed into) on the same kinds of expression",
    ),
    (
        "choices",
        "atlas-engine.typescript.module.collect_types",
        "the TypeScript reader's functions each read a construct by tree-sitter's kind: a type name stands for one thing in a type, another in a member path",
    ),
    (
        "choices",
        "atlas-engine.typescript.body.Body.bucket_of",
        "the TypeScript reader's functions each read a construct by tree-sitter's kind: `bucket_of` asks where a value comes from, `chain_root` what a member chain stands on",
    ),
    (
        "choices",
        "atlas-engine.typescript.libraries.Libraries.follow_in",
        "`\"default\"` is the language's name for the default export, read in two lines of one function",
    ),
    (
        "places",
        "atlas-code.run",
        "`run` is `main`'s body, split off so that what it holds drops before the exit status is returned: the command line's default repository, the directory it is run in, is the entry's decision, as it was `main`'s",
    ),
    (
        "choices",
        "atlas-engine.flow.body_of",
        "a method's call is resolved along the order of the class it runs on (`Ix::step_of`); a function's call, or a library operation the graph does not hold, runs itself: dispatch is a fact of methods alone",
    ),
];

/// Atlas is its own first subject, and `atlas check` is a signal on it as on any code: never an
/// oracle, never a target. What it asks of Atlas is the reviewed list above; the test fails when
/// the findings change, so that each new one is read and each one gone is seen to go, not so that
/// the list stays short.
#[test]
fn atlas_findings_are_the_reviewed_ones() {
    let fs = check(atlas());
    let mut got: Vec<(&str, &str)> = fs.iter().map(|f| (f.principle.id(), f.node.as_str())).collect();
    let mut want: Vec<(&str, &str)> = REVIEWED.iter().map(|(p, n, _)| (*p, *n)).collect();
    got.sort();
    want.sort();
    let new: Vec<String> = fs
        .iter()
        .filter(|f| !want.contains(&(f.principle.id(), f.node.as_str())))
        .map(|f| format!("{} {} ({}:{}) {}", f.principle, f.node, f.file, f.line, f.what))
        .collect();
    let gone: Vec<&(&str, &str)> = want.iter().filter(|w| !got.contains(w)).collect();
    assert!(
        got == want,
        "Atlas's findings are not the reviewed ones; read each and say in REVIEWED why it stands\nnew: {new:#?}\ngone: {gone:#?}"
    );
}
