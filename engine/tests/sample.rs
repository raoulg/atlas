//! The laws, asked of real programs at sites drawn from a seed. A law that is hard to prove for every
//! program is asked here of as many sites as the time allows: each site is one law at one place of one
//! program (a set of nodes a fiber keeps, a route opened at a set of steps), and passing is evidence,
//! never a proof. A site that breaks names its law, and the seed and its number bring it back.
//!
//! The test is ignored: it reads the programs it is pointed at, which `cargo test` does not have.
//!
//! ```text
//! ATLAS_SAMPLE_REPOS=~/code/a:~/code/b cargo test -p atlas-engine --test sample -- --ignored --nocapture
//! ```
//!
//! - `ATLAS_SAMPLE_REPOS`: the programs, as a path list (`:` between them, `;` on Windows).
//! - `ATLAS_SAMPLE_SECONDS`: a time budget; sites are drawn until it is spent. Else `ATLAS_SAMPLE_N` sites (20).
//!   Each program is read once whatever the budget; the laws of reading, which read it three times more,
//!   are asked where the budget leaves room for them.
//! - `ATLAS_SAMPLE_SEED`: the seed; drawn and printed when none is given.
//! - `ATLAS_SAMPLE_OPENINGS`: how many sets of open steps a route is asked at (8).
//! - `ATLAS_SAMPLE_OUT`: where the report goes (the system's temporary folder by default): the Atlas build,
//!   the seed, each program's revision, and every site with its law and what broke.
//!
//! Site `i` is a function of the seed, `i` and the programs as they are read, never of the sites before
//! it or of the budget: `ATLAS_SAMPLE_SEED=s ATLAS_SAMPLE_N=i+1` on the same revisions asks it again.
//!
//! Each program is first asked the laws of reading it: parse is a function (two cold readings are one),
//! and a warm reading is the cold one (the cache's law). Then each site is one of the laws of views
//! (`laws/views.rs`, the same functions `laws.rs` asks of the graphs proptest draws) or the laws of a
//! route's pictures (`plans.mjs`, the same oracle and quotient `plans.rs` asks of every opening of the
//! small programs, here at openings drawn from the seed, and at times with the plan cut as a large
//! program's is).

#[path = "laws/views.rs"]
mod views;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use atlas_engine::cache::Cache;
use atlas_engine::{flow, parts, principles, EntryKind, Graph, NodeKind, Options};
use serde_json::{json, Value};

/// splitmix64: a stream of numbers a seed decides.
struct Draw(u64);

impl Draw {
    fn new(seed: u64, site: u64) -> Draw {
        let mut d = Draw(seed ^ site.wrapping_mul(0x9e37_79b9_7f4a_7c15));
        d.next();
        d
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }

    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// A program as the sites see it: its graph, the page's data of it, its routes and its modules.
struct Program {
    path: PathBuf,
    graph: Graph,
    data: PathBuf,
    routes: Vec<String>,
    modules: Vec<String>,
}

/// One law at one place: what is asked, and what broke.
struct Site {
    number: u64,
    program: usize,
    law: String,
    at: Value,
    broke: Option<String>,
}

/// The laws of views a site may ask, and the pictures of a route, which count as many as they do.
const VIEWS: [&str; 5] =
    ["fibers commute", "quotients compose", "a quotient counts its crossings", "a saturated fiber commutes", "a fiber before a quotient"];
const PICTURES: &str = "a route's pictures are the images of its plan";

/// The budgets a route's plan may be cut at, as plan.js cuts a large program's (`ATLAS_FINE_BUDGET`); none is the page's own.
const BUDGETS: [Option<usize>; 4] = [None, None, Some(50), Some(500)];

fn var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

fn git(dir: &Path, args: &[&str]) -> String {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

/// What a site draws, before it is asked: a pure function of the seed, its number and the programs. A site of the views
/// draws the sets its law is asked of (the nodes a fiber keeps, or the modules), a site of the pictures a route and a budget.
fn draw(seed: u64, number: u64, programs: &[Program]) -> (usize, String, Value, Vec<BTreeSet<String>>) {
    let mut d = Draw::new(seed, number);
    let program = d.below(programs.len());
    let p = &programs[program];
    if !p.routes.is_empty() && d.below(2) == 0 {
        let route = p.routes[d.below(p.routes.len())].clone();
        let budget = BUDGETS[d.below(BUDGETS.len())];
        return (program, PICTURES.into(), json!({ "route": route, "budget": budget }), vec![]);
    }
    let law = VIEWS[d.below(VIEWS.len())];
    let share = d.unit();
    let modules = law == "a saturated fiber commutes";
    let mut nodes = || -> BTreeSet<String> { p.graph.nodes.iter().filter(|_| d.unit() < share).map(|n| n.id.clone()).collect() };
    let sets = match law {
        "fibers commute" => vec![nodes(), nodes()],
        "a saturated fiber commutes" => vec![p.modules.iter().filter(|_| d.unit() < share).cloned().collect()],
        _ => vec![nodes()],
    };
    let of = if modules { p.modules.len() } else { p.graph.nodes.len() };
    let at = json!({ "share": (share * 100.0).round() / 100.0, "kept": sets.iter().map(BTreeSet::len).collect::<Vec<_>>(), "of": of });
    (program, law.into(), at, sets)
}

fn ask_view(p: &Program, law: &str, sets: &[BTreeSet<String>]) -> Result<(), String> {
    match law {
        "fibers commute" => views::fibers_commute(&p.graph, &sets[0], &sets[1]),
        "quotients compose" => views::quotients_compose(&p.graph),
        "a quotient counts its crossings" => views::a_quotient_counts_the_lines_that_cross_its_borders(&p.graph),
        "a saturated fiber commutes" => views::a_saturated_fiber_commutes(&p.graph, &sets[0]),
        _ => views::a_fiber_before_a_quotient_counts_only_what_it_keeps(&p.graph, &sets[0]),
    }
}

/// The faults of the pictures of `routes` of a program, by route, from `plans.mjs` in its sampled mode; the routes it had no
/// time for are absent.
fn ask_pictures(
    p: &Program,
    routes: &[String],
    budget: Option<usize>,
    seed: u64,
    openings: usize,
    until: Option<SystemTime>,
) -> BTreeMap<String, Vec<String>> {
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut node = Command::new("node");
    node.env("ATLAS_SAMPLE_OPENINGS", openings.to_string()).env("ATLAS_SAMPLE_SEED", (seed as u32).to_string());
    if let Some(b) = budget {
        node.env("ATLAS_FINE_BUDGET", b.to_string());
    }
    if let Some(t) = until {
        node.env("ATLAS_SAMPLE_DEADLINE", t.duration_since(UNIX_EPOCH).unwrap().as_millis().to_string());
    }
    let out = node
        .arg(here.join("tests/plans.mjs"))
        .arg(here.join("../viewer/plan.js"))
        .arg(&p.data)
        .args(routes)
        .output()
        .expect("node runs the viewer's model: the sampler needs node on the PATH");
    if !out.status.success() {
        let why = format!("the model fails: {}", String::from_utf8_lossy(&out.stderr).lines().take(8).collect::<Vec<_>>().join("\n"));
        return routes.iter().map(|r| (r.clone(), vec![why.clone()])).collect();
    }
    let read: BTreeMap<String, Value> = serde_json::from_slice(&out.stdout).expect("the model prints JSON");
    read.into_iter()
        .map(|(r, v)| (r, v["faults"].as_array().into_iter().flatten().filter_map(|f| f.as_str().map(String::from)).collect()))
        .collect()
}

/// Where two values first differ, as a path into them (`/nodes/12/touches/0/artifact`), with what each holds there.
fn first_difference(a: &Value, b: &Value, at: String) -> Option<String> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            let keys: BTreeSet<&String> = x.keys().chain(y.keys()).collect();
            keys.into_iter()
                .find_map(|k| first_difference(x.get(k).unwrap_or(&Value::Null), y.get(k).unwrap_or(&Value::Null), format!("{at}/{k}")))
        }
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => {
            x.iter().zip(y).enumerate().find_map(|(i, (p, q))| first_difference(p, q, format!("{at}/{i}")))
        }
        (Value::Array(x), Value::Array(y)) => {
            let i = x.iter().zip(y).position(|(p, q)| p != q).unwrap_or(x.len().min(y.len()));
            let cut = |v: Option<&Value>| v.map_or("nothing".into(), |v| v.to_string().chars().take(300).collect::<String>());
            Some(format!("{at}: {} items against {}, first apart at {i}: {} against {}", x.len(), y.len(), cut(x.get(i)), cut(y.get(i))))
        }
        _ if a == b => None,
        _ => {
            let cut = |v: &Value| v.to_string().chars().take(300).collect::<String>();
            Some(format!("{at}: {} against {}", cut(a), cut(b)))
        }
    }
}

/// The laws of reading a program: two cold readings are one, and a warm one, read through a cache it filled, is the cold one.
fn reading_laws(path: &Path, cold: &Graph) -> Vec<(String, Option<String>)> {
    let opts = Options::default();
    let json = |g: &Graph| serde_json::to_value(g).unwrap();
    let mut out = vec![];
    let again = parts::analyze_repo(&[path.to_path_buf()], &opts).expect("reads the program again");
    let differ = first_difference(&json(cold), &json(&again), String::new());
    out.push(("parse is a function".to_string(), differ.map(|d| format!("two cold readings differ at {d}"))));
    let dir = std::env::temp_dir().join(format!(
        "atlas-sample-cache-{}-{}",
        std::process::id(),
        path.display().to_string().replace(['/', '\\', ':'], "_")
    ));
    // the working tree's index lies in `.atlas/index/<build>` at the top of the program's repository, as `atlas view` keeps
    // it: a build of the sampler's own, taken away after
    let build = format!("sample-{}", std::process::id());
    let top = git(path, &["rev-parse", "--show-toplevel"]);
    let indexed = Options { cache: Some(Cache::at(&dir, &build)), ..opts.clone() };
    let filled = parts::analyze_repo(&[path.to_path_buf()], &indexed).expect("reads the program into the cache");
    let warm = parts::analyze_repo(&[path.to_path_buf()], &indexed).expect("reads the program from the cache");
    let hits = indexed.cache.as_ref().unwrap().hits();
    let broke = if hits == 0 {
        Some("the second reading took nothing from the cache, so nothing was compared".to_string())
    } else if json(&filled) != json(cold) {
        first_difference(&json(cold), &json(&filled), String::new())
            .map(|d| format!("the reading that fills the cache is not the cold one, at {d}"))
    } else if json(&warm) != json(cold) {
        first_difference(&json(cold), &json(&warm), String::new()).map(|d| format!("the warm reading is not the cold one, at {d}"))
    } else if serde_json::to_value(flow::of(&warm)).unwrap() != serde_json::to_value(flow::of(cold)).unwrap() {
        Some("the warm reading's flow is not the cold one's".to_string())
    } else if principles::check(&warm) != principles::check(cold) {
        Some("the warm reading's findings are not the cold one's".to_string())
    } else {
        None
    };
    let _ = std::fs::remove_dir_all(&dir);
    if !top.is_empty() {
        let _ = std::fs::remove_dir_all(Path::new(&top).join(".atlas/index").join(&build));
    }
    out.push(("a warm reading is the cold one".to_string(), broke));
    out
}

#[test]
#[ignore = "reads the programs ATLAS_SAMPLE_REPOS names"]
fn the_laws_hold_at_sampled_sites() {
    let repos: Vec<PathBuf> = std::env::split_paths(&var("ATLAS_SAMPLE_REPOS").expect("ATLAS_SAMPLE_REPOS names the programs to sample"))
        .map(|p| PathBuf::from(p.to_string_lossy().replacen('~', &std::env::var("HOME").unwrap_or_default(), 1)))
        .collect();
    let seed: u64 = var("ATLAS_SAMPLE_SEED").map_or_else(
        || SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64 & 0xffff_ffff,
        |s| s.parse().expect("the seed is a number"),
    );
    let seconds: Option<u64> = var("ATLAS_SAMPLE_SECONDS").map(|s| s.parse().expect("seconds are a number"));
    let n: u64 = var("ATLAS_SAMPLE_N").map_or(20, |s| s.parse().expect("N is a number"));
    let openings: usize = var("ATLAS_SAMPLE_OPENINGS").map_or(8, |s| s.parse().expect("openings are a number"));
    let out = var("ATLAS_SAMPLE_OUT").map_or_else(|| std::env::temp_dir().join(format!("atlas-sample-{seed}.json")), PathBuf::from);
    println!("sampling with ATLAS_SAMPLE_SEED={seed}");
    let start = Instant::now();
    let until = seconds.map(|s| SystemTime::now() + Duration::from_secs(s));
    let spent = || seconds.is_some_and(|s| start.elapsed() >= Duration::from_secs(s));

    // each program read once, and asked the laws of reading it
    let (mut programs, mut read_sites) = (vec![], vec![]);
    for path in &repos {
        let t = Instant::now();
        let graph = parts::analyze_repo(&[path.clone()], &Options::default()).unwrap_or_else(|e| panic!("reads {}: {e}", path.display()));
        let mut data = serde_json::to_value(&graph).unwrap();
        data["flow"] = serde_json::to_value(flow::of(&graph)).unwrap();
        let file = std::env::temp_dir().join(format!("atlas-sample-{}-{}.json", std::process::id(), programs.len()));
        std::fs::write(&file, data.to_string()).unwrap();
        let routes: Vec<String> =
            graph.nodes.iter().filter(|n| n.entry.as_ref().is_some_and(|e| e.kind != EntryKind::Test)).map(|n| n.id.clone()).collect();
        let modules: Vec<String> = graph
            .nodes
            .iter()
            .filter(|n| n.kind != NodeKind::External)
            .map(|n| n.module.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let took = t.elapsed();
        println!("{}: {} nodes, {} routes, read in {took:.1?}", path.display(), graph.nodes.len(), routes.len());
        // the laws of reading read the program three times more: asked where the budget leaves room for that
        if seconds.is_none_or(|s| start.elapsed() + 3 * took <= Duration::from_secs(s)) {
            for (law, broke) in reading_laws(path, &graph) {
                read_sites.push(Site { number: u64::MAX, program: programs.len(), law, at: json!({}), broke });
            }
        } else {
            println!("  the laws of reading are not asked: three more readings do not fit the budget");
        }
        programs.push(Program { path: path.clone(), graph, data: file, routes, modules });
    }

    // the sites, drawn and asked a chunk at a time: the views at once, the pictures of one program and budget in one run of
    // the model
    let mut sites: Vec<Site> = vec![];
    let mut number = 0u64;
    'drawing: while !spent() && (seconds.is_some() || number < n) {
        let chunk = if seconds.is_some() { 16 } else { 16.min(n - number) };
        let mut pictures: BTreeMap<(usize, Option<usize>), Vec<(u64, String)>> = BTreeMap::new();
        for _ in 0..chunk {
            let (program, law, at, sets) = draw(seed, number, &programs);
            if law == PICTURES {
                let budget = at["budget"].as_u64().map(|b| b as usize);
                pictures.entry((program, budget)).or_default().push((number, at["route"].as_str().unwrap().to_string()));
            } else {
                let broke = ask_view(&programs[program], &law, &sets).err();
                sites.push(Site { number, program, law, at, broke });
            }
            number += 1;
        }
        for ((program, budget), asked) in pictures {
            if spent() {
                break 'drawing;
            }
            let routes: Vec<String> = asked.iter().map(|(_, r)| r.clone()).collect::<BTreeSet<_>>().into_iter().collect();
            let faults = ask_pictures(&programs[program], &routes, budget, seed, openings, until);
            for (number, route) in asked {
                let Some(fs) = faults.get(&route) else { continue };
                let broke = (!fs.is_empty()).then(|| fs.iter().take(12).cloned().collect::<Vec<_>>().join("\n"));
                sites.push(Site {
                    number,
                    program,
                    law: PICTURES.into(),
                    at: json!({ "route": route, "budget": budget, "openings": openings }),
                    broke,
                });
            }
        }
    }
    sites.sort_by_key(|s| s.number);
    for p in &programs {
        let _ = std::fs::remove_file(&p.data);
    }

    // the report: what was asked where, and what broke
    let atlas = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let all: Vec<&Site> = read_sites.iter().chain(&sites).collect();
    let report = json!({
        "atlas": { "version": env!("CARGO_PKG_VERSION"), "commit": git(&atlas, &["rev-parse", "HEAD"]), "dirty": !git(&atlas, &["status", "--porcelain"]).is_empty() },
        "seed": seed,
        "budget": { "seconds": seconds, "n": if seconds.is_some() { None } else { Some(n) }, "openings": openings, "spent": start.elapsed().as_secs_f64() },
        "programs": programs.iter().map(|p| json!({ "path": p.path, "revision": git(&p.path, &["rev-parse", "HEAD"]), "dirty": !git(&p.path, &["status", "--porcelain"]).is_empty() })).collect::<Vec<_>>(),
        "sites": all.iter().map(|s| json!({
            "site": if s.number == u64::MAX { Value::Null } else { json!(s.number) },
            "program": programs[s.program].path,
            "law": s.law,
            "at": s.at,
            "broke": s.broke,
        })).collect::<Vec<_>>(),
    });
    std::fs::write(&out, serde_json::to_string_pretty(&report).unwrap()).expect("writes the report");

    let mut by_law: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for s in &all {
        let e = by_law.entry(s.law.as_str()).or_default();
        e.0 += 1;
        e.1 += s.broke.is_some() as usize;
    }
    println!("{} sites in {:.1?}, report in {}", all.len(), start.elapsed(), out.display());
    for (law, (asked, broke)) in &by_law {
        println!("  {law}: {asked} asked, {broke} broke");
    }
    let broken: Vec<String> = all
        .iter()
        .filter_map(|s| {
            s.broke.as_ref().map(|b| {
                let again = if s.number == u64::MAX {
                    String::new()
                } else {
                    format!(" (ATLAS_SAMPLE_SEED={seed} ATLAS_SAMPLE_N={})", s.number + 1)
                };
                format!(
                    "site {}{again}, {}: {} at {}\n{b}",
                    if s.number == u64::MAX { "-".into() } else { s.number.to_string() },
                    programs[s.program].path.display(),
                    s.law,
                    s.at
                )
            })
        })
        .collect();
    assert!(
        broken.is_empty(),
        "{} of {} sites broke a law; each is a question, about the law, the code or this sampler:\n\n{}",
        broken.len(),
        all.len(),
        broken.join("\n\n")
    );
}
