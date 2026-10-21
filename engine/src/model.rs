//! The one central type: a typed graph of a repository.
//!
//! Everything between `parse` and `layout` is `Graph -> Graph`. The JSON shape is the
//! contract with the frontend; `ts-rs` generates the TypeScript side from these types.

use std::collections::{BTreeMap, HashMap, VecDeque};

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
    /// An operation of sketch code that is not built yet: a route into what is planned.
    Prophecy,
    Test,
}

/// A route into the program.
#[derive(Serialize, Deserialize, TS, Clone, PartialEq, Eq, Debug)]
#[ts(export)]
pub struct Entry {
    pub kind: EntryKind,
    pub label: String,
    /// What the human hands the entry on the command line, in the code's order, each as they type
    /// it: `--raw` for a named option, `<name>` for a positional argument. The domain of the entry:
    /// it reads as its usage line, `$ sunspots --raw --figure --months`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
}

impl Entry {
    pub fn new(kind: EntryKind, label: impl Into<String>) -> Entry {
        Entry { kind, label: label.into(), options: vec![] }
    }
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
    /// Owning class, for methods.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cls: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bases: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub methods: Vec<String>,
    /// For a class: the fields it declares, by name, beside its methods.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub decorators: Vec<String>,
    /// Sketch code kept in the tree, not built yet: what its marker says. Read by the language's reader.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prophecy: Option<crate::prophecy::Prophecy>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<Param>,
    /// What the signature says it gives back; none when it does not say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub returns: Option<Returns>,
    /// The signatures its callers are checked against, where the code declares them apart from
    /// the body that runs: its overloads, in the order they are written. `params` and `returns`
    /// are then the implementation's, which no caller is checked against.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overloads: Vec<Signature>,
    /// For a method that overrides another: whether it can stand where each method it overrides
    /// stood, what it takes and what it gives, as the language's type checker decides; none where no
    /// checker decided.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keeps: Option<bool>,
    /// For an override that does not keep its promise: the method whose promise it breaks, as the
    /// type checker names it, where the graph holds that method.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub breaks: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry: Option<Entry>,
    /// Its package exports it, where its entry is a route of another kind: a program a manifest
    /// declares, whose function the package also exports, is a command and a door.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub exported: bool,
    /// The calls that leave the package, with what they touch: a path literal, a table, a
    /// command, or the argument's name when nothing better is known.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub touches: Vec<Touch>,
    /// Calls as they were read, before their callers made their paths whole: a route that
    /// reaches none of those callers shows the call this way.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub instead: Vec<Touch>,
    /// The requests it makes whole that no route of its own part serves: the method when the code
    /// says it, and the path (`GET /api/items/{id}`). Another part's route may serve them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub asks: Vec<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_pkg: bool,
    /// For a method: it is called on its class, not on a value of it, so the class is no input.
    /// A Python class or static method, a Rust function in an `impl` without `self`, a
    /// TypeScript `static` method.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub on_class: bool,
    /// Calling it makes a value of its class: an initializer, which fills in the value a call to
    /// the class makes (so it is not `on_class`), or a function on the class that gives one.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ctor: bool,
    /// Only the code beside it may use it: its module, or for a member its class.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub private: bool,
    /// Its body may end it without giving back: it raises, throws or panics, outside the
    /// functions it defines, or it calls what ends the program (`sys.exit`, `process.exit`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub raises: bool,
    /// It calls a function it is handed: what that function does is its caller's to say.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub calls_handed: bool,
    /// For a method: it has no body of its own. An interface, a protocol, a trait or an abstract
    /// class declares it, and a call to it runs what implements it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub declared: bool,
    /// What its body does that the graph holds nothing else of, by line and as the code writes it:
    /// a call whose value goes nowhere, made for an effect Atlas does not know, a library's of
    /// which no table says it crosses into the world, changes a value or ends the program
    /// (`logger.info(..)`, `tensor.mul_(..)`), or one Atlas cannot follow (`self.on_done(..)`);
    /// and a value a class keeps, which the body changes (`Counter.total += 1`). What these leave
    /// behind, Atlas does not see.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(type = "Array<[number, string]>")]
    pub unseen: Vec<(u32, String)>,
    /// For a function handed to a call where it is declared (`memo((props) => ..)`,
    /// `createServerFn(..).handler(..)`): the call, which gives its parameters their types.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub typed_by: String,
    /// Where the body compares a value with literals.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub compares: Vec<Compare>,
    /// The part of the world a value of this type is a door to, when it is one. A library's type:
    /// what its library's calls reach (`db_sqlite` for `sqlite3.Connection`). A class of the
    /// package: what one of its operations reaches, through what it calls (a sink, a repository,
    /// a client).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(type = "string")]
    pub door: Option<World>,
    /// The part of the repository the node belongs to.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub part: String,
    /// Code of a library the project uses, read as far as the project's calls reach into it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub library: bool,
    /// Test code, by where it stands: a module the language keeps for tests (a test file, a Rust
    /// `#[cfg(test)]` module, a crate's `tests/`), or an item of one that the graph keeps in the
    /// module around it (a Rust `mod tests { .. }`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub test: bool,
    /// The calls of its body, in the order they run: the steps of a route through it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sites: Vec<Site>,
    /// The lines each arm of its body's choices spans, first and last: what is written there runs
    /// only in that arm.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(type = "Array<[number, number]>")]
    pub arm_lines: Vec<(u32, u32)>,
    /// What decides each arm of its body's choices, as the code writes it: `(choice, arm, when)`,
    /// with the choices and arms numbered as `Site::arms` numbers them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(type = "Array<[number, number, string]>")]
    pub whens: Vec<(u32, u32, String)>,
}

impl Node {
    /// For a class: what a call to it runs, among its methods: its initializer.
    pub fn initializer<'a>(&self, node: impl Fn(&str) -> Option<&'a Node>) -> Option<String> {
        self.methods.iter().map(|m| format!("{}.{m}", self.id)).find(|id| node(id).map_or(false, |m| m.ctor && !m.on_class))
    }
}

/// A parameter, as the signature declares it.
#[derive(Serialize, Deserialize, TS, Clone, Debug, Default, PartialEq, Eq)]
#[ts(export)]
pub struct Param {
    /// The name the body holds it by: `args` for `*args`, `xs` for `...xs`, a pattern as written.
    pub name: String,
    /// Its type as written; empty when the signature does not say.
    #[serde(default, rename = "type", skip_serializing_if = "String::is_empty")]
    pub ty: String,
    /// The value the function is called on, or the class it is called through: no input of its
    /// own.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub receiver: bool,
    /// It takes whatever arguments are left.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub rest: bool,
    /// The signature says the body may change it in place.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub changes: bool,
    /// Its type is the unchecked one, which every value passes as and which passes as every type:
    /// written out, it says no more than a parameter without a type.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unchecked: bool,
}

impl Param {
    /// Whether the signature says what it is: a type is written, and it is not the unchecked one.
    pub fn says(&self) -> bool {
        !self.ty.is_empty() && !self.unchecked
    }
}

/// `name: type`, and `…name` for one that takes whatever arguments are left.
impl std::fmt::Display for Param {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}{}", if self.rest { "…" } else { "" }, self.name)?;
        if !self.ty.is_empty() {
            write!(f, ": {}", self.ty)?;
        }
        Ok(())
    }
}

/// What a signature says a function gives back.
#[derive(Serialize, Deserialize, TS, Clone, Debug, Default, PartialEq, Eq)]
#[ts(export)]
pub struct Returns {
    /// The type as written.
    pub written: String,
    /// The type of the value it gives, out of the effect its language wraps a value in (it may
    /// fail, it arrives later) and out of a reference. Empty when it gives no value: the unit
    /// type, or a value of the class it runs on (`own`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub value: String,
    /// It gives a value of the class it runs on, whichever subclass that is.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub own: bool,
    /// The effect it is wrapped in says it may end without its value: a `Result` that fails. A
    /// `Promise` says the value arrives later; it rejects where its body throws, as any call may.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub fails: bool,
    /// The type of the value it gives is the unchecked one (`Param::unchecked`): it says no more
    /// than leaving the type out.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unchecked: bool,
}

impl Returns {
    /// Whether it says what the function gives: its type is not the unchecked one.
    pub fn says(&self) -> bool {
        !self.unchecked
    }
}

/// A signature declared apart from the body that runs: an overload.
#[derive(Serialize, Deserialize, TS, Clone, Debug, Default, PartialEq, Eq)]
#[ts(export)]
pub struct Signature {
    /// The line it is written on.
    pub line: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<Param>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub returns: Option<Returns>,
}

/// A call in a body, as a step of the route through it: what it reaches, what it is handed and
/// what it gives back.
#[derive(Serialize, Deserialize, TS, Clone, Debug, Default, PartialEq)]
#[ts(export)]
pub struct Site {
    pub line: u32,
    /// The function, method or class the call reaches, when Atlas reads it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub to: String,
    /// The operation of a library Atlas does not read, when it turns a value of one type into
    /// another (`to_numpy`), or a call Atlas cannot follow, as the code writes it; `to` is then
    /// empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub op: String,
    /// Atlas cannot follow the call: what it runs, and what becomes of what it is handed, the
    /// graph does not say.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub not_followed: bool,
    /// The call changes the value it is called on, and gives it (`rows.append(row)`): a step
    /// `T × A → T`, never an identity, however much of `A` the graph follows.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub changes: bool,
    /// The class of the value a method is called on, when that is a subclass of the method's own
    /// class (`ComparePlot(..).plot()` reaches `BasePlot.plot` on a `ComparePlot`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub on: String,
    /// The call is made on the body's own receiver (`self.build(..)`): on a value of a
    /// subclass, it reaches that subclass's method.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub on_receiver: bool,
    /// The values the call is handed: its arguments and the value it is called on.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hands: Vec<Held>,
    /// What it gives back.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gives: Vec<Held>,
    /// The arms of the choices the call sits in, as `Edge::arms`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(type = "Array<[number, number]>")]
    pub arms: Vec<(u32, u32)>,
    /// For a call that parses the command line the program is run with into values (clap's
    /// `Cli::parse()`, `parseArgs(..)`, argparse's `parser.parse_args()`): the options it declares,
    /// as `Entry::options` writes them. What it gives is the entry's domain, so the step belongs to
    /// the entry, not to the route.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_line: Option<Vec<String>>,
    /// Where the call is written, as a byte range of its file; `(0, 0)` for a step no expression
    /// of the body is (a function handed on, called by whoever receives it).
    #[serde(skip)]
    pub span: (u32, u32),
}

/// A value by its type, and the name the code holds it by when it has one. A value no name holds
/// (one call's result handed straight to another) is named by the step that makes it: `#3`.
#[derive(Serialize, Deserialize, TS, Clone, Debug, Default)]
#[ts(export)]
pub struct Held {
    #[serde(rename = "type")]
    pub ty: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Where the code writes it, as a byte offset of its file: where it comes from, not what it is.
    #[serde(skip)]
    pub at: u32,
}

impl Held {
    pub fn new(ty: impl Into<String>, name: impl Into<String>) -> Held {
        Held { ty: ty.into(), name: name.into(), at: 0 }
    }
}

/// A value is its type and its name; where it is written does not make it another.
impl PartialEq for Held {
    fn eq(&self, other: &Held) -> bool {
        (&self.ty, &self.name) == (&other.ty, &other.name)
    }
}

impl Eq for Held {}

impl std::hash::Hash for Held {
    fn hash<H: std::hash::Hasher>(&self, h: &mut H) {
        (&self.ty, &self.name).hash(h);
    }
}

impl Default for NodeKind {
    fn default() -> Self {
        NodeKind::Module
    }
}

/// The part of the world a call reaches outside the program: where data rests besides memory.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub enum World {
    /// Files and directories.
    Disk,
    /// The web, or another program over a socket.
    Net,
    Db(Db),
    /// A language model behind an API.
    Llm,
    /// A model run in the program on weights it loaded.
    Model,
    /// Another program, run by this one.
    Process,
    /// The terminal or a page: what a human sees.
    Screen,
    /// A figure, drawn to be seen.
    Figure,
    /// A value a module keeps between calls, which its functions change: a global.
    Global,
}

/// The kind of database a driver reaches, named the same in every language.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub enum Db {
    Sql,
    Sqlite,
    Duckdb,
    Graph,
    Kv,
}

/// Which way data goes between the program and a place.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub enum Dir {
    Read,
    Write,
}

impl World {
    /// The name the graph's JSON gives it: `disk`, `net`, `db_sqlite`.
    pub fn as_str(self) -> &'static str {
        match self {
            World::Disk => "disk",
            World::Net => "net",
            World::Db(Db::Sql) => "db_sql",
            World::Db(Db::Sqlite) => "db_sqlite",
            World::Db(Db::Duckdb) => "db_duckdb",
            World::Db(Db::Graph) => "db_graph",
            World::Db(Db::Kv) => "db_kv",
            World::Llm => "llm",
            World::Model => "model",
            World::Process => "process",
            World::Screen => "screen",
            World::Figure => "figure",
            World::Global => "global",
        }
    }

    pub fn parse(s: &str) -> Option<World> {
        ALL_WORLDS.iter().copied().find(|w| w.as_str() == s)
    }

    /// A service a door leads to: a database, the web, a model or a program. A file or the screen
    /// is none: a value may save or show itself.
    pub fn is_service(self) -> bool {
        matches!(self, World::Db(_) | World::Net | World::Llm | World::Model | World::Process)
    }

    /// Whether a literal handed to the call names a place someone could want elsewhere: a file
    /// or a URL. A table in a statement or a command line is what the call does, not where.
    pub fn located(self) -> bool {
        matches!(self, World::Disk | World::Net)
    }
}

const ALL_WORLDS: &[World] = &[
    World::Disk,
    World::Net,
    World::Db(Db::Sql),
    World::Db(Db::Sqlite),
    World::Db(Db::Duckdb),
    World::Db(Db::Graph),
    World::Db(Db::Kv),
    World::Llm,
    World::Model,
    World::Process,
    World::Screen,
    World::Figure,
    World::Global,
];

impl Dir {
    pub fn as_str(self) -> &'static str {
        match self {
            Dir::Read => "read",
            Dir::Write => "write",
        }
    }

    pub fn parse(s: &str) -> Option<Dir> {
        match s {
            "read" => Some(Dir::Read),
            "write" => Some(Dir::Write),
            _ => None,
        }
    }
}

impl Serialize for World {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for World {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        World::parse(&s).ok_or_else(|| serde::de::Error::custom(format!("no part of the world called `{s}`")))
    }
}

impl TS for World {
    type WithoutGenerics = Self;
    type OptionInnerType = Self;
    fn name(_: &ts_rs::Config) -> String {
        "string".into()
    }
    fn inline(_: &ts_rs::Config) -> String {
        "string".into()
    }
}

/// A boundary a call crosses: the part of the world it reaches, and which way when the call
/// says (`fs::write` writes; `open(p)` may go either way until its mode is read).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Crossing {
    pub world: World,
    pub dir: Option<Dir>,
}

impl Crossing {
    pub const fn to(world: World) -> Crossing {
        Crossing { world, dir: None }
    }
    pub const fn read(world: World) -> Crossing {
        Crossing { world, dir: Some(Dir::Read) }
    }
    pub const fn write(world: World) -> Crossing {
        Crossing { world, dir: Some(Dir::Write) }
    }

    /// The kind the graph's JSON gives a touch: a file's carries its direction (`disk_read`,
    /// `disk_write`, `disk` when the call does not say which way); another's is its world's name,
    /// with the direction beside it.
    pub fn kind_name(self) -> &'static str {
        match (self.world, self.dir) {
            (World::Disk, Some(Dir::Read)) => "disk_read",
            (World::Disk, Some(Dir::Write)) => "disk_write",
            (w, _) => w.as_str(),
        }
    }

    /// The direction the graph's JSON writes beside the kind: none for a file, whose kind says it.
    pub fn dir_name(self) -> &'static str {
        match (self.world, self.dir) {
            (World::Disk, _) | (_, None) => "",
            (_, Some(d)) => d.as_str(),
        }
    }

    /// The crossing a JSON kind and direction name.
    pub fn named(kind: &str, dir: &str) -> Option<Crossing> {
        match kind {
            "disk_read" => Some(Crossing::read(World::Disk)),
            "disk_write" => Some(Crossing::write(World::Disk)),
            k => World::parse(k).map(|world| Crossing { world, dir: Dir::parse(dir) }),
        }
    }
}

/// One kind of boundary call and what it touches. A file's direction is written into its kind
/// in the graph's JSON (`disk_read`, `disk_write`, `disk` when the call does not say); another
/// place's is `dir`.
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq, Eq)]
#[serde(into = "TouchJson", try_from = "TouchJson")]
#[ts(export, as = "TouchJson")]
pub struct Touch {
    pub kind: World,
    /// Which way data goes, when the call says: a file's mode, a database statement's verb, a
    /// download or an upload.
    pub dir: Option<Dir>,
    pub what: String,
    pub n: u32,
    /// What flows through the boundary, as the author named it, with its type when known.
    pub payload: String,
    /// The operation as called: `write`, `create_dir_all`, `read_text`, `get`.
    pub op: String,
    /// The file or table as the code names it: a literal path or pattern (`out.json`,
    /// `{name}.png`), `*.csv` when only the format is known, a table after a SQL verb. Empty
    /// when only a variable says which.
    pub artifact: String,
    /// The file a database connection opens, where the code says so: `duckdb.connect(path)`. A
    /// database that is no file (in memory, at an address) names none.
    pub file: String,
    /// The code alone decides where, in its own function: the path or URL starts at a literal
    /// (`"out.csv"`, `Path("data") / name`), not at a value a caller hands in (`root / "out.csv"`).
    pub fixed: bool,
    /// The functions whose own code decides where, where that is not its own function: each hands
    /// it a path or URL that starts at a literal (`read_rows(Path("data/rows.csv"))`).
    pub fixed_by: Vec<String>,
    /// The line of the first such call, 1-based.
    pub line: u32,
    /// The functions whose calls hand what makes its path or statement whole, where that is not
    /// its own function: a route shows it when it reaches one of them (`a&b`: both).
    pub by: Vec<String>,
    /// The entries whose users hand what makes it whole: their own routes show it.
    pub users_of: Vec<String>,
    /// What the call gives goes to nothing its function hands on: it only decides a choice, goes
    /// to an error, or is dropped. A read that checks what is there, not one that uses it.
    pub checks: bool,
}

impl Touch {
    /// A touch of `kind`, going `dir` when the call says, with nothing else known yet.
    pub fn new(kind: World, dir: Option<Dir>) -> Touch {
        Touch {
            kind,
            dir,
            what: String::new(),
            n: 1,
            payload: String::new(),
            op: String::new(),
            artifact: String::new(),
            file: String::new(),
            fixed: false,
            fixed_by: vec![],
            line: 0,
            by: vec![],
            users_of: vec![],
            checks: false,
        }
    }

    /// Whether the route that starts at `entry` and reaches `reached` runs this touch: one whole
    /// in its own function always, one whole by what callers hand when the route reaches one of
    /// them, one whole by what an entry's users hand in that entry's route.
    pub fn in_route(&self, entry: &str, reached: &dyn Fn(&str) -> bool) -> bool {
        (self.by.is_empty() && self.users_of.is_empty())
            || self.by.iter().any(|f| f.split('&').all(reached))
            || self.users_of.iter().any(|e| e == entry)
    }

    pub fn crossing(&self) -> Crossing {
        Crossing { world: self.kind, dir: self.dir }
    }

    /// The kind as the graph's JSON names it: `disk_read`, `net`, `db_sqlite`.
    pub fn kind_name(&self) -> &'static str {
        self.crossing().kind_name()
    }

    /// The direction as the graph's JSON writes it beside the kind.
    pub fn dir_name(&self) -> &'static str {
        self.crossing().dir_name()
    }
}

/// A touch as the graph's JSON writes it.
#[derive(Serialize, Deserialize, TS, Clone, Debug, Default, PartialEq, Eq)]
pub struct TouchJson {
    pub kind: String,
    pub what: String,
    pub n: u32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub payload: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub op: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub artifact: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub file: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub dir: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub fixed: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fixed_by: Vec<String>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub line: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub by: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub users_of: Vec<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub checks: bool,
}

impl From<Touch> for TouchJson {
    fn from(t: Touch) -> TouchJson {
        let (kind, dir) = (t.kind_name().to_string(), t.dir_name().to_string());
        TouchJson {
            kind,
            what: t.what,
            n: t.n,
            payload: t.payload,
            op: t.op,
            artifact: t.artifact,
            file: t.file,
            dir,
            fixed: t.fixed,
            fixed_by: t.fixed_by,
            line: t.line,
            by: t.by,
            users_of: t.users_of,
            checks: t.checks,
        }
    }
}

impl TryFrom<TouchJson> for Touch {
    type Error = String;
    fn try_from(j: TouchJson) -> Result<Touch, String> {
        let c = Crossing::named(&j.kind, &j.dir).ok_or_else(|| format!("no part of the world called `{}`", j.kind))?;
        Ok(Touch {
            kind: c.world,
            dir: c.dir,
            what: j.what,
            n: j.n,
            payload: j.payload,
            op: j.op,
            artifact: j.artifact,
            file: j.file,
            fixed: j.fixed,
            fixed_by: j.fixed_by,
            line: j.line,
            by: j.by,
            users_of: j.users_of,
            checks: j.checks,
        })
    }
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

/// Where a body compares one value with its cases: `args.mode == "train"`,
/// `mode in {"a", "b"}`, `mode is Mode.FAST`, `case "a":`, `"a" =>`.
#[derive(Serialize, Deserialize, TS, Clone, Debug, Default, PartialEq, Eq)]
#[ts(export)]
pub struct Compare {
    /// The value tested, as the body was handed it: `args.experiment`. A name the body binds once
    /// to a value it reads in place is that value: after `m = args.mode`, `m` is `args.mode`.
    pub subject: String,
    /// The subject as the comparison writes it, where a name stands for it: `m`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub written: String,
    /// The literals it is compared with.
    pub cases: Vec<String>,
    /// The members of a type it is compared with, as written: `Mode.FAST`, `Mode::Fast`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub members: Vec<String>,
    pub line: u32,
    /// The choice it decides, numbered like `Edge::arms`; a comparison that only computes a
    /// value (`weighted = mode in {..}`) decides none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choice: Option<u32>,
    /// The subject is the value the method runs on, or is read through it: `self.mode`,
    /// `this.mode`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub receiver: bool,
}

impl Compare {
    /// Each case as the code writes it: a literal quoted (`"fast"`), a member as it stands
    /// (`Mode.FAST`).
    pub fn named(&self) -> Vec<String> {
        self.cases.iter().map(|c| format!("\"{c}\"")).chain(self.members.iter().cloned()).collect()
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
    /// The unit it stands in: its Makefile.
    #[serde(default)]
    pub module: String,
    /// The part of the repository its Makefile belongs to.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub part: String,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, Default)]
#[ts(export)]
pub struct Stats {
    pub modules: usize,
    pub nodes: BTreeMap<String, usize>,
    pub edges: BTreeMap<String, usize>,
    pub unresolved_calls_total: u32,
    pub unresolved_calls_top: Vec<(String, u32)>,
    /// The same calls by what the name each starts from is bound to.
    #[serde(default)]
    pub unresolved_calls: Unresolved,
}

impl Stats {
    /// The share of the calls not followed that the engine should have followed.
    pub fn unknown_share(&self) -> f64 {
        if self.unresolved_calls_total == 0 {
            return 0.0;
        }
        f64::from(self.unresolved_calls.unknown) / f64::from(self.unresolved_calls_total)
    }
}

/// The calls a reader could not follow, by what the name each starts from is bound to, as the
/// reader knows it at the call. Only `unknown` counts against the engine.
#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[ts(export)]
pub struct Unresolved {
    /// The language's own, which nothing in the program binds: `len`, `Some`, `console`.
    pub builtin: u32,
    /// Imported from a library, or holding a value a library made: `click.echo`, `logger.info`.
    pub library: u32,
    /// Bound in the body: a parameter, the receiver, a local.
    pub local: u32,
    /// The rest: one of the program's own names that did not resolve, a name bound nowhere that
    /// is not the language's own, a call on a value an expression gives.
    pub unknown: u32,
}

/// What the name a call not followed starts from is bound to: one of `Unresolved`'s counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bucket {
    Builtin,
    Library,
    Local,
    Unknown,
}

impl Unresolved {
    /// One more call in bucket `b`.
    pub fn count(&mut self, b: Bucket) {
        *match b {
            Bucket::Builtin => &mut self.builtin,
            Bucket::Library => &mut self.library,
            Bucket::Local => &mut self.local,
            Bucket::Unknown => &mut self.unknown,
        } += 1;
    }
}

/// The calls of graphs read side by side.
impl std::ops::AddAssign for Unresolved {
    fn add_assign(&mut self, o: Unresolved) {
        self.builtin += o.builtin;
        self.library += o.library;
        self.local += o.local;
        self.unknown += o.unknown;
    }
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
    /// What its language leaves to the author.
    #[serde(default)]
    pub leaves: Leaves,
    /// Its language compares types by their members: a value stands where a type is asked for when
    /// it has every member that type declares, whatever it is declared as (TypeScript).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub structural: bool,
}

/// What a language leaves its author to write: what its compiler neither asks for nor works out,
/// so that code may leave it out and a reader then does not know it.
#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[ts(export)]
pub struct Leaves {
    /// What a package offers: whatever its root binds, unless the author names it.
    pub exports: bool,
    /// The types of a signature: it may say none, and nothing works them out.
    pub types: bool,
    /// What a function gives back, as its declaration reads: left out, or worked out where a
    /// reader of the declaration does not see it.
    pub returns: bool,
    /// That an override keeps the promise of the method it overrides: no compiler holds it to it,
    /// or one holds it only to part of it.
    #[serde(default)]
    pub overrides: bool,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, Default)]
#[ts(export)]
pub struct Graph {
    pub root: String,
    /// The language the repo was parsed as: `python`, `rust`, `typescript`; empty when its
    /// parts are read in several.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub language: String,
    /// The directory that holds the repo's own manifests: Makefile, pyproject.toml, Cargo.toml.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub project_root: String,
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

impl Node {
    /// What the route that starts at `entry` and reaches `reached` touches here: the touches it
    /// runs, and a call none of whose touches it runs as it was read.
    pub fn touches_in(&self, entry: &str, reached: &dyn Fn(&str) -> bool) -> Vec<&Touch> {
        let runs: Vec<&Touch> = self.touches.iter().filter(|t| t.in_route(entry, reached)).collect();
        let same_call = |a: &Touch, b: &Touch| a.line == b.line && a.op == b.op && a.kind == b.kind;
        let read: Vec<&Touch> = self.instead.iter().filter(|t| !runs.iter().any(|o| same_call(o, t))).collect();
        runs.into_iter().chain(read).collect()
    }
}

impl Graph {
    pub fn node(&self, id: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// What the route that starts at `start` can run: the functions it calls, the classes it
    /// builds with their methods, and the handlers it requests, through every step.
    pub fn reach(&self, start: &str) -> std::collections::HashSet<String> {
        let classes: std::collections::HashSet<&str> =
            self.nodes.iter().filter(|n| n.kind == NodeKind::Class).map(|n| n.id.as_str()).collect();
        let mut next: std::collections::HashMap<&str, Vec<&str>> = std::collections::HashMap::new();
        for e in &self.edges {
            if matches!(e.kind, EdgeKind::Calls | EdgeKind::Instantiates | EdgeKind::Requests)
                || (e.kind == EdgeKind::Contains && classes.contains(e.src.as_str()))
            {
                next.entry(e.src.as_str()).or_default().push(e.dst.as_str());
            }
        }
        let mut seen = std::collections::HashSet::from([start.to_string()]);
        let mut stack = vec![start];
        while let Some(id) = stack.pop() {
            for d in next.get(id).into_iter().flatten() {
                if seen.insert(d.to_string()) {
                    stack.push(d);
                }
            }
        }
        seen
    }

    /// Nodes by id, in insertion order.
    pub fn index(&self) -> IndexMap<&str, &Node> {
        self.nodes.iter().map(|n| (n.id.as_str(), n)).collect()
    }

    /// Every id the graph holds, mapped by `to`: each node's own, and each that names a node (its
    /// module, class and part, the method whose promise it breaks, what its prophecy replaces or may
    /// replace, what its calls reach and the types they hand and give, whose calls make its touches
    /// whole and whose code decides where they go), an edge's ends, a make target's own with its
    /// Makefile, its part and what it runs, the parts and the root.
    pub fn rename(&mut self, to: &dyn Fn(&str) -> String) {
        let m = |s: &mut String| {
            if !s.is_empty() {
                *s = to(s)
            }
        };
        m(&mut self.root);
        self.parts.iter_mut().for_each(|p| m(&mut p.id));
        for n in &mut self.nodes {
            m(&mut n.id);
            m(&mut n.module);
            m(&mut n.part);
            n.cls.iter_mut().chain(n.breaks.iter_mut()).for_each(&m);
            if let Some(p) = n.prophecy.as_mut() {
                p.supersedes.iter_mut().chain(p.replaces.iter_mut()).chain(p.candidates.iter_mut()).for_each(&m);
            }
            for s in &mut n.sites {
                m(&mut s.to);
                m(&mut s.on);
                s.hands.iter_mut().chain(s.gives.iter_mut()).for_each(|h| m(&mut h.ty));
            }
            for t in n.touches.iter_mut().chain(n.instead.iter_mut()) {
                for f in &mut t.by {
                    *f = f.split('&').map(to).collect::<Vec<_>>().join("&");
                }
                t.users_of.iter_mut().chain(t.fixed_by.iter_mut()).for_each(&m);
            }
        }
        for e in &mut self.edges {
            m(&mut e.src);
            m(&mut e.dst);
        }
        for t in &mut self.make {
            m(&mut t.id);
            m(&mut t.module);
            m(&mut t.part);
            t.calls.iter_mut().for_each(&m);
        }
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

/// The method resolution order of `class`: the class, then its ancestors nearest first, linearized
/// by C3 from the bases each class declares, in the order it declares them (Python's order). A class
/// comes before its bases, and its bases keep the order they are declared in. Where declared orders
/// contradict one another, as TypeScript's `implements A, B` with `B extends A` may, a class still
/// comes before its ancestors, and failing that the base declared first comes first. A class
/// reached again through a cycle of bases adds nothing. `known` keeps the order of every class
/// worked out on the way.
pub fn linearization<'a>(
    class: &'a str,
    bases: &dyn Fn(&'a str) -> Vec<&'a str>,
    known: &mut HashMap<&'a str, Vec<&'a str>>,
) -> Vec<&'a str> {
    fn order<'a>(
        class: &'a str,
        bases: &dyn Fn(&'a str) -> Vec<&'a str>,
        known: &mut HashMap<&'a str, Vec<&'a str>>,
        open: &mut Vec<&'a str>,
    ) -> Vec<&'a str> {
        if let Some(l) = known.get(class) {
            return l.clone();
        }
        if open.contains(&class) {
            return vec![];
        }
        open.push(class);
        let mut declared: Vec<&'a str> = vec![];
        for b in bases(class) {
            if b != class && !declared.contains(&b) {
                declared.push(b);
            }
        }
        // each base's own order, and the declared order last
        let mut seqs: Vec<(bool, VecDeque<&'a str>)> = declared.iter().map(|b| (false, order(b, bases, known, open).into())).collect();
        seqs.push((true, declared.into()));
        let mut out = vec![class];
        loop {
            seqs.retain(|(_, s)| !s.is_empty());
            let Some(first) = seqs.first().map(|(_, s)| s[0]) else { break };
            let later = |h: &str, all: bool| seqs.iter().filter(|(d, _)| all || !d).any(|(_, s)| s.iter().skip(1).any(|x| *x == h));
            let heads = || seqs.iter().map(|(_, s)| s[0]);
            let next = heads().find(|h| !later(h, true)).or_else(|| heads().find(|h| !later(h, false))).unwrap_or(first);
            for (_, s) in &mut seqs {
                s.retain(|x| *x != next);
            }
            if !out.contains(&next) {
                out.push(next);
            }
        }
        open.pop();
        known.insert(class, out.clone());
        out
    }
    order(class, bases, known, &mut vec![])
}

/// What building a value of a class runs: the initializer of the first class along its method
/// resolution order (`order`, as `linearization` gives it) that defines one, its own before a
/// base's. A call of the class hands it its arguments, and `super(..)` in a constructor hands its
/// own to the one the base runs.
pub fn initializer_along<'a>(order: &[&str], node: impl Fn(&str) -> Option<&'a Node> + Copy) -> Option<String> {
    order.iter().filter_map(|c| node(c)).filter(|n| n.kind == NodeKind::Class).find_map(|n| n.initializer(node))
}

pub fn kind_name(k: &NodeKind) -> String {
    serde_json::to_value(k).unwrap().as_str().unwrap().to_string()
}

pub fn edge_name(k: &EdgeKind) -> String {
    serde_json::to_value(k).unwrap().as_str().unwrap().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The method resolution order of `class` in a hierarchy written as `(class, [bases])`.
    fn mro<'a>(class: &'a str, hierarchy: &'a [(&'a str, &'a [&'a str])]) -> Vec<&'a str> {
        let bases = |c: &'a str| hierarchy.iter().find(|(k, _)| *k == c).map_or(vec![], |(_, bs)| bs.to_vec());
        linearization(class, &bases, &mut HashMap::new())
    }

    /// Single inheritance is the chain of bases.
    #[test]
    fn a_chain_is_its_own_order() {
        assert_eq!(mro("C", &[("C", &["B"]), ("B", &["A"])]), ["C", "B", "A"]);
    }

    /// A class's bases keep the order it declares them in, each followed by its own bases unless a
    /// later base shares them: `D(B, C)` with `B(A)` and `C(X)` runs `B`'s `A` before `C`, and in a
    /// diamond `D(B, C)` over `A`, `A` comes last.
    #[test]
    fn bases_keep_their_declared_order_and_a_shared_base_comes_last() {
        assert_eq!(mro("D", &[("D", &["B", "C"]), ("B", &["A"]), ("C", &["X"])]), ["D", "B", "A", "C", "X"]);
        assert_eq!(mro("D", &[("D", &["B", "C"]), ("B", &["A"]), ("C", &["A"])]), ["D", "B", "C", "A"]);
    }

    /// The order of the example Python's description of its MRO gives: `A(B, C)`, `B(D, E)`,
    /// `C(D, F)`, and each of `D`, `E`, `F` over `O`.
    #[test]
    fn the_order_is_pythons() {
        let h: &[(&str, &[&str])] =
            &[("A", &["B", "C"]), ("B", &["D", "E"]), ("C", &["D", "F"]), ("D", &["O"]), ("E", &["O"]), ("F", &["O"])];
        assert_eq!(mro("A", h), ["A", "B", "C", "D", "E", "F", "O"]);
    }

    /// Declared orders that contradict one another still put each class before its ancestors:
    /// `implements A, B` where `B extends A`. A cycle of bases ends.
    #[test]
    fn a_contradiction_keeps_a_class_before_its_ancestors_and_a_cycle_ends() {
        assert_eq!(mro("K", &[("K", &["A", "B"]), ("B", &["A"])]), ["K", "B", "A"]);
        assert_eq!(mro("A", &[("A", &["B"]), ("B", &["A"])]), ["A", "B"]);
    }
}
