//! A repository of several parts: every manifest roots one (a crate, a Python package, an
//! app), read in its own language. Together they are one graph, in which every node knows its
//! part, and a route one part serves joins it to the parts that name the route.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::cache::{Check, Written};
use crate::knowledge;
use crate::model::{Edge, EdgeKind, Graph, Lang, Node, NodeKind, Part};
use crate::options::Options;
use crate::paths::{served, serving, Served};
use crate::repo::Repo;

/// The directories whose manifests root a part: the repository itself, and every manifest
/// below it, at any depth, that no part around it reads as its own (a workspace reads its
/// members, a project its packages).
pub fn roots(repo: &Repo, opts: &Options) -> Vec<(PathBuf, Lang)> {
    let mut out: Vec<(PathBuf, Lang)> = vec![];
    let rel = |d: &Path| d.strip_prefix(repo.root()).unwrap_or(d).to_path_buf();
    // no part is under what a tool made, or under tests and the data they read
    let skipped = |d: &Path| {
        rel(d).components().any(|c| {
            let n = c.as_os_str().to_str().unwrap_or("");
            n.starts_with('.') || knowledge::made_dir(n) || knowledge::TEST_DIRS.contains(&n)
        })
    };
    // a directory comes after the directories around it
    for d in repo.dirs_under("").into_iter().filter(|d| !skipped(d)) {
        for l in crate::language::languages() {
            if !l.manifests().iter().any(|m| repo.is_file(d.join(m))) {
                continue;
            }
            let read = out.iter().any(|(o, lang)| *lang == l.lang() && d.starts_with(o) && l.reads(&repo.under(&rel(o)), &d));
            if !read {
                out.push((d.clone(), l.lang()));
            }
        }
    }
    if out.is_empty() {
        if let Some(lang) = crate::language::detect(repo, &Options { package: None, ..opts.clone() }) {
            out.push((repo.root().to_path_buf(), lang.lang()));
        }
    }
    out
}

/// The graph of a repository as read: parsed in its language, with the routes its Makefile adds
/// and its parts, kept to what the program reaches.
pub fn graph_of(repo: &Repo, opts: &Options) -> Result<Graph> {
    let lang = crate::language::of(repo, opts)?;
    let mut g = lang.parse(repo, opts)?;
    crate::entries::read_makefile(repo, &mut g);
    assign(&mut g, lang.lang());
    Ok(crate::view::View::apply(&crate::view::reached(&g, opts.libraries), &g))
}

/// Each part's own graph, read in its language: what `atlas-code check` asks, part by part. Each
/// repository is read once; a part is a view of its reading.
pub fn analyze_parts(repos: &[PathBuf], opts: &Options) -> Result<Vec<Graph>> {
    for _ in 0..3 {
        if let Some(graphs) = read_current_parts(repos, opts)? {
            return Ok(graphs);
        }
    }
    anyhow::bail!("repository changed while Atlas was reading it")
}

/// A complete reading of the listed repository and the source files its parts consume.
fn read_current_parts(repos: &[PathBuf], opts: &Options) -> Result<Option<Vec<Graph>>> {
    let read: Vec<Repo> = repos.iter().map(|r| crate::language::read(r, opts)).collect::<Result<_>>()?;
    // each part with where it lies: its directory in its repository, behind the repository's name
    // when several are read
    let found: Vec<(Repo, PathBuf)> = read
        .iter()
        .flat_map(|r| {
            let name = if read.len() > 1 { r.name().unwrap_or_default() } else { String::new() };
            found(r, opts).into_iter().map(move |p| {
                let at = Path::new(&name).join(p.at());
                (p, at)
            })
        })
        .collect();
    if found.is_empty() {
        anyhow::bail!(
            "no part found in {} ({})",
            repos.iter().map(|r| r.display().to_string()).collect::<Vec<_>>().join(", "),
            crate::language::looked_for()
        );
    }
    let opts = Options { package: None, ..opts.clone() };
    let mut graphs = vec![];
    let mut witnesses = vec![];
    for (part, at) in &found {
        match indexed_graph(part, &opts) {
            // a manifest without code of its own (tooling, a docs site, a workspace's root) is not a part
            Ok((g, witness)) if g.nodes.is_empty() && found.len() > 1 => {
                witnesses.push((part, witness));
                eprintln!("skipped {}: no code of its own", part.root().display());
            }
            Ok((g, witness)) => {
                witnesses.push((part, witness));
                graphs.push((g, at.clone()));
            }
            Err(e) if found.len() > 1 => eprintln!("skipped {}: {e}", part.root().display()),
            Err(e) => return Err(e),
        }
    }
    if graphs.is_empty() {
        anyhow::bail!("no code found in {}", repos.iter().map(|r| r.display().to_string()).collect::<Vec<_>>().join(", "));
    }
    let current: Vec<Repo> = repos.iter().map(|r| crate::language::read(r, &opts)).collect::<Result<_>>()?;
    let stable =
        read.iter().zip(&current).all(|(before, after)| before.listed_files() == after.listed_files() && before.stamps() == after.stamps())
            && witnesses.iter().all(|(part, witness)| witness.current(part));
    Ok(stable.then(|| {
        let mut graphs = apart(graphs);
        decided(&read, &mut graphs);
        graphs
    }))
}

/// The `.atlasignore` at the top of each repository read, among the files of every part: the
/// decisions it holds are the author's, whichever part the code they answer lies in.
fn decided(read: &[Repo], graphs: &mut [Graph]) {
    for r in read {
        let Some((file, text)) = r.hidden(crate::aside::FILE) else { continue };
        for g in graphs.iter_mut() {
            g.files.insert(crate::tree::path_str(&file), text.clone());
        }
    }
}

/// One part's complete analysis and the inputs that make it current.
#[derive(serde::Serialize, serde::Deserialize)]
struct Snapshot {
    listed: BTreeSet<PathBuf>,
    read: BTreeMap<PathBuf, String>,
    environment: Vec<(Check, String)>,
    written: Written,
    graph: Graph,
}

struct Witness {
    read: BTreeMap<PathBuf, String>,
    environment: Vec<(Check, String)>,
}

impl Witness {
    fn current(&self, part: &Repo) -> bool {
        self.read.iter().all(|(path, was)| part.listed_bytes(path).is_some_and(|bytes| crate::cache::digest(&bytes) == *was))
            && crate::cache::holds(&self.environment)
    }
}

impl Snapshot {
    fn current(&self, part: &Repo) -> bool {
        self.listed == part.listed_files() && self.witness().current(part)
    }

    fn witness(&self) -> Witness {
        Witness { read: self.read.clone(), environment: self.environment.clone() }
    }

    fn graph(mut self) -> Option<Graph> {
        self.written.apply(std::slice::from_mut(&mut self.graph)).then_some(self.graph)
    }
}

/// A part from the worktree index where its listed files, read contents and language environment
/// stand as they did; otherwise a fresh reading kept as a complete entry.
fn indexed_graph(part: &Repo, opts: &Options) -> Result<(Graph, Witness)> {
    let Some(cache) = opts.cache.as_ref().and_then(|cache| cache.workspace(part.root())) else {
        let language = crate::language::of(part, opts)?;
        let environment = crate::cache::witness(language.environment(part, opts));
        let graph = graph_of(part, opts)?;
        let read = part.read_contents().into_iter().map(|(path, bytes)| (path, crate::cache::digest(&bytes))).collect();
        return Ok((graph, Witness { read, environment }));
    };
    let key = format!(
        "{}\n{}\n{}\n{}\n{:?}\n{:?}",
        crate::repo::Repo::real(&part.root()).display(),
        part.language().map_or("", Lang::name),
        part.asked().map_or_else(String::new, |p| p.display().to_string()),
        opts.libraries,
        opts.named,
        opts.package
    );
    if let Some((graph, environment)) =
        cache.get("part", &key).and_then(|bytes| serde_json::from_slice::<Snapshot>(&bytes).ok()).and_then(|snapshot| {
            snapshot.current(part).then(|| {
                let witness = snapshot.witness();
                snapshot.graph().map(|graph| (graph, witness))
            })?
        })
    {
        return Ok((graph, environment));
    }
    let language = crate::language::of(part, opts)?;
    let environment = crate::cache::witness(language.environment(part, opts));
    let graph = graph_of(part, opts)?;
    let snapshot = Snapshot {
        listed: part.listed_files(),
        read: part.read_contents().into_iter().map(|(path, bytes)| (path, crate::cache::digest(&bytes))).collect(),
        environment: environment.clone(),
        written: Written::of(std::slice::from_ref(&graph)),
        graph: graph.clone(),
    };
    if let Ok(bytes) = serde_json::to_vec(&snapshot) {
        cache.put("part", &key, &bytes);
    }
    let witness = snapshot.witness();
    Ok((graph, witness))
}

/// The parts of the repository as read: each root's, read in the language of its manifest, and for
/// a package named on its own the project it is in, asked for that package, as one part.
fn found(r: &Repo, opts: &Options) -> Vec<Repo> {
    match r.asked() {
        Some(_) => vec![r.clone()],
        None => roots(r, opts).into_iter().map(|(d, l)| r.under(d.strip_prefix(r.root()).unwrap_or(&d)).read_as(l)).collect(),
    }
}

/// Parts that hold a top-level name alike (two projects whose package is `app`, the scripts beside
/// each, two Makefiles) hold it under where each part lies, `orders/app` and `billing/app`, so that
/// no two parts share an id: the part's directory, behind the repository's name when several are
/// read, and before the part's language when another part lies in the same directory. A name one
/// part holds keeps it, and so does the part at the root of the repository, which lies nowhere below
/// it. A Makefile two parts in one directory read is one Makefile, held once.
fn apart(parts: Vec<(Graph, PathBuf)>) -> Vec<Graph> {
    let head = |id: &str| id.split('.').next().unwrap_or("").to_string();
    // the top-level names of a part's own code, no library's (which the parts share), and of its
    // Makefile: the file and its targets
    let held: Vec<(BTreeSet<String>, BTreeSet<String>)> = parts
        .iter()
        .map(|(g, _)| {
            let code = g.nodes.iter().filter(|n| !n.library && n.kind != NodeKind::External).map(|n| head(&n.id)).collect();
            (code, g.make.iter().flat_map(|t| [t.module.clone(), t.id.clone()]).collect())
        })
        .collect();
    // who holds each name: a part its code's, the directory its Makefile lies in the Makefile's
    let mut holders: HashMap<&str, HashSet<String>> = HashMap::new();
    for (i, ((g, _), (code, make))) in parts.iter().zip(&held).enumerate() {
        for h in code {
            holders.entry(h).or_default().insert(format!("part {i}"));
        }
        for h in make {
            holders.entry(h).or_default().insert(format!("Makefile in {}", g.project_root));
        }
    }
    let dirs: Vec<PathBuf> = parts.iter().map(|(_, at)| at.clone()).collect();
    parts
        .into_iter()
        .zip(&held)
        .map(|((mut g, at), (code, make))| {
            let shared: HashSet<&str> = code.iter().chain(make).map(String::as_str).filter(|h| holders[h].len() > 1).collect();
            // a dot in a directory's name is no package boundary
            let dir: Vec<String> = at.components().map(|c| c.as_os_str().to_string_lossy().replace('.', "_")).collect();
            // the code of two parts in one directory is told apart by its language; their Makefile is the directory's
            let place: Vec<String> = if dirs.iter().filter(|d| **d == at).count() > 1 {
                dir.iter().cloned().chain(g.language.iter().map(|l| l.name().into())).collect()
            } else {
                dir.clone()
            };
            let (dir, place) = (dir.join("/"), place.join("/"));
            let under = |p: &str, id: &str| if p.is_empty() { id.to_string() } else { format!("{p}/{id}") };
            g.rename(&|id| match head(id) {
                h if !shared.contains(h.as_str()) => id.to_string(),
                h if make.contains(&h) => under(&dir, id),
                _ => under(&place, id),
            });
            g
        })
        .collect()
}

/// What reading the repository at `dir` takes from outside it, part by part: where the code of
/// the libraries each part reads lies, as checks of how each place stands.
pub fn environment(dir: &Path, opts: &Options) -> Result<Vec<Check>> {
    let read = crate::language::read(dir, opts)?;
    let parts = found(&read, opts);
    let opts = Options { package: None, ..opts.clone() };
    Ok(parts.iter().filter_map(|part| Some(crate::language::detect(part, &opts)?.environment(part, &opts))).flatten().collect())
}

/// The repositories as one graph: their parts side by side, joined where one names a route
/// another serves.
pub fn analyze_repo(repos: &[PathBuf], opts: &Options) -> Result<Graph> {
    Ok(merged(repos, analyze_parts(repos, opts)?))
}

/// The parts of the repositories as one graph, named after them, joined where one part requests
/// another's routes.
pub fn merged(repos: &[PathBuf], graphs: Vec<Graph>) -> Graph {
    let name = repos
        .iter()
        .map(|r| Repo::resolved(r).and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string())).unwrap_or_else(|| "repo".into()))
        .collect::<Vec<_>>()
        .join("+");
    joined(&name, graphs)
}

/// Parts as one graph named `name` (one part is its own graph), joined where one part requests
/// another's routes.
pub fn joined(name: &str, graphs: Vec<Graph>) -> Graph {
    let mut g = if graphs.len() == 1 { graphs.into_iter().next().unwrap() } else { merge(name, graphs) };
    join(&mut g);
    g
}

/// A graph's parts and every node's part: the top-level packages it holds (a crate of a
/// workspace, a Python package, an app); what sits beside them (scripts, notebooks) belongs
/// to the first.
pub fn assign(g: &mut Graph, language: Lang) {
    // a library read for its types is the environment's, not a part of the repository
    let mut ids: Vec<String> = g
        .nodes
        .iter()
        .filter(|n| n.kind == NodeKind::Module && n.is_pkg && !n.library && !n.id.contains('.'))
        .map(|n| n.id.clone())
        .collect();
    if ids.is_empty() {
        ids.push(g.root.clone());
    }
    // the package named after the graph comes first: scripts and notebooks are routes into it
    ids.sort_by_key(|id| id != &g.root);
    let (leaves, structural) = (crate::language::leaves(language), crate::language::structural(language));
    g.parts = ids.iter().map(|id| Part { id: id.clone(), language, dir: g.project_root.clone(), leaves, structural }).collect();
    let known: HashSet<&String> = ids.iter().collect();
    for n in &mut g.nodes {
        let head = n.id.split('.').next().unwrap_or("").to_string();
        n.part = if known.contains(&head) { head } else { ids[0].clone() };
    }
    // a Makefile beside the packages is a route into the first, as a script is
    for t in &mut g.make {
        t.part = ids[0].clone();
    }
}

/// Graphs side by side: their nodes, edges, files and routes, their languages, and the calls they
/// did not follow, counted by name over all of them. No graphs are the empty graph, which has no
/// language and no project root.
pub fn merge(name: &str, graphs: Vec<Graph>) -> Graph {
    let mut out = Graph {
        root: name.to_string(),
        language: graphs.iter().flat_map(|g| g.language.iter()).collect(),
        project_root: graphs.first().map(|g| g.project_root.clone()).unwrap_or_default(),
        ..Default::default()
    };
    let mut made = HashSet::new();
    let mut unresolved: HashMap<String, u32> = HashMap::new();
    for g in graphs {
        out.parts.extend(g.parts);
        out.nodes.extend(g.nodes);
        out.edges.extend(g.edges);
        for (file, text) in g.files {
            // a library's file two parts read keeps the lines either shows
            match out.files.get_mut(&file) {
                Some(had) if *had != text => *had = overlay(had, &text),
                Some(_) => {}
                None => {
                    out.files.insert(file, text);
                }
            }
        }
        out.make.extend(g.make.into_iter().filter(|t| made.insert(t.id.clone())));
        out.stats.modules += g.stats.modules;
        out.stats.unresolved_calls_total += g.stats.unresolved_calls_total;
        out.stats.unresolved_calls += g.stats.unresolved_calls;
        for (call, n) in g.stats.unresolved_calls_top {
            *unresolved.entry(call).or_default() += n;
        }
    }
    out.stats.unresolved_calls_top = ranked(unresolved);
    out.recount();
    out
}

/// Two texts of one file, each with blank lines where it shows nothing: line by line the first's
/// where it shows one, else the second's, as long as the longer.
fn overlay(had: &str, text: &str) -> String {
    let (mut a, mut b) = (had.split('\n'), text.split('\n'));
    let mut lines = vec![];
    loop {
        let line = match (a.next(), b.next()) {
            (None, None) => break,
            (Some(x), Some(y)) if x.is_empty() => y,
            (Some(x), _) => x,
            (None, Some(y)) => y,
        };
        lines.push(line);
    }
    lines.join("\n")
}

/// Counts of names, the most first and equal counts by name: the order a graph keeps the calls it
/// did not follow in.
fn ranked(counts: HashMap<String, u32>) -> Vec<(String, u32)> {
    let mut top: Vec<(String, u32)> = counts.into_iter().collect();
    top.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    top
}

/// Requests between parts: a request one part makes whole and none of its own routes serves is a
/// `requests` edge from the function that makes it whole to the handler of another part's route
/// that serves it, by method and path.
pub fn join(g: &mut Graph) {
    if g.parts.len() < 2 {
        return;
    }
    let mut routes: HashMap<&str, Vec<&Node>> = HashMap::new();
    for n in &g.nodes {
        routes.entry(n.part.as_str()).or_default().push(n);
    }
    let routes: Vec<(&str, Vec<Served>)> = routes.into_iter().map(|(part, nodes)| (part, served(nodes))).collect();
    let mut edges: BTreeMap<(String, String), String> = BTreeMap::new();
    for n in g.nodes.iter().filter(|n| !n.asks.is_empty()) {
        for ask in &n.asks {
            for (part, served) in &routes {
                if *part == n.part {
                    continue;
                }
                for s in serving(served, ask) {
                    edges.entry((n.id.clone(), s.handler.clone())).or_insert_with(|| s.path().to_string());
                }
            }
        }
    }
    for ((src, dst), via) in edges {
        g.edges.push(Edge { src, dst, kind: EdgeKind::Requests, via: Some(via), weight: None, arms: vec![], uses: false });
    }
    g.recount();
}
