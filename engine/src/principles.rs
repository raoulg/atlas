//! `check: Graph -> [Finding]`: the design principles Atlas stands on, computed from the graph.
//!
//! A finding names its principle, the symbol it is about, the evidence the graph holds and a
//! question for the author. It coaches; it does not fix. Atlas sees types, routes and places,
//! so these are the principles that live there; what a single line should look like belongs
//! to ruff, pyright and clippy. `docs/reference/principles.md` explains each one.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;

use serde::Serialize;

use crate::model::{Compare, Dir, Edge, EdgeKind, EntryKind, Graph, Held, Leaves, Node, NodeKind, Param, Part, Site, Touch, World};

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
    /// Each class's method resolution order, worked out once.
    mro: RefCell<HashMap<&'g str, Vec<&'g str>>>,
}

impl<'g> Index<'g> {
    fn new(g: &'g Graph) -> Self {
        let mut out: HashMap<&str, Vec<&Edge>> = HashMap::new();
        let mut into: HashMap<&str, Vec<&Edge>> = HashMap::new();
        for e in &g.edges {
            out.entry(e.src.as_str()).or_default().push(e);
            into.entry(e.dst.as_str()).or_default().push(e);
        }
        Index { g, nodes: g.nodes.iter().map(|n| (n.id.as_str(), n)).collect(), out, into, mro: RefCell::default() }
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

    /// Whether a node is test code, by where it stands: a test, code the reader marks as test code,
    /// or code in a test module. A test is a way in from outside the program.
    fn test(&self, n: &Node) -> bool {
        n.test || n.entry.as_ref().is_some_and(|e| e.kind == EntryKind::Test) || self.node(&n.module).is_some_and(|m| m.test)
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

    /// The part a node belongs to.
    fn part(&self, id: &str) -> Option<&'g Part> {
        let part = self.node(id).map_or("", |n| n.part.as_str());
        self.g.parts.iter().find(|p| p.id == part).or(self.g.parts.first())
    }

    /// What the language of a node's part leaves to its author.
    fn leaves(&self, id: &str) -> Leaves {
        self.part(id).map_or_else(Leaves::default, |p| p.leaves)
    }

    fn at(&self, n: &Node) -> String {
        format!("{}:{}", self.file(n), n.span.map_or(1, |s| s.0))
    }

    /// Whether the method `m` is defined in `cls` itself rather than inherited from a base.
    fn declares(&self, cls: &str, m: &Node) -> bool {
        m.id.rsplit_once('.').map_or(false, |(owner, _)| owner == cls)
    }

    /// A class's public methods (and its constructor), its own and those it inherits from
    /// the package's classes, each from the first class along its method resolution order that
    /// defines it.
    fn interface(&self, cls: &str) -> Vec<&'g Node> {
        let mut seen = HashSet::new();
        let mut out = vec![];
        for m in self.members(cls) {
            if (!m.private || m.ctor) && seen.insert(m.name.as_str()) {
                out.push(m);
            }
        }
        out
    }

    /// Every method a class has, its own and those of its ancestors, along its method resolution
    /// order: a name its own class or a nearer ancestor defines comes before the same name further
    /// on.
    fn members(&self, cls: &str) -> Vec<&'g Node> {
        self.mro(cls)
            .into_iter()
            .filter_map(|c| self.node(c))
            .flat_map(|n| n.methods.iter().filter_map(move |m| self.node(&format!("{}.{m}", n.id))))
            .collect()
    }

    /// The method resolution order along `inherits` (`model::linearization`): the class, then
    /// its ancestors nearest first.
    fn mro(&self, cls: &str) -> Vec<&'g str> {
        let Some(cls) = self.nodes.get_key_value(cls).map(|(k, _)| *k).or_else(|| self.out.get_key_value(cls).map(|(k, _)| *k)) else {
            return vec![];
        };
        let bases = |c: &'g str| self.targets(c, EdgeKind::Inherits);
        crate::model::linearization(cls, &bases, &mut self.mro.borrow_mut())
    }

    /// Ancestors along `inherits`, nearest first, in the method resolution order.
    fn ancestors(&self, cls: &str) -> Vec<&'g str> {
        self.mro(cls).into_iter().skip(1).collect()
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

/// What a node's code depends on another for: what it calls, builds and extends, a request to a
/// route the other serves, and the types it takes, returns and holds (a field, a variant of a sum).
const DEPENDS: [EdgeKind; 8] = [
    EdgeKind::Calls,
    EdgeKind::Instantiates,
    EdgeKind::Inherits,
    EdgeKind::Requests,
    EdgeKind::Takes,
    EdgeKind::Returns,
    EdgeKind::Has,
    EdgeKind::Variant,
];

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
        // a constructor gives its class
        let gives_unknown = ix.leaves(&f.id).returns && f.returns.is_none() && !f.ctor;
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

/// A sum type that a module returns and that two or more functions in other modules take as
/// input: each of them has to choose between the variants again. Unwrapped where it arises, the
/// choice is made once, and what travels on is the variant each consumer needs. One consumer is
/// no repeated choice: a coproduct is taken apart by a copairing, and that has to stand
/// somewhere. A library's sum arises where a call gives it, outside every module of the program,
/// so a public function that takes one has been handed it unopened, whatever module it is in;
/// the finding stands at the first that takes it, where the program has code to change.
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
        if (producers.is_empty() && !library) || consumers.len() < 2 {
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
        // a union written in place says its variants in its name
        let sum = if s.name.contains('|') { s.name.clone() } else { format!("{} ({})", s.name, variants.join(" | ")) };
        out.push(ix.finding(
            "sums",
            if library { consumers[0] } else { s },
            format!("{sum} is handed on unopened: {} take it", list(&taken_by)),
            evidence,
            choose,
        ));
    }
    out
}

// ---------- overrides ----------

/// An override that does not keep the promise of its base: a caller written against the base
/// cannot take it in its place (Liskov). A subclass of the promised type is fine. An initializer
/// fills in the value its class makes and answers to no base; a constructor called on the class
/// (a classmethod that gives `Self`) is handed what its callers hand the base's, and is asked.
/// Where the language holds every override to its base (Rust's trait implementations), there is
/// nothing to ask.
fn overrides(ix: &Index) -> Vec<Finding> {
    let mut broken: Vec<(&Node, Vec<&Node>)> = vec![];
    let initializer = |n: &Node| n.ctor && !n.on_class;
    for m in ix.g.nodes.iter().filter(|n| n.kind == NodeKind::Method && !initializer(n) && ix.leaves(&n.id).overrides) {
        let Some(base) = breaks(ix, m) else { continue };
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

/// The method whose promise the override `m` breaks. A caller written against any of its class's
/// ancestors may be handed it, so it keeps the promise of each method it overrides, and the one it
/// breaks is the first along its class's method resolution order whose promise it does not keep.
/// Where the language's type checker decided it (ty, for Python), by its answer: what the override
/// takes and what it gives, against each base as the subclass sees it, its type parameters filled
/// in; the checker names the base, and the nearest stands for one it names outside the graph.
/// Elsewhere by `keeps`; a promise the nearest base already breaks is the nearest's question, and
/// is not asked again of what overrides it (`C(B)` where `B.f` breaks `A.f`). Either way an array's
/// shape is a promise no checker reads.
fn breaks<'g>(ix: &Index<'g>, m: &Node) -> Option<&'g Node> {
    let cls = m.cls.as_deref()?;
    let bases: Vec<&'g Node> = ix.ancestors(cls).into_iter().filter_map(|a| ix.node(&format!("{a}.{}", m.name))).collect();
    let nearest = *bases.first()?;
    match m.keeps {
        Some(false) => Some(m.breaks.as_deref().and_then(|b| bases.iter().copied().find(|x| x.id == b)).unwrap_or(nearest)),
        Some(true) => bases.into_iter().find(|b| !shapes_fit(ix, b, m)),
        None => {
            let below = |b: &Node| nearest.cls.as_deref().is_some_and(|n| b.cls.as_deref().is_some_and(|c| ix.ancestors(n).contains(&c)));
            let inherited = |b: &Node| b.id != nearest.id && below(b) && !keeps(ix, b, nearest);
            bases.into_iter().find(|b| !keeps(ix, b, m) && !inherited(b))
        }
    }
}

/// Whether the override `m` keeps the promise of `base`, by the types the two signatures resolve
/// to (`signature_keeps`) and the shapes of the arrays they give.
fn keeps(ix: &Index, base: &Node, m: &Node) -> bool {
    let structural = ix.part(&m.id).is_some_and(|p| p.structural);
    signature_keeps(ix, base, m, structural, &mut vec![]) && shapes_fit(ix, base, m)
}

/// Whether `m` can stand where `base` stood, by the program's types its signature resolves to:
/// each type it gives fits one the base promises (what it gives may be narrower), and at each place
/// both take a parameter, each type the base takes there fits one the override takes (what it
/// takes may be wider, never narrower). A promise or a parameter whose type resolves to none of the
/// program's types, or a value of the class it runs on, gives these nothing to compare. A parameter
/// is matched by its place. `assumed` holds the pairs of types being compared further out, which
/// fit unless shown not to.
fn signature_keeps(ix: &Index, base: &Node, m: &Node, structural: bool, assumed: &mut Vec<(String, String)>) -> bool {
    let gives = match (&base.returns, &m.returns) {
        (Some(p), Some(g)) if !p.own && !g.own => {
            let promised = ix.targets(&base.id, EdgeKind::Returns);
            promised.is_empty()
                || ix.targets(&m.id, EdgeKind::Returns).iter().all(|g| promised.iter().any(|p| fits(ix, g, p, structural, assumed)))
        }
        _ => true,
    };
    let inputs = |f: &Node| f.params.iter().filter(|p| !p.receiver && !p.rest).cloned().collect::<Vec<_>>();
    let takes = |f: &Node, p: &Param| -> Vec<&str> {
        ix.targets(&f.id, EdgeKind::Takes).into_iter().filter(|t| ix.node(t).is_some_and(|n| names(&p.ty, &n.name))).collect()
    };
    gives
        && inputs(base).iter().zip(&inputs(m)).all(|(theirs, own)| {
            let own = takes(m, own);
            own.is_empty() || takes(base, theirs).iter().all(|b| own.iter().any(|o| fits(ix, b, o, structural, assumed)))
        })
}

/// Whether a value of type `x` stands where type `y` is asked for: it is `y`, a subclass of it, an
/// array of a shape `y` allows, a variant of a sum `y` names, or, in a language that compares types
/// by their members, a type that has every member `y` declares (`has_members`).
fn fits(ix: &Index, x: &str, y: &str, structural: bool, assumed: &mut Vec<(String, String)>) -> bool {
    if x == y || ix.ancestors(x).contains(&y) || array_id_fits(x, y) || assumed.iter().any(|(a, b)| a == x && b == y) {
        return true;
    }
    assumed.push((x.to_string(), y.to_string()));
    let fit = ix.targets(y, EdgeKind::Variant).iter().any(|v| fits(ix, x, v, structural, assumed))
        || (structural && has_members(ix, x, y, structural, assumed));
    assumed.pop();
    fit
}

/// Whether type `x` has every member type `y` declares, its own and its ancestors': each method by
/// its name, with a signature that keeps the promise of `y`'s, and each field by its name, holding
/// a type that fits the one `y`'s holds where the graph has both. A type with a private method is
/// had only by itself and its subclasses, and a library's type by what it is declared as.
fn has_members(ix: &Index, x: &str, y: &str, structural: bool, assumed: &mut Vec<(String, String)>) -> bool {
    let program = |t: &str| ix.node(t).filter(|n| n.kind == NodeKind::Class);
    let (Some(_), Some(_)) = (program(x), program(y)) else { return false };
    let methods = |t: &str| -> Vec<&Node> { ix.members(t).into_iter().filter(|m| !m.ctor && !m.on_class).collect() };
    let fields = |t: &str| -> Vec<&str> {
        ix.mro(t).into_iter().filter_map(|c| ix.node(c)).flat_map(|n| n.fields.iter().map(String::as_str)).collect()
    };
    let held = |t: &str, f: &str| -> Vec<&str> {
        ix.mro(t)
            .into_iter()
            .flat_map(|c| ix.out.get(c).into_iter().flatten())
            .filter(|e| e.kind == EdgeKind::Has && e.via.as_deref() == Some(f))
            .map(|e| e.dst.as_str())
            .collect()
    };
    let (theirs, own) = (methods(y), methods(x));
    if theirs.iter().any(|m| m.private) {
        return false;
    }
    let (their_fields, own_fields) = (fields(y), fields(x));
    let named = |name: &str| own.iter().any(|m| m.name == name) || own_fields.contains(&name);
    if !theirs.iter().all(|m| named(&m.name)) || !their_fields.iter().all(|f| named(f)) {
        return false;
    }
    let mut seen = HashSet::new();
    for m in theirs.iter().filter(|m| seen.insert(m.name.as_str())) {
        if let Some(o) = own.iter().find(|o| o.name == m.name) {
            if !signature_keeps(ix, m, o, structural, assumed) {
                return false;
            }
        }
    }
    their_fields.iter().all(|f| {
        let mine = held(x, f);
        mine.is_empty() || held(y, f).iter().all(|t| mine.iter().any(|o| fits(ix, o, t, structural, assumed)))
    })
}

/// Whether each array the override `m` gives fits one its base promises: an array's shape is a
/// promise no type checker reads (`Float[Tensor, "batch dim"]` where `Float[Tensor, "..."]` is
/// promised).
fn shapes_fit(ix: &Index, base: &Node, m: &Node) -> bool {
    let shapes: Vec<Array> = ix.targets(&base.id, EdgeKind::Returns).iter().filter_map(|p| array_of_id(p)).collect();
    shapes.is_empty()
        || ix.targets(&m.id, EdgeKind::Returns).iter().filter_map(|g| array_of_id(g)).all(|g| shapes.iter().any(|p| array_fits(&g, p)))
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

/// A function that changes an argument in place while its signature does not say so, whatever
/// the argument's type: a class of the program, a library's, a list. Each input it changes is
/// one change. Rust's `&mut T` says so; Python cannot, so there the change is better made by the
/// object itself (a method), or replaced by a new value handed back.
fn changes(ix: &Index) -> Vec<Finding> {
    let mut out = vec![];
    for e in ix.g.edges.iter().filter(|e| e.kind == EdgeKind::Mutates) {
        let Some(input) = e.via.as_deref() else { continue };
        let Some(f) = ix.node(&e.src).filter(|f| is_function(f)) else { continue };
        // an input handed in, not the value the function is called on, that the signature does not say changes
        let Some(p) = f.params.iter().find(|p| p.name == input && !p.receiver && !p.changes) else { continue };
        // the type as the signature writes it, a forward reference without its quotes: `list[Row]`, `Graph`
        let ty = match p.ty.trim_matches(['"', '\'']) {
            "" => ix.label(&e.dst),
            written => written.to_string(),
        };
        out.push(ix.finding(
            "changes",
            f,
            format!("{} changes the {ty} it is handed as `{input}`", ix.label(&f.id)),
            vec![signature(f)],
            format!(
                "Nothing in the signature shows that `{input}` changes. Should the {ty} change itself through a method of its own, or should {} hand back a new {ty}?",
                ix.label(&f.id)
            ),
        ));
    }
    out
}

// ---------- gives ----------

/// A function whose callers use what it gives back, with no return annotation: what they get
/// has no type, so neither a reader nor Atlas can follow it further, where the language leaves
/// types to the author; a door is asked about by `doors`.
fn gives(ix: &Index) -> Vec<Finding> {
    let doors: HashSet<&str> = door_fns(ix).into_iter().map(|f| f.id.as_str()).collect();
    let mut out = vec![];
    for f in
        ix.g.nodes
            .iter()
            .filter(|f| is_function(f) && f.returns.is_none() && !f.ctor && !doors.contains(f.id.as_str()) && ix.leaves(&f.id).types)
    {
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

/// Two ways a choice goes unnamed. The same case tested against the same value in several
/// places: what that case means is spelled out again in each of them, and a new case is a
/// change in all of them. And alternatives that each produce the same type by steps of their
/// own, sitting in a body between other steps: how to get that value is a step of its own,
/// without a name.
fn choices(ix: &Index) -> Vec<Finding> {
    let mut out = repeated(ix);
    for f in ix.g.nodes.iter().filter(|n| is_function(n)) {
        out.extend(unnamed(ix, f));
    }
    out
}

/// The value a comparison tests, as the functions that test it share it.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Tested<'g> {
    /// A field of a class, read through the value its methods run on (`self.mode`, `this.mode`):
    /// the class, and the path below the receiver.
    Field(&'g str, &'g str),
    /// A parameter of one name and written type in the functions of one module (`mode: str`), or
    /// what is read through it (`args.mode`): the module, the name, the type, the path below it.
    Param(&'g str, &'g str, &'g str, &'g str),
    /// Anything else a body is handed, the value its method runs on itself among them: the body,
    /// and the value as written.
    Own(&'g str, &'g str),
}

/// What a comparison of `f` tests, when it tests a value `f` was handed.
fn tested<'g>(f: &'g Node, c: &'g Compare) -> Option<Tested<'g>> {
    let root = crate::tree::root_of(&c.subject);
    let below = &c.subject[root.len()..];
    let own = Tested::Own(&f.id, &c.subject);
    if c.receiver {
        return Some(match f.cls.as_deref() {
            Some(cls) if !below.is_empty() && of_itself(below) => Tested::Field(cls, below),
            _ => own,
        });
    }
    let p = f.params.iter().find(|p| p.name == root)?;
    Some(if p.receiver || !of_itself(below) { own } else { Tested::Param(&f.module, &p.name, &p.ty, below) })
}

/// Whether a path below a value names nothing but that value: its fields, items at a literal,
/// calls handed nothing (`.mode`, `[0]`, `.kind()`). A path through another name (`[t]`,
/// `.get(k)`) reads a value of its own body.
fn of_itself(below: &str) -> bool {
    let mut rest = below;
    while let Some(i) = rest.find(['[', '(']) {
        let close = if rest.as_bytes()[i] == b'[' { ']' } else { ')' };
        let Some(j) = rest[i..].find(close) else { return false };
        let inside = rest[i + 1..i + j].trim();
        let literal = inside.is_empty() || inside.chars().all(|c| c.is_ascii_digit()) || inside.starts_with(['"', '\'']);
        if !literal {
            return false;
        }
        rest = &rest[i + j + 1..];
    }
    true
}

/// The cases tested against one value in more than one place: a parameter, or the value a
/// method runs on, that picks behaviour by name. A value is the same across bodies where the
/// graph says so: a field of a class in the methods that read it through their receiver, a
/// parameter of one name and written type in the functions of one module. The finding stands
/// where the value is defined: the class, or the first function that tests it.
fn repeated(ix: &Index) -> Vec<Finding> {
    let mut by_value: BTreeMap<Tested, Vec<(&Node, &Compare)>> = BTreeMap::new();
    for f in ix.g.nodes.iter().filter(|n| is_function(n)) {
        for c in &f.compares {
            if let Some(t) = tested(f, c) {
                by_value.entry(t).or_default().push((f, c));
            }
        }
    }
    let mut out = vec![];
    for (value, mut tests) in by_value {
        tests.sort_by(|(f, c), (g, d)| (&f.file, f.span.map_or(0, |s| s.0), c.line).cmp(&(&g.file, g.span.map_or(0, |s| s.0), d.line)));
        // a place is a choice, or a comparison that decides none on its own line, in one body
        let place = |(f, c): &(&Node, &Compare)| (f.id.clone(), c.choice.map_or((None, c.line), |ch| (Some(ch), 0)));
        let mut places: BTreeMap<String, BTreeSet<_>> = BTreeMap::new();
        for t in &tests {
            for case in t.1.named() {
                places.entry(case).or_default().insert(place(t));
            }
        }
        let spread: Vec<String> = places.iter().filter(|(_, ps)| ps.len() > 1).map(|(c, ps)| format!("{c} ({}×)", ps.len())).collect();
        if spread.is_empty() {
            continue;
        }
        let n_places = tests.iter().map(place).collect::<BTreeSet<_>>().len();
        let mut fns: Vec<&Node> = vec![];
        for (f, _) in &tests {
            if !fns.iter().any(|g| g.id == f.id) {
                fns.push(f);
            }
        }
        let across = fns.len() > 1;
        let evidence = tests
            .iter()
            .map(|(f, c)| {
                let at = if across { format!("line {} in {}", c.line, ix.label(&f.id)) } else { format!("line {}", c.line) };
                let tested = if c.written.is_empty() { c.subject.clone() } else { format!("{} ({})", c.written, c.subject) };
                format!("{at}: {tested} against {}", c.named().join(", "))
            })
            .collect();
        let subject = &tests[0].1.subject;
        let who = match across {
            true => format!("{} test", list(&fns.iter().map(|f| ix.label(&f.id)).collect::<Vec<_>>())),
            false => format!("{} tests", ix.label(&fns[0].id)),
        };
        let stands = match value {
            Tested::Field(cls, _) if across => ix.node(cls).unwrap_or(fns[0]),
            _ => fns[0],
        };
        out.push(ix.finding(
            "choices",
            stands,
            format!("{who} `{subject}` in {n_places} places; {} come back", list(&spread)),
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
/// steps of that arm. Arms that each make the value by the same step are that one step with a
/// choice of what it is handed (`h ∘ [f, g] = [h ∘ f, h ∘ g]`): they choose an argument, not where
/// the value comes from.
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
            let one_step = |arms: &BTreeMap<u32, BTreeSet<&str>>| arms.values().all(|s| arms.values().next() == Some(s));
            for (t, arms) in arms.into_iter().filter(|(_, arms)| arms.len() > 1 && !one_step(arms)) {
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
/// a call may reach) touches a place, changes a value or fails. An arrow into the terminal object
/// carries no information: the function is dead, or it writes a place the graph does not hold.
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
    for f in ix.g.nodes.iter().filter(|f| is_function(f) && !f.library && !f.ctor && f.entry.is_none()) {
        let gives_nothing = match &f.returns {
            Some(r) => r.value.is_empty() && !r.own,
            None => !ix.leaves(&f.id).returns,
        };
        let callers: Vec<&str> =
            ix.sources(&f.id, EdgeKind::Calls).into_iter().filter(|c| *c != f.id && ix.node(c).map_or(false, |n| !n.library)).collect();
        if !gives_nothing || callers.is_empty() {
            continue;
        }
        // what it runs, down to the end
        let mut runs: Vec<&str> = vec![];
        let mut seen: HashSet<&str> = HashSet::from([f.id.as_str()]);
        let mut todo = vec![f.id.as_str()];
        let mut something = false;
        while let Some(id) = todo.pop() {
            let Some(n) = ix.node(id) else { continue };
            if leaves_something(n) {
                something = true;
                break;
            }
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
        out.push(ix.finding(
            "nothing",
            f,
            format!("{name} gives nothing back, and Atlas sees no change and no place it reaches"),
            evidence,
            format!(
                "As far as Atlas can see, a call to {name} leaves nothing behind. Is it dead, or does it change what Atlas cannot see: a value it is handed (through an item or a loop), a global, a library's own state? Could it hand back what it makes, or let that value change itself through a method of its own?"
            ),
        ));
    }
    out
}

// ---------- prophecy ----------

/// Built code that leans on a prophecy: production would run on a stub. Code leans on what it calls,
/// builds, extends and requests, and on the types it takes, returns and holds; a member of a prophecy
/// is leaned on as the prophecy. A prophecy that supersedes something the code does not hold is a
/// false prophecy, and one whose name names several nodes does not say which it replaces. Prophecies
/// lean on one another freely, and test code may lean on the code it is written for before it is built.
fn prophecy(ix: &Index) -> Vec<Finding> {
    let mut out = vec![];
    for p in ix.g.nodes.iter().filter(|n| ix.superseded(n).is_none()) {
        let Some(said) = p.prophecy.as_ref().filter(|s| s.supersedes.is_some()) else { continue };
        let named = said.supersedes.as_deref().unwrap_or("");
        let finding = match said.candidates.as_slice() {
            [] => ix.finding(
                "prophecy",
                p,
                format!("{} supersedes `{named}`, which the code does not hold", ix.label(&p.id)),
                vec![format!("supersedes `{named}`")],
                format!("A prophecy that replaces nothing is built or false. Is `{named}` gone already, or is its name written otherwise?"),
            ),
            candidates => ix.finding(
                "prophecy",
                p,
                format!(
                    "{} supersedes `{named}`, which names {} nodes: {}",
                    ix.label(&p.id),
                    candidates.len(),
                    list(&candidates.iter().map(|c| format!("`{c}`")).collect::<Vec<_>>())
                ),
                candidates.iter().map(|c| format!("`{c}` ({})", ix.node(c).map_or_else(String::new, |n| ix.at(n)))).collect(),
                format!(
                    "A prophecy replaces one thing, and `{named}` names more than one. Which of them does it replace, named by its whole path?"
                ),
            ),
        };
        out.push(finding);
    }
    let mut by: BTreeMap<&str, Vec<(&Node, EdgeKind)>> = BTreeMap::new();
    for e in ix.g.edges.iter().filter(|e| DEPENDS.contains(&e.kind)) {
        let (Some(from), Some(to)) = (ix.node(&e.src), ix.node(&e.dst)) else { continue };
        if ix.prophetic(to) && !ix.prophetic(from) && !from.library && !ix.test(from) {
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
/// package quotient the picture draws (`flow::units`): the parts of a graph, the packages at the
/// top of each part, and the packages under each package, with the package's own module (`pkg$`,
/// its code in none of them) a block beside them. A block depends on another when its code calls,
/// builds, imports, extends or requests what the other holds, or takes, returns or holds one of its
/// types; test code is a way in from outside, not a layer.
fn layers(ix: &Index) -> Vec<Finding> {
    // the module a node's code stands in, for code that is a layer
    let module = |id: &str| -> Option<&str> {
        let n = ix.node(id).filter(|n| !n.library && n.kind != NodeKind::External && !ix.test(n))?;
        Some(if n.kind == NodeKind::Module { n.id.as_str() } else { n.module.as_str() })
    };
    // module -> module it depends on: the edges that make it so
    let mut between: BTreeMap<(&str, &str), Vec<&Edge>> = BTreeMap::new();
    for e in ix.g.edges.iter().filter(|e| e.kind == EdgeKind::Imports || DEPENDS.contains(&e.kind)) {
        if let (Some(a), Some(b)) = (module(&e.src), module(&e.dst)) {
            if a != b {
                between.entry((a, b)).or_default().push(e);
            }
        }
    }
    let first = ix.g.parts.first().map_or(ix.g.root.as_str(), |p| p.id.as_str());
    let part = |m: &str| ix.node(m).map(|n| n.part.as_str()).filter(|p| !p.is_empty()).unwrap_or(first);
    // each level: whether its blocks are the parts, what it is listed under, and its blocks
    let units = crate::flow::units(ix.g);
    let parts: Vec<String> = ix.g.parts.iter().map(|p| p.id.clone()).collect();
    let mut levels: Vec<(bool, Option<&str>, &[String])> = vec![];
    if parts.len() > 1 {
        levels.push((true, None, &parts));
    }
    levels.extend(units.tops.iter().map(|(p, top)| (false, Some(p.as_str()), top.as_slice())));
    levels.extend(units.kids.iter().map(|(u, kids)| (false, Some(u.as_str()), kids.as_slice())));
    let mut out = vec![];
    for (by_part, listed, blocks) in levels {
        let block = |m: &str| -> Option<&String> {
            match by_part {
                true => blocks.iter().find(|b| *b == part(m)),
                false => blocks.iter().find(|b| holds_module(b, m)),
            }
        };
        // block -> block it depends on -> the edges that make it so
        let mut deps: BTreeMap<String, BTreeMap<String, Vec<&Edge>>> = BTreeMap::new();
        for ((a, b), es) in &between {
            let (Some(x), Some(y)) = (block(a), block(b)) else { continue };
            if x != y {
                deps.entry(x.clone()).or_default().entry(y.clone()).or_default().extend(es.iter().copied());
            }
        }
        // the package the blocks stand under: the one they all share, else the one they are listed under
        let above = |b: &String| -> String {
            match b.strip_suffix('$') {
                Some(own) => own.to_string(),
                None => b.rsplit_once('.').map_or(String::new(), |(p, _)| p.to_string()),
            }
        };
        let shared = blocks.iter().map(above).reduce(|a, b| if a == b { a } else { String::new() }).filter(|p| !p.is_empty());
        let anchor = |p: &str| ix.node(p).filter(|n| !n.file.is_empty());
        for cycle in strongly_connected(&deps) {
            // the links inside the cycle, the thinnest first: the likeliest to run against the flow
            let mut links: Vec<(&str, &str, &Vec<&Edge>)> = vec![];
            for a in &cycle {
                for (b, es) in deps.get(a.as_str()).into_iter().flatten().filter(|(b, _)| cycle.contains(b)) {
                    links.push((a.as_str(), b.as_str(), es));
                }
            }
            links.sort_by_key(|(a, b, es)| (es.len(), *a, *b));
            // a call says more than a type, and a type more than an import
            let example = |es: &Vec<&Edge>| {
                let says = |e: &&&Edge| match e.kind {
                    EdgeKind::Imports => 2,
                    EdgeKind::Takes | EdgeKind::Returns | EdgeKind::Has | EdgeKind::Variant => 1,
                    _ => 0,
                };
                let e = es.iter().min_by_key(says).unwrap_or(&es[0]);
                format!("`{}` → `{}`", ix.label(&e.src), ix.label(&e.dst))
            };
            let mut evidence: Vec<String> = links
                .iter()
                .map(|(a, b, es)| format!("{} → {}: {} ({} in all)", block_name(a), block_name(b), example(es), es.len()))
                .collect();
            for own in cycle.iter().filter_map(|b| b.strip_suffix('$')) {
                let file = ix.node(own).map_or_else(String::new, |n| format!(" ({})", ix.file(n)));
                evidence.push(format!("{} itself: the code of `{own}` in none of the packages below it{file}", block_name(own)));
            }
            let Some(at) =
                shared.as_deref().and_then(anchor).or_else(|| listed.and_then(anchor)).or_else(|| ix.node(cycle[0].trim_end_matches('$')))
            else {
                continue;
            };
            let members: Vec<String> = cycle.iter().map(|c| block_name(c)).collect();
            let ask = match links.as_slice() {
                [(a, b, es), rest @ ..] if rest.iter().all(|(_, _, more)| more.len() > es.len()) => {
                    format!(
                        "The thinnest link is {} → {} ({}). Does what it uses belong in {}, or in a package below both?",
                        block_name(a),
                        block_name(b),
                        example(es),
                        block_name(a)
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

/// Whether a block of the package quotient holds a module: a package everything under it, a
/// package's own module (`pkg$`) itself alone.
fn holds_module(block: &str, m: &str) -> bool {
    match block.strip_suffix('$') {
        Some(own) => m == own,
        None => m.strip_prefix(block).is_some_and(|rest| rest.is_empty() || rest.starts_with('.')),
    }
}

/// A block of the package quotient as the picture names it: a package by its last name, and a
/// package's own module as `name itself`.
fn block_name(block: &str) -> String {
    match block.strip_suffix('$') {
        Some(own) => format!("{} itself", own.rsplit('.').next().unwrap_or(own)),
        None => block.rsplit('.').next().unwrap_or(block).to_string(),
    }
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

/// A class whose operations both write and read one place where data rests (a table, a file) is a
/// lens on it: its puts take a value and change the place (`A × S → S`), its gets give a value
/// from it (`S → A`). Put then get gives back what was put. Asked about each type of data a put
/// takes that no get of the same class gives from that place, as it is or inside what it gives
/// (`holds`): what the place holds is then another type than what goes in, or another class reads
/// it. A class that only reads a place (input from the world) or only writes it (an end product)
/// is no lens on it; an operation that reads the place and gives no value reads it on its way to
/// writing it, and is a put. A constructor that reads the place gives its class from it, and is a
/// get. A put is asked about in the class that defines it: a subclass's gets all count, but the
/// puts it inherits are its base's question. What a put takes is what reaches the place
/// (`Places::reached`).
fn lens<'g>(ix: &Index<'g>) -> Vec<Finding> {
    let data = |t: &str, own: &str| {
        t != own && ix.node(t).map_or(false, |n| matches!(n.kind, NodeKind::Class | NodeKind::External) && n.door.is_none())
    };
    // a place the code names, not only the variable that holds it: what two classes do there is
    // done to the same place
    let named = |place: &str| !place.starts_with("w:Disk:");
    let places = Places { ix, reached: RefCell::default(), into: RefCell::default() };
    // whether an operation gives a value: a constructor gives its class
    let gives = |m: &Node| m.ctor || m.returns.as_ref().map_or(false, |r| !r.value.is_empty() || r.own);
    // the types of data operation `m` of class `c` gives: what it returns, the class's own type
    // among them, which a constructor and a method that gives `Self` give too
    let given = |c: &'g Node, m: &Node| -> Vec<&'g str> {
        let mut types: Vec<&'g str> = ix.targets(&m.id, EdgeKind::Returns).into_iter().filter(|t| *t == c.id || data(t, &c.id)).collect();
        if (m.ctor || m.returns.as_ref().map_or(false, |r| r.own)) && !types.contains(&c.id.as_str()) {
            types.push(&c.id);
        }
        types
    };
    // what each operation puts to and gets from: a constructor puts nothing, and an operation that
    // gives no value gets nothing
    let operations = |c: &Node| -> Vec<(&'g Node, BTreeMap<String, String>, BTreeMap<String, String>)> {
        ix.interface(&c.id)
            .into_iter()
            .filter(|m| is_function(m) && (m.ctor || !m.private))
            .map(|m| {
                let (puts, gets) = places.reaches(&m.id);
                let puts = if m.ctor { BTreeMap::new() } else { puts };
                let gets = if gives(m) { gets } else { BTreeMap::new() };
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
                let held: BTreeSet<&str> = given(c, m).into_iter().flat_map(|t| holds(ix, t)).collect();
                for t in held {
                    got_from.entry((place.clone(), t)).or_default().push(ix.label(&m.id));
                }
            }
        }
    }
    let mut out = vec![];
    for c in classes {
        let ops = operations(c);
        let ids: HashSet<&str> = ops.iter().filter(|(m, _, _)| !m.ctor).map(|(m, _, _)| m.id.as_str()).collect();
        let name = ix.label(&c.id);
        // the places this class both puts to and gets from
        let places: BTreeMap<&String, &String> =
            ops.iter().flat_map(|(_, puts, _)| puts.iter()).filter(|(p, _)| ops.iter().any(|(_, _, gets)| gets.contains_key(*p))).collect();
        let (mut lost_all, mut labels, mut evidence): (Vec<String>, Vec<String>, Vec<String>) = (vec![], vec![], vec![]);
        for (place, label) in places {
            let (mut put, mut got, mut back): (BTreeMap<&str, Vec<String>>, Vec<String>, HashSet<&str>) =
                (BTreeMap::new(), vec![], HashSet::new());
            for (m, puts, gets) in &ops {
                if puts.contains_key(place) && ix.declares(&c.id, m) {
                    for t in put_types(c, m, place, &ids) {
                        put.entry(t).or_default().push(format!("{}: {} × {label} → {label}", ix.label(&m.id), ix.label(t)));
                    }
                }
                if gets.contains_key(place) {
                    let types = given(c, m);
                    back.extend(types.iter().flat_map(|t| holds(ix, t)));
                    let what = if types.is_empty() {
                        written(m).to_string()
                    } else {
                        types.iter().map(|t| ix.label(t)).collect::<Vec<_>>().join(", ")
                    };
                    got.push(format!("{}: {label} → {what}", ix.label(&m.id)));
                }
            }
            let lost: Vec<&str> = put.keys().copied().filter(|t| !back.contains(t)).collect();
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
/// its lists, the variants of its sums, and its subclasses, the variants of a sum left open. A get
/// that gives `t` gives back each of these: a lens composed with the projection to a part of what
/// it gives is a lens on that part.
fn holds<'g>(ix: &Index<'g>, t: &'g str) -> HashSet<&'g str> {
    let mut seen = HashSet::from([t]);
    let mut todo = vec![t];
    while let Some(x) = todo.pop() {
        let parts = ix.targets(x, EdgeKind::Has).into_iter().chain(ix.targets(x, EdgeKind::Variant));
        for y in parts.chain(ix.sources(x, EdgeKind::Inherits)) {
            if seen.insert(y) {
                todo.push(y);
            }
        }
    }
    seen
}

/// The place a touch holds data at, as a lens sees it: the table a statement names (a key-value
/// store's key), a file where the code names it, by its path or by the value that holds it
/// (`Disk(Log.path)`). A statement whose table the reader does not know, a database whose
/// statements it does not read, and a file known only by its format (`*.json`) give no place: a
/// lens is not guessed. A touch that makes or tests a place's shape holds no data there.
fn held(t: &Touch) -> Option<(String, String)> {
    let at = |t: &Touch| {
        let p = t.place();
        Some((p.id, p.label))
    };
    match t.kind {
        _ if t.shape() => None,
        World::Db(_) if !t.artifact.is_empty() => at(t),
        World::Disk if format_only(&t.artifact) => {
            (!t.what.is_empty()).then(|| at(&Touch { artifact: String::new(), ..t.clone() })).flatten()
        }
        World::Disk if !t.what.is_empty() || !t.artifact.is_empty() => at(t),
        _ => None,
    }
}

/// A file named only by its format (`*.json`): the code says what kind of file, not which.
fn format_only(artifact: &str) -> bool {
    artifact.strip_prefix("*.").map_or(false, |ext| !ext.is_empty() && !ext.contains(['/', '*']))
}

/// Whether a touch may write what its call is handed into its place: a statement whose values
/// hold a placeholder or a value spliced in, and any write that is no statement.
fn writes_handed(t: &Touch) -> bool {
    match t.kind {
        World::Db(db) if crate::effects::reads_statements(db) => crate::effects::writes_handed(db, &t.what),
        _ => true,
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
    /// call into the world is handed, unless its statement writes none of what it is handed (a
    /// `DELETE`, a `WHERE` key: `effects::writes_handed`); a call Atlas cannot follow may put what
    /// it is handed wherever `f` puts; a value no step gives and no parameter is
    /// (`for r in readings`, `x = p.field`) is taken from any parameter whose type holds its type,
    /// as the parameter is declared or handed; and a parameter neither handed to a step by name
    /// nor holding such a value reaches the place when any step puts there.
    fn reached(&self, f: &'g Node, place: &str, theirs: &HashSet<&str>) -> BTreeSet<&'g str> {
        let ix = self.ix;
        let params: Vec<&'g str> = f.params.iter().filter(|p| !p.receiver).map(|p| p.name.as_str()).collect();
        // a call of its own into the place may be handed any of them
        if f.touches.iter().any(|t| t.dir == Some(Dir::Write) && writes_handed(t) && held(t).map_or(false, |(p, _)| p == place)) {
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

#[cfg(test)]
mod tests {
    use super::of_itself;

    /// A value read through its own fields, items at a literal and calls handed nothing is the
    /// same value in every body; one read through another name is its body's own.
    #[test]
    fn a_path_of_the_value_itself_names_nothing_else() {
        for own in ["", ".mode", ".args.mode", "[0]", "['mode']", ".kind()", "[\"a\"].kind()"] {
            assert!(of_itself(own), "{own}");
        }
        for other in [".nodes[t].kind", ".get(k)", "[i + 1]", ".kind(x)", ".a[0"] {
            assert!(!of_itself(other), "{other}");
        }
    }
}
