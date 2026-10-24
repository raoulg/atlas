//! The page: one reading of a repository, or two drawn as one, in the viewer baked into the binary,
//! and the browser that opens it. `atlas view`, `serve`, `history` and `mcp` all draw it in the same
//! steps: the parts are read (`Reading`), the principles asked of them (`Findings`), the parts drawn
//! as one graph and its flow made (`flow::of`), and the page made of those (`Html`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use atlas_engine::changes::{self, Named};
use atlas_engine::diff::Diff;
use atlas_engine::flow::Flow;
use atlas_engine::principles::{self, Finding};
use atlas_engine::sight::Sight;
use atlas_engine::{parts, Graph, Options};
use serde::Serialize;

/// The viewer, baked into the binary, with its model and its fonts: the layout of the graph it is handed.
const VIEWER: &str = include_str!("../../viewer/index.html");
const PLAN: &str = include_str!("../../viewer/plan.js");
pub(crate) const FONTS: &str = include_str!("../../viewer/fonts.css");

/// What a page draws, as read: the parts of the repositories, each read in its own language, and,
/// where two readings of one repository are drawn as one, the earlier of them.
pub(crate) struct Reading {
    /// The repositories read, after which the graph is named.
    repos: Vec<PathBuf>,
    /// The parts drawn: of the repositories as they are, or of the later reading.
    parts: Vec<Graph>,
    since: Option<Since>,
}

/// The earlier of two readings drawn as one: the revision it was read at and its parts, and the
/// revision of the later.
struct Since {
    a: Named,
    parts: Vec<Graph>,
    b: Named,
}

/// What the principles ask of what a page draws, each finding at the node it is about, and, where
/// two readings are drawn as one, how many the later raises and resolves.
pub(crate) struct Findings {
    found: Vec<Finding>,
    changed: Option<Changed>,
}

/// How the findings of the later of two readings stand to the earlier's.
#[derive(Serialize, Clone, Copy)]
struct Changed {
    raised: usize,
    resolved: usize,
}

/// How the things of a union stand to the two readings it is of, and the revisions they were read at.
#[derive(Serialize)]
pub(crate) struct Compared {
    #[serde(flatten)]
    diff: Diff,
    a: Named,
    b: Named,
}

impl Reading {
    /// The repositories as they are: their parts.
    pub(crate) fn of(repos: &[PathBuf], parts: Vec<Graph>) -> Reading {
        Reading { repos: repos.to_vec(), parts, since: None }
    }

    /// Two readings of `repo` as one: revision `a` with its parts, and revision `b` with the parts drawn.
    pub(crate) fn compared(repo: &Path, a: (Named, Vec<Graph>), b: (Named, Vec<Graph>)) -> Reading {
        Reading { repos: vec![repo.to_path_buf()], parts: b.1, since: Some(Since { a: a.0, parts: a.1, b: b.0 }) }
    }

    /// What the principles ask of the parts drawn, each part checked in its own language, and,
    /// where two readings are drawn as one, how many findings the later raises and resolves.
    pub(crate) fn findings(&self) -> Findings {
        let found: Vec<Finding> = self.parts.iter().flat_map(principles::check).collect();
        let changed = self.since.as_ref().map(|s| {
            let before: Vec<Finding> = s.parts.iter().flat_map(principles::check).collect();
            let (raised, resolved) = changes::raised(&before, &found);
            Changed { raised: raised.len(), resolved: resolved.len() }
        });
        Findings { found, changed }
    }

    /// The parts as one graph, joined where one requests another's routes; where two readings are
    /// drawn as one, the union of the two, with how each of its things stands to them.
    pub(crate) fn drawn(self) -> (Graph, Option<Compared>) {
        let Some(since) = self.since else { return (parts::merged(&self.repos, self.parts), None) };
        let name = self
            .repos
            .first()
            .and_then(|r| std::fs::canonicalize(r).ok())
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_else(|| "repo".into());
        let (graph, diff) = atlas_engine::diff::union(&name, since.parts, self.parts);
        (graph, Some(Compared { diff, a: since.a, b: since.b }))
    }
}

/// What `atlas view` reads: the repositories as they are, or one repository since a revision
/// (`since`), until another (`until`) or the working tree.
pub(crate) fn reading(repos: &[PathBuf], opts: &Options, since: Option<&str>, until: Option<&str>) -> Result<Reading> {
    match (since, repos) {
        (None, _) => Ok(Reading::of(repos, parts::analyze_parts(repos, opts)?)),
        (Some(a), [repo]) => compared(repo, a, until, opts),
        (Some(_), _) => anyhow::bail!("--since compares one repository with its history"),
    }
}

/// Revision `a` of `repo` and revision `b` (the working tree when there is none), to be drawn as one.
fn compared(repo: &Path, a: &str, b: Option<&str>, opts: &Options) -> Result<Reading> {
    let (_, named_a) = changes::resolve(repo, a)?;
    let named_b = b.map(|b| changes::resolve(repo, b).map(|(_, n)| n)).transpose()?.unwrap_or_else(changes::worktree);
    let before = changes::parts_at(repo, Some(a), opts)?;
    Ok(Reading::compared(repo, (named_a, before), (named_b, changes::parts_at(repo, b, opts)?)))
}

/// Where a page opens, besides what it draws: on a route by its entry's id, in a sight, and, where
/// `atlas serve` serves it, as the drawing the server knows it by, to follow the next.
#[derive(Default)]
pub(crate) struct Opens {
    pub(crate) route: Option<String>,
    pub(crate) sight: Option<Sight>,
    pub(crate) served: Option<String>,
    pub(crate) preliminary: bool,
}

/// One project as the viewer is handed it: the graph, the flow its routes are read from, the
/// findings at the nodes they are about, how its things stand to two readings when it is their
/// union, and where the page opens.
#[derive(Serialize)]
struct Project<'a> {
    #[serde(flatten)]
    graph: &'a Graph,
    flow: &'a Flow,
    findings: &'a [Finding],
    #[serde(skip_serializing_if = "Option::is_none")]
    diff: Option<Marked<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    open: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    view: Option<&'a Sight>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    served: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    drawn: Option<&'a str>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    preliminary: bool,
}

/// What the viewer is handed: its projects by name, and the build of Atlas that drew them.
#[derive(Serialize)]
struct Payload<'a> {
    projects: BTreeMap<&'a str, Project<'a>>,
    atlas: &'a str,
}

/// A union as the page marks it: how its things stand to the two readings, and how many findings
/// the later raises and resolves.
#[derive(Serialize)]
struct Marked<'a> {
    #[serde(flatten)]
    compared: &'a Compared,
    findings: Changed,
}

/// A page: the viewer, with its model and its fonts, holding one project.
pub(crate) struct Html(String);

impl Html {
    /// The viewer holding one graph, its flow and its findings, and, where the graph is the union
    /// of two readings, how it stands to them; opened where `opens` says.
    pub(crate) fn of(graph: &Graph, flow: &Flow, findings: &Findings, compared: Option<&Compared>, opens: &Opens) -> Result<Html> {
        let project = Project {
            graph,
            flow,
            findings: &findings.found,
            diff: compared.zip(findings.changed).map(|(compared, findings)| Marked { compared, findings }),
            open: opens.route.as_deref(),
            view: opens.sight.as_ref(),
            served: opens.served.is_some(),
            drawn: opens.served.as_deref(),
            preliminary: opens.preliminary,
        };
        let payload = Payload { projects: BTreeMap::from([(graph.root.as_str(), project)]), atlas: crate::version::VERSION.as_str() };
        let payload = serde_json::to_string(&payload)?.replace("</", "<\\/");
        Ok(Html(VIEWER.replacen("/*__PLAN__*/", PLAN, 1).replacen("/*__FONTS__*/", FONTS, 1).replacen("/*__DATA__*/null", &payload, 1)))
    }

    /// The page as a file at `out`, in a folder made for it where there is none.
    pub(crate) fn write(&self, out: &Path) -> Result<()> {
        if let Some(dir) = out.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("making {}", dir.display()))?;
        }
        std::fs::write(out, &self.0).with_context(|| format!("writing {}", out.display()))
    }
}

impl From<Html> for String {
    fn from(html: Html) -> String {
        html.0
    }
}

/// What a page draws, in a line: the project, its parts, how many modules and entries.
pub(crate) fn summary(g: &Graph) -> String {
    let parts: Vec<&str> = g.parts.iter().map(|p| p.id.as_str()).collect();
    let entries = g.nodes.iter().filter(|n| n.entry.is_some()).count();
    format!("{} · {} · {} modules · {entries} entries", g.root, parts.join(" + "), g.stats.modules)
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
