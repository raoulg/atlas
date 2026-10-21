//! The page: one reading of a repository, or two drawn as one, in the viewer baked into the binary,
//! and the browser that opens it. `atlas view`, `serve`, `history` and `mcp` all draw it.

use std::path::{Path, PathBuf};

use anyhow::Result;
use atlas_engine::changes::{self, Named};
use atlas_engine::{parts, principles, Graph, Options};

/// The viewer, baked into the binary, with its fonts: the layout of the graph it is handed.
const VIEWER: &str = include_str!("../../viewer/index.html");
pub(crate) const FONTS: &str = include_str!("../../viewer/fonts.css");

/// What a page draws: one graph, what the principles ask of it, and, where two readings are drawn
/// as one, how its things stand to them.
pub(crate) struct Drawing {
    graph: Graph,
    findings: Vec<principles::Finding>,
    diff: Option<serde_json::Value>,
}

/// One reading as a drawing: its parts, each read and checked in its own language, as one graph.
pub(crate) fn single(repos: &[PathBuf], graphs: Vec<Graph>) -> Drawing {
    let findings = graphs.iter().flat_map(principles::check).collect();
    Drawing { graph: parts::merged(repos, graphs), findings, diff: None }
}

/// Two readings of `repo` as one drawing: the union of the parts read at A and at B, the findings
/// of B, and how each thing of the union stands to the two, with the revisions they were read at
/// and how many findings B raises and resolves.
pub(crate) fn union(repo: &Path, a: (Named, Vec<Graph>), b: (Named, Vec<Graph>)) -> Result<Drawing> {
    let name = std::fs::canonicalize(repo)
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        .unwrap_or_else(|| "repo".into());
    let (fa, fb): (Vec<principles::Finding>, Vec<principles::Finding>) =
        (a.1.iter().flat_map(principles::check).collect(), b.1.iter().flat_map(principles::check).collect());
    let (raised, resolved) = changes::raised(&fa, &fb);
    let (graph, d) = atlas_engine::diff::union(&name, a.1, b.1);
    let mut diff = serde_json::to_value(&d)?;
    diff["a"] = serde_json::to_value(&a.0)?;
    diff["b"] = serde_json::to_value(&b.0)?;
    diff["findings"] = serde_json::json!({ "raised": raised.len(), "resolved": resolved.len() });
    Ok(Drawing { graph, findings: fb, diff: Some(diff) })
}

/// Revision `a` of `repo` and revision `b` (the working tree when there is none) as one drawing.
fn compared(repo: &Path, a: &str, b: Option<&str>, opts: &Options) -> Result<Drawing> {
    let (_, named_a) = changes::resolve(repo, a)?;
    let named_b = b.map(|b| changes::resolve(repo, b).map(|(_, n)| n)).transpose()?.unwrap_or_else(changes::worktree);
    let before = changes::parts_at(repo, Some(a), opts)?;
    union(repo, (named_a, before), (named_b, changes::parts_at(repo, b, opts)?))
}

/// What `atlas view` draws: the repositories as they are, or one repository since a revision.
fn reading(repos: &[PathBuf], opts: &Options, since: Option<&str>, until: Option<&str>) -> Result<Drawing> {
    match (since, repos) {
        (None, _) => Ok(single(repos, parts::analyze_parts(repos, opts)?)),
        (Some(a), [repo]) => compared(repo, a, until, opts),
        (Some(_), _) => anyhow::bail!("--since compares one repository with its history"),
    }
}

/// The viewer holding one drawing: its graph, the flow its routes are read from, the findings at
/// the nodes they are about, how its things stand to two readings when it is their union, and
/// what else the page is handed (`open`, `view`).
pub(crate) fn viewer(d: &Drawing, extra: serde_json::Map<String, serde_json::Value>) -> Result<String> {
    let mut page = serde_json::to_value(&d.graph)?;
    // what each route runs and what each step takes and gives: the flow the agent reads its routes from
    page["flow"] = serde_json::to_value(atlas_engine::flow::of(&d.graph))?;
    // what the principles ask, each at the node it is about, where the page marks it; the merged
    // graph keeps every part's nodes, so each finding is about a node the picture knows
    page["findings"] = serde_json::to_value(&d.findings)?;
    if let Some(diff) = &d.diff {
        page["diff"] = diff.clone();
    }
    for (k, v) in extra {
        page[k] = v;
    }
    let mut projects = serde_json::Map::new();
    projects.insert(d.graph.root.clone(), page);
    let payload = serde_json::to_string(&serde_json::json!({ "projects": projects, "atlas": crate::version::VERSION.as_str() }))?
        .replace("</", "<\\/");
    Ok(VIEWER.replacen("/*__FONTS__*/", FONTS, 1).replacen("/*__DATA__*/null", &payload, 1))
}

/// The page of one or more repositories, or of one repository since a revision (`since`, until
/// `until` or the working tree), drawn in `shown` when it is given (a view: its route, kind,
/// part, focus and opened sets), else on the route of a file named on the command line. Gives the
/// page and the name of its project.
pub(crate) fn page(
    repos: &[PathBuf],
    opts: &Options,
    since: Option<&str>,
    until: Option<&str>,
    shown: Option<serde_json::Value>,
) -> Result<(String, String, String)> {
    let d = reading(repos, opts, since, until)?;
    let g = &d.graph;
    // what the page draws, in a line
    let parts: Vec<&str> = g.parts.iter().map(|p| p.id.as_str()).collect();
    let entries = g.nodes.iter().filter(|n| n.entry.is_some()).count();
    let summary = format!("{} · {} · {} modules · {entries} entries", g.root, parts.join(" + "), g.stats.modules);
    let mut extra = serde_json::Map::new();
    // a file named on the command line opens on its route
    if let Some(n) = g.nodes.iter().find(|n| n.entry.is_some() && std::fs::canonicalize(&n.file).map_or(false, |f| opts.named.contains(&f)))
    {
        extra.insert("open".into(), serde_json::Value::String(n.id.clone()));
    }
    if let Some(v) = shown {
        extra.insert("view".into(), v);
    }
    Ok((g.root.clone(), viewer(&d, extra)?, summary))
}

/// The page of the working tree of `repo`, served: it tells its server what it shows, asks it where
/// an agent points, keeps notes through it, and knows its drawing as `drawn`, to follow the next.
pub(crate) fn served_page(repo: &Path, opts: &Options, drawn: &str) -> Result<(String, String)> {
    let d = served(repo, opts)?;
    let g = &d.graph;
    let parts: Vec<&str> = g.parts.iter().map(|p| p.id.as_str()).collect();
    let summary = format!("{} · {} · {} modules", g.root, parts.join(" + "), g.stats.modules);
    let mut extra = serde_json::Map::new();
    extra.insert("served".into(), serde_json::Value::Bool(true));
    extra.insert("drawn".into(), serde_json::Value::String(drawn.into()));
    Ok((viewer(&d, extra)?, summary))
}

/// The working tree as `atlas serve` draws it: beside what an agent has in view, as two readings are
/// drawn (A the working tree, B the vision), and alone when no vision is kept.
fn served(repo: &Path, opts: &Options) -> Result<Drawing> {
    let Some(p) = crate::shared::vision(repo) else { return reading(&[repo.to_path_buf()], opts, None, None) };
    let envisioned = Named { rev: "vision".into(), short: "vision".into(), subject: p.title.clone() };
    let now = parts::analyze_parts(&[repo.to_path_buf()], opts)?;
    union(repo, (changes::worktree(), now), (envisioned, changes::envisioned(repo, &p.files, opts)?))
}

/// Open a page or an address in the browser the system names.
pub(crate) fn open(target: &std::ffi::OsStr) {
    #[cfg(target_os = "macos")]
    let status = std::process::Command::new("open").arg(target).status();
    #[cfg(target_os = "linux")]
    let status = std::process::Command::new("xdg-open").arg(target).status();
    #[cfg(target_os = "windows")]
    let status = std::process::Command::new("cmd").args(["/C", "start", ""]).arg(target).status();
    if let Err(e) = status {
        eprintln!("could not open a browser ({e}); open {} yourself", target.to_string_lossy());
    }
}
