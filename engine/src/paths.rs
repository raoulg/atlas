//! Paths as code builds them, in any language: literal text, the parameters of the function a
//! path is built in, and what cannot be known (a hole, named). A path built from a function's
//! parameters is completed at each call that hands them, out through the callers' own
//! parameters; the place of a path with an unknown part is the directory before that part, so
//! what one route unpacks into `D/{kind}/{id}` and another lists in `D` meet at `D/`. A
//! database statement is built and completed the same way, and read by its grammar where it is
//! whole: `count(con, sql)` reads the table its caller's `"SELECT count(*) FROM t"` names.

use std::sync::OnceLock;

use regex::Regex;

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
        out += &if !p.starts_with('{') { regex::escape(p) } else if p.ends_with(":path}") { ".+".into() } else { "[^/]+".into() };
    }
    Regex::new(&(out + "/?$")).ok()
}

/// Whether a route (`/api/customers/{id}`) serves a path (`/api/customers/x`).
pub fn route_matches(route: &str, path: &str) -> bool {
    let path = placeholder_re().replace_all(path.split(['?', '#']).next().unwrap_or(path), "x");
    route_pattern(route).map_or(false, |re| re.is_match(&path))
}

/// The path a request asks for wherever it is sent: its URL without the host, written out or a
/// value the URL starts with (`{BASE}/api/items/{id}`), and without its query.
pub fn asked_path(url: &str) -> Option<String> {
    let rest = match url.find("://") {
        Some(i) => &url[i + 3..][url[i + 3..].find('/')?..],
        None if url.starts_with('{') => &url[url.find('}')? + 1..],
        None => url,
    };
    let path = rest.split(['?', '#']).next()?;
    (path.starts_with('/') && path.len() > 1).then(|| path.to_string())
}

/// A request as its method and path, the method left out where the code does not say it.
pub fn request(method: &str, path: &str) -> String {
    if method.is_empty() {
        path.to_string()
    } else {
        format!("{method} {path}")
    }
}

/// A request's method and path; the method is empty where the code does not say it.
pub fn method_and_path(request: &str) -> (&str, &str) {
    match request.split_once(' ') {
        Some((m, p)) if !m.starts_with('/') => (m, p),
        _ => ("", request),
    }
}

/// The routes of `routes` (handler, method, path) that serve a request: by its method when the
/// code says it, and its path; a literal segment serves before a parameter (`/years` before
/// `/{year}`).
pub fn serving<'r>(routes: &'r [(String, String, String)], method: &str, path: &str) -> Vec<&'r (String, String, String)> {
    let known = !method.is_empty() && !method.contains('{');
    let served: Vec<&(String, String, String)> = routes.iter().filter(|(_, m, p)| (!known || m.split('|').any(|x| x == method)) && route_matches(p, path)).collect();
    let fewest = served.iter().map(|(_, _, p)| p.matches('{').count()).min().unwrap_or(0);
    served.into_iter().filter(|(_, _, p)| p.matches('{').count() == fewest).collect()
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
    /// What cannot be known, by the name the code gives it.
    Hole(String),
}

pub type Built = Vec<Seg>;

/// The most values a path or a parameter keeps: the alternatives of a choice multiply through
/// each join, so a bound keeps a path built from several choices from growing without end.
pub const MAX: usize = 4;

/// Operations whose path is a directory: what they list, unpack into or make.
pub const DIR_OPS: &[&str] = &[
    "iterdir", "glob", "rglob", "listdir", "scandir", "walk", "extractall", "mkdir", "makedirs", "rmtree", "copytree", "readdir", "readdirSync", "opendir",
    "opendirSync", "mkdirSync", "cp", "cpSync", "create_dir", "create_dir_all", "remove_dir", "remove_dir_all", "read_dir",
];

pub fn lit(s: &str) -> Built {
    vec![Seg::Lit(s.to_string())]
}

pub fn hole(s: &str) -> Built {
    vec![Seg::Hole(s.to_string())]
}

/// `a` then `b`, with `sep` between them; adjacent literal text merges.
pub fn join(a: &Built, b: &Built, sep: &str) -> Built {
    let mut out = a.clone();
    let rest: Vec<Seg> = if sep.is_empty() { b.clone() } else { std::iter::once(Seg::Lit(sep.into())).chain(b.iter().cloned()).collect() };
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

/// The directory a path is in: up to its last `/`.
pub fn parent(b: &Built) -> Built {
    for i in (0..b.len()).rev() {
        if let Seg::Lit(s) = &b[i] {
            let t = s.trim_end_matches('/');
            if let Some(k) = t.rfind('/') {
                let mut out = b[..i].to_vec();
                out.push(Seg::Lit(t[..k.max(1)].to_string()));
                return out;
            }
        }
    }
    match b.as_slice() {
        [Seg::Lit(_)] => lit("."),
        _ => hole("parent"),
    }
}

/// A path with its unknown parts written as `{name}`.
pub fn render(b: &Built) -> String {
    b.iter()
        .map(|s| match s {
            Seg::Lit(x) => x.clone(),
            Seg::Param(x) | Seg::Held(_, x) | Seg::Hole(x) => format!("{{{x}}}"),
        })
        .collect()
}

/// Whether what a call to a database is handed, as built, is a statement Atlas reads once it is
/// whole: the database's statements have a grammar, and the text starts with a verb or with
/// what a caller hands.
pub fn is_statement(db: crate::model::Db, built: &[Built]) -> bool {
    crate::effects::reads_statements(db)
        && built.iter().any(|b| match b.first() {
            Some(Seg::Param(_) | Seg::Held(..)) => true,
            Some(Seg::Lit(l)) => crate::effects::starts_statement(l),
            _ => false,
        })
}

/// The place a path names: the file, or for an operation on a directory that directory. A file
/// whose name is known under a directory that is not (`artefacts/{variant}/report.json`) is
/// that name under any such directory (`artefacts/*/report.json`); a path whose last part is
/// not known rests in the directory before its first unknown part, so what is unpacked into
/// `D/{kind}/{id}` and what lists `D` meet at `D/`. A path that names nothing in the repository
/// (only `.`, or nothing before an unknown) is no place.
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
    let unknown = |s: &str| s.contains(['{', '*', '?', '[']);
    let p = p.trim_end_matches('/').to_string();
    let parts: Vec<&str> = p.split('/').collect();
    let p = match parts.iter().position(|x| unknown(x)) {
        None if dir => format!("{p}/"),
        None => p.clone(),
        Some(i) if !dir && i + 1 < parts.len() && !unknown(parts[parts.len() - 1]) && i > 0 => {
            parts.iter().map(|x| if unknown(x) { "*" } else { x }).collect::<Vec<_>>().join("/")
        }
        Some(0) => return None,
        Some(i) => format!("{}/", parts[..i].join("/")),
    };
    (!matches!(p.as_str(), "" | "." | "./" | "/" | "*")).then_some(p)
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
    let mut out: Vec<Built> = vec![vec![]];
    for s in b {
        let piece = match s {
            Seg::Param(p) => value(p),
            s => vec![vec![s.clone()]],
        };
        out = cross(&out, &piece, "");
    }
    out
}

/// Where a value built from parameters becomes whole: in a function, from what its call hands
/// (`a&b` where it takes two: the function that built an object and the one that handed the
/// rest); or at an entry, from what the entry's users hand it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Whole {
    In(String),
    Users(String),
}

/// What a value built from the parameters of `f` is at each call that hands them, followed out
/// through the callers' parameters in turn, with where it is whole: the URL
/// `request(method, path)` fetches is `/api/customers` where `api.get("/customers")` is called.
/// A parameter a call leaves out holds its `default`; so does one of a function nobody in the
/// program calls (an entry's declared defaults), which otherwise holds what its users hand it,
/// unknown. An entry the program also calls is run by its users as well.
pub fn at_callers<H: Handing>(calls: &[H], family: &crate::builder::Family, default: &dyn Fn(&str, &str) -> Vec<Built>, entry: &dyn Fn(&str) -> bool, f: &str, b: &Built, depth: u32) -> Vec<(Whole, Built)> {
    callers_of(calls, family, default, entry, f, b, None, depth)
}

/// `at_callers`, keeping to the calls of `f` that hand its parameter `only.0` a value that is an
/// `only.1`, or one the code does not say: the member of a family a call on that parameter runs.
#[allow(clippy::too_many_arguments)]
fn callers_of<H: Handing>(calls: &[H], family: &crate::builder::Family, default: &dyn Fn(&str, &str) -> Vec<Built>, entry: &dyn Fn(&str) -> bool, f: &str, b: &Built, only: Option<(&str, &str)>, depth: u32) -> Vec<(Whole, Built)> {
    // what an object holds is what the callers of the function that set it up hand that function:
    // whole where that is, unless `f`'s own callers still have to hand the rest
    if let Some((g, p)) = b.iter().find_map(|s| match s {
        Seg::Held(g, p) => Some((g.clone(), p.clone())),
        _ => None,
    }) {
        let held = if depth > DEPTH { vec![(Whole::In(f.to_string()), hole(&p))] } else { at_callers(calls, family, default, entry, &g, &vec![Seg::Param(p.clone())], depth + 1) };
        let mut out = vec![];
        for (w, v) in held {
            for x in fill(b, &g, &p, &v) {
                for (w2, y) in at_callers(calls, family, default, entry, f, &x, depth + 1) {
                    // whole where both are: a route reaches the one that built the object and the
                    // one that handed `f` the rest
                    let both = match (&w, w2) {
                        (_, Whole::In(x)) if x == f => w.clone(),
                        (Whole::In(a), Whole::In(x)) if a != f && *a != x => Whole::In(format!("{x}&{a}")),
                        (_, w2) => w2,
                    };
                    out.push((both, y));
                    if out.len() >= COMPLETIONS {
                        return out;
                    }
                }
            }
        }
        return out;
    }
    if !b.iter().any(|s| matches!(s, Seg::Param(_))) {
        return vec![(Whole::In(f.to_string()), b.clone())];
    }
    let unknown = |p: &str| match default(f, p) {
        d if d.is_empty() => vec![hole(p)],
        d => d,
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
        .chain(family.overrides.get(f).unwrap_or(&none).iter().flat_map(|(base, named)| calls.iter().filter(move |a| a.callee() == base).map(move |a| (a, Some(named)))))
        .filter(|(a, named)| match only {
            Some((p, class)) => theirs(*named, p).and_then(|q| a.class_handed(&q)).map_or(true, |k| family.is_a(&k, class)),
            None => true,
        })
        .collect();
    if sites.is_empty() || depth > DEPTH {
        return substitute(b, unknown).into_iter().map(|x| (Whole::In(f.to_string()), x)).collect();
    }
    let mut out: Vec<(Whole, Built)> = if entry(f) { substitute(b, unknown).into_iter().map(|x| (Whole::Users(f.to_string()), x)).collect() } else { vec![] };
    for (a, named) in sites {
        let hands = |p: &str| theirs(named, p).and_then(|q| a.hands(&q));
        // through a base's method called on a parameter, the callers that hand that parameter
        // decide which member runs: the one `f` is a method of
        let class = f.rsplit_once('.').map_or("", |(c, _)| c);
        let next = named.and(a.on()).map(|q| (q, class));
        for x in substitute(b, |p| hands(p).filter(|v| !v.is_empty()).unwrap_or_else(|| unknown(p))) {
            out.extend(callers_of(calls, family, default, entry, a.caller(), &x, next, depth + 1));
            if out.len() >= COMPLETIONS {
                return out;
            }
        }
    }
    out
}

/// `b` with what the object holds for parameter `p` of `g` written in.
fn fill(b: &Built, g: &str, p: &str, v: &Built) -> Vec<Built> {
    let mut out: Vec<Built> = vec![vec![]];
    for s in b {
        let piece = match s {
            Seg::Held(h, q) if h == g && q == p => vec![v.clone()],
            s => vec![vec![s.clone()]],
        };
        out = cross(&out, &piece, "");
    }
    out
}

/// A file, network or database call whose path, URL or statement waits for what its callers
/// hand: the function that makes it, its touch, and the path, URL or statement as built.
pub type Waiting = (String, crate::model::Touch, Vec<Built>);

/// A waiting call with its path, URL or statement at each call that completes it, with where it
/// is whole.
pub type Completed = (String, crate::model::Touch, Vec<(Whole, Built)>);

/// Complete the waiting calls through their callers; `entry` says which functions are entries.
pub fn complete<H: Handing>(calls: Vec<Waiting>, handing: &[H], family: &crate::builder::Family, default: &dyn Fn(&str, &str) -> Vec<Built>, entry: &dyn Fn(&str) -> bool) -> Vec<Completed> {
    calls
        .into_iter()
        .map(|(f, t, built)| {
            let found = built.iter().flat_map(|b| at_callers(handing, family, default, entry, &f, b, 0)).collect();
            (f, t, found)
        })
        .collect()
}

/// Place the completed calls: a file is touched by the function that opens it, at every place
/// it is opened at; a URL without a host that one of the program's routes serves is a request
/// from the function that completes it to the route's handler; a statement is read by its
/// grammar, and the function that runs it touches each table it names, the way it goes there.
/// Each touch keeps where it became whole, so a route shows the places its own calls hand; where
/// a completion names no place, the call keeps the touch it was read with.
pub fn place_calls(store: &mut crate::builder::GraphBuilder, completed: Vec<Completed>) {
    use crate::model::{Crossing, EdgeKind, EntryKind, Touch, World};
    let routes: Vec<(String, String, String)> = store
        .nodes
        .values()
        .filter_map(|n| {
            let e = n.entry.as_ref().filter(|e| e.kind == EntryKind::Route)?;
            let (method, path) = e.label.split_once(' ')?;
            Some((n.id.clone(), method.to_string(), path.to_string()))
        })
        .collect();
    for (f, t, found) in completed {
        // what each place of completion makes of it: the alternatives one caller hands are one
        let mut wholes: Vec<&Whole> = vec![];
        for (w, _) in &found {
            if !wholes.contains(&w) {
                wholes.push(w);
            }
        }
        // a route that reaches none of the callers that make it whole sees the call as it was read
        if wholes.iter().any(|w| **w != Whole::In(f.clone())) {
            store.instead(&f, t.clone());
        }
        for whole in wholes {
            let (by, users_of) = match whole {
                Whole::In(h) if *h == f => (vec![], vec![]),
                Whole::In(h) => (vec![h.clone()], vec![]),
                Whole::Users(e) => (vec![], vec![e.clone()]),
            };
            let holder = match whole {
                Whole::In(h) | Whole::Users(h) => h,
            };
            let mut placed: Vec<(String, Option<crate::model::Dir>)> = vec![];
            let mut requested = false;
            for b in found.iter().filter(|(w, _)| w == whole).map(|(_, b)| b) {
                let rendered = render(b);
                // a request's method, when the code says it, and its URL
                let (method, url) = match rendered.split_once(' ') {
                    Some((m, u)) if t.kind == World::Net => (m.to_ascii_uppercase(), u.to_string()),
                    _ => (String::new(), rendered.clone()),
                };
                if t.kind == World::Net {
                    let asker = holder.split('&').next().unwrap_or(holder);
                    let mut served = false;
                    if url.starts_with('/') {
                        for (handler, _, path) in serving(&routes, &method, &url) {
                            store.add_edge(asker, handler, EdgeKind::Requests, Some(path.clone()));
                            served = true;
                        }
                    } else if let Some(host) = crate::effects::facts(Crossing::to(World::Net), None, &t.op, std::slice::from_ref(&url)).places().into_iter().next().map(|(a, _)| a).filter(|a| !a.is_empty()) {
                        placed.push((host, t.dir));
                    }
                    requested |= served;
                    // a request no route of the part serves may be another part's
                    if let Some(path) = asked_path(&url).filter(|_| !served) {
                        store.ask(asker, request(&method, &path));
                    }
                } else if let World::Db(db) = t.kind {
                    placed.extend(crate::effects::sql(db, std::slice::from_ref(&rendered)).unwrap_or_default().into_iter().map(|(table, dir)| (table, dir.or(t.dir))));
                } else if let Some(p) = place(b, &t.op) {
                    placed.push((p, t.dir));
                }
            }
            placed.sort();
            placed.dedup();
            let at = |artifact: String, dir, fixed| Touch { artifact, dir, fixed, by: by.clone(), users_of: users_of.clone(), ..t.clone() };
            if placed.is_empty() {
                if !requested {
                    // a URL without a host is the page's own origin, decided by whoever serves it
                    let fixed = t.fixed && (t.kind != World::Net || !t.artifact.is_empty());
                    store.touch(&f, at(t.artifact.clone(), t.dir, fixed));
                }
                continue;
            }
            for (artifact, dir) in placed {
                store.touch(&f, at(artifact, dir, t.fixed));
            }
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
        assert!(!item("/api/items"));
        assert_eq!(asked_path("/"), None, "the root names no route");
        assert_eq!(asked_path("{url}"), None, "a URL not known at all asks for no path");
        assert_eq!(asked_path("data/raw.csv"), None, "a relative path is a file");
        assert_eq!(method_and_path("POST /v1/orders"), ("POST", "/v1/orders"));
        assert_eq!(method_and_path("/v1/orders"), ("", "/v1/orders"));
    }
}
