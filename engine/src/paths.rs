//! Paths as code builds them, in any language: literal text, the parameters of the function a
//! path is built in, and what cannot be known (a hole, named). A parameter holds what its
//! callers pass, over all its call sites; the place of a path with an unknown part is the
//! directory before that part, so what one route unpacks into `D/{kind}/{id}` and another lists
//! in `D` meet at `D/`.


/// A part of a path as the code builds it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Seg {
    Lit(String),
    /// A parameter of the function the path is built in.
    Param(String),
    /// What cannot be known, by the name the code gives it.
    Hole(String),
}

pub type Built = Vec<Seg>;

/// The most values a path or a parameter keeps.
pub const MAX: usize = 4;

/// Operations whose path is a directory: what they list, unpack into or make.
pub const DIR_OPS: &[&str] = &[
    "iterdir", "glob", "rglob", "listdir", "scandir", "walk", "extractall", "mkdir", "makedirs", "rmtree", "copytree", "readdir", "readdirSync", "opendir",
    "opendirSync", "mkdirSync", "cp", "cpSync",
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
            Seg::Param(x) | Seg::Hole(x) => format!("{{{x}}}"),
        })
        .collect()
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
}

/// Each parameter of `b` replaced by what `value` says it holds, over every combination.
fn substitute(b: &Built, mut value: impl FnMut(&str) -> Vec<Built>) -> Vec<Built> {
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

/// What a value built from the parameters of `f` is at each call that hands them, followed out
/// through the callers' parameters in turn, with the function where it is complete: the URL
/// `request(method, path)` fetches is `/api/customers` where `api.get("/customers")` is called.
/// A parameter a call leaves out holds its `default`; so does one of a function nobody in the
/// program calls (an entry's declared defaults), which otherwise holds what its users hand it,
/// unknown.
pub fn at_callers<H: Handing>(calls: &[H], default: &dyn Fn(&str, &str) -> Vec<Built>, f: &str, b: &Built, depth: u32) -> Vec<(String, Built)> {
    if !b.iter().any(|s| matches!(s, Seg::Param(_))) {
        return vec![(f.to_string(), b.clone())];
    }
    let unknown = |p: &str| match default(f, p) {
        d if d.is_empty() => vec![hole(p)],
        d => d,
    };
    let sites: Vec<&H> = calls.iter().filter(|a| a.callee() == f).collect();
    if sites.is_empty() || depth > DEPTH {
        return substitute(b, unknown).into_iter().map(|x| (f.to_string(), x)).collect();
    }
    let mut out = vec![];
    for a in sites {
        for x in substitute(b, |p| a.hands(p).filter(|v| !v.is_empty()).unwrap_or_else(|| unknown(p))) {
            out.extend(at_callers(calls, default, a.caller(), &x, depth + 1));
            if out.len() >= COMPLETIONS {
                return out;
            }
        }
    }
    out
}

/// A file or network call whose path or URL waits for what its callers hand: the function that
/// makes it, its touch, and the path or URL as built.
pub type Waiting = (String, crate::model::Touch, Vec<Built>);

/// A waiting call with its path or URL complete at each call that completes it, by the function
/// where it is complete.
pub type Completed = (String, crate::model::Touch, Vec<(String, Built)>);

/// Complete the waiting calls through their callers.
pub fn complete<H: Handing>(calls: Vec<Waiting>, handing: &[H], default: &dyn Fn(&str, &str) -> Vec<Built>) -> Vec<Completed> {
    calls
        .into_iter()
        .map(|(f, t, built)| {
            let found = built.iter().flat_map(|b| at_callers(handing, default, &f, b, 0)).collect();
            (f, t, found)
        })
        .collect()
}

/// Place the completed calls: a file is touched by the function that opens it, at every place
/// it is opened at; a URL without a host that one of the program's routes serves is a request
/// from the function that completes it to the route's handler.
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
        let mut placed: Vec<String> = vec![];
        let mut requested = false;
        for (holder, b) in &found {
            let rendered = render(b);
            // a request's method, when the code says it, and its URL
            let (method, url) = match rendered.split_once(' ') {
                Some((m, u)) if t.kind == World::Net => (m.to_ascii_uppercase(), u.to_string()),
                _ => (String::new(), rendered.clone()),
            };
            if t.kind == World::Net {
                if url.starts_with('/') {
                    let known = !method.contains('{');
                    let served: Vec<&(String, String, String)> = routes.iter().filter(|(_, m, p)| (!known || *m == method) && crate::parts::route_matches(p, &url)).collect();
                    // a literal segment serves before a parameter: `/years` before `/{year}`
                    let fewest = served.iter().map(|(_, _, p)| p.matches('{').count()).min().unwrap_or(0);
                    for (handler, _, path) in served.into_iter().filter(|(_, _, p)| p.matches('{').count() == fewest) {
                        store.add_edge(holder, handler, EdgeKind::Requests, Some(path.clone()));
                        requested = true;
                    }
                } else if let Some(host) = crate::effects::facts(Crossing::to(World::Net), None, &t.op, &[url.clone()]).places().into_iter().next().map(|(a, _)| a).filter(|a| !a.is_empty()) {
                    placed.push(host);
                }
            } else if let Some(p) = place(b, &t.op) {
                placed.push(p);
            }
        }
        placed.sort();
        placed.dedup();
        if placed.is_empty() {
            if !requested {
                // a URL without a host is the page's own origin, decided by whoever serves it
                let fixed = t.fixed && (t.kind != World::Net || !t.artifact.is_empty());
                store.touch(&f, Touch { fixed, ..t });
            }
            continue;
        }
        for artifact in placed {
            store.touch(&f, Touch { artifact, ..t.clone() });
        }
    }
}
