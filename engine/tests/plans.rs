//! The finest plan the viewer builds of a route, held to the plan its code implies. The oracle in the page holds the
//! pictures of a route to one another: each is a quotient of one finest plan `F`, and opening a box refines it. Whether
//! `F` says what the code does is this test's question (docs/reference/mathematics.md, "One fine plan, every picture its
//! image"). Beside a program can lie a `.plan` file (`clean.plan` beside `clean/`), written from the code and the
//! mathematics, never from what Atlas draws: for each route it names, the route's occurrences, its values by type, name
//! and moment, its lines by channel, the frame each value stands in and the side of each place. The viewer's model
//! (`viewer/plan.js`) builds the plans in node from the data a page is handed (`plans.mjs`), and every fact of the file
//! must hold, and no other.
//!
//! Every route of every program under `pairs` and `plans` is also asked for its pictures, for every set of open steps
//! (each subset while there are few, else each step and each pair): the oracle (opening one more box refines the
//! picture) and the quotient (each line of the picture counts the fine lines that cross a block's border, by channel,
//! and each box stands where it stands with every step open); and each closed family opens into a frame with the id of
//! its box for each value it stands for, as the page draws the frame (the walk's rule); and each class, picked as the set of
//! its values, opens the families it cuts in two until no box is lit in part and none of its values is lost, and a family lit
//! whole opens into lit boxes only (the filter first, mathematics.md "Saturation"); and each box of files folded into a
//! directory opens in one step to what lies below it, every other file standing where it stood. The pictures are asked again of
//! the plan cut as a large program's is cut, at a few small budgets (`ATLAS_FINE_BUDGET`): a step beyond the cut is the
//! image of its body, and each of its lines counts the fine lines its body draws there. Opening a step beyond the cut plans
//! it anew, so the oracle, which opens a box of one plan, does not hold it: each such step of the closed picture is opened
//! as the page opens it, and the picture it opens to must refine the one it was cut in.
//!
//! What Atlas gets wrong today is marked in the file with the issue that tracks it: `known #N FACT` for a fact that does
//! not hold, `known #N extra FACT` for one Atlas has and should not, `known #N fault TEXT` for a fault of the pictures
//! (before any route, of every route of the program). The test reports each and passes, and fails once it holds or is
//! gone, so that the mark goes with the fix.

use atlas_engine::{flow, parts, Options};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

/// A fact Atlas gets wrong today, and the issue that tracks it.
struct Known {
    issue: String,
    fact: String,
    /// a fact Atlas has and should not
    extra: bool,
    /// a fault of the pictures, matched as part of the fault's text
    fault: bool,
}

/// What a `.plan` file says of one route.
#[derive(Default)]
struct Expected {
    facts: BTreeSet<String>,
    known: Vec<Known>,
}

/// A `.plan` file: the faults known of every route of the program, and the routes it writes down, in its order.
#[derive(Default)]
struct Plan {
    faults: Vec<Known>,
    routes: Vec<(String, Expected)>,
}

fn read_plan(path: &Path) -> Plan {
    let text = std::fs::read_to_string(path).expect("reads the plan");
    let mut plan = Plan::default();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.split(" # ").next().unwrap_or("").trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let words: Vec<&str> = line.split_whitespace().collect();
        if let Some(route) = line.strip_prefix("route ") {
            plan.routes.push((route.trim().to_string(), Expected::default()));
            continue;
        }
        let at = || format!("{}:{}", path.display(), i + 1);
        let known = (words[0] == "known").then(|| {
            let issue = words.get(1).filter(|w| w.starts_with('#')).unwrap_or_else(|| panic!("{}: known names its issue", at()));
            let (extra, fault) = (words.get(2) == Some(&"extra"), words.get(2) == Some(&"fault"));
            let fact = words[if extra || fault { 3 } else { 2 }..].join(" ");
            Known { issue: issue.to_string(), fact, extra, fault }
        });
        match (known, plan.routes.last_mut()) {
            (Some(k), None) if k.fault => plan.faults.push(k),
            (_, None) => panic!("{}: a fact before its route", at()),
            (Some(k), Some((_, e))) => {
                if !k.extra && !k.fault {
                    e.facts.insert(k.fact.clone());
                }
                e.known.push(k);
            }
            (None, Some((_, e))) => {
                e.facts.insert(words.join(" "));
            }
        }
    }
    plan
}

/// A fact that says one thing of its subject (where a value stands, which side a place is on) is compared by its
/// subject: another frame or side is the same fact, wrong.
fn subject(fact: &str) -> Option<String> {
    let w: Vec<&str> = fact.split(' ').collect();
    (matches!(w[0], "home" | "side") && w.len() > 2).then(|| w[..w.len() - 1].join(" "))
}

/// The page's data of a program: its graph and its flow.
fn data(program: &Path) -> String {
    let graph = parts::analyze_repo(&[program.to_path_buf()], &Options::default()).expect("reads the program");
    let mut data = serde_json::to_value(&graph).expect("the graph is data");
    data["flow"] = serde_json::to_value(flow::of(&graph)).expect("the flow is data");
    let data = data.to_string();
    // `ATLAS_PLANS_DATA=dir` keeps each program's data there, for `node plans.mjs ../viewer/plan.js DATA ROUTE` by hand
    if let Ok(keep) = std::env::var("ATLAS_PLANS_DATA") {
        let name = program.to_string_lossy().replace(['/', '\\', ':'], "_");
        let short = name.rsplit_once("tests_").map_or(name.as_str(), |(_, s)| s);
        std::fs::write(Path::new(&keep).join(format!("{short}.json")), &data).expect("keeps the data");
    }
    data
}

/// What the viewer's model builds of a program: for each route but its tests, the facts of its finest plan and the faults
/// of its pictures.
fn built(program: &Path) -> BTreeMap<String, (BTreeSet<String>, Vec<String>)> {
    modelled(program, &data(program), None)
}

/// What the viewer's model builds of a program's data, its plan cut at about `budget` steps as a large program's is cut
/// (`ATLAS_FINE_BUDGET`), or at its own budget.
fn modelled(program: &Path, data: &str, budget: Option<usize>) -> BTreeMap<String, (BTreeSet<String>, Vec<String>)> {
    let name = program.to_string_lossy().replace(['/', '\\', ':'], "_");
    let file = std::env::temp_dir().join(format!("atlas-plans-{}-{name}-{budget:?}.json", std::process::id()));
    std::fs::write(&file, data).expect("writes the data");
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut node = Command::new("node");
    if let Some(b) = budget {
        node.env("ATLAS_FINE_BUDGET", b.to_string());
    }
    let out = node
        .arg(here.join("tests/plans.mjs"))
        .arg(here.join("../viewer/plan.js"))
        .arg(&file)
        .output()
        .expect("node runs the viewer's model: the test needs node on the PATH");
    let _ = std::fs::remove_file(&file);
    assert!(out.status.success(), "{}: the model fails\n{}", program.display(), String::from_utf8_lossy(&out.stderr));
    let read: BTreeMap<String, serde_json::Value> = serde_json::from_slice(&out.stdout).expect("the model prints JSON");
    let strings = |v: &serde_json::Value| -> Vec<String> {
        v.as_array().into_iter().flatten().filter_map(|x| x.as_str().map(String::from)).collect()
    };
    read.into_iter().map(|(r, v)| (r, (strings(&v["facts"]).into_iter().collect(), strings(&v["faults"])))).collect()
}

/// The programs under `dir`: each directory with a manifest that no directory above it, up to `dir`, holds (the parts
/// of a program are read with it).
fn programs(dir: &Path, out: &mut Vec<PathBuf>) {
    let manifest = |d: &Path| ["pyproject.toml", "Cargo.toml", "package.json"].iter().any(|m| d.join(m).is_file());
    if manifest(dir) {
        out.push(dir.to_path_buf());
        return;
    }
    let mut dirs: Vec<PathBuf> =
        std::fs::read_dir(dir).expect("reads the directory").flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    dirs.sort();
    for d in dirs {
        if !matches!(d.file_name().and_then(|n| n.to_str()), Some("node_modules" | "typings" | "target" | ".atlas")) {
            programs(&d, out);
        }
    }
}

/// How a route's finest plan stands to what is written down: what fails, and what is known to.
fn compare(expected: &Expected, facts: &BTreeSet<String>, fails: &mut Vec<String>, known: &mut Vec<String>) {
    let marked = |f: &str| expected.known.iter().find(|k| !k.extra && !k.fault && k.fact == f);
    let excused: BTreeSet<String> = expected.known.iter().filter(|k| !k.extra && !k.fault).filter_map(|k| subject(&k.fact)).collect();
    let actual = |f: &str| {
        subject(f).and_then(|s| facts.iter().find(|a| subject(a).as_deref() == Some(s.as_str()))).map(|a| format!(" (Atlas: {a})"))
    };
    for f in &expected.facts {
        match (facts.contains(f), marked(f)) {
            (true, Some(k)) => fails.push(format!("holds now, so the mark goes: known {} {f}", k.issue)),
            (false, Some(k)) => known.push(format!("{} {f}{}", k.issue, actual(f).unwrap_or_default())),
            (false, None) => fails.push(format!("missing {f}{}", actual(f).unwrap_or_default())),
            (true, None) => {}
        }
    }
    for a in facts.iter().filter(|a| !expected.facts.contains(*a)) {
        // a frame or side other than the one written down is reported where it is missing
        if subject(a).is_some_and(|s| excused.contains(&s) || expected.facts.iter().any(|f| subject(f).as_deref() == Some(s.as_str()))) {
            continue;
        }
        match expected.known.iter().find(|k| k.extra && &k.fact == a) {
            Some(k) => known.push(format!("{} extra {a}", k.issue)),
            None => fails.push(format!("extra {a}")),
        }
    }
    for k in expected.known.iter().filter(|k| k.extra && !facts.contains(&k.fact)) {
        fails.push(format!("gone now, so the mark goes: known {} extra {}", k.issue, k.fact));
    }
}

/// The faults of a route's pictures, against those known of it.
fn faults_of(marks: &[&Known], faults: &[String], fails: &mut Vec<String>, known: &mut Vec<String>) {
    for f in faults {
        match marks.iter().find(|k| f.contains(&k.fact)) {
            Some(k) => known.push(format!("{} fault {f}", k.issue)),
            None => fails.push(format!("fault {f}")),
        }
    }
}

/// The programs under `pairs` and `plans`; `ATLAS_PLANS=pairs/pipeline` keeps only those under that path.
fn all_programs() -> (PathBuf, Vec<PathBuf>) {
    let tests = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut all = vec![];
    for dir in ["pairs", "plans"] {
        programs(&tests.join(dir), &mut all);
    }
    if let Ok(only) = std::env::var("ATLAS_PLANS") {
        all.retain(|p| p.strip_prefix(&tests).is_ok_and(|r| r.starts_with(&only)));
    }
    (tests, all)
}

/// Every plan written down holds of the plan the viewer builds, and every picture of every route is the image of its
/// plan, but what is marked as known, each with its issue.
#[test]
fn the_finest_plan_says_what_the_code_does() {
    let (tests, all) = all_programs();
    let (mut report, mut failed, mut written) = (vec![], 0, 0);
    for program in &all {
        let name = program.strip_prefix(&tests).unwrap().display().to_string();
        let file = program.with_extension("plan");
        let plan = if file.is_file() { read_plan(&file) } else { Plan::default() };
        let built = built(program);
        let (mut fails, mut known) = (vec![], vec![]);
        for (route, _) in plan.routes.iter().filter(|(r, _)| !built.contains_key(r)) {
            fails.push(format!("the plan writes down {route}, a route Atlas does not read"));
        }
        let mut faulted = BTreeSet::new();
        for (route, (facts, faults)) in &built {
            let expected = plan.routes.iter().find(|(r, _)| r == route).map(|(_, e)| e);
            let (mut f, mut k) = (vec![], vec![]);
            if let Some(e) = expected {
                compare(e, facts, &mut f, &mut k);
                written += 1;
            }
            let marks: Vec<&Known> =
                plan.faults.iter().chain(expected.into_iter().flat_map(|e| e.known.iter())).filter(|k| k.fault).collect();
            faults_of(&marks, faults, &mut f, &mut k);
            faulted.extend(marks.iter().filter(|m| faults.iter().any(|x| x.contains(&m.fact))).map(|m| m.fact.clone()));
            fails.extend(f.into_iter().map(|x| format!("{route}: {x}")));
            known.extend(k.into_iter().map(|x| format!("{route}: {x}")));
        }
        for (route, e) in &plan.routes {
            for k in e.known.iter().filter(|k| k.fault && !faulted.contains(&k.fact)) {
                fails.push(format!("{route}: gone now, so the mark goes: known {} fault {}", k.issue, k.fact));
            }
        }
        for k in plan.faults.iter().filter(|k| !faulted.contains(&k.fact)) {
            fails.push(format!("gone now, so the mark goes: known {} fault {}", k.issue, k.fact));
        }
        report.push(format!(
            "{name}: {} routes, {} written down, {} known, {} failing",
            built.len(),
            plan.routes.len(),
            known.len(),
            fails.len()
        ));
        report.extend(known.iter().map(|k| format!("    known {k}")));
        report.extend(fails.iter().map(|f| format!("    FAILS {f}")));
        failed += fails.len();
    }
    println!("{}", report.join("\n"));
    assert!(written > 0 || std::env::var("ATLAS_PLANS").is_ok(), "there are plans to hold");
    assert!(failed == 0, "{failed} facts of the plans do not hold:\n{}", report.join("\n"));
}

/// The budgets the plans are cut at as a large program's plan is cut, each small enough to cut the small programs here.
const BUDGETS: [usize; 3] = [1, 2, 3];

/// Cut as a large program's plan is cut (`ATLAS_FINE_BUDGET`), every picture of every route is still the image of its
/// plan: a step beyond the cut is the image of its body (`cutArrow`), each of its lines counting the fine lines its body
/// draws there (#169). The facts are not asked of a cut plan, which holds less of `F` than the finest one does; the faults
/// known of a program are known here too. The programs are read and modelled side by side, each at every budget.
#[test]
fn every_picture_of_a_cut_plan_is_its_image() {
    let (tests, all) = all_programs();
    let (next, fails) = (AtomicUsize::new(0), Mutex::new(vec![]));
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get());
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| {
                while let Some(program) = all.get(next.fetch_add(1, Ordering::Relaxed)) {
                    let name = program.strip_prefix(&tests).unwrap().display().to_string();
                    let file = program.with_extension("plan");
                    let plan = if file.is_file() { read_plan(&file) } else { Plan::default() };
                    let data = data(program);
                    for budget in BUDGETS {
                        for (route, (_, faults)) in modelled(program, &data, Some(budget)) {
                            let expected = plan.routes.iter().find(|(r, _)| *r == route).map(|(_, e)| e);
                            let marks: Vec<&Known> =
                                plan.faults.iter().chain(expected.into_iter().flat_map(|e| e.known.iter())).filter(|k| k.fault).collect();
                            let (mut f, mut known) = (vec![], vec![]);
                            faults_of(&marks, &faults, &mut f, &mut known);
                            fails.lock().unwrap().extend(f.into_iter().map(|x| format!("{name}, budget {budget}, {route}: {x}")));
                        }
                    }
                }
            });
        }
    });
    let fails = fails.into_inner().unwrap();
    assert!(fails.is_empty(), "{} faults of the cut plans:\n{}", fails.len(), fails.join("\n"));
}

/// How many levels the route of `a_step_beyond_the_cut_is_made_without_unfolding_its_body` calls down, each twice.
const LEVELS: usize = 40;

/// A step beyond the cut is the image of its body unfolded to its leaves, made without unfolding it: once for each function,
/// call and set of callers that make its paths whole (mathematics.md, "One fine plan, every picture its image"). Its subject
/// is a program whose route calls each of `LEVELS` levels twice, handing one value down by its name: unfolded, its body has
/// 2^40 occurrences. A model that kept anything of the body as it unfolds (an occurrence, a line, a group of values a step
/// takes) could not plan it; one that makes each step's arrow once for each call does, at every budget that cuts it, and
/// every picture of the cut plan is its image (#379).
#[test]
fn a_step_beyond_the_cut_is_made_without_unfolding_its_body() {
    let program = std::env::temp_dir().join(format!("atlas-plans-fanout-{}", std::process::id()));
    std::fs::create_dir_all(program.join("src")).expect("makes the program's directory");
    std::fs::write(program.join("Cargo.toml"), "[package]\nname = \"fanout\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n")
        .expect("writes the manifest");
    let mut code = String::from(
        "struct Config {\n    out: String,\n}\n\nfn main() {\n    let config = Config { out: String::from(\"out.txt\") };\n    level0(&config);\n}\n",
    );
    for i in 0..LEVELS {
        code += &format!("\nfn level{i}(config: &Config) {{\n    level{0}(config);\n    level{0}(config);\n}}\n", i + 1);
    }
    code += &format!("\nfn level{LEVELS}(config: &Config) {{\n    std::fs::write(&config.out, \"done\").unwrap();\n}}\n");
    std::fs::write(program.join("src").join("main.rs"), code).expect("writes the program");
    let data = data(&program);
    let mut fails = vec![];
    for budget in BUDGETS {
        let built = modelled(&program, &data, Some(budget));
        assert!(built.contains_key("fanout.main"), "the model plans the route at budget {budget}");
        fails.extend(
            built.into_iter().flat_map(|(route, (_, faults))| faults.into_iter().map(move |f| format!("budget {budget}, {route}: {f}"))),
        );
    }
    let _ = std::fs::remove_dir_all(&program);
    assert!(fails.is_empty(), "{} faults of the cut plans:\n{}", fails.len(), fails.join("\n"));
}
