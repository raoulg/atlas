//! What a change does to the picture: the routes it adds and removes, the steps and places
//! each route gains, loses and keeps changed, what changes on no route, and the findings it
//! raises and resolves, all read from the union of the two readings the page draws. Two
//! revisions are compared, or one with the working tree; a revision is read from git itself into
//! a directory of its own, so neither the working tree nor the index is touched. The history the
//! revisions come from is read from git as well: its commits in topological order, with their
//! refs.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};

use crate::cache::{Cache, Check, Written};
use crate::diff::{self, Reading, Status};
use crate::flow::Drawn;
use crate::model::{EdgeKind, Graph, NodeKind, Touch, World};
use crate::places::Tmp;
use crate::principles::Finding;
use crate::repo::Repo;
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
    /// Steps by node id: those the route reaches in one reading only, and those it reaches whose
    /// own arrow differs.
    pub steps_added: Vec<String>,
    pub steps_removed: Vec<String>,
    pub steps_changed: Vec<StepChange>,
    /// Ordered bodies that differ, independently of the set of functions a route reaches.
    pub compositions_changed: Vec<CompositionChange>,
    /// Places in words (`writes logs/run.log`), and the touches that reach them.
    pub places_added: Vec<String>,
    pub places_removed: Vec<String>,
    pub touches_added: Vec<Drawn>,
    pub touches_removed: Vec<Drawn>,
    /// Places the route touches in both readings whose arrows differ: other functions touch
    /// them, or touch them otherwise.
    pub places_changed: Vec<PlaceChange>,
}

/// A node both readings hold whose own arrow differs, as the page marks it.
#[derive(Debug, serde::Serialize)]
pub struct StepChange {
    pub id: String,
    /// Its id in A, when B renamed it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub renamed_from: Option<String>,
    /// What it was in A, in words.
    pub was: String,
}

/// A place, by its id and the name the picture gives it, with the arrows it had in A when they
/// differ.
#[derive(Debug, serde::Serialize)]
pub struct PlaceChange {
    pub id: String,
    pub place: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub was: String,
}

/// What the union of the two readings marks that no route reaches: nodes added, removed and
/// changed, and places, as the page marks them.
#[derive(Debug, Default, serde::Serialize)]
pub struct Elsewhere {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub changed: Vec<StepChange>,
    pub places_added: Vec<PlaceChange>,
    pub places_removed: Vec<PlaceChange>,
    pub places_changed: Vec<PlaceChange>,
}

impl Elsewhere {
    fn is_empty(&self) -> bool {
        self.added.is_empty()
            && self.removed.is_empty()
            && self.changed.is_empty()
            && self.places_added.is_empty()
            && self.places_removed.is_empty()
            && self.places_changed.is_empty()
    }
}

/// The execution structure of one reachable function in two readings.
#[derive(Debug, serde::Serialize)]
pub struct CompositionChange {
    pub function: String,
    pub before: crate::flow::Body,
    pub after: crate::flow::Body,
}

#[derive(Debug, Default, serde::Serialize)]
pub struct Changes {
    /// The revision compared from.
    pub a: String,
    /// The revision compared to; none for the working tree.
    pub b: Option<String>,
    /// The roots of the parts compared.
    pub roots: Vec<String>,
    pub routes: Vec<RouteChange>,
    /// What changed that no route reaches.
    pub elsewhere: Elsewhere,
    pub findings_added: Vec<Finding>,
    pub findings_resolved: Vec<Finding>,
    /// The findings asked before that are still found, and the author has since set aside.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub findings_set_aside: Vec<Finding>,
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
/// tree's when there is no revision. A commit never changes, so its parts are kept in the cache
/// and read from there while what the reading took from outside the commit (where its libraries
/// lie) stands as it did; the working tree is read anew each time.
pub fn parts_at(repo: &Path, rev: Option<&str>, opts: &crate::Options) -> Result<Vec<Graph>> {
    let Some(rev) = rev else { return crate::parts::analyze_parts(&[repo.to_path_buf()], opts) };
    // the commit the revision names now, read and kept as that commit
    let (commit, _) = resolve(repo, rev)?;
    let key = match &opts.cache {
        Some(_) => Some(revision_key(repo, &commit, opts)?),
        None => None,
    };
    if let Some(parts) = opts.cache.as_ref().zip(key.as_ref()).and_then(|(cache, key)| kept(cache, key)) {
        return Ok(parts);
    }
    let (tmp, root) = revision(repo, &commit)?;
    // what the reading takes from outside the commit, asked before it reads: a change while it
    // reads is a change the next reading sees
    let witness = opts.cache.as_ref().map(|_| outside(repo, &tmp.root, &root, opts));
    let parts = crate::parts::analyze_parts(&[root], opts);
    if let (Ok(parts), Some(cache), Some(key), Some(witness)) = (&parts, &opts.cache, &key, witness) {
        keep(cache, &key, &tmp.root, witness, parts);
    }
    parts
}

/// What a revision's parts are kept under: the commit, the repository's place in it and its
/// name, how it is read, and where revisions are read. The commit names its content, so the same
/// commit of another clone is the same reading.
fn revision_key(repo: &Path, commit: &str, opts: &crate::Options) -> Result<String> {
    let (_, rel, name) = located(repo)?;
    let crate::Options { libraries, named, package, cache: _ } = opts;
    Ok(format!(
        "{commit}\n{}\n{}\nlibraries {libraries}\npackage {package:?}\nnamed {named:?}\nin {}",
        crate::tree::path_str(&rel),
        name.to_string_lossy(),
        std::env::temp_dir().display()
    ))
}

/// The kind of entry a revision's parts are kept as.
const REVISION: &str = "revision";

/// The ways a reading writes the directory a revision was written to: as the file system
/// resolves it and as it was named, each with `/` and as the platform writes it. The resolved
/// ones come first, since the other may stand inside them (`/private/var/..` and `/var/..`).
fn written_as(base: &Path) -> [String; 4] {
    let resolved = Repo::resolved(base.parent().unwrap_or(base))
        .map(|p| p.join(base.file_name().unwrap_or_default()))
        .unwrap_or_else(|| base.to_path_buf());
    [
        crate::tree::path_str(&resolved),
        resolved.to_string_lossy().to_string(),
        crate::tree::path_str(base),
        base.to_string_lossy().to_string(),
    ]
}

/// What stands in a kept reading for each way of writing the directory the revision was written
/// to: no path holds it.
fn standing(i: usize) -> String {
    format!("\u{0}checkout {i}\u{0}")
}

/// A revision's parts as kept: what the reading took from outside the commit with how each
/// stood, where its steps and values are written, and the parts, with the directory the revision
/// was written to standing for itself.
#[derive(serde::Serialize, serde::Deserialize)]
struct Kept {
    witness: Vec<(Check, String)>,
    written: Written,
    parts: serde_json::Value,
}

/// The parts kept under `key`, while what their reading took from outside the commit stands as
/// it did, written to a directory of their own as a reading now would be.
fn kept(cache: &Cache, key: &str) -> Option<Vec<Graph>> {
    let kept: Kept = serde_json::from_slice(&cache.get(REVISION, key)?).ok()?;
    if !crate::cache::holds(&kept.witness) {
        return None;
    }
    let tmp = Tmp::new("revision").ok()?;
    let forms = written_as(&tmp.root);
    let pairs: Vec<(String, &str)> = forms.iter().enumerate().map(|(i, f)| (standing(i), f.as_str())).collect();
    let mut parts = kept.parts;
    replaced(&mut parts, &pairs.iter().map(|(s, f)| (s.as_str(), *f)).collect::<Vec<_>>());
    let mut graphs: Vec<Graph> = serde_json::from_value(parts).ok()?;
    kept.written.apply(&mut graphs).then_some(graphs)
}

/// Keep a revision's parts under `key`, read from `base`.
fn keep(cache: &Cache, key: &str, base: &Path, witness: Vec<(Check, String)>, parts: &[Graph]) {
    let Ok(mut value) = serde_json::to_value(parts) else { return };
    let forms = written_as(base);
    let pairs: Vec<(&str, String)> = forms.iter().enumerate().map(|(i, f)| (f.as_str(), standing(i))).collect();
    replaced(&mut value, &pairs.iter().map(|(f, s)| (*f, s.as_str())).collect::<Vec<_>>());
    let kept = Kept { witness, written: Written::of(parts), parts: value };
    if let Ok(bytes) = serde_json::to_vec(&kept) {
        cache.put(REVISION, key, &bytes);
    }
}

/// What reading the revision written to `root` takes from outside the commit, as checks of how
/// each stands: where the libraries of each part lie, and whether the environment beside the
/// repository is there to be linked in. What lies in the directory the revision was written to
/// is the commit's.
fn outside(repo: &Path, base: &Path, root: &Path, opts: &crate::Options) -> Vec<(Check, String)> {
    let mut checks = crate::parts::environment(root, opts).unwrap_or_default();
    checks.push(Check::Names(repo.join(".venv")));
    let resolved = Repo::real(&base);
    let inside = |c: &Check| c.path().map_or(false, |p| p.starts_with(base) || p.starts_with(&resolved));
    crate::cache::witness(checks).into_iter().filter(|(c, _)| !inside(c)).collect()
}

/// Every string of a JSON value, the keys of its objects among them, with each `from` replaced
/// by its `to`, in order.
fn replaced(v: &mut serde_json::Value, pairs: &[(&str, &str)]) {
    let swap = |s: &str| pairs.iter().fold(s.to_string(), |s, (from, to)| if s.contains(from) { s.replace(from, to) } else { s });
    match v {
        serde_json::Value::String(s) => *s = swap(s),
        serde_json::Value::Array(xs) => xs.iter_mut().for_each(|x| replaced(x, pairs)),
        serde_json::Value::Object(o) => {
            let entries: Vec<(String, serde_json::Value)> = std::mem::take(o).into_iter().collect();
            for (k, mut x) in entries {
                replaced(&mut x, pairs);
                o.insert(swap(&k), x);
            }
        }
        _ => {}
    }
}

/// How the questions `after` asks stand to those `before` asks, a question being a finding not set
/// aside: those `before` does not ask (raised), those `after` no longer finds (resolved), and those
/// `after` still finds and has set aside.
pub fn raised(before: &[Finding], after: &[Finding]) -> (Vec<Finding>, Vec<Finding>, Vec<Finding>) {
    let key = |f: &Finding| (f.principle, f.node.clone(), f.what.clone());
    let asked = |fs: &[Finding], aside: bool| -> HashSet<_> { fs.iter().filter(|f| f.aside.is_some() == aside).map(key).collect() };
    let (ka, kb, kept) = (asked(before, false), asked(after, false), asked(after, true));
    let gone = || before.iter().filter(|f| f.aside.is_none() && !kb.contains(&key(f)));
    (
        after.iter().filter(|f| f.aside.is_none() && !ka.contains(&key(f))).cloned().collect(),
        gone().filter(|f| !kept.contains(&key(f))).cloned().collect(),
        gone().filter(|f| kept.contains(&key(f))).cloned().collect(),
    )
}

/// What changed from the parts `before`, read at revision `a`, to the parts `after`, read at
/// revision `b` or in the working tree: the union of the two readings, as the page draws it, read
/// along each route, and what it marks that no route reaches.
pub fn between(a: &str, b: Option<&str>, before: &[Graph], after: &[Graph]) -> Changes {
    let several = before.len().max(after.len()) > 1;
    // the part each node is read in
    let mut roots: HashMap<&str, &str> = HashMap::new();
    for g in after.iter().chain(before) {
        for n in &g.nodes {
            roots.entry(n.id.as_str()).or_insert(g.root.as_str());
        }
    }
    let name = after.first().or(before.first()).map_or_else(String::new, |g| g.root.clone());
    let (union, d) = diff::union(&name, before.to_vec(), after.to_vec());
    let root = |id: &str| {
        let found = roots.get(id).or_else(|| d.renamed.get(id).and_then(|old| roots.get(old.as_str())));
        found.map_or_else(|| name.clone(), |r| r.to_string())
    };
    let (old, new) =
        (routes(&diff::reading(&union, &d, Reading::A), &root, several), routes(&diff::reading(&union, &d, Reading::B), &root, several));
    let changed = |id: &String| StepChange {
        id: id.clone(),
        renamed_from: d.renamed.get(id).cloned(),
        was: d.was.get(id).cloned().unwrap_or_default(),
    };
    let labels = place_labels(&union);
    let place_changed = |id: &String| PlaceChange {
        id: id.clone(),
        place: labels.get(id).cloned().unwrap_or_else(|| id.clone()),
        was: d.was.get(id).cloned().unwrap_or_default(),
    };
    let parts: BTreeSet<String> = after.iter().chain(before).map(|g| g.root.clone()).collect();
    let mut out = Changes { a: a.to_string(), b: b.map(str::to_string), roots: parts.into_iter().collect(), ..Default::default() };
    let keys: BTreeSet<&String> = old.keys().chain(new.keys()).collect();
    for k in keys {
        let (a, b) = (old.get(k), new.get(k));
        let empty = Route::default();
        let (a2, b2) = (a.unwrap_or(&empty), b.unwrap_or(&empty));
        let diff = |x: &BTreeSet<String>, y: &BTreeSet<String>| x.difference(y).cloned().collect::<Vec<_>>();
        // a step or place the route has in one reading only is added or removed: changed is what it
        // has in both
        let reached: BTreeSet<&String> = a2.reached.intersection(&b2.reached).collect();
        let at: BTreeSet<&String> = a2.at.intersection(&b2.at).collect();
        let gone = |x: &BTreeMap<String, Touch>, y: &BTreeMap<String, Touch>| {
            x.iter()
                .filter(|(p, _)| !y.contains_key(*p))
                .map(|(p, t)| (p.clone(), Drawn { at: t.drawn(), touch: t.clone() }))
                .unzip::<_, _, Vec<_>, Vec<_>>()
        };
        let ((places_added, touches_added), (places_removed, touches_removed)) =
            (gone(&b2.places, &a2.places), gone(&a2.places, &b2.places));
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
            steps_changed: reached.iter().filter(|id| d.nodes.get(id.as_str()) == Some(&Status::Changed)).map(|id| changed(id)).collect(),
            compositions_changed: b2
                .bodies
                .iter()
                .filter_map(|(id, after)| {
                    let before = a2.bodies.get(id)?;
                    (before != after).then(|| CompositionChange { function: id.clone(), before: before.clone(), after: after.clone() })
                })
                .collect(),
            places_added,
            places_removed,
            touches_added,
            touches_removed,
            places_changed: at
                .iter()
                .filter(|id| d.places.get(id.as_str()) == Some(&Status::Changed))
                .map(|id| place_changed(id))
                .collect(),
        };
        if c.status != "changed"
            || !(c.steps_added.is_empty()
                && c.steps_removed.is_empty()
                && c.steps_changed.is_empty()
                && c.compositions_changed.is_empty()
                && c.places_added.is_empty()
                && c.places_removed.is_empty()
                && c.places_changed.is_empty())
        {
            out.routes.push(c);
        }
    }
    // what the union marks that no route reaches: a node of the program's own, and a place drawn by
    // itself (a coarser place stands for those its routes split)
    let reached: HashSet<&String> = old.values().chain(new.values()).flat_map(|r| &r.reached).collect();
    let at: HashSet<&String> = old.values().chain(new.values()).flat_map(|r| &r.at).collect();
    let own: HashSet<&str> = union.nodes.iter().filter(|n| n.kind != NodeKind::External && !n.library).map(|n| n.id.as_str()).collect();
    let e = &mut out.elsewhere;
    for (id, s) in d.nodes.iter().filter(|(id, _)| own.contains(id.as_str()) && !reached.contains(id)) {
        match s {
            Status::Added => e.added.push(id.clone()),
            Status::Removed => e.removed.push(id.clone()),
            Status::Changed => e.changed.push(changed(id)),
        }
    }
    for (id, s) in d.places.iter().filter(|(id, _)| labels.contains_key(*id) && !at.contains(id)) {
        let place = PlaceChange { id: id.clone(), place: labels[id].clone(), was: String::new() };
        match s {
            Status::Added => e.places_added.push(place),
            Status::Removed => e.places_removed.push(place),
            Status::Changed => e.places_changed.push(place_changed(id)),
        }
    }
    let (fa, fb): (Vec<Finding>, Vec<Finding>) =
        (before.iter().flat_map(crate::aside::check).collect(), after.iter().flat_map(crate::aside::check).collect());
    (out.findings_added, out.findings_resolved, out.findings_set_aside) = raised(&fa, &fb);
    out
}

/// The places a graph's touches rest at, by id, with the names the picture gives them; a coarser
/// place one stands in until it is split is not among them.
fn place_labels(g: &Graph) -> BTreeMap<String, String> {
    g.nodes.iter().flat_map(|n| n.touches.iter().chain(&n.instead)).map(|t| t.drawn().place).map(|p| (p.id, p.label)).collect()
}

/// The changes as text: each route that changed with its steps and places, then the findings.
pub fn report(c: &Changes) -> String {
    let mut out = String::new();
    let span = match &c.b {
        Some(b) => format!("{}..{b}", c.a),
        None => format!("since {}", c.a),
    };
    if c.routes.is_empty()
        && c.elsewhere.is_empty()
        && c.findings_added.is_empty()
        && c.findings_resolved.is_empty()
        && c.findings_set_aside.is_empty()
    {
        return format!("nothing changed in the routes, their places or the findings, {span}\n");
    }
    out.push_str(&format!("{span}\n"));
    // a node changed, by its id from its part's root, renamed or not, and what it was
    let step = |short: &dyn Fn(&str) -> String, s: &StepChange| {
        let renamed = s.renamed_from.as_deref().map(|old| format!("renamed from {} · ", short(old))).unwrap_or_default();
        format!("{}: {renamed}was {}", short(&s.id), s.was)
    };
    for r in &c.routes {
        let mark = match r.status {
            "added" => "+",
            "removed" => "-",
            _ => "~",
        };
        out.push_str(&format!("\n{mark} {}\n", r.route));
        let short = |id: &str| id.strip_prefix(&format!("{}.", r.root)).unwrap_or(id).to_string();
        let changed: Vec<String> = r.steps_changed.iter().map(|s| step(&short, s)).collect();
        let places: Vec<String> = r.places_changed.iter().map(|p| format!("{}: was {}", p.place, p.was)).collect();
        for (sign, what, xs) in [
            ("+", "step", r.steps_added.iter().map(|x| short(x)).collect::<Vec<_>>()),
            ("-", "step", r.steps_removed.iter().map(|x| short(x)).collect()),
            ("~", "step", changed),
            ("+", "place", r.places_added.clone()),
            ("-", "place", r.places_removed.clone()),
            ("~", "place", places),
        ] {
            if r.status != "changed" && what == "step" {
                continue;
            }
            for x in xs {
                out.push_str(&format!("    {sign} {what} {x}\n"));
            }
        }
        for composition in &r.compositions_changed {
            let sequence = |body: &crate::flow::Body| body.steps().iter().map(|s| short(&s.to)).collect::<Vec<_>>().join(" → ");
            out.push_str(&format!(
                "    ~ composition {}: {} ⇒ {}\n",
                short(&composition.function),
                sequence(&composition.before),
                sequence(&composition.after)
            ));
        }
    }
    let e = &c.elsewhere;
    if !e.is_empty() {
        out.push_str("\n~ on no route\n");
        let short = |id: &str| c.roots.iter().find_map(|r| id.strip_prefix(&format!("{r}."))).unwrap_or(id).to_string();
        let changed: Vec<String> = e.changed.iter().map(|s| step(&short, s)).collect();
        let places: Vec<String> = e.places_changed.iter().map(|p| format!("{}: was {}", p.place, p.was)).collect();
        let (places_added, places_removed): (Vec<String>, Vec<String>) =
            (e.places_added.iter().map(|p| p.place.clone()).collect(), e.places_removed.iter().map(|p| p.place.clone()).collect());
        let (added, removed): (Vec<String>, Vec<String>) =
            (e.added.iter().map(|x| short(x)).collect(), e.removed.iter().map(|x| short(x)).collect());
        for (sign, what, xs) in [
            ("+", "", &added),
            ("-", "", &removed),
            ("~", "", &changed),
            ("+", "place ", &places_added),
            ("-", "place ", &places_removed),
            ("~", "place ", &places),
        ] {
            for x in xs {
                out.push_str(&format!("    {sign} {what}{x}\n"));
            }
        }
    }
    for (title, fs) in
        [("findings raised", &c.findings_added), ("findings resolved", &c.findings_resolved), ("findings set aside", &c.findings_set_aside)]
    {
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
    /// Every node it reaches, its start among them.
    reached: BTreeSet<String>,
    bodies: BTreeMap<String, crate::flow::Body>,
    places: BTreeMap<String, Touch>,
    /// The places it touches, by id.
    at: BTreeSet<String>,
}

/// Every route of one reading, by its kind and label (after the part it starts in, `root`, when
/// there are `several`).
fn routes(g: &Graph, root: &dyn Fn(&str) -> String, several: bool) -> HashMap<String, Route> {
    let mut out = HashMap::new();
    let bodies = crate::flow::bodies(g);
    let by_id: HashMap<&str, &crate::model::Node> = g.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let mut next: HashMap<&str, Vec<&str>> = HashMap::new();
    for e in g.edges.iter().filter(|e| matches!(e.kind, EdgeKind::Calls | EdgeKind::Instantiates | EdgeKind::Requests)) {
        next.entry(e.src.as_str()).or_default().push(e.dst.as_str());
    }
    // an exported class's route is its methods
    for e in
        g.edges.iter().filter(|e| e.kind == EdgeKind::Contains && by_id.get(e.src.as_str()).map_or(false, |n| n.kind == NodeKind::Class))
    {
        next.entry(e.src.as_str()).or_default().push(e.dst.as_str());
    }
    for (kind, label, start) in story::entries(g) {
        let part = root(start);
        let name = format!("{}{} · {label}", if several { format!("{part} · ") } else { String::new() }, story::kind_label(kind));
        let mut r = Route { id: start.to_string(), root: part, ..Default::default() };
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
            r.reached.insert(id.to_string());
            r.bodies.insert(id.to_string(), bodies.get(*id).cloned().unwrap_or_default());
            if *id != start && n.kind != NodeKind::External {
                r.steps.insert(id.to_string());
            }
            for t in n.touches_in(start, &|f| seen.contains(f)).into_iter().filter(|t| t.kind != World::Screen || *id == start) {
                r.at.insert(t.drawn().place.id);
                r.places.entry(place(t)).or_insert_with(|| t.clone());
            }
        }
        out.insert(name, r);
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
const REFS: &[(&str, RefKind)] = &[
    ("HEAD -> refs/heads/", RefKind::Head),
    ("tag: refs/tags/", RefKind::Tag),
    ("refs/heads/", RefKind::Branch),
    ("refs/remotes/", RefKind::Remote),
    ("HEAD", RefKind::Head),
];

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
/// `all`, else of HEAD's history. A commit comes before its parents, and of two where neither is the
/// other's ancestor, the newer comes first: the work of now stands on top, whichever branch it is on.
pub fn log(repo: &Path, all: bool, n: usize, skip: usize) -> Result<Log> {
    let (count, skipped) = (format!("-n{n}"), format!("--skip={skip}"));
    let mut args = vec![
        "log",
        "--date-order",
        "--decorate=full",
        "--format=%H%x1f%h%x1f%P%x1f%D%x1f%s%x1f%an%x1f%at%x1e",
        count.as_str(),
        skipped.as_str(),
    ];
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
    let found = git(repo, &["log", "-1", "--format=%H%x1f%h%x1f%s", "--end-of-options", rev, "--"])
        .with_context(|| format!("no commit `{rev}`"))?;
    let text = String::from_utf8_lossy(&found).trim_end().to_string();
    let mut f = text.splitn(3, '\x1f');
    let (Some(hash), Some(short), subject) = (f.next(), f.next(), f.next().unwrap_or("")) else { bail!("no commit `{rev}`") };
    Ok((hash.to_string(), Named { rev: rev.to_string(), short: short.to_string(), subject: subject.to_string() }))
}

/// Where the repository sits in its git checkout: the checkout's top, the repository's place
/// below it, and its name. git is handed paths as written; resolved ones only say where the
/// repository sits in it.
fn located(repo: &Path) -> Result<(PathBuf, PathBuf, std::ffi::OsString)> {
    let top = PathBuf::from(String::from_utf8_lossy(&git(repo, &["rev-parse", "--show-toplevel"])?).trim());
    let resolved = |p: &Path| Repo::real(&p);
    let rel = resolved(repo).strip_prefix(resolved(&top)).unwrap_or(Path::new("")).to_path_buf();
    let name = resolved(repo).file_name().map(|n| n.to_os_string()).unwrap_or_else(|| "repo".into());
    Ok((top, rel, name))
}

/// A vision as it is read: its parts, and the directory they were read from, the copy's place for
/// the directory the vision was asked for, where the vision's paths start. The copy is gone once it
/// is read; `dir` names where its files lay, so an item of the vision is placed by the path of its
/// file from `dir`.
#[derive(Debug)]
pub struct Envisioned {
    pub parts: Vec<Graph>,
    pub dir: PathBuf,
}

/// A vision: the working tree of `repo` with `files` written otherwise, each path from the
/// repository's root given whole or, where it is none, removed. It is read as a revision is, from a
/// directory of its own under the same name, so the graph's names agree with the working tree's; a
/// vision is sketch code, which has to parse, not to build.
pub fn envisioned(repo: &Path, files: &BTreeMap<PathBuf, Option<String>>, opts: &crate::Options) -> Result<Envisioned> {
    for rel in files.keys() {
        if rel.is_absolute() || rel.components().any(|c| !matches!(c, std::path::Component::Normal(_))) {
            bail!("a vision names files from the repository's root, inside it: {}", rel.display());
        }
    }
    let repo = Repo::real(repo);
    // what is read: the repository, or for a package named on its own the project it is in
    let listing = crate::language::read(&repo, opts)?;
    let top = Repo::real(listing.root());
    let inner = repo.strip_prefix(&top).unwrap_or(Path::new("")).to_path_buf();
    let name = top.file_name().map(|n| n.to_os_string()).unwrap_or_else(|| "repo".into());
    let tmp = Tmp::new("vision")?;
    let laid = PathBuf::from(name);
    for f in listing.files_under("") {
        let rel = f.strip_prefix(listing.root()).unwrap_or(&f).to_path_buf();
        if !rel.strip_prefix(&inner).map_or(false, |r| files.contains_key(r)) {
            tmp.copy_in(&f, &laid.join(&rel))?;
        }
    }
    for (rel, text) in files {
        if let Some(text) = text {
            tmp.write(&laid.join(&inner).join(rel), text.as_bytes())?;
        }
    }
    tmp.folder(&laid)?;
    // the environment beside the repository stands beside the vision too: its `.venv`, and what a
    // path out of it reaches (`extra-paths = ["../typings"]`)
    if Repo::has(&top, ".venv") {
        tmp.link(&top.join(".venv"), &laid.join(".venv"));
    }
    for beside in Repo::beside(&top) {
        if let Some(name) = beside.file_name() {
            tmp.link(&beside, Path::new(name));
        }
    }
    // read where the file system resolves it, so that every item's file starts with `dir`
    let dir = Repo::real(&tmp.root.join(&laid).join(&inner));
    Ok(Envisioned { parts: crate::parts::analyze_parts(&[dir.clone()], opts)?, dir })
}

/// The repository as it was at `rev`, written to a `Tmp` of its own under the same name, so the
/// graph's names agree: (the `Tmp`, removed when it is dropped, and the repository in it). A
/// Python environment beside the repository is linked in, so its types are read the same way.
fn revision(repo: &Path, rev: &str) -> Result<(Tmp, PathBuf)> {
    let (top, rel, name) = located(repo)?;
    let repo = Repo::real(repo);
    let rel_git = crate::tree::path_str(&rel);
    let tmp = Tmp::new("revision")?;
    let laid = PathBuf::from(name);
    tmp.folder(&laid)?;
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
    let mut cat = Command::new("git")
        .arg("-C")
        .arg(&top)
        .args(["cat-file", "--batch"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .context("running git")?;
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
        let size: usize =
            header.split_whitespace().nth(2).and_then(|s| s.parse().ok()).with_context(|| format!("git cat-file: {header}"))?;
        let mut content = vec![0; size];
        reader.read_exact(&mut content)?;
        let mut newline = [0u8; 1];
        reader.read_exact(&mut newline)?;
        tmp.write(&laid.join(path.strip_prefix(&rel).unwrap_or(path)), &content)?;
    }
    writer.join().map_err(|_| anyhow::anyhow!("writing to git"))??;
    cat.wait()?;
    if Repo::has(&repo, ".venv") {
        tmp.link(&repo.join(".venv"), &laid.join(".venv"));
    }
    let root = tmp.root.join(&laid);
    Ok((tmp, root))
}
