//! Boundaries: calls that leave the package, classified by the part of the world they
//! touch. The engine knows libraries, never a project.

use std::ops::ControlFlow;

use sqlparser::ast::{visit_relations, Query, SetExpr, Statement, Visit, Visitor};
use sqlparser::dialect::{Dialect, DuckDbDialect, GenericDialect, SQLiteDialect};
use sqlparser::parser::Parser;

use crate::knowledge::{self as k, Registry};
pub use crate::model::Crossing;
use crate::model::{Db, Dir, World};

const DISK_READ: Crossing = Crossing::read(World::Disk);
const DISK_WRITE: Crossing = Crossing::write(World::Disk);

/// Whether a network library's call moves data, by its signature: it is handed a URL (or a
/// socket's address), or it sends or receives on a socket. Building a client, or reading a
/// response that has arrived (`resp.headers.get(..)`), is handed none.
pub fn requests(params: &[&str], op: &str) -> bool {
    params.iter().any(|p| matches!(*p, "url" | "uri" | "address")) || k::SOCKET_OPS.contains(&op)
}

/// Whether a Python package is a network library, whose calls `requests` decides.
pub fn network(pkg: &str) -> bool {
    k::reaches(pkg, Some(Registry::PyPi)) == Some(World::Net)
}

/// Whether a Python package is a client of a place (the web, a language model): building one of
/// its clients moves nothing. A model library's class is no client: building a pretrained model
/// downloads its weights.
pub fn client(pkg: &str) -> bool {
    matches!(k::reaches(pkg, Some(Registry::PyPi)), Some(World::Net | World::Llm))
}

/// The part of the world a library's calls reach, for a library that is a client of one: a
/// database, the network, a language model, a process. A value of its types is a door to that
/// place, not data at rest. A package's own implementation (`_sqlite3`) is the package.
pub fn library_place(pkg: &str) -> Option<World> {
    k::reaches(pkg.strip_prefix('_').unwrap_or(pkg), None)
}

/// Classify a Rust call by its full path, e.g. `std::fs::read_to_string` reads the disk.
pub fn boundary_rust(parts: &[String]) -> Option<Crossing> {
    let head = parts.first().map(String::as_str).unwrap_or("");
    let last = parts.last().map(String::as_str).unwrap_or("");
    let has = |p: &str| parts.iter().any(|x| x == p);
    // building moves nothing: `Client::new()`, `Client::builder()..build()`, `OpenOptions::new()`
    let builds = k::BUILDS.contains(&last);
    if has(k::RUST_FS_MODULE) {
        if builds {
            return None;
        }
        return Some(if k::RUST_FS_READS.contains(&last) {
            DISK_READ
        } else if k::RUST_FS_WRITES.contains(&last) {
            DISK_WRITE
        } else {
            Crossing::to(World::Disk)
        });
    }
    // a crate's type that opens the file it is handed: `csv::Reader::from_path(p)`
    if let [.., ty, f] = parts {
        if let Some((_, _, dir)) = k::RUST_FILE_OPENERS.iter().find(|(t, op, _)| t == ty && op == f) {
            return Some(Crossing { world: World::Disk, dir: Some(*dir) });
        }
    }
    // what the human types: `io::stdin()`; what the human is shown: `io::stderr()`, as `eprintln!`
    if let [.., module, name] = parts {
        if k::INPUT_STREAMS.contains(&(module.as_str(), name.as_str())) {
            return Some(Crossing::read(World::Screen));
        }
        if k::STREAMS.contains(&(module.as_str(), name.as_str())) {
            return Some(Crossing::to(World::Screen));
        }
    }
    // a socket's traffic: `TcpStream::connect(addr)`, `conn.read(..)`; binding or a timeout moves none
    if has(k::RUST_NET_MODULE) {
        return k::RUST_SOCKET_OPS.contains(&last).then_some(Crossing::to(World::Net));
    }
    // a program is run by a `Command`; `process::exit` ends this one and runs none
    if has(k::RUST_COMMAND) {
        return Some(Crossing::to(World::Process));
    }
    if k::RUST_FILE_TREES.contains(&head) {
        return Some(DISK_READ);
    }
    match k::reaches(head, Some(Registry::Crates))? {
        World::Net => (!builds).then_some(Crossing::to(World::Net)),
        w => Some(Crossing::to(w)),
    }
}

/// Whether these tables say what a package of a registry does, or that it is the language's own.
/// Its code is then not read: its calls are places, or nothing. A crate is named as the code
/// names it (`tokio_postgres`).
pub fn described(pkg: &str, from: Registry) -> bool {
    if k::reaches(pkg, Some(from)).is_some() || k::OPERATIONS.iter().any(|(p, _, _)| *p == pkg) {
        return true;
    }
    match from {
        Registry::PyPi => k::ML_PACKAGES.contains(&pkg) || k::FILE_PACKAGES.contains(&pkg) || k::PYTHON_OWN.contains(&pkg),
        Registry::Npm => {
            k::NODE_BUILTINS.contains(&pkg)
                || k::NODE_FS.contains(&pkg)
                || k::ROUTERS.contains(&pkg)
                || k::MOUNTS.iter().any(|m| m.package == pkg)
                || k::PROMISIFIERS.iter().any(|(p, _)| *p == pkg)
                || k::OPTION_PARSERS.iter().any(|o| o.package == pkg)
                || k::PATH_FUNCTIONS.iter().any(|(p, _, _)| *p == pkg)
                || k::TYPESCRIPT_OWN_TYPES.packages.contains(&pkg)
        }
        Registry::Crates => {
            k::RUST_OWN_TYPES.packages.contains(&pkg)
                || k::RUST_FILE_TREES.contains(&pkg)
                || k::HANDS_ON.iter().any(|(c, _)| *c == pkg)
                || k::DERIVED_PARSERS.iter().any(|d| d.package == pkg)
                || k::RUST_PATH_FUNCTIONS.iter().any(|(m, _, _)| *m == pkg)
                || k::RUST_NAMED_BY_TYPE.contains(&pkg)
        }
    }
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
    let builtin = |name: &str| k::PYTHON_BUILTINS.iter().find(|(b, _)| *b == name).map(|(_, c)| *c);
    let Some(pkg) = pkg else {
        return if parts.len() == 1 { builtin(head) } else { None };
    };
    if pkg == "builtins" {
        return builtin(last);
    }
    // what prints, shows a figure, draws one, runs a model or saves a file, by itself
    let stream = parts.len() == 3 && k::STREAMS.contains(&(pkg, parts[1].as_str()));
    if stream {
        return Some(Crossing::to(World::Screen));
    }
    if parts.len() == 3 && k::INPUT_STREAMS.contains(&(pkg, parts[1].as_str())) {
        return Some(Crossing::read(World::Screen));
    }
    if let Some(c) = k::operation(pkg, last).filter(|c| matches!(c.world, World::Screen | World::Figure)) {
        return Some(c);
    }
    match k::reaches(pkg, Some(Registry::PyPi)) {
        Some(World::Net) => return k::moves_on_the_web(last).then_some(Crossing::to(World::Net)),
        // a driver's call that runs or opens something, or whose name says which way a key goes
        Some(World::Db(db)) => {
            return (k::reaches_the_database(pkg, last) || access(db, last, None).is_some()).then_some(Crossing::to(World::Db(db)))
        }
        Some(World::Llm) => return Some(Crossing::to(World::Llm)),
        _ => {}
    }
    if k::ML_PACKAGES.contains(&pkg) {
        return ml(pkg, parts, last);
    }
    // what any other library's operation crosses by itself: `os.system` runs a program,
    // `pd.read_sql` reads a database
    if let Some(c) = k::operation(pkg, last) {
        return Some(c);
    }
    if k::reaches(pkg, Some(Registry::PyPi)) == Some(World::Process) {
        return Some(Crossing::to(World::Process));
    }
    if k::FILE_PACKAGES.contains(&pkg) {
        if k::FILE_WRITES.contains(&last) && (!k::FILE_SYSTEM_OPS.contains(&last) || k::FILE_SYSTEM_PACKAGES.contains(&pkg)) {
            return Some(DISK_WRITE);
        }
        if k::FILE_READS.contains(&last) {
            return Some(DISK_READ);
        }
    }
    None
}

/// The calls of a machine-learning library that cross to a place: pretrained weights load a
/// model, the Hub sends and receives files, a checkpoint or a saved dataset is a file.
fn ml(pkg: &str, parts: &[String], last: &str) -> Option<Crossing> {
    let net = Some(Crossing::to(World::Net));
    let hub_load = k::MODEL_HUBS.iter().any(|(p, hub, op)| *p == pkg && parts.iter().any(|x| x == hub) && last == *op);
    if hub_load || k::HUB_MODELS.contains(&last) {
        return net;
    }
    if k::HUB_PACKAGES.contains(&pkg) && (k::HUB_READ.contains(&last) || k::HUB_WRITE.contains(&last)) {
        return net;
    }
    k::operation(pkg, last)
}

/// The boundary a TypeScript or JavaScript call crosses: `fetch` and HTTP clients go to the
/// web, `console` to the screen, `node:fs` reads and writes files, `child_process` runs a
/// program. `pkg` is the package the call's head was imported from, if any.
pub fn boundary_ts(pkg: Option<&str>, parts: &[String]) -> Option<Crossing> {
    let last = parts.last().map(String::as_str).unwrap_or("");
    let Some(pkg) = pkg else { return platform(parts) };
    // what shows the human something or reads what they type, by the package's own operation
    if let Some(c) = k::operation(pkg, last).filter(|c| c.world == World::Screen) {
        return Some(c);
    }
    // building a client moves nothing: `axios.create(..)`
    if k::CLIENT_MAKERS.contains(&last) && parts.len() == 2 && k::reaches(pkg, Some(Registry::Npm)) != Some(World::Llm) {
        return None;
    }
    // a request, by its method, or the package called as one (`got(url)`); a response read moves nothing
    if k::reaches(pkg, Some(Registry::Npm)) == Some(World::Net) {
        return (parts.len() == 1 || k::moves_on_the_web(last) || last == "fetch").then_some(Crossing::to(World::Net));
    }
    if let Some(w) = library_place(pkg) {
        return Some(Crossing::to(w));
    }
    if k::NODE_FS.contains(&pkg) {
        return if k::NODE_FS_READS.contains(&last) {
            Some(DISK_READ)
        } else if k::NODE_FS_WRITES.contains(&last) {
            Some(DISK_WRITE)
        } else {
            None
        };
    }
    None
}

/// What the platform's own names cross to, where no import binds them: `fetch` and a socket or
/// an event stream to the web (also through `window` or `globalThis`), `console` and the
/// process's streams to the screen, the browser's own key-value stores.
fn platform(parts: &[String]) -> Option<Crossing> {
    let parts: Vec<&str> = parts.iter().map(String::as_str).collect();
    let parts = match parts.as_slice() {
        [g, rest @ ..] if k::GLOBAL_OBJECTS.contains(g) && !rest.is_empty() => rest,
        p => p,
    };
    let (head, last) = (parts.first().copied().unwrap_or(""), parts.last().copied().unwrap_or(""));
    if parts.len() == 1 && k::WEB_GLOBALS.contains(&head) {
        return Some(Crossing::to(World::Net));
    }
    if head == k::CONSOLE || (parts.len() > 1 && k::STREAMS.contains(&(head, parts[1]))) {
        return Some(Crossing::to(World::Screen));
    }
    if parts.len() > 1 && k::INPUT_STREAMS.contains(&(head, parts[1])) {
        return Some(Crossing::read(World::Screen));
    }
    (k::BROWSER_STORES.contains(&head) && k::BROWSER_STORE_OPS.contains(&last)).then_some(Crossing::to(World::Db(Db::Kv)))
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
    let starts = |table: &[&str]| table.iter().any(|t| op.starts_with(t));
    if k::PROBES.contains(&op) {
        FsOp::Checks
    } else if k::LISTINGS.contains(&op) {
        FsOp::Lists
    } else if starts(k::CREATES) {
        FsOp::Creates
    } else if starts(k::REMOVES) {
        FsOp::Removes
    } else if starts(k::MOVES) {
        FsOp::Moves
    } else if starts(k::COPIES) {
        FsOp::Copies
    } else {
        FsOp::Data
    }
}

/// `println!` and friends print to the terminal.
pub fn screen_macro(name: &str) -> bool {
    k::RUST_SCREEN_MACROS.contains(&name)
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
            // a key-value store's place is the key it is handed; without a statement, the way the
            // data goes is what the operation's name says, else what its library's table says
            // (`to_sql` writes)
            let key = if db == Db::Kv { literals.first().cloned().unwrap_or_default() } else { String::new() };
            return Facts::of(Crossing { world: kind.world, dir: access(db, op, None).or(kind.dir) }, key);
        };
        let (artifact, dir) = sides.remove(0);
        return Facts { kind: kind.world, artifact, dir, also: sides };
    }
    if kind.world == World::Net {
        // a dataset built by one of the library's own loaders (`load_dataset("csv", data_files=..)`)
        // is read from local files
        if op == k::HUB_LOADER && literals.first().map_or(false, |l| k::HF_LOADERS.contains(&l.as_str())) {
            return Facts::of(DISK_READ, literals.iter().skip(1).find(|l| path_like(l)).cloned().unwrap_or_default());
        }
        // pretrained weights named by a local path are a file, by a name a download from the Hub
        if k::HUB_MODELS.contains(&op) {
            if let Some(local) = literals.first().filter(|l| l.starts_with(['.', '/', '~'])) {
                return Facts::of(DISK_READ, local.clone());
            }
        }
        // a web place is named by its host; the Hub's operations say which way the files go, and a
        // request named for its method goes that method's way
        let hub = k::HUB_READ.contains(&op) || k::HUB_WRITE.contains(&op) || k::HUB_MODELS.contains(&op);
        let artifact = if hub { k::HUB_HOST.to_string() } else { literals.iter().find_map(|l| host(l)).unwrap_or_default() };
        let dir = if k::HUB_WRITE.contains(&op) {
            Some(Dir::Write)
        } else if hub {
            Some(Dir::Read)
        } else {
            k::request_way(op)
        };
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

/// Path operations whose place is the path they are called on; their argument is a pattern
/// below it or what is written: `cache_dir.glob("*.arrow")`, `p.write_text("done")`.
pub fn on_receiver(op: &str) -> bool {
    k::ON_THE_PATH.contains(&op)
}

/// The host of a URL: `https://huggingface.co/api/x` -> `huggingface.co`.
fn host(s: &str) -> Option<String> {
    let rest = s.split_once("://")?.1;
    let h = rest.split(['/', '?', '#']).next()?;
    (!h.is_empty() && !h.contains('{')).then(|| h.to_string())
}

/// A file mode literal: `r`, `wb`, `a+`.
pub fn is_mode(s: &str) -> bool {
    (1..=3).contains(&s.len()) && s.chars().all(|c| "rwabxt+".contains(c))
}

/// The format a call implies when the code names no file.
fn format_of(pkg: Option<&str>, op: &str) -> Option<&'static str> {
    let find = |table: &'static [(&'static str, &'static str)], key: &str| table.iter().find(|(x, _)| *x == key).map(|(_, f)| *f);
    let (array, ops, format) = k::ARRAY_FORMAT;
    find(k::FORMATS_BY_OP, op)
        .or_else(|| find(k::FORMATS_BY_PACKAGE, pkg?))
        .or_else(|| (pkg? == array && ops.contains(&op)).then_some(format))
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
    [(WRITE_VERBS, Some(Dir::Write)), (READ_VERBS, Some(Dir::Read)), (SETTINGS, None)]
        .into_iter()
        .find(|(verbs, _)| verbs.contains(&first))
        .map(|(_, dir)| dir)
}

/// A SQL text the grammar reads whole: the sides of its statements.
fn parsed(db: Db, text: &str) -> Option<Vec<(String, Option<Dir>)>> {
    let stmts = grammar(db, text)?;
    let mut out: Vec<(String, Option<Dir>)> = stmts.iter().flat_map(sides).map(|(n, d)| (n, Some(d))).collect();
    out.sort_by_key(|(_, d)| *d != Some(Dir::Write));
    out.dedup();
    Some(out)
}

/// The statements of a SQL text that starts with a verb, as its database's dialect reads them,
/// each placeholder a name.
fn grammar(db: Db, text: &str) -> Option<Vec<Statement>> {
    verb(text)?;
    let sql = sql_holes(text);
    let dialect: Box<dyn Dialect> = match db {
        Db::Sqlite => Box::new(SQLiteDialect {}),
        Db::Duckdb => Box::new(DuckDbDialect {}),
        _ => Box::new(GenericDialect {}),
    };
    Parser::parse_sql(dialect.as_ref(), &sql).or_else(|_| Parser::parse_sql(&GenericDialect {}, &sql)).ok()
}

/// Whether a statement names its tables whatever its placeholders hold: its verb is written out,
/// the grammar reads it, and no table it names is a placeholder. A placeholder anywhere else
/// holds a value (`VALUES ('{station}', {t})`, `WHERE id = {id}`), which names no place.
pub fn names_its_tables(db: Db, text: &str) -> bool {
    grammar(db, text).map_or(false, |stmts| stmts.iter().all(|st| tables(st).iter().all(|n| !n.contains(HOLE))))
}

/// Whether a statement may write what its call is handed: a placeholder or a value spliced in
/// stands among the values it inserts or sets. A `DELETE` writes none of it, nor does an `UPDATE`
/// whose `SET` holds only what the statement says itself: what such a statement is handed is the
/// key it finds its rows by (`WHERE member = ?`). A text the grammar does not read may write
/// anything it is handed.
pub fn writes_handed(db: Db, text: &str) -> bool {
    grammar(db, text).map_or(true, |stmts| stmts.iter().any(writes_handed_in))
}

/// Whether one statement may write what its call is handed (`writes_handed`).
fn writes_handed_in(st: &Statement) -> bool {
    match st {
        Statement::Delete(_) | Statement::Truncate(_) | Statement::Drop { .. } => false,
        Statement::Update(u) => holds_handed(&u.assignments),
        Statement::Insert(i) => holds_handed(&i.source) || holds_handed(&i.assignments) || holds_handed(&i.on),
        Statement::Query(q) => match q.body.as_ref() {
            SetExpr::Insert(inner) | SetExpr::Update(inner) | SetExpr::Delete(inner) => writes_handed_in(inner),
            _ => true,
        },
        _ => true,
    }
}

/// Whether a part of a statement holds a value its call is handed: a placeholder (`?`, `$1`,
/// `:name`), or a value spliced into the text (`'{station}'`, `{t}`).
fn holds_handed(part: &impl Visit) -> bool {
    use sqlparser::ast::{visit_expressions, Expr, Value};
    let spliced = |s: &str| s.contains(HOLE);
    visit_expressions(part, |e| {
        let handed = match e {
            Expr::Value(v) => matches!(v.value, Value::Placeholder(_)) || spliced(&v.value.to_string()),
            Expr::Identifier(i) => spliced(&i.value),
            Expr::CompoundIdentifier(is) => is.iter().any(|i| spliced(&i.value)),
            _ => false,
        };
        if handed {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    })
    .is_break()
}

/// The verbs a statement starts with: those that change data or its shape, those that ask, and
/// the settings around them.
const WRITE_VERBS: &[&str] = &["insert", "update", "delete", "create", "merge", "copy", "drop", "alter", "replace", "upsert", "truncate"];
const READ_VERBS: &[&str] = &["select", "with", "values", "show", "describe", "call"];
const SETTINGS: &[&str] = &[
    "pragma",
    "set",
    "install",
    "load",
    "begin",
    "commit",
    "rollback",
    "attach",
    "detach",
    "vacuum",
    "analyze",
    "explain",
    "checkpoint",
    "use",
];

/// A placeholder the code fills in at run time (`{tabel}`, `${t}`, `%s`) as a name the grammar
/// accepts and Atlas does not take for a table.
const HOLE: &str = "atlas_hole";
fn sql_holes(text: &str) -> String {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"\$?\{[^{}]*\}|%\([A-Za-z_]\w*\)s|%s").unwrap());
    let mut n = 0;
    re.replace_all(text, |c: &regex::Captures| {
        n += 1;
        if c[0].starts_with('%') {
            "?".to_string()
        } else {
            format!("{HOLE}{n}")
        }
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

/// The tables a statement names: those it drops, or every one it reads or changes.
fn tables(st: &Statement) -> Vec<String> {
    match st {
        Statement::Drop { names, .. } => names.iter().map(|n| n.to_string().trim_matches(['"', '`']).to_string()).collect(),
        _ => relations(st),
    }
}

/// How a table a statement makes temporary is named among its sides (`temp:staging`): the
/// connection's working memory, no place. The builder drops every touch of such a table.
pub const SCRATCH: &str = "temp:";

/// How a table a statement gives its shape is named among its sides (`schema:readings`): a table
/// created, altered or indexed holds no new content for it. The builder names the table and makes
/// [`SCHEMA`] the touch's operation.
pub const SHAPED: &str = "schema:";

/// The operation of a touch that gives a table its shape.
pub const SCHEMA: &str = "schema";

/// The operation of a touch that tests whether a value holds a key (`k in d`): it looks at what the
/// value holds, as `path.exists()` looks at the disk, not at its content.
pub const MEMBERSHIP: &str = "in";

/// The tables one statement writes and reads: a statement that changes data writes the first
/// table it names (`INSERT INTO a SELECT .. FROM b`) and reads the others; a query reads all of
/// them; a setting names none. A temporary table (`CREATE TEMP TABLE staging ..`) is named as
/// scratch; a table a statement only gives its shape (`CREATE TABLE IF NOT EXISTS`, `ALTER TABLE`,
/// `CREATE INDEX`, a view) as shaped.
fn sides(st: &Statement) -> Vec<(String, Dir)> {
    // the table it makes or shapes: the first it names, unnamed when its name is filled in at run time
    let first = || tables(st).into_iter().next().filter(|n| !n.contains(HOLE)).unwrap_or_default();
    let made = |prefix: &str| vec![(format!("{prefix}{}", first()), Dir::Write)];
    match st {
        // a temporary table named at run time is no place either
        Statement::CreateTable(c) if c.temporary => return if first().is_empty() { vec![] } else { made(SCRATCH) },
        Statement::CreateTable(c) if c.query.is_none() => return made(SHAPED),
        Statement::CreateIndex(_) | Statement::AlterTable(_) | Statement::CreateView(_) => return made(SHAPED),
        _ => {}
    }
    // `WITH t AS (..) INSERT INTO b ..`: the change is the body; what `WITH` names it reads
    if let Statement::Query(q) = st {
        if let SetExpr::Insert(inner) | SetExpr::Update(inner) | SetExpr::Delete(inner) | SetExpr::Merge(inner) = q.body.as_ref() {
            let named = ctes(st);
            let mut out = sides(inner);
            out.retain(|(n, _)| !named.contains(n));
            let read: Vec<(String, Dir)> = relations(st)
                .into_iter()
                .filter(|n| !n.contains(HOLE) && !out.iter().any(|(m, _)| m == n))
                .map(|n| (n, Dir::Read))
                .collect();
            out.extend(read);
            return out;
        }
    }
    let names = tables(st);
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
    let mut out: Vec<(String, Dir)> = names
        .into_iter()
        .enumerate()
        .map(|(i, n)| (n, if writes && i == 0 { Dir::Write } else { Dir::Read }))
        .filter(|(n, _)| !n.contains(HOLE))
        .collect();
    // a table named at run time is unknown, but which way the statement goes is not
    if !out.iter().any(|(_, d)| *d == dir) {
        out.insert(0, (String::new(), dir));
    }
    out
}

/// The databases whose statements are SQL.
const SQL_DBS: &[Db] = &[Db::Duckdb, Db::Sqlite, Db::Sql];

/// Whether a database's statements are read by their grammar.
pub fn reads_statements(db: Db) -> bool {
    SQL_DBS.contains(&db)
}

/// Whether a text starts as a statement does: with a verb that asks, changes or sets.
pub fn starts_statement(text: &str) -> bool {
    verb(text).is_some()
}

/// Which way a graph database's data goes, where the Python code says it to the driver: a
/// managed transaction (`session.execute_read(work)`, `execute_write`), a query's routing
/// (`execute_query(q, routing_=RoutingControl.READ)`) or a session's access mode
/// (`driver.session(default_access_mode=READ_ACCESS)`). The query's own text is not read;
/// where the code says nothing, the direction stays open.
pub fn access(db: Db, op: &str, mode: Option<&str>) -> Option<Dir> {
    if db == Db::Kv {
        let op = op.to_ascii_lowercase();
        let starts = |table: &[&str]| table.iter().any(|t| op.starts_with(t));
        return if starts(k::KV_READS) {
            Some(Dir::Read)
        } else if starts(k::KV_WRITES) {
            Some(Dir::Write)
        } else {
            None
        };
    }
    if db != Db::Graph {
        return None;
    }
    if let Some((_, d)) = k::TRANSACTIONS.iter().find(|(t, _)| *t == op) {
        return Some(*d);
    }
    let mode = mode.map(|m| m.trim_matches(['"', '\'']).rsplit('.').next().unwrap_or("").to_ascii_uppercase()).unwrap_or_default();
    k::ACCESS_MODES.iter().find(|(m, _)| *m == mode).map(|(_, d)| *d)
}

/// A managed transaction, handed the work it runs: `session.execute_read(work, ..)`.
pub fn transaction(op: &str) -> bool {
    k::TRANSACTIONS.iter().any(|(t, _)| *t == op)
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
        assert_eq!(
            name(boundary(Some("transformers"), &parts(&["AutoModel", "from_pretrained"]))),
            Some("net"),
            "weights come down from the Hub"
        );
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
        assert_eq!(
            kind(&facts(c("net"), Some("transformers"), "from_pretrained", &lits(&["./checkpoints/best"]))),
            "disk_read",
            "a local checkpoint is a file"
        );
    }

    /// What a library's call reaches is a fact of its table: a driver's own queries, a key-value
    /// store's verbs, a frame's database, a program run, a model run.
    #[test]
    fn a_table_says_where_a_call_goes() {
        let b = |pkg: &str, xs: &[&str]| boundary(Some(pkg), &parts(xs));
        let n = |pkg: &str, xs: &[&str]| name(b(pkg, xs));
        assert_eq!(n("asyncpg", &["conn", "fetchval"]), Some("db_sql"), "asyncpg's query hands back its rows");
        assert_eq!(n("neo4j", &["result", "fetch"]), None, "a driver's own query is no other driver's");
        assert_eq!(n("psycopg", &["cur", "fetchall"]), None, "reading the rows a statement gave back");
        assert_eq!(n("redis", &["r", "hgetall"]), Some("db_kv"));
        assert_eq!(n("redis", &["redis", "Redis"]), None, "building a client moves nothing");
        assert_eq!((dir(access(Db::Kv, "zadd", None)), dir(access(Db::Kv, "smembers", None))), ("write", "read"));
        let to_sql = b("pandas", &["frame", "to_sql"]).unwrap();
        let written = facts(to_sql, Some("pandas"), "to_sql", &lits(&["readings", "append"]));
        assert_eq!(
            (kind(&written), written.artifact.as_str(), dir(written.dir)),
            ("db_sql", "", "write"),
            "a table handed by its name is no statement"
        );
        let read = facts(b("pandas", &["pd", "read_sql"]).unwrap(), Some("pandas"), "read_sql", &lits(&["SELECT * FROM readings"]));
        assert_eq!((read.artifact.as_str(), dir(read.dir)), ("readings", "read"));
        assert_eq!(n("pandas", &["pd", "read_csv"]), Some("disk_read"), "a frame's file is still a file");
        assert_eq!(n("os", &["os", "system"]), Some("process"));
        assert_eq!(n("os", &["os", "listdir"]), Some("disk_read"));
        assert_eq!(n("asyncio", &["asyncio", "create_subprocess_exec"]), Some("process"));
        assert_eq!(n("asyncio", &["proc", "wait"]), None);
        assert_eq!(n("nbclient", &["NotebookClient", "execute"]), Some("process"), "a notebook run in a kernel");
        assert_eq!(n("transformers", &["model", "generate"]), Some("model"));
    }

    #[test]
    fn a_library_names_what_it_is() {
        let p = |xs: &[&str]| xs.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(name(boundary_rust(&p(&["reqwest", "Client", "new"]))), None, "building a client moves nothing");
        assert_eq!(name(boundary_rust(&p(&["reqwest", "get"]))), Some("net"));
        assert_eq!(
            name(boundary_rust(&p(&["rusqlite", "Connection", "open"]))),
            Some("db_sqlite"),
            "one database, one name in every language"
        );
        assert_eq!(library_place("neo4j").map(World::as_str), Some("db_graph"), "the Bolt driver reaches a graph database, whichever");
        assert_eq!(library_place("duckdb").map(World::as_str), Some("db_duckdb"));
        let local = facts(c("net"), Some("datasets"), "load_dataset", &p(&["csv", "data/train.csv"]));
        assert_eq!((kind(&local), local.artifact.as_str()), ("disk_read", "data/train.csv"), "a builtin loader reads local files");
        assert!(requests(&["url", "params"], "get") && !requests(&["key", "default"], "get"), "a request is handed a URL");
    }

    /// Isomorphic is the same: a socket is the network in Python and in Rust, and the operations
    /// that say the same thing cross alike. Connecting, sending and receiving reach it; binding an
    /// address, accepting a connection, shutting a half and setting a timeout move no data.
    #[test]
    fn a_socket_moves_data_alike_in_python_and_rust() {
        let rs = |xs: &[&str]| name(boundary_rust(&xs.iter().map(|s| s.to_string()).collect::<Vec<_>>()));
        let py = |op: &str| name(boundary(Some("socket"), &parts(&["sock", op])));
        let alike = [
            ("connect", "TcpStream", "connect"),
            ("recv", "TcpStream", "read"),
            ("recv_into", "TcpStream", "read_exact"),
            ("send", "TcpStream", "write"),
            ("sendall", "TcpStream", "write_all"),
            ("shutdown", "TcpStream", "shutdown"),
            ("settimeout", "TcpStream", "set_read_timeout"),
            ("bind", "TcpListener", "bind"),
            ("accept", "TcpListener", "accept"),
            ("sendto", "UdpSocket", "send_to"),
            ("recvfrom", "UdpSocket", "recv_from"),
        ];
        for (p, ty, r) in alike {
            assert_eq!(py(p), rs(&["std", "net", ty, r]), "Python's `socket.{p}` and Rust's `{ty}::{r}`");
        }
        assert_eq!(py("recv"), Some("net"), "a socket's traffic reaches the network");
        assert_eq!(rs(&["std", "net", "IpAddr", "from_str"]), None, "an address parsed moves nothing");
    }

    #[test]
    fn files_are_named_by_literal_then_format() {
        assert_eq!(facts(c("disk_write"), None, "write_text", &lits(&["out.json"])).artifact, "out.json");
        assert_eq!(facts(c("disk_write"), None, "to_csv", &lits(&[";"])).artifact, "*.csv");
        assert_eq!(
            facts(c("disk_write"), None, "savefig", &lits(&["{name}.svg"])).artifact,
            "*.svg",
            "a name made of a value is its format"
        );
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
    /// takes from; one that only gives a table its shape names it as shaped; a setting names no
    /// table; a name filled in at run time is no table.
    #[test]
    fn statements_read_or_write_tables() {
        let s = |kind: &str, text: &str| statement(db(kind), text).unwrap();
        let t = |xs: &[(&str, &'static str)]| xs.iter().map(|(n, d)| (n.to_string(), Dir::parse(d))).collect::<Vec<_>>();
        assert_eq!(s("db_sqlite", "INSERT INTO nodes VALUES (?)"), t(&[("nodes", "write")]));
        assert_eq!(s("db_duckdb", "select * from edges e join nodes n on e.a = n.id"), t(&[("edges", "read"), ("nodes", "read")]));
        assert_eq!(s("db_sqlite", "INSERT INTO log SELECT * FROM oordeel WHERE status = :s"), t(&[("log", "write"), ("oordeel", "read")]));
        assert_eq!(
            s("db_sqlite", "WITH t AS (SELECT * FROM a) INSERT INTO b SELECT * FROM t"),
            t(&[("b", "write"), ("a", "read")]),
            "`t` names a subquery, not a table"
        );
        assert_eq!(s("db_sqlite", "CREATE TABLE IF NOT EXISTS oordeel (uuid TEXT PRIMARY KEY)"), t(&[("schema:oordeel", "write")]));
        assert_eq!(s("db_sqlite", "CREATE INDEX op_station ON readings (station)"), t(&[("schema:readings", "write")]));
        assert_eq!(
            s("db_duckdb", "CREATE TABLE totals AS SELECT * FROM readings"),
            t(&[("totals", "write"), ("readings", "read")]),
            "a table made from a query holds what it read"
        );
        assert_eq!(s("db_duckdb", "CREATE TEMP TABLE staging (id INT)"), t(&[("temp:staging", "write")]));
        assert_eq!(s("db_duckdb", "DROP TABLE IF EXISTS locaties"), t(&[("locaties", "write")]));
        assert_eq!(s("db_sqlite", "PRAGMA user_version = 3"), t(&[]), "a setting names no table");
        assert_eq!(s("db_sqlite", "PRAGMA journal_mode=WAL"), t(&[]), "nor when the grammar does not read it");
        assert_eq!(s("db_sqlite", "ALTER TABLE {tabel} ADD COLUMN x TEXT"), t(&[("schema:", "write")]), "a table named at run time");
        assert_eq!(s("db_sqlite", "SELECT count(*) FROM {tabel}"), t(&[("", "read")]));
        assert_eq!(statement(db("db_sqlite"), "hello world"), None);
        assert_eq!(sql(db("db_sqlite"), &lits(&["SELECT a ", "FROM t"])), Some(t(&[("t", "read")])), "a statement split over literals");
        assert_eq!(
            sql(db("db_sqlite"), &lits(&["INSERT INTO verbruik (moment, ", "model) VALUES (?, ?)", "seconds"])),
            Some(t(&[("verbruik", "write")])),
            "and the call's other literals"
        );
    }

    /// A statement names its tables whatever values fill its placeholders; one whose table or verb
    /// is a placeholder does not.
    #[test]
    fn a_statement_names_its_tables_whatever_values_fill_it() {
        let sqlite = db("db_sqlite");
        assert!(names_its_tables(sqlite, "INSERT INTO readings VALUES ('{station}', {t})"));
        assert!(names_its_tables(sqlite, "SELECT * FROM readings WHERE station = '{station}' ORDER BY {column}"));
        assert!(names_its_tables(sqlite, "UPDATE readings SET unit = %s WHERE station = %s"));
        assert!(!names_its_tables(sqlite, "SELECT count(*) FROM {tabel}"), "a table named at run time");
        assert!(!names_its_tables(sqlite, "SELECT * FROM {schema}.readings"));
        assert!(!names_its_tables(sqlite, "DROP TABLE IF EXISTS {tabel}"));
        assert!(!names_its_tables(sqlite, "{verb} INTO readings VALUES (1)"), "a verb named at run time");
    }

    /// A statement writes what it is handed where a placeholder or a spliced value stands among
    /// what it inserts or sets; what stands only in its `WHERE` is the key its rows are found by.
    #[test]
    fn a_statement_writes_what_it_is_handed_only_among_its_values() {
        let sqlite = db("db_sqlite");
        assert!(writes_handed(sqlite, "INSERT INTO visits (member, day) VALUES (?, ?)"));
        assert!(writes_handed(sqlite, "INSERT INTO readings VALUES ('{station}', {t})"), "values spliced in");
        assert!(writes_handed(sqlite, "UPDATE members SET name = :name WHERE id = :id"));
        assert!(writes_handed(sqlite, "INSERT INTO t (a) VALUES (1) ON CONFLICT (a) DO UPDATE SET b = ?"));
        assert!(writes_handed(sqlite, "WITH t AS (SELECT 1) INSERT INTO b VALUES ($1)"));
        assert!(!writes_handed(sqlite, "DELETE FROM visits WHERE member = ?"), "a key");
        assert!(!writes_handed(sqlite, "UPDATE visits SET seen = 1 WHERE member = '{member}'"), "a key spliced in");
        assert!(!writes_handed(sqlite, "INSERT INTO log SELECT count(*) FROM nodes"), "a copy of another table");
        assert!(writes_handed(sqlite, "INSERT INTO log SELECT * FROM visits WHERE day = ?"), "a key Atlas cannot tell from a value");
        assert!(writes_handed(sqlite, "sql"), "a text the grammar does not read");
    }

    /// A graph database's direction is what the Python code says to the driver, never the query.
    #[test]
    fn a_driver_says_which_way() {
        assert_eq!(dir(access(Db::Graph, "execute_read", None)), "read");
        assert_eq!(dir(access(Db::Graph, "execute_write", None)), "write");
        assert_eq!(dir(access(Db::Graph, "execute_query", Some("RoutingControl.READ"))), "read");
        assert_eq!(dir(access(Db::Graph, "session", Some("neo4j.WRITE_ACCESS"))), "write");
        assert_eq!(access(Db::Graph, "run", None), None, "a query alone says nothing");
        assert_eq!(
            facts(c("db_graph"), Some("neo4j"), "run", &lits(&["MATCH (n:Regeling) RETURN n"])),
            Facts::of(c("db_graph"), String::new())
        );
    }
}
