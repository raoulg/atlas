//! `check: Graph -> [Finding]`: the design principles Atlas stands on, computed from the graph.
//!
//! A finding names its principle, the symbol it is about, the evidence the graph holds and a
//! question for the author. It coaches; it does not fix. Atlas sees types, routes and places,
//! so these are the principles that live there; what a single line should look like belongs
//! to ruff, pyright and clippy. `docs/principles.md` explains each one.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::path::Path;

use serde::Serialize;

use crate::language::author_declares;
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
    Principle { id: "choices", asks: "A choice is made once, in a place with a name." },
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

/// `docs/principles.md`, the explanation of every principle, carried in the binary.
const DOCS: &str = include_str!("../../docs/principles.md");

/// A principle's section of the docs: what it asks, why, its PEP or Rust idiom, how Atlas
/// sees it, an example.
pub fn explain(id: &str) -> Option<&'static str> {
    let marker = format!("Id: `{id}`.");
    DOCS.split("\n## ").skip(1).find(|s| s.contains(&marker)).map(|s| s.trim_end())
}

/// The findings as text, grouped by principle, each ending in its question; the principles
/// with nothing to ask are named at the end.
pub fn report(root: &str, findings: &[Finding]) -> String {
    let n = findings.len();
    let mut out = format!("{root} · {n} {}\n", if n == 1 { "finding" } else { "findings" });
    let mut clear = vec![];
    for p in PRINCIPLES {
        let mine: Vec<&Finding> = findings.iter().filter(|f| f.principle == p.id).collect();
        if mine.is_empty() {
            clear.push(p.id);
            continue;
        }
        out.push_str(&format!("\n{} · {}\n", p.id, p.asks));
        for f in mine {
            out.push_str(&format!("  {}:{}  {}\n", f.file, f.line, f.what));
            for e in &f.evidence {
                out.push_str(&format!("      {e}\n"));
            }
            out.push_str(&format!("    ? {}\n", f.question));
        }
    }
    if !clear.is_empty() {
        out.push_str(&format!("\nnothing to ask about {}\n", clear.join(", ")));
    }
    out
}

/// Every finding, by principle in the order of `PRINCIPLES`, then by place in the code.
pub fn check(g: &Graph) -> Vec<Finding> {
    let ix = Index::new(g);
    let mut out = vec![];
    for rule in [doors, sums, overrides, places, changes, choices] {
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
        Path::new(&n.file).strip_prefix(root).map(crate::tree::path_str).unwrap_or_else(|_| n.file.clone())
    }

    fn at(&self, n: &Node) -> String {
        format!("{}:{}", self.file(n), n.span.map_or(1, |s| s.0))
    }

/// A class's public methods (and its constructor), its own and those it inherits from
    /// the package's classes, each from the nearest class that defines it.
    fn interface(&self, cls: &str) -> Vec<&'g Node> {
        let mut seen = HashSet::new();
        let mut out = vec![];
        for c in std::iter::once(cls).chain(self.ancestors(cls)) {
            let Some(n) = self.node(c) else { continue };
            for m in n.methods.iter().filter_map(|m| self.node(&format!("{c}.{m}"))) {
                if (public(m) || m.name == "__init__") && seen.insert(m.name.as_str()) {
                    out.push(m);
                }
            }
        }
        out
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
    let declared = author_declares(&g.language);
    // `src/` is where packages live, not a name: installed, it collides with every other project laid out the same way
    if declared && g.root == "src" {
        if let Some(root) = ix.node(&g.root) {
            out.push(ix.finding(
                "doors",
                root,
                "the package is called src, the folder packages live in".into(),
                vec![format!("{}: the layout folder is the import name, `from src.… import …`", ix.file(root))],
                "Under src/<name>/ with a build system in pyproject.toml, `uv sync` installs it as <name>, and `from <name> import …` works the same in a script, a test and a notebook. What is this package called?".into(),
            ));
        }
    } else if exports.is_empty() && declared {
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
    // a method two exported classes inherit is one door to fix
    let mut asked = HashSet::new();
    for e in exports {
        let fns: Vec<&Node> = match e.kind {
            NodeKind::Function => vec![e],
            NodeKind::Class => ix.interface(&e.id),
            _ => vec![],
        };
        for f in fns.into_iter().filter(|f| asked.insert(f.id.as_str())) {
            let untyped: Vec<String> = f
                .params
                .iter()
                .filter(|p| !p.contains(':') && !is_receiver(p) && !p.starts_with('*'))
                .map(|p| format!("`{}`", param_name(p)))
                .collect();
            let gives_unknown = declared && f.returns.is_empty() && f.name != "__init__";
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
        if base.returns.contains("Self") || m.returns.contains("Self") || keeps(ix, base, m) {
            continue;
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

/// Whether what an override gives is something its base promised: the same shape of
/// annotation with each array at least as specific (`Float[Tensor, "batch dim"]` where
/// `Float[Tensor, "..."]` is promised), or, when the base names one type, a subclass of it,
/// one of the variants of a sum it names, or an array of a shape it allows.
fn keeps(ix: &Index, base: &Node, m: &Node) -> bool {
    let (promised, given) = (arrays(&base.returns), arrays(&m.returns));
    if promised.0 == given.0 && promised.1.len() == given.1.len() && promised.1.iter().zip(&given.1).all(|(p, g)| array_fits(g, p)) {
        return true;
    }
    let bare = base.returns.trim_matches(|c| c == '"' || c == '\'').chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '.'));
    if !bare {
        return false;
    }
    let mut allowed: Vec<&str> = vec![];
    for p in ix.targets(&base.id, EdgeKind::Returns) {
        allowed.push(p);
        allowed.extend(ix.targets(p, EdgeKind::Variant));
    }
    let gives = ix.targets(&m.id, EdgeKind::Returns);
    !allowed.is_empty()
        && !gives.is_empty()
        && gives.iter().all(|g| allowed.iter().any(|p| p == g || ix.ancestors(g).contains(p) || array_id_fits(g, p)))
}

/// An array annotation, jaxtyping style: dtype, array type and dims (`Float`, `Tensor`, `batch dim`).
#[derive(PartialEq, Debug)]
struct Array {
    dtype: String,
    array: String,
    dims: Vec<String>,
}

/// An annotation with its arrays taken out: `dict[str, list[Float[Tensor, "_ dim"]]]` is
/// `dict[str,list[#]]` and one array.
fn arrays(annotation: &str) -> (String, Vec<Array>) {
    let re = regex::Regex::new(r#"([A-Za-z_][\w.]*)\[\s*([A-Za-z_][\w.]*)\s*,\s*["']([^"']*)["']\s*\]"#).unwrap();
    let found = re
        .captures_iter(annotation)
        .map(|c| Array {
            dtype: c[1].rsplit('.').next().unwrap_or("").to_string(),
            array: c[2].rsplit('.').next().unwrap_or("").to_string(),
            dims: c[3].split_whitespace().map(str::to_string).collect(),
        })
        .collect();
    let skeleton = re.replace_all(annotation, "#").chars().filter(|c| !c.is_whitespace() && *c != '"' && *c != '\'').collect();
    (skeleton, found)
}

/// `ext:jaxtyping.Float[Tensor, batch dim]` as an array.
fn array_of_id(id: &str) -> Option<Array> {
    let rest = id.strip_prefix("ext:jaxtyping.")?;
    let (dtype, inner) = rest.split_once('[')?;
    let (array, dims) = inner.strip_suffix(']')?.split_once(',')?;
    Some(Array { dtype: dtype.into(), array: array.trim().into(), dims: dims.split_whitespace().map(str::to_string).collect() })
}

fn array_id_fits(given: &str, promised: &str) -> bool {
    matches!((array_of_id(given), array_of_id(promised)), (Some(g), Some(p)) if array_fits(&g, &p))
}

/// A given array keeps a promised one when dtype and array type agree and the promised dims
/// admit the given ones: `...` (or `*name`) any run of dims, a name or `_` any one dim, a
/// number only itself.
fn array_fits(given: &Array, promised: &Array) -> bool {
    if given.dtype != promised.dtype || given.array != promised.array {
        return false;
    }
    let one = |p: &str, g: &str| !p.chars().all(|c| c.is_ascii_digit()) || p == g;
    let (p, g) = (&promised.dims, &given.dims);
    match p.iter().position(|d| d == "..." || d.starts_with('*')) {
        None => p.len() == g.len() && p.iter().zip(g).all(|(p, g)| one(p, g)),
        Some(i) => {
            let tail = p.len() - i - 1;
            g.len() >= i + tail && p[..i].iter().zip(g).all(|(p, g)| one(p, g)) && p[i + 1..].iter().zip(&g[g.len() - tail..]).all(|(p, g)| one(p, g))
        }
    }
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
        let fixed: Vec<String> = n
            .touches
            .iter()
            .filter(|t| t.fixed)
            .map(|t| format!("{}({})", t.op, if t.artifact.is_empty() { &t.what } else { &t.artifact }))
            .collect();
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

// ---------- choices ----------

/// Two ways a choice goes unnamed. The same literal tested against the same value in several
/// places: what that case means is spelled out again in each of them, and a new case is a
/// change in all of them. And alternatives that each produce the same type, sitting in a body
/// between other steps: how to get that value is a step of its own, without a name.
fn choices(ix: &Index) -> Vec<Finding> {
    let mut out = vec![];
    for f in ix.g.nodes.iter().filter(|n| is_function(n)) {
        out.extend(repeated(ix, f));
        out.extend(unnamed(ix, f));
    }
    out
}

/// The literals a body tests a value it was handed against in more than one place: a
/// parameter, or `self`, that picks behaviour by name.
fn repeated(ix: &Index, f: &Node) -> Vec<Finding> {
    let handed: HashSet<&str> = f.params.iter().map(|p| param_name(p).trim_start_matches('&').trim_start_matches("mut ").trim()).collect();
    let root = |s: &str| s.split(|c: char| !(c.is_alphanumeric() || c == '_')).next().unwrap_or("").to_string();
    // a place is a choice, or a comparison that decides none on its own line
    let mut places: BTreeMap<&str, BTreeMap<&str, BTreeSet<(Option<u32>, u32)>>> = BTreeMap::new();
    for c in f.compares.iter().filter(|c| handed.contains(root(&c.subject).as_str())) {
        let place = match c.choice {
            Some(ch) => (Some(ch), 0),
            None => (None, c.line),
        };
        for lit in &c.cases {
            places.entry(c.subject.as_str()).or_default().entry(lit.as_str()).or_default().insert(place);
        }
    }
    let mut out = vec![];
    for (subject, lits) in places {
        let spread: Vec<(&str, usize)> = lits.iter().filter(|(_, ps)| ps.len() > 1).map(|(l, ps)| (*l, ps.len())).collect();
        if spread.is_empty() {
            continue;
        }
        let n_places = f.compares.iter().filter(|c| c.subject == subject).map(|c| c.choice.map_or((None, c.line), |ch| (Some(ch), 0))).collect::<BTreeSet<_>>().len();
        let named: Vec<String> = spread.iter().map(|(l, n)| format!("\"{l}\" ({n}×)")).collect();
        let evidence = f
            .compares
            .iter()
            .filter(|c| c.subject == subject)
            .map(|c| format!("line {}: {subject} against {}", c.line, c.cases.iter().map(|x| format!("\"{x}\"")).collect::<Vec<_>>().join(", ")))
            .collect();
        out.push(ix.finding(
            "choices",
            f,
            format!("{} tests `{subject}` in {n_places} places; {} come back", ix.label(&f.id), list(&named)),
            evidence,
            format!(
                "What each case means is spelled out again in each place, so a new case is a change in all of them. Could the choice be made once, in a table or a type that says what each `{subject}` stands for?"
            ),
        ));
    }
    out
}

/// Alternatives that each produce the same type, in a body that does more than choose.
fn unnamed<'g>(ix: &Index<'g>, f: &Node) -> Vec<Finding> {
    let calls: Vec<&'g Edge> = ix.out.get(f.id.as_str()).into_iter().flatten().filter(|e| matches!(e.kind, EdgeKind::Calls | EdgeKind::Instantiates)).copied().collect();
    // the type a step hands on: what it constructs, or the class it returns
    let made = |e: &'g Edge| -> Vec<&'g str> {
        if e.kind == EdgeKind::Instantiates {
            return vec![e.dst.as_str()];
        }
        ix.targets(&e.dst, EdgeKind::Returns).into_iter().filter(|t| ix.node(t).map_or(false, |n| n.kind == NodeKind::Class)).collect()
    };
    // the choices the body itself makes; one nested in an arm is part of the arm
    let choice_ids: BTreeSet<u32> = calls.iter().filter_map(|e| e.arms.first().map(|(c, _)| *c)).collect();
    let mut out = vec![];
    for c in choice_ids {
        let inside: Vec<&'g Edge> = calls.iter().copied().filter(|e| e.arms.first().map_or(false, |(ch, _)| *ch == c)).collect();
        if inside.len() == calls.len() {
            continue; // the body is the choice: it has a name, the function's
        }
        // type -> arm -> the steps in that arm that produce it
        let mut by_type: BTreeMap<&str, BTreeMap<u32, Vec<&str>>> = BTreeMap::new();
        for e in &inside {
            let arm = e.arms.first().map_or(0, |(_, a)| *a);
            for t in made(e) {
                by_type.entry(t).or_default().entry(arm).or_default().push(e.dst.as_str());
            }
        }
        for (t, arms) in by_type.into_iter().filter(|(_, arms)| arms.len() > 1) {
            let steps: Vec<String> = arms.values().flatten().map(|s| ix.label(s)).collect();
            let others: Vec<String> = calls.iter().filter(|e| e.arms.first().map_or(true, |(ch, _)| *ch != c)).map(|e| ix.label(&e.dst)).collect();
            let ty = ix.label(t);
            out.push(ix.finding(
                "choices",
                f,
                format!("{} chooses where its {ty} comes from ({}) between its other steps", ix.label(&f.id), steps.join(" | ")),
                vec![format!("alternatives: {}", steps.join(" | ")), format!("around them: {}", list(&others))],
                format!("Where the {ty} comes from is a step of its own, with {} as its arms. What would that step be called?", list(&steps)),
            ));
        }
    }
    out
}
