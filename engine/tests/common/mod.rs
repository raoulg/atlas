//! What the pair tests share.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use atlas_engine::decide::{prophetize, Prophetized};
use atlas_engine::revelation::{revelation, Stand};
use atlas_engine::{analyze, Graph, Options};

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
