//! The package's `tsconfig.json` (or `jsconfig.json`) as the compiler reads it: JSON with comments,
//! the path aliases imports resolve through, and the files it compiles.
//!
//! The files: `files` by name, `include` by pattern (`**/*` when neither is given), less what `exclude`
//! matches (`node_modules`, `bower_components`, `jspm_packages` and `outDir` when it is not
//! given). Each of the three is taken from the nearest config along `extends` that gives it, read
//! from that config's directory, and a config's `references` add their projects' files, as
//! `tsc --build` reads them. A pattern's last segment without a wildcard or an extension names a
//! directory, and stands for everything below it; `*` and `?` never cross a `/`, `**/` crosses
//! any number of directories.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use path_clean::clean;
use regex::Regex;

use crate::repo::Repo;

/// The configs the compiler looks for at a package's root, in order.
const CONFIGS: [&str; 2] = ["tsconfig.json", "jsconfig.json"];

/// What `exclude` is when a config gives none (`outDir` besides).
const EXCLUDED: [&str; 3] = ["node_modules", "bower_components", "jspm_packages"];

/// The files one project of the package compiles, from the package's root.
struct Project {
    files: HashSet<PathBuf>,
    include: Vec<Regex>,
    exclude: Vec<Regex>,
}

impl Project {
    fn reads(&self, rel: &Path) -> bool {
        let s = crate::tree::path_str(rel);
        self.files.contains(rel) || (self.include.iter().any(|r| r.is_match(&s)) && !self.exclude.iter().any(|r| r.is_match(&s)))
    }
}

/// The projects a package's config names: its own and those its `references` add.
pub(super) struct Compiled(Vec<Project>);

impl Compiled {
    /// The package's config, when it has one.
    pub(super) fn of(repo: &Repo) -> Option<Compiled> {
        let config = CONFIGS.iter().find(|c| repo.is_file(c))?;
        let mut projects = vec![];
        let mut seen = HashSet::new();
        project(repo, Path::new(config), &mut seen, &mut projects);
        Some(Compiled(projects))
    }

    /// Whether the compiler reads the file at `rel`, from the package's root.
    pub(super) fn reads(&self, rel: &Path) -> bool {
        self.0.iter().any(|p| p.reads(rel))
    }
}

/// The config at `rel` read into `out` as a project, with the projects its `references` name.
fn project(repo: &Repo, rel: &Path, seen: &mut HashSet<PathBuf>, out: &mut Vec<Project>) {
    if !seen.insert(rel.to_path_buf()) || seen.len() > 16 {
        return;
    }
    let chain = chain(repo, rel);
    let Some((dir, json)) = chain.first() else { return };
    // each of `files`, `include`, `exclude` and `outDir` from the nearest config that gives it
    let given = |key: &str| chain.iter().find(|(_, j)| !j[key].is_null()).map(|(d, j)| (d.clone(), j[key].clone()));
    let out_dir = chain.iter().find_map(|(d, j)| j["compilerOptions"]["outDir"].as_str().map(|o| d.join(o)));
    let strings = |v: &serde_json::Value| -> Vec<String> {
        v.as_array().into_iter().flatten().filter_map(|s| s.as_str().map(str::to_string)).collect()
    };
    let files: HashSet<PathBuf> = given("files").map(|(d, v)| strings(&v).iter().map(|f| clean(d.join(f))).collect()).unwrap_or_default();
    let include: Vec<Regex> = match given("include") {
        Some((d, v)) => strings(&v).iter().filter_map(|p| pattern(&d, p)).collect(),
        None if given("files").is_none() => pattern(Path::new(""), "**/*").into_iter().collect(),
        None => vec![],
    };
    let exclude: Vec<Regex> = match given("exclude") {
        Some((d, v)) => strings(&v).iter().filter_map(|p| pattern(&d, p)).collect(),
        None => {
            EXCLUDED.iter().map(PathBuf::from).chain(out_dir).filter_map(|p| pattern(Path::new(""), &crate::tree::path_str(&p))).collect()
        }
    };
    out.push(Project { files, include, exclude });
    for r in json["references"].as_array().into_iter().flatten() {
        let Some(path) = r["path"].as_str() else { continue };
        let at = clean(dir.join(path));
        let at = if repo.is_dir(&at) { at.join("tsconfig.json") } else { at };
        project(repo, &at, seen, out);
    }
}

/// The config at `rel` and those it `extends`, nearest first, each with its directory from the
/// package's root. A config a library ships (`"extends": "astro/tsconfigs/strict"`) names no files
/// of the package and is not read.
fn chain(repo: &Repo, rel: &Path) -> Vec<(PathBuf, serde_json::Value)> {
    let mut out: Vec<(PathBuf, serde_json::Value)> = vec![];
    let mut read: HashSet<PathBuf> = HashSet::new();
    let mut next = vec![rel.to_path_buf()];
    while let Some(at) = next.pop() {
        if out.len() > 16 || !read.insert(at.clone()) {
            continue;
        }
        let Some(json) = repo.text(&at).and_then(|t| serde_json::from_str::<serde_json::Value>(&jsonc(&t)).ok()) else { continue };
        let dir = at.parent().map(Path::to_path_buf).unwrap_or_default();
        let bases: Vec<String> = match &json["extends"] {
            serde_json::Value::String(s) => vec![s.clone()],
            serde_json::Value::Array(a) => a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect(),
            _ => vec![],
        };
        // a later base overrides an earlier one, with what it extends in turn: the last is taken first
        for base in bases.iter().filter(|b| b.starts_with("./") || b.starts_with("../")) {
            let p = clean(dir.join(base));
            next.push(if repo.is_file(&p) { p } else { PathBuf::from(format!("{}.json", crate::tree::path_str(&p))) });
        }
        out.push((dir, json));
    }
    out
}

/// A `tsconfig` pattern read from `dir` as a regex over paths from the package's root: one that
/// names a directory matches everything below it. None for a pattern above the package.
fn pattern(dir: &Path, pat: &str) -> Option<Regex> {
    let joined = clean(dir.join(pat.trim_start_matches("./")));
    let p = crate::tree::path_str(&joined);
    if p.starts_with("..") {
        return None;
    }
    let p = if p == "." { String::new() } else { p };
    let last = p.rsplit('/').next().unwrap_or("");
    let directory = !last.contains(['*', '?', '.']);
    let mut re = String::from("^");
    let mut rest = p.as_str();
    while !rest.is_empty() {
        if let Some(r) = rest.strip_prefix("**/") {
            re.push_str("(?:[^/]+/)*");
            rest = r;
        } else if rest == "**" {
            re.push_str(".*");
            rest = "";
        } else {
            let c = rest.chars().next().unwrap();
            match c {
                '*' => re.push_str("[^/]*"),
                '?' => re.push_str("[^/]"),
                c => re.push_str(&regex::escape(&c.to_string())),
            }
            rest = &rest[c.len_utf8()..];
        }
    }
    if directory {
        re.push_str(if p.is_empty() { ".*" } else { "(?:/.*)?" });
    }
    re.push('$');
    Regex::new(&re).ok()
}

/// tsconfig `paths`: alias prefix -> directory, `"@/*": ["./src/*"]` -> (`@/`, `<repo>/src/`).
pub(super) fn path_aliases(repo: &Repo) -> Vec<(String, PathBuf)> {
    let mut out = vec![];
    for cfg in ["tsconfig.json", "jsconfig.json"] {
        let Some(text) = repo.text(cfg) else { continue };
        let Ok(json) = serde_json::from_str::<serde_json::Value>(&jsonc(&text)) else { continue };
        let opts = &json["compilerOptions"];
        let base = repo.root().join(opts["baseUrl"].as_str().unwrap_or("."));
        if let Some(paths) = opts["paths"].as_object() {
            for (alias, targets) in paths {
                let Some(first) = targets.as_array().and_then(|t| t.first()).and_then(|t| t.as_str()) else { continue };
                out.push((alias.trim_end_matches('*').to_string(), base.join(first.trim_end_matches('*'))));
            }
        }
    }
    out
}

/// JSON with comments and trailing commas, as tsconfig allows, made plain JSON. Comments are
/// recognised outside strings only: `"@/*"` is a path, not the start of one.
pub(super) fn jsonc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            match c {
                '\\' => out.extend(chars.next()),
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match (c, chars.peek()) {
            ('"', _) => {
                in_string = true;
                out.push(c);
            }
            ('/', Some('/')) => while chars.next_if(|&n| n != '\n').is_some() {},
            ('/', Some('*')) => {
                chars.next();
                let mut prev = ' ';
                for n in chars.by_ref() {
                    if prev == '*' && n == '/' {
                        break;
                    }
                    prev = n;
                }
            }
            _ => out.push(c),
        }
    }
    Regex::new(r",(\s*[}\]])").unwrap().replace_all(&out, "$1").to_string()
}

#[cfg(test)]
mod tests {
    use super::pattern;
    use std::path::Path;

    /// A pattern matches as the compiler matches it: `*` within one segment, `**/` across any
    /// number, a directory for everything below it, from the directory of the config that gives it.
    #[test]
    fn patterns_match_as_the_compiler_matches() {
        let matches = |dir: &str, pat: &str, path: &str| pattern(Path::new(dir), pat).is_some_and(|r| r.is_match(path));
        assert!(matches("", "**/*", "a.ts") && matches("", "**/*", "scripts/a/b.ts"));
        assert!(matches("", "src", "src/lib/x.ts") && !matches("", "src", "scripts/x.ts") && !matches("", "src", "srcs/x.ts"));
        assert!(matches("", "src/*.ts", "src/x.ts") && !matches("", "src/*.ts", "src/lib/x.ts"));
        assert!(matches("", "./functions/**/*", "functions/api/x.ts"));
        assert!(matches("config", "../scripts", "scripts/x.ts") && matches("config", "src", "config/src/x.ts"));
        assert!(pattern(Path::new(""), "../other").is_none());
    }
}
