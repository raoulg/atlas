//! A crate's own code, read as far as the program's calls go into it (`--libraries N`): the
//! modules of a crate the workspace depends on that the program's calls go into, then in each
//! further round the modules of the same crates that those modules' calls go into. What a library
//! calls in a module not read, or in another crate, stays a library's call, as the program's own
//! calls into a crate not read do.
//!
//! A crate is read where Cargo keeps it: a path dependency outside the repository, or the version
//! the workspace's `Cargo.lock` names under Cargo's registry sources. Nothing is fetched: a crate
//! not on disk stays a library's name. Not read: the language's own (`std`, `core`, `alloc`), a
//! crate the tables in `knowledge.rs` describe (the table is the knowledge), and a crate with
//! compiled code (one that links a native library, or holds C). A library's module is named by
//! its path as the code writes it (`serde_json.de`), and its code is the environment's: read from
//! the disk, never from the repository's listing.

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
use crate::knowledge::{self, Registry};
use crate::model::{Node, NodeKind};
use crate::places::Environment;
use crate::repo::{normal, Repo};
use crate::tree::{named_children, span, text, unquote, walk};

/// How many re-exports a path is followed through.
const DEPTH: u32 = 8;

impl Environment {
    /// Cargo's environment: the directories it keeps the source of fetched crates in, one per
    /// registry, below Cargo's home (`CARGO_HOME`, else `.cargo` in the user's home), with what decides
    /// them noted as asked.
    fn cargo() -> Environment {
        let mut asked: Vec<Check> = ["CARGO_HOME", "HOME", "USERPROFILE"].map(|v| Check::Var(v.to_string())).into();
        let user = || ["HOME", "USERPROFILE"].iter().find_map(std::env::var_os).map(|h| PathBuf::from(h).join(".cargo"));
        let home = std::env::var_os("CARGO_HOME").map(PathBuf::from).or_else(user);
        let Some(home) = home else { return Environment { checks: asked, ..Environment::default() } };
        let sources = knowledge::CARGO_SOURCES.iter().fold(home, |d, p| d.join(p));
        asked.push(Check::Names(sources.clone()));
        let mut registries: Vec<PathBuf> =
            std::fs::read_dir(sources).map(|r| r.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect()).unwrap_or_default();
        registries.sort();
        Environment { checks: asked, ..Environment::new(registries) }
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
        let file = normal(&package_dir.join(lib));
        (!holds && file.is_file()).then_some(file)
    }
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

/// The version of `package` that the lock gives the workspace's crate `locked`: the one its list
/// of dependencies names, else the only one locked.
fn version<'l>(locked: Option<&'l toml::Table>, packages: &[&'l toml::Table], package: &str) -> Option<&'l str> {
    let listed = locked.and_then(|l| l.get("dependencies")?.as_array()).into_iter().flatten().filter_map(|d| d.as_str());
    for d in listed {
        let mut words = d.split(' ');
        if words.next() == Some(package) {
            if let Some(v) = words.next() {
                return Some(v);
            }
            break;
        }
    }
    let mut found = packages.iter().filter(|p| p.get("name").and_then(|n| n.as_str()) == Some(package) && p.get("source").is_some());
    let only = found.next()?;
    found.next().is_none().then(|| only.get("version")?.as_str()).flatten()
}

/// The crates the workspace's crates depend on, by the name their code calls them by, where
/// their source is on disk: a path dependency outside the repository, or the version the lock
/// names in Cargo's registry sources. A crate of the repository is read as its own part. With
/// them, what was asked of the disk to find them: a path dependency's code, which changes without
/// a version, and whether each locked version is fetched, whose code is its version's.
fn dependencies(repo: &Repo, crates: &[Crate]) -> (HashMap<String, PathBuf>, Environment) {
    let lock: toml::Table = repo.text("Cargo.lock").and_then(|t| t.parse().ok()).unwrap_or_default();
    let packages: Vec<&toml::Table> =
        lock.get("package").and_then(|p| p.as_array()).into_iter().flatten().filter_map(|p| p.as_table()).collect();
    let root: toml::Table = repo.text("Cargo.toml").and_then(|t| t.parse().ok()).unwrap_or_default();
    let shared = root.get("workspace").and_then(|w| w.get("dependencies")).and_then(|d| d.as_table());
    let here = crate::repo::Repo::real(&repo.root());
    let mut env = Environment::cargo();
    let mut out = HashMap::new();
    for c in crates {
        let Some(manifest) = repo.text(c.dir.join("Cargo.toml")).and_then(|t| t.parse::<toml::Table>().ok()) else { continue };
        let locked =
            packages.iter().copied().find(|p| p.get("name").and_then(|n| n.as_str()) == Some(c.name.as_str()) && p.get("source").is_none());
        for table in dependency_tables(&manifest) {
            for (key, spec) in table {
                let ident = key.replace('-', "_");
                if out.contains_key(&ident) {
                    continue;
                }
                // a dependency the workspace declares for its crates is declared at its root
                let inherited = spec.get("workspace").and_then(|w| w.as_bool()) == Some(true);
                let (spec, base) = match shared.and_then(|s| s.get(key)).filter(|_| inherited) {
                    Some(s) => (s, repo.root()),
                    None => (spec, c.dir.as_path()),
                };
                let package = spec.get("package").and_then(|p| p.as_str()).unwrap_or(key);
                let dir = match spec.get("path").and_then(|p| p.as_str()) {
                    Some(p) => {
                        let d = normal(&base.join(p));
                        let inside = Repo::resolved(&d).map(|abs| abs.starts_with(&here));
                        if inside != Some(true) {
                            env.checks.push(Check::Tree(d.clone()));
                        }
                        if inside.unwrap_or(true) {
                            continue;
                        }
                        d
                    }
                    None => {
                        let Some(v) = version(locked, &packages, package) else { continue };
                        let Some(package_dir) = env.package(&format!("{package}-{v}"), "Cargo.toml") else { continue };
                        package_dir
                    }
                };
                out.insert(ident, dir);
            }
        }
    }
    (out, env)
}

/// What a reading of the workspace's crates takes from where Cargo keeps the crates they depend
/// on, as checks of how each stands.
pub(super) fn environment(repo: &Repo, crates: &[Crate]) -> Vec<Check> {
    dependencies(repo, crates).1.checks
}

/// The crates the workspace depends on, as read for their code.
struct Libraries {
    /// Where Cargo keeps them.
    env: Environment,
    deps: HashMap<String, PathBuf>,
    /// The root file of each crate Atlas reads, by its name in the code; none for one it does not.
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
    fn root(&mut self, ident: &str) -> Option<PathBuf> {
        if let Some(r) = self.roots.get(ident) {
            return r.clone();
        }
        let r = self.deps.get(ident).filter(|_| !effects::described(ident, Registry::Crates)).and_then(|d| self.env.crate_root(d));
        self.roots.insert(ident.to_string(), r.clone());
        r
    }

    /// A library's module from its file, before its symbols and imports are collected.
    fn module(&mut self, id: &str, file: &Path) -> Option<Module> {
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
                to.trim_start_matches("ext:").split('.').map(String::from).chain(from.iter().cloned()).collect()
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
        out.insert(name, candidates.into_iter().map(|c| normal(&c)).collect());
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
    let eval = Eval::new(modules, &store.nodes, crate_ids);
    let returned = returned_classes(store);
    let mut out = vec![];
    for n in store.nodes.values().filter(|n| matches!(n.kind, NodeKind::Function | NodeKind::Method) && probe.contains(&n.module)) {
        let Some(m) = modules.get(&n.module) else { continue };
        let Some(d) = def_node(m, n) else { continue };
        let Some(body) = d.child_by_field_name("body") else { continue };
        let b = Body::new(modules, store, crate_ids, &eval, &returned, m, n, d, body);
        out.extend(walk(body, &["call_expression"]).into_iter().filter_map(|c| b.library_target(c)));
    }
    out
}

/// Read the crates the program's code calls, `depth` rounds deep: the modules its calls go into
/// (with the modules their paths go through), then the modules of the same crates that the last
/// round's calls go into.
pub(super) fn read(
    repo: &Repo,
    crates: &[Crate],
    modules: &mut IndexMap<String, Module>,
    store: &mut GraphBuilder,
    crate_ids: &HashMap<String, String>,
    depth: usize,
) {
    if depth == 0 {
        return;
    }
    let (deps, env) = dependencies(repo, crates);
    if deps.is_empty() {
        return;
    }
    let mut lib = Libraries {
        env,
        deps,
        roots: HashMap::new(),
        files: HashMap::new(),
        parsed: HashMap::new(),
        followed: HashMap::new(),
        parser: new_parser().ok(),
        trees: HashMap::new(),
    };
    let mut probe: HashSet<String> = modules.keys().cloned().collect();
    let mut used: Option<HashSet<String>> = None;
    for _ in 0..depth {
        let mut targets = library_calls(modules, store, crate_ids, &probe);
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
        collect_impls(modules, store, crate_ids, &fresh);
        used.get_or_insert(reached);
        probe = defining.into_iter().filter(|d| fresh.contains(d)).collect();
        if probe.is_empty() {
            break;
        }
    }
    store.mark_library();
}
