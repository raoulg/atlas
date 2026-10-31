//! What the pair tests share.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use atlas_engine::decide::{prophetize, Prophetized};
use atlas_engine::revelation::{revelation, Stand};
use atlas_engine::{analyze, flow, Both, Graph, Options};

/// Callers each of whom makes a path whole alone, the disjunction `a ∨ b` as `Touch::by` holds it.
#[allow(dead_code)]
pub fn by<const N: usize>(callers: [&str; N]) -> BTreeSet<Both> {
    callers.into_iter().map(Both::one).collect()
}

/// A pair below `engine/tests/pairs`: the same program once as good practice writes it and once
/// with the usual shortcut.
#[allow(dead_code)]
pub struct Pair {
    pub clean: Graph,
    pub shortcut: Graph,
}

/// The pair at `dir` below `engine/tests/pairs` (`lens/python/club`), both its sides read.
#[allow(dead_code)]
pub fn pair(dir: &str) -> Pair {
    let side = |name: &str| {
        let at = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs").join(dir).join(name);
        analyze(&at, &Options::default()).unwrap_or_else(|e| panic!("reads {}: {e}", at.display()))
    };
    Pair { clean: side("clean"), shortcut: side("shortcut") }
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("makes the directory");
    for e in std::fs::read_dir(from).expect("reads the directory").flatten() {
        let dest = to.join(e.file_name());
        if e.path().is_dir() {
            copy_tree(&e.path(), &dest);
        } else {
            std::fs::copy(e.path(), &dest).expect("copies the file");
        }
    }
}

/// The `clean` side of a prophecy pair, copied to a directory of its own named after `name`, that
/// the caller removes with its parent.
#[allow(dead_code)]
pub fn clean_copy(language: &str, name: &str) -> PathBuf {
    let pair = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/prophecy").join(language).join("clean");
    let copy = std::env::temp_dir().join(format!("atlas-{name}-{language}-{}", std::process::id())).join("clean");
    let _ = std::fs::remove_dir_all(copy.parent().expect("has a parent"));
    copy_tree(&pair, &copy);
    copy
}

/// The `clean` side of a prophecy pair with `addition` appended to its file `file`, as a vision
/// that is decided: what `prophetize` writes, and the revelation of the tree it is written into.
/// The pair itself stays as it is; the tree is a copy.
pub fn decided(language: &str, file: &str, addition: &str, supersedes: &[(&str, &str)]) -> (Prophetized, Vec<Stand>) {
    let pair = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/prophecy").join(language).join("clean");
    let copy = std::env::temp_dir().join(format!("atlas-prophetize-{language}-{}", std::process::id())).join("clean");
    let _ = std::fs::remove_dir_all(copy.parent().expect("has a parent"));
    copy_tree(&pair, &copy);
    let now = std::fs::read_to_string(copy.join(file)).expect("reads the file");
    let vision = BTreeMap::from([(PathBuf::from(file), Some(format!("{now}{addition}")))]);
    let supersedes: BTreeMap<String, String> = supersedes.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
    let decided = prophetize(&copy, &vision, &supersedes, &Options::default()).expect("a vision that only adds is decided");
    for (rel, text) in &decided.files {
        std::fs::write(copy.join(rel), text).expect("writes the file");
    }
    let stands = revelation(&analyze(&copy, &Options::default()).expect("reads"));
    let _ = std::fs::remove_dir_all(copy.parent().expect("has a parent"));
    (decided, stands)
}

/// The `clean` side of a prophecy pair with `addition` appended to its file `file`, written to a
/// copy of its own that the caller removes: the copy, and what Atlas reads of it.
pub fn written(language: &str, file: &str, addition: &str) -> (PathBuf, atlas_engine::Graph) {
    let pair = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/prophecy").join(language).join("clean");
    let copy = std::env::temp_dir().join(format!("atlas-sealed-{language}-{}", std::process::id())).join("clean");
    let _ = std::fs::remove_dir_all(copy.parent().expect("has a parent"));
    copy_tree(&pair, &copy);
    let now = std::fs::read_to_string(copy.join(file)).expect("reads the file");
    std::fs::write(copy.join(file), format!("{now}{addition}")).expect("writes the file");
    let g = analyze(&copy, &Options::default()).expect("reads");
    (copy, g)
}

/// Of the function `id`, each parameter by its name, with the types among `types` its reader reads its type as holding
/// and the types the body's own code reads it as: how the tables pair (`tests/pairs/tables`) tells a table from the
/// lists it is cut into.
#[allow(dead_code)]
pub fn held_by_parameters(g: &Graph, id: &str, types: &[&str]) -> Vec<(String, Vec<String>, Vec<String>)> {
    let f = g.node(id).unwrap_or_else(|| panic!("no node {id}"));
    let short = |t: &str| t.rsplit('.').next().unwrap_or(t).to_string();
    f.params
        .iter()
        .filter(|p| !p.receiver)
        .map(|p| {
            let holds = types.iter().filter(|t| p.holds_type(t)).map(|t| t.to_string()).collect();
            let read = f.uses.iter().filter(|u| u.name == p.name && !u.ty.is_empty()).map(|u| short(&u.ty)).collect();
            (p.name.clone(), holds, read)
        })
        .collect()
}

/// The tables pair of `language`: the clean side looks a Plan up in one table by its Mode, which holds Plans; the
/// shortcut cuts the table into a list of modes and a list of plans, two values (#167).
#[allow(dead_code)]
pub fn a_table_is_one_value(language: &str, id: &str) {
    let tables = pair(&format!("tables/{language}"));
    let held = |g: &Graph| -> Vec<(String, Vec<String>, Vec<String>)> { held_by_parameters(g, id, &["Mode", "Plan"]) };
    let s = |xs: &[&str]| xs.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    assert_eq!(
        held(&tables.clean),
        [("plans".to_string(), s(&["Plan"]), s(&["Plan"])), ("mode".to_string(), s(&["Mode"]), s(&["Mode"]))],
        "the table holds Plans by their Mode: one value"
    );
    assert_eq!(
        held(&tables.shortcut),
        [
            ("modes".to_string(), s(&["Mode"]), s(&[])),
            ("plans".to_string(), s(&["Plan"]), s(&["Plan"])),
            ("mode".to_string(), s(&["Mode"]), s(&[]))
        ],
        "the modes and the plans are two values"
    );
}

/// The choices pair of `language`: what a mode means is one table the module keeps (`PLANS`), which the route's own code looks
/// its mode up in, a value from beyond of the Plans the table holds by their Mode; the shortcut keeps no table and tests the
/// mode in each function that needs it. A body's own code reads the module's table alike in every language (#171): each name
/// the route `id` reads that is none of its parameters, by the type it holds.
#[allow(dead_code)]
pub fn a_module_s_table_is_a_value_the_body_reads(language: &str, id: &str) {
    let choices = pair(&format!("choices/{language}"));
    let modular = |g: &Graph| -> Vec<(String, String)> {
        let f = g.node(id).unwrap_or_else(|| panic!("no node {id}"));
        let declared = |name: &str| f.params.iter().any(|p| p.name == name);
        let short = |t: &str| t.rsplit('.').next().unwrap_or(t).to_string();
        f.uses
            .iter()
            .filter(|u| !u.ty.is_empty() && !u.name.is_empty() && !declared(&u.name))
            .map(|u| (u.name.clone(), short(&u.ty)))
            .collect()
    };
    assert_eq!(modular(&choices.clean), [("PLANS".to_string(), "Plan".to_string())], "the route looks its mode up in the module's table");
    assert!(modular(&choices.shortcut).iter().all(|(n, _)| n != "PLANS"), "the shortcut keeps no table");
}

/// The choices pair of `language`, its functions in the module `module`: `run` hands `report` the settings it is handed
/// through their field `level` and `finished` through `mode` (`report(job, settings.level, finished(job, settings.mode))`).
/// A call is handed the value a projection is of (#121), and the graph keeps the field it reads beside it, the same in every
/// language (#172): which parameter of `report` and `finished` the settings fill, and through which field.
#[allow(dead_code)]
pub fn a_call_names_the_field_a_projection_reads(language: &str, module: &str) {
    let choices = pair(&format!("choices/{language}"));
    let run = choices.clean.node(&format!("{module}.run")).unwrap_or_else(|| panic!("no node {module}.run"));
    let through = |to: &str| -> Vec<(atlas_engine::Slot, String)> {
        let to = format!("{module}.{to}");
        let settings = run.sites.iter().filter(|s| s.to == to).flat_map(|s| &s.hands).filter(|h| h.name == "settings");
        settings.flat_map(|h| h.through.clone()).collect()
    };
    let at = |n: u32, field: &str| vec![(atlas_engine::Slot::At(n), field.to_string())];
    assert_eq!(through("report"), at(1, "level"), "report's level is the settings' level");
    assert_eq!(through("finished"), at(1, "mode"), "finished's mode is the settings' mode");
}

/// The choices pair of `language`, its functions in the module `module`: the shortcut's `Runner.batch` chooses by a field of the
/// instance it runs on (`self.mode == "parallel"`), `retries` by the parameter it is handed whole (`mode == "parallel"`). A
/// condition names the field it tests (`Node::tests`, #184), the same in every language: `mode` of the instance for `batch`,
/// no field for `retries`.
#[allow(dead_code)]
pub fn a_condition_names_the_field_it_tests(language: &str, module: &str) {
    let choices = pair(&format!("choices/{language}"));
    let tests = |id: &str| -> Vec<(String, String)> {
        let n = choices.shortcut.node(&format!("{module}.{id}")).unwrap_or_else(|| panic!("no node {module}.{id}"));
        n.tests.iter().map(|t| (t.on.ty.clone(), t.field.clone())).collect()
    };
    assert_eq!(tests("Runner.batch"), [(format!("{module}.Runner"), "mode".to_string())], "a field of the instance");
    assert_eq!(tests("retries"), [], "a value read whole names no field");
}

/// The projections pair of `language` (`tests/pairs/projections`), its classes in the module `module`: a chart lends another
/// its title and its width, handing each as a field of its own (`other.take(self.title, self.size.width)`), or binding them to
/// names first (`t = self.title; s = self.size; other.take(t, s.width)`). A projection bound to a name first is still the
/// projection (#121, #174): both forms fill `take`'s parameters through the same fields, the name's own after those it is
/// bound to, in every language.
#[allow(dead_code)]
pub fn a_projection_bound_to_a_name_is_still_the_projection(language: &str, module: &str) {
    let projections = pair(&format!("projections/{language}"));
    let through = |g: &Graph| -> Vec<(atlas_engine::Slot, String)> {
        let lend = g.node(&format!("{module}.Chart.lend")).unwrap_or_else(|| panic!("no node {module}.Chart.lend"));
        let take = format!("{module}.Chart.take");
        lend.sites.iter().filter(|s| s.to == take).flat_map(|s| &s.hands).flat_map(|h| h.through.clone()).collect()
    };
    let fields = vec![(atlas_engine::Slot::At(0), "title".to_string()), (atlas_engine::Slot::At(1), "size.width".to_string())];
    assert_eq!(through(&projections.clean), fields, "handed as fields of its own");
    assert_eq!(through(&projections.shortcut), fields, "bound to names first");
}

/// The facts of the finest plan the viewer's model builds of every route of `g`, as `tests/plans.rs` writes them
/// (`tests/plans.mjs`, run in node): with a `budget`, of the plan cut as a large program's is (`ATLAS_FINE_BUDGET`).
#[allow(dead_code)]
pub fn planned(g: &Graph, name: &str, budget: Option<usize>) -> BTreeMap<String, BTreeSet<String>> {
    let mut data = serde_json::to_value(g).expect("the graph is data");
    data["flow"] = serde_json::to_value(flow::of(g)).expect("the flow is data");
    let file = std::env::temp_dir().join(format!("atlas-planned-{}-{name}-{budget:?}.json", std::process::id()));
    std::fs::write(&file, data.to_string()).expect("writes the data");
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
    assert!(out.status.success(), "{name}: the model fails\n{}", String::from_utf8_lossy(&out.stderr));
    let read: BTreeMap<String, serde_json::Value> = serde_json::from_slice(&out.stdout).expect("the model prints JSON");
    read.into_iter()
        .map(|(r, v)| (r, v["facts"].as_array().into_iter().flatten().filter_map(|x| x.as_str().map(String::from)).collect()))
        .collect()
}

/// The budgets a route is planned at: uncut, and cut as a large program's plan is cut.
#[allow(dead_code)]
pub const BUDGETS: [Option<usize>; 4] = [None, Some(1), Some(2), Some(3)];

/// What the plan of a route takes from beyond it that is none of the route's own parameters (`params`), by type and name:
/// each value a line takes that no occurrence made, so that it has no moment (`docs/reference/mathematics.md`, "One fine
/// plan, every picture its image": a name its function does not declare, a module's constant, is what the module holds).
#[allow(dead_code)]
pub fn from_beyond(facts: &BTreeSet<String>, params: &[String]) -> BTreeSet<String> {
    facts
        .iter()
        .filter_map(|f| f.strip_prefix("line ")?.split(" > ").next())
        .filter(|v| !v.contains('@') && !v.contains(' '))
        .filter(|v| v.split_once('·').is_some_and(|(_, name)| !params.iter().any(|p| p == name.split('#').next().unwrap_or(name))))
        .map(String::from)
        .collect()
}

/// The parameters of the function `id`, by name.
#[allow(dead_code)]
pub fn parameters(g: &Graph, id: &str) -> Vec<String> {
    g.node(id).unwrap_or_else(|| panic!("no node {id}")).params.iter().filter(|p| !p.receiver).map(|p| p.name.clone()).collect()
}

/// What each step of a route's closed picture takes from beyond the route, by the value and the step: the values a line
/// takes that no occurrence made, but the route's own parameters (`from_beyond`), each with the step of the route the line
/// ends in or below (`docs/reference/mathematics.md`, "The closed picture is the pushforward of the fine one").
#[allow(dead_code)]
pub fn taken_from_beyond(facts: &BTreeSet<String>, params: &[String]) -> BTreeSet<(String, String)> {
    let beyond = from_beyond(facts, params);
    facts
        .iter()
        .filter_map(|f| f.strip_prefix("line ")?.split_once(" > "))
        .filter(|(v, _)| beyond.contains(*v))
        .map(|(v, to)| {
            let at = [" reached", " value"].iter().fold(to, |at, channel| at.strip_suffix(channel).unwrap_or(at));
            (v.to_string(), at.split('/').take(2).collect::<Vec<_>>().join("/"))
        })
        .collect()
}

/// The recursion pair of `language` (`tests/pairs/recursion`): `split` takes a node's own items from `items_of` and the rest
/// from `below`, which splits each child in turn, so that `below` calls `split` again, a call into a function the route is
/// already inside; `climb` enters a recursion of three (`deeper`, `rest`, `kids`) at two of its functions. Every value is
/// made by the body that names it: a recursive occurrence stays closed, a step whose body is drawn in the frame it stands in,
/// so it takes only what its call hands it (the child, the floor) and none of the values the body that holds it made by other
/// names (no line from the outer call's `mine` into the inner call), and no value the bodies make comes from beyond the route
/// (#237). The shortcut reads the module's floor (`FLOOR`), which alone comes from beyond. So uncut and at every budget the
/// plan is cut at, in every language; and the cut changes how much of the plan is held, never its picture: each step of the
/// closed route takes from beyond what it takes uncut, also where the function a step beyond the cut calls is on the path of
/// one occurrence of it and not of another (`climb`).
#[allow(dead_code)]
pub fn a_recursive_call_takes_what_its_call_hands_it(language: &str) {
    let p = pair(&format!("recursion/{language}"));
    for (side, g, beyond) in [("clean", &p.clean, vec![]), ("shortcut", &p.shortcut, vec!["Floor·FLOOR".to_string()])] {
        let want: BTreeSet<String> = beyond.into_iter().collect();
        let mut uncut = BTreeMap::new();
        for budget in BUDGETS {
            let at = format!("{language}, budget {budget:?}, {side}");
            let plans = planned(g, &format!("recursion-{language}-{side}"), budget);
            for route in ["tree.split", "tree.climb"] {
                let (facts, params) = (&plans[route], parameters(g, route));
                assert_eq!(from_beyond(facts, &params), want, "{at}: what comes from beyond {route}");
                let taken = taken_from_beyond(facts, &params);
                match budget {
                    None => drop(uncut.insert(route, taken)),
                    Some(_) => assert_eq!(taken, uncut[route], "{at}: what each step of {route} takes from beyond, cut and uncut"),
                }
                for r in facts.iter().filter_map(|f| f.strip_prefix("step ")?.strip_suffix(" recursive")) {
                    // what the call into the function the route is inside hands it, by name: the call in the body of the
                    // step around it, through the choices between them
                    let segs: Vec<&str> = r.split('/').collect();
                    let (callee, caller) = (segs[segs.len() - 1], segs[..segs.len() - 1].iter().rev().find(|s| !s.starts_with('?')));
                    let caller = g.node(&format!("tree.{}", caller.expect("a caller"))).unwrap_or_else(|| panic!("{at}: no caller of {r}"));
                    let handed: BTreeSet<&str> = caller
                        .sites
                        .iter()
                        .filter(|s| s.to == format!("tree.{callee}"))
                        .flat_map(|s| &s.hands)
                        .map(|h| h.name.as_str())
                        .collect();
                    let taken: BTreeSet<&str> = facts
                        .iter()
                        .filter_map(|f| f.strip_prefix("line ")?.strip_suffix(&format!(" > {r} value")))
                        .map(|v| v.split_once('·').map_or(v, |(_, n)| n).split(['@', '#']).next().unwrap_or(v))
                        .collect();
                    assert!(!taken.is_empty() && taken.is_subset(&handed), "{at}: {r} takes {taken:?}, its call hands it {handed:?}");
                }
            }
        }
    }
}

/// The literals pair of `language` (`tests/pairs/literals`): a list gathered by a function the body hands it to (`collect_into`),
/// made empty by a literal in `literal` (`vec![]`, `[]`) and by a call handed nothing in `call` (`Vec::new()`, `list()`, `new
/// Array<Item>()`). The two forms say the same thing, and Atlas draws them alike (CLAUDE.md, "Isomorphic is the same"): the graph
/// says that the body binds the name by a literal (`Node::binds`), the plan holds the value the body made in the route's
/// frame, and the plan of one is the plan of the other once the call that takes nothing and makes the list is contracted into
/// the list it makes (a homotopy, `mathematics.md`, rule 6). Nothing comes from beyond either route. The shortcut seeds the
/// list from the module's (`SEED`), which alone comes from beyond (#237). So in every language, and what comes from beyond
/// also at every budget the plan is cut at.
#[allow(dead_code)]
pub fn a_list_made_by_a_literal_is_made_by_the_body(language: &str, literal: &str, call: &str, constant: &str) {
    let p = pair(&format!("literals/{language}"));
    let binds = |g: &Graph, f: &str| -> Vec<(String, usize)> {
        g.node(&format!("gather.{f}"))
            .unwrap_or_else(|| panic!("no node gather.{f}"))
            .binds
            .iter()
            .map(|b| (b.name.clone(), b.arms.len()))
            .collect()
    };
    assert_eq!(binds(&p.clean, literal), [("items".to_string(), 0)], "{language}: the body binds the list by a literal");
    assert_eq!(binds(&p.clean, call), [], "{language}: a step makes the list");
    assert_eq!(binds(&p.shortcut, constant), [], "{language}: the list is the module's");
    for budget in BUDGETS {
        let at = format!("{language}, budget {budget:?}");
        let clean = planned(&p.clean, &format!("literals-{language}-clean"), budget);
        let (by_literal, by_call) = (&clean[&format!("gather.{literal}")], &clean[&format!("gather.{call}")]);
        assert_eq!(from_beyond(by_literal, &parameters(&p.clean, &format!("gather.{literal}"))), BTreeSet::new(), "{at}: {literal}");
        assert_eq!(from_beyond(by_call, &parameters(&p.clean, &format!("gather.{call}"))), BTreeSet::new(), "{at}: {call}");
        let shortcut = &planned(&p.shortcut, &format!("literals-{language}-shortcut"), budget)[&format!("gather.{constant}")];
        let want: BTreeSet<String> = ["Item·SEED".to_string()].into_iter().collect();
        assert_eq!(from_beyond(shortcut, &parameters(&p.shortcut, &format!("gather.{constant}"))), want, "{at}: {constant}");
        // the call that takes nothing and makes the list, contracted into the list it makes; a cut depends on how many occurrences
        // a plan holds, which the call adds, so the two plans are held to each other uncut
        if budget.is_some() {
            continue;
        }
        let lines_into = |s: &str| by_call.iter().any(|f| f.starts_with("line ") && f.ends_with(&format!(" > {s} value")));
        let makers: Vec<&str> = by_call
            .iter()
            .filter_map(|f| f.strip_prefix("step "))
            .filter(|s| {
                !lines_into(s) && by_call.iter().any(|f| f.starts_with(&format!("line {s} > ")) && f.contains(&format!("·items@{s} ")))
            })
            .collect();
        let [maker] = makers[..] else { panic!("{at}: one call makes the list in {call}: {makers:?}") };
        let contracted: BTreeSet<String> = by_call
            .iter()
            .filter(|f| *f != &format!("step {maker}") && !f.starts_with(&format!("line {maker} > ")))
            .map(|f| f.replace(&format!("@{maker} "), &format!("@{call}/~items ")).replace(call, literal))
            .collect();
        assert_eq!(by_literal, &contracted, "{at}: {literal} draws as {call} does");
    }
}

/// The locals pair of `language` (`tests/pairs/locals`): `pair` binds `both` to a list it writes of what it is handed, an
/// expression no step makes that is no name and no projection of one. A local is bound by the body's own code whatever
/// expression binds it, in every language (CLAUDE.md, "Isomorphic is the same"): the graph says the body binds it
/// (`Node::binds`), and it is never a module's name, so nothing comes from beyond the clean route. The shortcut writes the
/// module's quiet level into the list in place of a second it is handed: that level alone comes from beyond (#318). So at
/// every budget the plan is cut at.
#[allow(dead_code)]
pub fn a_local_is_bound_by_the_bodys_own_code(language: &str, pair_fn: &str) {
    let p = pair(&format!("locals/{language}"));
    for (side, g, beyond) in [("clean", &p.clean, vec![]), ("shortcut", &p.shortcut, vec!["Level·QUIET".to_string()])] {
        let leaf = g.node(&format!("signal.{pair_fn}")).unwrap_or_else(|| panic!("{language} {side}: no node signal.{pair_fn}"));
        let binds: Vec<(&str, usize)> = leaf.binds.iter().map(|b| (b.name.as_str(), b.arms.len())).collect();
        assert_eq!(binds, [("both", 0)], "{language} {side}: the body binds the list by its own code");
        let want: BTreeSet<String> = beyond.into_iter().collect();
        for budget in BUDGETS {
            let plans = planned(g, &format!("locals-{language}-{side}"), budget);
            let route = &plans["signal.paired"];
            assert_eq!(from_beyond(route, &parameters(g, "signal.paired")), want, "{language} {side}, budget {budget:?}: from beyond");
        }
    }
}

/// The parents pair of `language`: `summarise` (`read`) reads the data file `~/data/{name}` and hands its path to `log` (`log`),
/// which writes the log in the directory the file lies in. The clean side takes that directory with the path library
/// (`Path.parent`, `Path::parent`, `path.dirname`), which all go to one function on the graph, and the directory of a path is
/// the path without its last part, also where that part is a value the code does not say: the log rests in the directory the
/// data file is read from (#284). The shortcut cuts the path's text at its last `/`, which no code says is a directory: the log
/// is written, in a directory the code does not name, and never in the data files' directory. So in every language.
///
/// Both sides add to the log (`OpenOptions::new().append(true).create(true).open(..)`, `open(.., "a")`,
/// `appendFileSync`): an append writes what the file holds in each language, and the path its opening call is handed is
/// followed as a write's is (#300). A touch is named by the path its call is handed as the code builds it, so where that
/// path is fully known it names the place itself, never a literal or an expression of the call (#301).
#[allow(dead_code)]
pub fn a_parent_drops_the_last_part_of_a_path(language: &str, read: &str, log: &str) {
    let p = pair(&format!("parents/{language}"));
    let places = |g: &Graph, id: &str, kind: &str| -> Vec<String> {
        let n = g.node(id).unwrap_or_else(|| panic!("{language}: no node {id}"));
        n.touches.iter().filter(|t| t.kind_name() == kind).map(|t| t.artifact.clone()).collect()
    };
    for (side, g) in [("clean", &p.clean), ("shortcut", &p.shortcut)] {
        let read = places(g, read, "disk_read");
        let [dir] = &read[..] else { panic!("{language} {side}: one directory the data file is read from: {read:?}") };
        assert!(dir.ends_with('/') && dir.len() > 1, "{language} {side}: the data file's name is not said, its directory is: {dir}");
        let written = places(g, log, "disk_write");
        assert_eq!(written.len(), 1, "{language} {side}: the log is written once: {written:?}");
        let beside = written[0].strip_prefix(dir.as_str()).is_some_and(|name| !name.contains('/'));
        assert_eq!(beside, side == "clean", "{language} {side}: the log {written:?} in the data files' directory {dir}");
        let node = g.node(log).unwrap_or_else(|| panic!("{language}: no node {log}"));
        let t = node.touches.iter().find(|t| t.kind_name() == "disk_write").expect("the log is written");
        let op = atlas_engine::effects::fs_op(&t.op);
        assert_eq!(op, atlas_engine::effects::FsOp::Data, "{language} {side}: the append `{}` writes what the log holds", t.op);
        if !t.artifact.contains('{') {
            let named = atlas_engine::paths::place(&vec![atlas_engine::paths::Seg::Lit(t.what.clone())], &t.op);
            assert_eq!(
                named.as_ref(),
                Some(&t.artifact),
                "{language} {side}: the log at a known place is named by it, not by `{}`",
                t.what
            );
        }
    }
}

/// The closures pair: a function written in a body (a closure, a lambda, a `def` inside a `def`) and called there by the
/// name the body keeps it by is a function, and its parameter is what each call hands it, as a module's function's
/// parameter is what its callers hand it (#239, CLAUDE.md, "Isomorphic is the same"). The clean side reads a project's
/// settings through a function of the module, `text(root, "knot.toml")`; the shortcut writes the same function in the
/// body, `text("knot.toml")`. Each of `functions` reads the same files on both sides: the files the calls name, never a
/// hole named by the parameter.
#[allow(dead_code)]
pub fn a_closures_parameter_is_what_its_calls_hand_it(language: &str, functions: &[&str]) {
    let p = pair(&format!("closures/{language}"));
    // the files a function reads: its own touches, and those it hands a callee the place of
    let read = |g: &Graph, f: &str| -> BTreeSet<String> {
        g.nodes
            .iter()
            .flat_map(|n| n.touches.iter().filter(move |t| n.id == f || t.by.contains(&Both::one(f))))
            .filter(|t| t.kind_name() == "disk_read")
            .map(|t| t.artifact.clone())
            .collect()
    };
    for f in functions {
        let (clean, shortcut) = (read(&p.clean, f), read(&p.shortcut, f));
        assert!(!clean.is_empty() && clean.iter().all(|a| !a.contains("{name}")), "{language}: {f} reads named files: {clean:?}");
        assert_eq!(shortcut, clean, "{language}: {f} reads what a function of the module reads");
    }
}

/// The locals pair of `language` (`tests/pairs/locals`), its routes `named` and `written`: `named` binds the gap between two
/// levels and the two side by side to names before it hands each to a step (`gap = first.db - second.db; draw(gap)`, `both =
/// [first, second]; show(both)`), and `written` writes each where its step is handed it. What the language's own code
/// computes of names with no call has no moment of its own, and binding it to a name adds no arrow: the name stands for the
/// names it is computed of (#328, `mathematics.md`, "One fine plan"), and is still the body's own (`Node::binds`). So each
/// step of `named` is handed what the same step of `written` is handed, by the same parameters and through the same fields;
/// and in the plan, uncut and at every budget, each step of one route takes what the same step of the other takes. The
/// shortcut computes both of the module's quiet level in place of the second level it is handed: that level alone comes
/// from beyond, and every step of either route takes it. So in every language. What a route's own arrow takes where its
/// code computes the value (`Steps::read_here`) is no step's, and not held here.
#[allow(dead_code)]
pub fn a_name_computed_of_names_stands_for_them(language: &str) {
    let p = pair(&format!("locals/{language}"));
    // a fact of a route with the route's name written as `R`, so that the facts of two routes can be compared
    let renamed = |fact: &str, route: &str| -> String {
        let moment = |m: &str| match m.strip_prefix(route) {
            Some("") => "R".to_string(),
            Some(rest) if rest.starts_with('/') => format!("R{rest}"),
            _ => m.to_string(),
        };
        let token = |t: &str| match t.split_once('@') {
            Some((value, m)) => format!("{value}@{}", moment(m)),
            None => moment(t),
        };
        fact.split(' ').map(token).collect::<Vec<_>>().join(" ")
    };
    for (side, g, beyond) in [("clean", &p.clean, vec![]), ("shortcut", &p.shortcut, vec!["Level·QUIET".to_string()])] {
        let at = format!("{language} {side}");
        let node = |f: &str| g.node(&format!("signal.{f}")).unwrap_or_else(|| panic!("{at}: no node signal.{f}"));
        let binds: BTreeSet<(&str, usize)> = node("named").binds.iter().map(|b| (b.name.as_str(), b.arms.len())).collect();
        assert_eq!(binds, BTreeSet::from([("both", 0), ("gap", 0)]), "{at}: the body binds both names by its own code");
        let handed = |f: &str| -> Vec<(String, BTreeSet<String>)> {
            let hand = |h: &atlas_engine::Held| format!("{}·{} {:?} {:?} {}", h.ty, h.name, h.fills, h.through, h.param);
            node(f).sites.iter().map(|s| (s.to.clone(), s.hands.iter().map(hand).collect())).collect()
        };
        let named = handed("named");
        assert!(named.len() == 2 && named.iter().all(|(_, h)| !h.is_empty()), "{at}: each step of named is handed values: {named:?}");
        assert_eq!(named, handed("written"), "{at}: a step handed a name is handed what the value written in its place hands");
        let want: BTreeSet<String> = beyond.into_iter().collect();
        for budget in BUDGETS {
            let at = format!("{at}, budget {budget:?}");
            let plans = planned(g, &format!("computed-{language}-{side}"), budget);
            let route = |r: &str| &plans[&format!("signal.{r}")];
            // what the steps take, and what the route draws but its own arrow
            let of_steps = |r: &str| -> BTreeSet<String> {
                route(r).iter().filter(|f| !f.ends_with(&format!(" > {r} value"))).map(|f| renamed(f, r)).collect()
            };
            assert_eq!(of_steps("named"), of_steps("written"), "{at}: each step takes what the value written in its place hands");
            for r in ["named", "written"] {
                let params = parameters(g, &format!("signal.{r}"));
                assert_eq!(from_beyond(route(r), &params), want, "{at}: {r} from beyond");
                let steps: Vec<String> = route(r).iter().filter_map(|f| f.strip_prefix("step ")).map(String::from).collect();
                let taken: BTreeSet<(String, String)> = taken_from_beyond(route(r), &params).into_iter().filter(|(_, s)| s != r).collect();
                let every: BTreeSet<(String, String)> =
                    want.iter().flat_map(|v| steps.iter().map(move |s| (v.clone(), s.clone()))).collect();
                assert_eq!(taken, every, "{at}: {r}'s steps take from beyond");
            }
        }
    }
}
