//! Boundaries: calls that leave the package, classified by the part of the world they
//! touch. The engine knows libraries, never a project.

use std::ops::ControlFlow;

use sqlparser::ast::{visit_relations, Query, SetExpr, Statement, Visit, Visitor};
use sqlparser::dialect::{Dialect, DuckDbDialect, GenericDialect, SQLiteDialect};
use sqlparser::parser::Parser;

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
    "geopandas", "numpy", "rasterio", "lxml", "PIL", "matplotlib", "plt", "ruamel", "np", "pd",
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
fn rust_db(krate: &str) -> &'static str {
    match krate {
        "rusqlite" => "db_sqlite",
        "duckdb" => "db_duckdb",
        _ => "db_sql",
    }
}

const RUST_NET: &[&str] = &["reqwest", "ureq", "hyper", "tungstenite", "tokio_tungstenite", "curl", "axum", "TcpStream", "TcpListener"];
const RUST_DB: &[&str] = &["rusqlite", "sqlx", "diesel", "duckdb", "postgres", "tokio_postgres", "sea_orm"];
const TS_NET: &[&str] = &["axios", "ky", "ofetch", "node-fetch", "undici", "got", "ws"];
const TS_LLM: &[&str] = &["@anthropic-ai/sdk", "openai", "@google/generative-ai", "@google/genai", "@mistralai/mistralai", "ollama"];

/// The database an npm package is a driver or an ORM for.
fn ts_db(pkg: &str) -> Option<&'static str> {
    match pkg {
        "pg" | "postgres" | "mysql2" | "mysql" | "@prisma/client" | "drizzle-orm" | "kysely" | "knex" | "@neondatabase/serverless" => Some("db_sql"),
        "better-sqlite3" | "sqlite3" | "sqlite" | "@libsql/client" => Some("db_sqlite"),
        "duckdb" | "@duckdb/node-api" | "@duckdb/duckdb-wasm" => Some("db_duckdb"),
        "neo4j-driver" => Some("db_graph"),
        "redis" | "ioredis" | "@upstash/redis" | "idb-keyval" => Some("db_kv"),
        _ => None,
    }
}

/// The part of the world a library's calls reach, for a library that is a client of one: a
/// database, the network, a language model, a process. A value of its types is a door to that
/// place, not data at rest.
pub fn library_place(pkg: &str) -> Option<&'static str> {
    if NET_PKGS.contains(&pkg) || RUST_NET.contains(&pkg) || TS_NET.contains(&pkg) {
        return Some("net");
    }
    if let Some(db) = db_pkg(pkg) {
        return Some(db);
    }
    if RUST_DB.contains(&pkg) {
        return Some(rust_db(pkg));
    }
    if LLM_PKGS.contains(&pkg) || TS_LLM.contains(&pkg) {
        return Some("llm");
    }
    if let Some(db) = ts_db(pkg) {
        return Some(db);
    }
    match pkg {
        "subprocess" | "child_process" | "execa" => Some("process"),
        _ => None,
    }
}

fn db_pkg(pkg: &str) -> Option<&'static str> {
    match pkg {
        "duckdb" => Some("db_duckdb"),
        "neo4j" | "mgclient" => Some("db_graph"),
        "sqlite3" => Some("db_sqlite"),
        "psycopg2" | "sqlalchemy" => Some("db_sql"),
        _ => None,
    }
}

/// Classify a Rust call by its full path, e.g. `std::fs::read_to_string` -> `disk_read`.
pub fn boundary_rust(parts: &[String]) -> Option<&'static str> {
    let head = parts.first().map(String::as_str).unwrap_or("");
    let last = parts.last().map(String::as_str).unwrap_or("");
    let has = |p: &str| parts.iter().any(|x| x == p);
    // building moves nothing: `Client::new()`, `Client::builder()..build()`, `OpenOptions::new()`
    // (Rust's constructor conventions, C-CTOR and C-BUILDER)
    let builds = matches!(last, "new" | "builder" | "default" | "build");
    if has("fs") {
        if builds {
            return None;
        }
        return Some(match last {
            "read" | "read_to_string" | "read_to_end" | "read_exact" | "read_line" | "read_dir" | "metadata" | "canonicalize" | "exists" | "open" | "read_link" => "disk_read",
            "write" | "write_all" | "write_fmt" | "set_len" | "sync_all" | "create" | "create_dir" | "create_dir_all" | "remove_file" | "remove_dir" | "remove_dir_all" | "rename" | "copy" | "set_permissions" => "disk_write",
            _ => "disk",
        });
    }
    // a program is run by a `Command`; `process::exit` ends this one and runs none
    if has("Command") {
        return Some("process");
    }
    match head {
        "walkdir" | "WalkDir" | "glob" | "notify" => Some("disk_read"),
        h if RUST_NET.contains(&h) => (!builds).then_some("net"),
        h if RUST_DB.contains(&h) => Some(rust_db(h)),
        "git2" | "gix" => Some("disk_read"),
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
pub fn boundary(pkg: Option<&str>, parts: &[String]) -> Option<&'static str> {
    let head = parts.first().map(String::as_str).unwrap_or("");
    let last = parts.last().map(String::as_str).unwrap_or("");
    let builtin = |name: &str| match name {
        "print" => Some("screen"),
        "open" => Some("disk"),
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
        return Some("screen");
    }
    // a figure is drawn to be seen: `show` puts it on the screen, and what makes one is a
    // `figure`, which a notebook shows when the cell that made it ends
    match (pkg, last) {
        ("matplotlib" | "plotly", "show") => return Some("screen"),
        ("matplotlib", "figure" | "subplots" | "subplot_mosaic") => return Some("figure"),
        _ => {}
    }
    if NET_PKGS.contains(&pkg) {
        return NET_OPS.contains(&last).then_some("net");
    }
    if let Some(db) = db_pkg(pkg) {
        return DB_OPS.contains(&last).then_some(db);
    }
    if LLM_PKGS.contains(&pkg) {
        return Some("llm");
    }
    if ML_PKGS.contains(&pkg) {
        return ml(pkg, parts, last);
    }
    if pkg == "subprocess" {
        return Some("process");
    }
    if FILE_PKGS.contains(&pkg) {
        if WRITE_BOUND.contains(&last) && (!FILE_OPS.contains(&last) || FS_PKGS.contains(&pkg)) {
            return Some("disk_write");
        }
        if READ_BOUND.contains(&last) {
            return Some("disk_read");
        }
    }
    None
}

/// The calls of a machine-learning library that cross to a place: pretrained weights load a
/// model, the Hub sends and receives files, a checkpoint or a saved dataset is a file.
fn ml(pkg: &str, parts: &[String], last: &str) -> Option<&'static str> {
    let hub_load = pkg == "torch" && parts.iter().any(|p| p == "hub") && last == "load";
    let pretrained = last == "from_pretrained" || matches!((pkg, last), ("sentence_transformers", "SentenceTransformer" | "CrossEncoder") | ("transformers", "pipeline"));
    if pretrained || hub_load {
        return Some("net");
    }
    if matches!(pkg, "huggingface_hub" | "datasets") && (HUB_READ.contains(&last) || HUB_WRITE.contains(&last)) {
        return Some("net");
    }
    if pkg == "transformers" && last == "push_to_hub" {
        return Some("net");
    }
    match (pkg, last) {
        ("sentence_transformers", "encode" | "embed") => Some("model"),
        ("transformers" | "sentence_transformers", "save_pretrained") => Some("disk_write"),
        ("torch" | "datasets", "load" | "load_from_disk") => Some("disk_read"),
        ("torch" | "datasets", "save" | "save_to_disk") => Some("disk_write"),
        _ => None,
    }
}

/// The boundary a TypeScript or JavaScript call crosses: `fetch` and HTTP clients go to the
/// web, `console` to the screen, `node:fs` reads and writes files, `child_process` runs a
/// program. `pkg` is the package the call's head was imported from, if any.
pub fn boundary_ts(pkg: Option<&str>, parts: &[String]) -> Option<&'static str> {
    let head = parts.first().map(String::as_str).unwrap_or("");
    let last = parts.last().map(String::as_str).unwrap_or("");
    match pkg {
        None if matches!(parts, [f] if f == "fetch") || (matches!(head, "window" | "globalThis") && last == "fetch") => Some("net"),
        None if head == "console" || (head == "process" && matches!(parts.get(1).map(String::as_str), Some("stdout" | "stderr"))) => Some("screen"),
        // the browser's own key-value stores
        None if matches!(head, "localStorage" | "sessionStorage") && matches!(last, "getItem" | "setItem" | "removeItem" | "clear") => Some("db_kv"),
        // building a client moves nothing: `axios.create(..)`
        Some(_) if matches!(last, "create" | "createClient") && parts.len() == 2 && !TS_LLM.contains(&pkg.unwrap_or("")) => None,
        // a request, by its verb, or the package called as one (`got(url)`); a response read moves nothing
        Some(p) if TS_NET.contains(&p) => (parts.len() == 1 || NET_OPS.contains(&last) || last == "fetch").then_some("net"),
        Some(p) if library_place(p).is_some() => library_place(p),
        Some("fs" | "fs/promises") => {
            const READS: &[&str] = &["readFile", "readFileSync", "readdir", "readdirSync", "createReadStream", "existsSync", "stat", "statSync", "access"];
            const WRITES: &[&str] = &[
                "writeFile", "writeFileSync", "appendFile", "appendFileSync", "mkdir", "mkdirSync", "rm", "rmSync", "unlink", "unlinkSync", "rename",
                "renameSync", "copyFile", "copyFileSync", "createWriteStream",
            ];
            if READS.contains(&last) {
                Some("disk_read")
            } else if WRITES.contains(&last) {
                Some("disk_write")
            } else {
                None
            }
        }
        Some("child_process" | "execa") => Some("process"),
        _ => None,
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
    pub kind: &'static str,
    pub artifact: String,
    pub dir: &'static str,
    pub also: Vec<(String, &'static str)>,
}

impl Facts {
    fn of(kind: &'static str, artifact: String, dir: &'static str) -> Facts {
        Facts { kind, artifact, dir, also: vec![] }
    }

    /// Every place the call touches, with its direction: the first, then the others.
    pub fn places(&self) -> Vec<(String, &'static str)> {
        std::iter::once((self.artifact.clone(), self.dir)).chain(self.also.iter().cloned()).collect()
    }
}

/// Refine a boundary by its literals: `open(p, "w")` writes, `to_csv(path)` writes a csv,
/// `Path("out.json")` names its file, `execute("INSERT INTO nodes ..")` writes the table `nodes`.
pub fn facts(kind: &'static str, pkg: Option<&str>, op: &str, literals: &[String]) -> Facts {
    // a file call reads unless a mode says it writes; `open` is one whatever its package
    let file = kind == "disk";
    let kind = if op == "open" || file {
        match literals.iter().find(|l| is_mode(l)) {
            Some(m) if m.contains(['w', 'a', 'x', '+']) => "disk_write",
            Some(_) => "disk_read",
            None if file => "disk_read",
            None => kind,
        }
    } else {
        kind
    };
    if kind.starts_with("db_") {
        let sides = SQL_DBS.contains(&kind).then(|| sql(kind, literals)).flatten();
        let Some(mut sides) = sides.filter(|s| !s.is_empty()) else {
            // a key-value store's place is the key it is handed
            let key = if kind == "db_kv" { literals.first().cloned().unwrap_or_default() } else { String::new() };
            return Facts::of(kind, key, access(kind, op, None));
        };
        let (artifact, dir) = sides.remove(0);
        return Facts { kind, artifact, dir, also: sides };
    }
    if kind == "net" {
        // a dataset built by one of the library's own loaders (`load_dataset("csv", data_files=..)`)
        // is read from local files
        if op == "load_dataset" && literals.first().map_or(false, |l| HF_LOADERS.contains(&l.as_str())) {
            return Facts::of("disk_read", literals.iter().skip(1).find(|l| path_like(l)).cloned().unwrap_or_default(), "");
        }
        // pretrained weights named by a local path are a file, by a name a download from the Hub
        if HUB_MODELS.contains(&op) {
            if let Some(local) = literals.first().filter(|l| l.starts_with(['.', '/', '~'])) {
                return Facts::of("disk_read", local.clone(), "");
            }
        }
        // a web place is named by its host; the Hub's operations say which way the files go
        let hub = HUB_READ.contains(&op) || HUB_WRITE.contains(&op) || HUB_MODELS.contains(&op);
        let artifact = if hub { HUB_HOST.to_string() } else { literals.iter().find_map(|l| host(l)).unwrap_or_default() };
        let dir = if HUB_WRITE.contains(&op) { "write" } else if hub { "read" } else { "" };
        return Facts::of(kind, artifact, dir);
    }
    if !kind.starts_with("disk") {
        return Facts::of(kind, String::new(), "");
    }
    // a name that starts with a value (`{name}.csv`) is known only by its format
    let artifact = literals
        .iter()
        .find(|l| path_like(l) && !l.starts_with(['{', '$']))
        .cloned()
        .or_else(|| literals.iter().find_map(|l| extension(l)).map(|e| format!("*.{e}")))
        .or_else(|| format_of(pkg, op).map(|f| format!("*.{f}")))
        .unwrap_or_default();
    Facts::of(kind, artifact, "")
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

/// A boundary whose literal argument names a place someone could want elsewhere: a file or
/// a URL. A table in a statement or a command line is what the call does, not where.
pub fn locates(kind: &str) -> bool {
    kind.starts_with("disk") || kind == "net"
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
pub fn sql(kind: &str, literals: &[String]) -> Option<Vec<(String, &'static str)>> {
    let start = literals.iter().position(|l| verb(l).is_some())?;
    (start + 1..=literals.len()).rev().find_map(|end| parsed(kind, &literals[start..end].join(" "))).or_else(|| statement(kind, &literals[start]))
}

/// One SQL text read by its grammar, in its database's dialect. What the grammar cannot read
/// keeps its leading verb (`INSERT` writes) and names no table.
pub fn statement(kind: &str, text: &str) -> Option<Vec<(String, &'static str)>> {
    parsed(kind, text).or_else(|| verb(text).map(|v| if v.is_empty() { vec![] } else { vec![(String::new(), v)] }))
}

/// Whether a text starts as SQL does: `write` for a verb that changes data or its shape, `read`
/// for one that asks, nothing for a setting.
fn verb(text: &str) -> Option<&'static str> {
    let first = text.split_whitespace().next()?.to_ascii_lowercase();
    let first = first.as_str();
    [(WRITE_VERBS, "write"), (READ_VERBS, "read"), (SETTINGS, "")].into_iter().find(|(verbs, _)| verbs.contains(&first)).map(|(_, dir)| dir)
}

/// A SQL text the grammar reads whole: the sides of its statements.
fn parsed(kind: &str, text: &str) -> Option<Vec<(String, &'static str)>> {
    verb(text)?;
    let sql = sql_holes(text);
    let dialect: Box<dyn Dialect> = match kind {
        "db_sqlite" => Box::new(SQLiteDialect {}),
        "db_duckdb" => Box::new(DuckDbDialect {}),
        _ => Box::new(GenericDialect {}),
    };
    let stmts = Parser::parse_sql(dialect.as_ref(), &sql).or_else(|_| Parser::parse_sql(&GenericDialect {}, &sql)).ok()?;
    let mut out: Vec<(String, &'static str)> = stmts.iter().flat_map(sides).collect();
    out.sort_by_key(|(_, d)| *d != "write");
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
fn sides(st: &Statement) -> Vec<(String, &'static str)> {
    // `WITH t AS (..) INSERT INTO b ..`: the change is the body; what `WITH` names it reads
    if let Statement::Query(q) = st {
        if let SetExpr::Insert(inner) | SetExpr::Update(inner) | SetExpr::Delete(inner) | SetExpr::Merge(inner) = q.body.as_ref() {
            let named = ctes(st);
            let mut out = sides(inner);
            out.retain(|(n, _)| !named.contains(n));
            let read: Vec<(String, &'static str)> = relations(st).into_iter().filter(|n| !n.contains(HOLE) && !out.iter().any(|(m, _)| m == n)).map(|n| (n, "read")).collect();
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
    let dir = if writes { "write" } else { "read" };
    let mut out: Vec<(String, &'static str)> = names.into_iter().enumerate().map(|(i, n)| (n, if writes && i == 0 { "write" } else { "read" })).filter(|(n, _)| !n.contains(HOLE)).collect();
    // a table named at run time is unknown, but which way the statement goes is not
    if !out.iter().any(|(_, d)| *d == dir) {
        out.insert(0, (String::new(), dir));
    }
    out
}

/// The databases whose statements are SQL.
const SQL_DBS: &[&str] = &["db_duckdb", "db_sqlite", "db_sql"];

/// Which way a graph database's data goes, where the Python code says it to the driver: a
/// managed transaction (`session.execute_read(work)`, `execute_write`), a query's routing
/// (`execute_query(q, routing_=RoutingControl.READ)`) or a session's access mode
/// (`driver.session(default_access_mode=READ_ACCESS)`). The query's own text is not read;
/// where the code says nothing, the direction stays open.
pub fn access(kind: &str, op: &str, mode: Option<&str>) -> &'static str {
    if kind == "db_kv" {
        let op = op.to_ascii_lowercase();
        return if ["get", "mget", "hget", "keys", "scan", "exists"].iter().any(|r| op.starts_with(r)) {
            "read"
        } else if ["set", "mset", "hset", "del", "remove", "clear", "incr", "decr", "expire", "lpush", "rpush", "sadd"].iter().any(|w| op.starts_with(w)) {
            "write"
        } else {
            ""
        };
    }
    if kind != "db_graph" {
        return "";
    }
    match op {
        "execute_read" | "read_transaction" => return "read",
        "execute_write" | "write_transaction" => return "write",
        _ => {}
    }
    let mode = mode.map(|m| m.trim_matches(['"', '\'']).rsplit('.').next().unwrap_or("").to_ascii_uppercase()).unwrap_or_default();
    match mode.as_str() {
        "READ" | "R" | "READ_ACCESS" => "read",
        "WRITE" | "W" | "WRITE_ACCESS" => "write",
        _ => "",
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

    /// A machine-learning library computes in memory; only loading weights, moving files to
    /// and from the Hub, and checkpoints cross to a place.
    #[test]
    fn a_library_is_not_a_place() {
        assert_eq!(boundary(Some("torch"), &parts(&["torch", "cat"])), None);
        assert_eq!(boundary(Some("torch"), &parts(&["nn", "Linear"])), None);
        assert_eq!(boundary(Some("transformers"), &parts(&["AutoModel", "from_pretrained"])), Some("net"), "weights come down from the Hub");
        assert_eq!(boundary(Some("torch"), &parts(&["torch", "save"])), Some("disk_write"));
        assert_eq!(boundary(Some("huggingface_hub"), &parts(&["snapshot_download"])), Some("net"));
        assert_eq!(boundary(Some("huggingface_hub"), &parts(&["HfApi"])), None, "building a client moves nothing");
        assert_eq!(boundary(Some("datasets"), &parts(&["load_dataset"])), Some("net"));
        let down = facts("net", Some("huggingface_hub"), "snapshot_download", &lits(&["dataset"]));
        assert_eq!((down.artifact.as_str(), down.dir), ("huggingface.co", "read"), "the Hub is a host; a download reads");
        assert_eq!(facts("net", Some("huggingface_hub"), "upload_folder", &[]).dir, "write");
        assert_eq!(facts("net", Some("requests"), "get", &lits(&["https://api.example.org/v1/x"])).artifact, "api.example.org");
        let model = facts("net", Some("transformers"), "from_pretrained", &lits(&["facebook/dinov2-small", "auto"]));
        assert_eq!((model.kind, model.artifact.as_str(), model.dir), ("net", "huggingface.co", "read"));
        assert_eq!(facts("net", Some("transformers"), "from_pretrained", &lits(&["./checkpoints/best"])).kind, "disk_read", "a local checkpoint is a file");
    }

    #[test]
    fn a_library_names_what_it_is() {
        let p = |xs: &[&str]| xs.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(boundary_rust(&p(&["reqwest", "Client", "new"])), None, "building a client moves nothing");
        assert_eq!(boundary_rust(&p(&["reqwest", "get"])), Some("net"));
        assert_eq!(boundary_rust(&p(&["rusqlite", "Connection", "open"])), Some("db_sqlite"), "one database, one name in every language");
        assert_eq!(library_place("neo4j"), Some("db_graph"), "the Bolt driver reaches a graph database, whichever");
        assert_eq!(library_place("duckdb"), Some("db_duckdb"));
        let local = facts("net", Some("datasets"), "load_dataset", &p(&["csv", "data/train.csv"]));
        assert_eq!((local.kind, local.artifact.as_str()), ("disk_read", "data/train.csv"), "a builtin loader reads local files");
        assert!(requests(&["url", "params"], "get") && !requests(&["key", "default"], "get"), "a request is handed a URL");
    }

    #[test]
    fn files_are_named_by_literal_then_format() {
        assert_eq!(facts("disk_write", None, "write_text", &lits(&["out.json"])).artifact, "out.json");
        assert_eq!(facts("disk_write", None, "to_csv", &lits(&[";"])).artifact, "*.csv");
        assert_eq!(facts("disk_write", None, "savefig", &lits(&["{name}.svg"])).artifact, "*.svg", "a name made of a value is its format");
        assert_eq!(facts("disk_write", None, "savefig", &lits(&["plots/{name}.svg"])).artifact, "plots/{name}.svg");
        assert_eq!(facts("disk_write", Some("json"), "dump", &[]).artifact, "*.json");
        assert_eq!(facts("disk_read", None, "read_text", &lits(&["utf-8"])).artifact, "");
        assert_eq!(facts("disk_read", None, "read_to_string", &lits(&["Makefile"])).artifact, "Makefile");
    }

    #[test]
    fn open_mode_decides_read_or_write() {
        assert_eq!(facts("disk", None, "open", &lits(&["w"])).kind, "disk_write");
        assert_eq!(facts("disk", None, "open", &lits(&["data.csv"])).kind, "disk_read");
        assert_eq!(facts("disk", None, "open", &lits(&["data.csv", "w"])).artifact, "data.csv");
    }

    /// SQL is read by its grammar: a statement writes the table it changes and reads the ones it
    /// takes from; a setting names no table; a name filled in at run time is no table.
    #[test]
    fn statements_read_or_write_tables() {
        let s = |kind: &str, text: &str| statement(kind, text).unwrap();
        let t = |xs: &[(&str, &'static str)]| xs.iter().map(|(n, d)| (n.to_string(), *d)).collect::<Vec<_>>();
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
        assert_eq!(statement("db_sqlite", "hello world"), None);
        assert_eq!(sql("db_sqlite", &lits(&["SELECT a ", "FROM t"])), Some(t(&[("t", "read")])), "a statement split over literals");
        assert_eq!(sql("db_sqlite", &lits(&["INSERT INTO verbruik (moment, ", "model) VALUES (?, ?)", "seconds"])), Some(t(&[("verbruik", "write")])), "and the call's other literals");
    }

    /// A graph database's direction is what the Python code says to the driver, never the query.
    #[test]
    fn a_driver_says_which_way() {
        assert_eq!(access("db_graph", "execute_read", None), "read");
        assert_eq!(access("db_graph", "execute_write", None), "write");
        assert_eq!(access("db_graph", "execute_query", Some("RoutingControl.READ")), "read");
        assert_eq!(access("db_graph", "session", Some("neo4j.WRITE_ACCESS")), "write");
        assert_eq!(access("db_graph", "run", None), "", "a query alone says nothing");
        assert_eq!(facts("db_graph", Some("neo4j"), "run", &lits(&["MATCH (n:Regeling) RETURN n"])), Facts::of("db_graph", String::new(), ""));
    }
}
