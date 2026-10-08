//! Roles: what a project declares about itself, or, failing that, an honest inference.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use anyhow::{Context, Result};

use crate::model::{Architecture, Bands, EdgeKind, Graph, NodeKind};

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

/// The architecture a repo declares at its project root, if it has one.
pub fn find(g: &Graph) -> Option<Architecture> {
    if g.project_root.is_empty() {
        return None;
    }
    let root = Path::new(&g.project_root);

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

/// A role declared for a base class reaches every class whose base chain gets there.
fn base_role(bases: &HashMap<&str, Vec<&str>>, base_roles: &BTreeMap<String, String>, cid: &str, depth: u32) -> Option<String> {
    if let Some(r) = base_roles.get(cid) {
        return Some(r.clone());
    }
    if depth > 8 {
        return None;
    }
    for b in bases.get(cid).into_iter().flatten() {
        if let Some(r) = base_roles.get(*b) {
            return Some(r.clone());
        }
        if !b.starts_with("ext:") {
            if let Some(r) = base_role(bases, base_roles, b, depth + 1) {
                return Some(r);
            }
        }
    }
    None
}

/// The role of every node: per class through the base chain, otherwise by package; a method
/// takes its class's role.
fn roles_of(g: &Graph, arch: &Architecture) -> HashMap<String, (String, &'static str)> {
    let mut bases: HashMap<&str, Vec<&str>> = HashMap::new();
    for e in g.edges.iter().filter(|e| e.kind == EdgeKind::Inherits) {
        bases.entry(e.src.as_str()).or_default().push(e.dst.as_str());
    }
    let mut out: HashMap<String, (String, &'static str)> = HashMap::new();
    for n in &g.nodes {
        let r = match n.kind {
            NodeKind::External => ("External".to_string(), ""),
            NodeKind::Class => match base_role(&bases, &arch.base_roles, &n.id, 0) {
                Some(r) => (r, "base"),
                None => (module_role(&arch.rules, &n.module), "package"),
            },
            _ => (module_role(&arch.rules, &n.module), "package"),
        };
        out.insert(n.id.clone(), r);
    }
    for n in g.nodes.iter().filter(|n| n.kind == NodeKind::Method) {
        if let Some(r) = n.cls.as_ref().and_then(|c| out.get(c)).cloned() {
            out.insert(n.id.clone(), r);
        }
    }
    out
}

/// Paint every node with its role, and keep the architecture on the graph.
pub fn assign(g: &mut Graph, arch: Architecture) {
    let roles = roles_of(g, &arch);
    for n in g.nodes.iter_mut() {
        if let Some((role, source)) = roles.get(&n.id) {
            n.role = role.clone();
            n.role_source = source.to_string();
        }
    }
    g.roles = arch;
}

/// No declaration: one role per immediate subpackage (or per module in a flat package),
/// root modules and scripts in the orchestration band, columns ordered so providers stand
/// left of their consumers, a pure provider in the foundation band.
pub fn infer(g: &Graph) -> Architecture {
    let root = g.root.as_str();
    let pkg_mods: Vec<(String, bool)> = g
        .nodes
        .iter()
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
    let roles = roles_of(g, &arch);
    let mut deps: HashMap<(String, String), u32> = HashMap::new();
    for e in &g.edges {
        if e.kind == EdgeKind::Contains {
            continue;
        }
        let (Some((a, _)), Some((b, _))) = (roles.get(&e.src), roles.get(&e.dst)) else { continue };
        if a == b || a == "External" || b == "External" {
            continue;
        }
        *deps.entry((a.clone(), b.clone())).or_insert(0) += 1;
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
