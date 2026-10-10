//! Paths as code builds them, in any language: literal text, the parameters of the function a
//! path is built in, and what cannot be known (a hole, named). A parameter holds what its
//! callers pass, over all its call sites; the place of a path with an unknown part is the
//! directory before that part, so what one route unpacks into `D/{kind}/{id}` and another lists
//! in `D` meet at `D/`.

use std::collections::{HashMap, HashSet};

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

/// One argument a call hands a parameter, as the caller builds it.
pub struct Passed {
    pub caller: String,
    pub callee: String,
    pub param: String,
    pub values: Vec<Built>,
}

/// What each function's parameters hold: what their callers pass, carried through the
/// parameters of the callers in turn, until nothing changes.
pub fn handed(passed: &[Passed], base: HashMap<(String, String), Vec<Built>>) -> HashMap<(String, String), Vec<Built>> {
    let mut values = base;
    for _ in 0..10 {
        let mut changed = false;
        for p in passed {
            let got: Vec<Built> = p.values.iter().flat_map(|b| bind(b, &p.caller, &values)).collect();
            let slot = values.entry((p.callee.clone(), p.param.clone())).or_default();
            for g in got {
                if slot.len() < MAX && !slot.contains(&g) {
                    slot.push(g);
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    values
}

/// A path with the parameters of `f` replaced by what they hold; one that holds nothing known
/// stays a hole.
pub fn bind(b: &Built, f: &str, values: &HashMap<(String, String), Vec<Built>>) -> Vec<Built> {
    let mut out: Vec<Built> = vec![vec![]];
    for s in b {
        let piece: Vec<Built> = match s {
            Seg::Param(p) => match values.get(&(f.to_string(), p.clone())).filter(|v| !v.is_empty()) {
                Some(v) => v.iter().map(|x| x.iter().map(|s| if let Seg::Param(q) = s { Seg::Hole(q.clone()) } else { s.clone() }).collect()).collect(),
                None => vec![hole(p)],
            },
            s => vec![vec![s.clone()]],
        };
        out = cross(&out, &piece, "");
    }
    let mut seen = HashSet::new();
    out.retain(|b| seen.insert(b.clone()));
    out
}
