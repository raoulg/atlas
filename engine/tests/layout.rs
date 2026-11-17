//! The layout the page lays its pictures out with (`layOut` in `viewer/plan.js`), held to what the overview claims of it
//! (docs/reference/mathematics.md, "Layered drawing"): the overview is four columns, what the packages read of the world,
//! the packages, the types that cross between them, what they write. Its graphs are drawn by `proptest` in the overview's
//! shape: places read, with lines into packages; packages, some opened into a frame of functions; places a package leaves
//! for another (between the packages); types, with a line from each package that gives or takes one; places written,
//! with lines out of packages. `layout.mjs` lays each out in node with the ELK the page bundles.

use std::path::PathBuf;
use std::process::Command;

use proptest::prelude::*;
use serde_json::{json, Value};

/// An overview: how many places it reads, the packages (each with how many functions it is opened into: 0 is closed),
/// how many places between packages, types and places written, and its lines as indices into the candidates `lines` names.
#[derive(Debug, Clone)]
struct Overview {
    reads: usize,
    packages: Vec<usize>,
    between: usize,
    types: usize,
    writes: usize,
    picks: Vec<prop::sample::Index>,
}

fn overview() -> impl Strategy<Value = Overview> {
    (
        0usize..4,
        prop::collection::vec(prop_oneof![3 => Just(0usize), 1 => 1usize..4], 1..6),
        0usize..2,
        0usize..4,
        0usize..4,
        prop::collection::vec(any::<prop::sample::Index>(), 0..14),
    )
        .prop_map(|(reads, packages, between, types, writes, picks)| Overview { reads, packages, between, types, writes, picks })
}

impl Overview {
    /// The box a package's line starts or ends at: the package closed, or one of its functions opened.
    fn ends(&self) -> Vec<String> {
        self.packages
            .iter()
            .enumerate()
            .flat_map(|(i, &n)| if n == 0 { vec![format!("p{i}")] } else { (0..n).map(|f| format!("p{i}.f{f}")).collect() })
            .collect()
    }

    /// Every line the overview can draw: a place read into code, code into a place written, code into a type (a line from a
    /// type into the code that takes it is laid out from the code, `flipped`), and code to a place between and on.
    fn lines(&self) -> Vec<(String, String)> {
        let code = self.ends();
        let mut all = vec![];
        for c in &code {
            all.extend((0..self.reads).map(|r| (format!("r{r}"), c.clone())));
            all.extend((0..self.writes).map(|w| (c.clone(), format!("w{w}"))));
            all.extend((0..self.types).map(|t| (c.clone(), format!("t{t}"))));
            all.extend((0..self.between).flat_map(|m| [(c.clone(), format!("m{m}")), (format!("m{m}"), c.clone())]));
        }
        all
    }

    /// The ELK graph of the overview and the column of each box at its top.
    fn graph(&self) -> Value {
        let leaf = |id: String| json!({"id": id, "width": 60, "height": 30});
        let mut children = vec![];
        let mut columns = serde_json::Map::new();
        let mut put = |node: Value, column: &str| {
            columns.insert(node["id"].as_str().unwrap().to_string(), json!(column));
            children.push(node);
        };
        for r in 0..self.reads {
            put(leaf(format!("r{r}")), "in");
        }
        for (i, &n) in self.packages.iter().enumerate() {
            if n == 0 {
                put(leaf(format!("p{i}")), "code");
            } else {
                let kids: Vec<Value> = (0..n).map(|f| leaf(format!("p{i}.f{f}"))).collect();
                put(
                    json!({"id": format!("p{i}"), "children": kids, "layoutOptions": {"elk.padding": "[top=34,left=14,bottom=14,right=14]"}}),
                    "code",
                );
            }
        }
        for m in 0..self.between {
            put(leaf(format!("m{m}")), "code");
        }
        for t in 0..self.types {
            put(leaf(format!("t{t}")), "type");
        }
        for w in 0..self.writes {
            put(leaf(format!("w{w}")), "out");
        }
        let all = self.lines();
        let mut picked: Vec<&(String, String)> = if all.is_empty() { vec![] } else { self.picks.iter().map(|i| i.get(&all)).collect() };
        picked.sort();
        picked.dedup();
        let edges: Vec<Value> =
            picked.iter().enumerate().map(|(i, (a, b))| json!({"id": format!("e{i}"), "sources": [a], "targets": [b]})).collect();
        json!({"root": {"id": "root", "children": children, "edges": edges}, "columns": columns})
    }
}

/// Each graph (`Overview::graph`) laid out as the page lays it out: the error ELK gives, or where each box at the top stands.
fn laid(graphs: &[Value]) -> Vec<Value> {
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    static ASKED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let nth = ASKED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let file = std::env::temp_dir().join(format!("atlas-layout-{}-{nth}.json", std::process::id()));
    std::fs::write(&file, serde_json::to_string(graphs).unwrap()).expect("writes the graphs");
    let out = Command::new("node")
        .arg(here.join("tests/layout.mjs"))
        .arg(here.join("../viewer/plan.js"))
        .arg(here.join("../viewer/vendor/elk.bundled.js"))
        .arg(&file)
        .output()
        .expect("node runs the page's layout: the test needs node on the PATH");
    let _ = std::fs::remove_file(&file);
    assert!(out.status.success(), "the layout fails to run\n{}", String::from_utf8_lossy(&out.stderr));
    serde_json::from_slice(&out.stdout).expect("the layout prints JSON")
}

const COLUMNS: [&str; 4] = ["in", "code", "type", "out"];

/// What the layout of one overview breaks of the law, if anything: ELK refuses it, or a box stands left of a column before
/// its own.
fn broken(o: &Overview, laid: &Value) -> Option<String> {
    if let Some(e) = laid.get("error") {
        return Some(format!("the layout refuses the overview: {e}"));
    }
    let g = o.graph();
    let column = |id: &str| COLUMNS.iter().position(|c| g["columns"][id] == *c).unwrap();
    let x = laid["x"].as_object().unwrap();
    for (a, xa) in x {
        for (b, xb) in x {
            if column(a) < column(b) && xa[1].as_f64().unwrap() > xb[0].as_f64().unwrap() {
                return Some(format!("{a} (column {}) reaches past the start of {b} (column {})", g["columns"][a], g["columns"][b]));
            }
        }
    }
    None
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]
    /// Law: every overview a graph gives is laid out, its columns in their order. Each graph is a batch of overviews laid
    /// out by one run of node; a box whose every line runs into the last column is among them where the batch draws one.
    #[test]
    fn every_overview_is_laid_out_in_its_columns(batch in prop::collection::vec(overview(), 1..12)) {
        let laid = laid(&batch.iter().map(Overview::graph).collect::<Vec<_>>());
        for (o, l) in batch.iter().zip(&laid) {
            if let Some(fault) = broken(o, l) {
                prop_assert!(false, "{fault}\n{o:?}\n{}", o.graph());
            }
        }
    }
}

/// Law, at the smallest overview that asks it: a package that only writes beside one whose type it does not share. ELK
/// derives a constraint for a box whose every line runs into a box constrained to the last layer; the columns must leave
/// that box its place.
#[test]
fn a_package_that_only_writes_is_laid_out_beside_the_types() {
    let o = Overview { reads: 1, packages: vec![0, 0], between: 0, types: 1, writes: 1, picks: vec![] };
    let mut g = o.graph();
    g["root"]["edges"] = json!([
        {"id": "e0", "sources": ["r0"], "targets": ["p0"]},
        {"id": "e1", "sources": ["p0"], "targets": ["t0"]},
        {"id": "e2", "sources": ["p1"], "targets": ["w0"]},
    ]);
    let laid = laid(&[g]);
    if let Some(fault) = broken(&o, &laid[0]) {
        panic!("{fault}");
    }
}
