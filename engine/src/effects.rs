//! Boundaries: calls that leave the package, classified by the part of the world they
//! touch. The engine knows libraries, never a project.

use std::ops::ControlFlow;

use sqlparser::ast::{visit_relations, Query, SetExpr, Statement, Visit, Visitor};
use sqlparser::dialect::{Dialect, DuckDbDialect, GenericDialect, SQLiteDialect};
use sqlparser::parser::Parser;

pub use crate::model::Crossing;
use crate::model::{Db, Dir, World};

const DISK_READ: Crossing = Crossing::read(World::Disk);
const DISK_WRITE: Crossing = Crossing::write(World::Disk);

const NET_PKGS: &[&str] = &["requests", "httpx", "urllib", "http", "socket", "aiohttp"];
/// What a network library does that moves data: a request, by its verb, and a socket's sending
/// and receiving. Building a client, or reading a response that has arrived, moves nothing.
const NET_OPS: &[&str] = &[
    "get", "post", "put", "patch", "delete", "head", "options", "request", "send", "stream", "urlopen", "urlretrieve",
    "connect", "sendall", "sendto", "recv", "recvfrom", "recv_into",
];
const LLM_PKGS: &[&str] = &["openai", "anthropic"];
/// Libraries that compute in memory. Most of their calls stay in the program (`torch.cat`,
/// `nn.Linear`); `ml` names the few that cross to a place.
const ML_PKGS: &[&str] = &["sentence_transformers", "torch", "transformers", "huggingface_hub", "datasets", "torchvision"];
/// The Hugging Face Hub: what comes down, and what goes up.
const HUB_READ: &[&str] = &["snapshot_download", "hf_hub_download", "load_dataset", "repo_info", "dataset_info", "model_info", "list_repo_files"];
/// Pretrained weights, downloaded from the Hub by name: `AutoModel.from_pretrained(name)`.
const HUB_MODELS: &[&str] = &["from_pretrained", "SentenceTransformer", "CrossEncoder", "pipeline"];
const HUB_WRITE: &[&str] = &["upload_folder", "upload_file", "upload_large_folder", "create_repo", "create_commit", "delete_file", "delete_folder", "push_to_hub"];
const HUB_HOST: &str = "huggingface.co";
/// The datasets library's builtin loaders: a dataset made by one of them comes from local files.
const HF_LOADERS: &[&str] = &["csv", "json", "parquet", "text", "arrow", "pandas", "imagefolder", "audiofolder", "videofolder", "webdataset", "xml", "sql"];
const FILE_PKGS: &[&str] = &[
    "json", "yaml", "pickle", "csv", "tarfile", "zipfile", "shutil", "os", "pathlib", "pandas",
    "geopandas", "numpy", "rasterio", "lxml", "PIL", "matplotlib", "ruamel",
];
const READ_BOUND: &[&str] = &[
    "read_text", "read_bytes", "read_csv", "read_parquet", "read_file", "read_json", "read_excel",
    "iterdir", "glob", "rglob", "load", "load_workbook", "parse", "fromfile", "imread",
    "open_dataset", "read", "readlines", "safe_load", "listdir", "walk", "open",
];
const WRITE_BOUND: &[&str] = &[
    "write_text", "write_bytes", "to_csv", "to_parquet", "to_json", "dump", "safe_dump", "savefig",
    "save", "mkdir", "copy", "copyfile", "copytree", "move", "unlink", "rmtree", "touch", "imwrite",
    "extractall", "makedirs", "rename", "replace", "writelines",
];
/// Writes that move or remove files rather than store data: only the file system's own libraries
/// do them, and a frame's `copy()` or `replace(..)` stays in memory.
const FILE_OPS: &[&str] = &["copy", "copyfile", "copytree", "move", "unlink", "rmtree", "rename", "replace", "mkdir", "makedirs", "touch"];
const FS_PKGS: &[&str] = &["shutil", "os", "pathlib"];

/// What a database driver does that reaches the database: open a connection or a session, run
/// a statement, or flush or commit what a session holds. Handing an object to an ORM's session
/// (`add`, `merge`, `delete`) only stages it, and reading the rows a statement gave back or
/// closing is part of what already ran.
const DB_OPS: &[&str] = &[
    "connect", "driver", "session", "execute", "executemany", "executescript", "sql", "query", "run", "execute_query",
    "execute_read", "execute_write", "read_transaction", "write_transaction", "register", "read_csv", "read_parquet",
    "read_json", "flush", "commit",
];

/// A socket's sending and receiving: the network calls that are handed no URL.
const SOCKET_OPS: &[&str] = &["connect", "send", "sendall", "sendto", "recv", "recvfrom", "recv_into"];

/// Whether a network library's call moves data, by its signature: it is handed a URL (or a
/// socket's address), or it sends or receives on a socket. Building a client, or reading a
/// response that has arrived (`resp.headers.get(..)`), is handed none.
pub fn requests(params: &[&str], op: &str) -> bool {
    params.iter().any(|p| matches!(*p, "url" | "uri" | "address")) || SOCKET_OPS.contains(&op)
}

/// Whether a package is a network library, whose calls `requests` decides.
pub fn network(pkg: &str) -> bool {
    NET_PKGS.contains(&pkg)
}

/// Whether a package is a client of a place (the web, a language model): building one of its
/// clients moves nothing. A model library's class is no client: building a pretrained model
/// downloads its weights.
pub fn client(pkg: &str) -> bool {
    NET_PKGS.contains(&pkg) || LLM_PKGS.contains(&pkg)
}

/// The database a Rust crate is a driver for.
fn rust_db(krate: &str) -> World {
    World::Db(match krate {
        "rusqlite" => Db::Sqlite,
        "duckdb" => Db::Duckdb,
        _ => Db::Sql,
    })
}

const RUST_NET: &[&str] = &["reqwest", "ureq", "hyper", "tungstenite", "tokio_tungstenite", "curl", "axum", "TcpStream", "TcpListener"];
const RUST_DB: &[&str] = &["rusqlite", "sqlx", "diesel", "duckdb", "postgres", "tokio_postgres", "sea_orm"];
const TS_NET: &[&str] = &["axios", "ky", "ofetch", "node-fetch", "undici", "got", "ws"];
const TS_LLM: &[&str] = &["@anthropic-ai/sdk", "openai", "@google/generative-ai", "@google/genai", "@mistralai/mistralai", "ollama"];

/// The database an npm package is a driver or an ORM for.
fn ts_db(pkg: &str) -> Option<World> {
    Some(World::Db(match pkg {
        "pg" | "postgres" | "mysql2" | "mysql" | "@prisma/client" | "drizzle-orm" | "kysely" | "knex" | "@neondatabase/serverless" => Db::Sql,
        "better-sqlite3" | "sqlite3" | "sqlite" | "@libsql/client" => Db::Sqlite,
        "duckdb" | "@duckdb/node-api" | "@duckdb/duckdb-wasm" => Db::Duckdb,
        "neo4j-driver" => Db::Graph,
        "redis" | "ioredis" | "@upstash/redis" | "idb-keyval" => Db::Kv,
        _ => return None,
    }))
}

/// The part of the world a library's calls reach, for a library that is a client of one: a
/// database, the network, a language model, a process. A value of its types is a door to that
/// place, not data at rest. A package's own implementation (`_sqlite3`) is the package.
pub fn library_place(pkg: &str) -> Option<World> {
    let pkg = pkg.strip_prefix('_').unwrap_or(pkg);
    if NET_PKGS.contains(&pkg) || RUST_NET.contains(&pkg) || TS_NET.contains(&pkg) {
        return Some(World::Net);
    }
    if let Some(db) = db_pkg(pkg) {
        return Some(db);
    }
    if RUST_DB.contains(&pkg) {
        return Some(rust_db(pkg));
    }
    if LLM_PKGS.contains(&pkg) || TS_LLM.contains(&pkg) {
        return Some(World::Llm);
    }
    if let Some(db) = ts_db(pkg) {
        return Some(db);
    }
    match pkg {
        "subprocess" | "child_process" | "execa" => Some(World::Process),
        _ => None,
    }
}

fn db_pkg(pkg: &str) -> Option<World> {
    Some(World::Db(match pkg {
        "duckdb" => Db::Duckdb,
        "neo4j" | "mgclient" => Db::Graph,
        "sqlite3" => Db::Sqlite,
        "psycopg2" | "sqlalchemy" => Db::Sql,
        _ => return None,
    }))
}

/// Classify a Rust call by its full path, e.g. `std::fs::read_to_string` reads the disk.
pub fn boundary_rust(parts: &[String]) -> Option<Crossing> {
    let head = parts.first().map(String::as_str).unwrap_or("");
    let last = parts.last().map(String::as_str).unwrap_or("");
    let has = |p: &str| parts.iter().any(|x| x == p);
    // building moves nothing: `Client::new()`, `Client::builder()..build()`, `OpenOptions::new()`
    // (Rust's constructor conventions, C-CTOR and C-BUILDER)
    let builds = crate::knowledge::BUILDS.contains(&last);
    if has("fs") {
        if builds {
            return None;
        }
        return Some(match last {
            "read" | "read_to_string" | "read_to_end" | "read_exact" | "read_line" | "read_dir" | "metadata" | "canonicalize" | "exists" | "open" | "read_link" => DISK_READ,
            "write" | "write_all" | "write_fmt" | "set_len" | "sync_all" | "create" | "create_dir" | "create_dir_all" | "remove_file" | "remove_dir" | "remove_dir_all" | "rename" | "copy" | "set_permissions" => DISK_WRITE,
            _ => Crossing::to(World::Disk),
        });
    }
    // a program is run by a `Command`; `process::exit` ends this one and runs none
    if has("Command") {
        return Some(Crossing::to(World::Process));
    }
    match head {
        "walkdir" | "WalkDir" | "glob" | "notify" => Some(DISK_READ),
        h if RUST_NET.contains(&h) => (!builds).then_some(Crossing::to(World::Net)),
        h if RUST_DB.contains(&h) => Some(Crossing::to(rust_db(h))),
        "git2" | "gix" => Some(DISK_READ),
        _ => None,
    }
}

/// Whether these tables say what a Python package's calls do. Its code is then not read: its
/// calls are places, or nothing.
pub fn described(pkg: &str) -> bool {
    NET_PKGS.contains(&pkg)
        || LLM_PKGS.contains(&pkg)
        || ML_PKGS.contains(&pkg)
        || FILE_PKGS.contains(&pkg)
        || db_pkg(pkg).is_some()
        || matches!(pkg, "click" | "typer" | "rich" | "pprint" | "subprocess" | "sys" | "builtins" | "plotly")
}

/// Classify a call that leaves the package, by the package that defines what is called.
///
/// `pkg` is that package: the library of the call's callee, or of its receiver's type, and
/// `builtins` for Python's own; `parts` the dotted call name as written. A call whose callee is
/// not known (`pkg` is `None`) touches nothing, unless it is a bare name nothing binds, which in
/// Python is a builtin.
pub fn boundary(pkg: Option<&str>, parts: &[String]) -> Option<Crossing> {
    let head = parts.first().map(String::as_str).unwrap_or("");
    let last = parts.last().map(String::as_str).unwrap_or("");
    let builtin = |name: &str| match name {
        "print" => Some(Crossing::to(World::Screen)),
        "open" => Some(Crossing::to(World::Disk)),
        _ => None,
    };
    let Some(pkg) = pkg else {
        return if parts.len() == 1 { builtin(head) } else { None };
    };
    if pkg == "builtins" {
        return builtin(last);
    }
    if matches!((pkg, last), ("click" | "typer", "echo" | "secho") | ("rich", "print") | ("pprint", "pprint" | "pp"))
        || (pkg == "sys" && parts.len() == 3 && matches!(parts[1].as_str(), "stdout" | "stderr"))
    {
        return Some(Crossing::to(World::Screen));
    }
    // a figure is drawn to be seen: `show` puts it on the screen, and what makes one is a
    // `figure`, which a notebook shows when the cell that made it ends
    match (pkg, last) {
        ("matplotlib" | "plotly", "show") => return Some(Crossing::to(World::Screen)),
        ("matplotlib", "figure" | "subplots" | "subplot_mosaic") => return Some(Crossing::to(World::Figure)),
        _ => {}
    }
    if NET_PKGS.contains(&pkg) {
        return NET_OPS.contains(&last).then_some(Crossing::to(World::Net));
    }
    if let Some(db) = db_pkg(pkg) {
        return DB_OPS.contains(&last).then_some(Crossing::to(db));
    }
    if LLM_PKGS.contains(&pkg) {
        return Some(Crossing::to(World::Llm));
    }
    if ML_PKGS.contains(&pkg) {
        return ml(pkg, parts, last);
    }
    if pkg == "subprocess" {
        return Some(Crossing::to(World::Process));
    }
    if FILE_PKGS.contains(&pkg) {
        if WRITE_BOUND.contains(&last) && (!FILE_OPS.contains(&last) || FS_PKGS.contains(&pkg)) {
            return Some(DISK_WRITE);
        }
        if READ_BOUND.contains(&last) {
            return Some(DISK_READ);
        }
    }
    None
}

/// The calls of a machine-learning library that cross to a place: pretrained weights load a
/// model, the Hub sends and receives files, a checkpoint or a saved dataset is a file.
fn ml(pkg: &str, parts: &[String], last: &str) -> Option<Crossing> {
    let net = Some(Crossing::to(World::Net));
    let hub_load = pkg == "torch" && parts.iter().any(|p| p == "hub") && last == "load";
    let pretrained = last == "from_pretrained" || matches!((pkg, last), ("sentence_transformers", "SentenceTransformer" | "CrossEncoder") | ("transformers", "pipeline"));
    if pretrained || hub_load {
        return net;
    }
    if matches!(pkg, "huggingface_hub" | "datasets") && (HUB_READ.contains(&last) || HUB_WRITE.contains(&last)) {
        return net;
    }
    if pkg == "transformers" && last == "push_to_hub" {
        return net;
    }
    match (pkg, last) {
        ("sentence_transformers", "encode" | "embed") => Some(Crossing::to(World::Model)),
        ("transformers" | "sentence_transformers", "save_pretrained") => Some(DISK_WRITE),
        ("torch" | "datasets", "load" | "load_from_disk") => Some(DISK_READ),
        ("torch" | "datasets", "save" | "save_to_disk") => Some(DISK_WRITE),
        _ => None,
    }
}

/// The boundary a TypeScript or JavaScript call crosses: `fetch` and HTTP clients go to the
/// web, `console` to the screen, `node:fs` reads and writes files, `child_process` runs a
/// program. `pkg` is the package the call's head was imported from, if any.
pub fn boundary_ts(pkg: Option<&str>, parts: &[String]) -> Option<Crossing> {
    let head = parts.first().map(String::as_str).unwrap_or("");
    let last = parts.last().map(String::as_str).unwrap_or("");
    match pkg {
        None if matches!(parts, [f] if f == "fetch") || (matches!(head, "window" | "globalThis") && last == "fetch") => Some(Crossing::to(World::Net)),
        // opening a socket or an event stream connects to its URL: `new WebSocket(url)`
        None if matches!(parts, [g] if g == "WebSocket" || g == "EventSource") => Some(Crossing::to(World::Net)),
        None if head == "console" || (head == "process" && matches!(parts.get(1).map(String::as_str), Some("stdout" | "stderr"))) => Some(Crossing::to(World::Screen)),
        // the browser's own key-value stores
        None if matches!(head, "localStorage" | "sessionStorage") && matches!(last, "getItem" | "setItem" | "removeItem" | "clear") => Some(Crossing::to(World::Db(Db::Kv))),
        // building a client moves nothing: `axios.create(..)`
        Some(_) if matches!(last, "create" | "createClient") && parts.len() == 2 && !TS_LLM.contains(&pkg.unwrap_or("")) => None,
        // a request, by its verb, or the package called as one (`got(url)`); a response read moves nothing
        Some(p) if TS_NET.contains(&p) => (parts.len() == 1 || NET_OPS.contains(&last) || last == "fetch").then_some(Crossing::to(World::Net)),
        Some(p) if library_place(p).is_some() => library_place(p).map(Crossing::to),
        Some("fs" | "fs/promises") => {
            const READS: &[&str] = &["readFile", "readFileSync", "readdir", "readdirSync", "createReadStream", "existsSync", "stat", "statSync", "access"];
            const WRITES: &[&str] = &[
                "writeFile", "writeFileSync", "appendFile", "appendFileSync", "mkdir", "mkdirSync", "rm", "rmSync", "unlink", "unlinkSync", "rename",
                "renameSync", "copyFile", "copyFileSync", "createWriteStream",
            ];
            if READS.contains(&last) {
                Some(DISK_READ)
            } else if WRITES.contains(&last) {
                Some(DISK_WRITE)
            } else {
                None
            }
        }
        Some("child_process" | "execa") => Some(Crossing::to(World::Process)),
        _ => None,
    }
}

/// What a file operation does to the file system, by its name in the languages Atlas reads: look
/// at its shape (whether a path is there, what a directory holds), change its shape, or move the
/// data in a file.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FsOp {
    Checks,
    Lists,
    Creates,
    Removes,
    Moves,
    Copies,
    /// Reads or writes what a file holds.
    Data,
}

pub fn fs_op(op: &str) -> FsOp {
    match op {
        "exists" | "try_exists" | "is_file" | "is_dir" | "is_symlink" | "canonicalize" | "metadata" | "symlink_metadata" | "existsSync" | "stat" | "statSync" | "access" => FsOp::Checks,
        "read_dir" | "iterdir" | "glob" | "rglob" | "listdir" | "scandir" | "walk" | "readdir" | "readdirSync" | "opendir" | "opendirSync" => FsOp::Lists,
        o if o.starts_with("create_dir") || o.starts_with("mkdir") || o.starts_with("makedirs") || o == "touch" => FsOp::Creates,
        o if o.starts_with("remove") || o.starts_with("unlink") || o.starts_with("rmtree") || o.starts_with("rm") => FsOp::Removes,
        o if o.starts_with("rename") || o.starts_with("move") || o.starts_with("replace") => FsOp::Moves,
        o if o.starts_with("copy") || o == "cp" || o == "cpSync" => FsOp::Copies,
        _ => FsOp::Data,
    }
}

/// `Path.exists()` and friends on a receiver whose type is unknown still read the disk.
pub const RUST_PATH_READS: &[&str] = &["exists", "try_exists", "is_file", "is_dir", "read_dir", "canonicalize", "symlink_metadata"];

/// `println!` and friends print to the terminal.
pub fn screen_macro(name: &str) -> bool {
    matches!(name, "println" | "print" | "eprintln" | "eprint")
}

/// What a boundary call is, once the string literals under it have been read: the part of the
/// world it touches, which file or table it names, and whether a database is read or written.
/// A statement that writes one table from others names them in `also`.
#[derive(Debug, PartialEq)]
pub struct Facts {
    pub kind: World,
    pub artifact: String,
    pub dir: Option<Dir>,
    pub also: Vec<(String, Option<Dir>)>,
}

impl Facts {
    fn of(c: Crossing, artifact: String) -> Facts {
        Facts { kind: c.world, artifact, dir: c.dir, also: vec![] }
    }

    /// Every place the call touches, with its direction: the first, then the others.
    pub fn places(&self) -> Vec<(String, Option<Dir>)> {
        std::iter::once((self.artifact.clone(), self.dir)).chain(self.also.iter().cloned()).collect()
    }
}

/// Refine a boundary by its literals: `open(p, "w")` writes, `to_csv(path)` writes a csv,
/// `Path("out.json")` names its file, `execute("INSERT INTO nodes ..")` writes the table `nodes`.
pub fn facts(kind: Crossing, pkg: Option<&str>, op: &str, literals: &[String]) -> Facts {
    // a file call reads unless a mode says it writes; `open` is one whatever its package
    let file = kind == Crossing::to(World::Disk);
    let kind = if op == "open" || file {
        match literals.iter().find(|l| is_mode(l)) {
            Some(m) if m.contains(['w', 'a', 'x', '+']) => DISK_WRITE,
            Some(_) => DISK_READ,
            None if file => DISK_READ,
            None => kind,
        }
    } else {
        kind
    };
    if let World::Db(db) = kind.world {
        let sides = SQL_DBS.contains(&db).then(|| sql(db, literals)).flatten();
        let Some(mut sides) = sides.filter(|s| !s.is_empty()) else {
            // a key-value store's place is the key it is handed
            let key = if db == Db::Kv { literals.first().cloned().unwrap_or_default() } else { String::new() };
            return Facts::of(Crossing { world: kind.world, dir: access(db, op, None) }, key);
        };
        let (artifact, dir) = sides.remove(0);
        return Facts { kind: kind.world, artifact, dir, also: sides };
    }
    if kind.world == World::Net {
        // a dataset built by one of the library's own loaders (`load_dataset("csv", data_files=..)`)
        // is read from local files
        if op == "load_dataset" && literals.first().map_or(false, |l| HF_LOADERS.contains(&l.as_str())) {
            return Facts::of(DISK_READ, literals.iter().skip(1).find(|l| path_like(l)).cloned().unwrap_or_default());
        }
        // pretrained weights named by a local path are a file, by a name a download from the Hub
        if HUB_MODELS.contains(&op) {
            if let Some(local) = literals.first().filter(|l| l.starts_with(['.', '/', '~'])) {
                return Facts::of(DISK_READ, local.clone());
            }
        }
        // a web place is named by its host; the Hub's operations say which way the files go
        let hub = HUB_READ.contains(&op) || HUB_WRITE.contains(&op) || HUB_MODELS.contains(&op);
        let artifact = if hub { HUB_HOST.to_string() } else { literals.iter().find_map(|l| host(l)).unwrap_or_default() };
        let dir = if HUB_WRITE.contains(&op) { Some(Dir::Write) } else if hub { Some(Dir::Read) } else { None };
        return Facts::of(Crossing { world: World::Net, dir }, artifact);
    }
    if kind.world != World::Disk {
        return Facts::of(kind, String::new());
    }
    // a name that starts with a value (`{name}.csv`) is known only by its format
    let artifact = literals
        .iter()
        .find(|l| path_like(l) && !l.starts_with(['{', '$']))
        .cloned()
        .or_else(|| literals.iter().find_map(|l| extension(l)).map(|e| format!("*.{e}")))
        .or_else(|| format_of(pkg, op).map(|f| format!("*.{f}")))
        .unwrap_or_default();
    Facts::of(kind, artifact)
}

/// A file mode literal: `r`, `wb`, `a+`.
/// Path operations whose place is the path they are called on; their argument is a pattern
/// below it or what is written: `cache_dir.glob("*.arrow")`, `p.write_text("done")`.
pub fn on_receiver(op: &str) -> bool {
    matches!(
        op,
        "glob" | "rglob" | "iterdir" | "joinpath" | "read_text" | "read_bytes" | "write_text" | "write_bytes" | "exists" | "is_file"
            | "is_dir" | "mkdir" | "unlink" | "rmdir" | "touch" | "stat"
    )
}

/// The host of a URL: `https://huggingface.co/api/x` -> `huggingface.co`.
fn host(s: &str) -> Option<String> {
    let rest = s.split_once("://")?.1;
    let h = rest.split(['/', '?', '#']).next()?;
    (!h.is_empty() && !h.contains('{')).then(|| h.to_string())
}

pub fn is_mode(s: &str) -> bool {
    (1..=3).contains(&s.len()) && s.chars().all(|c| "rwabxt+".contains(c))
}

/// The format a call implies when the code names no file.
fn format_of(pkg: Option<&str>, op: &str) -> Option<&'static str> {
    let by_op = match op {
        "read_csv" | "to_csv" => Some("csv"),
        "read_parquet" | "to_parquet" => Some("parquet"),
        "read_json" | "to_json" => Some("json"),
        "read_excel" | "to_excel" | "load_workbook" => Some("xlsx"),
        "read_feather" | "to_feather" => Some("feather"),
        "savefig" | "imsave" => Some("png"),
        "safe_load" | "safe_dump" => Some("yaml"),
        _ => None,
    };
    if by_op.is_some() {
        return by_op;
    }
    match pkg? {
        "json" => Some("json"),
        "yaml" | "ruamel" => Some("yaml"),
        "pickle" => Some("pkl"),
        "csv" => Some("csv"),
        "toml" | "tomllib" | "tomli" => Some("toml"),
        "lxml" => Some("xml"),
        "rasterio" => Some("tif"),
        "numpy" | "np" if matches!(op, "save" | "load") => Some("npy"),
        _ => None,
    }
}

/// A literal that names a file: `out.json`, `assets/{id}/x.xml`, `Makefile`. Not a mode, an
/// encoding, a URL or a sentence.
fn path_like(s: &str) -> bool {
    let s = &holes(s);
    let Some(first) = s.chars().next() else { return false };
    s.len() >= 2
        && !s.contains("://")
        && !is_mode(s)
        && s.chars().all(|c| c.is_alphanumeric() || "._-/{}*~".contains(c))
        && (s.contains('/') || extension(s).is_some() || (first.is_ascii_uppercase() && s.chars().all(|c| c.is_alphanumeric())))
}

/// The extension a literal ends in: `{name}.png` -> `png`, `.env` -> `env`.
/// A pattern with its interpolations as holes, whatever fills them:
/// `figures/{safe(name)}.png` -> `figures/{}.png`.
fn holes(s: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for c in s.chars() {
        match c {
            '{' => {
                if depth == 0 {
                    out.push('{');
                }
                depth += 1;
            }
            '}' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    out.push('}');
                }
            }
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

fn extension(s: &str) -> Option<String> {
    let (_, e) = s.rsplit_once('.')?;
    let ok = (1..=8).contains(&e.len()) && e.chars().all(|c| c.is_ascii_alphanumeric()) && e.chars().any(|c| c.is_ascii_alphabetic());
    (ok && !s.contains(char::is_whitespace)).then(|| e.to_ascii_lowercase())
}

/// What the SQL under a call does to the tables it names: each table with whether it is read or
/// written, the written first. A literal that is not SQL says nothing. A statement split over
/// several literals (`"SELECT a " "FROM t"`) is the longest run from its first literal that the
/// grammar reads whole; the call's other literals (`timespec="seconds"`) are not part of it. A
/// statement that names no table (a `PRAGMA`, `SET`, `INSTALL`, a transaction) has no sides.
pub fn sql(db: Db, literals: &[String]) -> Option<Vec<(String, Option<Dir>)>> {
    let start = literals.iter().position(|l| verb(l).is_some())?;
    longest_statement(db, &literals[start..])
}

/// The statement that starts at the first of `run`: the longest run of literals the grammar reads
/// whole, else the first literal by its verb.
fn longest_statement(db: Db, run: &[String]) -> Option<Vec<(String, Option<Dir>)>> {
    (1..=run.len()).rev().find_map(|end| parsed(db, &run[..end].join(" "))).or_else(|| statement(db, run.first()?))
}

/// One SQL text read by its grammar, in its database's dialect. What the grammar cannot read
/// keeps its leading verb (`INSERT` writes) and names no table.
pub fn statement(db: Db, text: &str) -> Option<Vec<(String, Option<Dir>)>> {
    parsed(db, text).or_else(|| verb(text).map(|v| v.map(|d| (String::new(), Some(d))).into_iter().collect()))
}

/// Whether a text starts as SQL does: `Write` for a verb that changes data or its shape, `Read`
/// for one that asks, no direction for a setting.
fn verb(text: &str) -> Option<Option<Dir>> {
    let first = text.split_whitespace().next()?.to_ascii_lowercase();
    let first = first.as_str();
    [(WRITE_VERBS, Some(Dir::Write)), (READ_VERBS, Some(Dir::Read)), (SETTINGS, None)].into_iter().find(|(verbs, _)| verbs.contains(&first)).map(|(_, dir)| dir)
}

/// A SQL text the grammar reads whole: the sides of its statements.
fn parsed(db: Db, text: &str) -> Option<Vec<(String, Option<Dir>)>> {
    verb(text)?;
    let sql = sql_holes(text);
    let dialect: Box<dyn Dialect> = match db {
        Db::Sqlite => Box::new(SQLiteDialect {}),
        Db::Duckdb => Box::new(DuckDbDialect {}),
        _ => Box::new(GenericDialect {}),
    };
    let stmts = Parser::parse_sql(dialect.as_ref(), &sql).or_else(|_| Parser::parse_sql(&GenericDialect {}, &sql)).ok()?;
    let mut out: Vec<(String, Option<Dir>)> = stmts.iter().flat_map(sides).map(|(n, d)| (n, Some(d))).collect();
    out.sort_by_key(|(_, d)| *d != Some(Dir::Write));
    out.dedup();
    Some(out)
}

/// The verbs a statement starts with: those that change data or its shape, those that ask, and
/// the settings around them.
const WRITE_VERBS: &[&str] = &["insert", "update", "delete", "create", "merge", "copy", "drop", "alter", "replace", "upsert", "truncate"];
const READ_VERBS: &[&str] = &["select", "with", "values", "show", "describe", "call"];
const SETTINGS: &[&str] = &["pragma", "set", "install", "load", "begin", "commit", "rollback", "attach", "detach", "vacuum", "analyze", "explain", "checkpoint", "use"];

/// A placeholder the code fills in at run time (`{tabel}`, `${t}`, `%s`) as a name the grammar
/// accepts and Atlas does not take for a table.
const HOLE: &str = "atlas_hole";
fn sql_holes(text: &str) -> String {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"\$?\{[^{}]*\}|%\([A-Za-z_]\w*\)s|%s").unwrap());
    let mut n = 0;
    re.replace_all(text, |c: &regex::Captures| {
        n += 1;
        if c[0].starts_with('%') { "?".to_string() } else { format!("{HOLE}{n}") }
    })
    .into_owned()
}

/// The names a statement gives its own subqueries: `WITH t AS (..)` names `t`, not a table.
struct Ctes(Vec<String>);

impl Visitor for Ctes {
    type Break = ();
    fn pre_visit_query(&mut self, q: &Query) -> ControlFlow<()> {
        self.0.extend(q.with.iter().flat_map(|w| w.cte_tables.iter().map(|c| c.alias.name.value.clone())));
        ControlFlow::Continue(())
    }
}

fn ctes(st: &Statement) -> Vec<String> {
    let mut found = Ctes(vec![]);
    let _ = st.visit(&mut found);
    found.0
}

/// Every table a statement names, in the order it names them.
fn relations(st: &Statement) -> Vec<String> {
    let named = ctes(st);
    let mut names: Vec<String> = vec![];
    let _ = visit_relations(st, |n| {
        names.push(n.0.iter().map(|p| p.to_string().trim_matches(['"', '`', '[', ']']).to_string()).collect::<Vec<_>>().join("."));
        ControlFlow::<()>::Continue(())
    });
    names.retain(|n| !named.contains(n));
    names
}

/// The tables one statement writes and reads: a statement that changes data writes the first
/// table it names (`INSERT INTO a SELECT .. FROM b`) and reads the others; a query reads all of
/// them; a setting names none.
fn sides(st: &Statement) -> Vec<(String, Dir)> {
    // `WITH t AS (..) INSERT INTO b ..`: the change is the body; what `WITH` names it reads
    if let Statement::Query(q) = st {
        if let SetExpr::Insert(inner) | SetExpr::Update(inner) | SetExpr::Delete(inner) | SetExpr::Merge(inner) = q.body.as_ref() {
            let named = ctes(st);
            let mut out = sides(inner);
            out.retain(|(n, _)| !named.contains(n));
            let read: Vec<(String, Dir)> = relations(st).into_iter().filter(|n| !n.contains(HOLE) && !out.iter().any(|(m, _)| m == n)).map(|n| (n, Dir::Read)).collect();
            out.extend(read);
            return out;
        }
    }
    let mut names = relations(st);
    if let Statement::Drop { names: dropped, .. } = st {
        names = dropped.iter().map(|n| n.to_string().trim_matches(['"', '`']).to_string()).collect();
    }
    let writes = match st {
        Statement::Query(_) => false,
        Statement::Copy { to, .. } => !*to,
        Statement::Insert(_)
        | Statement::Update(_)
        | Statement::Delete(_)
        | Statement::Merge(_)
        | Statement::CreateTable(_)
        | Statement::CreateView(_)
        | Statement::CreateIndex(_)
        | Statement::AlterTable(_)
        | Statement::Drop { .. }
        | Statement::Truncate(_) => true,
        _ => return vec![],
    };
    let dir = if writes { Dir::Write } else { Dir::Read };
    let mut out: Vec<(String, Dir)> = names.into_iter().enumerate().map(|(i, n)| (n, if writes && i == 0 { Dir::Write } else { Dir::Read })).filter(|(n, _)| !n.contains(HOLE)).collect();
    // a table named at run time is unknown, but which way the statement goes is not
    if !out.iter().any(|(_, d)| *d == dir) {
        out.insert(0, (String::new(), dir));
    }
    out
}

/// The databases whose statements are SQL.
const SQL_DBS: &[Db] = &[Db::Duckdb, Db::Sqlite, Db::Sql];

/// Which way a graph database's data goes, where the Python code says it to the driver: a
/// managed transaction (`session.execute_read(work)`, `execute_write`), a query's routing
/// (`execute_query(q, routing_=RoutingControl.READ)`) or a session's access mode
/// (`driver.session(default_access_mode=READ_ACCESS)`). The query's own text is not read;
/// where the code says nothing, the direction stays open.
pub fn access(db: Db, op: &str, mode: Option<&str>) -> Option<Dir> {
    if db == Db::Kv {
        let op = op.to_ascii_lowercase();
        return if ["get", "mget", "hget", "keys", "scan", "exists"].iter().any(|r| op.starts_with(r)) {
            Some(Dir::Read)
        } else if ["set", "mset", "hset", "del", "remove", "clear", "incr", "decr", "expire", "lpush", "rpush", "sadd"].iter().any(|w| op.starts_with(w)) {
            Some(Dir::Write)
        } else {
            None
        };
    }
    if db != Db::Graph {
        return None;
    }
    match op {
        "execute_read" | "read_transaction" => return Some(Dir::Read),
        "execute_write" | "write_transaction" => return Some(Dir::Write),
        _ => {}
    }
    let mode = mode.map(|m| m.trim_matches(['"', '\'']).rsplit('.').next().unwrap_or("").to_ascii_uppercase()).unwrap_or_default();
    match mode.as_str() {
        "READ" | "R" | "READ_ACCESS" => Some(Dir::Read),
        "WRITE" | "W" | "WRITE_ACCESS" => Some(Dir::Write),
        _ => None,
    }
}

/// A managed transaction, handed the work it runs: `session.execute_read(work, ..)`.
pub fn transaction(op: &str) -> bool {
    matches!(op, "execute_read" | "execute_write" | "read_transaction" | "write_transaction")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lits(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|s| s.to_string()).collect()
    }

    fn parts(xs: &[&str]) -> Vec<String> {
        lits(xs)
    }

    /// A crossing by the name the graph's JSON gives it.
    fn c(kind: &str) -> Crossing {
        Crossing::named(kind, "").unwrap()
    }

    fn name(c: Option<Crossing>) -> Option<&'static str> {
        c.map(Crossing::kind_name)
    }

    fn kind(f: &Facts) -> &'static str {
        Crossing { world: f.kind, dir: f.dir }.kind_name()
    }

    fn dir(d: Option<Dir>) -> &'static str {
        d.map_or("", Dir::as_str)
    }

    fn db(kind: &str) -> Db {
        match c(kind).world {
            World::Db(db) => db,
            w => panic!("{w:?} is no database"),
        }
    }

    /// A machine-learning library computes in memory; only loading weights, moving files to
    /// and from the Hub, and checkpoints cross to a place.
    #[test]
    fn a_library_is_not_a_place() {
        assert_eq!(name(boundary(Some("torch"), &parts(&["torch", "cat"]))), None);
        assert_eq!(name(boundary(Some("torch"), &parts(&["nn", "Linear"]))), None);
        assert_eq!(name(boundary(Some("transformers"), &parts(&["AutoModel", "from_pretrained"]))), Some("net"), "weights come down from the Hub");
        assert_eq!(name(boundary(Some("torch"), &parts(&["torch", "save"]))), Some("disk_write"));
        assert_eq!(name(boundary(Some("huggingface_hub"), &parts(&["snapshot_download"]))), Some("net"));
        assert_eq!(name(boundary(Some("huggingface_hub"), &parts(&["HfApi"]))), None, "building a client moves nothing");
        assert_eq!(name(boundary(Some("datasets"), &parts(&["load_dataset"]))), Some("net"));
        let down = facts(c("net"), Some("huggingface_hub"), "snapshot_download", &lits(&["dataset"]));
        assert_eq!((down.artifact.as_str(), dir(down.dir)), ("huggingface.co", "read"), "the Hub is a host; a download reads");
        assert_eq!(facts(c("net"), Some("huggingface_hub"), "upload_folder", &[]).dir, Some(Dir::Write));
        assert_eq!(facts(c("net"), Some("requests"), "get", &lits(&["https://api.example.org/v1/x"])).artifact, "api.example.org");
        let model = facts(c("net"), Some("transformers"), "from_pretrained", &lits(&["facebook/dinov2-small", "auto"]));
        assert_eq!((kind(&model), model.artifact.as_str(), dir(model.dir)), ("net", "huggingface.co", "read"));
        assert_eq!(kind(&facts(c("net"), Some("transformers"), "from_pretrained", &lits(&["./checkpoints/best"]))), "disk_read", "a local checkpoint is a file");
    }

    #[test]
    fn a_library_names_what_it_is() {
        let p = |xs: &[&str]| xs.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(name(boundary_rust(&p(&["reqwest", "Client", "new"]))), None, "building a client moves nothing");
        assert_eq!(name(boundary_rust(&p(&["reqwest", "get"]))), Some("net"));
        assert_eq!(name(boundary_rust(&p(&["rusqlite", "Connection", "open"]))), Some("db_sqlite"), "one database, one name in every language");
        assert_eq!(library_place("neo4j").map(World::as_str), Some("db_graph"), "the Bolt driver reaches a graph database, whichever");
        assert_eq!(library_place("duckdb").map(World::as_str), Some("db_duckdb"));
        let local = facts(c("net"), Some("datasets"), "load_dataset", &p(&["csv", "data/train.csv"]));
        assert_eq!((kind(&local), local.artifact.as_str()), ("disk_read", "data/train.csv"), "a builtin loader reads local files");
        assert!(requests(&["url", "params"], "get") && !requests(&["key", "default"], "get"), "a request is handed a URL");
    }

    #[test]
    fn files_are_named_by_literal_then_format() {
        assert_eq!(facts(c("disk_write"), None, "write_text", &lits(&["out.json"])).artifact, "out.json");
        assert_eq!(facts(c("disk_write"), None, "to_csv", &lits(&[";"])).artifact, "*.csv");
        assert_eq!(facts(c("disk_write"), None, "savefig", &lits(&["{name}.svg"])).artifact, "*.svg", "a name made of a value is its format");
        assert_eq!(facts(c("disk_write"), None, "savefig", &lits(&["plots/{name}.svg"])).artifact, "plots/{name}.svg");
        assert_eq!(facts(c("disk_write"), Some("json"), "dump", &[]).artifact, "*.json");
        assert_eq!(facts(c("disk_read"), None, "read_text", &lits(&["utf-8"])).artifact, "");
        assert_eq!(facts(c("disk_read"), None, "read_to_string", &lits(&["Makefile"])).artifact, "Makefile");
    }

    #[test]
    fn open_mode_decides_read_or_write() {
        assert_eq!(kind(&facts(c("disk"), None, "open", &lits(&["w"]))), "disk_write");
        assert_eq!(kind(&facts(c("disk"), None, "open", &lits(&["data.csv"]))), "disk_read");
        assert_eq!(facts(c("disk"), None, "open", &lits(&["data.csv", "w"])).artifact, "data.csv");
    }

    /// SQL is read by its grammar: a statement writes the table it changes and reads the ones it
    /// takes from; a setting names no table; a name filled in at run time is no table.
    #[test]
    fn statements_read_or_write_tables() {
        let s = |kind: &str, text: &str| statement(db(kind), text).unwrap();
        let t = |xs: &[(&str, &'static str)]| xs.iter().map(|(n, d)| (n.to_string(), Dir::parse(d))).collect::<Vec<_>>();
        assert_eq!(s("db_sqlite", "INSERT INTO nodes VALUES (?)"), t(&[("nodes", "write")]));
        assert_eq!(s("db_duckdb", "select * from edges e join nodes n on e.a = n.id"), t(&[("edges", "read"), ("nodes", "read")]));
        assert_eq!(s("db_sqlite", "INSERT INTO log SELECT * FROM oordeel WHERE status = :s"), t(&[("log", "write"), ("oordeel", "read")]));
        assert_eq!(s("db_sqlite", "WITH t AS (SELECT * FROM a) INSERT INTO b SELECT * FROM t"), t(&[("b", "write"), ("a", "read")]), "`t` names a subquery, not a table");
        assert_eq!(s("db_sqlite", "CREATE TABLE IF NOT EXISTS oordeel (uuid TEXT PRIMARY KEY)"), t(&[("oordeel", "write")]));
        assert_eq!(s("db_duckdb", "DROP TABLE IF EXISTS locaties"), t(&[("locaties", "write")]));
        assert_eq!(s("db_sqlite", "PRAGMA user_version = 3"), t(&[]), "a setting names no table");
        assert_eq!(s("db_sqlite", "PRAGMA journal_mode=WAL"), t(&[]), "nor when the grammar does not read it");
        assert_eq!(s("db_sqlite", "ALTER TABLE {tabel} ADD COLUMN x TEXT"), t(&[("", "write")]), "a table named at run time");
        assert_eq!(s("db_sqlite", "SELECT count(*) FROM {tabel}"), t(&[("", "read")]));
        assert_eq!(statement(db("db_sqlite"), "hello world"), None);
        assert_eq!(sql(db("db_sqlite"), &lits(&["SELECT a ", "FROM t"])), Some(t(&[("t", "read")])), "a statement split over literals");
        assert_eq!(sql(db("db_sqlite"), &lits(&["INSERT INTO verbruik (moment, ", "model) VALUES (?, ?)", "seconds"])), Some(t(&[("verbruik", "write")])), "and the call's other literals");
    }

    /// A graph database's direction is what the Python code says to the driver, never the query.
    #[test]
    fn a_driver_says_which_way() {
        assert_eq!(dir(access(Db::Graph, "execute_read", None)), "read");
        assert_eq!(dir(access(Db::Graph, "execute_write", None)), "write");
        assert_eq!(dir(access(Db::Graph, "execute_query", Some("RoutingControl.READ"))), "read");
        assert_eq!(dir(access(Db::Graph, "session", Some("neo4j.WRITE_ACCESS"))), "write");
        assert_eq!(access(Db::Graph, "run", None), None, "a query alone says nothing");
        assert_eq!(facts(c("db_graph"), Some("neo4j"), "run", &lits(&["MATCH (n:Regeling) RETURN n"])), Facts::of(c("db_graph"), String::new()));
    }
}
