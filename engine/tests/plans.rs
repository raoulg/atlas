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
//! directory opens in one step to what lies below it, every other file standing where it stood; and the picture the model
//! hands the page, its siblings that draw alike merged into one box, has both ends of every line among its boxes and joins
//! only siblings that draw the same lines, calls among them (#412); and a place on the disk that lies in another the
//! picture draws stands in the outermost of them that is closed, or in the frame of the innermost where all are open
//! (#381). The pictures are asked again of
//! the plan cut as a large program's is cut, at a few small budgets (`ATLAS_FINE_BUDGET`): a step beyond the cut is the
//! image of its body, and each of its lines counts the fine lines its body draws there. Opening a step beyond the cut plans
//! it anew, so the oracle, which opens a box of one plan, does not hold it: each such step of the closed picture is opened
//! as the page opens it, and the picture it opens to must refine the one it was cut in.
//!
//! What Atlas gets wrong today is marked in the file with the issue that tracks it: `known #N FACT` for a fact that does
//! not hold, `known #N extra FACT` for one Atlas has and should not, `known #N fault TEXT` for a fault of the pictures
//! (before any route, of every route of the program), and `known #N VIEW TEXT` for a fault of the program's views that its
//! law (`overview`, `focus`, `kinds`, `views` or `ends`) finds. The test reports each and passes, and fails once it holds or is
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
    /// the law of the views that finds the fault, for a fault of the program's views
    view: Option<String>,
}

/// What a `.plan` file says of one route.
#[derive(Default)]
struct Expected {
    facts: BTreeSet<String>,
    known: Vec<Known>,
}

/// A `.plan` file: the faults known of every route of the program and of its views, and the routes it writes down, in its
/// order.
#[derive(Default)]
struct Plan {
    faults: Vec<Known>,
    views: Vec<Known>,
    routes: Vec<(String, Expected)>,
}

/// The laws of the views, each by the word its marks carry and the flag `plans.mjs` is asked with.
const VIEW_LAWS: [&str; 5] = ["overview", "focus", "kinds", "views", "ends"];

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
            let view = words.get(2).filter(|w| VIEW_LAWS.contains(w)).map(|w| w.to_string());
            let fact = words[if extra || fault || view.is_some() { 3 } else { 2 }..].join(" ");
            Known { issue: issue.to_string(), fact, extra, fault: fault || view.is_some(), view }
        });
        match (known, plan.routes.last_mut()) {
            (Some(k), _) if k.view.is_some() => plan.views.push(k),
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
    asked(program, data, budget, &[])
}

/// What the viewer's model says of a program's data, asked as `plans.mjs` is asked: of its routes, or with `--overview`, of
/// its overview.
fn asked(program: &Path, data: &str, budget: Option<usize>, args: &[&str]) -> BTreeMap<String, (BTreeSet<String>, Vec<String>)> {
    let name = program.to_string_lossy().replace(['/', '\\', ':'], "_");
    let file = std::env::temp_dir().join(format!("atlas-plans-{}-{name}-{budget:?}-{}.json", std::process::id(), args.join("")));
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
        .args(args)
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

/// The faults one law of the views finds of every program, against the marks of that law: what fails, a mark whose fault is
/// gone among it.
fn views_hold(law: &str) -> Vec<String> {
    views_said(law).0
}

/// What one law of the views says of every program: its faults, against the marks of that law, and what it finds of the
/// programs themselves, each by the program it is of.
fn views_said(law: &str) -> (Vec<String>, Vec<String>) {
    let (tests, all) = all_programs();
    let (next, fails, findings) = (AtomicUsize::new(0), Mutex::new(vec![]), Mutex::new(vec![]));
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get());
    let flag = format!("--{law}");
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| {
                while let Some(program) = all.get(next.fetch_add(1, Ordering::Relaxed)) {
                    let name = program.strip_prefix(&tests).unwrap().display().to_string();
                    let file = program.with_extension("plan");
                    let plan = if file.is_file() { read_plan(&file) } else { Plan::default() };
                    let marks: Vec<&Known> = plan.views.iter().filter(|k| k.view.as_deref() == Some(law)).collect();
                    let (mut f, mut known) = (vec![], vec![]);
                    let said = asked(program, &data(program), None, &[&flag]);
                    findings.lock().unwrap().extend(said.values().flat_map(|(x, _)| x).map(|x| format!("{name}: {x}")));
                    let faults: Vec<String> = said.into_values().flat_map(|(_, x)| x).collect();
                    faults_of(&marks, &faults, &mut f, &mut known);
                    for k in marks.iter().filter(|k| !faults.iter().any(|x| x.contains(&k.fact))) {
                        f.push(format!("gone now, so the mark goes: known {} {law} {}", k.issue, k.fact));
                    }
                    let mut fails = fails.lock().unwrap();
                    fails.extend(f.into_iter().map(|x| format!("{name}: {x}")));
                    for k in known {
                        println!("{name}: known {k}");
                    }
                }
            });
        }
    });
    let mut findings = findings.into_inner().unwrap();
    findings.sort();
    (fails.into_inner().unwrap(), findings)
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

/// The overview is the image of one fine plan of generators, as a route's pictures are of theirs (mathematics.md, "One fine
/// plan, every picture its image"; #69): for every set of open packages and parts of every program, each line of the picture
/// counts the fine lines it stands for, a type internal to a closed package draws none, a call goes between the blocks of its
/// ends, opening one more box refines the picture (the oracle), the picture is a function of the set of open boxes, and the
/// picture the page draws, its siblings merged, has both ends of every line and every call among its boxes and joins only
/// siblings that draw the same lines (`plans/siblings-call`, #412). The
/// faults known of a program are known here too. All of it holds of the overview without the test code as well, which is the
/// image of the fiber of the plan where the code is no test code (mathematics.md, "A filter: the program without its test
/// code"; #400): its fine plan, built from the code without its test code, is the fiber of the fine plan with it, taken on
/// that plan, and for every set of open packages its picture is the image of that fiber.
#[test]
fn every_package_view_is_the_image_of_its_plan() {
    let fails = views_hold("overview");
    assert!(fails.is_empty(), "{} faults of the overviews:\n{}", fails.len(), fails.join("\n"));
}

/// A focus is the fiber of the overview's fine plan `F₀` over the packages in it, with the rest of the code around it, and
/// its picture is the image of that fiber under the map the open boxes define, as the overview's is of `F₀` (mathematics.md,
/// "One fine plan, every picture its image", the focus; #399). For every set of packages in focus (each subset while there are few,
/// else each alone and each pair) and every set of open boxes (its packages and the rest): the focus's fine plan is the fiber
/// of `F₀` as the test restates it; each line counts the fine lines it stands for, a type internal to a closed package draws
/// none, each call goes between the blocks of its ends, opening one more box refines the picture, the picture is a function
/// of the set of open boxes, and every line of the picture the page draws ends at a box it draws; a package of the focus drawn
/// closed has at its border the type boxes the overview draws at its border (`plans/focus-shapes`: four ranks of one tensor
/// type, each named otherwise by the package that takes it); and a type box stands where the overview puts it: a type's
/// home is a fact of `F₀`, the same in every view. All of it with the test code and without it.
#[test]
fn every_focus_is_the_image_of_a_fiber_of_the_overviews_plan() {
    let fails = views_hold("focus");
    assert!(fails.is_empty(), "{} faults of the focus:\n{}", fails.len(), fails.join("\n"));
}

/// All routes of a kind are the union of the routes' coarsest pictures, each route closed, its values as their types
/// (mathematics.md, "One fine plan, every picture its image", all routes of a kind; #434): the kind view's fine plan has a
/// box for each route, which takes and gives what crosses the border of its route closed. Each route's box is held to the
/// image of the route's finest plan with no box open, line by line, with the fine lines each stands for and those it
/// reaches; a route the page plans cut at its first level (`cutArrow`) and that draws otherwise is a fault of the cut (#438).
#[test]
fn every_kind_view_is_the_union_of_its_routes_closed() {
    let fails = views_hold("kinds");
    assert!(fails.is_empty(), "{} routes the kind view draws otherwise than closed:\n{}", fails.len(), fails.join("\n"));
}

/// Every view the page draws is the image of its fine plan through one function, `plan()`, on one view state `V`
/// (mathematics.md, "One fine plan, every picture its image", the view state; #437). For every program the law walks that state: each view (the overview and each
/// part alone, each focus, each kind, each route), with the test code and without, at the closed picture, at each box of
/// each coordinate of `V.open` alone and at sets of them all drawn from a seed (`ATLAS_VIEWS_SEED`). Each picture has both
/// ends of every line among its boxes, opening one more box refines it (the oracle), each line counts the fine lines it
/// stands for and each box stands where its plan puts it (a type at its home in `F₀`); a coordinate the view does not read
/// leaves its picture as it is, and drawn again after every other state, each picture is the one it was. The keys of `V`
/// are read from the page: a coordinate the law does not walk is a fault.
#[test]
fn every_view_state_draws_the_image_of_its_plan() {
    let fails = views_hold("views");
    assert!(fails.is_empty(), "{} faults of the view states:\n{}", fails.len(), fails.join("\n"));
}

/// A route starts at an entry point and ends at a place, and both are visible (sentence 3): every route closed, alone and
/// among the routes of its kind, whose code gives something (a value back, whatever its type, a change, a write, a request,
/// an error), as the graph says and no plan, gives a value, a place it changes or a call it makes. A route whose code gives
/// nothing and changes nothing ends at the terminal object, no place: a finding of the program, which the law says and does
/// not fail on (mathematics.md, "The terminal object").
#[test]
fn every_route_ends_at_a_place() {
    let (fails, findings) = views_said("ends");
    for f in &findings {
        println!("finding {f}");
    }
    assert!(fails.is_empty(), "{} routes end at no place:\n{}", fails.len(), fails.join("\n"));
}

/// Test code is not saturated by the package quotient, so the fiber and the quotient do not commute (mathematics.md,
/// "Saturation"): in each language, a package that holds test code beside ordinary code (`plans/overview-tests`: a pytest
/// file, a `#[cfg(test)] mod tests`, a `*.test.ts` beside the store) is a box both overviews draw, with other lines. A filter
/// taken on the quotient keeps or drops a box whole, and so cannot draw the fiber's picture: this box is the counterexample.
#[test]
fn test_code_cuts_a_package_in_two() {
    let tests = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut all = vec![];
    programs(&tests.join("plans/overview-tests"), &mut all);
    assert_eq!(all.len(), 3, "a program in each language");
    for program in &all {
        let asked = asked(program, &data(program), None, &["--overview"]);
        let facts = asked.values().flat_map(|(facts, _)| facts.iter()).collect::<Vec<_>>();
        assert!(facts.iter().any(|f| f.starts_with("cut unit:")), "{}: no package is cut in two by the test code", program.display());
    }
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

/// An opened group of places (the storage quotient) splits into its places, and they fold further into their directories
/// (mathematics.md, "Places by the type that holds them"): the page draws an opened group by `foldGroups` of the viewer's
/// model. For groups and folds drawn from a seed, every member of a group is a box the picture draws, no box stands in two
/// groups, a directory stands in a group only where every file it folds is that group's, and every place a group holds
/// stays in the picture, as itself in the group or as the directory it folds into beside it.
#[test]
fn an_opened_group_holds_its_files_as_the_disk_folds_them() {
    let program = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/plans/overview-packages");
    let file = std::env::temp_dir().join(format!("atlas-fold-groups-{}.json", std::process::id()));
    std::fs::write(&file, data(&program)).expect("writes the data");
    let check = r#"
        const fs = require("node:fs");
        const [planJs, dataJson] = process.argv.slice(1);
        const D = JSON.parse(fs.readFileSync(dataJson, "utf8"));
        const foldGroups = new Function("D", "window", fs.readFileSync(planJs, "utf8") + "\nreturn foldGroups;")(D, {});
        let a = 7;
        const random = () => { a = (a + 0x6d2b79f5) >>> 0; let t = Math.imul(a ^ (a >>> 15), 1 | a); t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t; return ((t ^ (t >>> 14)) >>> 0) / 4294967296; };
        const pick = n => Math.floor(random() * n);
        const faults = [];
        for (let round = 0; round < 500; round++) {
          // files f0.., places that are no file p0.., directories d0..; each file folds into a directory or stands as itself
          const files = [...Array(1 + pick(8)).keys()].map(i => `f${i}`), others = [...Array(pick(3)).keys()].map(i => `p${i}`);
          const into = new Map(files.map(f => [f, random() < 0.7 ? `d${pick(3)}` : f]));
          const to = k => into.get(k) ?? k;
          const drawnBoxes = new Set([...files.map(to), ...others].filter(() => random() < 0.95));
          const drawn = k => drawnBoxes.has(k);
          const groups = new Map();
          for (const k of [...files, ...others]) { const g = pick(4); if (g < 2) { if (!groups.has(`g${g}`)) groups.set(`g${g}`, new Set()); groups.get(`g${g}`).add(k); } }
          const out = foldGroups(groups, files, to, drawn), seen = new Map();
          for (const [g, ks] of out) for (const k of ks) {
            if (!drawn(k)) faults.push(`${round}: ${g} holds ${k}, which the picture does not draw`);
            if (seen.has(k)) faults.push(`${round}: ${k} stands in ${seen.get(k)} and in ${g}`);
            seen.set(k, g);
            for (const f of files) if (to(f) === k && f !== k && ![...(groups.get(g) || [])].includes(f)) faults.push(`${round}: ${g} holds ${k}, which folds ${f}, no file of ${g}`);
          }
          for (const [g, ks] of groups) for (const k of ks) if (drawn(to(k)) && !out.get(g).has(to(k)) && !(to(k) !== k && [...out.values()].every(s => !s.has(to(k))))) faults.push(`${round}: ${k} of ${g} is lost`);
        }
        process.stdout.write(JSON.stringify(faults.slice(0, 20)));
    "#;
    let out = Command::new("node")
        .arg("-e")
        .arg(check)
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../viewer/plan.js"))
        .arg(&file)
        .output()
        .expect("node runs the viewer's model: the test needs node on the PATH");
    let _ = std::fs::remove_file(&file);
    assert!(out.status.success(), "the model fails\n{}", String::from_utf8_lossy(&out.stderr));
    let faults: Vec<String> = serde_json::from_slice(&out.stdout).expect("the check prints JSON");
    assert!(faults.is_empty(), "{} faults of opened groups:\n{}", faults.len(), faults.join("\n"));
}
