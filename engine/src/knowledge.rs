//! What Atlas knows of the languages, libraries and frameworks code uses, as data: one table per
//! kind of fact. A reader looks a fact up here; it never tests a library's name in its own
//! control flow.

// ---------- the web ----------

/// The methods a request goes by.
pub const HTTP_METHODS: &[&str] = &["get", "post", "put", "patch", "delete", "head", "options"];

/// What else an app or a router registers a route by, beside an HTTP method: a websocket
/// (FastAPI), any method (Express's `all`).
pub const ROUTE_KINDS: &[&str] = &["websocket", "all"];

/// Whether an app's or a router's method of this name registers a route.
pub fn registers_route(verb: &str) -> bool {
    HTTP_METHODS.contains(&verb) || ROUTE_KINDS.contains(&verb)
}

/// Where a call hands an argument: by position, or by keyword.
pub enum Arg {
    At(usize),
    Named(&'static str),
}

/// How an app takes another app's or a router's routes under a prefix: the method, and where it
/// is handed the prefix and what it mounts. A prefix handed by position must be there; one by
/// keyword may be left out.
pub struct Mount {
    pub verb: &'static str,
    pub prefix: Arg,
    pub child: Arg,
}

pub const MOUNTS: &[Mount] = &[
    // Starlette
    Mount { verb: "mount", prefix: Arg::At(0), child: Arg::At(1) },
    // FastAPI
    Mount { verb: "include_router", prefix: Arg::Named("prefix"), child: Arg::At(0) },
    // Express
    Mount { verb: "use", prefix: Arg::At(0), child: Arg::At(1) },
    // Hono
    Mount { verb: "route", prefix: Arg::At(0), child: Arg::At(1) },
];

/// Routers that carry their own prefix, and the keyword they are handed it by: FastAPI's
/// `APIRouter(prefix=..)`.
pub const PREFIXED_ROUTERS: &[(&str, &str)] = &[("APIRouter", "prefix")];

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

// ---------- the files of a project ----------

/// Directories a tool made rather than the author: dependencies, environments, build output.
pub const MADE_DIRS: &[&str] = &[
    "node_modules", "target", "dist", "build", "out", "coverage", "venv", "env", "site-packages", "__pycache__", ".next", ".output", ".wrangler",
    ".vercel", ".turbo", "public",
];

/// Directories that hold tests and the data they read.
pub const TEST_DIRS: &[&str] = &["tests", "test", "__tests__", "fixtures", "testdata"];

// ---------- values ----------

/// Methods that hand on the value they are called on, opened or unchanged: the standard
/// library's `Result` and `Option`, anyhow's context. By crate.
pub const HANDS_ON: &[(&str, &[&str])] = &[
    ("std", &["unwrap", "expect", "unwrap_or", "unwrap_or_else", "unwrap_or_default", "ok", "err", "ok_or", "ok_or_else", "map", "map_err", "and_then", "or_else"]),
    ("anyhow", &["context", "with_context"]),
];

/// Whether a method of this name hands on the value it is called on.
pub fn hands_on(method: &str) -> bool {
    HANDS_ON.iter().any(|(_, ms)| ms.contains(&method))
}

/// Python's generic types whose parameters are the values they hold: `list[Graph]` holds
/// Graphs.
pub const PYTHON_HOLDERS: &[&str] = &[
    "list", "dict", "set", "tuple", "frozenset", "Iterable", "Iterator", "Sequence", "Mapping", "Generator", "AsyncIterator", "List", "Dict",
    "Tuple", "Set", "FrozenSet", "Deque", "deque", "DefaultDict", "defaultdict", "OrderedDict", "Counter", "ChainMap", "Collection", "Container",
    "AbstractSet", "MutableSet", "MutableSequence", "MutableMapping", "AsyncIterable", "AsyncGenerator", "Reversible",
];

/// Python's generic types whose value is one of their parameters: `Optional[Node]` is a Node or
/// nothing.
pub const PYTHON_ALTERNATIVES: &[&str] = &["Optional", "Union"];

/// Python's generic types whose parameters describe the value without being it:
/// `Annotated[T, ..]` is a `T` with notes, `Callable[[A], B]` takes an `A` and gives a `B`,
/// `type[T]` is the class itself.
pub const PYTHON_DESCRIBES: &[&str] = &[
    "type", "Type", "Callable", "Awaitable", "Coroutine", "Annotated", "ClassVar", "Final", "Required", "NotRequired", "ReadOnly", "TypeGuard", "TypeIs",
    "Unpack",
];

/// Whether a Python generic type's parameters are the values it holds or stands for.
pub fn python_container(name: &str) -> bool {
    PYTHON_HOLDERS.contains(&name) || PYTHON_ALTERNATIVES.contains(&name) || PYTHON_DESCRIBES.contains(&name)
}

/// TypeScript's generic types whose arguments are the values they hold or stand for:
/// `Promise<Regel[]>` hands on Regels.
pub const TYPESCRIPT_CONTAINERS: &[&str] = &[
    "Promise", "Array", "ReadonlyArray", "Set", "ReadonlySet", "Map", "ReadonlyMap", "Record", "Partial", "Required", "Readonly", "Pick", "Omit",
    "NonNullable", "Awaited", "Iterable", "AsyncIterable", "Generator", "AsyncGenerator",
];

/// Methods that change the value they are called on: a language's own containers', and a
/// library's (networkx's graphs). By language or package.
pub const MUTATORS: &[(&str, &[&str])] = &[
    ("python", &["append", "extend", "insert", "remove", "pop", "clear", "update", "add", "discard", "setdefault", "sort", "reverse", "popitem"]),
    ("networkx", &["add_node", "add_edge", "add_nodes_from", "add_edges_from", "remove_node", "remove_edge"]),
    ("typescript", &["push", "pop", "shift", "unshift", "splice", "sort", "reverse", "fill", "copyWithin", "set", "delete", "clear", "add"]),
];

/// The methods of these sources that change what they are called on.
pub fn mutators(sources: &[&str]) -> Vec<&'static str> {
    MUTATORS.iter().filter(|(s, _)| sources.contains(s)).flat_map(|(_, ms)| ms.iter().copied()).collect()
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
