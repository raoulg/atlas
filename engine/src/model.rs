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
    Mutates,
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
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_pkg: bool,
}

impl Default for NodeKind {
    fn default() -> Self {
        NodeKind::Module
    }
}

#[derive(Serialize, Deserialize, TS, Clone, Debug)]
#[ts(export)]
pub struct Edge {
    pub src: String,
    pub dst: String,
    pub kind: EdgeKind,
    /// The attribute or field that carries a `has` edge.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub via: Option<String>,
    /// How many original edges a quotient edge stands for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight: Option<u32>,
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
    #[serde(default, deserialize_with = "de_rules")]
    #[ts(type = "Array<[string, string]>")]
    pub rules: Vec<(String, String)>,
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

/// Rules may be written as a map (`rkg.source: Source`) or as a list of pairs.
fn de_rules<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<(String, String)>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Rules {
        Map(IndexMap<String, String>),
        List(Vec<(String, String)>),
    }
    Ok(match Rules::deserialize(d)? {
        Rules::Map(m) => m.into_iter().collect(),
        Rules::List(l) => l,
    })
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

#[derive(Serialize, Deserialize, TS, Clone, Debug, Default)]
#[ts(export)]
pub struct Graph {
    pub root: String,
    pub roles: Architecture,
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
