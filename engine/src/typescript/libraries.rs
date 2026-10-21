//! A package's own code, read as far as the program's calls go into it (`--libraries N`): the
//! files of a package in `node_modules` that define what the program calls, then in each further
//! round the files of the same packages that those files' calls go into. What a library calls in
//! a file not read, or in another package, stays a library's call, as the program's own calls
//! into a package not read do.
//!
//! A specifier names what the package exports at that path (`exports`, else `module` or `main`),
//! as code: never its types (`.d.ts`). Not read: Node's own modules, a package the tables in
//! `knowledge.rs` describe (the table is the knowledge), and a package with compiled code (a
//! native addon, WebAssembly). A library's code is the environment's: read from the disk, never
//! from the repository's listing.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use serde_json::Value;
use tree_sitter::Parser;
use walkdir::WalkDir;

use super::module::{function_of, is_code, member_parts, module_id, package_of, segment, ts_parser, Bound, Module, EXTENSIONS};
use super::passes::{collect_imports, collect_symbols, collect_wrapped};
use crate::builder::GraphBuilder;
use crate::effects;
use crate::knowledge::{self, Registry};
use crate::model::{Node, NodeKind};
use crate::repo::{normal, Repo};
use crate::tree::{span, text, walk};

/// The conditions of a package's `exports` that an import of its code meets, the first a
/// package names chosen: never `types`.
const CONDITIONS: &[&str] = &["import", "module", "default", "node", "require", "browser"];

/// How many re-exports a name is followed through.
const DEPTH: u32 = 12;

/// A package installed for the program, as Atlas reads it.
struct Package {
    /// Its name: `filekit`, `@scope/name`.
    name: String,
    dir: PathBuf,
    manifest: Value,
    /// Its code, each file by its module (`filekit.lib.read` for `lib/read.js`), and each module
    /// by its file.
    files: HashMap<PathBuf, String>,
    by_id: HashMap<String, PathBuf>,
}

impl Package {
    /// The package at `dir`, unless Atlas does not read it: one the tables describe, or one
    /// with compiled code.
    fn read(dir: &Path) -> Option<Package> {
        let manifest: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("package.json")).ok()?).ok()?;
        let name = manifest["name"].as_str()?.to_string();
        if effects::described(&name, Registry::Npm) {
            return None;
        }
        let compiled = knowledge::compiled(Registry::Npm);
        let mut paths = vec![];
        for e in WalkDir::new(dir).into_iter().filter_entry(|e| e.depth() == 0 || e.file_name() != knowledge::NODE_PACKAGES).flatten() {
            let ext = e.path().extension().and_then(|x| x.to_str()).unwrap_or("");
            if compiled.contains(&ext) {
                return None;
            }
            if e.file_type().is_file() && is_code(e.path()) {
                paths.push(normal(e.path()));
            }
        }
        paths.sort();
        let root = name.split('/').map(segment).collect::<Vec<_>>().join(".");
        let (mut files, mut by_id) = (HashMap::new(), HashMap::new());
        for p in paths {
            // two builds of one module (`index.js`, `index.mjs`) are two modules
            let mut id = module_id(&root, dir, &p);
            if by_id.contains_key(&id) {
                id = format!("{id}_{}", p.extension().and_then(|x| x.to_str()).unwrap_or(""));
            }
            by_id.insert(id.clone(), p.clone());
            files.insert(p, id);
        }
        Some(Package { name, dir: dir.to_path_buf(), manifest, files, by_id })
    }

    /// The file a specifier's path below the package names (`""` for the package itself): what
    /// the package exports there, else the file at that path.
    fn entry(&self, sub: &str) -> Option<PathBuf> {
        let exports = &self.manifest["exports"];
        let target = match exports {
            Value::Null if sub.is_empty() => {
                ["module", "main"].iter().find_map(|k| self.manifest[*k].as_str()).unwrap_or("index").to_string()
            }
            Value::Null => sub.to_string(),
            _ => exported(exports, &if sub.is_empty() { ".".to_string() } else { format!("./{sub}") })?,
        };
        self.file_at(&target)
    }

    /// The code file a path in the package names, with or without its extension, or the index
    /// of the directory it names.
    fn file_at(&self, target: &str) -> Option<PathBuf> {
        let base = normal(&self.dir.join(target));
        let stripped = base.with_extension("");
        let mut candidates = vec![base.clone()];
        for b in [&base, &stripped] {
            for ext in EXTENSIONS {
                candidates.push(PathBuf::from(format!("{}.{ext}", b.display())));
                candidates.push(b.join(format!("index.{ext}")));
            }
        }
        candidates.into_iter().find(|c| self.files.contains_key(c))
    }
}

/// What a package's `exports` gives at `key` (`.`, `./sub`): a path, by the conditions an import
/// meets; a pattern (`./*`) by what its `*` stands for.
fn exported(exports: &Value, key: &str) -> Option<String> {
    match exports {
        Value::Object(o) if o.keys().any(|k| k.starts_with('.')) => {
            if let Some(v) = o.get(key) {
                return condition(v);
            }
            o.iter().find_map(|(k, v)| {
                let (pre, post) = k.split_once('*')?;
                let star = key.strip_prefix(pre)?.strip_suffix(post)?;
                condition(v).map(|t| t.replace('*', star))
            })
        }
        _ if key == "." => condition(exports),
        _ => None,
    }
}

/// The path a value of `exports` gives an import of code.
fn condition(v: &Value) -> Option<String> {
    match v {
        Value::String(s) if !types_only(s) => Some(s.clone()),
        Value::Array(a) => a.iter().find_map(condition),
        Value::Object(o) => CONDITIONS.iter().find_map(|c| o.get(*c).and_then(condition)),
        _ => None,
    }
}

/// Whether a file holds only types: `.d.ts`, `.d.mts`, `.d.cts`.
fn types_only(p: &str) -> bool {
    [".d.ts", ".d.mts", ".d.cts"].iter().any(|x| p.ends_with(x))
}

/// Where a name is defined: the file, the definition's id, and how many of the names followed
/// it stands for (`Client` of `Client.create` is one).
type Def = (PathBuf, String, usize);

/// The packages the program uses, read as far as their code is asked for.
struct Libraries {
    /// The repository as the caller named it, and on disk: a package below it is shown below it.
    root: PathBuf,
    here: PathBuf,
    packages: HashMap<PathBuf, Option<Package>>,
    /// Where a package of a name is found from a directory.
    found: HashMap<(PathBuf, String), Option<PathBuf>>,
    /// The package each code file is in.
    owner: HashMap<PathBuf, PathBuf>,
    /// Files parsed to follow a name through, with what they import, bind and define.
    parsed: HashMap<PathBuf, Option<Module>>,
    followed: HashMap<(PathBuf, Vec<String>), Option<Def>>,
    /// The files read as code.
    read: HashSet<PathBuf>,
    ts: Option<Parser>,
    tsx: Option<Parser>,
}

impl Libraries {
    fn new(root: &Path) -> Libraries {
        Libraries {
            root: root.to_path_buf(),
            here: std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf()),
            packages: HashMap::new(),
            found: HashMap::new(),
            owner: HashMap::new(),
            parsed: HashMap::new(),
            followed: HashMap::new(),
            read: HashSet::new(),
            ts: ts_parser(false).ok(),
            tsx: ts_parser(true).ok(),
        }
    }

    /// The package `name` as Node finds it from a file: in `node_modules` beside the file's
    /// directory or beside one above it.
    fn locate(&mut self, from: &Path, name: &str) -> Option<PathBuf> {
        let dir = std::fs::canonicalize(from).ok()?.parent()?.to_path_buf();
        let key = (dir.clone(), name.to_string());
        if let Some(f) = self.found.get(&key) {
            return f.clone();
        }
        let found = dir.ancestors().map(|a| a.join(knowledge::NODE_PACKAGES).join(name)).find(|d| d.join("package.json").is_file());
        // a package of the repository's own, linked into `node_modules` (a workspace's member), is
        // read as its own part
        let own = |d: &PathBuf| {
            std::fs::canonicalize(d)
                .map_or(false, |abs| abs.starts_with(&self.here) && !abs.components().any(|c| c.as_os_str() == knowledge::NODE_PACKAGES))
        };
        // a package below the repository is named from it, like its own files
        let found = found.filter(|d| !own(d)).map(|d| d.strip_prefix(&self.here).map(|r| normal(&self.root.join(r))).unwrap_or(d));
        self.found.insert(key, found.clone());
        found
    }

    fn package(&mut self, dir: &Path) -> Option<&Package> {
        if !self.packages.contains_key(dir) {
            let p = Package::read(dir);
            for f in p.iter().flat_map(|p| p.files.keys()) {
                self.owner.insert(f.clone(), dir.to_path_buf());
            }
            self.packages.insert(dir.to_path_buf(), p);
        }
        self.packages.get(dir)?.as_ref()
    }

    /// The package a code file is in.
    fn holding(&self, file: &Path) -> Option<&Package> {
        self.packages.get(self.owner.get(file)?)?.as_ref()
    }

    /// The file a package's specifier names, imported from `from`: none for Node's own modules,
    /// for a package Atlas does not read, and for a path that is no code.
    fn entry(&mut self, from: &Path, spec: &str) -> Option<PathBuf> {
        if spec.starts_with(['.', '/']) || spec.starts_with("node:") {
            return None;
        }
        let name = package_of(spec);
        if knowledge::NODE_BUILTINS.contains(&name.as_str()) {
            return None;
        }
        let sub = spec[name.len()..].trim_start_matches('/').to_string();
        let dir = self.locate(from, &name)?;
        self.package(&dir)?.entry(&sub)
    }

    /// A file of a package, parsed: its module with what it imports, binds and defines.
    fn parse(&mut self, file: &Path) -> Option<Module> {
        let id = self.holding(file)?.files.get(file)?.clone();
        let src = std::fs::read(file).ok()?;
        let jsx = matches!(file.extension().and_then(|e| e.to_str()), Some("tsx" | "jsx" | "js" | "mjs" | "cjs"));
        let tree = (if jsx { self.tsx.as_mut() } else { self.ts.as_mut() })?.parse(&src, None)?;
        let mut m = Module::new(id, file.to_path_buf(), src, tree);
        collect_imports(&mut m, &[], &self.holding(file)?.files);
        Some(m)
    }

    /// A file parsed to follow a name through, with its symbols.
    fn parsed(&mut self, file: &Path) -> Option<&Module> {
        if !self.parsed.contains_key(file) {
            let m = self.parse(file).map(|mut m| {
                collect_wrapped(&mut m);
                collect_symbols(&mut m, &mut GraphBuilder::default());
                m
            });
            self.parsed.insert(file.to_path_buf(), m);
        }
        self.parsed.get(file)?.as_ref()
    }

    /// Where `names` (a name the file exports, and the members written after it) is defined:
    /// through the file's re-exports, its `export *`, and the packages it re-exports from.
    fn follow(&mut self, file: &Path, names: &[String], depth: u32) -> Option<Def> {
        if names.is_empty() || depth > DEPTH {
            return None;
        }
        let key = (file.to_path_buf(), names.to_vec());
        if let Some(d) = self.followed.get(&key) {
            return d.clone();
        }
        // a cycle of re-exports ends where it started
        self.followed.insert(key.clone(), None);
        let d = self.follow_in(file, names, depth);
        self.followed.insert(key, d.clone());
        d
    }

    fn follow_in(&mut self, file: &Path, names: &[String], depth: u32) -> Option<Def> {
        let m = self.parsed(file)?;
        let name = &names[0];
        if let Some(id) = m.symbols.get(name).filter(|_| name != "default") {
            return Some((file.to_path_buf(), id.clone(), 1));
        }
        let next = if name == "default" { m.default.clone() } else { m.bindings.get(name).cloned() };
        let (mid, symbols, wildcards) = (m.id.clone(), m.symbols.clone(), m.wildcards.clone());
        let imported = m.imported.iter().find(|i| i.local == *name).cloned();
        let rest = |first: &str| std::iter::once(first.to_string()).chain(names[1..].iter().cloned()).collect::<Vec<_>>();
        match next {
            Some(Bound::Own(t)) => {
                // a module of the package: what follows is a name it exports
                if let Some(f) = self.holding(file)?.by_id.get(&t).cloned() {
                    return self.follow(&f, &names[1..], depth + 1).map(|(f, id, n)| (f, id, n + 1));
                }
                let (tm, local) = t.rsplit_once('.')?;
                // a name of this module under another: `export default X`, `export { a as b }`
                if tm == mid {
                    if let Some(id) = symbols.get(local) {
                        return Some((file.to_path_buf(), id.clone(), 1));
                    }
                    return (local != name).then(|| self.follow(file, &rest(local), depth + 1)).flatten();
                }
                let f = self.holding(file)?.by_id.get(tm).cloned()?;
                return self.follow(&f, &rest(local), depth + 1);
            }
            // re-exported from another package
            Some(Bound::Ext(_)) => {
                let imported = imported?;
                let f = self.entry(file, &imported.spec)?;
                return match imported.name {
                    Some(n) => self.follow(&f, &rest(&n), depth + 1),
                    None => self.follow(&f, &names[1..], depth + 1).map(|(f, id, n)| (f, id, n + 1)),
                };
            }
            None => {}
        }
        for w in wildcards {
            let Some(f) = self.holding(file).and_then(|p| p.by_id.get(&w).cloned()) else { continue };
            if let Some(d) = self.follow(&f, names, depth + 1) {
                return Some(d);
            }
        }
        None
    }

    /// The names `m` imports from a package Atlas reads, by the file each comes from.
    fn link(&mut self, m: &mut Module) {
        for i in m.imported.clone() {
            if let Some(f) = self.entry(&m.path, &i.spec) {
                m.links.insert(i.local, (f, i.name));
            }
        }
    }

    /// A package's file read as code: its module, its functions and types, as the library's.
    /// What it imports from the package's other files is bound to them, to be followed where
    /// they are read; none where its module is the program's own or already read.
    fn read(&mut self, file: &Path, modules: &mut IndexMap<String, Module>, store: &mut GraphBuilder) -> Option<String> {
        let id = self.holding(file)?.files.get(file)?.clone();
        if modules.contains_key(&id) || !self.read.insert(file.to_path_buf()) {
            return None;
        }
        let mut m = self.parse(file)?;
        let pkg = self.holding(file)?;
        let local: Vec<(String, String)> = m
            .bindings
            .iter()
            .chain(&m.dynamic)
            .filter_map(|(k, b)| if let Bound::Own(t) = b { Some((k.clone(), t.clone())) } else { None })
            .collect();
        for (k, t) in local {
            let (f, name) = match pkg.by_id.get(&t) {
                Some(f) => (f.clone(), None),
                None => match t.rsplit_once('.') {
                    Some((tm, n)) if tm != id => match pkg.by_id.get(tm) {
                        Some(f) => (f.clone(), Some(n.to_string())),
                        None => continue,
                    },
                    _ => continue,
                },
            };
            let ext = Bound::Ext(format!("ext:{t}"));
            if m.dynamic.contains_key(&k) {
                m.dynamic.insert(k.clone(), ext);
            } else {
                m.bindings.insert(k.clone(), ext);
            }
            m.links.insert(k, (f, name));
        }
        // a library's own names are its package's
        m.ext_pkgs.insert(pkg.name.clone());
        self.link(&mut m);
        let root = m.tree.root_node();
        store.files.insert(crate::tree::path_str(file), String::from_utf8_lossy(&m.src).to_string());
        store.add_node(Node {
            id: id.clone(),
            kind: NodeKind::Module,
            name: file.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string(),
            file: crate::tree::path_str(file),
            span: span(root),
            module: id.clone(),
            is_pkg: file.file_stem().and_then(|s| s.to_str()) == Some("index"),
            library: true,
            ..Default::default()
        });
        collect_wrapped(&mut m);
        collect_symbols(&mut m, store);
        modules.insert(id.clone(), m);
        Some(id)
    }

    /// Bind what `m` imports from a library's code to its definition, where Atlas read it as
    /// code: a name, or each member of a namespace the module uses.
    fn bind(&mut self, m: &mut Module) {
        for (local, (file, name)) in m.links.clone() {
            let names: Vec<(String, Vec<String>)> = match name {
                Some(n) => vec![(local.clone(), vec![n])],
                None => members(m, &local).into_iter().map(|x| (format!("{local}.{x}"), vec![x])).collect(),
            };
            for (written, names) in names {
                if let Some((f, id, 1)) = self.follow(&file, &names, 0) {
                    if self.read.contains(&f) {
                        m.read.insert(written, id);
                    }
                }
            }
        }
    }
}

/// The members of a namespace a module uses: `readConfig` of `fk.readConfig(..)`, `Client` of
/// `fk.Client`.
fn members(m: &Module, ns: &str) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for n in walk(m.tree.root_node(), &["member_expression", "nested_type_identifier", "nested_identifier"]) {
        let written = text(n, &m.src);
        let Some(member) =
            written.strip_prefix(ns).and_then(|r| r.strip_prefix('.')).map(|r| r.split('.').next().unwrap_or("").to_string())
        else {
            continue;
        };
        if !member.is_empty() && member.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$') && !out.contains(&member) {
            out.push(member);
        }
    }
    out
}

/// What a module's calls go into a library's code by: the file of the name each call, `new` and
/// rendered element is written with, and that name and the members after it.
fn targets(m: &Module) -> Vec<(PathBuf, Vec<String>)> {
    let mut out = vec![];
    for c in walk(m.tree.root_node(), &["call_expression", "new_expression", "jsx_opening_element", "jsx_self_closing_element"]) {
        let callee = match c.kind() {
            "call_expression" => function_of(c),
            "new_expression" => c.child_by_field_name("constructor"),
            _ => c.child_by_field_name("name"),
        };
        let parts = callee.map(|f| member_parts(f, &m.src)).unwrap_or_default();
        let Some((file, name)) = parts.first().and_then(|h| m.links.get(h)) else { continue };
        let names: Vec<String> = name.iter().cloned().chain(parts[1..].iter().cloned()).collect();
        if !names.is_empty() {
            out.push((file.clone(), names));
        }
    }
    out
}

/// Read the packages the program's code calls, `depth` rounds deep: the files that define what
/// its calls go into, then the files of the same packages that the last round's calls go into.
/// Read together, every name bound to a library's code goes to its definition where Atlas read it.
pub(super) fn read(repo: &Repo, modules: &mut IndexMap<String, Module>, store: &mut GraphBuilder, depth: usize) {
    if depth == 0 {
        return;
    }
    let mut lib = Libraries::new(repo.root());
    for m in modules.values_mut() {
        lib.link(m);
    }
    let mut round: Vec<String> = modules.keys().cloned().collect();
    let mut used: Option<HashSet<PathBuf>> = None;
    for _ in 0..depth {
        let (mut fresh, mut reached) = (vec![], HashSet::new());
        for id in &round {
            for (file, names) in targets(&modules[id]) {
                let Some((def, _, _)) = lib.follow(&file, &names, 0) else { continue };
                let Some(pkg) = lib.owner.get(&def).cloned() else { continue };
                if used.as_ref().map_or(false, |u| !u.contains(&pkg)) {
                    continue;
                }
                reached.insert(pkg);
                fresh.extend(lib.read(&def, modules, store));
            }
        }
        used.get_or_insert(reached);
        if fresh.is_empty() {
            break;
        }
        round = fresh;
    }
    for m in modules.values_mut() {
        lib.bind(m);
    }
    store.mark_library();
}
