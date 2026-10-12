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
/// library's (networkx's graphs). By registry and source.
pub const MUTATORS: &[(Registry, &str, &[&str])] = &[
    (Registry::PyPi, "builtins", &["append", "extend", "insert", "remove", "pop", "clear", "update", "add", "discard", "setdefault", "sort", "reverse", "popitem"]),
    (Registry::PyPi, "networkx", &["add_node", "add_edge", "add_nodes_from", "add_edges_from", "remove_node", "remove_edge"]),
    (Registry::Npm, "javascript", &["push", "pop", "shift", "unshift", "splice", "sort", "reverse", "fill", "copyWithin", "set", "delete", "clear", "add"]),
];

/// The methods that change what they are called on, in the code of one registry's languages.
pub fn mutators(from: Registry) -> Vec<&'static str> {
    MUTATORS.iter().filter(|(r, _, _)| *r == from).flat_map(|(_, _, ms)| ms.iter().copied()).collect()
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
    (PyPi, "requests", NET), (PyPi, "httpx", NET), (PyPi, "urllib", NET), (PyPi, "http", NET), (PyPi, "socket", NET), (PyPi, "aiohttp", NET),
    (Crates, "reqwest", NET), (Crates, "ureq", NET), (Crates, "hyper", NET), (Crates, "tungstenite", NET), (Crates, "tokio_tungstenite", NET),
    (Crates, "curl", NET), (Crates, "axum", NET), (Crates, "TcpStream", NET), (Crates, "TcpListener", NET),
    (Npm, "axios", NET), (Npm, "ky", NET), (Npm, "ofetch", NET), (Npm, "node-fetch", NET), (Npm, "undici", NET), (Npm, "got", NET), (Npm, "ws", NET),
    (PyPi, "openai", LLM), (PyPi, "anthropic", LLM),
    (Npm, "@anthropic-ai/sdk", LLM), (Npm, "openai", LLM), (Npm, "@google/generative-ai", LLM), (Npm, "@google/genai", LLM), (Npm, "@mistralai/mistralai", LLM), (Npm, "ollama", LLM),
    (PyPi, "duckdb", DUCKDB), (PyPi, "neo4j", GRAPH), (PyPi, "mgclient", GRAPH), (PyPi, "sqlite3", SQLITE), (PyPi, "psycopg2", SQL), (PyPi, "sqlalchemy", SQL),
    (Crates, "rusqlite", SQLITE), (Crates, "duckdb", DUCKDB), (Crates, "sqlx", SQL), (Crates, "diesel", SQL), (Crates, "postgres", SQL), (Crates, "tokio_postgres", SQL),
    (Crates, "sea_orm", SQL),
    (Npm, "pg", SQL), (Npm, "postgres", SQL), (Npm, "mysql2", SQL), (Npm, "mysql", SQL), (Npm, "@prisma/client", SQL), (Npm, "drizzle-orm", SQL), (Npm, "kysely", SQL),
    (Npm, "knex", SQL), (Npm, "@neondatabase/serverless", SQL), (Npm, "better-sqlite3", SQLITE), (Npm, "sqlite3", SQLITE), (Npm, "sqlite", SQLITE),
    (Npm, "@libsql/client", SQLITE), (Npm, "duckdb", DUCKDB), (Npm, "@duckdb/node-api", DUCKDB), (Npm, "@duckdb/duckdb-wasm", DUCKDB), (Npm, "neo4j-driver", GRAPH),
    (Npm, "redis", KV), (Npm, "ioredis", KV), (Npm, "@upstash/redis", KV), (Npm, "idb-keyval", KV),
    (PyPi, "subprocess", PROCESS), (Npm, "child_process", PROCESS), (Npm, "execa", PROCESS),
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
    "connect", "driver", "session", "execute", "executemany", "executescript", "sql", "query", "run", "execute_query", "execute_read", "execute_write",
    "read_transaction", "write_transaction", "register", "read_csv", "read_parquet", "read_json", "flush", "commit",
];

/// A graph database driver's managed transactions, by the way their work goes.
pub const TRANSACTIONS: &[(&str, Dir)] = &[("execute_read", Dir::Read), ("read_transaction", Dir::Read), ("execute_write", Dir::Write), ("write_transaction", Dir::Write)];

/// The access modes a graph database's driver is told, by the way they go.
pub const ACCESS_MODES: &[(&str, Dir)] = &[("READ", Dir::Read), ("R", Dir::Read), ("READ_ACCESS", Dir::Read), ("WRITE", Dir::Write), ("W", Dir::Write), ("WRITE_ACCESS", Dir::Write)];

/// A key-value store's operations, by the start of their name and the way they go.
pub const KV_READS: &[&str] = &["get", "mget", "hget", "keys", "scan", "exists"];
pub const KV_WRITES: &[&str] = &["set", "mset", "hset", "del", "remove", "clear", "incr", "decr", "expire", "lpush", "rpush", "sadd"];

/// Libraries that compute in memory: most of their calls stay in the program (`torch.cat`,
/// `nn.Linear`); OPERATIONS and the Hub's tables name the few that cross to a place.
pub const ML_PACKAGES: &[&str] = &["sentence_transformers", "torch", "transformers", "huggingface_hub", "datasets", "torchvision"];

/// The Hugging Face Hub: its libraries, what comes down, what goes up, and the weights a
/// pretrained model downloads by name (`AutoModel.from_pretrained(name)`).
pub const HUB_PACKAGES: &[&str] = &["huggingface_hub", "datasets"];
pub const HUB_READ: &[&str] = &["snapshot_download", "hf_hub_download", "load_dataset", "repo_info", "dataset_info", "model_info", "list_repo_files"];
pub const HUB_WRITE: &[&str] = &["upload_folder", "upload_file", "upload_large_folder", "create_repo", "create_commit", "delete_file", "delete_folder", "push_to_hub"];
pub const HUB_MODELS: &[&str] = &["from_pretrained", "SentenceTransformer", "CrossEncoder", "pipeline"];
pub const HUB_HOST: &str = "huggingface.co";

/// A model library's hub, as a module of it: `torch.hub.load` downloads.
pub const MODEL_HUBS: &[(&str, &str, &str)] = &[("torch", "hub", "load")];

/// The datasets library's builtin loaders: a dataset made by one of them comes from local files.
pub const HF_LOADERS: &[&str] = &["csv", "json", "parquet", "text", "arrow", "pandas", "imagefolder", "audiofolder", "videofolder", "webdataset", "xml", "sql"];

/// A library's operations that cross to a place by themselves, by package and operation: what
/// prints, what shows or draws a figure, what runs a model, and what saves or loads a file.
pub const OPERATIONS: &[(&str, &str, Crossing)] = &[
    ("click", "echo", Crossing::to(World::Screen)), ("click", "secho", Crossing::to(World::Screen)),
    ("typer", "echo", Crossing::to(World::Screen)), ("typer", "secho", Crossing::to(World::Screen)),
    ("rich", "print", Crossing::to(World::Screen)), ("pprint", "pprint", Crossing::to(World::Screen)), ("pprint", "pp", Crossing::to(World::Screen)),
    ("matplotlib", "show", Crossing::to(World::Screen)), ("plotly", "show", Crossing::to(World::Screen)),
    ("matplotlib", "figure", Crossing::to(World::Figure)), ("matplotlib", "subplots", Crossing::to(World::Figure)), ("matplotlib", "subplot_mosaic", Crossing::to(World::Figure)),
    ("sentence_transformers", "encode", Crossing::to(World::Model)), ("sentence_transformers", "embed", Crossing::to(World::Model)),
    ("transformers", "save_pretrained", Crossing::write(World::Disk)), ("sentence_transformers", "save_pretrained", Crossing::write(World::Disk)),
    ("transformers", "push_to_hub", Crossing::to(World::Net)),
    ("torch", "load", Crossing::read(World::Disk)), ("torch", "load_from_disk", Crossing::read(World::Disk)),
    ("datasets", "load", Crossing::read(World::Disk)), ("datasets", "load_from_disk", Crossing::read(World::Disk)),
    ("torch", "save", Crossing::write(World::Disk)), ("torch", "save_to_disk", Crossing::write(World::Disk)),
    ("datasets", "save", Crossing::write(World::Disk)), ("datasets", "save_to_disk", Crossing::write(World::Disk)),
];

/// What a package's operation crosses by itself.
pub fn operation(pkg: &str, op: &str) -> Option<Crossing> {
    OPERATIONS.iter().find(|(p, o, _)| *p == pkg && *o == op).map(|(_, _, c)| *c)
}

/// A program's streams that a write to shows the human: `sys.stdout`, `process.stderr`.
pub const STREAMS: &[(&str, &str)] = &[("sys", "stdout"), ("sys", "stderr"), ("process", "stdout"), ("process", "stderr")];

/// Python's own modules whose calls these tables describe: `print` and `open`, a stream.
pub const PYTHON_OWN: &[&str] = &["builtins", "sys"];

/// Python's builtins that cross: by name.
pub const PYTHON_BUILTINS: &[(&str, World)] = &[("print", World::Screen), ("open", World::Disk)];

// ---------- files ----------

/// Libraries whose calls read or write files: data formats, archives, arrays, images.
pub const FILE_PACKAGES: &[&str] = &[
    "json", "yaml", "pickle", "csv", "tarfile", "zipfile", "shutil", "os", "pathlib", "pandas", "geopandas", "numpy", "rasterio", "lxml", "PIL", "matplotlib",
    "ruamel",
];

/// What such a library's operation does with a file, by its name.
pub const FILE_READS: &[&str] = &[
    "read_text", "read_bytes", "read_csv", "read_parquet", "read_file", "read_json", "read_excel", "iterdir", "glob", "rglob", "load", "load_workbook", "parse",
    "fromfile", "imread", "open_dataset", "read", "readlines", "safe_load", "listdir", "walk", "open",
];
pub const FILE_WRITES: &[&str] = &[
    "write_text", "write_bytes", "to_csv", "to_parquet", "to_json", "dump", "safe_dump", "savefig", "save", "mkdir", "copy", "copyfile", "copytree", "move",
    "unlink", "rmtree", "touch", "imwrite", "extractall", "makedirs", "rename", "replace", "writelines",
];

/// Writes that move or remove files rather than store data, and the libraries of the file system
/// that alone do them: a frame's `copy()` or `replace(..)` stays in memory.
pub const FILE_SYSTEM_OPS: &[&str] = &["copy", "copyfile", "copytree", "move", "unlink", "rmtree", "rename", "replace", "mkdir", "makedirs", "touch"];
pub const FILE_SYSTEM_PACKAGES: &[&str] = &["shutil", "os", "pathlib"];

/// A path's methods whose place is the path they are called on; their argument is a pattern
/// below it or what is written: `cache_dir.glob("*.arrow")`, `p.write_text("done")`.
pub const ON_THE_PATH: &[&str] = &["glob", "rglob", "iterdir", "joinpath", "read_text", "read_bytes", "write_text", "write_bytes", "exists", "is_file", "is_dir", "mkdir", "unlink", "rmdir", "touch", "stat"];

/// The format an operation or a library implies when the code names no file.
pub const FORMATS_BY_OP: &[(&str, &str)] = &[
    ("read_csv", "csv"), ("to_csv", "csv"), ("read_parquet", "parquet"), ("to_parquet", "parquet"), ("read_json", "json"), ("to_json", "json"),
    ("read_excel", "xlsx"), ("to_excel", "xlsx"), ("load_workbook", "xlsx"), ("read_feather", "feather"), ("to_feather", "feather"), ("savefig", "png"),
    ("imsave", "png"), ("safe_load", "yaml"), ("safe_dump", "yaml"),
];
pub const FORMATS_BY_PACKAGE: &[(&str, &str)] = &[("json", "json"), ("yaml", "yaml"), ("ruamel", "yaml"), ("pickle", "pkl"), ("csv", "csv"), ("toml", "toml"), ("tomllib", "toml"), ("tomli", "toml"), ("lxml", "xml"), ("rasterio", "tif")];
/// numpy's own format, for what it saves and loads.
pub const ARRAY_FORMAT: (&str, &[&str], &str) = ("numpy", &["save", "load"], "npy");

/// What a file operation does to the file system, by its name in the languages Atlas reads: the
/// operations that look at a path's shape, those that list a directory, and the starts of the
/// names of those that create, remove, move and copy.
pub const PROBES: &[&str] = &["exists", "try_exists", "is_file", "is_dir", "is_symlink", "canonicalize", "metadata", "symlink_metadata", "existsSync", "stat", "statSync", "access"];
pub const LISTINGS: &[&str] = &["read_dir", "iterdir", "glob", "rglob", "listdir", "scandir", "walk", "readdir", "readdirSync", "opendir", "opendirSync"];
pub const CREATES: &[&str] = &["create_dir", "mkdir", "makedirs", "touch"];
pub const REMOVES: &[&str] = &["remove", "unlink", "rmtree", "rm"];
pub const MOVES: &[&str] = &["rename", "move", "replace"];
pub const COPIES: &[&str] = &["copy", "cp", "cpSync"];

/// Rust's: std's file system operations that read and that write, `Path`'s methods that read the
/// disk when called on a value whose type is unknown, the crates that read a tree of files, the
/// type that runs a program, and the macros that print.
pub const RUST_FS_READS: &[&str] = &["read", "read_to_string", "read_to_end", "read_exact", "read_line", "read_dir", "metadata", "canonicalize", "exists", "open", "read_link"];
pub const RUST_FS_WRITES: &[&str] = &[
    "write", "write_all", "write_fmt", "set_len", "sync_all", "create", "create_dir", "create_dir_all", "remove_file", "remove_dir", "remove_dir_all", "rename", "copy",
    "set_permissions",
];
pub const RUST_PATH_READS: &[&str] = &["exists", "try_exists", "is_file", "is_dir", "read_dir", "canonicalize", "symlink_metadata"];
pub const RUST_FILE_TREES: &[&str] = &["walkdir", "WalkDir", "ignore", "glob", "notify", "git2", "gix"];
pub const RUST_COMMAND: &str = "Command";
pub const RUST_SCREEN_MACROS: &[&str] = &["println", "print", "eprintln", "eprint"];

/// The JavaScript platform's: `node:fs`'s packages and what of them reads and writes, the global
/// objects a name may be reached through, the functions and constructors that go to the web, the
/// object that prints, the browser's key-value stores and their operations, and the names a
/// package's client is made by.
pub const NODE_FS: &[&str] = &["fs", "fs/promises"];
pub const NODE_FS_READS: &[&str] = &["readFile", "readFileSync", "readdir", "readdirSync", "createReadStream", "existsSync", "stat", "statSync", "access"];
pub const NODE_FS_WRITES: &[&str] = &[
    "writeFile", "writeFileSync", "appendFile", "appendFileSync", "mkdir", "mkdirSync", "rm", "rmSync", "unlink", "unlinkSync", "rename", "renameSync",
    "copyFile", "copyFileSync", "createWriteStream",
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
pub const PATH_MAKERS: &[&str] = &["Path", "PurePath", "PosixPath", "WindowsPath", "PurePosixPath", "PureWindowsPath", "Path::new", "PathBuf::from", "URL", "new URL"];

/// Libraries whose values are paths: a method of one names its file by the path it is called on.
pub const PATH_LIBRARIES: &[&str] = &["pathlib"];

/// A path's methods that give back the path they are called on, written another way.
pub const SAME_PATH: &[&str] = &["resolve", "absolute", "expanduser", "as_posix", "__fspath__"];

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
}

/// Path functions, by the package or global object they are on and their name.
pub const PATH_FUNCTIONS: &[(&str, &str, PathFn)] = &[
    ("path", "join", PathFn::Join),
    ("path", "resolve", PathFn::Join),
    ("path", "dirname", PathFn::Parent),
    ("url", "fileURLToPath", PathFn::Same),
    ("os", "homedir", PathFn::Home),
    ("process", "cwd", PathFn::Here),
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
    packages: &["typing", "collections", "pathlib", "builtins", "datetime", "abc", "os", "enum", "dataclasses", "functools", "itertools", "types"],
    names: &[],
};

/// Rust's: std, core and alloc, the containers and wrappers, and the associated types a trait
/// names (`Self::Target`, `S::Ok`).
pub const RUST_OWN_TYPES: OwnTypes = OwnTypes {
    packages: &["std", "core", "alloc"],
    names: &["Result", "Option", "Vec", "Box", "HashMap", "HashSet", "BTreeMap", "String", "Path", "PathBuf", "Ok", "Err", "Error", "Target", "Output", "Item", "Self"],
};

/// TypeScript's: a UI framework's components and props, a schema library's schemas.
pub const TYPESCRIPT_OWN_TYPES: OwnTypes = OwnTypes { packages: &["react", "react-dom", "preact", "vue", "svelte", "solid-js", "zod", "yup", "valibot", "superstruct", "io-ts"], names: &[] };
