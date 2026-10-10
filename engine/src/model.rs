//! The one central type: a typed graph of a repository.
//!
//! Everything between `parse` and `layout` is `Graph -> Graph`. The JSON shape is the
//! contract with the frontend; `ts-rs` generates the TypeScript side from these types.

use std::collections::BTreeMap;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Serialize, Deserialize, TS, Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum NodeKind {
    Module,
    Class,
    Function,
    Method,
    External,
    /// A file or table that data rests in between routes.
    Artifact,
    /// A resting place in the world: web, disk, a database, a hub type, the browser.
    Place,
}

#[derive(Serialize, Deserialize, TS, Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum EdgeKind {
    Contains,
    Imports,
    Inherits,
    Calls,
    Instantiates,
    /// Parameter type.
    Takes,
    /// Return type.
    Returns,
    /// Attribute of a type.
    Has,
    Reads,
    Writes,
    /// A parameter or the receiver the body changes in place; `via` names which.
    Mutates,
    /// A sum type and one of the types it may be: `Result = FitResult | FailedFit`.
    Variant,
    /// A node that names a route another part serves, to the route's handler: a request over
    /// HTTP; `via` names the path.
    Requests,
}

#[derive(Serialize, Deserialize, TS, Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum EntryKind {
    Make,
    Cli,
    Script,
    Route,
    Mcp,
    Notebook,
    /// A name a package or crate exports at its root: a route into it as a library.
    Export,
    Test,
}

/// A route into the program.
#[derive(Serialize, Deserialize, TS, Clone, PartialEq, Eq, Debug)]
#[ts(export)]
pub struct Entry {
    pub kind: EntryKind,
    pub label: String,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, Default)]
#[ts(export)]
pub struct Node {
    pub id: String,
    pub kind: NodeKind,
    pub name: String,
    pub file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<(u32, u32)>,
    pub module: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub doc: String,
    #[serde(default)]
    pub role: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub role_source: String,
    /// Owning class, for methods.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cls: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bases: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub methods: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub decorators: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub returns: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry: Option<Entry>,
    /// Calls that leave the package, by the part of the world they touch.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub boundaries: BTreeMap<String, u32>,
    /// The same calls with what they touch: a path literal, a table, a command, or the
    /// argument's name when nothing better is known.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub touches: Vec<Touch>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_pkg: bool,
    /// Where the body compares a value with literals.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub compares: Vec<Compare>,
    /// The part of the repository the node belongs to.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub part: String,
}

impl Default for NodeKind {
    fn default() -> Self {
        NodeKind::Module
    }
}

/// One kind of boundary call and what it touches.
#[derive(Serialize, Deserialize, TS, Clone, Debug, Default, PartialEq, Eq)]
#[ts(export)]
pub struct Touch {
    pub kind: String,
    pub what: String,
    pub n: u32,
    /// What flows through the boundary, as the author named it, with its type when known.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub payload: String,
    /// The operation as called: `write`, `create_dir_all`, `read_text`, `get`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub op: String,
    /// The file or table as the code names it: a literal path or pattern (`out.json`,
    /// `{name}.png`), `*.csv` when only the format is known, a table after a SQL verb. Empty
    /// when only a variable says which.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub artifact: String,
    /// `read` or `write`, when the kind does not say: a database statement's verb decides.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub dir: String,
    /// The code alone decides where: the path or URL starts at a literal (`"out.csv"`,
    /// `Path("data") / name`), not at a value a caller hands in (`root / "out.csv"`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub fixed: bool,
    /// The line of the first such call, 1-based.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub line: u32,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

#[derive(Serialize, Deserialize, TS, Clone, Debug)]
#[ts(export)]
pub struct Edge {
    pub src: String,
    pub dst: String,
    pub kind: EdgeKind,
    /// The attribute or field that carries a `has` edge; the parameter (or `self`) a
    /// `mutates` edge changes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub via: Option<String>,
    /// How many original edges a quotient edge stands for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight: Option<u32>,
    /// The arms of the choices a call sits in, from the outermost in: `(choice, arm)`, with
    /// the choices of a body numbered in source order. Two calls from one body are
    /// alternatives when they sit in different arms of the same choice.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(type = "Array<[number, number]>")]
    pub arms: Vec<(u32, u32)>,
    /// For a call: the caller uses what the call gives back (assigns it, hands it on, calls a
    /// method on it), rather than only calling it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub uses: bool,
}

/// Where a body compares one value with literals: `args.mode == "train"`,
/// `mode in {"a", "b"}`, `case "a":`, `"a" =>`.
#[derive(Serialize, Deserialize, TS, Clone, Debug, Default, PartialEq, Eq)]
#[ts(export)]
pub struct Compare {
    /// The value as written: `args.experiment`.
    pub subject: String,
    /// The literals it is compared with.
    pub cases: Vec<String>,
    pub line: u32,
    /// The choice it decides, numbered like `Edge::arms`; a comparison that only computes a
    /// value (`weighted = mode in {..}`) decides none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choice: Option<u32>,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, Default)]
#[ts(export)]
pub struct Bands {
    #[serde(default)]
    pub top: Vec<String>,
    #[serde(default)]
    pub bottom: Vec<String>,
}

/// What a project declares about itself: an override on naming and on which flows are
/// by design, never the source of the picture.
#[derive(Serialize, Deserialize, TS, Clone, Debug, Default)]
#[ts(export)]
pub struct Architecture {
    #[serde(default)]
    pub order: Vec<String>,
    #[serde(default)]
    pub bands: Bands,
    /// Package prefix -> role, longest prefix wins.
    #[serde(default)]
    pub rules: Rules,
    /// Base class -> role; a class whose base chain reaches one takes that role.
    #[serde(default)]
    pub base_roles: BTreeMap<String, String>,
    /// `against_flow` (consumers call providers) or `with_flow`.
    #[serde(default = "against_flow")]
    pub dependencies: String,
    /// Role pairs that are fine in both directions.
    #[serde(default)]
    pub allowed: Vec<(String, String)>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub inferred: bool,
}

fn against_flow() -> String {
    "against_flow".into()
}

/// Package prefix -> role pairs. Written as a map (`rkg.source: Source`) or as a list of
/// pairs; serialized as a list.
#[derive(TS, Clone, Debug, Default, PartialEq)]
#[ts(export)]
pub struct Rules(#[ts(type = "Array<[string, string]>")] pub Vec<(String, String)>);

impl Serialize for Rules {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(s)
    }
}

impl<'de> Deserialize<'de> for Rules {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Input {
            Map(IndexMap<String, String>),
            List(Vec<(String, String)>),
        }
        Ok(Rules(match Input::deserialize(d)? {
            Input::Map(m) => m.into_iter().collect(),
            Input::List(l) => l,
        }))
    }
}

impl std::ops::Deref for Rules {
    type Target = Vec<(String, String)>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<Vec<(String, String)>> for Rules {
    fn from(v: Vec<(String, String)>) -> Self {
        Rules(v)
    }
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, Default)]
#[ts(export)]
pub struct MakeTarget {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub doc: String,
    #[serde(default)]
    pub deps: Vec<String>,
    #[serde(default)]
    pub recipe: Vec<String>,
    /// Entry ids this target runs, in recipe order.
    #[serde(default)]
    pub calls: Vec<String>,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, Default)]
#[ts(export)]
pub struct Stats {
    pub modules: usize,
    pub nodes: BTreeMap<String, usize>,
    pub edges: BTreeMap<String, usize>,
    pub unresolved_calls_total: u32,
    pub unresolved_calls_top: Vec<(String, u32)>,
}

/// One part of a repository: what one manifest roots (a crate, a Python package, an app),
/// read in its own language.
#[derive(Serialize, Deserialize, TS, Clone, Debug, Default, PartialEq)]
#[ts(export)]
pub struct Part {
    pub id: String,
    pub language: String,
    /// The directory that holds the part's manifest.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub dir: String,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, Default)]
#[ts(export)]
pub struct Graph {
    pub root: String,
    /// The language the repo was parsed as: `python`, `rust`, `typescript`; empty when its
    /// parts are read in several.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub language: String,
    /// The directory that holds the repo's own declarations: Makefile, pyproject.toml,
    /// Cargo.toml, architecture.yml.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub project_root: String,
    pub roles: Architecture,
    /// The parts of the repository, in the order they were found.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<Part>,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    /// Absolute path -> file text, for `inspect`.
    #[serde(default)]
    pub files: BTreeMap<String, String>,
    #[serde(default)]
    pub make: Vec<MakeTarget>,
    #[serde(default)]
    pub stats: Stats,
}

impl Graph {
    pub fn node(&self, id: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// Nodes by id, in insertion order.
    pub fn index(&self) -> IndexMap<&str, &Node> {
        self.nodes.iter().map(|n| (n.id.as_str(), n)).collect()
    }

    pub fn recount(&mut self) {
        let mut nodes = BTreeMap::new();
        for n in &self.nodes {
            *nodes.entry(kind_name(&n.kind)).or_insert(0) += 1;
        }
        let mut edges = BTreeMap::new();
        for e in &self.edges {
            *edges.entry(edge_name(&e.kind)).or_insert(0) += 1;
        }
        self.stats.nodes = nodes;
        self.stats.edges = edges;
        self.stats.modules = self.nodes.iter().filter(|n| n.kind == NodeKind::Module).count();
    }
}

pub fn kind_name(k: &NodeKind) -> String {
    serde_json::to_value(k).unwrap().as_str().unwrap().to_string()
}

pub fn edge_name(k: &EdgeKind) -> String {
    serde_json::to_value(k).unwrap().as_str().unwrap().to_string()
}
