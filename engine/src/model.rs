//! The one central type: a typed graph of a repository.
//!
//! Everything between `parse` and `layout` is `Graph -> Graph`. The JSON shape is the
//! contract with the frontend; `ts-rs` generates the TypeScript side from these types.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub use crate::lang::Lang;

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

/// An id is in the coproduct `Code ⊔ Beyond` (`mathematics.md`, "Lenses"): what the reading holds as code (the program,
/// and a library read as code: `plotkit.data.load`), or a name past the fiber the graph keeps of a library, an
/// `External` node with no body. The second injection is tagged, and the tag with its partial inverse is a prism:
/// `path(&of(p)) == Some(p)`, `path(id) == None` for an id of the code's. Its spelling stands here and nowhere else.
pub mod ext {
    const TAG: &str = "ext:";

    /// The id of a name past the code: `plotkit.data.load` is `ext:plotkit.data.load`.
    pub fn of(path: &str) -> String {
        format!("{TAG}{path}")
    }

    /// The name past the code an id is; None for an id of the code's.
    pub fn path(id: &str) -> Option<&str> {
        id.strip_prefix(TAG)
    }

    /// Whether an id is a name past the code.
    pub fn is(id: &str) -> bool {
        path(id).is_some()
    }

    /// The dotted name an id says, whichever side it is on: its path past the code, or the id itself.
    pub fn name(id: &str) -> &str {
        path(id).unwrap_or(id)
    }
}

/// What a reader resolves a name to, a value of `Code ⊔ Beyond` (`ext`): one of the code's ids, which carries no tag, or a
/// name past the code.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Resolved {
    Code(String),
    Beyond(String),
}

impl Resolved {
    /// What an id is, by its injection.
    pub fn of_id(id: String) -> Resolved {
        match ext::path(&id) {
            Some(p) => Resolved::Beyond(p.to_string()),
            None => Resolved::Code(id),
        }
    }

    /// The id the graph knows it by.
    pub fn id(self) -> String {
        match self {
            Resolved::Code(id) => id,
            Resolved::Beyond(p) => ext::of(&p),
        }
    }

    /// The code's id, where it names the code.
    pub fn code(self) -> Option<String> {
        match self {
            Resolved::Code(id) => Some(id),
            Resolved::Beyond(_) => None,
        }
    }

    /// The name past the code, where it names one.
    pub fn beyond(self) -> Option<String> {
        match self {
            Resolved::Code(_) => None,
            Resolved::Beyond(p) => Some(p),
        }
    }

    pub fn is_beyond(&self) -> bool {
        matches!(self, Resolved::Beyond(_))
    }
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
    /// For a route: the requests it serves, of which the label is the words. None where the route
    /// serves no request a program sends (a page's route in the browser).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<Route>,
}

impl Entry {
    pub fn new(kind: EntryKind, label: impl Into<String>) -> Entry {
        Entry { kind, label: label.into(), options: vec![], request: None }
    }

    /// A route that serves `route`, labelled by it.
    pub fn route(route: Route) -> Entry {
        let mut e = Entry::new(EntryKind::Route, String::new());
        e.serve(route);
        e
    }

    /// Serve `route`: the label is its words, written by `Route`'s `Display` alone.
    pub fn serve(&mut self, route: Route) {
        self.label = route.to_string();
        self.request = Some(route);
    }
}

/// A method a route serves or a request goes by, upper case as the protocol writes it (`GET`,
/// `WEBSOCKET`). Methods go in the order of the table of methods, `knowledge::REQUEST_WAYS`,
/// which orders them (`Ord`, beside the table).
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq, Eq, Hash)]
#[serde(transparent)]
#[ts(export)]
pub struct Method(String);

impl Method {
    pub fn new(name: &str) -> Method {
        Method(name.to_ascii_uppercase())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Method {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The methods a route serves: a set, so the order the code lists them in says nothing
/// (`["POST", "GET"]`, `{"GET", "POST"}`). Written `GET|POST`, in the order of its methods.
#[derive(Serialize, Deserialize, TS, Clone, Debug, Default, PartialEq, Eq, Hash)]
#[serde(transparent)]
#[ts(export)]
pub struct Methods(pub BTreeSet<Method>);

impl Methods {
    /// The methods of these names, in any case.
    pub fn of<S: AsRef<str>>(names: impl IntoIterator<Item = S>) -> Methods {
        Methods(names.into_iter().map(|n| Method::new(n.as_ref())).collect())
    }

    pub fn contains(&self, m: &Method) -> bool {
        self.0.contains(m)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Method> {
        self.0.iter()
    }
}

impl std::fmt::Display for Methods {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let names: Vec<&str> = self.0.iter().map(Method::as_str).collect();
        f.write_str(&names.join("|"))
    }
}

/// The requests a route serves: its methods at its path. No path where the code names no
/// literal one. Written `GET|POST /items`, or the methods alone without a path.
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq, Eq, Hash)]
#[ts(export)]
pub struct Route {
    pub methods: Methods,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

impl std::fmt::Display for Route {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match &self.path {
            Some(p) => write!(f, "{} {p}", self.methods),
            None => write!(f, "{}", self.methods),
        }
    }
}

/// A request a function makes whole: its method where the code says it, and its path. Written
/// `GET /api/items/{id}`, or the path alone where the code does not say the method, and read
/// back where a rendered request is split (`From<String>`, beside `paths::method_and_path`).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
#[serde(into = "String", from = "String")]
pub struct Request {
    pub method: Option<Method>,
    pub path: String,
}

impl std::fmt::Display for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match &self.method {
            Some(m) => write!(f, "{m} {}", self.path),
            None => f.write_str(&self.path),
        }
    }
}

impl From<Request> for String {
    fn from(r: Request) -> String {
        r.to_string()
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
    /// For a class: the class of the program each of its fields holds, by the field's name, as its declarations and its
    /// methods' assignments say (`self.left = left` with `left: Chart`): what a value of it is reached through, field by
    /// field (`board.left` is a `Chart`). The `has` edges say which classes it holds; this says through which field.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(type = "Record<string, string>")]
    pub holds: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub decorators: Vec<String>,
    /// Sketch code kept in the tree, not built yet: what its marker says. Read by the language's reader.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prophecy: Option<Prophecy>,
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
    #[ts(type = "Array<string>")]
    pub asks: Vec<Request>,
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
    /// The error its body's own code hands on outside its steps, as its signature converts it, `""`
    /// for one of no type of the code's: a `?` written in no step (`Ok(fs::read_to_string(p)?.lines()..)`).
    /// What a step hands on, the step's site says (`Site::fails`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fails: Option<String>,
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
    /// What the body's own code uses outside the calls that are its steps: each name it reads in a
    /// condition, an operator, a format, a return or an argument of a call that is no step
    /// (`print(store.path)`, `if done:`, `Ok(path)`), once in each arm it is read in. A projection of a
    /// name uses that name; a name bound to a part of another (`width = settings.width`) uses it where
    /// the body reads that name. The instance a method runs on is used by no name, as a call is handed
    /// it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub uses: Vec<Use>,
    /// The names the arms of its body's choices bind by the body's own code, no step of it: what an
    /// arm sets a name to (`let unit; if .. { unit = KELVIN }`), and what an arm of a choice a name is
    /// bound to gives where no step makes it (`let out = match f { "csv" => String::from(..), _ =>
    /// format!(..) }`, `x = a if c else b`). Each name once in each arm, with its type when known and
    /// the arms the value is made in. Such an arm makes the name's value as an arm whose step gives it
    /// does. Outside the choices, with no arms, the names it binds to a value no step makes: a literal
    /// (`let mut held = vec![]`, `items = []`), which the body makes as a call handed nothing makes it
    /// (`Vec::new()`, `list()`), or any other expression (`mask = samples.abs() > 0`, `let total = a +
    /// b`, `const shown = n > 0`), whatever it reads. Each is the body's own name, never a module's
    /// (#318). What the language's own code computes of names with no call (`let total = a + b`, `both
    /// = [first, second]`) is no value of its own: the name stands for those names (`body::stands_for`),
    /// so a step handed it is handed what they hold, as one handed `a + b` written in place is (#328).
    /// A name bound to another, to what a step gives or to a part of either (`width = settings.width`,
    /// `g = &figures().clean`) holds that value, and a function written in place is a function: none of
    /// these.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub binds: Vec<Use>,
    /// What its body's own code sets a part of a value to, by assignment through the name that holds
    /// the value (`other.fig = self.fig`, `rows[i] = row`): the value changed and what the assigned
    /// expression reads, a lens's put made in place.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sets: Vec<Set>,
    /// The lines each arm of its body's choices spans, first and last: what is written there runs
    /// only in that arm.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(type = "Array<[number, number]>")]
    pub arm_lines: Vec<(u32, u32)>,
    /// The choice each of `arm_lines` is an arm of, numbered as `Site::arms` numbers them: two arms
    /// of one choice never run in one run of the body.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub arm_choices: Vec<u32>,
    /// The lines each loop of its body spans, first and last: what runs there may run again after
    /// what follows it in the loop.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(type = "Array<[number, number]>")]
    pub loop_lines: Vec<(u32, u32)>,
    /// What decides each arm of its body's choices, as the code writes it: `(choice, arm, when)`,
    /// with the choices and arms numbered as `Site::arms` numbers them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(type = "Array<[number, number, string]>")]
    pub whens: Vec<(u32, u32, String)>,
    /// The fields the conditions that decide its body's choices read of the values they read, as the code
    /// names them: `ax` of the instance for `if self.ax is None:` (#184).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tests: Vec<Test>,
}

/// A field a condition reads (`Node::tests`): the choice it decides, numbered as `Site::arms` numbers them,
/// the value it reads the field of, as a call is handed it (the instance a method runs on by no name), and the
/// field, through the fields it is reached by (`left.ax` of `self.left.ax is None`), an element left out. A
/// condition that reads a value whole, or calls a method on it, reads no field of it.
#[derive(Serialize, Deserialize, TS, Clone, Debug, Default, PartialEq)]
#[ts(export)]
pub struct Test {
    pub choice: u32,
    pub on: Held,
    pub field: String,
}

impl Node {
    /// For a class: what a call to it runs, among its methods: its initializer.
    pub fn initializer<'a>(&self, node: impl Fn(&str) -> Option<&'a Node>) -> Option<String> {
        self.methods.iter().map(|m| format!("{}.{m}", self.id)).find(|id| node(id).map_or(false, |m| m.ctor && !m.on_class))
    }
}

/// What a prophecy says of itself: sketch code kept in the tree, marked as not built yet. `prophecy.rs`
/// reads it from the marker and settles it in the whole graph.
#[derive(Serialize, Deserialize, TS, Clone, Debug, Default, PartialEq, Eq)]
#[ts(export)]
pub struct Prophecy {
    /// The existing thing the prophecy replaces: it stays until the prophecy is built.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<String>,
    /// The place is known and the shape is not.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub veiled: bool,
    /// The shape waits on an open question, as an issue (`#144`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sealed: Option<String>,
    /// The node `supersedes` names, once the graph is whole: its id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replaces: Option<String>,
    /// The nodes `supersedes` names when it names more than one: which it replaces, it does not say.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<String>,
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
    /// The types its type holds as values, as its language reads the type, each by the last part of
    /// its name: `dict[Mode, Plan]` holds a `Plan` by its `Mode`, `Option<Vec<Node>>` a `Node`,
    /// `VectorCache[TVectorizer]` a `VectorCache`. A name written through an alias is held by the name
    /// of the type it stands for (`Order as Parcel` holds an `Order`), once the reader resolves it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub holds: Vec<String>,
}

impl Param {
    /// Whether the signature says what it is: a type is written, and it is not the unchecked one.
    pub fn says(&self) -> bool {
        !self.ty.is_empty() && !self.unchecked
    }

    /// Whether its type holds a value of the type named `name` (the last part of its name).
    pub fn holds_type(&self, name: &str) -> bool {
        self.holds.iter().any(|h| h == name)
    }

    /// What a type holds as values, as `type_names` reads it, by the last part of each name.
    pub fn held(names: Vec<Vec<String>>) -> Vec<String> {
        let mut out: Vec<String> = vec![];
        for n in names.into_iter().filter_map(|n| n.last().cloned()) {
            if !out.contains(&n) {
                out.push(n);
            }
        }
        out
    }
}

/// A type's own name: its id without the library or module it is in (`ext:pandas.DataFrame` is `DataFrame`). A
/// shaped array keeps its shape, a dot in it too: `ext:jaxtyping.Float[Tensor, batch {self.size}]` is
/// `Float[Tensor, batch {self.size}]`.
pub fn type_name(id: &str) -> &str {
    let s = ext::name(id);
    let head = &s[..s.find('[').unwrap_or(s.len())];
    &s[head.rfind('.').map_or(0, |i| i + 1)..]
}

/// The names of `held`, each as the last part of the type it resolves to: a type written through an
/// alias (`Order as Parcel`, `use tree_sitter::Node as TsNode`) is the type it stands for, held by that
/// type's name, as the `takes` it resolves to names it. A name that resolves to nothing stays as written.
pub fn resolve_held(held: &[String], resolve: &impl Fn(&str) -> Option<String>) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for h in held {
        let name = resolve(h).map_or(h.clone(), |id| type_name(&id).to_string());
        if !out.contains(&name) {
            out.push(name);
        }
    }
    out
}

/// What the parameters and the result of `n` hold, each name resolved by `resolve` (`resolve_held`);
/// `set_holds` puts them back once the reader no longer borrows the graph to resolve them.
pub fn holds_resolved(n: &Node, resolve: impl Fn(&str) -> Option<String>) -> (Vec<Vec<String>>, Option<Vec<String>>) {
    (n.params.iter().map(|p| resolve_held(&p.holds, &resolve)).collect(), n.returns.as_ref().map(|r| resolve_held(&r.holds, &resolve)))
}

pub fn set_holds(n: &mut Node, (params, returns): (Vec<Vec<String>>, Option<Vec<String>>)) {
    for (p, h) in n.params.iter_mut().zip(params) {
        p.holds = h;
    }
    if let (Some(r), Some(h)) = (n.returns.as_mut(), returns) {
        r.holds = h;
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
    /// The types the written type holds as values, as its language reads the type (`Param::holds`), each by the last part
    /// of its name: `Result<Vec<Node>, Error>` holds a `Node` and an `Error`; `Vec<(String, Vec<Where>)>` holds pairs, of
    /// a type the code does not name, and no `Where`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub holds: Vec<String>,
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
    /// The body throws away what the call gives back: the call is the whole of a statement
    /// (`other.take_from(board)`, `void remove(..)`, `store.add(x);`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub dropped: bool,
    /// What the call gives leaves the body as an error: it builds the error a `raise`, a `throw`
    /// or a `return Err(..)` ends the body with.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub raises: bool,
    /// The errors the call hands on, outside a handler that catches them: each error what it runs
    /// can end with, by its type, `""` where it is no type of the code's. A call into a family hands
    /// on what any of the overrides it may run can end with; a Rust `?` the one error its signature
    /// converts to. Sorted, so that a reading is a function of the code.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fails: Vec<String>,
    /// The call hands on what its callee's body raises, as a `raise` propagates in its language: read
    /// where the call stands, and `fails` is settled from what the callee's body ends with once the
    /// graph is whole. A call `?` hands on fails as the reader reads it.
    #[serde(skip)]
    pub handed_on: bool,
    /// The handlers that watch the call (the body of a `try` it stands in), innermost first, each by
    /// the classes it catches as the code names them, none for one that catches whatever is raised.
    /// `fails` is settled from them once the graph is whole.
    #[serde(skip)]
    pub caught: Vec<Vec<String>>,
    /// The class of the value a method is called on, when the reader knows it: the class the code declares the value
    /// of, where that is a subclass of the method's own class (`plot: ComparePlot`, `plot.plot()` reaches `BasePlot.plot`
    /// on a `ComparePlot` or a class below it), or the class it builds the value of at the call (`built`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub on: String,
    /// The value the call runs on is built where the call stands (`ComparePlot(..).plot()`, `new Section().show()`):
    /// its class is `on`, also where that is the method's own class, and none below it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub built: bool,
    /// The call is made on the body's own receiver (`self.build(..)`): on a value of a
    /// subclass, it reaches that subclass's method.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub on_receiver: bool,
    /// The call is made through a class (`Cached.read(self)`): it runs the method that class's
    /// order finds, and no override of it, whatever the class of the value it runs on.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub named: bool,
    /// The class a `super()` call is written in (`super().read()` in `Buffered`): the call runs
    /// the next method after it along the order of the value's class.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub after: String,
    /// The values the call is handed: its arguments and the value it is called on, each with the
    /// parameters it fills (`Held::fills`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hands: Vec<Held>,
    /// The parameters the call writes an argument for, as the call writes them, in the order written:
    /// the value it runs on, each argument by its place and by its name. A parameter none of them
    /// fills holds its default; one an argument fills that hands no value holds what no step makes
    /// (a literal, a constant).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<Slot>,
    /// Where each of `args` is written, as a byte range of its file.
    #[serde(skip)]
    pub written: Vec<(u32, u32)>,
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
    /// The call stands in a function written in place in the body (`|row| ..`, `lambda r: ..`,
    /// `(r) => ..`): what it gives under a name that function binds is the function's own.
    #[serde(skip)]
    pub in_function: bool,
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
    /// Where the value comes from when the name written at `at` holds it through a binding of a projection of it that the
    /// reader follows (`let t = &self.title`): the byte offset of the name that projection starts from (`self`), so that the
    /// fields the name is bound to can be read where it is used (`Steps::fills`, `Set::of`). None (0) where `at` says it.
    #[serde(skip)]
    pub bound: u32,
    /// For a value a call is handed: the parameters it fills, each argument of the call it is written
    /// in (`Site::args`). `f(x.title, x.lines)` hands `x` for the first two; `f(g(y))` hands what the
    /// step `g` gives for the first, and `y` to `g`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fills: Vec<Slot>,
    /// Of the parameters it fills, those it fills through a field of it, each with that field as the code names it,
    /// through the fields it is reached by, as `Set::of` names one: `title` for the first parameter of
    /// `other.take(self.title, self.ax)` and `ax` for the second, `left` for the value `self.left.line(..)` runs on. The
    /// call is handed the value, the projection glued into it (`take ∘ π_title`, whose domain is the value's type); the
    /// field says which part of it fills the parameter. A parameter it fills whole, or through what the call computes of
    /// it (`f(x.n + 1)`), is none of them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(type = "Array<[Slot, string]>")]
    pub through: Vec<(Slot, String)>,
    /// The value is one of the body's own parameters of no type of the code's, handed or read by its name, or what a step
    /// gives of such parameters alone (a loop's element): it holds a value only where the function's calls fill them with
    /// one, settled once every body is read (`builder::settle_parameters`).
    #[serde(skip)]
    pub param: bool,
}

impl Held {
    pub fn new(ty: impl Into<String>, name: impl Into<String>) -> Held {
        Held { ty: ty.into(), name: name.into(), ..Held::default() }
    }

    /// The value as one of the body's code holds it, written at byte `at`: no parameter of a call filled yet.
    pub fn at(ty: impl Into<String>, name: impl Into<String>, at: usize, param: bool) -> Held {
        Held { at: at as u32, param, ..Held::new(ty, name) }
    }
}

/// Which parameter of the function a call reaches an argument fills, as the call writes it. Inside the
/// callee the parameter holds what the argument hands (`mathematics.md`, "One fine plan"): a call
/// pairs its values with the parameters by where it writes them, not by their types.
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Slot {
    /// The value the call runs on: `x` of `x.f(..)`, `Trend::new(..)` of `Trend::new(..).save(..)`, and
    /// the first argument of a method called through its class or path (`Cached.read(self)`,
    /// `Trend::save(&t, ..)`, `Source.prototype.read.call(v)`).
    Receiver,
    /// The parameter after the receiver at this place, the first being 0: the argument written there
    /// by place.
    At(u32),
    /// The parameter of this name: a keyword argument (`f(title=t)`), a field a literal of a struct or
    /// an element writes (`Trend { settings }`, `<Chart rows={rows} />`).
    Name(String),
    /// What a spread hands (`f(*xs)`, `f(...xs)`) and what any argument by place after it hands: the
    /// parameters from this place on, of which the call does not say which.
    Spread(u32),
    /// What a spread of names hands (`f(**options)`, `Settings { ..base }`): the parameters the call
    /// writes no argument for by name.
    Keywords,
}

/// A value a body's own code reads (`Node::uses`) or an arm binds (`Node::binds`), by the name that
/// holds it (none for the instance a method runs on), with its type when known and the arms of the
/// choices the use or the binding sits in, numbered as `Site::arms` numbers them.
#[derive(Serialize, Deserialize, TS, Clone, Debug, Default, PartialEq, Eq)]
#[ts(export)]
pub struct Use {
    pub name: String,
    #[serde(rename = "type", default, skip_serializing_if = "String::is_empty")]
    pub ty: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(type = "Array<[number, number]>")]
    pub arms: Vec<(u32, u32)>,
    /// The name is one of the body's own parameters of no type of the code's (`Held::param`).
    #[serde(skip)]
    pub param: bool,
}

/// A part of a value the body's own code sets (`Node::sets`), one for each part a binding sets: the value,
/// as a call is handed it (`other` of `other.fig = self.fig`, the instance a method runs on by no name for
/// `self.n = ..`), the field of it that is set, what the assigned expression reads (the instance of
/// `self.fig`) and the field of it that the expression is, the lines the binding spans, and the arms of
/// the choices the setting sits in, numbered as `Site::arms` numbers them. A binding that sets two parts
/// at once sets each to the part of the value at its place where the value is written as as many parts
/// (`other.fig, other.ax = self.fig, self.ax`), else each to the whole value (`self.fig, self.ax =
/// plt.subplots()`).
#[derive(Serialize, Deserialize, TS, Clone, Debug, Default, PartialEq)]
#[ts(export)]
pub struct Set {
    pub on: Held,
    /// The field of the value that is set, as the code names it, through the fields it is reached by:
    /// `fig` of `other.fig = ..`, `fig.title` of `self.fig.title = ..`; none for an element of the
    /// value itself (`rows[i] = row`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub field: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub from: Vec<Held>,
    /// Where the assigned expression is a field of a value, as the code names it (`fig` of `other.fig =
    /// self.fig`): that field of the one value of `from`. None where it is a whole value (`self.fig =
    /// fig`) or what the body computes (`self.title = title + "!"`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub of: String,
    /// The lines the binding spans, first and last; none for one the signature writes
    /// (`constructor(readonly until: string)`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(type = "[number, number] | null")]
    pub lines: Option<(u32, u32)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(type = "Array<[number, number]>")]
    pub arms: Vec<(u32, u32)>,
    /// Where the assigned expression is written, as a byte range of its file: a step written there that the
    /// set keeps (`self.left = Chart("left")`) is among what it reads once its result is named.
    #[serde(skip)]
    pub value: (u32, u32),
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
    /// What names the place it touches: for a file whose path is followed to where it is whole, the
    /// place itself where the path is written out in full, else the term the code writes
    /// (`{root}/out.csv`); else what the call is handed, its literal or the name it is written as.
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
    /// The lines of the other calls it stands for, each once: two `write_text` calls of one file,
    /// one in an arm and one after a read, are one touch that runs on both lines.
    pub also_at: Vec<u32>,
    /// Who makes its path or statement whole, where that is not its own function: callers whose
    /// calls hand what does, and entries whose users hand it. A disjunction of conjunctions, so a
    /// route shows it when every condition of one of them holds: it reaches each caller and starts
    /// at the entry (`mathematics.md`, "Who makes a path whole"). Kept with no conjunction beside
    /// one it contains (`absorb`). Empty: whole in its own function.
    pub by: BTreeSet<Both>,
    /// What the call gives goes to nothing its function hands on: it only decides a choice, goes
    /// to an error, or is dropped. A read that checks what is there, not one that uses it.
    pub checks: bool,
    /// The program's type whose code gives the path its root, when one does: a value of it holds
    /// the root (`self.root`), or one of its methods writes it. Its places are a group the picture
    /// folds (`mathematics.md`, "Places by the type that holds them").
    pub holder: String,
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
            also_at: vec![],
            by: BTreeSet::new(),
            checks: false,
            holder: String::new(),
        }
    }

    /// Whether the route that starts at `entry` and reaches `reached` runs this touch: one whole
    /// in its own function always, else where one of the conjunctions that make it whole holds.
    pub fn in_route(&self, entry: &str, reached: &dyn Fn(&str) -> bool) -> bool {
        self.by.is_empty() || self.by.iter().any(|b| b.holds(entry, reached))
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

/// One condition a route meets for a touch whose path others make whole to run in it: it reaches
/// a caller whose call hands what makes the path whole, or it starts at an entry whose users hand
/// it. The graph's JSON writes the first as the caller's id and the second as `users_of:` and the
/// entry's id.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Atom {
    Reaches(String),
    Starts(String),
}

impl Atom {
    const STARTS: &'static str = "users_of:";

    /// The function the condition names.
    pub fn name(&self) -> &str {
        match self {
            Atom::Reaches(f) | Atom::Starts(f) => f,
        }
    }

    /// Whether the route that starts at `entry` and reaches `reached` meets it.
    pub fn holds(&self, entry: &str, reached: &dyn Fn(&str) -> bool) -> bool {
        match self {
            Atom::Reaches(f) => reached(f),
            Atom::Starts(e) => e == entry,
        }
    }

    /// The same condition on what `f` makes of the function it names.
    pub fn map(&self, mut f: impl FnMut(&str) -> String) -> Atom {
        match self {
            Atom::Reaches(x) => Atom::Reaches(f(x)),
            Atom::Starts(x) => Atom::Starts(f(x)),
        }
    }

    fn parse(s: &str) -> Atom {
        match s.strip_prefix(Atom::STARTS) {
            Some(e) => Atom::Starts(e.to_string()),
            None => Atom::Reaches(s.to_string()),
        }
    }
}

impl std::fmt::Display for Atom {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Atom::Reaches(x) => f.write_str(x),
            Atom::Starts(e) => write!(f, "{}{e}", Atom::STARTS),
        }
    }
}

/// Conditions that make a path whole together: a conjunction, so a set (`a∧b = b∧a`, `a∧a = a`).
/// The graph's JSON writes it `a&b`, its conditions in order.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Both(pub BTreeSet<Atom>);

impl Both {
    /// The conjunction of one caller.
    pub fn one(f: impl Into<String>) -> Both {
        Both(BTreeSet::from([Atom::Reaches(f.into())]))
    }

    /// The conjunction of one entry's users.
    pub fn users(e: impl Into<String>) -> Both {
        Both(BTreeSet::from([Atom::Starts(e.into())]))
    }

    /// The callers a route reaches, in order.
    pub fn members(&self) -> impl Iterator<Item = &str> {
        self.0.iter().filter_map(|a| match a {
            Atom::Reaches(f) => Some(f.as_str()),
            Atom::Starts(_) => None,
        })
    }

    /// The function of each condition: the callers and the entries.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(Atom::name)
    }

    /// The one function, when the conjunction has one condition.
    pub fn single(&self) -> Option<&str> {
        match self.0.len() {
            1 => self.names().next(),
            _ => None,
        }
    }

    /// The entry whose users alone make the path whole, when the conjunction is that.
    pub fn users_of(&self) -> Option<&str> {
        match self.0.iter().collect::<Vec<_>>().as_slice() {
            [Atom::Starts(e)] => Some(e),
            _ => None,
        }
    }

    /// Whether the route that starts at `entry` and reaches `reached` meets every condition.
    pub fn holds(&self, entry: &str, reached: &dyn Fn(&str) -> bool) -> bool {
        self.0.iter().all(|a| a.holds(entry, reached))
    }

    /// The conjunction of what `f` makes of the function of each condition.
    pub fn map(&self, mut f: impl FnMut(&str) -> String) -> Both {
        Both(self.0.iter().map(|a| a.map(&mut f)).collect())
    }

    /// The conjunction the graph's JSON writes `a&b`.
    pub fn parse(s: &str) -> Both {
        Both(s.split('&').map(Atom::parse).collect())
    }
}

impl std::fmt::Display for Both {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut written: Vec<String> = self.0.iter().map(Atom::to_string).collect();
        written.sort();
        f.write_str(&written.join("&"))
    }
}

impl FromIterator<Atom> for Both {
    fn from_iter<I: IntoIterator<Item = Atom>>(xs: I) -> Both {
        Both(xs.into_iter().collect())
    }
}

/// Who makes a path whole in its canonical form: a monotone formula as the set of its minimal
/// conjunctions. A conjunction that contains another holds only where the other does, so the
/// formula with it is the formula without it (`a ∨ (a∧b) = a`): none is kept beside one it
/// contains, and two formulas that hold at the same routes are one (`mathematics.md`, "Who makes a
/// path whole"). Every formula is made or changed through it: read, merged, renamed, glued.
pub fn absorb(by: &mut BTreeSet<Both>) {
    let all: Vec<Both> = by.iter().cloned().collect();
    by.retain(|b| !all.iter().any(|c| c != b && c.0.is_subset(&b.0)));
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
    pub also_at: Vec<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub by: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub users_of: Vec<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub checks: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub holder: String,
}

impl From<Touch> for TouchJson {
    fn from(t: Touch) -> TouchJson {
        let (kind, dir) = (t.kind_name().to_string(), t.dir_name().to_string());
        // a conjunction of one entry's users alone is written in `users_of`, every other in `by`
        let (mut by, mut users_of) = (vec![], vec![]);
        for b in &t.by {
            match b.users_of() {
                Some(e) => users_of.push(e.to_string()),
                None => by.push(b.to_string()),
            }
        }
        by.sort();
        users_of.sort();
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
            also_at: t.also_at,
            by,
            users_of,
            checks: t.checks,
            holder: t.holder,
        }
    }
}

impl TryFrom<TouchJson> for Touch {
    type Error = String;
    fn try_from(j: TouchJson) -> Result<Touch, String> {
        let c = Crossing::named(&j.kind, &j.dir).ok_or_else(|| format!("no part of the world called `{}`", j.kind))?;
        let mut by: BTreeSet<Both> = j.by.iter().map(|b| Both::parse(b)).chain(j.users_of.into_iter().map(Both::users)).collect();
        absorb(&mut by);
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
            also_at: j.also_at,
            by,
            checks: j.checks,
            holder: j.holder,
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
    /// Every name a call not followed starts from, with how often, most first.
    pub unresolved_calls_top: Vec<(String, u32)>,
    /// The same calls by what the name each starts from is bound to.
    #[serde(default)]
    pub unresolved_calls: Unresolved,
    /// The bodies the reading linked: the program's and those of the libraries' code it read.
    #[serde(default)]
    pub linked_bodies: usize,
    /// Of those, the bodies of the libraries' code.
    #[serde(default)]
    pub linked_library_bodies: usize,
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

/// The languages a graph is read in: the image of its parts' languages, since a repository's graph
/// is the coproduct of its parts' and each part is read in one. Empty for no parts; a merge's is
/// the union of the merged graphs'. The JSON writes the one name when there is one and nothing
/// otherwise; a graph read back without one takes its parts' (`Graph`'s `Deserialize`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Languages(pub BTreeSet<Lang>);

impl Languages {
    /// The one language, when there is exactly one.
    pub fn one(&self) -> Option<Lang> {
        match self.0.len() {
            1 => self.0.first().copied(),
            _ => None,
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = Lang> + '_ {
        self.0.iter().copied()
    }

    /// Not exactly one language: what the JSON leaves out.
    pub fn not_one(&self) -> bool {
        self.one().is_none()
    }
}

impl From<Lang> for Languages {
    fn from(l: Lang) -> Languages {
        Languages(BTreeSet::from([l]))
    }
}

impl FromIterator<Lang> for Languages {
    fn from_iter<I: IntoIterator<Item = Lang>>(it: I) -> Languages {
        Languages(it.into_iter().collect())
    }
}

impl Serialize for Languages {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.one().serialize(s)
    }
}

impl<'de> Deserialize<'de> for Languages {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Languages, D::Error> {
        Ok(Option::<Lang>::deserialize(d)?.into_iter().collect())
    }
}

/// One part of a repository: what one manifest roots (a crate, a Python package, an app),
/// read in its own language.
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct Part {
    pub id: String,
    pub language: Lang,
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
#[serde(remote = "Graph")]
#[ts(export)]
pub struct Graph {
    pub root: String,
    /// The languages the repo was parsed in, one per part: `python`, `rust`, `typescript`.
    #[serde(default, skip_serializing_if = "Languages::not_one")]
    #[ts(as = "Option<Lang>", optional)]
    pub language: Languages,
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

impl Serialize for Graph {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        Graph::serialize(self, s)
    }
}

/// A graph whose JSON names no language is read in its parts' languages: none for no parts, each of
/// them where the parts are read in several.
impl<'de> Deserialize<'de> for Graph {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Graph, D::Error> {
        let mut g = Graph::deserialize(d)?;
        if g.language.0.is_empty() {
            g.language = g.parts.iter().map(|p| p.language).collect();
        }
        Ok(g)
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
    /// replace, what its calls reach and the types they hand and give, the values its own code uses,
    /// binds and sets and the error it hands on, a value named by its step, whose calls make its touches
    /// whole and whose code decides where they go), an edge's ends, a make target's own with its
    /// Makefile, its part and what it runs, the parts and the root.
    pub fn rename(&mut self, to: &dyn Fn(&str) -> String) {
        let m = |s: &mut String| {
            if !s.is_empty() {
                *s = to(s)
            }
        };
        // a value no name holds is named by the step that makes it, in the function it is made in: `#<function>@<where>`
        let named = |s: &mut String| {
            if let Some((f, at)) = s.strip_prefix('#').and_then(|r| r.rsplit_once('@')) {
                *s = format!("#{}@{at}", to(f));
            }
        };
        let held = |h: &mut Held| {
            m(&mut h.ty);
            named(&mut h.name);
        };
        m(&mut self.root);
        self.parts.iter_mut().for_each(|p| m(&mut p.id));
        for n in &mut self.nodes {
            m(&mut n.id);
            m(&mut n.module);
            m(&mut n.part);
            n.cls.iter_mut().chain(n.breaks.iter_mut()).chain(n.holds.values_mut()).for_each(&m);
            if let Some(p) = n.prophecy.as_mut() {
                p.supersedes.iter_mut().chain(p.replaces.iter_mut()).chain(p.candidates.iter_mut()).for_each(&m);
            }
            for s in &mut n.sites {
                m(&mut s.to);
                m(&mut s.on);
                m(&mut s.after);
                s.fails.iter_mut().for_each(&m);
                s.hands.iter_mut().chain(s.gives.iter_mut()).for_each(held);
            }
            for u in n.uses.iter_mut().chain(n.binds.iter_mut()) {
                m(&mut u.ty);
                named(&mut u.name);
            }
            n.fails.iter_mut().for_each(&m);
            for s in &mut n.sets {
                held(&mut s.on);
                s.from.iter_mut().for_each(held);
            }
            for t in &mut n.tests {
                held(&mut t.on);
            }
            for t in n.touches.iter_mut().chain(n.instead.iter_mut()) {
                t.by = t.by.iter().map(|b| b.map(to)).collect();
                absorb(&mut t.by);
                t.fixed_by.iter_mut().for_each(&m);
                if !t.holder.is_empty() {
                    m(&mut t.holder);
                }
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

    /// A parameter holds the type its function takes by that type's own name, whatever its id holds: a module path, or a
    /// shape with a dot in it (`{self.size}`), which is part of the name and no module.
    #[test]
    fn a_parameter_holds_the_type_it_takes_by_its_own_name() {
        for (written, id, name) in [
            ("Frame", "pandas.core.frame.Frame", "Frame"),
            ("Frame", "ext:pandas.Frame", "Frame"),
            ("Float[Tensor, batch dim]", "ext:jaxtyping.Float[Tensor, batch dim]", "Float[Tensor, batch dim]"),
            ("Float[Tensor, batch {self.size}]", "ext:jaxtyping.Float[Tensor, batch {self.size}]", "Float[Tensor, batch {self.size}]"),
            ("Float[Tensor, ... {self.a.b}]", "ext:jaxtyping.Float[Tensor, ... {self.a.b}]", "Float[Tensor, ... {self.a.b}]"),
        ] {
            assert_eq!(type_name(id), name, "{id}");
            assert_eq!(resolve_held(&[written.to_string()], &|_| Some(id.to_string())), [name], "{id}");
        }
    }

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

    /// Conditions (callers, and entries whose users hand a path), conjunctions of them as the
    /// graph's JSON writes them, and the same conjunctions with the conditions of each and the
    /// alternatives in another order.
    fn written_twice() -> impl proptest::strategy::Strategy<Value = (Vec<String>, Vec<String>)> {
        use proptest::prelude::*;
        let both = proptest::collection::vec("(users_of:)?[a-e](\\.[a-c])?", 1..4);
        proptest::collection::vec(both, 0..4)
            .prop_flat_map(|alts| {
                let shuffled: Vec<_> = alts.iter().map(|b| Just(b.clone()).prop_shuffle()).collect();
                (Just(alts), shuffled.prop_shuffle())
            })
            .prop_map(|(alts, shuffled)| {
                let written = |xs: &[Vec<String>]| xs.iter().map(|b| b.join("&")).collect::<Vec<_>>();
                (written(&alts), written(&shuffled))
            })
    }

    /// Methods a route is registered with, as the code lists them (a method of `REQUEST_WAYS`, a
    /// websocket, or one no table names), and the same methods in another order.
    fn listed_twice() -> impl proptest::strategy::Strategy<Value = (Vec<String>, Vec<String>)> {
        use proptest::prelude::*;
        proptest::collection::vec("get|head|options|post|put|patch|delete|websocket|[a-z]{3,6}", 1..5)
            .prop_flat_map(|ms| (Just(ms.clone()), Just(ms).prop_shuffle()))
    }

    proptest::proptest! {
        /// Law: who makes a path whole is a disjunction of conjunctions, a set of sets
        /// (`a∧b = b∧a`, `a∨b = b∨a`). Writing the conditions of each conjunction and the
        /// alternatives in another order gives the same touch, the same JSON, and the same routes it
        /// runs in; and the JSON reads back as the touch it was written from.
        #[test]
        fn the_order_callers_are_written_in_says_nothing(
            (by, again) in written_twice(),
            entry in "[a-e]",
            reached in proptest::collection::btree_set("[a-e](\\.[a-c])?", 0..8),
        ) {
            let touch = |by: Vec<String>| Touch::try_from(TouchJson { kind: "disk_read".into(), by, ..TouchJson::default() }).unwrap();
            let (t, u) = (touch(by), touch(again));
            proptest::prop_assert_eq!(&t, &u);
            let json = serde_json::to_string(&t).unwrap();
            proptest::prop_assert_eq!(&json, &serde_json::to_string(&u).unwrap());
            proptest::prop_assert_eq!(serde_json::from_str::<Touch>(&json).unwrap(), t.clone());
            let reached = |f: &str| reached.contains(f);
            proptest::prop_assert_eq!(t.in_route(&entry, &reached), u.in_route(&entry, &reached));
        }

        /// Law: who makes a path whole is a monotone formula, and two formulas that hold at the same
        /// routes are one (absorption, `a ∨ (a∧b) = a`). A formula with conjunctions added that
        /// contain one of its own gives the same touch and the same JSON as the formula alone; no
        /// conjunction the touch keeps contains another; and it runs where the formula as written
        /// holds.
        #[test]
        fn a_conjunction_that_contains_another_adds_nothing(
            (by, _) in written_twice(),
            more in proptest::collection::vec((0usize..4, "(users_of:)?[a-e](\\.[a-c])?"), 0..4),
            entry in "[a-e]",
            reached in proptest::collection::btree_set("[a-e](\\.[a-c])?", 0..8),
        ) {
            let mut longer = by.clone();
            for (i, extra) in &more {
                if !by.is_empty() {
                    longer.push(format!("{}&{extra}", by[i % by.len()]));
                }
            }
            let touch = |by: Vec<String>| Touch::try_from(TouchJson { kind: "disk_read".into(), by, ..TouchJson::default() }).unwrap();
            let (t, u) = (touch(by), touch(longer.clone()));
            proptest::prop_assert_eq!(&t, &u);
            proptest::prop_assert_eq!(serde_json::to_string(&t).unwrap(), serde_json::to_string(&u).unwrap());
            for a in u.by.iter() {
                proptest::prop_assert!(u.by.iter().all(|b| a == b || !a.0.is_subset(&b.0)), "{a} beside one that contains it");
            }
            let reached = |f: &str| reached.contains(f);
            let as_written = longer.is_empty() || longer.iter().any(|b| Both::parse(b).holds(&entry, &reached));
            proptest::prop_assert_eq!(u.in_route(&entry, &reached), as_written);
        }

        /// Law: a route serves a set of methods (`GET∨POST = POST∨GET`, `GET∨GET = GET`). Methods
        /// listed in another order, repeated, or in another case give the same route, the same label
        /// and the same JSON, and the JSON reads back as the route it was written from.
        #[test]
        fn the_order_methods_are_listed_in_says_nothing(
            (listed, again) in listed_twice(),
            twice in proptest::bool::ANY,
            path in proptest::option::of("(/[a-z{}]{1,4}){1,3}"),
        ) {
            let mut again: Vec<String> = again.iter().map(|m| m.to_ascii_uppercase()).collect();
            if twice {
                again.extend(again.clone());
            }
            let (r, s) = (Route { methods: Methods::of(&listed), path: path.clone() }, Route { methods: Methods::of(&again), path });
            proptest::prop_assert_eq!(&r, &s);
            proptest::prop_assert_eq!(r.to_string(), s.to_string());
            let json = serde_json::to_string(&r).unwrap();
            proptest::prop_assert_eq!(&json, &serde_json::to_string(&s).unwrap());
            proptest::prop_assert_eq!(serde_json::from_str::<Route>(&json).unwrap(), r);
        }

        /// Law: a request is written `METHOD path`, or its path alone where the code does not say
        /// the method, and what is written reads back as the request (`Display`, `Request::of`).
        #[test]
        fn a_request_reads_back_as_written(
            method in proptest::option::of("get|post|delete|[a-z]{3,6}"),
            path in "(/[a-z{}]{1,4}){1,3}",
        ) {
            let r = Request { method: method.as_deref().map(Method::new), path };
            proptest::prop_assert_eq!(Request::of(&r.to_string()), r.clone());
            let json = serde_json::to_string(&r).unwrap();
            proptest::prop_assert_eq!(serde_json::from_str::<Request>(&json).unwrap(), r);
        }
    }

    proptest::proptest! {
        /// Law: the tag of a name past the code is a prism (`ext`). Previewing what was reviewed
        /// gives the name back (`path(of(p)) = Some(p)`), reviewing what a preview gives is the id it
        /// was taken from, and an id of the code's previews to nothing. `name` is the copairing: the
        /// name either side says.
        #[test]
        fn the_tag_of_a_name_past_the_code_is_a_prism(p in "(ext:)?[a-z.:\\[\\] ]{0,10}", id in "(ext:|ex|e)?[a-z.:]{0,10}") {
            let reviewed = ext::of(&p);
            proptest::prop_assert_eq!(ext::path(&reviewed), Some(p.as_str()));
            proptest::prop_assert_eq!(ext::name(&reviewed), p.as_str());
            match ext::path(&id) {
                Some(q) => proptest::prop_assert_eq!(ext::of(q), id.clone()),
                None => {
                    proptest::prop_assert!(!ext::is(&id));
                    proptest::prop_assert_eq!(ext::name(&id), id.as_str());
                }
            }
        }

        /// Law: what a reader resolves a name to is a value of `Code ⊔ Beyond`, written as an id and
        /// read back as the same value; one side is the code's, the other past it, never both.
        #[test]
        fn a_resolved_name_reads_back_from_its_id(beyond in proptest::bool::ANY, p in "[a-z.:]{0,10}") {
            proptest::prop_assume!(beyond || !ext::is(&p));
            let r = if beyond { Resolved::Beyond(p.clone()) } else { Resolved::Code(p.clone()) };
            let id = r.clone().id();
            proptest::prop_assert_eq!(Resolved::of_id(id.clone()), r.clone());
            proptest::prop_assert_eq!(ext::is(&id), beyond);
            proptest::prop_assert_eq!(r.is_beyond(), beyond);
            proptest::prop_assert_eq!(r.clone().code().is_some(), !beyond);
            proptest::prop_assert_eq!(r.beyond(), beyond.then_some(p));
        }
    }
}
