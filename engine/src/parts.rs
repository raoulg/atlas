//! A repository of several parts: every manifest roots one (a crate, a Python package, an
//! app), read in its own language. Together they are one graph, in which every node knows its
//! part, and a route one part serves joins it to the parts that name the route.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::model::{Edge, EdgeKind, EntryKind, Graph, NodeKind, Part};
use crate::paths::{method_and_path, serving};
use crate::repo::Repo;
use crate::{knowledge, Options};

/// The directories whose manifests root a part: the repository itself, and every manifest
/// below it, at any depth, that no part around it reads as its own (a workspace reads its
/// members, a project its packages).
pub fn roots(repo: &Repo, opts: &Options) -> Vec<(PathBuf, &'static str)> {
    let mut out: Vec<(PathBuf, &'static str)> = vec![];
    let rel = |d: &Path| d.strip_prefix(repo.root()).unwrap_or(d).to_path_buf();
    // no part is under what a tool made, or under tests and the data they read
    let skipped = |d: &Path| {
        rel(d).components().any(|c| {
            let n = c.as_os_str().to_str().unwrap_or("");
            n.starts_with('.') || knowledge::made_dir(n) || knowledge::TEST_DIRS.contains(&n)
        })
    };
    // a directory comes after the directories around it
    for d in repo.dirs_under("").into_iter().filter(|d| !skipped(d)) {
        for l in crate::language::languages() {
            if !repo.is_file(d.join(l.manifest())) {
                continue;
            }
            let read = out.iter().any(|(o, name)| *name == l.name() && d.starts_with(o) && l.reads(&repo.under(&rel(o)), &d));
            if !read {
                out.push((d.clone(), l.name()));
            }
        }
    }
    if out.is_empty() {
        if let Some(lang) = crate::language::detect(repo, &Options { package: None, ..opts.clone() }) {
            out.push((repo.root().to_path_buf(), lang.name()));
        }
    }
    out
}

/// Each part's own graph, read in its language: what `atlas check` asks, part by part. Each
/// repository is read once; a part is a view of its reading.
pub fn analyze_parts(repos: &[PathBuf], opts: &Options) -> Result<Vec<Graph>> {
    let read: Vec<Repo> = repos.iter().map(|r| crate::language::read(r, opts)).collect::<Result<_>>()?;
    // a package named on its own is one part: the project it is in, asked for that package
    let found: Vec<Repo> = read
        .iter()
        .flat_map(|r| match r.asked() {
            Some(_) => vec![r.clone()],
            None => roots(r, opts).into_iter().map(|(d, _)| r.under(d.strip_prefix(r.root()).unwrap_or(&d))).collect(),
        })
        .collect();
    if found.is_empty() {
        anyhow::bail!("no part found in {} ({})", repos.iter().map(|r| r.display().to_string()).collect::<Vec<_>>().join(", "), crate::language::looked_for());
    }
    let opts = Options { package: None, ..opts.clone() };
    let mut graphs = vec![];
    for part in &found {
        match crate::graph_of(part, &opts) {
            // a manifest without code of its own (tooling, a docs site, a workspace's root) is not a part
            Ok(g) if g.nodes.is_empty() && found.len() > 1 => eprintln!("skipped {}: no code of its own", part.root().display()),
            Ok(g) => graphs.push(g),
            Err(e) if found.len() > 1 => eprintln!("skipped {}: {e}", part.root().display()),
            Err(e) => return Err(e),
        }
    }
    if graphs.is_empty() {
        anyhow::bail!("no code found in {}", repos.iter().map(|r| r.display().to_string()).collect::<Vec<_>>().join(", "));
    }
    Ok(graphs)
}

/// The repositories as one graph: their parts side by side, joined where one names a route
/// another serves.
pub fn analyze_repo(repos: &[PathBuf], opts: &Options) -> Result<Graph> {
    Ok(merged(repos, analyze_parts(repos, opts)?))
}

/// The parts of the repositories as one graph, named after them, joined where one part requests
/// another's routes.
pub fn merged(repos: &[PathBuf], graphs: Vec<Graph>) -> Graph {
    let name = repos
        .iter()
        .map(|r| std::fs::canonicalize(r).ok().and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string())).unwrap_or_else(|| "repo".into()))
        .collect::<Vec<_>>()
        .join("+");
    let mut g = if graphs.len() == 1 { graphs.into_iter().next().unwrap() } else { merge(&name, graphs) };
    join(&mut g);
    g
}

/// A graph's parts and every node's part: the top-level packages it holds (a crate of a
/// workspace, a Python package, an app); what sits beside them (scripts, notebooks) belongs
/// to the first.
pub fn assign(g: &mut Graph) {
    // a library read for its types is the environment's, not a part of the repository
    let mut ids: Vec<String> = g.nodes.iter().filter(|n| n.kind == NodeKind::Module && n.is_pkg && !n.library && !n.id.contains('.')).map(|n| n.id.clone()).collect();
    if ids.is_empty() {
        ids.push(g.root.clone());
    }
    // the package named after the graph comes first: scripts and notebooks are routes into it
    ids.sort_by_key(|id| id != &g.root);
    let leaves = crate::language::leaves(&g.language);
    g.parts = ids.iter().map(|id| Part { id: id.clone(), language: g.language.clone(), dir: g.project_root.clone(), leaves }).collect();
    let known: HashSet<&String> = ids.iter().collect();
    for n in &mut g.nodes {
        let head = n.id.split('.').next().unwrap_or("").to_string();
        n.part = if known.contains(&head) { head } else { ids[0].clone() };
    }
}

/// Graphs side by side: their nodes, edges, files and routes.
pub fn merge(name: &str, graphs: Vec<Graph>) -> Graph {
    let languages: HashSet<&str> = graphs.iter().map(|g| g.language.as_str()).collect();
    let mut out = Graph {
        root: name.to_string(),
        language: if languages.len() == 1 { graphs[0].language.clone() } else { String::new() },
        project_root: graphs[0].project_root.clone(),
        ..Default::default()
    };
    let mut made = HashSet::new();
    for g in graphs {
        out.parts.extend(g.parts);
        out.nodes.extend(g.nodes);
        out.edges.extend(g.edges);
        out.files.extend(g.files);
        out.make.extend(g.make.into_iter().filter(|t| made.insert(t.id.clone())));
        out.stats.modules += g.stats.modules;
        out.stats.unresolved_calls_total += g.stats.unresolved_calls_total;
        out.stats.unresolved_calls += g.stats.unresolved_calls;
        out.stats.unresolved_calls_top.extend(g.stats.unresolved_calls_top);
    }
    out.recount();
    out
}


/// Requests between parts: a request one part makes whole and none of its own routes serves is a
/// `requests` edge from the function that makes it whole to the handler of another part's route
/// that serves it, by method and path.
pub fn join(g: &mut Graph) {
    if g.parts.len() < 2 {
        return;
    }
    let mut routes: HashMap<&str, Vec<(String, String, String)>> = HashMap::new();
    for n in &g.nodes {
        let Some(e) = n.entry.as_ref().filter(|e| e.kind == EntryKind::Route) else { continue };
        let Some((method, path)) = e.label.split_once(' ') else { continue };
        if method.split('|').all(|m| knowledge::registers_route(&m.to_ascii_lowercase())) && path.len() > 1 {
            routes.entry(n.part.as_str()).or_default().push((n.id.clone(), method.to_string(), path.to_string()));
        }
    }
    let mut edges: BTreeMap<(String, String), String> = BTreeMap::new();
    for n in g.nodes.iter().filter(|n| !n.asks.is_empty()) {
        for ask in &n.asks {
            let (method, path) = method_and_path(ask);
            for (part, served) in &routes {
                if *part == n.part {
                    continue;
                }
                for (handler, _, route) in serving(served, method, path) {
                    edges.entry((n.id.clone(), handler.clone())).or_insert_with(|| route.clone());
                }
            }
        }
    }
    for ((src, dst), via) in edges {
        g.edges.push(Edge { src, dst, kind: EdgeKind::Requests, via: Some(via), weight: None, arms: vec![], uses: false });
    }
    g.recount();
}
