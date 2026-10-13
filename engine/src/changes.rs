//! What a change does to the picture: the routes it adds and removes, the steps and places
//! each route gains and loses, and the findings it raises and resolves. Two revisions are
//! compared, or one with the working tree; a revision is read from git itself into a directory
//! of its own, so neither the working tree nor the index is touched. The history the revisions
//! come from is read from git as well: its commits in topological order, with their refs.

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
    /// The revision compared from.
    pub a: String,
    /// The revision compared to; none for the working tree.
    pub b: Option<String>,
    pub routes: Vec<RouteChange>,
    pub findings_added: Vec<Finding>,
    pub findings_resolved: Vec<Finding>,
}

/// The revisions a range names, as git reads one: `A..B` is A against B, a side left out is
/// `HEAD`; a single revision is that revision against the working tree.
pub fn range(spec: &str) -> (&str, Option<&str>) {
    match spec.split_once("..") {
        Some((a, b)) => (or_head(a), Some(or_head(b.trim_start_matches('.')))),
        None => (spec, None),
    }
}

/// A side of a range, `HEAD` where it is left out.
fn or_head(rev: &str) -> &str {
    if rev.is_empty() {
        "HEAD"
    } else {
        rev
    }
}

/// The working tree of `repo` against revision `rev`.
pub fn since(repo: &Path, rev: &str, opts: &crate::Options) -> Result<Changes> {
    compare(repo, rev, None, opts)
}

/// Revision `b` of `repo` (the working tree when there is none) against revision `a`.
pub fn compare(repo: &Path, a: &str, b: Option<&str>, opts: &crate::Options) -> Result<Changes> {
    let after = parts_at(repo, b, opts)?;
    let before = parts_at(repo, Some(a), opts)?;
    Ok(between(a, b, &before, &after))
}

/// The parts of `repo` as they are at revision `rev`, each read in its own language; the working
/// tree's when there is no revision.
pub fn parts_at(repo: &Path, rev: Option<&str>, opts: &crate::Options) -> Result<Vec<Graph>> {
    let Some(rev) = rev else { return crate::parts::analyze_parts(&[repo.to_path_buf()], opts) };
    let (base, root) = revision(repo, rev)?;
    let parts = crate::parts::analyze_parts(&[root], opts);
    let _ = std::fs::remove_dir_all(&base);
    parts
}

/// The findings of `after` that `before` does not have (raised), and those of `before` that
/// `after` no longer has (resolved).
pub fn raised(before: &[Finding], after: &[Finding]) -> (Vec<Finding>, Vec<Finding>) {
    let key = |f: &Finding| (f.principle, f.node.clone(), f.what.clone());
    let (ka, kb): (HashSet<_>, HashSet<_>) = (before.iter().map(key).collect(), after.iter().map(key).collect());
    (after.iter().filter(|f| !ka.contains(&key(f))).cloned().collect(), before.iter().filter(|f| !kb.contains(&key(f))).cloned().collect())
}

/// What changed from the parts `before`, read at revision `a`, to the parts `after`, read at
/// revision `b` or in the working tree.
pub fn between(a: &str, b: Option<&str>, before: &[Graph], after: &[Graph]) -> Changes {
    let several = before.len().max(after.len()) > 1;
    let (old, new) = (routes(before, several), routes(after, several));
    let mut out = Changes { a: a.to_string(), b: b.map(str::to_string), ..Default::default() };
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
    let (fa, fb): (Vec<Finding>, Vec<Finding>) = (before.iter().flat_map(principles::check).collect(), after.iter().flat_map(principles::check).collect());
    (out.findings_added, out.findings_resolved) = raised(&fa, &fb);
    out
}

/// The changes as text: each route that changed with its steps and places, then the findings.
pub fn report(c: &Changes) -> String {
    let mut out = String::new();
    let span = match &c.b {
        Some(b) => format!("{}..{b}", c.a),
        None => format!("since {}", c.a),
    };
    if c.routes.is_empty() && c.findings_added.is_empty() && c.findings_resolved.is_empty() {
        return format!("nothing changed in the routes, their places or the findings, {span}\n");
    }
    out.push_str(&format!("{span}\n"));
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

/// The history of a repository: the commit HEAD is at, whether the working tree differs from it
/// and in how many files, and the commits, newest first, each after the commits that build on it.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Log {
    pub head: String,
    pub dirty: bool,
    /// The files the working tree changes, adds or removes against HEAD, those git does not know
    /// yet included.
    pub changed: usize,
    pub commits: Vec<Commit>,
}

/// A commit as the history shows it.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Commit {
    pub hash: String,
    pub short: String,
    pub parents: Vec<String>,
    pub refs: Vec<Ref>,
    pub subject: String,
    pub author: String,
    /// When it was written, in seconds since 1970.
    pub time: i64,
}

/// A name that points at a commit.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Ref {
    pub name: String,
    pub kind: RefKind,
}

/// What a name that points at a commit is: the branch HEAD is on (or HEAD itself, detached), a
/// branch, a branch of a remote, a tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RefKind {
    Head,
    Branch,
    Remote,
    Tag,
}

/// How git writes each kind of ref it decorates a commit with, in full, the most specific first.
const REFS: &[(&str, RefKind)] = &[("HEAD -> refs/heads/", RefKind::Head), ("tag: refs/tags/", RefKind::Tag), ("refs/heads/", RefKind::Branch), ("refs/remotes/", RefKind::Remote), ("HEAD", RefKind::Head)];

/// The refs of git's full decoration of a commit (`HEAD -> refs/heads/main, tag: refs/tags/v1`);
/// another kind of ref (a stash, a note) is no name of the history.
pub fn refs(decoration: &str) -> Vec<Ref> {
    decoration
        .split(", ")
        .filter_map(|d| {
            let (prefix, kind) = REFS.iter().find(|(p, _)| d.starts_with(p))?;
            let rest = &d[prefix.len()..];
            Some(Ref { name: if rest.is_empty() { d.to_string() } else { rest.to_string() }, kind: *kind })
        })
        .collect()
}

/// `n` commits of `repo`, after the newest `skip`: of every branch, remote branch and tag when
/// `all`, else of HEAD's history.
pub fn log(repo: &Path, all: bool, n: usize, skip: usize) -> Result<Log> {
    let (count, skipped) = (format!("-n{n}"), format!("--skip={skip}"));
    let mut args = vec!["log", "--topo-order", "--decorate=full", "--format=%H%x1f%h%x1f%P%x1f%D%x1f%s%x1f%an%x1f%at%x1e", count.as_str(), skipped.as_str()];
    args.extend(if all { ["--branches", "--tags", "--remotes", "HEAD"].as_slice() } else { ["HEAD"].as_slice() });
    let text = String::from_utf8_lossy(&git(repo, &args)?).to_string();
    let commits = text
        .split('\x1e')
        .filter_map(|r| {
            let f: Vec<&str> = r.trim_start_matches('\n').split('\x1f').collect();
            let [hash, short, parents, decoration, subject, author, time] = f.as_slice() else { return None };
            Some(Commit {
                hash: hash.to_string(),
                short: short.to_string(),
                parents: parents.split_whitespace().map(str::to_string).collect(),
                refs: refs(decoration),
                subject: subject.to_string(),
                author: author.to_string(),
                time: time.trim().parse().unwrap_or(0),
            })
        })
        .collect();
    let head = String::from_utf8_lossy(&git(repo, &["rev-parse", "HEAD"])?).trim().to_string();
    // `XY path`, one file each; a file renamed or copied is followed by the path it had
    let status = git(repo, &["status", "--porcelain", "-z"])?;
    let (mut fields, mut changed) = (status.split(|b| *b == 0).filter(|e| !e.is_empty()), 0);
    while let Some(e) = fields.next() {
        changed += 1;
        if e.iter().take(2).any(|x| matches!(x, b'R' | b'C')) {
            fields.next();
        }
    }
    Ok(Log { head, dirty: changed > 0, changed, commits })
}

/// A revision as a page names it: as it was asked for, by its short hash, and by its subject.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Named {
    pub rev: String,
    pub short: String,
    pub subject: String,
}

/// The working tree, named as a revision is.
pub fn worktree() -> Named {
    Named { rev: "worktree".into(), short: "working tree".into(), subject: String::new() }
}

/// The commit revision `rev` of `repo` names: its full hash, and the revision as a page names it.
pub fn resolve(repo: &Path, rev: &str) -> Result<(String, Named)> {
    let found = git(repo, &["log", "-1", "--format=%H%x1f%h%x1f%s", "--end-of-options", rev, "--"]).with_context(|| format!("no commit `{rev}`"))?;
    let text = String::from_utf8_lossy(&found).trim_end().to_string();
    let mut f = text.splitn(3, '\x1f');
    let (Some(hash), Some(short), subject) = (f.next(), f.next(), f.next().unwrap_or("")) else { bail!("no commit `{rev}`") };
    Ok((hash.to_string(), Named { rev: rev.to_string(), short: short.to_string(), subject: subject.to_string() }))
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
