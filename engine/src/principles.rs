//! `check: Graph -> [Finding]`: the design principles Atlas stands on, computed from the graph.
//!
//! A finding names its principle, the symbol it is about, the evidence the graph holds and a
//! question for the author. It coaches; it does not fix. Atlas sees types, routes and places,
//! so these are the principles that live there; what a single line should look like belongs
//! to ruff, pyright and clippy. `docs/reference/principles.md` explains each one.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::path::Path;

use serde::Serialize;

use crate::model::{Dir, Edge, EdgeKind, EntryKind, Graph, Held, Leaves, Node, NodeKind, Site, Touch, World};

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
    Principle { id: "gives", asks: "A function whose value is used says what it gives back." },
    Principle { id: "choices", asks: "A choice is made once, in a place with a name." },
    Principle { id: "nothing", asks: "A call leaves something behind." },
    Principle { id: "layers", asks: "The packages of a part depend on one another in one direction." },
    Principle { id: "lens", asks: "What a lens puts, it gets back." },
    Principle { id: "prophecy", asks: "Code that is built does not lean on code that is not." },
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

/// `docs/reference/principles.md`, the explanation of every principle, carried in the binary.
const DOCS: &str = include_str!("../../docs/reference/principles.md");

/// A principle's section of the docs: what it asks, why, its PEP or Rust idiom, how Atlas
/// sees it, an example.
pub fn explain(id: &str) -> Option<&'static str> {
    let marker = format!("Id: `{id}`.");
    DOCS.split("\n## ").skip(1).find(|s| s.contains(&marker)).map(|s| s.trim_end())
}

/// What a piece of a report is, so that whoever prints it can give each piece its own look.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ink {
    /// A part's name.
    Part,
    /// A number of questions.
    Count,
    /// A principle's id.
    Principle,
    /// What a principle asks.
    Asks,
    /// The reason all of a principle's questions give, said once above them.
    Why,
    /// Where a finding stands, `file:line`.
    At,
    /// What a finding says, and its question.
    Text,
    /// The signatures, edges and touches that show a finding.
    Evidence,
    /// The mark before a question.
    Ask,
    /// A count drawn as a bar, for the eye; plain text leaves it out.
    Bar,
    /// What there is nothing to say about.
    Quiet,
}

/// How a report looks: what each piece is wrapped in, and the width its lines wrap at.
pub struct Look {
    pub ink: fn(Ink, &str) -> String,
    pub width: Option<usize>,
}

impl Look {
    /// Text without colour, one line per piece: for an agent, a pipe or a file.
    pub fn plain() -> Look {
        Look { ink: |ink, s| if ink == Ink::Bar { String::new() } else { s.to_string() }, width: None }
    }

    fn paint(&self, ink: Ink, s: &str) -> String {
        (self.ink)(ink, s)
    }

    /// `text` in `ink`, starting at column `start` and wrapped onto lines that start at column
    /// `indent`.
    fn wrapped(&self, ink: Ink, text: &str, start: usize, indent: usize) -> String {
        let mut lines = vec![];
        let mut line = String::new();
        let mut col = start;
        for word in text.split(' ') {
            let w = word.chars().count();
            if !line.is_empty() && self.width.is_some_and(|width| col + 1 + w > width) {
                lines.push(std::mem::take(&mut line));
                col = indent;
            }
            if !line.is_empty() {
                line.push(' ');
                col += 1;
            }
            line += word;
            col += w;
        }
        lines.push(line);
        lines.iter().map(|l| self.paint(ink, l)).collect::<Vec<_>>().join(&format!("\n{}", " ".repeat(indent)))
    }
}

fn questions(n: usize) -> String {
    format!("{n} {}", if n == 1 { "question" } else { "questions" })
}

/// A question's sentences, each with its closing mark.
fn sentences(q: &str) -> Vec<&str> {
    let mut out = vec![];
    let mut start = 0;
    for (i, b) in q.bytes().enumerate() {
        if matches!(b, b'.' | b'?' | b'!') && q.as_bytes().get(i + 1) == Some(&b' ') {
            out.push(&q[start..=i]);
            start = i + 2;
        }
    }
    if start < q.len() {
        out.push(&q[start..]);
    }
    out
}

/// How many sentences all the questions open with alike, keeping at least the last one of each:
/// the reason a principle gives, said once above its questions.
fn shared_lead(qs: &[Vec<&str>]) -> usize {
    if qs.len() < 2 {
        return 0;
    }
    let most = qs.iter().map(Vec::len).min().unwrap_or(0).saturating_sub(1);
    (0..most).take_while(|&i| qs.iter().all(|q| q[i] == qs[0][i])).count()
}

/// The findings of one part as text: by principle, what it asks and the reason its questions
/// share, then each finding with its evidence and what is left of its question.
pub fn report(root: &str, findings: &[Finding], look: &Look) -> String {
    let mut out = format!("{} · {}\n", look.paint(Ink::Part, root), look.paint(Ink::Count, &questions(findings.len())));
    for p in PRINCIPLES {
        let mine: Vec<&Finding> = findings.iter().filter(|f| f.principle == p.id).collect();
        if mine.is_empty() {
            continue;
        }
        let asked: Vec<Vec<&str>> = mine.iter().map(|f| sentences(&f.question)).collect();
        let lead = shared_lead(&asked);
        let hang = p.id.len() + 3;
        out += &format!("\n{} · {}\n", look.paint(Ink::Principle, p.id), look.wrapped(Ink::Asks, p.asks, hang, hang));
        if lead > 0 {
            out += &format!("{}{}\n", " ".repeat(hang), look.wrapped(Ink::Why, &asked[0][..lead].join(" "), hang, hang));
        }
        for (f, q) in mine.iter().zip(&asked) {
            // a finding about packages together stands in no one file
            let (at, gap) = if f.file.is_empty() { (String::new(), "") } else { (format!("{}:{}", f.file, f.line), "  ") };
            out += &format!("\n  {}{gap}{}\n", look.paint(Ink::At, &at), look.wrapped(Ink::Text, &f.what, 2 + at.len() + gap.len(), 4));
            for e in &f.evidence {
                out += &format!("      {}\n", look.wrapped(Ink::Evidence, e, 6, 8));
            }
            out += &format!("    {} {}\n", look.paint(Ink::Ask, "?"), look.wrapped(Ink::Text, &q[lead..].join(" "), 6, 6));
        }
    }
    out
}

/// Every part's questions counted by principle, to read after the reports: a row for each
/// principle with something to ask, a column for each part, and on one line the principles in
/// `asked` with nothing to ask.
pub fn summary(parts: &[(&str, &[Finding])], asked: &[&Principle], look: &Look) -> String {
    let count = |part: &[Finding], p: &Principle| part.iter().filter(|f| f.principle == p.id).count();
    let total: usize = parts.iter().map(|(_, fs)| fs.iter().filter(|f| asked.iter().any(|p| p.id == f.principle)).count()).sum();
    let rows: Vec<(&Principle, Vec<usize>)> = asked
        .iter()
        .map(|p| (*p, parts.iter().map(|(_, fs)| count(fs, p)).collect::<Vec<_>>()))
        .filter(|(_, n)| n.iter().any(|&n| n > 0))
        .collect();
    let quiet: Vec<String> = asked.iter().filter(|p| !rows.iter().any(|(r, _)| r.id == p.id)).map(|p| p.id.to_string()).collect();
    let rule = "─".repeat(look.width.unwrap_or(60).min(60));
    let mut out = format!("\n{}\n", look.paint(Ink::Quiet, &rule));
    out += &look.paint(Ink::Count, &questions(total));
    if parts.len() > 1 {
        out += &format!(" in {} parts", parts.len());
    }
    out += "\n";
    if !rows.is_empty() {
        let label = rows.iter().map(|(p, _)| p.id.len()).max().unwrap_or(0);
        let widths: Vec<usize> = parts.iter().map(|(name, _)| name.chars().count().max(3)).collect();
        out += &format!("  {:label$}", "");
        for ((name, _), w) in parts.iter().zip(&widths) {
            out += &format!("  {}", look.paint(Ink::Part, &format!("{name:>w$}")));
        }
        out += "\n";
        let most = rows.iter().map(|(_, n)| n.iter().sum::<usize>()).max().unwrap_or(1);
        for (p, ns) in &rows {
            out += &format!("  {}", look.paint(Ink::Principle, &format!("{:label$}", p.id)));
            for (n, w) in ns.iter().zip(&widths) {
                out += &match n {
                    0 => format!("  {}", look.paint(Ink::Quiet, &format!("{:>w$}", "·"))),
                    n => format!("  {}", look.paint(Ink::Count, &format!("{n:>w$}"))),
                };
            }
            let sum: usize = ns.iter().sum();
            let bar = look.paint(Ink::Bar, &"▇".repeat(if most > 24 { (sum * 24).div_ceil(most) } else { sum }));
            if !bar.is_empty() {
                out += &format!("  {bar}");
            }
            out += "\n";
        }
    }
    if !quiet.is_empty() {
        out += &format!("  {}\n", look.wrapped(Ink::Quiet, &format!("nothing to ask: {}", quiet.join(", ")), 2, 2));
    }
    out
}

/// Every finding, by principle in the order of `PRINCIPLES`, then by place in the code.
pub fn check(g: &Graph) -> Vec<Finding> {
    let ix = Index::new(g);
    let answers = ix.answers();
    let mut out = vec![];
    type Rule = fn(&Index) -> Vec<Finding>;
    let rules: [Rule; 11] = [doors, sums, overrides, places, changes, gives, choices, nothing, layers, lens, prophecy];
    for (i, rule) in rules.into_iter().enumerate() {
        // a library read as code is not the project's to change
        let mut found: Vec<Finding> = rule(&ix).into_iter().filter(|f| ix.node(&f.node).map_or(true, |n| !n.library)).collect();
        // a prophecy is a todo, not a finding: its empty bodies are what it is, and the rule that
        // asks about prophecies is the only one that speaks of them
        if i + 1 != rules.len() {
            found.retain(|f| ix.node(&f.node).map_or(true, |n| !ix.prophetic(n)));
        }
        // what a prophecy supersedes is still asked about; the prophecy is the answer
        for f in &mut found {
            if let Some(p) =
                ix.node(&f.node).and_then(|n| answers.get(n.id.as_str()).or_else(|| n.cls.as_deref().and_then(|c| answers.get(c))))
            {
                f.evidence.push(format!("answered by the prophecy `{}`, which supersedes it", ix.label(p)));
            }
        }
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

    /// Whether a node is sketch code that is not built: a prophecy, or a member of one.
    fn prophetic(&self, n: &Node) -> bool {
        n.prophecy.is_some() || n.cls.as_deref().and_then(|c| self.node(c)).is_some_and(|c| c.prophecy.is_some())
    }

    /// What the prophecy `p` supersedes, when the code holds it.
    fn superseded(&self, p: &Node) -> Option<&'g Node> {
        self.node(p.prophecy.as_ref()?.replaces.as_deref()?)
    }

    /// The prophecy that supersedes each node it names.
    fn answers(&self) -> HashMap<&'g str, &'g str> {
        self.g.nodes.iter().filter_map(|p| Some((self.superseded(p)?.id.as_str(), p.id.as_str()))).collect()
    }

    fn finding(&self, principle: &'static str, n: &Node, what: String, evidence: Vec<String>, question: String) -> Finding {
        Finding { principle, node: n.id.clone(), file: self.file(n), line: n.span.map_or(1, |s| s.0), what, evidence, question }
    }

    fn file(&self, n: &Node) -> String {
        let root = Path::new(&self.g.project_root);
        Path::new(&n.file).strip_prefix(root).map(crate::tree::path_str).unwrap_or_else(|_| n.file.clone())
    }

    /// What the language of a node's part leaves to its author.
    fn leaves(&self, id: &str) -> Leaves {
        let part = self.node(id).map_or("", |n| n.part.as_str());
        self.g.parts.iter().find(|p| p.id == part).or(self.g.parts.first()).map_or_else(Leaves::default, |p| p.leaves)
    }

    /// Whether a reader of `f`'s declaration does not see what it gives: its language leaves the
    /// return type to the author, and the author left it out. A constructor gives its class.
    fn gives_unknown(&self, f: &Node) -> bool {
        self.leaves(&f.id).returns && f.returns.is_none() && !f.ctor
    }

    fn at(&self, n: &Node) -> String {
        format!("{}:{}", self.file(n), n.span.map_or(1, |s| s.0))
    }

    /// Whether the method `m` is defined in `cls` itself rather than inherited from a base.
    fn declares(&self, cls: &str, m: &Node) -> bool {
        m.id.rsplit_once('.').map_or(false, |(owner, _)| owner == cls)
    }

    /// A class's public methods (and its constructor), its own and those it inherits from
    /// the package's classes, each from the nearest class that defines it.
    fn interface(&self, cls: &str) -> Vec<&'g Node> {
        let mut seen = HashSet::new();
        let mut out = vec![];
        for c in std::iter::once(cls).chain(self.ancestors(cls)) {
            let Some(n) = self.node(c) else { continue };
            for m in n.methods.iter().filter_map(|m| self.node(&format!("{c}.{m}"))) {
                if (!m.private || m.ctor) && seen.insert(m.name.as_str()) {
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

fn signature(n: &Node) -> String {
    let ret = n.returns.as_ref().map_or(String::new(), |r| format!(" -> {}", r.written));
    format!("{}({}){ret}", n.name, n.params.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(", "))
}

/// What a function says it gives, as written.
fn written(n: &Node) -> &str {
    n.returns.as_ref().map_or("", |r| r.written.as_str())
}

/// `a, b and c`, each named once.
fn list(xs: &[String]) -> String {
    let mut seen = HashSet::new();
    let xs: Vec<String> = xs.iter().filter(|x| seen.insert(x.as_str())).cloned().collect();
    match xs.as_slice() {
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
/// The functions that are doors: each exported function, and the public methods of each exported
/// class; a method two exported classes inherit is one door.
fn door_fns<'g>(ix: &Index<'g>) -> Vec<&'g Node> {
    let mut asked = HashSet::new();
    let mut out = vec![];
    for e in ix.g.nodes.iter().filter(|n| n.entry.as_ref().map_or(false, |e| e.kind == EntryKind::Export)) {
        let fns: Vec<&Node> = match e.kind {
            NodeKind::Function => vec![e],
            NodeKind::Class => ix.interface(&e.id),
            _ => vec![],
        };
        out.extend(fns.into_iter().filter(|f| asked.insert(f.id.as_str())));
    }
    out
}

fn doors(ix: &Index) -> Vec<Finding> {
    let g = ix.g;
    let exports: Vec<&Node> = g.nodes.iter().filter(|n| n.entry.as_ref().map_or(false, |e| e.kind == EntryKind::Export)).collect();
    let mut out = vec![];
    let declared = ix.leaves(&g.root).exports;
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
                format!(
                    "Named in `__all__`, each door becomes a route Atlas can draw. What may a user of {} import, and from where?",
                    g.root
                ),
            ));
        }
    }
    for f in door_fns(ix) {
        // a function handed to a call where it is declared takes its parameters' types from the call
        let untyped: Vec<String> = f
            .params
            .iter()
            .filter(|p| f.typed_by.is_empty() && p.ty.is_empty() && !p.receiver && !p.rest)
            .map(|p| format!("`{}`", p.name))
            .collect();
        let gives_unknown = ix.gives_unknown(f);
        if untyped.is_empty() && !gives_unknown {
            continue;
        }
        let what = match (untyped.is_empty(), gives_unknown) {
            (false, true) => format!("{} does not say what {} are, nor what it gives", ix.label(&f.id), list(&untyped)),
            (false, false) => {
                format!("{} does not say what {} {}", ix.label(&f.id), list(&untyped), if untyped.len() == 1 { "is" } else { "are" })
            }
            _ => format!("{} does not say what it gives", ix.label(&f.id)),
        };
        out.push(ix.finding(
            "doors",
            f,
            what,
            vec![signature(f)],
            format!(
                "Until its signature says so, a reader and Atlas both lose the route at this door. What does {} take and give?",
                ix.label(&f.id)
            ),
        ));
    }
    out
}

// ---------- sums ----------

/// A sum type that a module returns and that functions in other modules take as input: each
/// of them has to choose between the variants again. Unwrapped where it arises, the choice is
/// made once, and what travels on is the variant each consumer needs. A library's sum arises
/// where a call gives it, outside every module of the program, so a public function that takes
/// one has been handed it unopened, whatever module it is in; the finding stands at the first
/// that takes it, where the program has code to change.
fn sums(ix: &Index) -> Vec<Finding> {
    let mut out = vec![];
    for s in ix.g.nodes.iter().filter(|n| !ix.targets(&n.id, EdgeKind::Variant).is_empty()) {
        let library = s.kind == NodeKind::External;
        let variants: Vec<String> = ix.targets(&s.id, EdgeKind::Variant).iter().map(|v| ix.label(v)).collect();
        let producers: Vec<&Node> = ix.sources(&s.id, EdgeKind::Returns).iter().filter_map(|p| ix.node(p)).collect();
        let homes: HashSet<&str> = producers.iter().map(|p| p.module.as_str()).collect();
        let consumers: Vec<&Node> = ix
            .sources(&s.id, EdgeKind::Takes)
            .iter()
            .filter_map(|c| ix.node(c))
            .filter(|c| !c.private && !homes.contains(c.module.as_str()))
            .collect();
        if (producers.is_empty() && !library) || consumers.is_empty() {
            continue;
        }
        let mut evidence: Vec<String> =
            producers.iter().filter(|p| !p.private).map(|p| format!("returned by {} ({})", ix.label(&p.id), ix.at(p))).collect();
        evidence.extend(consumers.iter().map(|c| format!("taken by {} ({})", ix.label(&c.id), ix.at(c))));
        let taken_by: Vec<String> = consumers.iter().map(|c| ix.label(&c.id)).collect();
        let choose = match variants.as_slice() {
            [one] => format!("Each of them looks again whether it holds a {one}. Could the code that makes a {} hand on only the {one}?", s.name),
            _ => format!(
                "Each of them chooses again between {}. Does each need all of them, or could the code that makes a {} hand on only what each one uses?",
                list(&variants),
                s.name
            ),
        };
        out.push(ix.finding(
            "sums",
            if library { consumers[0] } else { s },
            format!(
                "{} ({}) is handed on unopened: {} {} it",
                s.name,
                variants.join(" | "),
                list(&taken_by),
                if taken_by.len() == 1 { "takes" } else { "take" }
            ),
            evidence,
            choose,
        ));
    }
    out
}

// ---------- overrides ----------

/// An override that does not keep the promise of its base: a caller written against the base
/// cannot take it in its place (Liskov). A subclass of the promised type is fine.
fn overrides(ix: &Index) -> Vec<Finding> {
    let mut broken: Vec<(&Node, Vec<&Node>)> = vec![];
    for m in ix.g.nodes.iter().filter(|n| n.kind == NodeKind::Method && !n.ctor) {
        let Some(cls) = m.cls.as_deref() else { continue };
        let Some(base) = ix.ancestors(cls).into_iter().find_map(|a| ix.node(&format!("{a}.{}", m.name))) else { continue };
        if keeps(ix, base, m) {
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
            let evidence = ms.iter().map(|m| format!("{}{} ({})", ix.label(&m.id), promise(m), ix.at(m))).collect();
            let classes: Vec<String> = ms.iter().filter_map(|m| m.cls.as_deref()).map(|c| ix.label(c)).collect();
            let base_cls = base.cls.as_deref().map(|c| ix.label(c)).unwrap_or_default();
            ix.finding(
                "overrides",
                base,
                format!(
                    "{} promises {}; {} it",
                    ix.label(&base.id),
                    promise(base),
                    if ms.len() == 1 { "1 override breaks".to_string() } else { format!("{} overrides break", ms.len()) },
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

/// What a method promises: what it takes, without the value it runs on, and what it gives, as
/// written (`(rij: tuple[str, int]) -> Versie[R]`).
fn promise(n: &Node) -> String {
    let takes: Vec<String> = n.params.iter().filter(|p| !p.receiver).map(|p| p.to_string()).collect();
    let ret = n.returns.as_ref().map_or(String::new(), |r| format!(" -> {}", r.written));
    format!("({}){ret}", takes.join(", "))
}

/// Whether an override keeps the promise of its base. Where the language's type checker decided
/// it (ty, for Python), by its answer: what the override takes and what it gives, against the
/// base as the subclass sees it, its type parameters filled in. Elsewhere, where both signatures
/// say what they give, by the types they resolve to: each type the override gives is one the
/// base promises, a subclass of one, or a variant of a sum it promises; a promise that resolves
/// to none of the program's types, or a value of the class it runs on, gives these nothing to
/// compare. Either way an array's shape is a promise no checker reads: each array the override
/// gives fits one the base promises (`Float[Tensor, "batch dim"]` where `Float[Tensor, "..."]`
/// is promised).
fn keeps(ix: &Index, base: &Node, m: &Node) -> bool {
    let (promised, given) = (ix.targets(&base.id, EdgeKind::Returns), ix.targets(&m.id, EdgeKind::Returns));
    let allowed: Vec<&str> = promised.iter().flat_map(|p| std::iter::once(*p).chain(ix.targets(p, EdgeKind::Variant))).collect();
    let typed = m.keeps.unwrap_or_else(|| match (&base.returns, &m.returns) {
        (Some(p), Some(g)) if !p.own && !g.own => {
            promised.is_empty()
                || given.iter().all(|g| allowed.iter().any(|p| p == g || ix.ancestors(g).contains(p) || array_id_fits(g, p)))
        }
        _ => true,
    });
    let shapes: Vec<Array> = promised.iter().filter_map(|p| array_of_id(p)).collect();
    typed && (shapes.is_empty() || given.iter().filter_map(|g| array_of_id(g)).all(|g| shapes.iter().any(|p| array_fits(&g, p))))
}

/// An array annotation, jaxtyping style: dtype, array type and dims (`Float`, `Tensor`, `batch dim`).
#[derive(PartialEq, Debug)]
struct Array {
    dtype: String,
    array: String,
    dims: Vec<String>,
}

/// A shaped array's id (`ext:jaxtyping.Float[Tensor, batch dim]`) as an array.
fn array_of_id(id: &str) -> Option<Array> {
    let rest = crate::knowledge::shaped(id)?;
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
            g.len() >= i + tail
                && p[..i].iter().zip(g).all(|(p, g)| one(p, g))
                && p[i + 1..].iter().zip(&g[g.len() - tail..]).all(|(p, g)| one(p, g))
        }
    }
}

// ---------- places ----------

/// A file or URL that library code names itself. Where data lives is decided where the
/// program starts: a script, a CLI, a test, a notebook. Code those call takes the place as an
/// argument, so each of them can choose. Where the program starts is the entry itself (a
/// command, a route, a test), or a file that is one as a whole (a script, a notebook, a test
/// file) with the code it defines; a library function beside an entry is library code.
fn places(ix: &Index) -> Vec<Finding> {
    let entry = |id: &str| ix.node(id).and_then(|n| n.entry.as_ref()).map_or(false, |e| e.kind != EntryKind::Export);
    let mut out = vec![];
    // a file that runs as a whole names its places with the code it defines; a module that runs as
    // a command is imported all the same, and its functions are library code
    let whole =
        |id: &str| ix.node(id).and_then(|n| n.entry.as_ref()).map_or(false, |e| !matches!(e.kind, EntryKind::Export | EntryKind::Cli));
    for n in &ix.g.nodes {
        if entry(&n.id) || whole(&n.module) {
            continue;
        }
        let fixed: Vec<String> = n
            .touches
            .iter()
            .filter(|t| t.fixed)
            .map(|t| format!("{}({})", t.op, if t.artifact.is_empty() { crate::flow::shown(&t.what) } else { t.artifact.clone() }))
            .collect();
        if fixed.is_empty() {
            continue;
        }
        let what = n
            .touches
            .iter()
            .filter(|t| t.fixed)
            .map(|t| if t.artifact.is_empty() { crate::flow::shown(&t.what) } else { t.artifact.clone() })
            .collect::<Vec<_>>();
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
        let Some(input) = e.via.as_deref() else { continue };
        let Some(f) = ix.node(&e.src).filter(|f| is_function(f)) else { continue };
        // an input handed in, not the value the function is called on, that the signature does not say changes
        if !f.params.iter().any(|p| p.name == input && !p.receiver && !p.changes) {
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

// ---------- gives ----------

/// A function whose callers use what it gives back, with no return annotation, where the language
/// leaves the return type to the author: what they get has no type a reader of the declaration
/// sees, so neither a reader nor Atlas can follow it further. A door is asked about by `doors`,
/// on the same condition.
fn gives(ix: &Index) -> Vec<Finding> {
    let doors: HashSet<&str> = door_fns(ix).into_iter().map(|f| f.id.as_str()).collect();
    let mut out = vec![];
    for f in ix.g.nodes.iter().filter(|f| is_function(f) && ix.gives_unknown(f) && !doors.contains(f.id.as_str())) {
        let mut users: Vec<String> = ix
            .into
            .get(f.id.as_str())
            .into_iter()
            .flatten()
            .filter(|e| e.kind == EdgeKind::Calls && e.uses)
            .map(|e| format!("`{}`", ix.label(&e.src)))
            .collect();
        if users.is_empty() {
            continue;
        }
        let name = ix.label(&f.id);
        let more = users.len().saturating_sub(3);
        users.truncate(3);
        if more > 0 {
            users.push(format!("{more} more"));
        }
        out.push(ix.finding(
            "gives",
            f,
            format!("{name} gives back a value its callers use, and does not say what it is"),
            vec![signature(f), format!("used by {}", list(&users))],
            format!(
                "Without a return annotation, what its callers get has no type, and the route stops there. What does {name} give back?"
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
    let handed: HashSet<&str> = f.params.iter().map(|p| p.name.as_str()).collect();
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
        let n_places = f
            .compares
            .iter()
            .filter(|c| c.subject == subject)
            .map(|c| c.choice.map_or((None, c.line), |ch| (Some(ch), 0)))
            .collect::<BTreeSet<_>>()
            .len();
        let named: Vec<String> = spread.iter().map(|(l, n)| format!("\"{l}\" ({n}×)")).collect();
        let evidence = f
            .compares
            .iter()
            .filter(|c| c.subject == subject)
            .map(|c| {
                format!("line {}: {subject} against {}", c.line, c.cases.iter().map(|x| format!("\"{x}\"")).collect::<Vec<_>>().join(", "))
            })
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

/// Alternatives that each produce the same type for one later step: ways to get one value in
/// the middle of a body. Read from the body's steps, one per call with its own arms, as the route
/// draws them, and from what each step is handed; a choice nested in an arm is read among the
/// steps of that arm.
fn unnamed<'g>(ix: &Index<'g>, f: &'g Node) -> Vec<Finding> {
    // the program's own steps: a call into a library is the library's operation, however far
    // Atlas reads its code
    let body: Vec<&'g Site> = f.sites.iter().filter(|s| ix.node(&s.to).map_or(false, |n| !n.library)).collect();
    // the type a step hands on: the class it constructs, or the class it returns
    let made = |s: &'g Site| -> Vec<&'g str> {
        match ix.node(&s.to) {
            Some(n) if n.kind == NodeKind::Class => vec![s.to.as_str()],
            _ => ix
                .targets(&s.to, EdgeKind::Returns)
                .into_iter()
                .filter(|t| ix.node(t).map_or(false, |n| n.kind == NodeKind::Class))
                .collect(),
        }
    };
    // every choice, by the arms around it: the steps of that arm are its scope
    let choices: BTreeSet<(&'g [(u32, u32)], u32)> =
        body.iter().flat_map(|s| (0..s.arms.len()).map(move |d| (&s.arms[..d], s.arms[d].0))).collect();
    let mut out = vec![];
    for (around, c) in choices {
        let d = around.len();
        let (inside, outside): (Vec<&'g Site>, Vec<&'g Site>) =
            body.iter().copied().filter(|s| s.arms.starts_with(around)).partition(|s| s.arms.get(d).map_or(false, |(ch, _)| *ch == c));
        // type -> (arm -> the steps in it that make the value, the steps outside handed it); a
        // value returned, dropped or used inside its arm goes to no step outside
        let mut by_type: BTreeMap<&str, (BTreeMap<u32, BTreeSet<&str>>, Vec<String>)> = BTreeMap::new();
        for next in &outside {
            let handed: HashSet<&str> = next.hands.iter().map(|h| h.name.as_str()).filter(|n| !n.is_empty()).collect();
            let mut arms: BTreeMap<&str, BTreeMap<u32, BTreeSet<&str>>> = BTreeMap::new();
            for s in inside.iter().filter(|s| s.gives.iter().any(|g| handed.contains(g.name.as_str()))) {
                for t in made(s) {
                    arms.entry(t).or_default().entry(s.arms[d].1).or_default().insert(s.to.as_str());
                }
            }
            for (t, arms) in arms.into_iter().filter(|(_, arms)| arms.len() > 1) {
                let (all, nexts) = by_type.entry(t).or_default();
                for (arm, steps) in arms {
                    all.entry(arm).or_default().extend(steps);
                }
                let label = ix.label(&next.to);
                if !nexts.contains(&label) {
                    nexts.push(label);
                }
            }
        }
        for (t, (arms, next)) in by_type {
            let steps: Vec<String> = arms.values().flatten().map(|s| ix.label(s)).collect();
            let ty = ix.label(t);
            out.push(ix.finding(
                "choices",
                f,
                format!("{} chooses where its {ty} comes from ({}) and goes on with it", ix.label(&f.id), steps.join(" | ")),
                vec![format!("alternatives: {}", steps.join(" | ")), format!("then: {}", list(&next))],
                format!(
                    "Where the {ty} comes from is a step of its own, with {} as its arms. What would that step be called?",
                    list(&steps)
                ),
            ));
        }
    }
    out
}

// ---------- nothing ----------

/// A function that is called and leaves nothing behind: it gives nothing back and cannot fail,
/// changes nothing it is handed, and nothing it runs (what it calls, what it builds, the overrides
/// a call may reach) touches a place, changes a value, fails or ends the program. An arrow into the
/// terminal object carries no information. Where Atlas sees all it runs, the function is dead, or
/// changes a value through a field, an item or a loop, which no call shows. Where it runs what
/// Atlas does not see (a call it cannot follow, a library's call no table describes, a method only
/// declared, a value a class keeps), the question is what that leaves behind. A method only
/// declared has no body to ask about; its callers are asked.
fn nothing(ix: &Index) -> Vec<Finding> {
    // a method -> the methods that override it
    let mut overridden: HashMap<String, Vec<&str>> = HashMap::new();
    for m in ix.g.nodes.iter().filter(|n| is_function(n)) {
        for a in m.cls.as_deref().map(|c| ix.ancestors(c)).unwrap_or_default() {
            let base = format!("{a}.{}", m.name);
            if ix.node(&base).is_some() {
                overridden.entry(base).or_default().push(m.id.as_str());
            }
        }
    }
    let leaves_something = |n: &Node| {
        !n.touches.is_empty()
            || n.raises
            || n.calls_handed
            || n.returns.as_ref().map_or(false, |r| r.fails)
            || n.params.iter().any(|p| p.changes)
            || !ix.targets(&n.id, EdgeKind::Mutates).is_empty()
    };
    let mut out = vec![];
    for f in ix.g.nodes.iter().filter(|f| is_function(f) && !f.library && !f.ctor && !f.declared) {
        let gives_nothing = match &f.returns {
            Some(r) => r.value.is_empty() && !r.own,
            None => !ix.leaves(&f.id).returns,
        };
        let callers: Vec<&str> =
            ix.sources(&f.id, EdgeKind::Calls).into_iter().filter(|c| *c != f.id && ix.node(c).map_or(false, |n| !n.library)).collect();
        if !gives_nothing || callers.is_empty() {
            continue;
        }
        // what it runs, down to the end, and what of that Atlas does not see
        let mut runs: Vec<&str> = vec![];
        let mut blind: Vec<Blind> = vec![];
        let mut seen: HashSet<&str> = HashSet::from([f.id.as_str()]);
        let mut todo = vec![f.id.as_str()];
        let mut something = false;
        while let Some(id) = todo.pop() {
            let Some(n) = ix.node(id) else { continue };
            if leaves_something(n) {
                something = true;
                break;
            }
            blind.extend(unseen(ix, n, overridden.contains_key(id)));
            let built = ix.targets(id, EdgeKind::Instantiates).into_iter().filter_map(|c| ix.node(c)?.initializer(|i| ix.node(i)));
            let next: Vec<String> = ix
                .targets(id, EdgeKind::Calls)
                .into_iter()
                .map(str::to_string)
                .chain(built)
                .chain(overridden.get(id).into_iter().flatten().map(|o| o.to_string()))
                .collect();
            for t in next {
                if let Some(t) = ix.node(&t).map(|n| n.id.as_str()).filter(|t| seen.insert(t)) {
                    runs.push(t);
                    todo.push(t);
                }
            }
        }
        if something {
            continue;
        }
        let name = ix.label(&f.id);
        let mut by: Vec<String> = callers.iter().map(|c| format!("`{}`", ix.label(c))).collect();
        by.sort();
        by.dedup();
        let mut evidence = vec![signature(f), format!("called by {}", list(&by))];
        if !runs.is_empty() {
            evidence.push(format!(
                "runs {}, none of which touches a place or changes a value",
                list(&runs.iter().map(|r| format!("`{}`", ix.label(r))).collect::<Vec<_>>())
            ));
        }
        if blind.is_empty() {
            out.push(ix.finding(
                "nothing",
                f,
                format!("{name} gives nothing back, and Atlas sees no change and no place it reaches"),
                evidence,
                format!(
                    "Atlas sees all that {name} runs, and none of it leaves anything behind. Is it dead, or does it change a value it is handed through a field, an item or a loop, which no call shows? Could it hand back what it makes, or let that value change itself through a method of its own?"
                ),
            ));
            continue;
        }
        let what: Vec<String> = blind.iter().map(|b| format!("`{}`", b.what)).collect();
        let shown = match what.len() {
            n if n > 3 => format!("{} and {} more", what[..3].join(", "), n - 3),
            _ => list(&what),
        };
        let does = if what.len() == 1 { "does" } else { "do" };
        evidence.extend(blind.iter().map(|b| format!("`{}` ({}): {}", b.what, b.at, b.why)));
        out.push(ix.finding(
            "nothing",
            f,
            format!("{name} gives nothing back, and Atlas cannot see what {shown} {does}"),
            evidence,
            format!(
                "Atlas sees no change and no place in what {name} runs, but it cannot see what {shown} {does}. Does that leave something behind which the picture lacks (a log, a change in a library's or a class's value, a wait), or is {name} dead?"
            ),
        ));
    }
    out
}

/// Something a function runs that Atlas does not see into: as the code writes it, where, and why
/// the graph holds nothing of what it does.
struct Blind {
    what: String,
    at: String,
    why: &'static str,
}

/// What Atlas does not see in `n`: what its body does that the graph holds nothing else of (a call
/// made for an effect no table states, a value a class keeps), and `n` itself where it is a method
/// only declared that nothing in the graph implements.
fn unseen(ix: &Index, n: &Node, implemented: bool) -> Vec<Blind> {
    let at = |line: u32| format!("{}:{line}", ix.file(n));
    let mut out: Vec<Blind> = n
        .unseen
        .iter()
        .map(|(line, what)| Blind { what: what.clone(), at: at(*line), why: "the graph holds nothing of what it does" })
        .collect();
    if n.declared && !implemented {
        out.push(Blind { what: ix.label(&n.id), at: ix.at(n), why: "only declared, and nothing in the graph implements it" });
    }
    out
}

// ---------- prophecy ----------

/// Built code that leans on a prophecy: production would run on a stub. A prophecy that
/// supersedes something the code does not hold is a false prophecy. Prophecies lean on one another
/// freely, and a test may lean on the code it is written for before it is built.
fn prophecy(ix: &Index) -> Vec<Finding> {
    let mut out = vec![];
    for p in ix.g.nodes.iter().filter(|n| n.prophecy.as_ref().is_some_and(|p| p.supersedes.is_some())) {
        if ix.superseded(p).is_none() {
            let named = p.prophecy.as_ref().and_then(|p| p.supersedes.as_deref()).unwrap_or("");
            out.push(ix.finding(
                "prophecy",
                p,
                format!("{} supersedes `{named}`, which the code does not hold", ix.label(&p.id)),
                vec![format!("supersedes `{named}`")],
                format!("A prophecy that replaces nothing is built or false. Is `{named}` gone already, or is its name written otherwise?"),
            ));
        }
    }
    let leans = [EdgeKind::Calls, EdgeKind::Instantiates, EdgeKind::Inherits, EdgeKind::Takes, EdgeKind::Returns, EdgeKind::Has];
    let mut by: BTreeMap<&str, Vec<(&Node, EdgeKind)>> = BTreeMap::new();
    for e in ix.g.edges.iter().filter(|e| leans.contains(&e.kind)) {
        let (Some(from), Some(to)) = (ix.node(&e.src), ix.node(&e.dst)) else { continue };
        let test = from.entry.as_ref().is_some_and(|en| en.kind == EntryKind::Test);
        if ix.prophetic(to) && !ix.prophetic(from) && !from.library && !test {
            // a member of a prophecy is leaned on as the prophecy it belongs to
            let root = to.cls.as_deref().and_then(|c| ix.node(c)).filter(|c| c.prophecy.is_some()).unwrap_or(to);
            by.entry(from.id.as_str()).or_default().push((root, e.kind));
        }
    }
    for (id, leaned) in by {
        let Some(from) = ix.node(id) else { continue };
        let mut names: Vec<String> = leaned.iter().map(|(to, _)| format!("`{}`", ix.label(&to.id))).collect();
        names.sort();
        names.dedup();
        let mut evidence: Vec<String> = leaned.iter().map(|(to, k)| format!("{k:?} `{}`, which is not built", ix.label(&to.id))).collect();
        evidence.sort();
        evidence.dedup();
        out.push(ix.finding(
            "prophecy",
            from,
            format!("{} leans on {}, which is not built", ix.label(id), list(&names)),
            evidence,
            format!(
                "Production would run on a stub here. Build {} first, or is {} meant to change with it, as a prophecy that supersedes it?",
                list(&names),
                ix.label(id)
            ),
        ));
    }
    out
}

// ---------- layers ----------

/// Packages that depend on one another in a cycle. The code quotiented by package is a preorder,
/// and only where it is a partial order do its packages have layers. Read at every level of the
/// package forest, among the packages under one parent, from what their code calls, builds,
/// imports and extends; a test is a way in from outside, not a layer.
fn layers(ix: &Index) -> Vec<Finding> {
    let module = |id: &str| -> Option<&str> {
        let n = ix
            .node(id)
            .filter(|n| !n.library && n.kind != NodeKind::External && n.entry.as_ref().map_or(true, |e| e.kind != EntryKind::Test))?;
        Some(if n.kind == NodeKind::Module { n.id.as_str() } else { n.module.as_str() })
    };
    let modules: Vec<&str> = ix.g.nodes.iter().filter(|n| n.kind == NodeKind::Module && !n.library).map(|n| n.id.as_str()).collect();
    let parents: BTreeSet<String> = modules.iter().flat_map(|m| m.match_indices('.').map(move |(i, _)| m[..i].to_string())).collect();
    let mut out = vec![];
    for parent in parents {
        // the package under `parent` that holds a module
        let under = |m: &str| -> Option<String> {
            let rest = m.strip_prefix(parent.as_str())?.strip_prefix('.')?;
            Some(format!("{parent}.{}", rest.split('.').next()?))
        };
        // package -> package it depends on -> the edges that make it so
        let mut deps: BTreeMap<String, BTreeMap<String, Vec<&Edge>>> = BTreeMap::new();
        for e in
            ix.g.edges
                .iter()
                .filter(|e| matches!(e.kind, EdgeKind::Calls | EdgeKind::Instantiates | EdgeKind::Imports | EdgeKind::Inherits))
        {
            let (Some(a), Some(b)) = (module(&e.src).and_then(under), module(&e.dst).and_then(under)) else { continue };
            if a != b {
                deps.entry(a).or_default().entry(b).or_default().push(e);
            }
        }
        for cycle in strongly_connected(&deps) {
            let name = |p: &str| p.rsplit('.').next().unwrap_or(p).to_string();
            // the links inside the cycle, the thinnest first: the likeliest to run against the flow
            let mut links: Vec<(&str, &str, &Vec<&Edge>)> = vec![];
            for a in &cycle {
                for (b, es) in deps.get(a.as_str()).into_iter().flatten().filter(|(b, _)| cycle.contains(b)) {
                    links.push((a.as_str(), b.as_str(), es));
                }
            }
            links.sort_by_key(|(a, b, es)| (es.len(), *a, *b));
            // a call says more than an import
            let example = |es: &Vec<&Edge>| {
                let e = es.iter().find(|e| e.kind != EdgeKind::Imports).unwrap_or(&es[0]);
                format!("`{}` → `{}`", ix.label(&e.src), ix.label(&e.dst))
            };
            let evidence: Vec<String> =
                links.iter().map(|(a, b, es)| format!("{} → {}: {} ({} in all)", name(a), name(b), example(es), es.len())).collect();
            let Some(at) = ix.node(&parent).filter(|n| !n.file.is_empty()).or_else(|| ix.node(&cycle[0])) else { continue };
            let members: Vec<String> = cycle.iter().map(|c| name(c)).collect();
            let ask = match links.as_slice() {
                [(a, b, es), rest @ ..] if rest.iter().all(|(_, _, more)| more.len() > es.len()) => {
                    format!(
                        "The thinnest link is {} → {} ({}). Does what it uses belong in {}, or in a package below both?",
                        name(a),
                        name(b),
                        example(es),
                        name(a)
                    )
                }
                _ => "Which of these links runs against the flow, and where does what it uses belong?".to_string(),
            };
            out.push(ix.finding(
                "layers",
                at,
                format!("{} depend on one another", list(&members)),
                evidence,
                format!("Packages in a cycle have no order: none can be read, tested or replaced without the others. {ask}"),
            ));
        }
    }
    out
}

/// The strongly connected components of more than one node (Tarjan).
fn strongly_connected<V>(deps: &BTreeMap<String, BTreeMap<String, V>>) -> Vec<Vec<String>> {
    struct Walk<'d, V> {
        deps: &'d BTreeMap<String, BTreeMap<String, V>>,
        index: HashMap<&'d str, usize>,
        low: HashMap<&'d str, usize>,
        stack: Vec<&'d str>,
        on: HashSet<&'d str>,
        out: Vec<Vec<String>>,
    }
    impl<'d, V> Walk<'d, V> {
        fn visit(&mut self, v: &'d str) {
            let i = self.index.len();
            self.index.insert(v, i);
            self.low.insert(v, i);
            self.stack.push(v);
            self.on.insert(v);
            for w in self.deps.get(v).into_iter().flat_map(|m| m.keys()) {
                if !self.index.contains_key(w.as_str()) {
                    self.visit(w);
                    let lw = self.low[w.as_str()];
                    self.low.insert(v, self.low[v].min(lw));
                } else if self.on.contains(w.as_str()) {
                    let iw = self.index[w.as_str()];
                    self.low.insert(v, self.low[v].min(iw));
                }
            }
            if self.low[v] == self.index[v] {
                let mut comp = vec![];
                while let Some(w) = self.stack.pop() {
                    self.on.remove(w);
                    comp.push(w.to_string());
                    if w == v {
                        break;
                    }
                }
                if comp.len() > 1 {
                    comp.sort();
                    self.out.push(comp);
                }
            }
        }
    }
    let mut walk = Walk { deps, index: HashMap::new(), low: HashMap::new(), stack: vec![], on: HashSet::new(), out: vec![] };
    let all: BTreeSet<&str> = deps.iter().flat_map(|(a, bs)| std::iter::once(a.as_str()).chain(bs.keys().map(String::as_str))).collect();
    for v in all {
        if !walk.index.contains_key(v) {
            walk.visit(v);
        }
    }
    walk.out
}

// ---------- lens ----------

/// A class whose operations both write and read one place where data rests (a file, a database)
/// is a lens on it: its puts take a value and change the place (`A × S → S`), its gets give a
/// value from it (`S → A`). Put then get gives back what was put. Asked about each type of data a
/// put takes that no get of the same class gives from that place, as it is or inside what it
/// gives (`holds`): what the place holds is then another type than what goes in, or another class
/// reads it. A class that only reads a place (input from the world) or only writes it (an end
/// product) is no lens on it. A put is asked about in the class that defines it: a subclass's
/// gets all count, but the puts it inherits are its base's question. What a put takes is what
/// reaches the place (`Places::reached`).
fn lens<'g>(ix: &Index<'g>) -> Vec<Finding> {
    let data = |t: &str, own: &str| {
        t != own && ix.node(t).map_or(false, |n| matches!(n.kind, NodeKind::Class | NodeKind::External) && n.door.is_none())
    };
    // a place the code names, not only the variable that holds it: what two classes do there is
    // done to the same place
    let named = |place: &str| !place.starts_with("w:Disk:");
    let places = Places { ix, reached: RefCell::default(), into: RefCell::default() };
    let operations = |c: &Node| -> Vec<(&'g Node, BTreeMap<String, String>, BTreeMap<String, String>)> {
        ix.interface(&c.id)
            .into_iter()
            .filter(|m| is_function(m) && !m.ctor && !m.private)
            .map(|m| {
                let (puts, gets) = places.reaches(&m.id);
                (m, puts, gets)
            })
            .collect()
    };
    // the types of data operation `m` of class `c` puts into `place`, among `ops`, the operations
    // of `c`: those of the parameters that reach it, and a type no parameter is handed on as when
    // a parameter the graph does not follow does
    let put_types = |c: &Node, m: &'g Node, place: &str, ops: &HashSet<&str>| -> Vec<&'g str> {
        let reached = places.reached(m, place, ops);
        let hands = || m.sites.iter().flat_map(|s| &s.hands);
        let unseen = reached.iter().any(|p| !hands().any(|h| h.name == *p));
        let shown: BTreeSet<&str> =
            m.params.iter().flat_map(|p| hands().filter(move |h| h.name == p.name)).map(|h| h.ty.as_str()).collect();
        let put: BTreeSet<&str> = hands().filter(|h| reached.contains(h.name.as_str())).map(|h| h.ty.as_str()).collect();
        ix.targets(&m.id, EdgeKind::Takes)
            .into_iter()
            .filter(|t| data(t, &c.id))
            .filter(|t| put.contains(t) || (unseen && !shown.contains(t)))
            .collect()
    };
    let classes: Vec<&'g Node> = ix.g.nodes.iter().filter(|n| n.kind == NodeKind::Class && !n.library).collect();
    // what each class's operations get from each place, as it is or inside what they give:
    // (place, type) -> `Class.method`
    let mut got_from: BTreeMap<(String, &str), Vec<String>> = BTreeMap::new();
    for c in &classes {
        for (m, _, gets) in operations(c) {
            for place in gets.keys() {
                let given: BTreeSet<&str> =
                    ix.targets(&m.id, EdgeKind::Returns).into_iter().filter(|t| data(t, &c.id)).flat_map(|t| holds(ix, t)).collect();
                for t in given {
                    got_from.entry((place.clone(), t)).or_default().push(ix.label(&m.id));
                }
            }
        }
    }
    let mut out = vec![];
    for c in classes {
        let ops = operations(c);
        let ids: HashSet<&str> = ops.iter().map(|(m, _, _)| m.id.as_str()).collect();
        let name = ix.label(&c.id);
        // the places this class both puts to and gets from
        let places: BTreeMap<&String, &String> =
            ops.iter().flat_map(|(_, puts, _)| puts.iter()).filter(|(p, _)| ops.iter().any(|(_, _, gets)| gets.contains_key(*p))).collect();
        let (mut lost_all, mut labels, mut evidence): (Vec<String>, Vec<String>, Vec<String>) = (vec![], vec![], vec![]);
        for (place, label) in places {
            let (mut put, mut got, mut given): (BTreeMap<&str, Vec<String>>, Vec<String>, HashSet<&str>) =
                (BTreeMap::new(), vec![], HashSet::new());
            for (m, puts, gets) in &ops {
                if puts.contains_key(place) && ix.declares(&c.id, m) {
                    for t in put_types(c, m, place, &ids) {
                        put.entry(t).or_default().push(format!("{}: {} × {label} → {label}", ix.label(&m.id), ix.label(t)));
                    }
                }
                if gets.contains_key(place) && m.returns.as_ref().map_or(false, |r| !r.value.is_empty() || r.own) {
                    let types: Vec<&str> = ix.targets(&m.id, EdgeKind::Returns).into_iter().filter(|t| data(t, &c.id)).collect();
                    given.extend(types.iter().flat_map(|t| holds(ix, t)));
                    let gives = if types.is_empty() {
                        written(m).to_string()
                    } else {
                        types.iter().map(|t| ix.label(t)).collect::<Vec<_>>().join(", ")
                    };
                    got.push(format!("{}: {label} → {gives}", ix.label(&m.id)));
                }
            }
            let lost: Vec<&str> = put.keys().copied().filter(|t| !given.contains(t)).collect();
            if lost.is_empty() {
                continue;
            }
            for t in &lost {
                if !lost_all.contains(&ix.label(t)) {
                    lost_all.push(ix.label(t));
                }
            }
            labels.push(label.clone());
            evidence.extend(put.values().flatten().map(|p| format!("put {p}")));
            evidence.extend(got.iter().map(|g| format!("get {g}")));
            for t in lost.iter().filter(|_| named(place)) {
                let elsewhere: Vec<String> = got_from
                    .get(&(place.clone(), *t))
                    .into_iter()
                    .flatten()
                    .filter(|m| !m.starts_with(&format!("{}.", c.name)))
                    .map(|m| format!("`{m}`"))
                    .collect();
                if !elsewhere.is_empty() {
                    evidence.push(format!("{} is got from {label} by {}", ix.label(t), elsewhere.join(", ")));
                }
            }
        }
        if lost_all.is_empty() {
            continue;
        }
        let (types, at) = (list(&lost_all), list(&labels));
        out.push(ix.finding(
            "lens",
            c,
            format!("{name} puts {types} into {at} and gets no {types} back"),
            evidence,
            format!("A lens gives back what it is given: put then get. {name} puts {types}, and none of its gets gives that back. What does {at} hold, and could {name} get it back as it went in?"),
        ));
    }
    out
}

/// Whether a type as written names `name`: `list[Reading]`, `&[Reading]` and `Reading[]` name
/// `Reading`.
fn names(written: &str, name: &str) -> bool {
    written.split(|c: char| !(c.is_alphanumeric() || c == '_')).any(|w| w == name)
}

/// What a value of type `t` holds, `t` among it: its fields and theirs in turn, the elements of
/// its lists, the variants of its sums. A get that gives `t` gives back each of these: a lens
/// composed with the projection to a part of what it gives is a lens on that part.
fn holds<'g>(ix: &Index<'g>, t: &'g str) -> HashSet<&'g str> {
    let mut seen = HashSet::from([t]);
    let mut todo = vec![t];
    while let Some(x) = todo.pop() {
        for y in ix.targets(x, EdgeKind::Has).into_iter().chain(ix.targets(x, EdgeKind::Variant)) {
            if seen.insert(y) {
                todo.push(y);
            }
        }
    }
    seen
}

/// The place a touch holds data at, as a lens sees it: a database as a whole, a file by itself.
fn held(t: &Touch) -> Option<(String, String)> {
    match t.kind {
        World::Db(_) => Some((t.kind.name().to_string(), t.kind.name().to_string())),
        World::Disk if !t.what.is_empty() || !t.artifact.is_empty() => {
            let p = t.place();
            Some((p.id, p.label))
        }
        _ => None,
    }
}

/// The places a function puts to and gets from, through what it calls: id -> label.
type Reached = (BTreeMap<String, String>, BTreeMap<String, String>);

/// What the functions of a graph do at the places where data rests, each worked out once.
struct Places<'a, 'g> {
    ix: &'a Index<'g>,
    reached: RefCell<HashMap<String, Reached>>,
    into: RefCell<HashMap<(String, String), bool>>,
}

impl<'g> Places<'_, 'g> {
    /// The places a function puts to and gets from, through what it calls. A call that does not
    /// say which way the data goes is neither: a lens is not guessed.
    fn reaches(&self, from: &str) -> Reached {
        if let Some(r) = self.reached.borrow().get(from) {
            return r.clone();
        }
        let (mut puts, mut gets) = (BTreeMap::new(), BTreeMap::new());
        let mut seen: HashSet<&str> = HashSet::from([from]);
        let mut todo = vec![from];
        while let Some(id) = todo.pop() {
            let Some(n) = self.ix.node(id).filter(|n| !n.library) else { continue };
            for t in &n.touches {
                if let Some((place, label)) = held(t) {
                    match t.dir {
                        Some(Dir::Write) => puts.insert(place, label),
                        Some(Dir::Read) => gets.insert(place, label),
                        None => None,
                    };
                }
            }
            todo.extend(self.ix.targets(id, EdgeKind::Calls).into_iter().filter(|t| seen.insert(t)));
        }
        self.reached.borrow_mut().insert(from.to_string(), (puts.clone(), gets.clone()));
        (puts, gets)
    }

    /// Whether function `f` puts what it is handed into `place`: one of its parameters reaches it.
    fn into(&self, f: &str, place: &str) -> bool {
        let key = (f.to_string(), place.to_string());
        if let Some(b) = self.into.borrow().get(&key) {
            return *b;
        }
        // a function that calls itself puts nothing more on the way round
        self.into.borrow_mut().insert(key.clone(), false);
        let b =
            self.ix.node(f).filter(|n| !n.library && is_function(n)).map_or(false, |n| !self.reached(n, place, &HashSet::new()).is_empty());
        self.into.borrow_mut().insert(key, b);
        b
    }

    /// The parameters of `f` that reach `place`, by name: those whose value, or what the body's
    /// steps make of it, is handed to a step that puts what it is handed there. A step that reads
    /// and puts nothing it is handed there gives what the place holds, so what it is handed is a
    /// key (a `WHERE`, a filter, a file's name) and goes no further. What `f` hands an operation
    /// among `theirs` that takes that type is that operation's to put, and asked about there.
    ///
    /// Where the graph does not show where a value goes, it may go anywhere: a call of `f`'s own
    /// into the place may be handed any of its parameters, since the graph does not say what a
    /// call into the world is handed; a call Atlas cannot follow may put what it is handed
    /// wherever `f` puts; a value no step gives and no parameter is (`for r in readings`,
    /// `x = p.field`) is taken from any parameter whose type holds its type, as the parameter is
    /// declared or handed; and a parameter neither handed to a step by name nor holding such a
    /// value reaches the place when any step puts there.
    fn reached(&self, f: &'g Node, place: &str, theirs: &HashSet<&str>) -> BTreeSet<&'g str> {
        let ix = self.ix;
        let params: Vec<&'g str> = f.params.iter().filter(|p| !p.receiver).map(|p| p.name.as_str()).collect();
        // a call of its own into the place may be handed any of them
        if f.touches.iter().any(|t| t.dir == Some(Dir::Write) && held(t).map_or(false, |(p, _)| p == place)) {
            return params.into_iter().collect();
        }
        let hands = || f.sites.iter().flat_map(|s| &s.hands);
        let given: HashSet<&str> = f.sites.iter().flat_map(|s| &s.gives).map(|g| g.name.as_str()).collect();
        // what each parameter holds: the types it is declared to take, and those it is handed as
        let takes = ix.targets(&f.id, EdgeKind::Takes);
        let holding: Vec<(&'g str, HashSet<&'g str>)> = f
            .params
            .iter()
            .filter(|p| !p.receiver)
            .map(|p| {
                let declared = takes.iter().copied().filter(|t| ix.node(t).map_or(false, |n| names(&p.ty, &n.name)));
                let handed = hands().filter(|h| h.name == p.name && !h.ty.is_empty()).map(|h| h.ty.as_str());
                (p.name.as_str(), declared.chain(handed).flat_map(|t| holds(ix, t)).collect())
            })
            .collect();
        // a value no step gives and no parameter is, by the parameters that hold its type
        let held_in =
            |h: &Held| -> BTreeSet<&'g str> { holding.iter().filter(|(_, ts)| ts.contains(h.ty.as_str())).map(|(p, _)| *p).collect() };
        let loose = |h: &&Held| !params.contains(&h.name.as_str()) && !given.contains(h.name.as_str());
        let taken_from: HashSet<&'g str> = hands().filter(loose).flat_map(held_in).collect();
        let unseen: Vec<&'g str> = params.iter().copied().filter(|p| !hands().any(|h| h.name == *p) && !taken_from.contains(p)).collect();
        // the parameters each value of the body is made from, by the name the body holds it by
        let mut made: HashMap<&'g str, BTreeSet<&'g str>> = params.iter().map(|p| (*p, BTreeSet::from([*p]))).collect();
        let mut out = BTreeSet::new();
        for s in &f.sites {
            let to = s.to.as_str();
            // a call Atlas cannot follow may put what it is handed wherever `f` puts
            let puts = if s.not_followed { self.reaches(&f.id).0.contains_key(place) } else { !to.is_empty() && self.into(to, place) };
            // and a parameter no step is handed by name may be handed to this one
            if puts {
                out.extend(&unseen);
            }
            let inputs: Vec<(&Held, BTreeSet<&'g str>)> = s
                .hands
                .iter()
                .map(|h| {
                    let from = match made.get(h.name.as_str()) {
                        Some(ps) => ps.clone(),
                        None if given.contains(h.name.as_str()) => BTreeSet::new(),
                        None => held_in(h),
                    };
                    (h, from)
                })
                .filter(|(_, from)| !from.is_empty())
                .collect();
            if inputs.is_empty() {
                continue;
            }
            if puts {
                // what an operation among `theirs` takes, it puts as its own
                let own: Vec<&str> = if theirs.contains(to) { ix.targets(to, EdgeKind::Takes) } else { vec![] };
                out.extend(inputs.iter().filter(|(h, _)| !own.contains(&h.ty.as_str())).flat_map(|(_, from)| from.iter().copied()));
            } else if !to.is_empty() && !self.reaches(to).1.is_empty() {
                // it reads, and gives what the place holds: what it was handed is a key
                continue;
            }
            let from: BTreeSet<&'g str> = inputs.iter().flat_map(|(_, from)| from.iter().copied()).collect();
            for g in s.gives.iter().filter(|g| !g.name.is_empty()) {
                made.entry(g.name.as_str()).or_default().extend(&from);
            }
        }
        out
    }
}
