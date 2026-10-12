//! Routes into a program: decorators, `[project.scripts]`, Makefile targets.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;

use crate::model::{Entry, EntryKind, Graph, MakeTarget, NodeKind};
use crate::builder::GraphBuilder;
use crate::repo::Repo;

fn deco_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"^([\w.]+)\s*(?:\(\s*(?:['"]([^'"]*)['"])?)?"#).unwrap())
}

/// Where an app's routes are served: each route by the router it is declared on (`routes`, its
/// node and the router's key), each router under the one it is mounted on with a prefix
/// (`mounted`, by key), and the prefix a router gives its own routes (`own`). A route's path
/// is its prefixes from the app nobody mounts, down to the router it is declared on, then its
/// own; `segment` writes a part of a path as the graph writes one.
pub fn mount_routes(store: &mut GraphBuilder, routes: &[(String, String)], own: &HashMap<String, String>, mounted: &HashMap<String, (String, String)>, segment: fn(&str) -> String) {
    let trim = |p: &str| p.trim_end_matches('/').to_string();
    let full = |start: &str| {
        let (mut key, mut out) = (start.to_string(), trim(own.get(start).map(String::as_str).unwrap_or("")));
        for _ in 0..10 {
            let Some((parent, prefix)) = mounted.get(&key) else { break };
            out = format!("{}{}{out}", trim(own.get(parent).map(String::as_str).unwrap_or("")), trim(prefix));
            key = parent.clone();
        }
        out
    };
    for (id, router) in routes {
        let prefix = full(router);
        let Some(e) = store.nodes.get_mut(id).and_then(|n| n.entry.as_mut()).filter(|e| e.kind == EntryKind::Route) else { continue };
        let Some((method, path)) = e.label.split_once(' ') else { continue };
        let path = if path == "/" && !prefix.is_empty() { "" } else { path };
        let segs: Vec<String> = format!("{prefix}{path}").split('/').map(segment).collect();
        e.label = format!("{method} {}", segs.join("/"));
    }
}

/// A decorator that makes a function a route into the program.
pub fn entry_of(decorators: &[String], name: &str) -> Option<Entry> {
    for d in decorators {
        let Some(m) = deco_re().captures(d) else { continue };
        let dn = m.get(1).map_or("", |x| x.as_str());
        let arg = m.get(2).map(|x| x.as_str().to_string());
        let parts: Vec<&str> = dn.split('.').collect();
        let last = *parts.last().unwrap_or(&"");
        if last == "command" {
            let label = arg.unwrap_or_else(|| name.replace('_', "-"));
            return Some(Entry { kind: EntryKind::Cli, label });
        }
        if crate::knowledge::registers_route(last) && parts.len() == 2 {
            let label = format!("{} {}", last.to_uppercase(), arg.unwrap_or_default()).trim().to_string();
            return Some(Entry { kind: EntryKind::Route, label });
        }
    }
    None
}

/// `[project.scripts]` in the project's pyproject.toml are entry points too.
pub fn mark_scripts(repo: &Repo, store: &mut GraphBuilder, project_root: Option<&Path>) {
    let Some(root) = project_root else { return };
    let Some(text) = repo.text(root.join("pyproject.toml")) else { return };
    let Ok(doc) = text.parse::<toml::Table>() else { return };
    let Some(table) = doc.get("project").and_then(|p| p.get("scripts")).and_then(|s| s.as_table()) else {
        return;
    };
    for (name, target) in table {
        let Some(target) = target.as_str() else { continue };
        let (module, func) = target.split_once(':').unwrap_or((target, ""));
        let mut nid = if func.is_empty() { module.to_string() } else { format!("{module}.{func}") };
        if !store.nodes.contains_key(&nid) && !func.is_empty() {
            if let Some(re) = store.bindings.get(module).and_then(|b| b.get(func)) {
                nid = re.clone(); // re-exported main
            }
        }
        if let Some(n) = store.nodes.get_mut(&nid) {
            if matches!(n.kind, NodeKind::Function | NodeKind::Method) {
                if n.entry.is_none() {
                    n.entry = Some(Entry { kind: EntryKind::Cli, label: name.clone() });
                }
            }
        }
    }
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

/// The programs a project installs, by the names a recipe runs them by: a Python project's
/// `[project.scripts]`, a crate's package and its binaries in `src/bin`, an npm package's `bin`.
fn programs(repo: &Repo, root: &Path) -> Vec<String> {
    let toml = |file: &str| repo.text(root.join(file)).and_then(|t| t.parse::<toml::Table>().ok());
    let mut out: Vec<String> = vec![];
    if let Some(doc) = toml("pyproject.toml") {
        out.extend(doc.get("project").and_then(|p| p.get("scripts")).and_then(|s| s.as_table()).map(|t| t.keys().cloned().collect::<Vec<_>>()).unwrap_or_default());
    }
    if let Some(doc) = toml("Cargo.toml") {
        out.extend(doc.get("package").and_then(|p| p.get("name")).and_then(|n| n.as_str()).map(str::to_string));
        out.extend(repo.files_under(root.join("src/bin")).iter().filter(|p| p.extension().map_or(false, |e| e == "rs")).filter_map(|p| p.file_stem()?.to_str().map(str::to_string)));
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
