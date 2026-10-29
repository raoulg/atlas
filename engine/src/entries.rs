//! Routes into a program: where a web app serves its routes, the entry points a Python project's
//! `pyproject.toml` declares, what the human hands an entry on the command line, Makefile targets.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use regex::Regex;

use crate::builder::GraphBuilder;
use crate::knowledge::{self, Declares};
use crate::model::{Entry, EntryKind, Graph, MakeTarget, NodeKind};
use crate::repo::Repo;

/// Where an app's routes are served: each route by the router it is declared on (`routes`, its
/// node and the router's key), each router under the one it is mounted on with a prefix
/// (`mounted`, by key), and the prefix a router gives its own routes (`own`). A route's path
/// is its prefixes from the app nobody mounts, down to the router it is declared on, then its
/// own; `segment` writes a part of a path as the graph writes one.
pub fn mount_routes(
    store: &mut GraphBuilder,
    routes: &[(String, String)],
    own: &HashMap<String, String>,
    mounted: &HashMap<String, (String, String, bool)>,
    segment: fn(&str) -> String,
) {
    let trim = |p: &str| p.trim_end_matches('/').to_string();
    let mine = |key: &str| trim(own.get(key).map(String::as_str).unwrap_or(""));
    // out through the routers a router is mounted on; a prefix that replaces stands in for the
    // mounted router's own
    let full = |start: &str| {
        let (mut key, mut below) = (start.to_string(), String::new());
        // a router mounted under more routers than this is mounted in a cycle
        const MOUNTED: usize = 10;
        for _ in 0..MOUNTED {
            let Some((parent, prefix, replaces)) = mounted.get(&key) else { break };
            below = format!("{}{}{below}", trim(prefix), if *replaces { String::new() } else { mine(&key) });
            key = parent.clone();
        }
        format!("{}{below}", mine(&key))
    };
    for (id, router) in routes {
        let prefix = full(router);
        let Some(e) = store.nodes.get_mut(id).and_then(|n| n.entry.as_mut()).filter(|e| e.kind == EntryKind::Route) else { continue };
        let Some(mut route) = e.request.clone() else { continue };
        let Some(path) = route.path.as_deref() else { continue };
        let path = if path == "/" && !prefix.is_empty() { "" } else { path };
        let segs: Vec<String> = format!("{prefix}{path}").split('/').map(segment).collect();
        route.path = Some(segs.join("/"));
        e.serve(route);
    }
}

/// An entry point a Python project's `pyproject.toml` declares: the name it is declared by, the
/// object it names (`module:object`), and for a plugin the group its host loads it from.
struct Declared {
    name: String,
    object: String,
    group: Option<String>,
}

/// The entry points a Python project's `pyproject.toml` declares, in every table that declares
/// them (knowledge::ENTRY_TABLES).
fn declared(repo: &Repo, root: &Path) -> Vec<Declared> {
    let Some(doc) = repo.text(root.join("pyproject.toml")).and_then(|t| t.parse::<toml::Table>().ok()) else { return vec![] };
    // the object an entry names, without the extras it asks for: `pkg.cli:main [ui]`
    let object = |v: &toml::Value| {
        let named = match v {
            toml::Value::Table(t) => knowledge::ENTRY_REFERENCES.iter().find_map(|k| t.get(*k)?.as_str()),
            _ => v.as_str(),
        };
        named.map(|o| o.split('[').next().unwrap_or(o).trim().to_string())
    };
    let mut out = vec![];
    for (path, declares) in knowledge::ENTRY_TABLES {
        let Some((first, rest)) = path.split_first() else { continue };
        let Some(table) = doc.get(*first).and_then(|v| rest.iter().try_fold(v, |v, k| v.get(*k))).and_then(|t| t.as_table()) else {
            continue;
        };
        let groups: Vec<(Option<&String>, &toml::Table)> = match declares {
            Declares::Programs => vec![(None, table)],
            Declares::Plugins => table.iter().filter_map(|(g, t)| Some((Some(g), t.as_table()?))).collect(),
        };
        for (group, entries) in groups {
            for (name, v) in entries {
                if let Some(object) = object(v) {
                    out.push(Declared { name: name.clone(), object, group: group.cloned() });
                }
            }
        }
    }
    out
}

/// The entry points a Python project's `pyproject.toml` declares are routes into it: a program
/// is a command run by its name, a plugin a door its host loads, by `group:name`. The declaration
/// says what the function is to its user, so a program is the command also where its package
/// exports the function (a flat module's public `main`), and stays a door as well.
pub fn mark_scripts(repo: &Repo, store: &mut GraphBuilder, project_root: Option<&Path>) {
    let Some(root) = project_root else { return };
    for d in declared(repo, root) {
        let (module, func) = d.object.split_once(':').unwrap_or((&d.object, ""));
        let mut nid = if func.is_empty() { module.to_string() } else { format!("{module}.{func}") };
        if !store.nodes.contains_key(&nid) && !func.is_empty() {
            if let Some(re) = store.bindings.get(module).and_then(|b| b.get(func)) {
                nid = re.clone(); // re-exported main
            }
        }
        // a program runs a function; a host loads a module or a class as well
        let (entry, loads) = match &d.group {
            None => (Entry::new(EntryKind::Cli, d.name), &[NodeKind::Function, NodeKind::Method][..]),
            Some(g) => (
                Entry::new(EntryKind::Export, format!("{g}:{}", d.name)),
                &[NodeKind::Function, NodeKind::Method, NodeKind::Class, NodeKind::Module][..],
            ),
        };
        let Some(n) = store.nodes.get_mut(&nid).filter(|n| loads.contains(&n.kind)) else { continue };
        match n.entry.as_ref().map(|e| e.kind) {
            None => n.entry = Some(entry),
            Some(EntryKind::Export) if d.group.is_none() => {
                n.exported = true;
                n.entry = Some(entry);
            }
            Some(_) => {}
        }
    }
}

// ---------- the command line: what the human hands an entry ----------

/// An option as the human types it, from the flags it is declared with: its longest `--` flag,
/// else its first flag; declared with none, `--` and its name with `_` written `-`.
pub fn flag(flags: &[String], name: &str) -> String {
    let flags: Vec<&String> = flags.iter().filter(|f| f.starts_with('-')).collect();
    let long = flags.iter().filter(|f| f.starts_with("--")).max_by_key(|f| f.len());
    long.or(flags.first()).map(|f| f.to_string()).unwrap_or_else(|| format!("--{}", name.replace('_', "-")))
}

/// A positional argument as a usage line writes it: `<name>`.
pub fn argument(name: &str) -> String {
    format!("<{name}>")
}

/// Makefile targets are the routes a developer actually uses: each becomes a `make:` entry
/// whose steps are the CLI commands and other targets its recipe invokes.
pub fn read_makefile(repo: &Repo, g: &mut Graph) {
    if g.project_root.is_empty() {
        return;
    }
    let root = Path::new(&g.project_root).to_path_buf();
    let Some(text) = repo.text(root.join("Makefile")) else { return };
    let scripts = programs(repo, &root);
    let target_re = Regex::new(r"^([A-Za-z0-9_-]+):(.*)$").unwrap();
    let mut targets: Vec<MakeTarget> = vec![];
    let mut open = false;
    for line in text.lines() {
        // `NAME := value` is an assignment, not a target
        if let Some(m) = target_re.captures(line).filter(|m| !m[2].starts_with('=')) {
            let (deps, doc) = m[2].split_once("##").unwrap_or((&m[2], ""));
            targets.push(MakeTarget {
                id: format!("make:{}", &m[1]),
                label: m[1].to_string(),
                doc: doc.trim().to_string(),
                deps: deps.split_whitespace().map(str::to_string).collect(),
                recipe: vec![],
                calls: vec![],
                module: "Makefile".into(),
                part: String::new(),
            });
            open = true;
        } else if let Some(rest) = line.strip_prefix('\t') {
            if open {
                if let Some(t) = targets.last_mut() {
                    t.recipe.push(rest.to_string());
                }
            }
        } else {
            open = false;
        }
    }
    let by_label: BTreeMap<String, String> = targets.iter().map(|t| (t.label.clone(), t.id.clone())).collect();
    let cli: BTreeMap<String, String> = g
        .nodes
        .iter()
        .filter_map(|n| n.entry.as_ref().filter(|e| e.kind == EntryKind::Cli).map(|e| (e.label.clone(), n.id.clone())))
        .collect();
    // a program the project installs, run as a word or a path's last part, and the subcommand
    // it is handed
    let script_re = if scripts.is_empty() {
        None
    } else {
        let names: Vec<String> = scripts.iter().map(|s| regex::escape(s)).collect();
        Some(Regex::new(&format!(r"(?:^|[\s/])({})(?:\s+([a-z0-9][a-z0-9-]*))?(?:$|[\s;&|)])", names.join("|"))).unwrap())
    };
    let make_re = Regex::new(r"(?:\$\(MAKE\)|\bmake)\s+([A-Za-z0-9_-]+)").unwrap();
    let echo_re = Regex::new(r"^\s*@?echo\b").unwrap();
    for t in targets.iter_mut() {
        let mut seen: Vec<String> = vec![];
        for dep in &t.deps {
            if let Some(id) = by_label.get(dep) {
                if !seen.contains(id) {
                    seen.push(id.clone());
                }
            }
        }
        for line in &t.recipe {
            if echo_re.is_match(line) {
                continue;
            }
            if let Some(re) = &script_re {
                for m in re.captures_iter(line) {
                    // the subcommand when the program has one by that name, else the program
                    let id = m.get(2).and_then(|sub| cli.get(sub.as_str())).or_else(|| cli.get(&m[1]));
                    if let Some(id) = id.filter(|id| !seen.contains(id)) {
                        seen.push(id.clone());
                    }
                }
            }
            for m in make_re.captures_iter(line) {
                if let Some(id) = by_label.get(&m[1]) {
                    if !seen.contains(id) {
                        seen.push(id.clone());
                    }
                }
            }
        }
        t.calls = seen;
    }
    g.make = targets.into_iter().filter(|t| !t.recipe.is_empty() || !t.calls.is_empty()).collect();
}

/// The programs a project installs, by the names a recipe runs them by: the programs a Python
/// project declares, a crate's package and its binaries in `src/bin`, an npm package's `bin`.
fn programs(repo: &Repo, root: &Path) -> Vec<String> {
    let mut out: Vec<String> = declared(repo, root).into_iter().filter(|d| d.group.is_none()).map(|d| d.name).collect();
    if let Some(doc) = repo.text(root.join("Cargo.toml")).and_then(|t| t.parse::<toml::Table>().ok()) {
        out.extend(doc.get("package").and_then(|p| p.get("name")).and_then(|n| n.as_str()).map(str::to_string));
        out.extend(
            repo.files_under(root.join("src/bin"))
                .iter()
                .filter(|p| p.extension().map_or(false, |e| e == "rs"))
                .filter_map(|p| p.file_stem()?.to_str().map(str::to_string)),
        );
    }
    if let Some(json) = repo.text(root.join("package.json")).and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) {
        match &json["bin"] {
            serde_json::Value::String(_) => out.extend(json["name"].as_str().map(str::to_string)),
            serde_json::Value::Object(o) => out.extend(o.keys().cloned()),
            _ => {}
        }
    }
    out.sort();
    out.dedup();
    out
}
