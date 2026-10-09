//! `check: Graph -> [Finding]`: the design principles Atlas stands on, computed from the graph.
//!
//! A finding names its principle, the symbol it is about, the evidence the graph holds and a
//! question for the author. It coaches; it does not fix. Atlas sees types, routes and places,
//! so these are the principles that live there; what a single line should look like belongs
//! to ruff, pyright and clippy. `docs/principles.md` explains each one.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

use serde::Serialize;

use crate::model::{Edge, EdgeKind, EntryKind, Graph, Node, NodeKind};

/// A principle by its id and what it asks of code.
pub struct Principle {
    pub id: &'static str,
    pub asks: &'static str,
}

pub const PRINCIPLES: &[Principle] = &[
    Principle { id: "doors", asks: "A package names its doors, and each door says what goes in and what comes out." },
    Principle { id: "sums", asks: "A sum type is unwrapped where it arises." },
    Principle { id: "overrides", asks: "An override keeps the promise of its base." },
    Principle { id: "places", asks: "Library code does not decide where data lives." },
    Principle { id: "changes", asks: "A change to an input shows in the signature." },
];

/// One place where the code and a principle disagree.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Finding {
    pub principle: &'static str,
    /// The node the finding is about.
    pub node: String,
    /// Relative to the project root.
    pub file: String,
    pub line: u32,
    /// What the graph shows, in one line.
    pub what: String,
    /// The signatures, edges and touches that show it.
    pub evidence: Vec<String>,
    /// What the author could ask of the code.
    pub question: String,
}

/// Every finding, by principle in the order of `PRINCIPLES`, then by place in the code.
pub fn check(g: &Graph) -> Vec<Finding> {
    let ix = Index::new(g);
    let mut out = vec![];
    for rule in [doors, sums, overrides, places, changes] {
        let mut found = rule(&ix);
        found.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
        out.extend(found);
    }
    out
}

struct Index<'g> {
    g: &'g Graph,
    nodes: HashMap<&'g str, &'g Node>,
    out: HashMap<&'g str, Vec<&'g Edge>>,
    into: HashMap<&'g str, Vec<&'g Edge>>,
}

impl<'g> Index<'g> {
    fn new(g: &'g Graph) -> Self {
        let mut out: HashMap<&str, Vec<&Edge>> = HashMap::new();
        let mut into: HashMap<&str, Vec<&Edge>> = HashMap::new();
        for e in &g.edges {
            out.entry(e.src.as_str()).or_default().push(e);
            into.entry(e.dst.as_str()).or_default().push(e);
        }
        Index { g, nodes: g.nodes.iter().map(|n| (n.id.as_str(), n)).collect(), out, into }
    }

    fn node(&self, id: &str) -> Option<&'g Node> {
        self.nodes.get(id).copied()
    }

    fn targets(&self, id: &str, kind: EdgeKind) -> Vec<&'g str> {
        self.out.get(id).into_iter().flatten().filter(|e| e.kind == kind).map(|e| e.dst.as_str()).collect()
    }

    fn sources(&self, id: &str, kind: EdgeKind) -> Vec<&'g str> {
        self.into.get(id).into_iter().flatten().filter(|e| e.kind == kind).map(|e| e.src.as_str()).collect()
    }

    /// `BasePlot.plot` for a method, the plain name for anything else.
    fn label(&self, id: &str) -> String {
        let Some(n) = self.node(id) else { return id.rsplit('.').next().unwrap_or(id).to_string() };
        match n.cls.as_deref().and_then(|c| self.node(c)) {
            Some(c) => format!("{}.{}", c.name, n.name),
            None => n.name.clone(),
        }
    }

    fn finding(&self, principle: &'static str, n: &Node, what: String, evidence: Vec<String>, question: String) -> Finding {
        Finding { principle, node: n.id.clone(), file: self.file(n), line: n.span.map_or(1, |s| s.0), what, evidence, question }
    }

    fn file(&self, n: &Node) -> String {
        let root = Path::new(&self.g.project_root);
        Path::new(&n.file).strip_prefix(root).map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|_| n.file.clone())
    }

    fn at(&self, n: &Node) -> String {
        format!("{}:{}", self.file(n), n.span.map_or(1, |s| s.0))
    }

    /// Ancestors along `inherits`, nearest first.
    fn ancestors(&self, cls: &str) -> Vec<&'g str> {
        let mut seen = HashSet::new();
        let mut q: VecDeque<&str> = self.targets(cls, EdgeKind::Inherits).into();
        let mut out = vec![];
        while let Some(c) = q.pop_front() {
            if seen.insert(c) {
                out.push(c);
                q.extend(self.targets(c, EdgeKind::Inherits));
            }
        }
        out
    }
}

fn is_function(n: &Node) -> bool {
    matches!(n.kind, NodeKind::Function | NodeKind::Method)
}

fn public(n: &Node) -> bool {
    !n.name.starts_with('_')
}

/// The name a parameter is called by: `*args` -> `args`, `mut g: &mut Graph` -> `g`.
fn param_name(p: &str) -> &str {
    let name = p.split([':', '=']).next().unwrap_or(p).trim();
    name.trim_start_matches('*').trim_start_matches("mut ").trim()
}

/// `self`, `cls`, `&self`, `&mut self`, `mut self`: the receiver, not something handed in.
fn is_receiver(p: &str) -> bool {
    matches!(param_name(p).trim_start_matches('&').trim_start_matches("mut ").trim(), "self" | "cls")
}

fn signature(n: &Node) -> String {
    let ret = if n.returns.is_empty() { String::new() } else { format!(" -> {}", n.returns) };
    format!("{}({}){ret}", n.name, n.params.join(", "))
}

fn same_type(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.chars().filter(|c| !c.is_whitespace() && *c != '"' && *c != '\'').collect::<String>();
    norm(a) == norm(b)
}

fn list(xs: &[String]) -> String {
    match xs {
        [] => String::new(),
        [x] => x.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

// ---------- doors ----------

/// A package names what it exports; an exported function, and each public method of an
/// exported class, says what it takes and gives. Python makes every module importable, so
/// what a package offers is a declaration it has to make (`__all__`); a Rust lib's `pub` items
/// are that declaration, and the compiler asks for every type.
fn doors(ix: &Index) -> Vec<Finding> {
    let g = ix.g;
    let exports: Vec<&Node> = g.nodes.iter().filter(|n| n.entry.as_ref().map_or(false, |e| e.kind == EntryKind::Export)).collect();
    let mut out = vec![];
    if exports.is_empty() && g.language == "python" {
        if let Some(root) = ix.node(&g.root) {
            out.push(ix.finding(
                "doors",
                root,
                format!("{} exports nothing at its root", g.root),
                vec![format!("{}: no `__all__`, and no public name of the package bound there", ix.file(root))],
                format!("Named in `__all__`, each door becomes a route Atlas can draw. What may a user of {} import, and from where?", g.root),
            ));
        }
    }
    for e in exports {
        let fns: Vec<&Node> = match e.kind {
            NodeKind::Function => vec![e],
            NodeKind::Class => e.methods.iter().filter_map(|m| ix.node(&format!("{}.{m}", e.id))).filter(|m| public(m) || m.name == "__init__").collect(),
            _ => vec![],
        };
        for f in fns {
            let untyped: Vec<String> = f
                .params
                .iter()
                .filter(|p| !p.contains(':') && !is_receiver(p) && !p.starts_with('*'))
                .map(|p| format!("`{}`", param_name(p)))
                .collect();
            let gives_unknown = g.language == "python" && f.returns.is_empty() && f.name != "__init__";
            if untyped.is_empty() && !gives_unknown {
                continue;
            }
            let what = match (untyped.is_empty(), gives_unknown) {
                (false, true) => format!("{} does not say what {} are, nor what it gives", ix.label(&f.id), list(&untyped)),
                (false, false) => format!("{} does not say what {} {}", ix.label(&f.id), list(&untyped), if untyped.len() == 1 { "is" } else { "are" }),
                _ => format!("{} does not say what it gives", ix.label(&f.id)),
            };
            out.push(ix.finding(
                "doors",
                f,
                what,
                vec![signature(f)],
                format!("Until its signature says so, a reader and Atlas both lose the route at this door. What does {} take and give?", ix.label(&f.id)),
            ));
        }
    }
    out
}

// ---------- sums ----------

/// A sum type that a module returns and that functions in other modules take as input: each
/// of them has to choose between the variants again. Unwrapped where it arises, the choice is
/// made once, and what travels on is the variant each consumer needs.
fn sums(ix: &Index) -> Vec<Finding> {
    let mut out = vec![];
    for s in ix.g.nodes.iter().filter(|n| !ix.targets(&n.id, EdgeKind::Variant).is_empty()) {
        let variants: Vec<String> = ix.targets(&s.id, EdgeKind::Variant).iter().map(|v| ix.label(v)).collect();
        let producers: Vec<&Node> = ix.sources(&s.id, EdgeKind::Returns).iter().filter_map(|p| ix.node(p)).collect();
        let homes: HashSet<&str> = producers.iter().map(|p| p.module.as_str()).collect();
        let consumers: Vec<&Node> = ix
            .sources(&s.id, EdgeKind::Takes)
            .iter()
            .filter_map(|c| ix.node(c))
            .filter(|c| public(c) && !homes.contains(c.module.as_str()))
            .collect();
        if producers.is_empty() || consumers.is_empty() {
            continue;
        }
        let mut evidence: Vec<String> = producers.iter().filter(|p| public(p)).map(|p| format!("returned by {} ({})", ix.label(&p.id), ix.at(p))).collect();
        evidence.extend(consumers.iter().map(|c| format!("taken by {} ({})", ix.label(&c.id), ix.at(c))));
        let taken_by: Vec<String> = consumers.iter().map(|c| ix.label(&c.id)).collect();
        out.push(ix.finding(
            "sums",
            s,
            format!("{} ({}) is handed on unopened: {} {} it", s.name, variants.join(" | "), list(&taken_by), if taken_by.len() == 1 { "takes" } else { "take" }),
            evidence,
            format!(
                "Each of them chooses again between {}. Does each need all of them, or could the code that makes a {} hand on only what each one uses?",
                list(&variants),
                s.name
            ),
        ));
    }
    out
}

// ---------- overrides ----------

/// An override that gives something its base does not promise: a caller written against the
/// base cannot take it in its place (Liskov). A subclass of the promised type is fine.
fn overrides(ix: &Index) -> Vec<Finding> {
    const CONSTRUCTORS: &[&str] = &["__init__", "__new__", "__post_init__", "__init_subclass__", "new", "default"];
    let mut broken: Vec<(&Node, Vec<&Node>)> = vec![];
    for m in ix.g.nodes.iter().filter(|n| n.kind == NodeKind::Method && !CONSTRUCTORS.contains(&n.name.as_str())) {
        let Some(cls) = m.cls.as_deref() else { continue };
        let Some(base) = ix.ancestors(cls).into_iter().find_map(|a| ix.node(&format!("{a}.{}", m.name))) else { continue };
        if base.returns.is_empty() || m.returns.is_empty() || same_type(&base.returns, &m.returns) {
            continue;
        }
        if base.returns.contains("Self") || m.returns.contains("Self") {
            continue;
        }
        // a subclass of the promised type keeps the promise; only a bare name says exactly which type
        let bare = |s: &str| s.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | '"' | '\''));
        if bare(&base.returns) && bare(&m.returns) {
            let (promised, given) = (ix.targets(&base.id, EdgeKind::Returns), ix.targets(&m.id, EdgeKind::Returns));
            if let ([p], [t]) = (promised.as_slice(), given.as_slice()) {
                if ix.ancestors(t).contains(p) {
                    continue;
                }
            }
        }
        match broken.iter_mut().find(|(b, _)| b.id == base.id) {
            Some((_, ms)) => ms.push(m),
            None => broken.push((base, vec![m])),
        }
    }
    broken
        .into_iter()
        .map(|(base, ms)| {
            let evidence = ms.iter().map(|m| format!("{} gives {} ({})", ix.label(&m.id), m.returns, ix.at(m))).collect();
            let classes: Vec<String> = ms.iter().filter_map(|m| m.cls.as_deref()).map(|c| ix.label(c)).collect();
            let base_cls = base.cls.as_deref().map(|c| ix.label(c)).unwrap_or_default();
            ix.finding(
                "overrides",
                base,
                format!(
                    "{} promises {}; {} something else",
                    ix.label(&base.id),
                    base.returns,
                    if ms.len() == 1 { "1 override gives".to_string() } else { format!("{} overrides give", ms.len()) },
                ),
                evidence,
                format!(
                    "Can every caller of {} take {} in its place? If not, {} {} a {base_cls}, or {} one?",
                    ix.label(&base.id),
                    list(&ms.iter().map(|m| ix.label(&m.id)).collect::<Vec<_>>()),
                    if classes.len() == 1 { "is" } else { "are" },
                    list(&classes),
                    if classes.len() == 1 { "does it use" } else { "do they use" },
                ),
            )
        })
        .collect()
}

// ---------- places ----------

/// A file or URL that library code names itself. Where data lives is decided where the
/// program starts: a script, a CLI, a test, a notebook. Code those call takes the place as an
/// argument, so each of them can choose.
fn places(ix: &Index) -> Vec<Finding> {
    let starts: HashSet<&str> = ix
        .g
        .nodes
        .iter()
        .filter(|n| n.entry.as_ref().map_or(false, |e| e.kind != EntryKind::Export))
        .map(|n| n.module.as_str())
        .collect();
    let mut out = vec![];
    for n in &ix.g.nodes {
        if starts.contains(n.module.as_str()) {
            continue;
        }
        let fixed: Vec<String> = n.touches.iter().filter(|t| t.fixed).map(|t| format!("{}({})", t.op, t.what)).collect();
        if fixed.is_empty() {
            continue;
        }
        let what = n.touches.iter().filter(|t| t.fixed).map(|t| if t.artifact.is_empty() { t.what.clone() } else { t.artifact.clone() }).collect::<Vec<_>>();
        out.push(ix.finding(
            "places",
            n,
            format!("{} decides itself where data lives: {}", ix.label(&n.id), list(&what)),
            fixed,
            format!(
                "Taken as a parameter, a script, a test and a notebook can each choose. Who should decide where this lives: {}, or whoever calls it?",
                ix.label(&n.id)
            ),
        ));
    }
    out
}

// ---------- changes ----------

/// A function that changes an argument in place while its signature does not say so. Rust's
/// `&mut T` says so; Python cannot, so there the change is better made by the object itself
/// (a method), or replaced by a new value handed back.
fn changes(ix: &Index) -> Vec<Finding> {
    let mut out = vec![];
    for e in ix.g.edges.iter().filter(|e| e.kind == EdgeKind::Mutates) {
        let Some(input) = e.via.as_deref().filter(|v| *v != "self") else { continue };
        let Some(f) = ix.node(&e.src).filter(|f| is_function(f)) else { continue };
        let declared = f.params.iter().find(|p| param_name(p) == input).map_or(false, |p| p.split_once(':').map_or(false, |(_, t)| t.trim().starts_with("&mut")));
        if declared {
            continue;
        }
        let ty = ix.label(&e.dst);
        out.push(ix.finding(
            "changes",
            f,
            format!("{} changes the {ty} it is handed as `{input}`", ix.label(&f.id)),
            vec![signature(f)],
            format!(
                "Nothing in the signature shows that `{input}` changes. Should {ty} change itself through a method of its own, or should {} hand back a new {ty}?",
                ix.label(&f.id)
            ),
        ));
    }
    out
}
