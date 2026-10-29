//! Paths as code builds them, in any language: literal text, the parameters of the function a
//! path is built in, the options of the command line, and what cannot be known (a hole, named).
//! A path built from a function's parameters is completed at each call that hands them, out
//! through the callers' own parameters; an option holds its default where the command line is
//! read, and like a parameter it is an input, so the code never decides a place it names alone.
//! The place of a path with an unknown part is the directory before that part, so
//! what one route unpacks into `D/{kind}/{id}` and another lists in `D` meet at `D/`. A
//! database statement is built and completed the same way, and read by its grammar where it is
//! whole: `count(con, sql)` reads the table its caller's `"SELECT count(*) FROM t"` names. A
//! statement whose verb and tables are written out is whole where it is built; the values it is
//! built from name no place.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::OnceLock;

use regex::Regex;

use crate::model::{Atom, Both};

/// A regex compiled once, at its first use.
pub fn re(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).unwrap())
}

// ---------- routes: the paths a request asks for and a route serves ----------

/// A placeholder in a route or a template: `{id}`, `${id}`.
pub fn placeholder_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    re(&RE, r"\$?\{[^}]*\}")
}

/// A route path as a pattern: `{id}` is one segment, `{rest:path}` any number.
pub fn route_pattern(path: &str) -> Option<Regex> {
    static PIECE: OnceLock<Regex> = OnceLock::new();
    let mut out = String::from("^");
    for piece in re(&PIECE, r"\{[^}]*\}|[^{]+").find_iter(path.trim_end_matches('/')) {
        let p = piece.as_str();
        out += &if !p.starts_with('{') {
            regex::escape(p)
        } else if p.ends_with(":path}") {
            ".+".into()
        } else {
            "[^/]+".into()
        };
    }
    Regex::new(&(out + "/?$")).ok()
}

/// Whether a route (`/api/customers/{id}`) serves a path (`/api/customers/x`).
pub fn route_matches(route: &str, path: &str) -> bool {
    let path = placeholder_re().replace_all(path.split(['?', '#']).next().unwrap_or(path), "x");
    route_pattern(route).map_or(false, |re| re.is_match(&path))
}

/// The path a request may ask of another part's route: one sent to the machine it runs on
/// (`http://localhost:8000/api/items/{id}`), or to a host the code does not say, a value the
/// URL starts with (`{BASE}/api/items/{id}`); without its query. A host the code writes out
/// that is no local one is another machine's, and a path of nothing but values names no route.
pub fn asked_path(url: &str) -> Option<String> {
    let rest = match url.find("://") {
        Some(i) => {
            let after = &url[i + 3..];
            let slash = after.find('/')?;
            let host = after[..slash].rsplit_once(':').map_or(&after[..slash], |(h, port)| {
                if port.chars().all(|c| c.is_ascii_digit()) {
                    h
                } else {
                    &after[..slash]
                }
            });
            if !crate::knowledge::LOCAL_HOSTS.contains(&host) && !host.contains('{') {
                return None;
            }
            &after[slash..]
        }
        None if url.starts_with('{') => &url[url.find('}')? + 1..],
        None => url,
    };
    let path = rest.split(['?', '#']).next()?;
    let named = path.split('/').any(|s| !s.is_empty() && !s.contains('{'));
    (path.starts_with('/') && named).then(|| path.to_string())
}

/// A request as built: its method, where the operation names one (`get`, `post`), else a hole,
/// and its URL.
pub fn request_of(op: &str, url: Vec<Built>) -> Vec<Built> {
    let method = if crate::knowledge::HTTP_METHODS.contains(&op) { lit(&op.to_uppercase()) } else { hole("method") };
    cross(&[method], &url, " ")
}

/// A request's method and path, as a rendered term writes them; the method is empty where the
/// code does not say it. The one place a rendered request is split.
pub fn method_and_path(request: &str) -> (&str, &str) {
    match request.split_once(' ') {
        Some((m, p)) if !m.starts_with('/') => (m, p),
        _ => ("", request),
    }
}

impl crate::model::Request {
    /// A request as a rendered term writes it (`method_and_path`): a method that is a value the
    /// code does not say (`{method}`) is none.
    pub fn of(rendered: &str) -> crate::model::Request {
        let (method, path) = method_and_path(rendered);
        let said = !method.is_empty() && !method.contains('{');
        crate::model::Request { method: said.then(|| crate::model::Method::new(method)), path: path.to_string() }
    }
}

impl From<String> for crate::model::Request {
    fn from(s: String) -> crate::model::Request {
        crate::model::Request::of(&s)
    }
}

/// A route of the program by the function that handles it, at a path the code names: what a
/// request may meet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Served {
    pub handler: String,
    pub route: crate::model::Route,
}

impl Served {
    pub fn path(&self) -> &str {
        self.route.path.as_deref().unwrap_or_default()
    }
}

/// The routes these nodes serve at a path the code names, each by its handler.
pub fn served<'n>(nodes: impl IntoIterator<Item = &'n crate::model::Node>) -> Vec<Served> {
    nodes
        .into_iter()
        .filter_map(|n| {
            let route = n.entry.as_ref().filter(|e| e.kind == crate::model::EntryKind::Route)?.request.clone()?;
            route.path.is_some().then(|| Served { handler: n.id.clone(), route })
        })
        .collect()
}

/// The routes of `routes` that serve a request: by its method when the code says it, and its
/// path; a literal segment serves before a parameter (`/years` before `/{year}`).
pub fn serving<'r>(routes: &'r [Served], request: &crate::model::Request) -> Vec<&'r Served> {
    let served: Vec<&Served> = routes
        .iter()
        .filter(|s| request.method.as_ref().map_or(true, |m| s.route.methods.contains(m)) && route_matches(s.path(), &request.path))
        .collect();
    let fewest = served.iter().map(|s| s.path().matches('{').count()).min().unwrap_or(0);
    served.into_iter().filter(|s| s.path().matches('{').count() == fewest).collect()
}

/// A part of a path as the code builds it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Seg {
    Lit(String),
    /// A parameter of the function the path is built in.
    Param(String),
    /// A parameter of another function, through the object it set up: `self.cache_dir`, set in
    /// `__init__` from its `cache_dir`, by the function's id and the parameter's name.
    Held(String, String),
    /// What cannot be known, by the name the code gives it; what the environment holds for a
    /// variable is named by `$` and the variable (`environment`).
    Hole(String),
    /// An input with a default, by its name: what the code takes where nobody says otherwise, and
    /// which whoever hands the input may override. An option of the command line, whatever parser
    /// reads it (clap's `default_value`, parseArgs' `default`, argparse's `default=`), a
    /// parameter's default (`path: Path = Path("out.csv")`, typer's `raw: Path = RAW`), and what
    /// the code falls back to where a caller or the environment says nothing
    /// (`os.environ.get("STORE", "out.csv")`, `process.env.STORE ?? "out.csv"`) are each one:
    /// never decided by the code alone.
    Option(String, Built),
    /// Where the package's own code lies, by its path in the repository: the file of the module an
    /// expression is written in (`__file__`, `import.meta.url`), or the directory a package ships
    /// its files in (`importlib.resources.files(..)`). A file read from there is part of the
    /// package, wherever it is installed.
    Package(String),
}

pub type Built = Vec<Seg>;

/// The values an input with a default holds: its default, each as reached through the input; no
/// value where it declares none.
pub fn option(name: &str, default: Vec<Built>) -> Vec<Built> {
    default.into_iter().map(|d| vec![Seg::Option(name.to_string(), d)]).collect()
}

/// What the environment holds for the variable `name`: set where the program runs, and not known
/// to the code.
pub fn environment(name: &str) -> Built {
    hole(&format!("${name}"))
}

/// What the environment holds for the variable a call names, where the code names it as text.
pub fn environment_named(names: &[Built]) -> Option<Built> {
    match names {
        [b] if known(b) => Some(environment(&render(b))),
        _ => None,
    }
}

/// A value, or what the code takes where the value says nothing: `os.environ.get("X") or "d"`,
/// `x ?? "d"`, `env::var("X").unwrap_or(..)`, `opts.config ?? "d"`. Where every value is one the
/// code does not decide (what a caller hands, an input with a default, what the environment says,
/// a value the code does not know), it is an input, and what the code takes is its default, which
/// whoever says the value may override; else the two are alternatives. An input nothing in the
/// program hands (what the environment says, a value the code does not know) is that input with
/// its default and nothing beside it, as an option of the command line is: one place, drawn as
/// its default, which a run reads or writes whatever the input says. What a caller hands is
/// followed to the caller, so it stays beside the default.
pub fn or_else(value: Vec<Built>, fallback: Vec<Built>) -> Vec<Built> {
    let input = |b: &Built| match b.as_slice() {
        [Seg::Param(x) | Seg::Held(_, x) | Seg::Option(x, _) | Seg::Hole(x)] => Some(x.clone()),
        _ => None,
    };
    let named: Option<Vec<String>> = value.iter().map(input).collect();
    let untraced = value.iter().all(|b| matches!(b.as_slice(), [Seg::Hole(_)]));
    match named.and_then(|n| n.into_iter().next()) {
        Some(name) if untraced && !fallback.is_empty() => option(&name, fallback),
        Some(name) => union(value, option(&name, fallback)),
        None => union(value, fallback),
    }
}

/// Whether a value is known as text throughout: literal, or an input's default that is.
pub fn known(b: &Built) -> bool {
    b.iter().all(|s| match s {
        Seg::Lit(_) | Seg::Package(_) => true,
        Seg::Option(_, d) => known(d),
        _ => false,
    })
}

/// The literal text a value ends in, inside the input it ends in, if any: where a file's name
/// and its extension are written.
pub fn last_lit(b: &mut Built) -> Option<&mut String> {
    match b.last_mut()? {
        Seg::Lit(s) | Seg::Package(s) => Some(s),
        Seg::Option(_, d) => last_lit(d),
        _ => None,
    }
}

/// The most values a path or a parameter keeps: the alternatives of a choice multiply through
/// each join, so a bound keeps a path built from several choices from growing without end.
pub const MAX: usize = 4;

/// Operations whose path is a directory: what they list, unpack into or make.
pub const DIR_OPS: &[&str] = &[
    "iterdir",
    "glob",
    "rglob",
    "listdir",
    "scandir",
    "walk",
    "extractall",
    "mkdir",
    "makedirs",
    "rmtree",
    "copytree",
    "readdir",
    "readdirSync",
    "opendir",
    "opendirSync",
    "mkdirSync",
    "cp",
    "cpSync",
    "create_dir",
    "create_dir_all",
    "remove_dir",
    "remove_dir_all",
    "read_dir",
];

pub fn lit(s: &str) -> Built {
    vec![Seg::Lit(s.to_string())]
}

pub fn hole(s: &str) -> Built {
    vec![Seg::Hole(s.to_string())]
}

/// `a` then `b`, with `sep` between them; adjacent literal text merges. The empty path is the unit
/// of a path's join, in every language (`Path("") / "x"`, `join("", "x")`, `PathBuf::from("").join("x")`
/// are `x`), so nothing stands between it and what is joined to it.
pub fn join(a: &Built, b: &Built, sep: &str) -> Built {
    let mut out = a.clone();
    let empty = sep == "/" && known(a) && render(a).is_empty();
    let rest: Vec<Seg> =
        if sep.is_empty() || empty { b.clone() } else { std::iter::once(Seg::Lit(sep.into())).chain(b.iter().cloned()).collect() };
    for s in rest {
        match (out.last_mut(), &s) {
            (Some(Seg::Lit(x)), Seg::Lit(y)) => x.push_str(y),
            _ => out.push(s),
        }
    }
    out
}

pub fn cross(xs: &[Built], ys: &[Built], sep: &str) -> Vec<Built> {
    let mut out = vec![];
    for x in xs {
        for y in ys {
            let b = join(x, y, sep);
            if !out.contains(&b) {
                out.push(b);
            }
        }
    }
    out.truncate(MAX);
    out
}

pub fn union(mut xs: Vec<Built>, ys: Vec<Built>) -> Vec<Built> {
    for y in ys {
        if !xs.contains(&y) {
            xs.push(y);
        }
    }
    xs.truncate(MAX);
    xs
}

/// The directory a path is in: the path without its last part. The parts of a path are what lies
/// between the `/`s its literal text writes, so a value written between two of them is one part:
/// the directory of `~/data/{name}` is `~/data`, and of `{root}/{name}.json` it is `{root}`. A path
/// with no `/` before its last part has nothing written to go back to: a name alone is in `.`, and
/// a path not said yet (what a caller hands, an option, what the environment holds) is in `{p}/..`,
/// which is the directory of what is said for it once it is (`written_in`).
pub fn parent(b: &Built) -> Built {
    let last = b.len().saturating_sub(1);
    for i in (0..b.len()).rev() {
        let (Seg::Lit(s) | Seg::Package(s)) = &b[i] else { continue };
        // a directory written with its closing `/` (`a/b/`) ends in its last part; a `/` before a
        // value written after it closes the part before that value
        let t = if i == last { s.trim_end_matches('/') } else { s.as_str() };
        // the directory above `{p}/..` is one more step up, once the path is said
        if i == last && (t == ".." || t.ends_with("/..")) {
            return [b.clone(), lit("/..")].concat();
        }
        if let Some(k) = t.rfind('/') {
            let mut out = b[..i].to_vec();
            // the root's own `/` stays, where nothing is written before it
            let up = t[..if i == 0 { k.max(1) } else { k }].to_string();
            if !up.is_empty() {
                out.push(if matches!(b[i], Seg::Package(_)) { Seg::Package(up) } else { Seg::Lit(up) });
            }
            return out;
        }
    }
    // text after the last part that is no literal is more of that part's name (`{to}.json`)
    let name = b.iter().rposition(|s| !matches!(s, Seg::Lit(_))).map_or(b.len(), |k| k + 1);
    match (b.as_slice(), &b[..name]) {
        ([Seg::Lit(_)], _) => lit("."),
        // a module at the top of the repository is in its root
        ([Seg::Package(_)], _) => vec![Seg::Package(".".into())],
        // a path a caller hands, an option or the environment names: its directory, once the path
        // is said (`{to}/..`)
        (_, [.., Seg::Param(_) | Seg::Held(..) | Seg::Option(..)]) => [b[..name].to_vec(), lit("/..")].concat(),
        (_, [.., Seg::Hole(h)]) if !anonymous(h) => [b[..name].to_vec(), lit("/..")].concat(),
        // the directory of a value the code gives no name is no more named
        _ => hole("parent"),
    }
}

/// Whether literal text starts with a step up from what is written before it (`/..`, `/../x`).
fn up_first(s: &str) -> bool {
    s == "/.." || s.starts_with("/../")
}

/// `b` with what `value` says each of its segments holds written in, where it says one, over every
/// combination. A step up written right after such a segment (`{source}/..`, the directory `parent`
/// gives of a path not said yet) is the directory of what is written in: going up from a path a
/// caller hands is going up from what the caller hands, so `{source}/..` handed `~/data/{name}`
/// is `~/data`.
fn written_in(b: &Built, mut value: impl FnMut(&Seg) -> Option<Vec<Built>>) -> Vec<Built> {
    let mut out: Vec<Built> = vec![vec![]];
    let mut said = false;
    for s in b {
        match (s, value(s)) {
            (_, Some(piece)) => {
                out = cross(&out, &piece, "");
                said = true;
            }
            (Seg::Lit(t), None) if said && up_first(t) => {
                let mut up = vec![];
                for o in &out {
                    let (mut o, mut rest) = (o.clone(), t.as_str());
                    while up_first(rest) {
                        o = parent(&o);
                        rest = &rest[3..];
                    }
                    let o = if rest.is_empty() { o } else { join(&o, &lit(rest), "") };
                    if !up.contains(&o) {
                        up.push(o);
                    }
                }
                out = up;
                said = false;
            }
            (s, None) => {
                out = cross(&out, &[vec![s.clone()]], "");
                said = false;
            }
        }
    }
    out
}

/// Marks, in a path's text, that the suffix of the name before it is taken away: what
/// `with_suffix` does to a name not said where it is written (a path a caller hands), done once
/// the name is said, as `/..` is for `parent`.
const UNSUFFIXED: char = '\u{1}';

/// A path with another suffix, written with its dot (`.html`, `.{}.part`), or with none (the
/// empty suffix takes it away): `page.json` is `page.html`. A name the path does not say yet (a
/// path a caller hands) gets its suffix once the caller says it: `{path}` with `.part`, handed
/// `look.json`, is `look.part`.
pub fn with_suffix(b: &Built, suffix: &Built) -> Built {
    let mut out = b.clone();
    let whole = b.len() == 1;
    match last_lit(&mut out) {
        Some(s) if !s.ends_with('/') && s.contains('/') || (whole && !s.is_empty()) => {
            let name_at = s.rfind('/').map_or(0, |i| i + 1);
            if let Some(dot) = s[name_at..].rfind('.').filter(|d| *d > 0) {
                s.truncate(name_at + dot);
            }
        }
        _ => out = join(&out, &lit(&UNSUFFIXED.to_string()), ""),
    }
    join(&out, suffix, "")
}

/// A path's text with each suffix `with_suffix` takes away from a name said later taken away.
fn unsuffixed(p: String) -> String {
    if !p.contains(UNSUFFIXED) {
        return p;
    }
    let mut out = String::new();
    for c in p.chars() {
        if c != UNSUFFIXED {
            out.push(c);
            continue;
        }
        let name_at = out.rfind('/').map_or(0, |i| i + 1);
        if let Some(dot) = out[name_at..].rfind('.').filter(|d| *d > 0) {
            out.truncate(name_at + dot);
        }
    }
    out
}

/// A path with its unknown parts written as `{name}`, and an option as its default.
pub fn render(b: &Built) -> String {
    unsuffixed(
        b.iter()
            .map(|s| match s {
                Seg::Lit(x) | Seg::Package(x) => x.clone(),
                Seg::Param(x) | Seg::Held(_, x) | Seg::Hole(x) => format!("{{{x}}}"),
                Seg::Option(_, d) => render(d),
            })
            .collect(),
    )
}

/// A path with what is given to the function written as `{name}`: what a caller hands, what the
/// environment says, and an input with a default.
fn inputs(b: &Built) -> String {
    b.iter()
        .map(|s| match s {
            Seg::Option(x, _) => format!("{{{x}}}"),
            s => render(&vec![s.clone()]),
        })
        .collect()
}

/// Whether what a call to a database is handed, as built, is a statement Atlas reads once it is
/// whole: the database's statements have a grammar, and the text starts with a verb or with
/// what a caller hands.
pub fn is_statement(db: crate::model::Db, built: &[Built]) -> bool {
    fn starts(b: &Built) -> bool {
        match b.first() {
            Some(Seg::Param(_) | Seg::Held(..)) => true,
            Some(Seg::Lit(l)) => crate::effects::starts_statement(l),
            Some(Seg::Option(_, d)) => starts(d),
            _ => false,
        }
    }
    crate::effects::reads_statements(db) && built.iter().any(starts)
}

/// The names Atlas gives a hole where the code gives it none: an expression it does not follow
/// (`value`, `call`, `format`, `method`, `parent`) or one beyond its depth (`…`). A call Atlas
/// does not follow is `call` in every language, never the name of what it calls: the callee names
/// an operation, not the value it gives back.
const ANONYMOUS: &[&str] = &["value", "call", "format", "method", "parent", "…"];

/// Whether a hole, by its name or as a place writes it (`call`, `{call}`), is one the code gives
/// no name.
fn anonymous(hole: &str) -> bool {
    let name = hole.trim_start_matches('$').trim_start_matches('{').trim_end_matches('}');
    ANONYMOUS.contains(&name.trim_start_matches('$'))
}

/// A hole as a place writes it: `{name}` where the code names it, `{}` where it does not.
fn named_hole(hole: &str) -> String {
    if anonymous(hole) {
        "{}".into()
    } else {
        hole.trim_start_matches('$').to_string()
    }
}

/// What a name the code binds holds, as the code names it. A value that no code Atlas follows
/// decides and that the code gives no name (what a library call gives back:
/// `let home = dirs::home_dir()`) is named by the binding, the first name the code gives it,
/// which every later name of it hands on; so one value is one hole however many names it passes
/// through. A path built around such a value keeps its hole unnamed: the binding names the
/// whole, not the part.
pub fn bound(name: &str, values: Vec<Built>) -> Vec<Built> {
    values
        .into_iter()
        .map(|b| match b.as_slice() {
            [Seg::Hole(h)] if anonymous(h) => hole(name),
            _ => b,
        })
        .collect()
}

/// The place a path names: the file, or for an operation on a directory that directory. A file
/// whose name is known under a directory that is not (`artefacts/{variant}/report.json`,
/// `{$DATA_DIR}/report.txt`) is that name under that directory, the hole kept in the term by the
/// name the code gives it; a hole the code gives no name (`{value}`) is `{}`, so it is never taken
/// for a wildcard the code writes, nor two named holes for one directory; a path whose last part is not known rests in
/// the directory before its first unknown part, so what is unpacked into `D/{kind}/{id}` and what
/// lists `D` meet at `D/`. A pattern the code writes (`D.glob("*/IO-*")`) is the fiber below its
/// directory, whatever operation it is handed to: the listing and the reads of what it yields are
/// one place. A path that names nothing (only `.`, only wildcards, or nothing before an unknown
/// last part) is no place.
pub fn place(b: &Built, op: &str) -> Option<String> {
    let mut p = render(b).replace('\\', "/");
    while p.contains("//") {
        p = p.replace("//", "/");
    }
    p = p.replace("/./", "/");
    while let Some(r) = p.strip_prefix("./") {
        p = r.to_string();
    }
    // `server/..` is where `server` is
    let mut kept: Vec<&str> = vec![];
    for x in p.split('/') {
        match (x, kept.last()) {
            ("..", Some(prev)) if !matches!(*prev, ".." | "") && !prev.contains(['{', '*']) => {
                kept.pop();
            }
            _ => kept.push(x),
        }
    }
    let p = if kept.is_empty() { ".".to_string() } else { kept.join("/") };
    let dir = DIR_OPS.contains(&op);
    // a part the code leaves unknown; a wildcard the code writes is part of the term a file
    // operation names, and an operation on a directory rests above it
    let wild = |s: &str| s.contains(['*', '?', '[']);
    let unknown = |s: &str| s.contains('{') || (dir && wild(s));
    let p = p.trim_end_matches('/').to_string();
    let mut parts: Vec<&str> = p.split('/').collect();
    // a pattern below a directory is the part of the directory it holds: the place keeps it; one that
    // holds everything below it (`iterdir()`, `glob("*")`, `rglob("*")`) is the fiber over a
    // predicate that always holds, the whole directory, which is what is listed
    let mut pattern = crate::knowledge::PATH_PATTERNS.iter().any(|(name, _, _)| *name == op);
    if pattern && parts.len() > 1 && parts.last() == Some(&"*") {
        parts.pop();
        if parts.len() > 1 && parts.last() == Some(&"**") {
            parts.pop();
        }
        pattern = false;
    }
    let p = parts.join("/");
    let any = || {
        parts
            .iter()
            .map(|x| placeholder_re().replace_all(x, |c: &regex::Captures| named_hole(&c[0])).into_owned())
            .collect::<Vec<_>>()
            .join("/")
    };
    let p = match parts.iter().position(|x| unknown(x)) {
        // the pattern's own wildcards stay; a part the code leaves unknown is one more
        _ if pattern && parts.iter().any(|x| unknown(x) || wild(x)) => any(),
        None if dir => format!("{p}/"),
        None => p.clone(),
        // a file whose name is known is that name under any directory the code leaves unknown
        Some(i) if !dir && i + 1 < parts.len() && !unknown(parts[parts.len() - 1]) => any(),
        Some(0) => return None,
        Some(i) => format!("{}/", parts[..i].join("/")),
    };
    let names = p.split('/').any(|x| !matches!(x, "" | "." | "*" | "**" | "{}"));
    names.then_some(p)
}

/// The places the program names a place below, which are therefore directories: what
/// `D.glob("*/IO-*")` lists and `io_dir / "Metadata.xml"` reads in is the directory `D/*/IO-*/`,
/// so the listing and the files in it are one folder, whatever the operation that names it.
pub fn folders<'a>(places: impl Iterator<Item = &'a str> + Clone) -> HashSet<String> {
    let below: HashSet<&str> = places.clone().flat_map(|a| a.trim_end_matches('/').match_indices('/').map(move |(i, _)| &a[..i])).collect();
    places.filter(|a| !a.ends_with('/') && below.contains(a)).map(str::to_string).collect()
}

/// The name of the one value `built` holds, where it is whole in `holders` (the functions of the
/// conditions that make it whole), when it is a parameter of one of them, by that function's
/// owner (`GraphBuilder::owner`): the parameter a constructor takes is its class's (`Store.path`),
/// a method's its class's and its own (`Repo.read.dir`), any other function's its own
/// (`main.out`). Nothing when the alternatives hold more than one value, or a value with a literal
/// part, or when it is a parameter of more than one of them.
fn by_value<'b>(store: &crate::builder::GraphBuilder, holders: &[&str], mut built: impl Iterator<Item = &'b Built>) -> Option<String> {
    let name = |b: &Built| match b.as_slice() {
        [Seg::Hole(x) | Seg::Param(x) | Seg::Held(_, x)] => Some(x.clone()),
        _ => None,
    };
    let first = name(built.next()?)?;
    if !built.all(|b| name(b).as_ref() == Some(&first)) {
        return None;
    }
    // a value of one of the holder's parameters; anything else unknown (what the environment
    // says) keeps the name the code gives it
    let takes = |h: &&&str| store.nodes.get(**h).is_some_and(|n| n.params.iter().any(|p| !p.receiver && p.name == first));
    let [holder] = holders.iter().filter(takes).collect::<Vec<_>>()[..] else { return None };
    Some(format!("{}.{first}", store.owner(holder)?))
}

/// The deepest a value is followed out through the callers.
const DEPTH: u32 = 12;
/// The most completions one built value is followed to, over all the calls that complete it.
const COMPLETIONS: usize = 64;

/// A call of one of the program's functions, as the paths pass reads it: who calls whom, and what
/// the call hands each parameter, built where the caller builds it.
pub trait Handing {
    fn caller(&self) -> &str;
    fn callee(&self) -> &str;
    /// What the call hands parameter `p`; none when it leaves `p` out.
    fn hands(&self, p: &str) -> Option<Vec<Built>>;
    /// The caller's parameter the call is made on, when it is one: `exporter` in
    /// `exporter.write(..)`.
    fn on(&self) -> Option<&str> {
        None
    }
    /// The class of what the call hands parameter `p`, where the code says it: `new CsvExporter()`.
    fn class_handed(&self, _p: &str) -> Option<String> {
        None
    }
}

/// Each parameter of `b` replaced by what `value` says it holds, over every combination.
pub fn substitute(b: &Built, mut value: impl FnMut(&str) -> Vec<Built>) -> Vec<Built> {
    written_in(b, |s| match s {
        Seg::Param(p) => Some(value(p)),
        _ => None,
    })
}

/// Where a value built from parameters becomes whole: a conjunction of conditions on a route
/// (`model::Atom`): in a function, from what its calls hand, or at an entry, from what the entry's
/// users hand it; and where it takes several, also the others (the caller that built an object
/// whose field holds its root, and the one that handed the rest).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Whole {
    /// Where what is left is handed: the part of a URL that picks the route that serves it, so the
    /// function a request is from.
    pub asks: Atom,
    /// The other conditions of the conjunction, never `asks`.
    pub with: BTreeSet<Atom>,
}

impl Whole {
    /// Whole in `f` alone.
    pub fn at(f: impl Into<String>) -> Whole {
        Whole { asks: Atom::Reaches(f.into()), with: BTreeSet::new() }
    }

    /// Whole by what the users of entry `e` hand it.
    pub fn users(e: impl Into<String>) -> Whole {
        Whole { asks: Atom::Starts(e.into()), with: BTreeSet::new() }
    }

    /// Whether whole in `f` alone.
    fn is_at(&self, f: &str) -> bool {
        self.with.is_empty() && self.asks == Atom::Reaches(f.to_string())
    }

    /// Whole where both are, `self` handing what is left: the union of the conditions, with the
    /// function a touch is in, `home`, as the unit, since every route that runs the touch reaches
    /// it (`mathematics.md`, "Who makes a path whole").
    fn and(self, other: &Whole, home: &str) -> Whole {
        if self.is_at(home) {
            return other.clone();
        }
        if other.is_at(home) {
            return self;
        }
        let Whole { asks, mut with } = self;
        with.extend(std::iter::once(&other.asks).chain(&other.with).cloned());
        with.remove(&asks);
        Whole { asks, with }
    }

    /// The conjunction a touch in `home` holds: every condition but reaching `home`; empty where
    /// the value is whole in `home` itself.
    fn both(&self, home: &str) -> Both {
        let home = Atom::Reaches(home.to_string());
        std::iter::once(&self.asks).chain(&self.with).filter(|a| **a != home).cloned().collect()
    }
}

/// What completes a value through the callers of the function it is built in, `home`: the
/// program's calls, its families, the defaults its parameters declare, which functions are
/// entries, and whether a value, as far as it is whole, decides where its call goes.
pub struct Callers<'c, H> {
    pub home: &'c str,
    pub calls: &'c [H],
    pub family: &'c crate::builder::Family,
    pub default: &'c dyn Fn(&str, &str) -> Vec<Built>,
    pub entry: &'c dyn Fn(&str) -> bool,
    pub decides: &'c dyn Fn(&Built) -> bool,
}

/// A completion of a value at one place of the callers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Completion {
    /// Where the value is whole.
    pub whole: Whole,
    /// What it is there.
    pub term: Built,
    /// The function whose own code decides it, if one does.
    pub decided_in: Option<String>,
    /// The function that set up the object whose field holds the value's root, if one does
    /// (`Workbench.at` for what `self.root` holds).
    pub set_up_in: Option<String>,
}

impl<H: Handing> Callers<'_, H> {
    /// What a value built from the parameters of `f` is at each call that hands them, followed out
    /// through the callers' parameters in turn, with where it is whole: the URL
    /// `request(method, path)` fetches is `/api/customers` where `api.get("/customers")` is
    /// called. A parameter a call leaves out holds its `default`, an input whoever calls `f` may
    /// override; so does one of a function nobody in the program calls (an entry's declared
    /// defaults), which otherwise holds what its users hand it, unknown. An entry the program also
    /// calls is run by its users as well. The function that decides the value is the first, out
    /// through the callers, in whose own code it decides: `default_rows` for `read_rows(path)`
    /// called as `read_rows(Path("data/rows.csv"))`.
    pub fn at(&self, f: &str, b: &Built, depth: u32) -> Vec<Completion> {
        self.of(f, b, None, None, depth)
    }

    /// `at`, keeping to the calls of `f` that hand its parameter `only.0` a value that is an
    /// `only.1`, or one the code does not say: the member of a family a call on that parameter
    /// runs. `decided` is the function that decides the value, where a callee's code already did.
    fn of(&self, f: &str, b: &Built, only: Option<(&str, &str)>, decided: Option<&str>, depth: u32) -> Vec<Completion> {
        // decided by a callee, else here where the value as `f` builds it decides
        let here = decided.map(str::to_string).or_else(|| (self.decides)(b).then(|| f.to_string()));
        // what an object holds is what the callers of the function that set it up hand that
        // function: whole where that is, unless `f`'s own callers still have to hand the rest
        if let Some((g, p)) = b.iter().find_map(|s| match s {
            Seg::Held(g, p) => Some((g.clone(), p.clone())),
            _ => None,
        }) {
            let held = if depth > DEPTH {
                vec![Completion { whole: Whole::at(f), term: hole(&p), decided_in: None, set_up_in: None }]
            } else {
                self.at(&g, &vec![Seg::Param(p.clone())], depth + 1)
            };
            let mut out = vec![];
            for object in held {
                for x in fill(b, &g, &p, &object.term) {
                    // a value that what the object holds decides is decided where the object was built
                    let by = here.clone().or_else(|| (self.decides)(&x).then_some(object.decided_in.clone()).flatten());
                    for rest in self.of(f, &x, None, by.as_deref(), depth + 1) {
                        // whole where both are: every condition of the one that built the object and
                        // of the one that handed `f` the rest
                        let both = rest.whole.and(&object.whole, self.home);
                        out.push(Completion { whole: both, set_up_in: rest.set_up_in.or_else(|| Some(g.clone())), ..rest });
                        if out.len() >= COMPLETIONS {
                            return out;
                        }
                    }
                }
            }
            return out;
        }
        if !b.iter().any(|s| matches!(s, Seg::Param(_))) {
            return vec![Completion { whole: Whole::at(f), term: b.clone(), decided_in: here, set_up_in: None }];
        }
        let (calls, family) = (self.calls, self.family);
        // a parameter a call leaves out holds its default, an input its callers may override
        let unknown = |p: &str| match (self.default)(f, p) {
            d if d.is_empty() => vec![hole(p)],
            d => option(p, d),
        };
        // a call of a method `f` overrides is a call of `f`, handing its parameters by place
        let none = vec![];
        let theirs = |named: Option<&Vec<(String, String)>>, p: &str| -> Option<String> {
            match named {
                None => Some(p.to_string()),
                Some(named) => named.iter().find(|(own, _)| own == p).map(|(_, theirs)| theirs.clone()),
            }
        };
        let sites: Vec<(&H, Option<&Vec<(String, String)>>)> = calls
            .iter()
            .filter(|a| a.callee() == f)
            .map(|a| (a, None))
            .chain(
                family
                    .overrides
                    .get(f)
                    .unwrap_or(&none)
                    .iter()
                    .flat_map(|(base, named)| calls.iter().filter(move |a| a.callee() == base).map(move |a| (a, Some(named)))),
            )
            .filter(|(a, named)| match only {
                Some((p, class)) => theirs(*named, p).and_then(|q| a.class_handed(&q)).map_or(true, |k| family.is_a(&k, class)),
                None => true,
            })
            .collect();
        if sites.is_empty() || depth > DEPTH {
            return substitute(b, unknown)
                .into_iter()
                .map(|term| Completion { whole: Whole::at(f), term, decided_in: here.clone(), set_up_in: None })
                .collect();
        }
        let mut out: Vec<Completion> = if (self.entry)(f) {
            substitute(b, unknown)
                .into_iter()
                .map(|term| Completion { whole: Whole::users(f), term, decided_in: here.clone(), set_up_in: None })
                .collect()
        } else {
            vec![]
        };
        for (a, named) in sites {
            let hands = |p: &str| theirs(named, p).and_then(|q| a.hands(&q));
            // through a base's method called on a parameter, the callers that hand that parameter
            // decide which member runs: the one `f` is a method of
            let class = f.rsplit_once('.').map_or("", |(c, _)| c);
            let next = named.and(a.on()).map(|q| (q, class));
            for x in substitute(b, |p| hands(p).filter(|v| !v.is_empty()).unwrap_or_else(|| unknown(p))) {
                out.extend(self.of(a.caller(), &x, next, here.as_deref(), depth + 1));
                if out.len() >= COMPLETIONS {
                    return out;
                }
            }
        }
        out
    }
}

/// `b` with what the object holds for parameter `p` of `g` written in.
fn fill(b: &Built, g: &str, p: &str, v: &Built) -> Vec<Built> {
    written_in(b, |s| match s {
        Seg::Held(h, q) if h == g && q == p => Some(vec![v.clone()]),
        _ => None,
    })
}

/// A file, network or database call whose path, URL or statement waits for what its callers
/// hand: the function that makes it, its touch, and the path, URL or statement as built.
pub type Waiting = (String, crate::model::Touch, Vec<Built>);

/// A waiting call with its path, URL or statement as built, and at each call that completes it,
/// with where it is whole, the function whose own code decides it and the function that set up
/// the object whose field holds its root, if one does.
pub type Completed = (String, crate::model::Touch, Vec<Built>, Vec<Completion>);

/// The way a request goes by the method it is built with (`knowledge::REQUEST_WAYS`), where every
/// alternative says the same; none where one does not say its method.
fn way_of_request<'b>(mut built: impl Iterator<Item = &'b Built>) -> Option<crate::model::Dir> {
    let way = |b: &Built| crate::knowledge::request_way(method_and_path(&render(b)).0);
    let first = way(built.next()?)?;
    built.all(|b| way(b) == Some(first)).then_some(first)
}

/// Whether the code decides where a call goes, once its path or URL is whole: it starts at
/// something the code holds, a literal or a constant, not at what a caller hands, an input with a
/// default, or what the environment says (a value, the user's home `~`, the system's `$TMPDIR`).
/// A request's URL is decided only with its host. A file read from where the package's code lies
/// is part of the package; one written there is a place the code decides.
fn decided(b: &Built, t: &crate::model::Touch) -> bool {
    use crate::model::{Dir, World};
    let rendered = inputs(b);
    let given = |s: &str| s.is_empty() || s.starts_with(['{', '$', '~']);
    match t.kind {
        World::Net => {
            let url = method_and_path(&rendered).1;
            url.contains("://") && !given(url)
        }
        World::Disk if matches!(b.first(), Some(Seg::Package(_))) => t.dir == Some(Dir::Write),
        World::Disk => !given(&rendered),
        _ => false,
    }
}

/// Whether a statement as built names its places whatever its callers hand: its grammar names
/// the verb and every table without them, so what they hand are values.
fn names_its_places(b: &Built, kind: crate::model::World) -> bool {
    match kind {
        crate::model::World::Db(db) => crate::effects::reads_statements(db) && crate::effects::names_its_tables(db, &render(b)),
        _ => false,
    }
}

/// Complete the waiting calls through their callers; `entry` says which functions are entries.
/// A statement that names its places is whole where it is built. Where the code decides a path
/// or URL, the function that decides it is the one whose own code the value starts in, wherever
/// the call that touches the place is.
pub fn complete<H: Handing>(
    calls: Vec<Waiting>,
    handing: &[H],
    family: &crate::builder::Family,
    default: &dyn Fn(&str, &str) -> Vec<Built>,
    entry: &dyn Fn(&str) -> bool,
) -> Vec<Completed> {
    calls
        .into_iter()
        .map(|(f, t, built)| {
            let decides = |b: &Built| decided(b, &t);
            let callers = Callers { home: &f, calls: handing, family, default, entry, decides: &decides };
            let found = built
                .iter()
                .flat_map(|b| match names_its_places(b, t.kind) {
                    true => vec![Completion {
                        whole: Whole::at(f.clone()),
                        term: b.clone(),
                        decided_in: decides(b).then(|| f.clone()),
                        set_up_in: None,
                    }],
                    false => callers.at(&f, b, 0),
                })
                .collect();
            (f, t, built, found)
        })
        .collect()
}

/// Place the completed calls: a file is touched by the function that opens it, at every place
/// it is opened at; where the language's requests are a page's (`origin`), a URL without a host
/// is the page's own, and one that one of the program's routes serves is a request from the
/// function that completes it to the route's handler (elsewhere it goes where the client
/// connects, which the code says apart from the request); a statement is read by its
/// grammar, and the function that runs it touches each table it names, the way it goes there.
/// Each touch keeps where it became whole, so a route shows the places its own calls hand; where
/// a completion names no place, the call keeps the touch it was read with.
pub fn place_calls(store: &mut crate::builder::GraphBuilder, completed: Vec<Completed>, origin: bool) {
    use crate::model::{Crossing, EdgeKind, Request, Touch, World};
    let routes = served(store.nodes.values());
    // the class each of the program's methods is of: the type whose code it is; a library's types
    // keep no places of the program
    let class_of: HashMap<String, String> = store
        .nodes
        .values()
        .filter(|n| n.kind == crate::model::NodeKind::Method && !n.library)
        .filter_map(|n| Some((n.id.clone(), n.cls.clone()?)))
        .collect();
    for (f, t, built, found) in completed {
        // the type whose code gives the path its root: one whose value holds it (`self.root`), or
        // whose constructor takes it, as a constructor's parameter is its class's (`Store.path`)
        let held = built.iter().flatten().find_map(|s| match s {
            Seg::Held(g, _) => class_of.get(g).cloned(),
            _ => None,
        });
        // else the one whose constructor takes the path, as a constructor's parameter is its class's
        // (`Store.path`), or one of whose methods touches it: the type whose code goes to the disk there
        let own = store.nodes.get(&f).filter(|n| n.ctor && !n.library).and_then(|n| n.cls.clone()).or_else(|| class_of.get(&f).cloned());
        // what each place of completion makes of it: the alternatives one caller hands are one
        let mut wholes: Vec<&Whole> = vec![];
        for c in &found {
            if !wholes.contains(&&c.whole) {
                wholes.push(&c.whole);
            }
        }
        // a route that reaches none of the callers that make it whole sees the call as it was read,
        // a file named by the value its function holds as it is where nothing calls the function
        if wholes.iter().any(|w| !w.both(&f).0.is_empty()) {
            let what = if t.kind == World::Disk { by_value(store, &[f.as_str()], built.iter()) } else { None };
            let dir = if t.kind == World::Net { way_of_request(built.iter()).or(t.dir) } else { t.dir };
            store.instead(
                &f,
                Touch {
                    what: what.unwrap_or_else(|| t.what.clone()),
                    dir,
                    holder: held.clone().or_else(|| own.clone()).unwrap_or_default(),
                    ..t.clone()
                },
            );
        }
        for whole in wholes {
            let both = whole.both(&f);
            let by: BTreeSet<Both> = if both.0.is_empty() { BTreeSet::new() } else { BTreeSet::from([both.clone()]) };
            // the function a request is from, and the functions the value is whole in: its own where
            // no other makes it whole
            let asker = whole.asks.name();
            let holders: Vec<&str> = if both.0.is_empty() { vec![f.as_str()] } else { both.names().collect() };
            // the way a request goes, where the method its alternatives say gives one
            let alternatives = found.iter().filter(|c| c.whole == *whole).map(|c| &c.term);
            // or one whose value a caller's object holds, set up where the completion went through
            let held = held
                .clone()
                .or_else(|| found.iter().filter(|c| c.whole == *whole).find_map(|c| class_of.get(c.set_up_in.as_ref()?).cloned()))
                .or_else(|| own.clone());
            let way = if t.kind == World::Net { way_of_request(alternatives).or(t.dir) } else { t.dir };
            // each place with the functions whose own code decides it
            let mut placed: Vec<(String, Option<crate::model::Dir>, Vec<String>)> = vec![];
            let (mut requested, mut decides) = (false, vec![]);
            // the alternatives that name no place: what a caller or the environment says
            let mut unnamed: Vec<&Built> = vec![];
            // the file a connection opens
            let mut opened = String::new();
            // a file the term names with a part no code decides, by the term the code writes
            let mut terms: Vec<(String, String)> = vec![];
            for (b, decider) in found.iter().filter(|c| c.whole == *whole).map(|c| (&c.term, &c.decided_in)) {
                let rendered = render(b);
                let before = placed.len();
                let fixed: Vec<String> = decider.iter().cloned().collect();
                decides.extend(fixed.iter().cloned());
                if t.kind == World::Net {
                    // a request's method, when the code says it, and its URL
                    let request = Request::of(&rendered);
                    let url = request.path.clone();
                    let mut served = false;
                    if url.starts_with('/') && origin {
                        for s in serving(&routes, &request) {
                            store.add_edge(asker, &s.handler, EdgeKind::Requests, Some(s.path().to_string()));
                            served = true;
                        }
                    } else if let Some(host) = crate::effects::facts(Crossing::to(World::Net), None, &t.op, std::slice::from_ref(&url))
                        .places()
                        .into_iter()
                        .next()
                        .map(|(a, _)| a)
                        .filter(|a| !a.is_empty())
                    {
                        placed.push((host, way, fixed));
                    }
                    requested |= served;
                    if !served && placed.len() == before {
                        unnamed.push(b);
                    }
                    // a request no route of the part serves may be another part's
                    if let Some(path) = asked_path(&url).filter(|_| !served) {
                        store.ask(asker, Request { path, ..request });
                    }
                } else if let World::Db(db) = t.kind {
                    if crate::knowledge::opens_a_file(db, &t.op) {
                        opened = place(b, "").filter(|f| crate::knowledge::names_a_file(f)).unwrap_or_default();
                        continue;
                    }
                    placed.extend(
                        crate::effects::sql(db, std::slice::from_ref(&rendered))
                            .unwrap_or_default()
                            .into_iter()
                            .map(|(table, dir)| (table, dir.or(t.dir), vec![])),
                    );
                } else if let Some(p) = place(b, &t.op) {
                    if !known(b) && !terms.iter().any(|(q, _)| *q == p) {
                        terms.push((p.clone(), render(b).replace('\\', "/")));
                    }
                    placed.push((p, t.dir, fixed));
                } else {
                    unnamed.push(b);
                }
            }
            // a place two alternatives name is decided where either decides it
            placed.sort();
            placed.dedup_by(|b, a| {
                let same = a.0 == b.0 && a.1 == b.1;
                if same {
                    a.2.append(&mut b.2);
                }
                same
            });
            // the touch is the function's own decision where its own code decides it, and its
            // callers' where theirs does
            let at = |artifact: String, dir, mut deciders: Vec<String>| {
                deciders.sort();
                deciders.dedup();
                let fixed = deciders.contains(&f);
                // else one of whose methods writes the root itself (`Tmp::new`)
                let holder = held.clone().or_else(|| deciders.iter().find_map(|d| class_of.get(d).cloned())).unwrap_or_default();
                deciders.retain(|d| *d != f);
                Touch { artifact, dir, fixed, fixed_by: deciders, by: by.clone(), holder, ..t.clone() }
            };
            // a file whose path is one value no literal makes is named by that value where it is
            // whole, so every call handed it touches the same place: `path` in a helper and
            // `self.path` in a method are both `Store.path` when the constructor takes it; a path
            // no place is named by is the term it evaluates to, its unknown parts left as the names
            // the code gives them, and what the environment says as its variable
            let named = |built: &[&Built]| {
                let value = if t.kind == World::Disk { by_value(store, &holders, built.iter().copied()) } else { None };
                value.or_else(|| {
                    let mut terms: Vec<String> = vec![];
                    for b in built {
                        let term = render(b).replace('\\', "/");
                        let said = b.iter().any(|s| !matches!(s, Seg::Hole(h) if !h.starts_with('$')));
                        if t.kind == World::Disk && said && !terms.contains(&term) {
                            terms.push(term);
                        }
                    }
                    (!terms.is_empty()).then(|| terms.join("|"))
                })
            };
            let all: Vec<&Built> = found.iter().filter(|c| c.whole == *whole).map(|c| &c.term).collect();
            let (value, unnamed_value) = (named(&all), named(&unnamed));
            if placed.is_empty() {
                if !requested {
                    // a URL without a host is the page's own origin, decided by whoever serves it
                    let fixed = if t.kind != World::Net || !t.artifact.is_empty() { decides } else { vec![] };
                    let touch = at(t.artifact.clone(), way, fixed);
                    store.touch(&f, Touch { what: value.unwrap_or_else(|| touch.what.clone()), file: opened, ..touch });
                }
                continue;
            }
            for (artifact, dir, fixed) in placed {
                let what = terms.iter().find(|(p, _)| *p == artifact).map_or_else(|| t.what.clone(), |(_, term)| term.clone());
                store.touch(&f, Touch { what, ..at(artifact, dir, fixed) });
            }
            // an alternative that names no place stays one the code does not name, by what names
            // it: what a caller hands that no code says
            if !unnamed.is_empty() {
                let touch = at(String::new(), way, vec![]);
                store.touch(&f, Touch { what: unnamed_value.unwrap_or_else(|| touch.what.clone()), ..touch });
            }
        }
    }
    let disk: Vec<&str> = store
        .nodes
        .values()
        .flat_map(|n| n.touches.iter().chain(&n.instead))
        .filter(|t| t.kind == World::Disk)
        .map(|t| t.artifact.as_str())
        .collect();
    let dirs = folders(disk.into_iter());
    for t in store.nodes.values_mut().flat_map(|n| n.touches.iter_mut().chain(n.instead.iter_mut())) {
        if t.kind == World::Disk && dirs.contains(&t.artifact) {
            t.artifact.push('/');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_asks_for_a_route() {
        assert!(route_matches("/api/graphs/graph/rond/{uuid:path}", "/api/graphs/graph/rond/a/b"));
        let item = |url: &str| asked_path(url).map_or(false, |p| route_matches("/api/items/{id}", &p));
        assert!(item("http://localhost:8000/api/items/{id}?full=1"));
        assert!(item("{$BASE}/api/items/{id}"), "a value the URL starts with is the host");
        assert!(!item("https://example.org/api/items/{id}"), "another machine serves no part of the repository");
        assert!(item("http://127.0.0.1/api/items/{id}"));
        assert!(!item("/api/items"));
        assert_eq!(asked_path("/"), None, "the root names no route");
        assert_eq!(asked_path("{url}"), None, "a URL not known at all asks for no path");
        assert_eq!(asked_path("{endpoint}/{id}"), None, "a path of nothing but values names no route");
        assert_eq!(asked_path("data/raw.csv"), None, "a relative path is a file");
        assert_eq!(method_and_path("POST /v1/orders"), ("POST", "/v1/orders"));
        assert_eq!(method_and_path("/v1/orders"), ("", "/v1/orders"));
    }

    /// A call as a test writes it: its caller, its callee, and what it hands each parameter.
    struct Call(&'static str, &'static str, Vec<(&'static str, Built)>);

    impl Handing for Call {
        fn caller(&self) -> &str {
            self.0
        }
        fn callee(&self) -> &str {
            self.1
        }
        fn hands(&self, p: &str) -> Option<Vec<Built>> {
            self.2.iter().find(|(q, _)| *q == p).map(|(_, b)| vec![b.clone()])
        }
    }

    /// The completions of `read(path)`'s path through `calls`, with `default` for what a call
    /// leaves out.
    fn completed(calls: &[Call], default: &dyn Fn(&str, &str) -> Vec<Built>, dir: crate::model::Dir) -> Vec<Completion> {
        let t = crate::model::Touch::new(crate::model::World::Disk, Some(dir));
        let decides = |b: &Built| decided(b, &t);
        let family = crate::builder::Family::default();
        let callers = Callers { home: "read", calls, family: &family, default, entry: &|_| false, decides: &decides };
        callers.at("read", &vec![Seg::Param("path".into())], 0)
    }

    /// The function that decides a path is the one whose own code writes what the path starts at,
    /// however many calls lie between it and the call that opens the file; a default a call
    /// leaves out decides nothing.
    #[test]
    fn the_function_that_writes_the_path_decides_it() {
        use crate::model::Dir;
        let none = |_: &str, _: &str| vec![];
        let handed = [Call("rows", "read", vec![("path", lit("data/rows.csv"))]), Call("main", "rows", vec![])];
        assert_eq!(
            completed(&handed, &none, Dir::Read),
            [Completion { whole: Whole::at("rows"), term: lit("data/rows.csv"), decided_in: Some("rows".into()), set_up_in: None }]
        );
        let through = [
            Call("rows", "read", vec![("path", vec![Seg::Param("dir".into()), Seg::Lit("/rows.csv".into())])]),
            Call("main", "rows", vec![("dir", lit("data"))]),
        ];
        assert_eq!(
            completed(&through, &none, Dir::Read),
            [Completion { whole: Whole::at("main"), term: lit("data/rows.csv"), decided_in: Some("main".into()), set_up_in: None }]
        );
        let left_out = [Call("main", "read", vec![])];
        let default = |_: &str, p: &str| if p == "path" { vec![lit("out.csv")] } else { vec![] };
        assert_eq!(
            completed(&left_out, &default, Dir::Read),
            [Completion {
                whole: Whole::at("main"),
                term: vec![Seg::Option("path".into(), lit("out.csv"))],
                decided_in: None,
                set_up_in: None,
            }],
            "the parameter's default is an option of whoever calls it"
        );
    }

    /// What the code falls back to where an input says nothing is that input's default: an
    /// option, never decided by the code alone. A file found from where the package's code lies
    /// is the package's when it is read, and a place the code decides when it is written.
    #[test]
    fn a_fallback_of_an_input_is_its_default() {
        use crate::model::{Dir, Touch, World};
        let fallback = or_else(vec![environment("STORE")], vec![lit("out.csv")]);
        assert_eq!(fallback, [vec![Seg::Option("$STORE".into(), lit("out.csv"))]], "what the environment says with a default is one input");
        let handed = or_else(vec![vec![Seg::Param("path".into())]], vec![lit("out.csv")]);
        assert_eq!(
            handed,
            [vec![Seg::Param("path".into())], vec![Seg::Option("path".into(), lit("out.csv"))]],
            "what a caller hands is followed"
        );
        assert_eq!(or_else(vec![lit("a.csv")], vec![lit("b.csv")]), [lit("a.csv"), lit("b.csv")], "two literals are alternatives");
        let (read, write) = (Touch::new(World::Disk, Some(Dir::Read)), Touch::new(World::Disk, Some(Dir::Write)));
        assert!(fallback.iter().all(|b| !decided(b, &write)), "{fallback:?}");
        let shipped = vec![Seg::Package("src/pages".into()), Seg::Lit("/template.html".into())];
        assert!(!decided(&shipped, &read) && decided(&shipped, &write));
        assert_eq!(place(&shipped, "read_text").as_deref(), Some("src/pages/template.html"));
    }

    /// A part no code decides stays a hole in the term, by the name the code gives it: a file whose
    /// name is known is that name under that directory, wherever the unknown part stands; a hole the
    /// code gives no name is `{}`, never a wildcard; a pattern is one place, the fiber below its
    /// directory, for the listing and for what is read of what it yields.
    #[test]
    fn a_hole_stays_in_the_term() {
        let env = |rest: &str| [environment("DATA_DIR"), lit(rest)].concat();
        assert_eq!(place(&env("/report.txt"), "read_text").as_deref(), Some("{$DATA_DIR}/report.txt"));
        assert_eq!(place(&env("/sub/report.txt"), "open").as_deref(), Some("{$DATA_DIR}/sub/report.txt"));
        assert_eq!(
            place(&[lit("artefacts/"), hole("variant"), lit("/report.json")].concat(), "open").as_deref(),
            Some("artefacts/{variant}/report.json")
        );
        assert_ne!(
            place(&[hole("base"), lit("/report.txt")].concat(), "read"),
            place(&[hole("top"), lit("/report.txt")].concat(), "read"),
            "two holes the code names are two directories"
        );
        assert_eq!(place(&[hole("value"), lit("/report.txt")].concat(), "read").as_deref(), Some("{}/report.txt"));
        assert_ne!(place(&[hole("value"), lit("/x/*.xml")].concat(), "glob"), place(&lit("*/x/*.xml"), "glob"), "a hole is no wildcard");
        assert_eq!(place(&environment("DATA_DIR"), "read_text"), None, "nothing known is no place");
        assert_eq!(place(&[environment("DATA_DIR"), lit("/"), hole("name")].concat(), "read_text"), None);
        assert_eq!(place(&[lit("data/"), hole("name")].concat(), "read_text").as_deref(), Some("data/"));
        let pattern = lit("assets/regelingen/*/IO-*");
        assert_eq!(place(&pattern, "glob").as_deref(), Some("assets/regelingen/*/IO-*"));
        assert_eq!(place(&pattern, "read_text").as_deref(), Some("assets/regelingen/*/IO-*"), "what the glob yields");
        assert_eq!(place(&lit("build/*"), "rmtree").as_deref(), Some("build/"));
        assert_eq!(place(&lit("*"), "read_text"), None);
        let unset = or_else(vec![environment("DATA_DIR")], vec![lit("")]);
        let joined: Vec<Option<String>> = cross(&unset, &[lit("sub/report.txt")], "/").iter().map(|b| place(b, "read")).collect();
        assert_eq!(joined, [Some("sub/report.txt".into())], "the empty path joins as nothing, and the variable is its input");
    }

    /// A place the program names a place below is a directory, whatever operation names it: the
    /// listing of `D/*/IO-*` and the files read in what it yields are one folder.
    #[test]
    fn a_place_with_places_below_is_a_folder() {
        let places = ["D/*/IO-*", "D/*/IO-*/Metadata.xml", "D/x.csv", "D/*/IO-*/"];
        assert_eq!(folders(places.into_iter()), HashSet::from(["D/*/IO-*".to_string()]));
    }

    /// A hole keeps the name the code gives the value: a value the code binds and no code Atlas
    /// follows decides is named by the binding, and a later name of it hands that name on, so one
    /// value is one hole; a path built around such a value is no name of it, and a hole the code
    /// names keeps its name.
    #[test]
    fn a_binding_names_the_value_it_holds() {
        let given = vec![hole("call")];
        assert_eq!(bound("folder", given.clone()), [hole("folder")]);
        assert_eq!(bound("path", bound("folder", given.clone())), bound("folder", given), "a later name hands the first on");
        let built = vec![[hole("call"), lit("/note.json")].concat()];
        assert_eq!(bound("path", built.clone()), built, "the binding names the whole, not the part");
        assert_eq!(bound("x", vec![environment("HOME")]), [environment("HOME")]);
        assert_eq!(bound("x", vec![lit("data")]), [lit("data")]);
    }

    /// A suffix given to a path a caller hands is the suffix given to what the caller hands:
    /// `path.with_extension("part")` with `path = a/look.json` is `a/look.part`, the same as written
    /// out, and so with a suffix the code builds (`format!("{}.part", id)`). A name the path says
    /// gets its suffix where it is written.
    #[test]
    fn a_suffix_of_a_handed_path_is_the_suffix_of_what_is_handed() {
        let path = vec![Seg::Param("path".into())];
        for suffix in [lit(".part"), [lit("."), hole("value"), lit(".part")].concat(), lit("")] {
            let handed = substitute(&with_suffix(&path, &suffix), |_| vec![lit("a/look.json")]);
            let written = with_suffix(&lit("a/look.json"), &suffix);
            assert_eq!(handed.iter().map(render).collect::<Vec<_>>(), [render(&written)], "{suffix:?}");
        }
        assert_eq!(render(&with_suffix(&lit("a/look.json"), &lit(".part"))), "a/look.part");
        assert_eq!(render(&with_suffix(&lit("a/.env"), &lit(".bak"))), "a/.env.bak", "a dot file has no suffix");
        let up = parent(&with_suffix(&path, &lit(".part")));
        assert_eq!(place(&substitute(&up, |_| vec![lit("a/look.json")])[0], "read").as_deref(), Some("a"));
    }

    /// Going up twice from a path a caller hands is going up twice from what the caller hands:
    /// `x.parent.parent / "m.json"` with `x = a/b/c.xml` is `a/m.json`, the same as written out.
    #[test]
    fn a_parent_of_a_parent_is_two_steps_up() {
        let x = vec![Seg::Param("x".into())];
        let up = join(&parent(&parent(&x)), &lit("m.json"), "/");
        let handed = substitute(&up, |_| vec![lit("a/b/c.xml")]);
        let written = join(&parent(&parent(&lit("a/b/c.xml"))), &lit("m.json"), "/");
        assert_eq!(handed.iter().map(|b| place(b, "read")).collect::<Vec<_>>(), [place(&written, "read")]);
        assert_eq!(place(&written, "read").as_deref(), Some("a/m.json"));
    }

    /// One part of a path: a name written out, a value a caller hands, what the environment holds,
    /// or a handed value with a suffix written after it (`{to}.json`).
    fn a_part() -> impl proptest::strategy::Strategy<Value = Built> {
        use proptest::prelude::*;
        prop_oneof![
            "[a-z]{1,4}(\\.[a-z]{1,3})?".prop_map(|s| lit(&s)),
            "[a-z]{1,4}".prop_map(|s| vec![Seg::Param(s)]),
            "[A-Z]{1,4}".prop_map(|s| environment(&s)),
            "[a-z]{1,4}".prop_map(|s| vec![Seg::Param(s), Seg::Lit(".json".into())]),
        ]
    }

    /// A path of one or more parts, from the user's home or relative.
    fn a_path() -> impl proptest::strategy::Strategy<Value = Built> {
        use proptest::prelude::*;
        (any::<bool>(), proptest::collection::vec(a_part(), 1..5)).prop_map(|(home, parts)| {
            let first = if home { lit("~") } else { parts[0].clone() };
            parts.iter().skip(usize::from(!home)).fold(first, |b, p| join(&b, p, "/"))
        })
    }

    proptest::proptest! {
        /// Law: the directory of a path drops exactly its last part, whatever each part is: the
        /// directory of `t/q` is `t`, so a path of n parts has a directory of n - 1, and joining the
        /// last part back gives the path.
        #[test]
        fn a_parent_drops_exactly_the_last_part(t in a_path(), q in a_part()) {
            proptest::prop_assert_eq!(parent(&join(&t, &q, "/")), t);
        }

        /// Law: going up from a path a caller hands is going up from what it hands. The directory
        /// of `{P}/a/b`, with `v` written in for `{P}`, is the directory of `v/a/b`, and the
        /// directory of `{P}` alone, `{P}/..`, is the directory of `v` once `v` is written in.
        #[test]
        fn a_parent_commutes_with_what_is_handed(rest in proptest::collection::vec(a_part(), 0..3), v in a_path()) {
            let p = vec![Seg::Param("P".into())];
            let t = rest.iter().fold(p, |b, q| join(&b, q, "/"));
            let hand = |x: &str| if x == "P" { vec![v.clone()] } else { vec![vec![Seg::Param(x.into())]] };
            let handed = substitute(&t, hand);
            proptest::prop_assert_eq!(handed.len(), 1);
            proptest::prop_assert_eq!(substitute(&parent(&t), hand), vec![parent(&handed[0])]);
        }

        /// Law: where two callers meet a path, the meet keeps every condition of both, the
        /// function the touch is in (`h`) being its unit: the conjunction a touch holds is the
        /// union of theirs, whichever side is the unit, a caller or an entry's users, and the
        /// one that hands what is left asks. So the meet is commutative, associative and
        /// idempotent on what a route must do for the touch to run.
        #[test]
        fn the_meet_keeps_every_condition(a in a_whole(), b in a_whole(), c in a_whole()) {
            let h = "h";
            let cond = |w: &Whole| w.both(h).0;
            let union = |x: &Whole, y: &Whole| cond(x).union(&cond(y)).cloned().collect::<BTreeSet<_>>();
            proptest::prop_assert_eq!(cond(&a.clone().and(&b, h)), union(&a, &b));
            proptest::prop_assert_eq!(cond(&a.clone().and(&b, h).and(&c, h)), cond(&a.clone().and(&b.clone().and(&c, h), h)));
            proptest::prop_assert_eq!(cond(&a.clone().and(&a, h)), cond(&a));
            proptest::prop_assert_eq!(a.clone().and(&Whole::at(h), h), a.clone());
            proptest::prop_assert_eq!(Whole::at(h).and(&a, h), a.clone());
            if !a.is_at(h) && !b.is_at(h) {
                proptest::prop_assert_eq!(a.clone().and(&b, h).asks, a.asks.clone());
            }
        }
    }

    /// Where a value becomes whole: the function a touch is in (`h`), callers, and entries whose
    /// users hand it, each alone or with others.
    fn a_whole() -> impl proptest::strategy::Strategy<Value = Whole> {
        use proptest::prelude::*;
        let atom = (any::<bool>(), "[a-c]|h").prop_map(|(users, f)| if users { Atom::Starts(f) } else { Atom::Reaches(f) });
        (atom.clone(), proptest::collection::btree_set(atom, 0..3)).prop_map(|(asks, mut with)| {
            with.remove(&asks);
            Whole { asks, with }
        })
    }

    /// The directory of a path of several parts is the path without its last, also where the last
    /// part is a value the code does not say (`~/data/{name}` is in `~/data`); a path that is one
    /// value not said yet is in the directory above it, named once the value is said (`{source}/..`,
    /// `{$SHELF_DATA}/..`).
    #[test]
    fn a_parent_of_a_path_of_several_parts() {
        let name = join(&lit("~/data"), &vec![Seg::Param("name".into())], "/");
        assert_eq!(render(&parent(&name)), "~/data");
        assert_eq!(render(&parent(&vec![Seg::Param("source".into())])), "{source}/..");
        assert_eq!(render(&parent(&environment("SHELF_DATA"))), "{$SHELF_DATA}/..");
        let log = join(&parent(&vec![Seg::Param("source".into())]), &lit("shelf.log"), "/");
        let handed = substitute(&log, |_| vec![name.clone()]);
        assert_eq!(handed.iter().map(render).collect::<Vec<_>>(), ["~/data/shelf.log"]);
    }

    /// A pattern that holds everything below a directory is the fiber over a predicate that always
    /// holds: the directory itself. `D.iterdir()`, `D.glob("*")` and `D.rglob("*")` list the place a
    /// listing handed `D` lists, also below a part the code leaves unknown; a pattern that holds a
    /// part of it stays that part.
    #[test]
    fn a_pattern_that_holds_everything_is_the_directory() {
        for d in [lit("data/raw"), join(&lit("data"), &hole("kind"), "/")] {
            let listed = place(&d, "listdir");
            assert!(listed.is_some());
            for (op, pattern) in [("iterdir", "*"), ("glob", "*"), ("rglob", "**/*")] {
                assert_eq!(place(&join(&d, &lit(pattern), "/"), op), listed, "{op}({pattern}) of {d:?}");
            }
        }
        assert_eq!(place(&lit("data/raw/*.csv"), "glob").as_deref(), Some("data/raw/*.csv"));
    }
}
