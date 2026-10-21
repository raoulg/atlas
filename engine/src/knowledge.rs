//! What Atlas knows of the languages, libraries and frameworks code uses, as data: one table per
//! kind of fact. A reader looks a fact up here; it never tests a library's name in its own
//! control flow.

// ---------- the web ----------

/// The methods a request goes by.
pub const HTTP_METHODS: &[&str] = &["get", "post", "put", "patch", "delete", "head", "options"];

/// The hosts that are the machine a program runs on, where the other parts of a repository may
/// serve their routes.
pub const LOCAL_HOSTS: &[&str] = &["localhost", "127.0.0.1", "0.0.0.0", "[::1]"];

/// What else an app or a router registers a route by, beside an HTTP method: a websocket
/// (FastAPI), any method (Express's `all`).
pub const ROUTE_KINDS: &[&str] = &["websocket", "all"];

/// Whether an app's or a router's method of this name registers a route.
pub fn registers_route(verb: &str) -> bool {
    HTTP_METHODS.contains(&verb) || ROUTE_KINDS.contains(&verb)
}

/// Web frameworks that register a function as a route by a decorator, by package. A decorator
/// named for what it serves (`registers_route`) serves that: `@app.get("/x")`, Litestar's
/// `@get("/x")`. A decorator among `routes` serves the methods it is handed by the keyword that
/// `methods` names, and DEFAULT_METHOD when handed none: `@app.route("/x", methods=["POST"])`.
/// The path is the first argument or the keyword that `path` names, and `/` when none is handed.
pub struct WebFramework {
    pub package: &'static str,
    pub routes: &'static [&'static str],
    pub methods: &'static str,
    pub path: &'static str,
}

pub const WEB_FRAMEWORKS: &[WebFramework] = &[
    WebFramework { package: "fastapi", routes: &["api_route", "route"], methods: "methods", path: "path" },
    WebFramework { package: "starlette", routes: &["route"], methods: "methods", path: "path" },
    WebFramework { package: "flask", routes: &["route"], methods: "methods", path: "rule" },
    WebFramework { package: "quart", routes: &["route"], methods: "methods", path: "rule" },
    WebFramework { package: "litestar", routes: &["route"], methods: "http_method", path: "path" },
];

/// The method a route serves when its decorator names none.
pub const DEFAULT_METHOD: &str = "get";

/// The web framework a package is.
pub fn web_framework(package: &str) -> Option<&'static WebFramework> {
    WEB_FRAMEWORKS.iter().find(|w| w.package == package)
}

/// Whether a decorator of this name registers a route in some web framework.
pub fn registers_route_by_decorator(name: &str) -> bool {
    registers_route(name) || WEB_FRAMEWORKS.iter().any(|w| w.routes.contains(&name))
}

/// Where a call hands an argument: by position, or by keyword.
pub enum Arg {
    At(usize),
    Named(&'static str),
}

/// How an app takes another app's or a router's routes under a prefix: the package whose app
/// does it, the method, and where it is handed the prefix and what it mounts. A prefix handed by
/// position must be there; one by keyword may be left out. A prefix that `replaces` stands in
/// for the mounted router's own (Flask's `register_blueprint(bp, url_prefix=..)`).
pub struct Mount {
    pub package: &'static str,
    pub verb: &'static str,
    pub prefix: Arg,
    pub child: Arg,
    pub replaces: bool,
}

pub const MOUNTS: &[Mount] = &[
    Mount { package: "starlette", verb: "mount", prefix: Arg::At(0), child: Arg::At(1), replaces: false },
    Mount { package: "fastapi", verb: "mount", prefix: Arg::At(0), child: Arg::At(1), replaces: false },
    Mount { package: "fastapi", verb: "include_router", prefix: Arg::Named("prefix"), child: Arg::At(0), replaces: false },
    Mount { package: "flask", verb: "register_blueprint", prefix: Arg::Named("url_prefix"), child: Arg::At(0), replaces: true },
    Mount { package: "express", verb: "use", prefix: Arg::At(0), child: Arg::At(1), replaces: false },
    Mount { package: "hono", verb: "route", prefix: Arg::At(0), child: Arg::At(1), replaces: false },
];

/// A router that carries its own prefix: its package and class, the keyword it is handed the
/// prefix by, and the keyword it is handed its handlers by, when it is handed them.
pub struct PrefixedRouter {
    pub package: &'static str,
    pub class: &'static str,
    pub prefix: &'static str,
    pub handlers: Option<&'static str>,
}

pub const PREFIXED_ROUTERS: &[PrefixedRouter] = &[
    PrefixedRouter { package: "fastapi", class: "APIRouter", prefix: "prefix", handlers: None },
    PrefixedRouter { package: "flask", class: "Blueprint", prefix: "url_prefix", handlers: None },
    PrefixedRouter { package: "litestar", class: "Router", prefix: "path", handlers: Some("route_handlers") },
];

/// A class whose methods' routes are under a path the class names: its package, the base class,
/// and the attribute that holds the path (Litestar's `Controller.path`).
pub const CONTROLLERS: &[(&str, &str, &str)] = &[("litestar", "Controller", "path")];

/// Packages whose app or router registers a route by a call: `app.get("/x", handler)`.
pub const ROUTERS: &[&str] = &["hono", "express", "fastify", "koa-router", "@koa/router", "elysia", "itty-router", "h3"];

/// Functions that declare a route by the file they are in (TanStack Router), and those that
/// declare the root route.
pub const FILE_ROUTES: &[&str] = &["createFileRoute", "createLazyFileRoute"];
pub const ROOT_ROUTES: &[&str] = &["createRootRoute", "createRootRouteWithContext"];

/// The parameters a call to the web names what it asks for by, the first that a call's library
/// has: a request's URL, the Hub repository a download or an upload names (`repo_id`, and the
/// datasets library's `path`).
pub const ASKED_BY: &[&str] = &["url", "repo_id", "path"];

/// Servers that register a function as a tool an agent may call, by a decorator on a value of
/// the server's class: `@server.tool`, `@server.resource(..)`, `@server.prompt`. By package and
/// class.
pub const TOOL_SERVERS: &[(&str, &str)] = &[("fastmcp", "FastMCP"), ("mcp", "FastMCP")];

/// The decorators of a tool server that register a function.
pub const TOOL_DECORATORS: &[&str] = &["tool", "resource", "prompt"];

// ---------- command lines ----------

/// Command-line frameworks that register a function as a command by a decorator, by package:
/// the decorators that do (`@cli.command()`, `@click.command`), and the suffixes their naming
/// rule drops. A command is run by the name its decorator is handed, first or as COMMAND_NAME,
/// else by its function's name lowercased with `_` written `-`, less a last part among the
/// suffixes (click's `export_cmd` is `export`).
pub struct CliFramework {
    pub package: &'static str,
    pub commands: &'static [&'static str],
    pub drops: &'static [&'static str],
    /// Where a command declares what the command line hands it.
    pub options: OptionsBy,
}

/// How a command-line framework declares a command's options and arguments, by the names of the
/// framework's functions that declare one of each.
pub enum OptionsBy {
    /// By the command's parameters (typer): one with a default is an option, one without an
    /// argument; a default or an annotation that calls `option` or `argument` says which, and
    /// `option` may be handed the option's flags (`typer.Option("--out", "-o")`).
    Parameters { option: &'static str, argument: &'static str },
    /// By decorators on the command (click), in the order they are written: `option` handed the
    /// option's flags (`@click.option("--out", "-o")`), `argument` the argument's name.
    Decorators { option: &'static str, argument: &'static str },
}

const CLICK_DROPS: &[&str] = &["command", "cmd", "group", "grp"];
const CLICK_OPTIONS: OptionsBy = OptionsBy::Decorators { option: "option", argument: "argument" };

pub const CLI_FRAMEWORKS: &[CliFramework] = &[
    CliFramework { package: "click", commands: &["command"], drops: CLICK_DROPS, options: CLICK_OPTIONS },
    CliFramework { package: "rich_click", commands: &["command"], drops: CLICK_DROPS, options: CLICK_OPTIONS },
    CliFramework {
        package: "typer",
        commands: &["command"],
        drops: &[],
        options: OptionsBy::Parameters { option: "Option", argument: "Argument" },
    },
];

/// The keyword a command's decorator is handed the command's name by.
pub const COMMAND_NAME: &str = "name";

/// The command-line framework a package is.
pub fn cli_framework(package: &str) -> Option<&'static CliFramework> {
    CLI_FRAMEWORKS.iter().find(|c| c.package == package)
}

impl CliFramework {
    /// The name a command is run by when its decorator names none.
    pub fn command_name(&self, function: &str) -> String {
        let name = function.to_lowercase().replace('_', "-");
        match name.rsplit_once('-') {
            Some((rest, suffix)) if self.drops.contains(&suffix) => rest.to_string(),
            _ => name,
        }
    }
}

// ---------- a Python project's manifest ----------

/// What a table of entry points declares: programs, run by their names, or plugins, each loaded
/// by the host that names its group.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Declares {
    Programs,
    Plugins,
}

/// The tables of `pyproject.toml` that declare a project's entry points, by their path in the
/// file: the standard's and Poetry's. An entry names its object as `module:object`, by itself or
/// under one of ENTRY_REFERENCES.
pub const ENTRY_TABLES: &[(&[&str], Declares)] = &[
    (&["project", "scripts"], Declares::Programs),
    (&["project", "gui-scripts"], Declares::Programs),
    (&["tool", "poetry", "scripts"], Declares::Programs),
    (&["project", "entry-points"], Declares::Plugins),
    (&["tool", "poetry", "plugins"], Declares::Plugins),
];

/// The keys an entry written as a table names its object by: Poetry's `{ callable = .. }` and
/// `{ reference = .. }`.
pub const ENTRY_REFERENCES: &[&str] = &["callable", "reference"];

/// How a build backend's setting names the packages it builds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Names {
    /// Each a package's directory: Hatch's `packages = ["src/pkg"]`.
    Dirs,
    /// Each a package's import name, `pkg` or `pkg.sub`, found in the directories PACKAGE_ROOTS
    /// give: setuptools' `packages = ["pkg"]`, uv's `module-name = "pkg"`.
    Imports,
    /// A table of import names, each with its package's directory; the empty name's is the
    /// directory the others are in, one of PACKAGE_ROOTS: setuptools'
    /// `package-dir = { pkg = "lib/pkg" }`.
    DirsByName,
    /// Each a table that names the package under `name` and the directory it is in under `dir`:
    /// Poetry's `packages = [{ include = "pkg", from = "src" }]`.
    Tables { name: &'static str, dir: &'static str },
}

/// The settings of `pyproject.toml` by which a build backend names the packages it builds, by
/// their path in the file, in the order they are asked. A setting holds one name or a list.
pub const PACKAGE_SETTINGS: &[(&[&str], Names)] = &[
    (&["tool", "hatch", "build", "targets", "wheel", "packages"], Names::Dirs),
    (&["tool", "hatch", "build", "packages"], Names::Dirs),
    (&["tool", "setuptools", "packages"], Names::Imports),
    (&["tool", "setuptools", "package-dir"], Names::DirsByName),
    (&["tool", "poetry", "packages"], Names::Tables { name: "include", dir: "from" }),
    (&["tool", "uv", "build-backend", "module-name"], Names::Imports),
];

/// The settings of `pyproject.toml` that name the directories a build backend finds packages
/// in, by their path in the file: setuptools' `package-dir = { "" = "src" }` and
/// `packages.find.where`, uv's `module-root`. Each holds a directory or a list.
pub const PACKAGE_ROOTS: &[&[&str]] = &[
    &["tool", "setuptools", "package-dir", ""],
    &["tool", "setuptools", "packages", "find", "where"],
    &["tool", "uv", "build-backend", "module-root"],
];

/// The directories a Python project keeps its packages in beside those PACKAGE_ROOTS name: the
/// src layout's `src/` and the flat layout's root.
pub const PACKAGE_LAYOUTS: &[&str] = &["src", ""];

/// Where a project names itself: the standard's `[project]` and Poetry's own table. Without a
/// setting of PACKAGE_SETTINGS, the backends build the package of the project's name.
pub const PROJECT_NAMES: &[&[&str]] = &[&["project", "name"], &["tool", "poetry", "name"]];

// ---------- the files of a project ----------

/// Directories never listed at all: installed dependencies and build output, too large to read
/// and never the author's.
pub const NEVER_READ: &[&str] = &["node_modules", "target", "site-packages", "__pycache__"];

/// Directories a tool made rather than the author, beside those never read: environments,
/// build output, a framework's output.
pub const MADE_DIRS: &[&str] =
    &["dist", "build", "out", "coverage", "venv", "env", ".next", ".output", ".wrangler", ".vercel", ".turbo", "public"];

/// Whether a directory of this name is one a tool made.
pub fn made_dir(name: &str) -> bool {
    NEVER_READ.contains(&name) || MADE_DIRS.contains(&name)
}

/// Directories that hold tests and the data they read.
pub const TEST_DIRS: &[&str] = &["tests", "test", "__tests__", "fixtures", "testdata"];

/// A Python test file by pytest's convention, wherever it stands: `test_*.py`, `*_test.py`, and
/// `conftest.py`, which holds what its tests share.
pub fn python_test_file(name: &str) -> bool {
    name.starts_with("test_") || name.ends_with("_test.py") || name == "conftest.py"
}

// ---------- a library's code ----------

/// The directory a JavaScript package is installed in, below the project or a directory above it.
pub const NODE_PACKAGES: &str = "node_modules";

/// Node's own modules, imported by their names (`fs`) or under `node:`: the language's library,
/// never a package's.
pub const NODE_BUILTINS: &[&str] = &[
    "assert",
    "async_hooks",
    "buffer",
    "child_process",
    "cluster",
    "console",
    "constants",
    "crypto",
    "dgram",
    "diagnostics_channel",
    "dns",
    "domain",
    "events",
    "fs",
    "http",
    "http2",
    "https",
    "inspector",
    "module",
    "net",
    "os",
    "path",
    "perf_hooks",
    "process",
    "punycode",
    "querystring",
    "readline",
    "repl",
    "sqlite",
    "stream",
    "string_decoder",
    "sys",
    "test",
    "timers",
    "tls",
    "trace_events",
    "tty",
    "url",
    "util",
    "v8",
    "vm",
    "wasi",
    "worker_threads",
    "zlib",
];

/// Where Cargo keeps the source of the crates it fetched from a registry, below its home: one
/// directory per registry, holding `<crate>-<version>/`.
pub const CARGO_SOURCES: &[&str] = &["registry", "src"];

/// The files that say a package holds compiled code, by registry: Python's extension modules,
/// Node's native addons and WebAssembly, the C and C++ a crate builds. What such a package does
/// is in code no reader of its language reads, so Atlas reads none of it.
pub const COMPILED: &[(Registry, &[&str])] =
    &[(PyPi, &["so", "pyd", "dylib"]), (Npm, &["node", "wasm"]), (Crates, &["c", "cc", "cpp", "cxx"])];

/// The extensions of compiled code in a package of a registry.
pub fn compiled(from: Registry) -> &'static [&'static str] {
    COMPILED.iter().find(|(r, _)| *r == from).map_or(&[], |(_, exts)| exts)
}

/// The crates the tables name by a type or a function alone: csv's and plotters' file openers,
/// the containers of indexmap, smallvec, arrayvec and dashmap, the cells of parking_lot,
/// once_cell and lazy_static.
pub const RUST_NAMED_BY_TYPE: &[&str] =
    &["csv", "plotters", "indexmap", "smallvec", "arrayvec", "dashmap", "parking_lot", "once_cell", "lazy_static"];

// ---------- values ----------

/// Methods that hand on the value they are called on, opened or unchanged: the standard
/// library's `Result` and `Option`, anyhow's context. By crate.
pub const HANDS_ON: &[(&str, &[&str])] = &[
    (
        "std",
        &[
            "unwrap",
            "expect",
            "unwrap_or",
            "unwrap_or_else",
            "unwrap_or_default",
            "ok",
            "err",
            "ok_or",
            "ok_or_else",
            "map",
            "map_err",
            "and_then",
            "or_else",
        ],
    ),
    ("anyhow", &["context", "with_context"]),
];

/// Whether a method of this name hands on the value it is called on.
pub fn hands_on(method: &str) -> bool {
    HANDS_ON.iter().any(|(_, ms)| ms.contains(&method))
}

/// Python's generic types whose parameters are the values they hold: `list[Graph]` holds
/// Graphs.
pub const PYTHON_HOLDERS: &[&str] = &[
    "list",
    "dict",
    "set",
    "tuple",
    "frozenset",
    "Iterable",
    "Iterator",
    "Sequence",
    "Mapping",
    "Generator",
    "AsyncIterator",
    "List",
    "Dict",
    "Tuple",
    "Set",
    "FrozenSet",
    "Deque",
    "deque",
    "DefaultDict",
    "defaultdict",
    "OrderedDict",
    "Counter",
    "ChainMap",
    "Collection",
    "Container",
    "AbstractSet",
    "MutableSet",
    "MutableSequence",
    "MutableMapping",
    "AsyncIterable",
    "AsyncGenerator",
    "Reversible",
];

/// The keywords a Python call is handed a function by whose value the call uses: the key it
/// orders, picks or groups by (`sorted(xs, key=score)`, `max(xs, key=..)`, `list.sort(key=..)`,
/// `itertools.groupby(xs, key=..)`).
pub const PYTHON_USES_HANDED: &[&str] = &["key"];

/// What makes a Python method a declaration that what implements it fills in: a decorator
/// (`@abstractmethod`), or a base, by package and name, that makes its class a protocol, which any
/// class with its methods implements without naming it (`typing.Protocol`).
pub const PYTHON_ABSTRACT: &[&str] = &["abstractmethod"];
pub const PYTHON_PROTOCOLS: &[(&str, &str)] = &[("typing", "Protocol"), ("typing_extensions", "Protocol")];

/// Python's generic types whose value is one of their parameters: `Optional[Node]` is a Node or
/// nothing.
pub const PYTHON_ALTERNATIVES: &[&str] = &["Optional", "Union"];

/// Python's generic types whose parameters describe the value without being it:
/// `Annotated[T, ..]` is a `T` with notes, `Callable[[A], B]` takes an `A` and gives a `B`,
/// `type[T]` is the class itself.
pub const PYTHON_DESCRIBES: &[&str] = &[
    "type",
    "Type",
    "Callable",
    "Awaitable",
    "Coroutine",
    "Annotated",
    "ClassVar",
    "Final",
    "Required",
    "NotRequired",
    "ReadOnly",
    "TypeGuard",
    "TypeIs",
    "Unpack",
];

/// Whether a Python generic type's parameters are the values it holds or stands for.
pub fn python_container(name: &str) -> bool {
    PYTHON_HOLDERS.contains(&name) || PYTHON_ALTERNATIVES.contains(&name) || PYTHON_DESCRIBES.contains(&name)
}

/// TypeScript's generic types whose arguments are the values they hold or stand for:
/// `Promise<Regel[]>` hands on Regels.
pub const TYPESCRIPT_CONTAINERS: &[&str] = &[
    "Promise",
    "Array",
    "ReadonlyArray",
    "Set",
    "ReadonlySet",
    "Map",
    "ReadonlyMap",
    "Record",
    "Partial",
    "Required",
    "Readonly",
    "Pick",
    "Omit",
    "NonNullable",
    "Awaited",
    "Iterable",
    "AsyncIterable",
    "Generator",
    "AsyncGenerator",
];

/// Rust's generic types whose arguments are the values they hold or stand for, by the name Rust
/// code writes them with: std's collections, pointers, cells and locks with their guards, an
/// `Option`; what an iterator, a future or a stream gives (`impl Iterator<Item = Month>` gives
/// Months); a value that converts to or borrows as its argument (`impl Into<Settings>`); and the
/// containers of common crates (indexmap's, smallvec's, arrayvec's, dashmap's, once_cell's
/// `Lazy`). The library's sums (`RUST_SUMS`) are their arguments as well. Any other generic's
/// arguments are types: `DrawingArea<DB, Shift>` is a `DrawingArea`.
pub const RUST_CONTAINERS: &[&str] = &[
    "Vec",
    "VecDeque",
    "HashMap",
    "HashSet",
    "BTreeMap",
    "BTreeSet",
    "BinaryHeap",
    "LinkedList",
    "Box",
    "Rc",
    "Arc",
    "Weak",
    "Cow",
    "Pin",
    "Cell",
    "RefCell",
    "Ref",
    "RefMut",
    "Mutex",
    "MutexGuard",
    "RwLock",
    "RwLockReadGuard",
    "RwLockWriteGuard",
    "OnceLock",
    "OnceCell",
    "LazyLock",
    "LazyCell",
    "Option",
    "Iterator",
    "IntoIterator",
    "DoubleEndedIterator",
    "ExactSizeIterator",
    "Future",
    "IntoFuture",
    "Stream",
    "AsRef",
    "AsMut",
    "Borrow",
    "BorrowMut",
    "Into",
    "IndexMap",
    "IndexSet",
    "SmallVec",
    "ArrayVec",
    "DashMap",
    "DashSet",
    "Lazy",
];

/// The Rust containers among `RUST_CONTAINERS` that hold many values, not one: std's
/// collections, what an iterator or a stream gives, and the collections of common crates. A pair
/// one holds is an element of a type the code does not name (`Vec<(NaiveDate, f64)>`).
pub const RUST_COLLECTIONS: &[&str] = &[
    "Vec",
    "VecDeque",
    "HashMap",
    "HashSet",
    "BTreeMap",
    "BTreeSet",
    "BinaryHeap",
    "LinkedList",
    "Iterator",
    "IntoIterator",
    "DoubleEndedIterator",
    "ExactSizeIterator",
    "Stream",
    "IndexMap",
    "IndexSet",
    "SmallVec",
    "ArrayVec",
    "DashMap",
    "DashSet",
];

/// Whether a Rust generic type's arguments are the values it holds or stands for.
pub fn rust_container(name: &str) -> bool {
    RUST_CONTAINERS.contains(&name) || RUST_SUMS.contains(&name)
}

/// Methods that change the value they are called on: a language's own containers', and a
/// library's (networkx's graphs). By registry and source.
/// A Python call changes its receiver when ty says it goes to the package of a row that has the
/// method, and ty says where the method is defined: `list.append` in `builtins`, the methods
/// `list`, `dict` and `set` inherit (`clear`, `update`, `setdefault`, `reverse`, `popitem`) on
/// `typing`'s `MutableSequence`, `MutableMapping` and `MutableSet`, `deque`'s, `Counter`'s and
/// `OrderedDict`'s in `collections`, and `os.environ`'s own `setdefault` and `pop` in `os`. The
/// `std` row is the methods of Rust's collections (`Vec`, `String`, `VecDeque`, the maps and sets)
/// that take `&mut self` and change what they hold.
pub const MUTATORS: &[(Registry, &str, &[&str])] = &[
    (
        Registry::PyPi,
        "builtins",
        &[
            "append",
            "extend",
            "insert",
            "remove",
            "pop",
            "clear",
            "update",
            "add",
            "discard",
            "setdefault",
            "sort",
            "reverse",
            "popitem",
            "difference_update",
            "intersection_update",
            "symmetric_difference_update",
        ],
    ),
    (
        Registry::PyPi,
        "typing",
        &["append", "extend", "insert", "remove", "pop", "clear", "reverse", "add", "discard", "update", "setdefault", "popitem"],
    ),
    (
        Registry::PyPi,
        "collections",
        &[
            "append",
            "appendleft",
            "extend",
            "extendleft",
            "insert",
            "remove",
            "pop",
            "popleft",
            "rotate",
            "sort",
            "update",
            "subtract",
            "setdefault",
            "popitem",
            "move_to_end",
        ],
    ),
    (Registry::PyPi, "os", &["setdefault", "pop"]),
    (Registry::PyPi, "networkx", &["add_node", "add_edge", "add_nodes_from", "add_edges_from", "remove_node", "remove_edge"]),
    (
        Registry::Npm,
        "javascript",
        &["push", "pop", "shift", "unshift", "splice", "sort", "reverse", "fill", "copyWithin", "set", "delete", "clear", "add"],
    ),
    (
        Registry::Crates,
        "std",
        &[
            "push",
            "push_str",
            "push_back",
            "push_front",
            "append",
            "extend",
            "insert",
            "remove",
            "swap_remove",
            "pop",
            "pop_back",
            "pop_front",
            "clear",
            "retain",
            "truncate",
            "resize",
            "drain",
            "split_off",
            "dedup",
            "sort",
            "sort_by",
            "sort_by_key",
            "sort_unstable",
            "reverse",
        ],
    ),
];

/// The language's own containers that change in place, as TypeScript names them in a type or a
/// `new`: an array (`Row[]`, `Array<Row>`), a `Map` and a `Set`. What changes them is the
/// `javascript` row of `MUTATORS`.
pub const TYPESCRIPT_CHANGEABLE: &[&str] = &["Array", "Map", "Set", "WeakMap", "WeakSet"];

/// The language's own type guards that narrow the value they are handed to one of its containers,
/// where they hold: `Array.isArray(x)` says `x` is an `Array`. By the global and the guard's name.
pub const TYPESCRIPT_GUARDS: &[(&str, &str)] = &[("Array", "isArray")];

/// A package whose methods change the value they are called on when handed this keyword as
/// `True`: pandas' `inplace=True`.
pub const IN_PLACE: &[(&str, &str)] = &[("pandas", "inplace")];

/// The methods that change what they are called on, in the code of one registry's languages.
pub fn mutators(from: Registry) -> Vec<&'static str> {
    MUTATORS.iter().filter(|(r, _, _)| *r == from).flat_map(|(_, _, ms)| ms.iter().copied()).collect()
}

/// Whether a method of a package changes what it is called on.
pub fn mutates(from: Registry, package: &str, method: &str) -> bool {
    MUTATORS.iter().any(|(r, p, ms)| *r == from && *p == package && ms.contains(&method))
}

/// Functions that change an argument they are handed, by package, name and the argument's
/// position: `setattr(obj, ..)`, `random.shuffle(xs)`, the heap and the sorted list that `heapq`
/// and `bisect` keep in a list they are handed.
pub const CHANGES_ARGUMENT: &[(&str, &str, usize)] = &[
    ("builtins", "setattr", 0),
    ("builtins", "delattr", 0),
    ("random", "shuffle", 0),
    ("heapq", "heappush", 0),
    ("heapq", "heappop", 0),
    ("heapq", "heapify", 0),
    ("heapq", "heapreplace", 0),
    ("heapq", "heappushpop", 0),
    ("bisect", "insort", 0),
    ("bisect", "insort_left", 0),
    ("bisect", "insort_right", 0),
];

/// Python's own containers that change in place, by module and class as ty finds them: an
/// augmented assignment (`xs += ys`) changes one, where it makes any other value anew.
pub const PYTHON_CHANGEABLE: &[(&str, &str)] = &[
    ("builtins", "list"),
    ("builtins", "dict"),
    ("builtins", "set"),
    ("builtins", "bytearray"),
    ("collections", "deque"),
    ("collections", "Counter"),
    ("collections", "OrderedDict"),
    ("collections", "defaultdict"),
    ("typing", "MutableSequence"),
    ("typing", "MutableMapping"),
    ("typing", "MutableSet"),
    ("typing", "List"),
    ("typing", "Dict"),
    ("typing", "Set"),
    ("typing", "Deque"),
    ("typing", "DefaultDict"),
    ("typing", "OrderedDict"),
    ("typing", "Counter"),
];

/// Libraries whose array types carry their shape in the annotation (`Float[Tensor, "batch dim"]`):
/// such a type keeps its shape in its name.
pub const SHAPED_ARRAYS: &[&str] = &["jaxtyping"];

/// A shaped array's id without its library: `Float[Tensor, batch dim]`.
pub fn shaped(id: &str) -> Option<&str> {
    SHAPED_ARRAYS.iter().find_map(|p| id.strip_prefix("ext:")?.strip_prefix(p)?.strip_prefix('.'))
}

/// Functions that wrap a callback-taking function into one that returns a promise: what they
/// make is the function they are handed. By package and name.
pub const PROMISIFIERS: &[(&str, &str)] = &[("util", "promisify")];

// ---------- construction (Rust's C-CTOR and C-BUILDER) ----------

/// Functions that build a value and move nothing: a constructor, a default, a builder and what
/// finishes it.
pub const BUILDS: &[&str] = &["new", "builder", "default", "build"];

/// A builder's start and its end: every call between them builds.
pub const BUILDER: (&str, &str) = ("builder", "build");

/// The options of a file opener that make it write: `OpenOptions::new().append(true)`.
pub const OPENS_TO_WRITE: &[&str] = &["write", "append", "create", "create_new", "truncate"];

// ---------- databases ----------

/// The keywords a call tells a database's driver which way it goes by: neo4j's `routing_` on a
/// query and `default_access_mode` on a session.
pub const DRIVER_ACCESS: &[&str] = &["routing_", "default_access_mode"];

// ---------- where libraries reach ----------

use crate::model::{Crossing, Db, Dir, World};

/// The registry a package comes from: a package's name means a library in one of them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Registry {
    PyPi,
    Crates,
    Npm,
}

use Registry::{Crates, Npm, PyPi};

const NET: World = World::Net;
const LLM: World = World::Llm;
const PROCESS: World = World::Process;
const SQL: World = World::Db(Db::Sql);
const SQLITE: World = World::Db(Db::Sqlite);
const DUCKDB: World = World::Db(Db::Duckdb);
const GRAPH: World = World::Db(Db::Graph);
const KV: World = World::Db(Db::Kv);

/// Libraries that are a client of a part of the world, by registry and package: their calls
/// reach it, and a value of their types is a door to it.
pub const PACKAGES: &[(Registry, &str, World)] = &[
    (PyPi, "requests", NET),
    (PyPi, "httpx", NET),
    (PyPi, "urllib", NET),
    (PyPi, "http", NET),
    (PyPi, "socket", NET),
    (PyPi, "aiohttp", NET),
    (Crates, "reqwest", NET),
    (Crates, "ureq", NET),
    (Crates, "hyper", NET),
    (Crates, "tungstenite", NET),
    (Crates, "tokio_tungstenite", NET),
    (Crates, "curl", NET),
    (Crates, "axum", NET),
    (Crates, "TcpStream", NET),
    (Crates, "TcpListener", NET),
    (Npm, "axios", NET),
    (Npm, "ky", NET),
    (Npm, "ofetch", NET),
    (Npm, "node-fetch", NET),
    (Npm, "undici", NET),
    (Npm, "got", NET),
    (Npm, "ws", NET),
    (PyPi, "openai", LLM),
    (PyPi, "anthropic", LLM),
    (Npm, "@anthropic-ai/sdk", LLM),
    (Npm, "openai", LLM),
    (Npm, "@google/generative-ai", LLM),
    (Npm, "@google/genai", LLM),
    (Npm, "@mistralai/mistralai", LLM),
    (Npm, "ollama", LLM),
    (PyPi, "duckdb", DUCKDB),
    (PyPi, "neo4j", GRAPH),
    (PyPi, "mgclient", GRAPH),
    (PyPi, "sqlite3", SQLITE),
    (PyPi, "psycopg2", SQL),
    (PyPi, "psycopg", SQL),
    (PyPi, "asyncpg", SQL),
    (PyPi, "sqlalchemy", SQL),
    (PyPi, "redis", KV),
    (Crates, "rusqlite", SQLITE),
    (Crates, "duckdb", DUCKDB),
    (Crates, "sqlx", SQL),
    (Crates, "diesel", SQL),
    (Crates, "postgres", SQL),
    (Crates, "tokio_postgres", SQL),
    (Crates, "sea_orm", SQL),
    (Npm, "pg", SQL),
    (Npm, "postgres", SQL),
    (Npm, "mysql2", SQL),
    (Npm, "mysql", SQL),
    (Npm, "@prisma/client", SQL),
    (Npm, "drizzle-orm", SQL),
    (Npm, "kysely", SQL),
    (Npm, "knex", SQL),
    (Npm, "@neondatabase/serverless", SQL),
    (Npm, "better-sqlite3", SQLITE),
    (Npm, "sqlite3", SQLITE),
    (Npm, "sqlite", SQLITE),
    (Npm, "@libsql/client", SQLITE),
    (Npm, "duckdb", DUCKDB),
    (Npm, "@duckdb/node-api", DUCKDB),
    (Npm, "@duckdb/duckdb-wasm", DUCKDB),
    (Npm, "neo4j-driver", GRAPH),
    (Npm, "redis", KV),
    (Npm, "ioredis", KV),
    (Npm, "@upstash/redis", KV),
    (Npm, "idb-keyval", KV),
    (PyPi, "subprocess", PROCESS),
    (Npm, "child_process", PROCESS),
    (Npm, "execa", PROCESS),
];

/// The part of the world a package reaches, from one registry or from any.
pub fn reaches(pkg: &str, from: Option<Registry>) -> Option<World> {
    PACKAGES.iter().find(|(r, p, _)| *p == pkg && from.map_or(true, |f| f == *r)).map(|(_, _, w)| *w)
}

/// What a request does that moves data, beside its HTTP method: send one, stream one, open a URL.
pub const REQUESTS: &[&str] = &["request", "send", "stream", "urlopen", "urlretrieve"];

/// A socket's connecting, sending and receiving: the network calls that are handed no URL.
pub const SOCKET_OPS: &[&str] = &["connect", "send", "sendall", "sendto", "recv", "recvfrom", "recv_into"];

/// Whether a web library's operation moves data: a request by its method or its kind, or a
/// socket's traffic.
pub fn moves_on_the_web(op: &str) -> bool {
    HTTP_METHODS.contains(&op) || REQUESTS.contains(&op) || SOCKET_OPS.contains(&op)
}

/// What a database driver does that reaches the database: open a connection or a session, run
/// a statement, or flush or commit what a session holds. Handing an object to an ORM's session
/// (`add`, `merge`, `delete`) only stages it, and reading the rows a statement gave back or
/// closing is part of what already ran.
pub const DB_OPS: &[&str] = &[
    "connect",
    "driver",
    "session",
    "execute",
    "executemany",
    "executescript",
    "sql",
    "query",
    "run",
    "execute_query",
    "execute_read",
    "execute_write",
    "read_transaction",
    "write_transaction",
    "register",
    "read_csv",
    "read_parquet",
    "read_json",
    "flush",
    "commit",
];

/// What a file database's driver calls to open the database: it is handed the file first.
pub const DB_OPENS: &[&str] = &["connect", "open"];

/// Whether `op` opens `db` on the file it is handed. A server, a graph or a store is reached at an
/// address, not as a file.
pub fn opens_a_file(db: Db, op: &str) -> bool {
    matches!(db, Db::Sqlite | Db::Duckdb) && DB_OPENS.contains(&op)
}

/// Whether what a file database is opened on names a file: not the database in memory.
pub fn names_a_file(path: &str) -> bool {
    let memory = path.trim_start_matches("file:").starts_with(":memory:") || path.contains("mode=memory");
    !path.is_empty() && !memory && !path.contains("://")
}

/// A driver's own operations that reach the database beside DB_OPS, by package: asyncpg's
/// queries that hand back their rows and its pool of connections.
pub const DRIVER_OPS: &[(&str, &[&str])] = &[("asyncpg", &["fetch", "fetchrow", "fetchval", "create_pool"])];

/// Whether a database driver's operation reaches the database: one of DB_OPS, or one of the
/// driver's own.
pub fn reaches_the_database(pkg: &str, op: &str) -> bool {
    DB_OPS.contains(&op) || DRIVER_OPS.iter().any(|(p, ops)| *p == pkg && ops.contains(&op))
}

/// A graph database driver's managed transactions, by the way their work goes.
pub const TRANSACTIONS: &[(&str, Dir)] =
    &[("execute_read", Dir::Read), ("read_transaction", Dir::Read), ("execute_write", Dir::Write), ("write_transaction", Dir::Write)];

/// The access modes a graph database's driver is told, by the way they go.
pub const ACCESS_MODES: &[(&str, Dir)] = &[
    ("READ", Dir::Read),
    ("R", Dir::Read),
    ("READ_ACCESS", Dir::Read),
    ("WRITE", Dir::Write),
    ("W", Dir::Write),
    ("WRITE_ACCESS", Dir::Write),
];

/// A key-value store's operations, by the start of their name and the way they go: a key's,
/// and those of a hash, a list, a set and a sorted set kept under one.
pub const KV_READS: &[&str] = &[
    "get",
    "mget",
    "hget",
    "hmget",
    "hkeys",
    "hvals",
    "keys",
    "scan",
    "exists",
    "lrange",
    "llen",
    "smembers",
    "sismember",
    "scard",
    "zrange",
    "zscore",
];
pub const KV_WRITES: &[&str] = &[
    "set", "mset", "hset", "hdel", "hincr", "del", "remove", "clear", "incr", "decr", "expire", "lpush", "rpush", "sadd", "srem", "zadd",
    "zrem",
];

/// Libraries that compute in memory: most of their calls stay in the program (`torch.cat`,
/// `nn.Linear`); OPERATIONS and the Hub's tables name the few that cross to a place.
pub const ML_PACKAGES: &[&str] = &["sentence_transformers", "torch", "transformers", "huggingface_hub", "datasets", "torchvision"];

/// The Hugging Face Hub: its libraries, what comes down, what goes up, and the weights a
/// pretrained model downloads by name (`AutoModel.from_pretrained(name)`).
pub const HUB_PACKAGES: &[&str] = &["huggingface_hub", "datasets"];
pub const HUB_READ: &[&str] =
    &["snapshot_download", "hf_hub_download", "load_dataset", "repo_info", "dataset_info", "model_info", "list_repo_files"];
pub const HUB_WRITE: &[&str] =
    &["upload_folder", "upload_file", "upload_large_folder", "create_repo", "create_commit", "delete_file", "delete_folder", "push_to_hub"];
pub const HUB_MODELS: &[&str] = &["from_pretrained", "SentenceTransformer", "CrossEncoder", "pipeline"];
pub const HUB_HOST: &str = "huggingface.co";

/// A model library's hub, as a module of it: `torch.hub.load` downloads.
pub const MODEL_HUBS: &[(&str, &str, &str)] = &[("torch", "hub", "load")];

/// The datasets library's builtin loaders: a dataset made by one of them comes from local files.
pub const HF_LOADERS: &[&str] =
    &["csv", "json", "parquet", "text", "arrow", "pandas", "imagefolder", "audiofolder", "videofolder", "webdataset", "xml", "sql"];

/// A library's operations that cross to a place by themselves, by package and operation: what
/// prints, what shows or draws a figure, what runs a model, what saves or loads a file, what runs
/// another program, and what reads or writes a database through the connection it is handed.
pub const OPERATIONS: &[(&str, &str, Crossing)] = &[
    ("click", "echo", Crossing::to(World::Screen)),
    ("click", "secho", Crossing::to(World::Screen)),
    ("typer", "echo", Crossing::to(World::Screen)),
    ("typer", "secho", Crossing::to(World::Screen)),
    ("rich", "print", Crossing::to(World::Screen)),
    ("pprint", "pprint", Crossing::to(World::Screen)),
    ("pprint", "pp", Crossing::to(World::Screen)),
    ("matplotlib", "show", Crossing::to(World::Screen)),
    ("plotly", "show", Crossing::to(World::Screen)),
    // what the human types: a prompt, a password, a key in a terminal window
    ("getpass", "getpass", Crossing::read(World::Screen)),
    ("click", "prompt", Crossing::read(World::Screen)),
    ("click", "confirm", Crossing::read(World::Screen)),
    ("typer", "prompt", Crossing::read(World::Screen)),
    ("typer", "confirm", Crossing::read(World::Screen)),
    ("readline", "question", Crossing::read(World::Screen)),
    ("curses", "getch", Crossing::read(World::Screen)),
    ("curses", "getkey", Crossing::read(World::Screen)),
    ("curses", "getstr", Crossing::read(World::Screen)),
    ("curses", "get_wch", Crossing::read(World::Screen)),
    // what a terminal window shows
    ("curses", "addstr", Crossing::to(World::Screen)),
    ("curses", "addnstr", Crossing::to(World::Screen)),
    ("curses", "addch", Crossing::to(World::Screen)),
    ("curses", "insstr", Crossing::to(World::Screen)),
    ("matplotlib", "figure", Crossing::to(World::Figure)),
    ("matplotlib", "subplots", Crossing::to(World::Figure)),
    ("matplotlib", "subplot_mosaic", Crossing::to(World::Figure)),
    // what runs a model on its input
    ("sentence_transformers", "encode", Crossing::to(World::Model)),
    ("sentence_transformers", "embed", Crossing::to(World::Model)),
    ("transformers", "generate", Crossing::to(World::Model)),
    // what saves or loads weights, a checkpoint or a dataset, or sends them to the Hub
    ("transformers", "save_pretrained", Crossing::write(World::Disk)),
    ("sentence_transformers", "save_pretrained", Crossing::write(World::Disk)),
    ("transformers", "push_to_hub", Crossing::to(World::Net)),
    ("torch", "load", Crossing::read(World::Disk)),
    ("torch", "load_from_disk", Crossing::read(World::Disk)),
    ("datasets", "load", Crossing::read(World::Disk)),
    ("datasets", "load_from_disk", Crossing::read(World::Disk)),
    ("torch", "save", Crossing::write(World::Disk)),
    ("torch", "save_to_disk", Crossing::write(World::Disk)),
    ("datasets", "save", Crossing::write(World::Disk)),
    ("datasets", "save_to_disk", Crossing::write(World::Disk)),
    // what runs another program: a command line, a notebook in a kernel
    ("os", "system", Crossing::to(PROCESS)),
    ("asyncio", "create_subprocess_exec", Crossing::to(PROCESS)),
    ("asyncio", "create_subprocess_shell", Crossing::to(PROCESS)),
    ("nbclient", "execute", Crossing::to(PROCESS)),
    // a frame read from a database by a statement or a table's name, and written to a table
    ("pandas", "read_sql", Crossing::read(SQL)),
    ("pandas", "read_sql_query", Crossing::read(SQL)),
    ("pandas", "read_sql_table", Crossing::read(SQL)),
    ("pandas", "to_sql", Crossing::write(SQL)),
];

/// What a package's operation crosses by itself.
pub fn operation(pkg: &str, op: &str) -> Option<Crossing> {
    OPERATIONS.iter().find(|(p, o, _)| *p == pkg && *o == op).map(|(_, _, c)| *c)
}

/// The functions that end the program where they are called, as an error no one catches does:
/// by the module that holds them and their name, Python's `sys.exit` and `os._exit`, Node's
/// `process.exit`, Rust's `std::process::exit`.
pub const ENDS_THE_RUN: &[(&str, &str)] = &[("sys", "exit"), ("os", "_exit"), ("os", "abort"), ("process", "exit"), ("process", "abort")];

/// Whether the function `name` of the module `module` ends the program.
pub fn ends_the_run(module: &str, name: &str) -> bool {
    ENDS_THE_RUN.contains(&(module, name))
}

/// A program's streams that a write to shows the human: `sys.stdout`, `process.stderr`, Rust's
/// `io::stderr()`.
pub const STREAMS: &[(&str, &str)] =
    &[("sys", "stdout"), ("sys", "stderr"), ("process", "stdout"), ("process", "stderr"), ("io", "stdout"), ("io", "stderr")];

/// A program's streams that read what the human types: `sys.stdin`, `process.stdin`, Rust's
/// `io::stdin()`.
pub const INPUT_STREAMS: &[(&str, &str)] = &[("sys", "stdin"), ("process", "stdin"), ("io", "stdin")];

/// The keyword a print is handed a file by: such a print writes that file, not the screen.
pub const PRINT_TO: &str = "file";

/// Python's own modules whose calls these tables describe: `print` and `open`, a stream.
pub const PYTHON_OWN: &[&str] = &["builtins", "sys"];

/// Python's builtins that cross: by name.
pub const PYTHON_BUILTINS: &[(&str, Crossing)] =
    &[("print", Crossing::to(World::Screen)), ("input", Crossing::read(World::Screen)), ("open", Crossing::to(World::Disk))];

// ---------- files ----------

/// Libraries whose calls read or write files: data formats, archives, arrays, images.
pub const FILE_PACKAGES: &[&str] = &[
    "json",
    "yaml",
    "pickle",
    "csv",
    "tarfile",
    "zipfile",
    "shutil",
    "os",
    "pathlib",
    "importlib",
    "pandas",
    "geopandas",
    "numpy",
    "rasterio",
    "lxml",
    "PIL",
    "matplotlib",
    "ruamel",
];

/// What such a library's operation does with a file, by its name: a path's tests of what is
/// there (`exists`) among the reads, as in every language.
pub const FILE_READS: &[&str] = &[
    "exists",
    "is_file",
    "is_dir",
    "stat",
    "read_text",
    "read_bytes",
    "read_csv",
    "read_parquet",
    "read_file",
    "read_json",
    "read_excel",
    "iterdir",
    "glob",
    "rglob",
    "load",
    "load_workbook",
    "parse",
    "fromfile",
    "imread",
    "open_dataset",
    "read",
    "readlines",
    "safe_load",
    "listdir",
    "walk",
    "open",
];
pub const FILE_WRITES: &[&str] = &[
    "write_text",
    "write_bytes",
    "to_csv",
    "to_parquet",
    "to_json",
    "dump",
    "safe_dump",
    "savefig",
    "save",
    "mkdir",
    "copy",
    "copyfile",
    "copytree",
    "move",
    "unlink",
    "rmtree",
    "touch",
    "imwrite",
    "extractall",
    "makedirs",
    "rename",
    "replace",
    "writelines",
];

/// Writes that move or remove files rather than store data, and the libraries of the file system
/// that alone do them: a frame's `copy()` or `replace(..)` stays in memory.
pub const FILE_SYSTEM_OPS: &[&str] =
    &["copy", "copyfile", "copytree", "move", "unlink", "rmtree", "rename", "replace", "mkdir", "makedirs", "touch"];
pub const FILE_SYSTEM_PACKAGES: &[&str] = &["shutil", "os", "pathlib"];

/// A path's methods whose place is the path they are called on; their argument is a pattern
/// below it or what is written: `cache_dir.glob("*.arrow")`, `p.write_text("done")`.
pub const ON_THE_PATH: &[&str] = &[
    "glob",
    "rglob",
    "iterdir",
    "joinpath",
    "read_text",
    "read_bytes",
    "write_text",
    "write_bytes",
    "exists",
    "is_file",
    "is_dir",
    "mkdir",
    "unlink",
    "rmdir",
    "touch",
    "stat",
];

/// The format an operation or a library implies when the code names no file.
pub const FORMATS_BY_OP: &[(&str, &str)] = &[
    ("read_csv", "csv"),
    ("to_csv", "csv"),
    ("read_parquet", "parquet"),
    ("to_parquet", "parquet"),
    ("read_json", "json"),
    ("to_json", "json"),
    ("read_excel", "xlsx"),
    ("to_excel", "xlsx"),
    ("load_workbook", "xlsx"),
    ("read_feather", "feather"),
    ("to_feather", "feather"),
    ("savefig", "png"),
    ("imsave", "png"),
    ("safe_load", "yaml"),
    ("safe_dump", "yaml"),
];
pub const FORMATS_BY_PACKAGE: &[(&str, &str)] = &[
    ("json", "json"),
    ("yaml", "yaml"),
    ("ruamel", "yaml"),
    ("pickle", "pkl"),
    ("csv", "csv"),
    ("toml", "toml"),
    ("tomllib", "toml"),
    ("tomli", "toml"),
    ("lxml", "xml"),
    ("rasterio", "tif"),
];
/// numpy's own format, for what it saves and loads.
pub const ARRAY_FORMAT: (&str, &[&str], &str) = ("numpy", &["save", "load"], "npy");

/// What a file operation does to the file system, by its name in the languages Atlas reads: the
/// operations that look at a path's shape, those that list a directory, and the starts of the
/// names of those that create, remove, move and copy.
pub const PROBES: &[&str] = &[
    "exists",
    "try_exists",
    "is_file",
    "is_dir",
    "is_symlink",
    "canonicalize",
    "metadata",
    "symlink_metadata",
    "existsSync",
    "stat",
    "statSync",
    "access",
];
pub const LISTINGS: &[&str] =
    &["read_dir", "iterdir", "glob", "rglob", "listdir", "scandir", "walk", "readdir", "readdirSync", "opendir", "opendirSync"];
pub const CREATES: &[&str] = &["create_dir", "mkdir", "makedirs", "touch"];
pub const REMOVES: &[&str] = &["remove", "unlink", "rmtree", "rm"];
pub const MOVES: &[&str] = &["rename", "move", "replace"];
pub const COPIES: &[&str] = &["copy", "cp", "cpSync"];

/// Rust's: std's file system operations that read and that write, `Path`'s methods that read the
/// disk when called on a value whose type is unknown, the crates that read a tree of files, the
/// type that runs a program, and the macros that print.
pub const RUST_FS_READS: &[&str] = &[
    "read",
    "read_to_string",
    "read_to_end",
    "read_exact",
    "read_line",
    "read_dir",
    "metadata",
    "canonicalize",
    "exists",
    "open",
    "read_link",
];
pub const RUST_FS_WRITES: &[&str] = &[
    "write",
    "write_all",
    "write_fmt",
    "set_len",
    "sync_all",
    "create",
    "create_dir",
    "create_dir_all",
    "remove_file",
    "remove_dir",
    "remove_dir_all",
    "rename",
    "copy",
    "set_permissions",
];
pub const RUST_PATH_READS: &[&str] = &["exists", "try_exists", "is_file", "is_dir", "read_dir", "canonicalize", "symlink_metadata"];
pub const RUST_FILE_TREES: &[&str] = &["walkdir", "WalkDir", "ignore", "glob", "notify", "git2", "gix"];
/// A crate's type whose function opens the file at the path it is handed first, and which way:
/// csv's readers and writers, plotters' image backends.
pub const RUST_FILE_OPENERS: &[(&str, &str, Dir)] = &[
    ("ReaderBuilder", "from_path", Dir::Read),
    ("Reader", "from_path", Dir::Read),
    ("WriterBuilder", "from_path", Dir::Write),
    ("Writer", "from_path", Dir::Write),
    ("BitMapBackend", "new", Dir::Write),
    ("SVGBackend", "new", Dir::Write),
];
pub const RUST_COMMAND: &str = "Command";
pub const RUST_SCREEN_MACROS: &[&str] = &["println", "print", "eprintln", "eprint"];

/// Rust's macros whose arguments after the first are a pattern, not values: `Some(_)` in
/// `matches!(x, Some(_))` builds nothing.
pub const RUST_PATTERN_MACROS: &[&str] = &["matches", "assert_matches", "debug_assert_matches"];

/// The library types whose value is one of their type arguments, each a variant, by the name Rust
/// code writes them with: std's `Result<T, E>` is a `T` or an `E`, either's `Either<L, R>` an `L`
/// or an `R`. An `Option<T>` is no sum here: it holds a `T` or nothing, and an argument that may be
/// left out is a door like any other, not a choice between outcomes made elsewhere.
pub const RUST_SUMS: &[&str] = &["Result", "Either"];

/// A Rust type whose value changes through a shared reference (interior mutability), so that a
/// `static` of it is a value its functions change: its names, the methods that change it, and
/// those that give a guard on what it holds, through which that changes.
pub struct Interior {
    pub types: &'static [&'static str],
    pub changes: &'static [&'static str],
    pub guards: &'static [&'static str],
}

/// Rust's types that change in place: std's and parking_lot's locks, std's cells (a thread-local's
/// key changes its cell by the cell's own methods and hands out a guard in `with_borrow_mut`), the
/// atomics, and the cells set once (std's and once_cell's `OnceLock` and `OnceCell`), which
/// `get_or_init` only reads. A cell made at its first use (`LazyLock`, once_cell's and
/// lazy_static's `Lazy`) changes through what it holds: `LazyLock<Mutex<..>>`.
pub const RUST_INTERIOR: &[Interior] = &[
    Interior { types: &["Mutex"], changes: &[], guards: &["lock", "try_lock"] },
    Interior { types: &["RwLock"], changes: &[], guards: &["write", "try_write"] },
    Interior {
        types: &["RefCell"],
        changes: &["set", "replace", "replace_with", "swap", "take"],
        guards: &["borrow_mut", "try_borrow_mut", "with_borrow_mut"],
    },
    Interior { types: &["Cell"], changes: &["set", "replace", "swap", "take", "update"], guards: &[] },
    Interior {
        types: &[
            "AtomicBool",
            "AtomicI8",
            "AtomicI16",
            "AtomicI32",
            "AtomicI64",
            "AtomicIsize",
            "AtomicU8",
            "AtomicU16",
            "AtomicU32",
            "AtomicU64",
            "AtomicUsize",
            "AtomicPtr",
        ],
        changes: &[
            "store",
            "swap",
            "fetch_add",
            "fetch_sub",
            "fetch_and",
            "fetch_nand",
            "fetch_or",
            "fetch_xor",
            "fetch_max",
            "fetch_min",
            "fetch_not",
            "fetch_update",
            "compare_exchange",
            "compare_exchange_weak",
        ],
        guards: &[],
    },
    Interior { types: &["OnceLock", "OnceCell"], changes: &["set"], guards: &[] },
];

/// The methods that change the value a guard gives: those of std's collections, `String` and
/// `Option` that take `&mut self`.
pub const RUST_GUARDED_CHANGES: &[&str] = &[
    "push",
    "push_str",
    "push_back",
    "push_front",
    "pop",
    "pop_back",
    "pop_front",
    "pop_first",
    "pop_last",
    "insert",
    "insert_str",
    "extend",
    "extend_from_slice",
    "append",
    "remove",
    "remove_entry",
    "swap_remove",
    "clear",
    "truncate",
    "retain",
    "retain_mut",
    "drain",
    "splice",
    "split_off",
    "dedup",
    "dedup_by",
    "dedup_by_key",
    "sort",
    "sort_by",
    "sort_by_key",
    "sort_unstable",
    "sort_unstable_by",
    "sort_unstable_by_key",
    "reverse",
    "resize",
    "resize_with",
    "fill",
    "swap",
    "entry",
    "get_mut",
    "first_mut",
    "last_mut",
    "iter_mut",
    "values_mut",
    "take",
    "replace",
    "get_or_insert",
    "get_or_insert_with",
];

/// Whether a Rust type of this name changes in place.
pub fn rust_interior(ty: &str) -> bool {
    RUST_INTERIOR.iter().any(|i| i.types.contains(&ty))
}

/// Whether a method changes the value it is called on: a type's that changes in place, or, on
/// what a guard gives, one that takes `&mut self`.
pub fn rust_changes(method: &str, guarded: bool) -> bool {
    RUST_INTERIOR.iter().any(|i| i.changes.contains(&method)) || (guarded && RUST_GUARDED_CHANGES.contains(&method))
}

/// Whether a method gives a guard through which what a value holds changes.
pub fn rust_guards(method: &str) -> bool {
    RUST_INTERIOR.iter().any(|i| i.guards.contains(&method))
}

/// The JavaScript platform's: `node:fs`'s packages and what of them reads and writes, the global
/// objects a name may be reached through, the functions and constructors that go to the web, the
/// object that prints, the browser's key-value stores and their operations, and the names a
/// package's client is made by.
pub const NODE_FS: &[&str] = &["fs", "fs/promises"];
pub const NODE_FS_READS: &[&str] =
    &["readFile", "readFileSync", "readdir", "readdirSync", "createReadStream", "existsSync", "stat", "statSync", "access"];
pub const NODE_FS_WRITES: &[&str] = &[
    "writeFile",
    "writeFileSync",
    "appendFile",
    "appendFileSync",
    "mkdir",
    "mkdirSync",
    "rm",
    "rmSync",
    "unlink",
    "unlinkSync",
    "rename",
    "renameSync",
    "copyFile",
    "copyFileSync",
    "createWriteStream",
];
pub const GLOBAL_OBJECTS: &[&str] = &["window", "globalThis"];
pub const WEB_GLOBALS: &[&str] = &["fetch", "WebSocket", "EventSource"];
pub const CONSOLE: &str = "console";
pub const BROWSER_STORES: &[&str] = &["localStorage", "sessionStorage"];
pub const BROWSER_STORE_OPS: &[&str] = &["getItem", "setItem", "removeItem", "clear"];
pub const CLIENT_MAKERS: &[&str] = &["create", "createClient"];

/// Parts of the world a library reaches only through a client the code made from it: its other
/// functions are helpers (a language model's SDK formats a file or a schema without asking the
/// model anything).
pub const THROUGH_A_CLIENT: &[World] = &[World::Llm];

/// Rust's std module whose functions read and write files.
pub const RUST_FS_MODULE: &str = "fs";

/// The datasets library's function that loads a dataset, from the Hub or by a builtin loader.
pub const HUB_LOADER: &str = "load_dataset";

// ---------- paths ----------

/// The constructors a path is made by from its parts: pathlib's, Rust's `Path::new` and
/// `PathBuf::from`, the web platform's `URL`.
pub const PATH_MAKERS: &[&str] =
    &["Path", "PurePath", "PosixPath", "WindowsPath", "PurePosixPath", "PureWindowsPath", "Path::new", "PathBuf::from", "URL", "new URL"];

/// Libraries whose values are paths: a method of one names its file by the path it is called on.
/// importlib's are what a package ships (`files("pages").joinpath("template.html").read_text()`).
pub const PATH_LIBRARIES: &[&str] = &["pathlib", "importlib"];

/// The functions that give the directory a package ships its files in, handed the package by its
/// name, by the library's module and the function's name: `importlib.resources.files("pages")`.
/// What is read from there is part of the package.
pub const PACKAGE_FILES: &[(&str, &str)] = &[("importlib.resources", "files"), ("importlib_resources", "files")];

/// A path's methods that give back the path they are called on, written another way.
pub const SAME_PATH: &[&str] = &["resolve", "absolute", "expanduser", "as_posix", "__fspath__"];

/// A path's methods that give the paths below the one they are called on that match a pattern,
/// with what the pattern starts with: `glob("*/IO-*")` is `*/IO-*` below it, `rglob("*.gml")` is
/// `**/*.gml`.
pub const PATH_PATTERNS: &[(&str, &str)] = &[("glob", ""), ("rglob", "**/")];

/// Functions that give back the elements of the collection they are handed, in another order or
/// kind: `sorted(paths)` holds the paths `paths` holds.
pub const SAME_ELEMENTS: &[&str] = &["sorted", "list", "set", "tuple", "frozenset", "reversed", "iter", "fromkeys"];

/// What a path library's function makes of what it is handed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PathFn {
    /// Its arguments, one below the other.
    Join,
    /// The directory its first argument is in.
    Parent,
    /// Its first argument, as it is.
    Same,
    /// The user's home directory.
    Home,
    /// The directory the program runs in.
    Here,
    /// The system's directory for temporary files.
    Temp,
    /// What the environment holds for the variable its first argument names.
    Environment,
}

/// A command-line parser whose parsed values are attributes of one object (`args.out`): its
/// library, the object's class, the method that declares an argument, that method's keywords
/// for the argument's default and for the attribute's name, and the methods that parse the
/// command line into the object.
pub struct ArgParser {
    pub package: &'static str,
    pub namespace: &'static str,
    pub declares: &'static str,
    pub default: &'static str,
    pub dest: &'static str,
    pub parses: &'static [&'static str],
}

pub const ARG_PARSERS: &[ArgParser] = &[ArgParser {
    package: "argparse",
    namespace: "Namespace",
    declares: "add_argument",
    default: "default",
    dest: "dest",
    parses: &["parse_args", "parse_known_args", "parse_intermixed_args", "parse_known_intermixed_args"],
}];

/// A command-line parser handed every option at once, in an object, which gives their values as
/// the fields of one object (`values.out`): its package, its function, the key its options stand
/// under, an option's key for its default, and the field of what it gives that holds the values.
pub struct OptionParser {
    pub package: &'static str,
    pub function: &'static str,
    pub options: &'static str,
    pub default: &'static str,
    pub values: &'static str,
}

pub const OPTION_PARSERS: &[OptionParser] =
    &[OptionParser { package: "util", function: "parseArgs", options: "options", default: "default", values: "values" }];

/// A command-line parser derived from a type, whose values are the fields of that type
/// (`cli.raw`): its package, the derives that make a type one, the attributes that declare a
/// field's option, an option's keys for its default, the functions the derive defines that parse
/// the command line the program is run with, the keys that name a field's option long (`--raw`)
/// and short (`-r`) where a field without either is a positional argument, the keys that make a
/// field no option of the command (a subcommand, a field the parser skips), and the key that adds
/// the options of the field's own type.
pub struct DerivedParser {
    pub package: &'static str,
    pub derives: &'static [&'static str],
    pub attributes: &'static [&'static str],
    pub defaults: &'static [&'static str],
    pub reads: &'static [&'static str],
    pub long: &'static str,
    pub short: &'static str,
    pub not_options: &'static [&'static str],
    pub flatten: &'static str,
}

/// The functions that give the command line the program is run with, word by word, unparsed: what
/// they give is the entry's, whatever the program makes of it.
pub const RUST_COMMAND_LINE: &[&[&str]] = &[&["std", "env", "args"], &["std", "env", "args_os"]];

/// clap's derive: `#[derive(Parser)] struct Cli { #[arg(long, default_value = RAW)] raw: PathBuf }`,
/// with the options a struct of `Args` or a variant of `Subcommand` adds to it.
pub const DERIVED_PARSERS: &[DerivedParser] = &[DerivedParser {
    package: "clap",
    derives: &["Parser", "Args", "Subcommand"],
    attributes: &["arg", "clap", "command"],
    defaults: &["default_value", "default_value_t", "default_value_os"],
    reads: &["parse", "try_parse"],
    long: "long",
    short: "short",
    not_options: &["subcommand", "skip"],
    flatten: "flatten",
}];

/// How a path under the system's directory for temporary files is written: a place of its own,
/// as `~` is the user's home.
pub const TEMP_DIR: &str = "$TMPDIR";

/// Path functions, by the package or global object they are on and their name.
pub const PATH_FUNCTIONS: &[(&str, &str, PathFn)] = &[
    ("path", "join", PathFn::Join),
    ("path", "resolve", PathFn::Join),
    ("path", "dirname", PathFn::Parent),
    ("url", "fileURLToPath", PathFn::Same),
    ("os", "homedir", PathFn::Home),
    ("os", "tmpdir", PathFn::Temp),
    ("process", "cwd", PathFn::Here),
];

/// Rust's path functions, by the type or module they are on and their name.
pub const RUST_PATH_FUNCTIONS: &[(&str, &str, PathFn)] = &[
    ("Path", "new", PathFn::Same),
    ("PathBuf", "from", PathFn::Same),
    ("OsStr", "new", PathFn::Same),
    ("OsString", "from", PathFn::Same),
    ("String", "from", PathFn::Same),
    ("env", "current_dir", PathFn::Here),
    ("env", "home_dir", PathFn::Home),
    ("dirs", "home_dir", PathFn::Home),
    ("env", "temp_dir", PathFn::Temp),
    ("env", "var", PathFn::Environment),
    ("env", "var_os", PathFn::Environment),
];

/// The macros that give, where the program is built, the directory its crate's manifest is in,
/// by the macro and the variable Cargo sets: `env!("CARGO_MANIFEST_DIR")`. Read at run time, it
/// names the source directory of the machine the program was built on.
pub const RUST_BUILD_DIRS: &[(&str, &str)] = &[("env", "CARGO_MANIFEST_DIR")];

/// What a method of a path makes of the path it is called on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PathMethod {
    /// The path with its arguments below it.
    Join,
    /// The directory the path is in.
    Parent,
    /// The path, written another way.
    Same,
    /// The path with its first argument as its extension.
    Extension,
    /// Its first argument, in the directory the path is in.
    Sibling,
}

/// Rust's methods of a path and of a string, by name.
pub const RUST_PATH_METHODS: &[(&str, PathMethod)] = &[
    ("join", PathMethod::Join),
    ("parent", PathMethod::Parent),
    ("with_extension", PathMethod::Extension),
    ("with_file_name", PathMethod::Sibling),
    ("to_path_buf", PathMethod::Same),
    ("as_path", PathMethod::Same),
    ("into_os_string", PathMethod::Same),
    ("canonicalize", PathMethod::Same),
    ("display", PathMethod::Same),
    ("to_string_lossy", PathMethod::Same),
    ("to_string", PathMethod::Same),
    ("to_owned", PathMethod::Same),
    ("as_str", PathMethod::Same),
    ("as_ref", PathMethod::Same),
    ("clone", PathMethod::Same),
    ("into", PathMethod::Same),
];

// ---------- a language's own types ----------

/// The types a language's code describes itself with rather than holds data in: its standard
/// library and the packages that describe code (a UI library's components, a schema library's
/// validators), and the names that wrap or stand for a value.
pub struct OwnTypes {
    pub packages: &'static [&'static str],
    pub names: &'static [&'static str],
}

/// Python's: the standard library's modules that describe code, not data.
pub const PYTHON_OWN_TYPES: OwnTypes = OwnTypes {
    packages: &[
        "typing",
        "collections",
        "pathlib",
        "builtins",
        "datetime",
        "abc",
        "os",
        "enum",
        "dataclasses",
        "functools",
        "itertools",
        "types",
    ],
    names: &[],
};

/// Rust's: std, core and alloc, the containers and wrappers, and the associated types a trait
/// names (`Self::Target`, `S::Ok`).
pub const RUST_OWN_TYPES: OwnTypes = OwnTypes {
    packages: &["std", "core", "alloc"],
    names: &[
        "Result", "Option", "Vec", "Box", "HashMap", "HashSet", "BTreeMap", "String", "Path", "PathBuf", "Ok", "Err", "Error", "Target",
        "Output", "Item", "Self",
    ],
};

/// TypeScript's: a UI framework's components and props, a schema library's schemas.
pub const TYPESCRIPT_OWN_TYPES: OwnTypes = OwnTypes {
    packages: &["react", "react-dom", "preact", "vue", "svelte", "solid-js", "zod", "yup", "valibot", "superstruct", "io-ts"],
    names: &[],
};

// ---------- what a language binds without an import ----------

/// Python's module of the names every module has without an import (`len`, `isinstance`), as ty
/// names where they are defined.
pub const PYTHON_BUILTINS_MODULE: &str = "builtins";

/// Rust's sequences: a loop over one (`for r in &rows`, `for r in rows.iter()`) goes over its
/// elements, as does one over a slice or an array.
pub const RUST_SEQUENCES: &[&str] = &["Vec", "VecDeque", "HashSet", "BTreeSet", "BinaryHeap", "LinkedList"];

/// The methods that go over a sequence's elements, by reference or by value.
pub const RUST_ELEMENTS: &[&str] = &["iter", "iter_mut", "into_iter", "drain"];

/// The adaptors of an iterator that keep its elements as they are, fewer or in another order.
pub const RUST_KEEPS_ELEMENTS: &[&str] =
    &["filter", "skip", "take", "rev", "skip_while", "take_while", "peekable", "inspect", "step_by", "by_ref"];

/// The crate the prelude's types are of: `Vec` and `String` are `std`'s.
pub const RUST_PRELUDE_CRATE: &str = "std";

/// Rust's names in scope without a `use`: the prelude's types, variants, traits and `drop`, and
/// the primitive types (`u32::from`). Its macros (`println!`, `vec!`) are not calls.
pub const RUST_PRELUDE: &[&str] = &[
    "Box",
    "Vec",
    "String",
    "Option",
    "Some",
    "None",
    "Result",
    "Ok",
    "Err",
    "drop",
    "Copy",
    "Send",
    "Sized",
    "Sync",
    "Unpin",
    "Drop",
    "Fn",
    "FnMut",
    "FnOnce",
    "AsMut",
    "AsRef",
    "From",
    "Into",
    "DoubleEndedIterator",
    "ExactSizeIterator",
    "Extend",
    "IntoIterator",
    "Iterator",
    "Clone",
    "Default",
    "Eq",
    "Ord",
    "PartialEq",
    "PartialOrd",
    "ToOwned",
    "ToString",
    "TryFrom",
    "TryInto",
    "FromIterator",
    "Future",
    "IntoFuture",
    "bool",
    "char",
    "str",
    "u8",
    "u16",
    "u32",
    "u64",
    "u128",
    "usize",
    "i8",
    "i16",
    "i32",
    "i64",
    "i128",
    "isize",
    "f32",
    "f64",
];

/// TypeScript's names in scope without an import: the language's global objects and functions,
/// and those of the hosts it runs in (a browser, Node).
pub const TYPESCRIPT_GLOBALS: &[&str] = &[
    "globalThis",
    "eval",
    "isFinite",
    "isNaN",
    "parseFloat",
    "parseInt",
    "decodeURI",
    "decodeURIComponent",
    "encodeURI",
    "encodeURIComponent",
    "Object",
    "Function",
    "Boolean",
    "Symbol",
    "Error",
    "AggregateError",
    "EvalError",
    "RangeError",
    "ReferenceError",
    "SyntaxError",
    "TypeError",
    "URIError",
    "Number",
    "BigInt",
    "Math",
    "Date",
    "String",
    "RegExp",
    "Array",
    "Int8Array",
    "Uint8Array",
    "Uint8ClampedArray",
    "Int16Array",
    "Uint16Array",
    "Int32Array",
    "Uint32Array",
    "Float32Array",
    "Float64Array",
    "BigInt64Array",
    "BigUint64Array",
    "Map",
    "Set",
    "WeakMap",
    "WeakSet",
    "WeakRef",
    "FinalizationRegistry",
    "ArrayBuffer",
    "SharedArrayBuffer",
    "DataView",
    "Atomics",
    "JSON",
    "Promise",
    "Reflect",
    "Proxy",
    "Intl",
    "structuredClone",
    "console",
    "window",
    "self",
    "document",
    "navigator",
    "location",
    "history",
    "localStorage",
    "sessionStorage",
    "indexedDB",
    "caches",
    "crypto",
    "performance",
    "customElements",
    "fetch",
    "Request",
    "Response",
    "Headers",
    "URL",
    "URLSearchParams",
    "FormData",
    "Blob",
    "File",
    "FileReader",
    "TextEncoder",
    "TextDecoder",
    "AbortController",
    "AbortSignal",
    "Event",
    "CustomEvent",
    "EventTarget",
    "EventSource",
    "WebSocket",
    "Worker",
    "Image",
    "Audio",
    "atob",
    "btoa",
    "setTimeout",
    "clearTimeout",
    "setInterval",
    "clearInterval",
    "queueMicrotask",
    "requestAnimationFrame",
    "cancelAnimationFrame",
    "alert",
    "confirm",
    "prompt",
    "getComputedStyle",
    "matchMedia",
    "ResizeObserver",
    "IntersectionObserver",
    "MutationObserver",
    "process",
    "Buffer",
    "require",
    "module",
    "exports",
    "global",
    "__dirname",
    "__filename",
    "setImmediate",
    "clearImmediate",
];
