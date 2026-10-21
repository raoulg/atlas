//! The revelation: every prophecy in a graph, what it replaces and what is left of it.

use serde::Serialize;

use crate::model::{EdgeKind, Graph, Node, NodeKind};

/// Where one prophecy stands.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Stand {
    /// The prophecy: a class, or a function that is not a member of one.
    pub id: String,
    pub file: String,
    pub line: u32,
    /// What it replaces: the node, or the name when the code holds no such node.
    pub supersedes: Option<String>,
    /// The name resolves to a node.
    pub holds: bool,
    /// The nodes the name names, when it names more than one.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<String>,
    pub veiled: bool,
    pub sealed: Option<String>,
    /// The built code that leans on it.
    pub leaned_on_by: Vec<String>,
    /// What is still empty: the operations of a class, or the function itself.
    pub bodies: usize,
}

/// What an issue is: asked of the host the repository is on, so a lookup that cannot be made says
/// it does not know.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IssueState {
    Open,
    Closed,
    Unknown,
}

/// The open question a prophecy's shape waits on, and whether it is still open.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Seal {
    /// The prophecy.
    pub id: String,
    pub file: String,
    pub line: u32,
    /// The issue as the marker names it: `#144`.
    pub issue: String,
    pub state: IssueState,
    pub title: String,
}

impl Seal {
    /// The issue is closed: the shape can be decided, and the seal broken.
    pub fn can_be_broken(&self) -> bool {
        self.state == IssueState::Closed
    }
}

/// The seals among the prophecies, each with what `lookup` says of its issue (its state and title,
/// or none where it cannot tell).
pub fn seals(stands: &[Stand], lookup: &dyn Fn(&str) -> Option<(IssueState, String)>) -> Vec<Seal> {
    stands
        .iter()
        .filter_map(|s| {
            let issue = s.sealed.clone()?;
            let (state, title) = lookup(&issue).unwrap_or((IssueState::Unknown, String::new()));
            Some(Seal { id: s.id.clone(), file: s.file.clone(), line: s.line, issue, state, title })
        })
        .collect()
}

/// The seals of one part as text, those that can be broken first.
pub fn seals_report(root: &str, seals: &[Seal]) -> String {
    let mut sorted: Vec<&Seal> = seals.iter().collect();
    sorted.sort_by_key(|s| (!s.can_be_broken(), s.file.clone(), s.line));
    let mut out = format!("{root} · {} sealed, {} to break\n", seals.len(), seals.iter().filter(|s| s.can_be_broken()).count());
    for s in sorted {
        let state = match s.state {
            IssueState::Closed => "closed: the seal can be broken",
            IssueState::Open => "open",
            IssueState::Unknown => "state unknown",
        };
        let title = if s.title.is_empty() { String::new() } else { format!("  {}", s.title) };
        out +=
            &format!("\n  {}:{}  {}  sealed on {} ({state}){title}\n", s.file, s.line, s.id.rsplit('.').next().unwrap_or(&s.id), s.issue);
    }
    out
}

/// The revelation: every prophecy, what it replaces and what is left of it. The epic is done at
/// zero. A prophecy that is fulfilled has lost its marker, and is not here.
pub fn revelation(g: &Graph) -> Vec<Stand> {
    let nodes: std::collections::HashMap<&str, &Node> = g.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let mut out: Vec<Stand> = g
        .nodes
        .iter()
        .filter(|n| n.prophecy.is_some() && !n.library)
        .filter(|n| n.cls.as_deref().and_then(|c| nodes.get(c)).map_or(true, |c| c.prophecy.is_none()))
        .map(|n| {
            let p = n.prophecy.clone().unwrap_or_default();
            let members: Vec<&str> = n
                .methods
                .iter()
                .map(|m| format!("{}.{m}", n.id))
                .filter_map(|id| nodes.get_key_value(id.as_str()).map(|(k, _)| *k))
                .collect();
            let mut leaned: Vec<String> = g
                .edges
                .iter()
                .filter(|e| !matches!(e.kind, EdgeKind::Contains | EdgeKind::Imports))
                .filter(|e| e.dst == n.id || members.contains(&e.dst.as_str()))
                .filter(|e| {
                    nodes.get(e.src.as_str()).is_some_and(|s| s.prophecy.is_none() && s.cls.as_deref() != Some(n.id.as_str()) && !s.library)
                })
                .map(|e| e.src.clone())
                .collect();
            leaned.sort();
            leaned.dedup();
            Stand {
                id: n.id.clone(),
                file: std::path::Path::new(&n.file)
                    .strip_prefix(&g.project_root)
                    .map(crate::tree::path_str)
                    .unwrap_or_else(|_| n.file.clone()),
                line: n.span.map_or(1, |s| s.0),
                supersedes: p.replaces.clone().or(p.supersedes.clone()),
                holds: p.supersedes.is_none() || p.replaces.is_some(),
                candidates: p.candidates.clone(),
                veiled: p.veiled,
                sealed: p.sealed,
                leaned_on_by: leaned,
                bodies: if n.kind == NodeKind::Class { members.len() } else { 1 },
            }
        })
        .collect();
    out.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
    out
}

/// The revelation of one part as text: a line for each prophecy, and under it what is left.
pub fn report(root: &str, stands: &[Stand], seals: &[Seal]) -> String {
    let many = |n: usize, one: &str, more: &str| format!("{n} {}", if n == 1 { one } else { more });
    let mut out = format!("{root} · {}\n", many(stands.len(), "prophecy", "prophecies"));
    for s in stands {
        let replaces = match (&s.supersedes, s.holds) {
            (Some(r), true) => format!("  supersedes `{r}`"),
            (Some(r), false) if !s.candidates.is_empty() => format!(
                "  supersedes `{r}`, which names {} nodes: {}",
                s.candidates.len(),
                s.candidates.iter().map(|c| format!("`{c}`")).collect::<Vec<_>>().join(", ")
            ),
            (Some(r), false) => format!("  supersedes `{r}`, which the code does not hold (a false prophecy)"),
            (None, _) => String::new(),
        };
        out += &format!("\n  {}:{}  {}{replaces}\n", s.file, s.line, s.id.rsplit('.').next().unwrap_or(&s.id));
        let mut left = vec![many(s.bodies, "body to write", "bodies to write")];
        if s.veiled {
            left.push("veiled: the place is known, the shape is not".into());
        }
        if let Some(q) = &s.sealed {
            let state = match seals.iter().find(|x| x.id == s.id).map(|x| x.state) {
                Some(IssueState::Closed) => " (closed: the seal can be broken)",
                Some(IssueState::Open) => " (open)",
                _ => "",
            };
            left.push(format!("sealed on {q}{state}"));
        }
        if !s.leaned_on_by.is_empty() {
            left.push(format!("leaned on by {}", s.leaned_on_by.iter().map(|l| format!("`{l}`")).collect::<Vec<_>>().join(", ")));
        }
        out += &format!("      {}\n", left.join(" · "));
    }
    out
}
