//! A repository of several parts: every manifest roots one (a crate, a Python package, an
//! app), read in its own language. Together they are one graph, in which every node knows its
//! part, and a route one part serves joins it to the parts that name the route.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::Result;
use regex::Regex;

use crate::model::{Edge, EdgeKind, EntryKind, Graph, NodeKind, Part};
use crate::repo::Repo;
use crate::{knowledge, Options};

/// The directories whose manifests root a part: the repository itself, and every manifest
/// below it in a language no enclosing part already reads (a workspace's member crates belong
/// to the workspace).
pub fn roots(repo: &Repo, opts: &Options) -> Vec<(PathBuf, &'static str)> {
    let mut out: Vec<(PathBuf, &'static str)> = vec![];
    // no part is under what a tool made, or under tests and the data they read, nor more than five levels down
    let skipped = |d: &Path| {
        let rel = d.strip_prefix(repo.root()).unwrap_or(d);
        rel.components().count() > 5
            || rel.components().any(|c| {
                let n = c.as_os_str().to_str().unwrap_or("");
                n.starts_with('.') || knowledge::MADE_DIRS.contains(&n) || knowledge::TEST_DIRS.contains(&n)
            })
    };
    for d in repo.dirs_under("").into_iter().filter(|d| !skipped(d)) {
        for l in crate::language::languages() {
            let inside = out.iter().any(|(o, name)| *name == l.name() && d.starts_with(o));
            if repo.is_file(d.join(l.manifest())) && !inside {
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
    let read: Vec<Repo> = repos.iter().map(|r| Repo::read(r, &opts.named)).collect::<Result<_>>()?;
    let found: Vec<Repo> = read.iter().flat_map(|r| roots(r, opts).into_iter().map(|(d, _)| r.under(d.strip_prefix(r.root()).unwrap_or(&d)))).collect();
    if found.is_empty() {
        anyhow::bail!("no part found in {} ({})", repos.iter().map(|r| r.display().to_string()).collect::<Vec<_>>().join(", "), crate::language::looked_for());
    }
    let opts = Options { package: None, ..opts.clone() };
    let mut graphs = vec![];
    for part in &found {
        match crate::graph_of(part, &opts) {
            Ok(g) => graphs.push(g),
            // a manifest without code of its own (tooling, a docs site) is not a part
            Err(e) if found.len() > 1 => eprintln!("skipped {}: {e}", part.root().display()),
            Err(e) => return Err(e),
        }
    }
    Ok(graphs)
}

/// The repositories as one graph: their parts side by side, joined where one names a route
/// another serves.
pub fn analyze_repo(repos: &[PathBuf], opts: &Options) -> Result<Graph> {
    let graphs = analyze_parts(repos, opts)?;
    let name = repos
        .iter()
        .map(|r| std::fs::canonicalize(r).ok().and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string())).unwrap_or_else(|| "repo".into()))
        .collect::<Vec<_>>()
        .join("+");
    let mut g = if graphs.len() == 1 { graphs.into_iter().next().unwrap() } else { merge(&name, graphs) };
    join(&mut g);
    Ok(g)
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
        out.stats.unresolved_calls_top.extend(g.stats.unresolved_calls_top);
    }
    out.recount();
    out
}

fn re(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).unwrap())
}

/// A string literal in any of the three quotes.
fn literal_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    re(&RE, r#""([^"\s]*)"|'([^'\s]*)'|`([^`\s]*)`"#)
}

/// A placeholder in a route or a template: `{id}`, `${id}`.
fn placeholder_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    re(&RE, r"\$?\{[^}]*\}")
}

/// A route path as a pattern: `{id}` is one segment, `{rest:path}` any number.
fn route_pattern(path: &str) -> Option<Regex> {
    static PIECE: OnceLock<Regex> = OnceLock::new();
    let mut out = String::from("^");
    for piece in re(&PIECE, r"\{[^}]*\}|[^{]+").find_iter(path.trim_end_matches('/')) {
        let p = piece.as_str();
        out += &if !p.starts_with('{') { regex::escape(p) } else if p.ends_with(":path}") { ".+".into() } else { "[^/]+".into() };
    }
    Regex::new(&(out + "/?$")).ok()
}

/// Whether a route (`/api/customers/{id}`) serves a path (`/api/customers/x`).
pub fn route_matches(route: &str, path: &str) -> bool {
    let path = placeholder_re().replace_all(path.split(['?', '#']).next().unwrap_or(path), "x");
    route_pattern(route).map_or(false, |re| re.is_match(&path))
}

/// The path a string literal asks for: without its host (written out, or a leading `${base}`)
/// and its query; a placeholder is a segment.
fn requested(lit: &str) -> Option<String> {
    let lit = lit.strip_prefix('$').and_then(|r| r.strip_prefix('{')).and_then(|r| r.split_once('}')).map_or(lit, |(_, rest)| rest);
    let path = match lit.find("://") {
        Some(i) => &lit[i + 3..][lit[i + 3..].find('/')?..],
        None => lit,
    };
    let path = path.split(['?', '#']).next()?;
    if !path.starts_with('/') || path.len() < 2 {
        return None;
    }
    Some(placeholder_re().replace_all(path, "x").into_owned())
}

/// Requests between parts: a string literal in one part that is the full path of a route
/// another part serves, and none of its own, is a `requests` edge from the node that holds it
/// (the innermost function, class or module around it) to the route's handler.
pub fn join(g: &mut Graph) {
    const METHODS: &[&str] = &["GET", "POST", "PUT", "DELETE", "PATCH", "WEBSOCKET"];
    let routes: Vec<(String, String, String, Regex)> = g
        .nodes
        .iter()
        .filter_map(|n| {
            let e = n.entry.as_ref().filter(|e| e.kind == EntryKind::Route)?;
            let (method, path) = e.label.split_once(' ')?;
            (METHODS.contains(&method) && path.len() > 1).then_some(())?;
            Some((n.part.clone(), n.id.clone(), path.to_string(), route_pattern(path)?))
        })
        .collect();
    if routes.is_empty() || g.parts.len() < 2 {
        return;
    }
    // a path a part routes itself (a page, its own API) is a link within it, not a request
    let own: Vec<(String, Regex)> = g
        .nodes
        .iter()
        .filter_map(|n| {
            let e = n.entry.as_ref().filter(|e| e.kind == EntryKind::Route)?;
            let path = e.label.split_once(' ').map_or(e.label.as_str(), |(_, p)| p);
            Some((n.part.clone(), route_pattern(path)?))
        })
        .collect();
    // per file: its part and its nodes by span, innermost first
    let mut by_file: HashMap<&str, Vec<(u32, u32, &str, &str)>> = HashMap::new();
    for n in &g.nodes {
        if let Some((a, b)) = n.span {
            by_file.entry(n.file.as_str()).or_default().push((a, b, n.id.as_str(), n.part.as_str()));
        }
    }
    for spans in by_file.values_mut() {
        spans.sort_by_key(|(a, b, _, _)| b - a);
    }
    let mut edges: BTreeMap<(String, String), String> = BTreeMap::new();
    for (file, src) in &g.files {
        let Some(spans) = by_file.get(file.as_str()) else { continue };
        for c in literal_re().captures_iter(src) {
            let m = c.get(1).or(c.get(2)).or(c.get(3)).unwrap();
            let Some(path) = requested(m.as_str()) else { continue };
            let line = src[..m.start()].matches('\n').count() as u32 + 1;
            let Some(&(_, _, holder, part)) = spans.iter().find(|(a, b, _, _)| *a <= line && line <= *b) else { continue };
            if own.iter().any(|(p, re)| p == part && re.is_match(&path)) {
                continue;
            }
            for (rpart, handler, rpath, re) in &routes {
                if rpart != part && re.is_match(&path) {
                    edges.entry((holder.to_string(), handler.clone())).or_insert_with(|| rpath.clone());
                }
            }
        }
    }
    for ((src, dst), via) in edges {
        g.edges.push(Edge { src, dst, kind: EdgeKind::Requests, via: Some(via), weight: None, arms: vec![], uses: false });
    }
    g.recount();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_literal_asks_for_a_route() {
        let re = route_pattern("/api/graphs/graph/rond/{uuid:path}").unwrap();
        assert!(re.is_match(&requested("/api/graphs/graph/rond/a/b").unwrap()));
        let re = route_pattern("/api/items/{id}").unwrap();
        assert!(re.is_match(&requested("http://localhost:8000/api/items/${id}?full=1").unwrap()));
        assert!(re.is_match(&requested("${BASE}/api/items/${id}").unwrap()), "a leading placeholder is the host");
        assert!(!re.is_match(&requested("/api/items").unwrap()));
        assert_eq!(requested("/"), None, "the root names no route");
        assert_eq!(requested("data/raw.csv"), None, "a relative path is a file");
    }
}
