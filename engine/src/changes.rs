//! What a change does to the picture: the routes it adds and removes, the steps and places
//! each route gains and loses, and the findings it raises and resolves. The working tree is
//! read against a git revision, which is read from git itself into a directory of its own, so
//! neither the working tree nor the index is touched.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};

use crate::flow::Drawn;
use crate::model::{EdgeKind, Graph, NodeKind, Touch, World};
use crate::principles::{self, Finding};
use crate::story;

/// How one route differs between two revisions.
#[derive(Debug, Default, serde::Serialize)]
pub struct RouteChange {
    /// `API · GET /api/regels/{id}`, with its part when the repository has several.
    pub route: String,
    /// The node the route starts at, and the root of its part.
    pub id: String,
    pub root: String,
    /// `added`, `removed` or `changed`.
    pub status: &'static str,
    /// Steps by node id.
    pub steps_added: Vec<String>,
    pub steps_removed: Vec<String>,
    /// Places in words (`writes logs/run.log`), and the touches that reach them.
    pub places_added: Vec<String>,
    pub places_removed: Vec<String>,
    pub touches_added: Vec<Drawn>,
    pub touches_removed: Vec<Drawn>,
}

#[derive(Debug, Default, serde::Serialize)]
pub struct Changes {
    pub rev: String,
    pub routes: Vec<RouteChange>,
    pub findings_added: Vec<Finding>,
    pub findings_resolved: Vec<Finding>,
}

/// The working tree of `repo` against revision `rev`.
pub fn since(repo: &Path, rev: &str, opts: &crate::Options) -> Result<Changes> {
    let after = crate::parts::analyze_parts(&[repo.to_path_buf()], opts)?;
    let (base, root) = revision(repo, rev)?;
    let before = crate::parts::analyze_parts(&[root], opts);
    let _ = std::fs::remove_dir_all(&base);
    Ok(between(rev, &before?, &after))
}

/// What changed from the parts `before` to the parts `after`.
pub fn between(rev: &str, before: &[Graph], after: &[Graph]) -> Changes {
    let several = before.len().max(after.len()) > 1;
    let (old, new) = (routes(before, several), routes(after, several));
    let mut out = Changes { rev: rev.to_string(), ..Default::default() };
    let keys: BTreeSet<&String> = old.keys().chain(new.keys()).collect();
    for k in keys {
        let (a, b) = (old.get(k), new.get(k));
        let empty = Route::default();
        let (a2, b2) = (a.unwrap_or(&empty), b.unwrap_or(&empty));
        let diff = |x: &BTreeSet<String>, y: &BTreeSet<String>| x.difference(y).cloned().collect::<Vec<_>>();
        let gone = |x: &BTreeMap<String, Touch>, y: &BTreeMap<String, Touch>| x.iter().filter(|(p, _)| !y.contains_key(*p)).map(|(p, t)| (p.clone(), Drawn { at: t.drawn(), touch: t.clone() })).unzip::<_, _, Vec<_>, Vec<_>>();
        let ((places_added, touches_added), (places_removed, touches_removed)) = (gone(&b2.places, &a2.places), gone(&a2.places, &b2.places));
        let here = b.or(a).unwrap_or(&empty);
        let c = RouteChange {
            route: k.clone(),
            id: here.id.clone(),
            root: here.root.clone(),
            status: match (a, b) {
                (None, _) => "added",
                (_, None) => "removed",
                _ => "changed",
            },
            steps_added: diff(&b2.steps, &a2.steps),
            steps_removed: diff(&a2.steps, &b2.steps),
            places_added,
            places_removed,
            touches_added,
            touches_removed,
        };
        if c.status != "changed" || !(c.steps_added.is_empty() && c.steps_removed.is_empty() && c.places_added.is_empty() && c.places_removed.is_empty()) {
            out.routes.push(c);
        }
    }
    let key = |f: &Finding| (f.principle, f.node.clone(), f.what.clone());
    let (fa, fb): (Vec<Finding>, Vec<Finding>) = (before.iter().flat_map(principles::check).collect(), after.iter().flat_map(principles::check).collect());
    let (ka, kb): (HashSet<_>, HashSet<_>) = (fa.iter().map(key).collect(), fb.iter().map(key).collect());
    out.findings_added = fb.into_iter().filter(|f| !ka.contains(&key(f))).collect();
    out.findings_resolved = fa.into_iter().filter(|f| !kb.contains(&key(f))).collect();
    out
}

/// The changes as text: each route that changed with its steps and places, then the findings.
pub fn report(c: &Changes) -> String {
    let mut out = String::new();
    if c.routes.is_empty() && c.findings_added.is_empty() && c.findings_resolved.is_empty() {
        return format!("nothing changed in the routes, their places or the findings since {}\n", c.rev);
    }
    out.push_str(&format!("since {}\n", c.rev));
    for r in &c.routes {
        let mark = match r.status {
            "added" => "+",
            "removed" => "-",
            _ => "~",
        };
        out.push_str(&format!("\n{mark} {}\n", r.route));
        let short = |id: &str| id.strip_prefix(&format!("{}.", r.root)).unwrap_or(id).to_string();
        for (sign, what, xs) in [("+", "step", &r.steps_added), ("-", "step", &r.steps_removed), ("+", "place", &r.places_added), ("-", "place", &r.places_removed)] {
            if r.status != "changed" && what == "step" {
                continue;
            }
            for x in xs {
                let x = if what == "step" { short(x) } else { x.clone() };
                out.push_str(&format!("    {sign} {what} {x}\n"));
            }
        }
    }
    for (title, fs) in [("findings raised", &c.findings_added), ("findings resolved", &c.findings_resolved)] {
        if fs.is_empty() {
            continue;
        }
        out.push_str(&format!("\n{title}\n"));
        for f in fs {
            out.push_str(&format!("  {} · {}:{}  {}\n", f.principle, f.file, f.line, f.what));
        }
    }
    out
}

/// A route: where it starts, the steps it can run, and the places they touch.
#[derive(Default)]
struct Route {
    id: String,
    root: String,
    steps: BTreeSet<String>,
    places: BTreeMap<String, Touch>,
}

/// Every route of the parts, by its kind and label.
fn routes(parts: &[Graph], several: bool) -> HashMap<String, Route> {
    let mut out = HashMap::new();
    for g in parts {
        let by_id: HashMap<&str, &crate::model::Node> = g.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
        let mut next: HashMap<&str, Vec<&str>> = HashMap::new();
        for e in g.edges.iter().filter(|e| matches!(e.kind, EdgeKind::Calls | EdgeKind::Instantiates | EdgeKind::Requests)) {
            next.entry(e.src.as_str()).or_default().push(e.dst.as_str());
        }
        // an exported class's route is its methods
        for e in g.edges.iter().filter(|e| e.kind == EdgeKind::Contains && by_id.get(e.src.as_str()).map_or(false, |n| n.kind == NodeKind::Class)) {
            next.entry(e.src.as_str()).or_default().push(e.dst.as_str());
        }
        for (kind, label, start) in story::entries(g) {
            let name = format!("{}{} · {label}", if several { format!("{} · ", g.root) } else { String::new() }, story::kind_label(kind));
            let mut r = Route { id: start.to_string(), root: g.root.clone(), ..Default::default() };
            let mut seen = HashSet::from([start]);
            let (mut stack, mut order) = (vec![start], vec![]);
            while let Some(id) = stack.pop() {
                order.push(id);
                for d in next.get(id).into_iter().flatten() {
                    if seen.insert(d) {
                        stack.push(d);
                    }
                }
            }
            // the places its own calls hand
            for id in &order {
                let Some(n) = by_id.get(id) else { continue };
                if *id != start && n.kind != NodeKind::External {
                    r.steps.insert(id.to_string());
                }
                for t in n.touches_in(start, &|f| seen.contains(f)).into_iter().filter(|t| t.kind != World::Screen || *id == start) {
                    r.places.entry(place(t)).or_insert_with(|| t.clone());
                }
            }
            out.insert(name, r);
        }
    }
    out
}

/// A place as a route touches it, in words: `writes logs/run.log`, `writes SQLite(log)`.
fn place(t: &Touch) -> String {
    let what = if t.artifact.is_empty() { crate::flow::shown(&t.what) } else { t.artifact.clone() };
    format!("{} {what}", t.verb()).trim().to_string()
}

fn git(dir: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output().context("running git")?;
    if !out.status.success() {
        bail!("git {}: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(out.stdout)
}

/// The repository as it was at `rev`, written to a directory of its own under the same name,
/// so the graph's names agree: (the directory to remove afterwards, the repository in it). A
/// Python environment beside the repository is linked in, so its types are read the same way.
fn revision(repo: &Path, rev: &str) -> Result<(PathBuf, PathBuf)> {
    // git is handed paths as written; resolved ones only say where the repository sits in it
    let top = PathBuf::from(String::from_utf8_lossy(&git(repo, &["rev-parse", "--show-toplevel"])?).trim());
    let resolved = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let rel = resolved(repo).strip_prefix(resolved(&top)).unwrap_or(Path::new("")).to_path_buf();
    let repo = resolved(repo);
    let rel_git = crate::tree::path_str(&rel);
    let name = repo.file_name().map(|n| n.to_os_string()).unwrap_or_else(|| "repo".into());
    // one directory per reading, also when a process reads several at once
    static READ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = READ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let base = std::env::temp_dir().join(format!("atlas-changes-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let root = base.join(name);
    std::fs::create_dir_all(&root)?;
    let listed = git(&top, &["ls-tree", "-r", "-z", rev, "--", if rel_git.is_empty() { "." } else { &rel_git }])?;
    // `<mode> blob <sha>\t<path>`; links and submodules are not files of the revision
    let blobs: Vec<(String, PathBuf)> = listed
        .split(|b| *b == 0)
        .filter_map(|e| {
            let e = String::from_utf8_lossy(e);
            let (meta, path) = e.split_once('\t')?;
            let mut m = meta.split(' ');
            let (mode, kind, sha) = (m.next()?, m.next()?, m.next()?);
            (kind == "blob" && mode != "120000").then(|| (sha.to_string(), PathBuf::from(path)))
        })
        .collect();
    let mut cat = Command::new("git").arg("-C").arg(&top).args(["cat-file", "--batch"]).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().context("running git")?;
    let mut stdin = cat.stdin.take().context("git's input")?;
    let shas: Vec<String> = blobs.iter().map(|(s, _)| s.clone()).collect();
    let writer = std::thread::spawn(move || -> std::io::Result<()> {
        for s in shas {
            writeln!(stdin, "{s}")?;
        }
        Ok(())
    });
    let mut reader = BufReader::new(cat.stdout.take().context("git's output")?);
    for (_, path) in &blobs {
        let mut header = String::new();
        reader.read_line(&mut header)?;
        let size: usize = header.split_whitespace().nth(2).and_then(|s| s.parse().ok()).with_context(|| format!("git cat-file: {header}"))?;
        let mut content = vec![0; size];
        reader.read_exact(&mut content)?;
        let mut newline = [0u8; 1];
        reader.read_exact(&mut newline)?;
        let dest = root.join(path.strip_prefix(&rel).unwrap_or(path));
        if let Some(d) = dest.parent() {
            std::fs::create_dir_all(d)?;
        }
        std::fs::write(&dest, content)?;
    }
    writer.join().map_err(|_| anyhow::anyhow!("writing to git"))??;
    cat.wait()?;
    #[cfg(unix)]
    if repo.join(".venv").is_dir() {
        let _ = std::os::unix::fs::symlink(repo.join(".venv"), root.join(".venv"));
    }
    Ok((base, root))
}
