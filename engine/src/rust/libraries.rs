//! A crate's own code, read as far as the program's calls go into it (`--libraries N`): the
//! modules of a crate the workspace depends on that the program's calls go into, then in each
//! further round the modules of the same crates that those modules' calls go into. What a library
//! calls in a module not read, or in another crate, stays a library's call, as the program's own
//! calls into a crate not read do.
//!
//! A crate is read where Cargo keeps it for the crate whose code calls it: a path dependency
//! outside the repository, or the version the workspace's `Cargo.lock` pins for that crate under
//! Cargo's registry sources. Nothing is fetched: a crate not on disk stays a library's name. Not
//! read: the language's own (`std`, `core`, `alloc`), a crate the tables in `knowledge.rs` describe
//! (the table is the knowledge), and a crate with compiled code (one that links a native library,
//! or holds C). A library's module is named by its path as the code writes it (`serde_json.de`),
//! behind the version where the workspace's crates read two (`notekit-0_2_0.save`), and its code is
//! the environment's: read from the disk, never from the repository's listing.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use tree_sitter::{Parser, Tree};

use super::body::Body;
use super::module::{attrs_of, def_node, definitions, module_doc, new_parser, type_parameters, Crate, Module};
use super::passes::{collect_impls, collect_symbols, collect_uses, returned_classes};
use super::paths::Eval;
use crate::builder::GraphBuilder;
use crate::cache::Check;
use crate::effects;
use crate::environment::Environment;
use crate::knowledge::{self, Registry};
use crate::model::{ext, Dependency, Node, NodeKind};
use crate::options::Reach;
use crate::repo::Repo;
use crate::tree::{named_children, span, text, unquote, walk};
use path_clean::clean;

/// How many re-exports a path is followed through.
const DEPTH: u32 = 8;

impl Environment {
    /// Cargo's environment for the workspace at `repo`: the directories it keeps the source of
    /// fetched crates in, one per registry, below Cargo's home (`CARGO_HOME`, else `.cargo` in the
    /// user's home), with what decides them noted as asked, and the versions the workspace's
    /// `Cargo.lock` pins.
    pub(crate) fn cargo(repo: &Repo) -> Environment {
        let pins = pins(repo);
        let mut asked: Vec<Check> = ["CARGO_HOME", "HOME", "USERPROFILE"].map(|v| Check::Var(v.to_string())).into();
        let user = || ["HOME", "USERPROFILE"].iter().find_map(std::env::var_os).map(|h| PathBuf::from(h).join(".cargo"));
        let home = std::env::var_os("CARGO_HOME").map(PathBuf::from).or_else(user);
        let Some(home) = home else { return Environment { checks: asked, pins, ..Environment::default() } };
        let sources = knowledge::CARGO_SOURCES.iter().fold(home, |d, p| d.join(p));
        asked.push(Check::Names(sources.clone()));
        let mut registries: Vec<PathBuf> =
            std::fs::read_dir(sources).map(|r| r.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect()).unwrap_or_default();
        registries.sort();
        Environment { checks: asked, pins, ..Environment::new(registries) }
    }

    /// The file of a fetched crate's root: its library target's, unless Atlas does not read the crate:
    /// one that links a native library, one that holds compiled code.
    fn crate_root(&self, package_dir: &Path) -> Option<PathBuf> {
        let manifest: toml::Table = self.manifest(package_dir, "Cargo.toml")?.parse().ok()?;
        if manifest.get("package").and_then(|p| p.get("links")).is_some() {
            return None;
        }
        let holds = self.compiled(package_dir, Registry::Crates);
        let lib = manifest.get("lib").and_then(|l| l.get("path")).and_then(|p| p.as_str()).unwrap_or("src/lib.rs");
        let file = clean(&package_dir.join(lib));
        (!holds && file.is_file()).then_some(file)
    }

    /// A fetched crate as the dependency it is: its directory, the version its manifest names, and
    /// the name the code calls it by.
    fn dependency(&self, package_dir: &Path, name: &str) -> Dependency {
        let manifest: Option<toml::Table> = self.manifest(package_dir, "Cargo.toml").and_then(|t| t.parse().ok());
        let version = manifest.as_ref().and_then(|m| m.get("package")?.get("version")?.as_str()).unwrap_or_default();
        Dependency { dir: crate::tree::path_str(package_dir), version: version.to_string(), name: name.to_string() }
    }
}

/// The versions the workspace's `Cargo.lock` pins: of each crate a workspace's crate depends on,
/// the one its list of dependencies names, and of a crate the lock holds one version of, that one.
fn pins(repo: &Repo) -> HashMap<(String, String), String> {
    let lock: toml::Table = repo.text("Cargo.lock").and_then(|t| t.parse().ok()).unwrap_or_default();
    let packages: Vec<&toml::Table> =
        lock.get("package").and_then(|p| p.as_array()).into_iter().flatten().filter_map(|p| p.as_table()).collect();
    let name = |p: &toml::Table| p.get("name").and_then(|n| n.as_str()).unwrap_or_default().to_string();
    let mut out = HashMap::new();
    let mut fetched: HashMap<String, Vec<String>> = HashMap::new();
    for p in &packages {
        match p.get("source") {
            Some(_) => fetched.entry(name(p)).or_default().push(p.get("version").and_then(|v| v.as_str()).unwrap_or_default().to_string()),
            // a crate of the workspace: the version of each dependency it lists by its version
            None => {
                for d in p.get("dependencies").and_then(|d| d.as_array()).into_iter().flatten().filter_map(|d| d.as_str()) {
                    if let [package, version, ..] = d.split(' ').collect::<Vec<_>>()[..] {
                        out.insert((name(p), package.to_string()), version.to_string());
                    }
                }
            }
        }
    }
    for (package, versions) in fetched {
        if let [only] = &versions[..] {
            out.insert((String::new(), package), only.clone());
        }
    }
    out
}

/// A manifest's tables of the crates its code may call: its dependencies, those of its tests,
/// and those of a platform.
fn dependency_tables(manifest: &toml::Table) -> Vec<&toml::Table> {
    let mut out: Vec<&toml::Table> = ["dependencies", "dev-dependencies"].iter().filter_map(|k| manifest.get(*k)?.as_table()).collect();
    for platform in manifest.get("target").and_then(|t| t.as_table()).into_iter().flat_map(|t| t.values()) {
        out.extend(["dependencies", "dev-dependencies"].iter().filter_map(|k| platform.get(*k)?.as_table()));
    }
    out
}

/// Where the code of the crate that the code calls `name` lies, called from the file `from` of the
/// workspace at `repo`: as its crate's manifest declares it (the crate nearest above `from`), a
/// path dependency outside the repository, or the version the lock pins for that crate where Cargo
/// keeps it (`env`). A crate of the repository is read as its own part, and a crate not on disk
/// stays a library's name. What was asked of the disk to find it is noted in `env`: a path
/// dependency's code, which changes without a version, and whether each pinned version is
/// fetched, whose code is its version's.
pub(crate) fn library_code(repo: &Repo, env: &mut Environment, from: &Path, name: &str) -> Option<PathBuf> {
    let dir = from.ancestors().skip(1).find(|a| repo.is_file(a.join("Cargo.toml")))?;
    let manifest: toml::Table = repo.text(dir.join("Cargo.toml"))?.parse().ok()?;
    let dependent = manifest.get("package").and_then(|p| p.get("name")).and_then(|n| n.as_str()).unwrap_or_default();
    let (key, spec) = dependency_tables(&manifest).into_iter().flatten().find(|(key, _)| key.replace('-', "_") == name)?;
    // a dependency the workspace declares for its crates is declared at its root
    let root: Option<toml::Table> =
        spec.get("workspace").and_then(|w| w.as_bool()).filter(|w| *w).and_then(|_| repo.text("Cargo.toml")).and_then(|t| t.parse().ok());
    let shared = root.as_ref().and_then(|r| r.get("workspace")?.get("dependencies")?.get(key.as_str()));
    let (spec, base) = match shared {
        Some(s) => (s, repo.root()),
        None => (spec, dir),
    };
    let package = spec.get("package").and_then(|p| p.as_str()).unwrap_or(key);
    match spec.get("path").and_then(|p| p.as_str()) {
        Some(p) => {
            let d = clean(&base.join(p));
            let inside = Repo::resolved(&d).map(|abs| abs.starts_with(Repo::real(&repo.root())));
            if inside != Some(true) {
                env.checks.push(Check::Tree(d.clone()));
            }
            (inside == Some(false)).then_some(d)
        }
        None => {
            let version = env.pinned(dependent, package)?.to_string();
            env.package(&format!("{package}-{version}"), "Cargo.toml")
        }
    }
}

/// The crates the workspace's crates depend on, as each crate's code calls them, where their
/// source is on disk (`library_code`); with what was asked of the disk to find them.
pub(super) struct Dependencies {
    env: Environment,
    /// Each crate's dependencies, by its name: the library each name its code calls one by is.
    of: HashMap<String, HashMap<String, String>>,
    /// Each library by its root: its directory, and the name the code calls it by.
    dirs: HashMap<String, (PathBuf, String)>,
}

impl Dependencies {
    /// The names the code of the crate `krate` calls a library by under another name than its
    /// root (`notekit` for `notekit-0_2_0`).
    pub(super) fn externs(&self, krate: &str) -> HashMap<String, String> {
        self.of.get(krate).into_iter().flatten().filter(|(ident, root)| ident != root).map(|(i, r)| (i.clone(), r.clone())).collect()
    }
}

/// The crates the workspace's crates depend on, each crate's by the name its code calls them by.
/// A directory is one library, under the name the code calls it by while that name is one
/// directory throughout the workspace; a name two crates read from two directories (two versions
/// the lock pins) is a library per directory, each named after its version
/// (`notekit-0_2_0`), or where the versions do not tell them apart after the first crate that
/// reads it (`orders/notekit`).
pub(super) fn dependencies(repo: &Repo, crates: &[Crate]) -> Dependencies {
    let mut env = Environment::cargo(repo);
    let mut found: Vec<(String, String, PathBuf)> = vec![];
    for c in crates {
        let Some(manifest) = repo.text(c.dir.join("Cargo.toml")).and_then(|t| t.parse::<toml::Table>().ok()) else { continue };
        let keys: Vec<String> = dependency_tables(&manifest).into_iter().flat_map(|t| t.keys().map(|k| k.replace('-', "_"))).collect();
        for ident in keys {
            if found.iter().any(|(k, i, _)| *k == c.name && *i == ident) {
                continue;
            }
            if let Some(dir) = library_code(repo, &mut env, &c.dir.join("Cargo.toml"), &ident) {
                found.push((c.name.clone(), ident, dir));
            }
        }
    }
    // each name's directories, in the order the crates read them, with the first crate reading each
    let mut by_name: Vec<(String, Vec<(PathBuf, String)>)> = vec![];
    for (krate, ident, dir) in &found {
        let at = match by_name.iter().position(|(i, _)| i == ident) {
            Some(p) => p,
            None => {
                by_name.push((ident.clone(), vec![]));
                by_name.len() - 1
            }
        };
        if !by_name[at].1.iter().any(|(d, _)| d == dir) {
            by_name[at].1.push((dir.clone(), krate.clone()));
        }
    }
    let mut roots: HashMap<(String, PathBuf), String> = HashMap::new();
    let mut dirs = HashMap::new();
    for (ident, at) in by_name {
        let versions: Vec<String> = at.iter().map(|(d, _)| env.dependency(d, &ident).version).collect();
        let by_version = versions.iter().all(|v| !v.is_empty()) && versions.iter().collect::<HashSet<_>>().len() == at.len();
        for ((dir, first), version) in at.iter().zip(&versions) {
            let root = match () {
                _ if at.len() == 1 => ident.clone(),
                _ if by_version => format!("{ident}-{}", version.replace('.', "_")),
                _ => format!("{first}/{ident}"),
            };
            roots.insert((ident.clone(), dir.clone()), root.clone());
            dirs.insert(root, (dir.clone(), ident.clone()));
        }
    }
    let mut of: HashMap<String, HashMap<String, String>> = HashMap::new();
    for (krate, ident, dir) in found {
        let root = roots[&(ident.clone(), dir)].clone();
        of.entry(krate).or_default().insert(ident, root);
    }
    Dependencies { env, of, dirs }
}

/// What a reading of the workspace's crates takes from where Cargo keeps the crates they depend
/// on, as checks of how each stands.
pub(super) fn environment(repo: &Repo, crates: &[Crate]) -> Vec<Check> {
    dependencies(repo, crates).env.checks
}

/// The crates the workspace depends on, as read for their code.
struct Libraries {
    /// Where Cargo keeps them.
    env: Environment,
    /// Each library by its root: its directory, and the name the code calls it by.
    dirs: HashMap<String, (PathBuf, String)>,
    /// The root file of each crate Atlas reads, by its root; none for one it does not.
    roots: HashMap<String, Option<PathBuf>>,
    /// Each module's file, as found from the crate's root.
    files: HashMap<String, PathBuf>,
    /// Modules parsed to follow a path through: what they declare, import and define.
    parsed: HashMap<String, Option<Module>>,
    /// The paths followed, with the modules each goes through.
    followed: HashMap<Vec<String>, Option<Vec<(String, PathBuf)>>>,
    parser: Option<Parser>,
    /// Each file read, with its tree: read once, whether to follow a path through it or as code.
    trees: HashMap<PathBuf, (Vec<u8>, Tree)>,
}

impl Libraries {
    /// The file of a crate's root: its library target's, unless Atlas does not read the crate:
    /// one the tables describe, one that links a native library, one that holds compiled code.
    fn root(&mut self, root: &str) -> Option<PathBuf> {
        if let Some(r) = self.roots.get(root) {
            return r.clone();
        }
        let r =
            self.dirs.get(root).filter(|(_, name)| !effects::described(name, Registry::Crates)).and_then(|(d, _)| self.env.crate_root(d));
        self.roots.insert(root.to_string(), r.clone());
        r
    }

    /// A library's module from its file, before its symbols and imports are collected.
    fn module(&mut self, id: &str, file: &Path) -> Option<Module> {
        let _parse = crate::clock::of("parse", id.split('.').next().unwrap_or(id), true);
        if !self.trees.contains_key(file) {
            let src = self.env.source(file)?;
            let tree = self.parser.as_mut()?.parse(&src, None)?;
            self.trees.insert(file.to_path_buf(), (src, tree));
        }
        let (src, tree) = self.trees.get(file).cloned()?;
        let krate = id.split('.').next().unwrap_or(id).to_string();
        let is_root = id == krate;
        let parent = if is_root { id.to_string() } else { id.rsplit_once('.').map_or(id, |x| x.0).to_string() };
        let (defs, generics) = (definitions(&tree, &src), type_parameters(&tree, &src));
        Some(Module {
            id: id.to_string(),
            krate,
            manifest_dir: self.env.manifest_dir(file, "Cargo.toml").as_deref().map(crate::tree::path_str).unwrap_or_default(),
            parent,
            path: file.to_path_buf(),
            src,
            tree,
            bindings: HashMap::new(),
            wildcards: vec![],
            globs: vec![],
            generics,
            symbols: HashMap::new(),
            is_root,
            bin: None,
            defs,
            library: true,
            externs: HashMap::new(),
        })
    }

    /// What a module's names are bound to, with the modules it declares as its own.
    fn collect(m: &mut Module, store: &mut GraphBuilder, crate_ids: &HashMap<String, String>) {
        collect_symbols(m, store);
        let declared: HashSet<String> = children(m).into_keys().map(|c| format!("{}.{c}", m.id)).collect();
        collect_uses(m, crate_ids, &declared, store);
    }

    /// A module parsed to follow a path through.
    fn parsed(&mut self, id: &str, crate_ids: &HashMap<String, String>) -> Option<&Module> {
        if !self.parsed.contains_key(id) {
            let _reach = crate::clock::of("reach", id.split('.').next().unwrap_or(id), true);
            let file = self.files.get(id).cloned();
            let m = file.and_then(|f| self.module(id, &f)).map(|mut m| {
                Self::collect(&mut m, &mut GraphBuilder::default(), crate_ids);
                m
            });
            self.parsed.insert(id.to_string(), m);
        }
        self.parsed.get(id)?.as_ref()
    }

    /// The modules a library's path (`serde_json.from_str`) goes through to where it is defined,
    /// with their files: the crate's root, the modules the path names, those it is re-exported
    /// from. None where Atlas does not find it.
    fn follow(&mut self, path: &[String], crate_ids: &HashMap<String, String>, depth: u32) -> Option<Vec<(String, PathBuf)>> {
        if let Some(f) = self.followed.get(path) {
            return f.clone();
        }
        // a cycle of re-exports ends where it started
        self.followed.insert(path.to_vec(), None);
        let found = self.follow_in(path, crate_ids, depth);
        self.followed.insert(path.to_vec(), found.clone());
        found
    }

    fn follow_in(&mut self, path: &[String], crate_ids: &HashMap<String, String>, depth: u32) -> Option<Vec<(String, PathBuf)>> {
        let ident = path.first()?;
        let root = self.root(ident)?;
        self.files.insert(ident.clone(), root.clone());
        let mut chain = vec![(ident.clone(), root)];
        let mut cur = ident.clone();
        for (i, seg) in path.iter().enumerate().skip(1) {
            let m = self.parsed(&cur, crate_ids)?;
            let onward = |to: &str, from: &[String]| -> Vec<String> {
                ext::name(to).split('.').map(String::from).chain(from.iter().cloned()).collect()
            };
            let (candidates, defines) = (children(m).remove(seg), m.symbols.contains_key(seg));
            let next: Vec<Vec<String>> = match m.bindings.get(seg) {
                Some(b) => vec![onward(b, &path[i + 1..])],
                None => m.globs.iter().map(|g| onward(g, &path[i..])).collect(),
            };
            let child = candidates.map(|c| self.env.first_file(c));
            match child {
                Some(Some(f)) => {
                    let id = format!("{cur}.{seg}");
                    self.files.insert(id.clone(), f.clone());
                    chain.push((id.clone(), f));
                    cur = id;
                    continue;
                }
                // a module written inline is read with the module around it, its names among that
                // module's, where they may clash: a path through it is not followed
                Some(None) => return None,
                None => {}
            }
            if defines {
                return Some(chain);
            }
            if depth >= DEPTH {
                return None;
            }
            let found = next.iter().find_map(|n| self.follow(n, crate_ids, depth + 1))?;
            chain.extend(found);
            return Some(chain);
        }
        Some(chain)
    }

    /// A library's module read as code: its functions and types, as the library's; none where it
    /// is read already.
    fn read(
        &mut self,
        id: &str,
        file: &Path,
        modules: &mut IndexMap<String, Module>,
        store: &mut GraphBuilder,
        crate_ids: &HashMap<String, String>,
    ) -> Option<String> {
        if modules.contains_key(id) {
            return None;
        }
        let _items = crate::clock::of("items", id.split('.').next().unwrap_or(id), true);
        let mut m = self.module(id, file)?;
        let root = m.tree.root_node();
        store.files.insert(crate::tree::path_str(file), String::from_utf8_lossy(&m.src).to_string());
        store.add_node(Node {
            id: id.to_string(),
            kind: NodeKind::Module,
            name: id.rsplit('.').next().unwrap_or(id).to_string(),
            file: crate::tree::path_str(file),
            span: span(root),
            module: id.to_string(),
            doc: module_doc(root, &m.src),
            is_pkg: m.is_root,
            library: true,
            ..Default::default()
        });
        Self::collect(&mut m, store, crate_ids);
        store.bindings.insert(m.id.clone(), m.bindings.clone());
        modules.insert(id.to_string(), m);
        Some(id.to_string())
    }
}

/// The modules a module declares, by name: the file of each one declared `mod x;` (beside a
/// crate's root or a `mod.rs`, else in the directory named after the file, or where `#[path]`
/// says), none for one written inline.
fn children(m: &Module) -> HashMap<String, Vec<PathBuf>> {
    let dir = m.path.parent().unwrap_or(Path::new(""));
    let stem = m.path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let base = if m.is_root || stem == "mod" { dir.to_path_buf() } else { dir.join(stem) };
    let mut out = HashMap::new();
    for item in named_children(m.tree.root_node()).into_iter().filter(|x| x.kind() == "mod_item") {
        let Some(name) = item.child_by_field_name("name").map(|n| text(n, &m.src).to_string()) else { continue };
        if item.child_by_field_name("body").is_some() {
            out.insert(name, vec![]);
            continue;
        }
        let at =
            attrs_of(item, &m.src).into_iter().find_map(|a| Some(unquote(a.strip_prefix("path")?.trim_start().strip_prefix('=')?.trim())));
        let candidates = match at {
            Some(p) => vec![dir.join(p)],
            None => vec![base.join(format!("{name}.rs")), base.join(&name).join("mod.rs")],
        };
        out.insert(name, candidates.into_iter().map(|c| clean(&c)).collect());
    }
    out
}

/// The library paths the calls of the functions in `probe` go into.
fn library_calls(
    modules: &IndexMap<String, Module>,
    store: &GraphBuilder,
    crate_ids: &HashMap<String, String>,
    probe: &HashSet<String>,
) -> Vec<Vec<String>> {
    let copied = crate::clock::stage("reach");
    let eval = Eval::new(modules, &store.nodes, crate_ids);
    let returned = returned_classes(store);
    drop(copied);
    let mut out = vec![];
    for n in store.nodes.values().filter(|n| matches!(n.kind, NodeKind::Function | NodeKind::Method) && probe.contains(&n.module)) {
        let Some(m) = modules.get(&n.module) else { continue };
        let _reach = crate::clock::of("reach", &m.krate, n.library);
        let Some(d) = def_node(m, n) else { continue };
        let Some(body) = d.child_by_field_name("body") else { continue };
        crate::tree::indexed(d, || {
            let b = Body::new(modules, store, crate_ids, &eval, &returned, m, n, d, body);
            out.extend(walk(body, &["call_expression"]).into_iter().filter_map(|c| b.library_target(c)));
        });
    }
    out
}

/// Read the crates the program's code calls, `depth` rounds deep: the modules its calls go into
/// (with the modules their paths go through), then the modules of the same crates that the last
/// round's calls go into. What the repository's other parts reach of a crate read from the same
/// directory (`reach`) is reached in the first round, as the program's own calls are.
pub(super) fn read(
    deps: Dependencies,
    reach: &Reach,
    modules: &mut IndexMap<String, Module>,
    store: &mut GraphBuilder,
    crate_ids: &HashMap<String, String>,
    depth: usize,
) {
    if depth == 0 || deps.dirs.is_empty() {
        return;
    }
    let mut lib = Libraries {
        env: deps.env,
        dirs: deps.dirs,
        roots: HashMap::new(),
        files: HashMap::new(),
        parsed: HashMap::new(),
        followed: HashMap::new(),
        parser: new_parser().ok(),
        trees: HashMap::new(),
    };
    // what the other parts reach, by the paths that name it here
    let mut entries: Vec<Vec<String>> = vec![];
    for (dir, reached) in reach {
        let Some(root) = lib.dirs.iter().find(|(_, (d, _))| crate::tree::path_str(d) == *dir).map(|(r, _)| r.clone()) else { continue };
        entries.extend(
            reached
                .iter()
                .map(|e| std::iter::once(root.clone()).chain(e.id.split('.').filter(|s| !s.is_empty()).map(String::from)).collect()),
        );
    }
    let mut probe: HashSet<String> = modules.keys().cloned().collect();
    let mut used: Option<HashSet<String>> = None;
    for _ in 0..depth {
        let mut targets = library_calls(modules, store, crate_ids, &probe);
        targets.append(&mut entries);
        targets.sort();
        targets.dedup();
        let (mut fresh, mut defining, mut reached) = (vec![], HashSet::new(), HashSet::new());
        for t in targets {
            let Some(chain) = lib.follow(&t, crate_ids, 0) else { continue };
            let Some((def, _)) = chain.last().cloned() else { continue };
            let krate = def.split('.').next().unwrap_or("").to_string();
            if used.as_ref().map_or(false, |u| !u.contains(&krate)) {
                continue;
            }
            reached.insert(krate);
            for (id, file) in chain {
                fresh.extend(lib.read(&id, &file, modules, store, crate_ids));
            }
            defining.insert(def);
        }
        let items = crate::clock::stage("items");
        collect_impls(modules, store, crate_ids, &fresh);
        drop(items);
        used.get_or_insert(reached);
        probe = defining.into_iter().filter(|d| fresh.contains(d)).collect();
        if probe.is_empty() {
            break;
        }
    }
    // each crate read, as the dependency it is
    for (root, file) in &lib.roots {
        if let (Some(_), Some((dir, name))) = (file, lib.dirs.get(root)) {
            store.libraries.insert(root.clone(), lib.env.dependency(dir, name));
        }
    }
    let _items = crate::clock::stage("items");
    store.mark_library();
}
