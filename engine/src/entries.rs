//! Routes into a program: decorators, `[project.scripts]`, Makefile targets.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;

use crate::model::{Entry, EntryKind, MakeTarget, NodeKind};
use crate::store::Store;

const HTTP_METHODS: &[&str] = &["get", "post", "put", "delete", "patch", "websocket"];

fn deco_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"^([\w.]+)\s*(?:\(\s*(?:['"]([^'"]*)['"])?)?"#).unwrap())
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
            let label = arg.unwrap_or_else(|| name.strip_prefix("cmd_").unwrap_or(name).replace('_', "-"));
            return Some(Entry { kind: EntryKind::Cli, label });
        }
        if HTTP_METHODS.contains(&last) && parts.len() == 2 {
            let label = format!("{} {}", last.to_uppercase(), arg.unwrap_or_default()).trim().to_string();
            return Some(Entry { kind: EntryKind::Route, label });
        }
        if ["tool", "resource", "prompt"].contains(&last) && dn.to_lowercase().contains("mcp") {
            return Some(Entry { kind: EntryKind::Mcp, label: arg.unwrap_or_else(|| name.to_string()) });
        }
    }
    None
}

/// `[project.scripts]` in the project's pyproject.toml are entry points too.
/// Returns script name -> node id.
pub fn mark_scripts(store: &mut Store, project_root: Option<&Path>) -> BTreeMap<String, String> {
    let mut scripts = BTreeMap::new();
    let Some(root) = project_root else { return scripts };
    let Ok(text) = std::fs::read_to_string(root.join("pyproject.toml")) else { return scripts };
    let Ok(doc) = text.parse::<toml::Table>() else { return scripts };
    let Some(table) = doc.get("project").and_then(|p| p.get("scripts")).and_then(|s| s.as_table()) else {
        return scripts;
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
                scripts.insert(name.clone(), nid.clone());
            }
        }
    }
    scripts
}

/// Makefile targets are the routes a developer actually uses: each becomes a `make:` entry
/// whose steps are the CLI commands and other targets its recipe invokes.
pub fn read_makefile(store: &Store, project_root: Option<&Path>, scripts: &BTreeMap<String, String>) -> Vec<MakeTarget> {
    let Some(root) = project_root else { return vec![] };
    let Ok(text) = std::fs::read_to_string(root.join("Makefile")) else { return vec![] };
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
    let cli: BTreeMap<String, String> = store
        .nodes
        .values()
        .filter_map(|n| n.entry.as_ref().filter(|e| e.kind == EntryKind::Cli).map(|e| (e.label.clone(), n.id.clone())))
        .collect();
    let script_re = if scripts.is_empty() {
        None
    } else {
        let names: Vec<String> = scripts.keys().map(|s| regex::escape(s)).collect();
        Some(Regex::new(&format!(r"\b(?:{})\s+([a-z0-9][a-z0-9-]*)", names.join("|"))).unwrap())
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
                    if let Some(id) = cli.get(&m[1]) {
                        if !seen.contains(id) {
                            seen.push(id.clone());
                        }
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
    targets.into_iter().filter(|t| !t.recipe.is_empty() || !t.calls.is_empty()).collect()
}
