//! Roles: what a project declares about itself, or, failing that, an honest inference.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use anyhow::{Context, Result};

use crate::model::{Architecture, Bands, EdgeKind, NodeKind};
use crate::python::Store;

/// Load `architecture.yml` / `.yaml` or a spike-style `roles.json`.
pub fn load(path: &Path) -> Result<Architecture> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let arch = if path.extension().and_then(|e| e.to_str()) == Some("json") {
        serde_json::from_str(&text)?
    } else {
        serde_yaml::from_str(&text)?
    };
    Ok(arch)
}

/// The declared architecture of a project root, if it has one.
pub fn find(project_root: Option<&Path>) -> Option<Architecture> {
    let root = project_root?;
    ["architecture.yml", "architecture.yaml", ".atlas/architecture.yml"]
        .iter()
        .map(|f| root.join(f))
        .find(|p| p.exists())
        .and_then(|p| load(&p).ok())
}

fn module_role(rules: &[(String, String)], mid: &str) -> String {
    let mut best: Option<&(String, String)> = None;
    for r in rules {
        if mid == r.0 || mid.starts_with(&format!("{}.", r.0)) {
            if best.map_or(true, |b| r.0.len() > b.0.len()) {
                best = Some(r);
            }
        }
    }
    best.map(|b| b.1.clone()).unwrap_or_else(|| "Other".into())
}

fn base_role(store: &Store, base_roles: &BTreeMap<String, String>, cid: &str, depth: u32) -> Option<String> {
    if let Some(r) = base_roles.get(cid) {
        return Some(r.clone());
    }
    if depth > 8 {
        return None;
    }
    for b in store.classes.get(cid).map(|c| c.resolved.clone()).unwrap_or_default() {
        if let Some(r) = base_roles.get(&b) {
            return Some(r.clone());
        }
        if !b.starts_with("ext:") {
            if let Some(r) = base_role(store, base_roles, &b, depth + 1) {
                return Some(r);
            }
        }
    }
    None
}

/// Assign a role to every node: per class through the base chain, otherwise by package.
pub fn assign(store: &mut Store, arch: &Architecture) {
    let ids: Vec<String> = store.nodes.keys().cloned().collect();
    for id in &ids {
        let (kind, module) = {
            let n = &store.nodes[id];
            (n.kind, n.module.clone())
        };
        if kind == NodeKind::External {
            store.nodes[id].role = "External".into();
            continue;
        }
        let mut role = module_role(&arch.rules, &module);
        let mut source = "package";
        if kind == NodeKind::Class {
            if let Some(r) = base_role(store, &arch.base_roles, id, 0) {
                role = r;
                source = "base";
            }
        }
        let n = &mut store.nodes[id];
        n.role = role;
        n.role_source = source.into();
    }
    for id in &ids {
        let cls = store.nodes[id].cls.clone();
        if let (NodeKind::Method, Some(c)) = (store.nodes[id].kind, cls) {
            let (role, src) = store.nodes.get(&c).map(|c| (c.role.clone(), c.role_source.clone())).unwrap_or_default();
            let n = &mut store.nodes[id];
            n.role = role;
            n.role_source = src;
        }
    }
}

/// No declaration: one role per immediate subpackage (or per module in a flat package),
/// root modules and scripts in the orchestration band, columns ordered so providers stand
/// left of their consumers, a pure provider in the foundation band.
pub fn infer(store: &mut Store, root: &str) -> Architecture {
    let pkg_mods: Vec<(String, bool)> = store
        .nodes
        .values()
        .filter(|n| n.kind == NodeKind::Module && (n.module == root || n.module.starts_with(&format!("{root}."))))
        .map(|n| (n.module.clone(), n.is_pkg))
        .collect();
    let second = |m: &str| m.split('.').nth(1).map(str::to_string);
    let subpkgs: Vec<String> = {
        let mut s: Vec<String> = pkg_mods
            .iter()
            .filter(|(m, is_pkg)| *is_pkg && m.split('.').count() == 2)
            .filter_map(|(m, _)| second(m))
            .collect();
        s.sort();
        s.dedup();
        s
    };
    let mut subs = subpkgs;
    if subs.is_empty() {
        subs = pkg_mods.iter().filter(|(m, _)| m.split('.').count() == 2).filter_map(|(m, _)| second(m)).collect();
        subs.sort();
        subs.dedup();
    }
    let cap = |s: &str| {
        let mut c = s.chars();
        c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
    };
    let mut rules: Vec<(String, String)> = subs.iter().map(|s| (format!("{root}.{s}"), cap(s))).collect();
    rules.push((root.to_string(), "Root".into()));
    rules.push(("scripts".into(), "Scripts".into()));
    rules.push(("_root".into(), "Scripts".into()));
    let mut arch = Architecture {
        order: vec![],
        bands: Bands { top: vec!["Root".into(), "Scripts".into()], bottom: vec![] },
        rules: rules.into(),
        base_roles: BTreeMap::new(),
        dependencies: "against_flow".into(),
        allowed: vec![],
        inferred: true,
    };
    assign(store, &arch);
    let mut deps: HashMap<(String, String), u32> = HashMap::new();
    for e in &store.edges {
        if e.kind == EdgeKind::Contains {
            continue;
        }
        let (Some(a), Some(b)) = (store.nodes.get(&e.src), store.nodes.get(&e.dst)) else { continue };
        if a.role == b.role || a.role == "External" || b.role == "External" {
            continue;
        }
        *deps.entry((a.role.clone(), b.role.clone())).or_insert(0) += 1;
    }
    let roles: Vec<String> = arch.rules.iter().map(|r| r.1.clone()).filter(|r| r != "Root" && r != "Scripts").collect();
    let incoming = |r: &str| deps.iter().filter(|((_, y), _)| y == r).map(|(_, v)| *v as i64).sum::<i64>();
    let outgoing = |r: &str| deps.iter().filter(|((x, _), _)| x == r).map(|(_, v)| *v as i64).sum::<i64>();
    let foundation: Vec<String> =
        roles.iter().filter(|r| outgoing(r) == 0 && incoming(r) > 0 && roles.len() > 2).cloned().collect();
    let mut order: Vec<String> = roles.iter().filter(|r| !foundation.contains(r)).cloned().collect();
    order.sort_by_key(|r| (outgoing(r) - incoming(r), r.clone()));
    arch.order = order;
    arch.bands.bottom = foundation;
    arch
}
