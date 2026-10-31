//! `atlas-code serve`: the page of a repository, served on this machine and shared with an agent. The page
//! says what it shows, goes where an agent points it, and keeps the notes its reader writes on boxes
//! and lines; all three are files of `.atlas/` (`shared`), which `atlas-code mcp` reads and writes too.
//!
//! The page is the working tree's, and follows it: when what a reading rests on changes (a file Atlas
//! reads, the answer to a question it asked of the listing, `.atlasignore`, the vision), the server
//! publishes a complete snapshot and the open page draws it in the view it was in; until then the page
//! keeps the last complete one. Beside it, at `/history`, the
//! history of `atlas-code history`, so that two commits are a click away. At `/api/files`, the files Atlas
//! reads, each with what the graph defines in it: the tree a file browser shows beside the page.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::{Hash, Hasher};
use std::net::{SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use atlas_engine::changes::{self, Named};
use atlas_engine::parts::Witnesses;
use atlas_engine::repo::Stamp;
use atlas_engine::sight::Sight;
use atlas_engine::Options;
use serde_json::{json, Value};

use crate::http::{self, Answer, Request};
use crate::log::{Log, Who};
use crate::page::{self, Html, Opens, Reading};
use crate::shared::{self, Note, State};

const LOADING: &str = include_str!("../../viewer/loading.html");

/// The loading page, with the measure of progress the page shares.
fn loading() -> String {
    LOADING.replacen("/*__PROGRESS__*/", page::PROGRESS, 1).replacen("/*__PALETTE__*/", page::PALETTE, 1)
}

/// The working tree as far as Atlas lists it: each file by its size and the time it was written.
type Stamps = BTreeMap<PathBuf, Stamp>;

/// The one phase of a drawing: reading the code, with the libraries it calls as far as asked.
const READING: &str = "reading";

/// A page of the working tree, with the files it was drawn from, the name the page knows it by,
/// what its graph defines in each file, and what its reading rests on: the witnesses of the
/// working tree's reading and the vision beside it.
struct Drawn {
    stamps: Stamps,
    id: String,
    data: String,
    facts: Facts,
    witnesses: Mutex<Witnesses>,
    vision: Stamp,
}

impl Drawn {
    /// Whether the working tree of `repo` and the vision beside it still give this page.
    fn stands(&self, repo: &Path) -> bool {
        vision(repo) == self.vision && self.witnesses.lock().unwrap_or_else(|e| e.into_inner()).hold()
    }
}

/// What a graph defines in each file of the repository, by the file's path from the root: its
/// modules, classes and functions, each with its id, the line it starts at, and whether a route
/// starts at it.
type Facts = BTreeMap<String, Vec<Value>>;

/// The phase the drawing is in, since when, and what went wrong.
struct Progress {
    phase: &'static str,
    error: Option<String>,
    since: Instant,
}

struct Server {
    repo: PathBuf,
    opts: Options,
    hosts: Vec<String>,
    log: Arc<Log>,
    /// The page of the working tree last drawn.
    kept: Mutex<Option<Arc<Drawn>>>,
    progress: Mutex<Progress>,
    /// Held while the page is drawn: it is drawn once, however many ask for it at a time.
    drawing: Mutex<()>,
    /// How long each phase took the last few times the server started, in seconds.
    timings: Mutex<shared::Timings>,
    /// Whether this start found the index of its build of Atlas, so reads little: the times it is
    /// expected to take are those of starts of its kind.
    warm: bool,
}

/// Serve the page of `repo` on this machine until the process stops, at `port`, else at the
/// repository's own port or a free one; opening it in the browser when `shown`.
pub fn serve(repo: &Path, port: Option<u16>, opts: &Options, shown: bool) -> Result<()> {
    // this machine only: the page shows the repository's code
    let at = |p: u16| SocketAddr::from(([127, 0, 0, 1], p));
    let listener = match port {
        Some(p) => TcpListener::bind(at(p)).with_context(|| format!("serving at {}", at(p)))?,
        None => TcpListener::bind(at(own_port(repo))).or_else(|_| TcpListener::bind(at(0))).context("serving on 127.0.0.1")?,
    };
    let at = listener.local_addr()?;
    let log = Arc::new(Log::new());
    let server = Arc::new(Server {
        repo: repo.to_path_buf(),
        opts: opts.clone(),
        hosts: vec![at.to_string(), format!("localhost:{}", at.port())],
        log: Arc::clone(&log),
        kept: Mutex::new(None),
        progress: Mutex::new(Progress { phase: READING, error: None, since: Instant::now() }),
        drawing: Mutex::new(()),
        timings: Mutex::new(shared::timings(repo)),
        warm: opts.cache.as_ref().and_then(|c| c.workspace(repo)).is_some_and(|c| c.holds_any()),
    });
    let url = format!("http://{at}/");
    shared::set_serving(repo, &url)?;
    log.start(&repo.display().to_string(), &url);
    // a note is for the work at hand: this page starts without the ones resolved before it
    if let Err(e) = shared::forget_resolved(repo) {
        log.say(Who::Atlas, &format!("could not read the notes: {e:#}"));
    }
    let (watched, told) = (repo.to_path_buf(), Arc::clone(&log));
    std::thread::spawn(move || watch_notes(&watched, &told));
    let code = Arc::clone(&server);
    std::thread::spawn(move || code.watch());
    if shown {
        page::open(url.as_ref());
    }
    http::listen(listener, |e| log.say(Who::Atlas, e), move |req| server.respond(req));
    Ok(())
}

/// Whether a request asks what changes something or runs an analysis: a change, the look an agent
/// left (taken once) or a page to keep (`/export`). The history beside the page says the same of its
/// own (`history::respond`).
fn acts(req: &Request) -> bool {
    req.method != "GET" || matches!(req.path.as_str(), "/api/look" | "/export")
}

impl Server {
    /// The page, and what it and an agent share. A request that changes something or runs an
    /// analysis comes from the page itself or its reader (`Request::own`).
    fn respond(&self, req: &Request) -> Answer {
        if !self.hosts.contains(&req.host) {
            return Answer::text("403 Forbidden", "atlas-code serve answers only at the address it serves".into());
        }
        if acts(req) && !req.own {
            return Answer::text(
                "403 Forbidden",
                "atlas-code serve takes what changes something or runs an analysis from its own page only".into(),
            );
        }
        if let Some(answer) = crate::history::respond(&self.repo, &self.opts, req, "/history") {
            return answer;
        }
        let json = |v: Result<Value>| v.map_or_else(Answer::failed, |v| Answer::json(v.to_string()));
        match (req.method.as_str(), req.path.as_str()) {
            ("GET", "/") => Answer::page(self.kept().map_or_else(loading, |_| Html::shell().into())),
            ("GET", "/api/drawn") => Answer::json(self.drawn().to_string()),
            ("GET", "/api/snapshot") => self.kept().map_or_else(Answer::none, |d| Answer::json(d.data.clone())),
            ("GET", "/api/files") => json(self.files()),
            ("GET", "/export") => {
                self.export(req).map_or_else(Answer::failed, |(name, html)| Answer::download(format!("{name}.html"), html))
            }
            ("POST", "/api/shown") => json(self.body(req).and_then(|v| shared::set_shown(&self.repo, &v).map(|_| Value::Null))),
            ("GET", "/api/look") => match shared::take_look(&self.repo) {
                Ok(Some(look)) => {
                    let said = serde_json::to_string(&look).unwrap_or_default();
                    self.log.say(Who::Agent, &format!("look here: {said}"));
                    Answer::json(said)
                }
                Ok(None) => Answer::none(),
                Err(e) => Answer::failed(e),
            },
            ("GET", "/api/notes") => json(shared::notes(&self.repo).and_then(|n| Ok(serde_json::to_value(n)?))),
            ("GET", "/api/annotations") => json(shared::annotations(&self.repo).and_then(|a| Ok(serde_json::to_value(a)?))),
            ("POST", "/api/notes") => json(self.note(req)),
            ("POST", "/api/notes/reopen") => json(self.reopen(req)),
            _ => Answer::text("404 Not Found", format!("nothing at {} {}", req.method, req.path)),
        }
    }

    /// The page as `atlas-code view` writes it, opened in the sight the reader has (`v`): a file to keep
    /// or send, that needs no server.
    fn export(&self, req: &Request) -> Result<(String, String)> {
        let sight = req.query.get("v").map(|v| v.parse::<Sight>()).transpose().context("the view is not one the page writes")?;
        let said = serde_json::to_string(sight.as_ref().unwrap_or(&Sight::default()))?;
        let read = page::reading(&[self.repo.clone()], &self.opts, None, None)?;
        let findings = read.findings();
        let (graph, compared) = read.drawn();
        let flow = atlas_engine::flow::of(&graph);
        let html = Html::of(&graph, &flow, &findings, compared.as_ref(), &Opens { route: None, sight, served: None })?;
        self.log.say(Who::You, &format!("exported {}.html, in the view {said}", graph.root));
        Ok((graph.root, html.into()))
    }

    /// Draw the working tree, once, with the libraries asked for: the page keeps the last complete
    /// picture until this one is drawn, and the open browser follows it by its drawing id.
    fn page(&self, now: Stamps) -> Result<()> {
        let _drawing = self.drawing.lock().unwrap_or_else(|e| e.into_inner());
        let kept = self.kept();
        match &kept {
            Some(d) if d.stands(&self.repo) => return Ok(()),
            Some(d) => self.log.say(Who::Atlas, &format!("changed {}", changed(&d.stamps, &now))),
            None => {}
        }
        self.progress(READING, None);
        let drawn = self.draw(&now, &name(&now))?;
        let took = self.since();
        *self.kept.lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(drawn));
        self.progress("ready", None);
        // what a start takes is what the bar is shown for: a drawing after a change reads most of
        // it from the cache, and would make the next start look quick
        if kept.is_some() {
            return Ok(());
        }
        let mut timings = self.timings.lock().unwrap_or_else(|e| e.into_inner());
        let phases = timings.of_mut(self.warm);
        // a phase no drawing has is no measure of one
        phases.retain(|phase, _| phase == READING);
        let times = phases.entry(READING.to_string()).or_default();
        times.push(took);
        let gone = times.len().saturating_sub(STARTS);
        times.drain(..gone);
        if let Err(e) = shared::set_timings(&self.repo, &timings) {
            self.log.say(Who::Atlas, &format!("could not keep how long the drawing took: {e:#}"));
        }
        Ok(())
    }

    fn draw(&self, stamps: &Stamps, id: &str) -> Result<Drawn> {
        // the vision as it stands before it is read: a change while it is read draws again
        let vision = vision(&self.repo);
        let (read, witnesses) = reading(&self.repo, &self.opts)?;
        let findings = read.findings();
        let (graph, compared) = read.drawn();
        let flow = atlas_engine::flow::of(&graph);
        let opens = Opens { route: None, sight: None, served: Some(id.to_string()) };
        let data = Html::payload(&graph, &flow, &findings, compared.as_ref(), &opens)?;
        self.log.say(Who::Atlas, &format!("drew {}", page::summary(&graph)));
        let facts = witnesses.roots().first().map(|root| facts(root, &graph)).unwrap_or_default();
        Ok(Drawn { stamps: stamps.clone(), id: id.to_string(), data, facts, witnesses: Mutex::new(witnesses), vision })
    }

    /// The files of the working tree as Atlas reads them, now, each with what the last page drawn
    /// defines in it; a file the graph defines nothing in has no facts. The paths go from the
    /// listing's root, with `/` between their parts on every platform.
    fn files(&self) -> Result<Value> {
        let read = atlas_engine::language::read(&self.repo, &self.opts)?;
        let kept = self.kept();
        let files: Vec<Value> = read
            .listed_files()
            .iter()
            .map(|f| {
                let path = slashed(f);
                let facts = kept.as_ref().and_then(|d| d.facts.get(&path)).cloned().unwrap_or_default();
                json!({"path": path, "facts": facts})
            })
            .collect();
        Ok(json!({"root": read.root(), "drawn": kept.map(|d| d.id.clone()), "files": files}))
    }

    fn kept(&self) -> Option<Arc<Drawn>> {
        self.kept.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn progress(&self, phase: &'static str, error: Option<String>) {
        *self.progress.lock().unwrap_or_else(|e| e.into_inner()) = Progress { phase, error, since: Instant::now() };
    }

    /// How long the phase under way has taken so far, in seconds.
    fn since(&self) -> f64 {
        self.progress.lock().unwrap_or_else(|e| e.into_inner()).since.elapsed().as_secs_f64()
    }

    /// Which page the server has drawn, and whether it is drawing one: the open page compares the
    /// first with its own, and follows. Beside it, the phases a drawing goes through, the one it is in,
    /// how long it has been in it and how long each took the last time: how far the drawing is.
    fn drawn(&self) -> Value {
        let progress = self.progress.lock().unwrap_or_else(|e| e.into_inner());
        let phases = [READING];
        let expected = expected(&self.timings.lock().unwrap_or_else(|e| e.into_inner()), self.warm);
        // a drawing that panicked leaves the lock poisoned, and is no longer drawing
        let drawing = matches!(self.drawing.try_lock(), Err(std::sync::TryLockError::WouldBlock));
        json!({"drawn": self.kept().map(|d| d.id.clone()), "drawing": drawing,
            "phase": progress.phase, "error": progress.error, "phases": phases,
            "elapsed": progress.since.elapsed().as_secs_f64(), "expected": expected})
    }

    /// The working tree, looked at every second: when what the page's reading rests on no longer
    /// holds (`Drawn::stands`) and the working tree has stayed as it is for a second, the page is
    /// drawn anew, for the open page to follow. A file no reading asked about changes nothing. A
    /// drawing that fails is said once, and tried again when the files change again.
    fn watch(&self) {
        let (mut last, mut failed): (Option<Stamps>, Option<Stamps>) = (None, None);
        loop {
            let kept = self.kept();
            if kept.as_ref().is_some_and(|d| d.stands(&self.repo)) {
                last = None;
            } else if let Ok(now) = stamps(&self.repo, &self.opts) {
                if (kept.is_none() || last.as_ref() == Some(&now)) && failed.as_ref() != Some(&now) {
                    self.tend(now.clone(), &mut failed, |now| self.page(now));
                }
                last = Some(now);
            }
            std::thread::sleep(Duration::from_secs(1));
        }
    }

    /// Draw the working tree at `now` with `draw`. A drawing that fails, or panics, is said in the log
    /// and on the page (its progress), and `failed` keeps what it failed on: the watch goes on.
    fn tend(&self, now: Stamps, failed: &mut Option<Stamps>, draw: impl FnOnce(Stamps) -> Result<()>) {
        let fault = match http::guarded(|| draw(now.clone())) {
            Ok(Ok(())) => return,
            Ok(Err(e)) => format!("{e:#}"),
            Err(said) => format!("Atlas failed while drawing: {said}"),
        };
        self.log.say(Who::Atlas, &format!("could not draw: {fault}"));
        self.progress("failed", Some(fault));
        *failed = Some(now);
    }

    fn body(&self, req: &Request) -> Result<Value> {
        serde_json::from_slice(&req.body).context("the page sent no JSON")
    }

    /// A note the reader wrote, kept against the commit the working tree stands on.
    fn note(&self, req: &Request) -> Result<Value> {
        let mut note: Note = serde_json::from_value(self.body(req)?).context("a note has `on`, `label`, `view` and `text`")?;
        note.rev = changes::resolve(&self.repo, "HEAD").map(|(c, _)| c).unwrap_or_default();
        Ok(serde_json::to_value(shared::add(&self.repo, note)?)?)
    }

    /// The reader opens a resolved note again, with a new line for the agent.
    fn reopen(&self, req: &Request) -> Result<Value> {
        let body = self.body(req)?;
        let id = body.get("id").and_then(Value::as_u64).and_then(|id| u32::try_from(id).ok()).context("which note: a note's number")?;
        let line = body.get("text").and_then(Value::as_str).filter(|t| !t.trim().is_empty()).context("a reopened note has a new line")?;
        Ok(serde_json::to_value(shared::reopen(&self.repo, id, line)?)?)
    }
}

/// What happened to the notes, said in the log as it happens: a note written or reopened, read by
/// an agent, resolved. The agent writes through `atlas-code mcp`, another process, so the notes are
/// looked at every second.
fn watch_notes(repo: &Path, log: &Log) {
    let mut known: HashMap<u32, State> = shared::notes(repo).unwrap_or_default().into_iter().map(|n| (n.id, n.state)).collect();
    let mut unread = false;
    loop {
        std::thread::sleep(Duration::from_secs(1));
        let notes = match shared::notes(repo) {
            Ok(notes) => notes,
            Err(e) => {
                // said once, until the notes read again
                if !std::mem::replace(&mut unread, true) {
                    log.say(Who::Atlas, &format!("could not read the notes: {e:#}"));
                }
                continue;
            }
        };
        unread = false;
        for n in notes {
            let before = known.insert(n.id, n.state);
            if before == Some(n.state) {
                continue;
            }
            let last = n.text.lines().last().unwrap_or("");
            match (before, n.state) {
                (Some(State::Resolved), State::Open) => log.say(Who::You, &format!("reopened note {} on {}: {last}", n.id, n.label)),
                (_, State::Open) => log.say(Who::You, &format!("note {} on {}: {last}", n.id, n.label)),
                (_, State::Seen) => log.say(Who::Agent, &format!("read note {} on {}", n.id, n.label)),
                (_, State::Resolved) => {
                    let by: Vec<String> =
                        [n.commit.as_deref().map(|c| format!("in {}", shared::short(c))), n.issue.clone()].into_iter().flatten().collect();
                    let by = if by.is_empty() { String::new() } else { format!(" ({})", by.join(", ")) };
                    log.say(Who::Agent, &format!("resolved note {} on {}{by}", n.id, n.label))
                }
            }
        }
    }
}

/// The port a repository is served at when none is asked for: the same each time, so that a link
/// to a place in it (one an agent put in an issue) keeps working, and another repository's
/// apart. From its path, by FNV-1a, into 8700–8899.
fn own_port(repo: &Path) -> u16 {
    let path = atlas_engine::repo::Repo::real(repo);
    let hash = path.to_string_lossy().bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3));
    8700 + (hash % 200) as u16
}

/// The working tree as the page draws it, with what its reading rests on: beside what an agent has
/// in view, as two readings are drawn (A the working tree, B the vision), and alone when no vision is
/// kept.
fn reading(repo: &Path, opts: &Options) -> Result<(Reading, Witnesses)> {
    let (now, witnesses) = atlas_engine::parts::witnessed(&[repo.to_path_buf()], opts)?;
    let Some(p) = shared::vision(repo) else { return Ok((Reading::of(&[repo.to_path_buf()], now), witnesses)) };
    let envisioned = Named { rev: "vision".into(), short: "vision".into(), subject: p.title.clone() };
    let vision = changes::envisioned(repo, &p.files, opts)?.parts;
    Ok((Reading::compared(repo, (changes::worktree(), now), (envisioned, vision)), witnesses))
}

/// The vision an agent keeps beside the working tree, as it is now: its size and when it was written.
fn vision(repo: &Path) -> Stamp {
    std::fs::metadata(shared::vision_file(repo)).ok().map(|m| (m.len(), m.modified().ok()))
}

/// The working tree as far as Atlas lists it, with its `.atlasignore` and what an agent has in view
/// beside it: what a change is waited out on, and what a drawing that failed is kept by.
fn stamps(repo: &Path, opts: &Options) -> Result<Stamps> {
    let read = atlas_engine::language::read(repo, opts)?;
    let mut now = read.stamps();
    // `.atlasignore` by its path from the root, as the listed files are
    let decided = (PathBuf::from(atlas_engine::aside::FILE), read.root().join(atlas_engine::aside::FILE));
    for (key, file) in [decided, (shared::vision_file(repo), shared::vision_file(repo))] {
        if let Ok(m) = std::fs::metadata(&file) {
            now.insert(key, Some((m.len(), m.modified().ok())));
        }
    }
    Ok(now)
}

/// What `graph` defines in the files under `root`, by their paths from it: a library's code is not
/// the repository's, and has no file in it.
fn facts(root: &Path, graph: &atlas_engine::model::Graph) -> Facts {
    let mut facts = Facts::new();
    for n in graph.nodes.iter().filter(|n| n.kind != atlas_engine::model::NodeKind::External) {
        let Ok(rel) = Path::new(&n.file).strip_prefix(root) else { continue };
        let fact = json!({"id": n.id, "name": n.name, "kind": n.kind, "line": n.span.map(|s| s.0), "entry": n.entry.is_some()});
        facts.entry(slashed(rel)).or_default().push(fact);
    }
    facts
}

/// A path from a root, with `/` between its parts.
fn slashed(p: &Path) -> String {
    p.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/")
}

/// How many starts the expected time of a phase is taken from.
const STARTS: usize = 3;

/// What each phase of a start is expected to take: the longest it took at the last starts of the same
/// kind, with the index of their build of Atlas (`warm`) or without it.
fn expected(timings: &shared::Timings, warm: bool) -> BTreeMap<String, f64> {
    timings.of(warm).iter().filter_map(|(phase, times)| Some((phase.clone(), slowest(times)?))).collect()
}

/// The longest of the last `STARTS` of `times`. A bar that expects too little waits before its end and
/// looks stuck; one that expects too much is done early, which looks like what it is. So a phase is
/// expected to take as long as the slowest recent start, recent enough to follow a project that grows.
fn slowest(times: &[f64]) -> Option<f64> {
    times[times.len().saturating_sub(STARTS)..].iter().copied().reduce(f64::max)
}

/// The name a page knows its drawing by: the stamps it was drawn from, hashed.
fn name(stamps: &Stamps) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    stamps.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// The files two readings differ in, for the log: the first three, and how many more.
fn changed(before: &Stamps, now: &Stamps) -> String {
    let files: BTreeSet<&PathBuf> = before.keys().chain(now.keys()).filter(|f| before.get(*f) != now.get(*f)).collect();
    let names: Vec<String> = files.iter().take(3).map(|f| f.display().to_string()).collect();
    match files.len() {
        0..=3 => names.join(", "),
        n => format!("{}, and {} more", names.join(", "), n - 3),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A start is never expected to be quicker than the slowest of the last three of its kind, and a slow
    /// start further back no longer counts.
    #[test]
    fn a_start_is_expected_to_take_the_longest_of_the_last_few() {
        assert_eq!(slowest(&[]), None);
        assert_eq!(slowest(&[20.0, 0.3, 21.0]), Some(21.0));
        assert_eq!(slowest(&[90.0, 20.0, 22.0, 21.0]), Some(22.0));
    }

    /// A start is expected to take what starts of its kind took: one that reads everything is not hurried
    /// by the quick ones that found their index, nor a quick one held up by them. What an older file kept
    /// of both kinds together is no kind, and is forgotten.
    #[test]
    fn a_start_is_expected_to_take_what_starts_of_its_kind_took() {
        let timings = shared::Timings {
            cold: BTreeMap::from([("code".to_string(), vec![79.0, 81.0, 80.0])]),
            warm: BTreeMap::from([("code".to_string(), vec![0.04, 0.003, 0.006, 0.004])]),
        };
        assert_eq!(expected(&timings, false)["code"], 81.0);
        assert!(expected(&timings, true)["code"] < 0.01);
        let older: shared::Timings = serde_json::from_str(r#"{"code": [79.4, 0.04, 0.003]}"#).unwrap();
        assert_eq!(older, shared::Timings::default());
    }

    fn server() -> Server {
        Server {
            repo: PathBuf::new(),
            opts: Options::default(),
            hosts: vec!["localhost:8797".into()],
            log: Arc::new(Log::new()),
            kept: Mutex::new(None),
            progress: Mutex::new(Progress { phase: READING, error: None, since: Instant::now() }),
            drawing: Mutex::new(()),
            timings: Mutex::new(shared::Timings::default()),
            warm: false,
        }
    }

    /// A drawing that panics is a drawing that failed: the page's progress says so, with what the
    /// panic said, and no longer says it is drawing; the watch keeps what it failed on, and goes on to
    /// draw the next change.
    #[test]
    fn a_drawing_that_panics_is_shown_and_the_watch_goes_on() {
        let server = server();
        let (now, mut failed) = (Stamps::from([(PathBuf::from("spots.py"), None)]), None);
        server.tend(now.clone(), &mut failed, |_| {
            let _drawing = server.drawing.lock().unwrap();
            panic!("a reader made to panic")
        });
        let state = server.drawn();
        assert_eq!(state["phase"], "failed");
        assert!(state["error"].as_str().unwrap().contains("a reader made to panic"), "{state}");
        assert_eq!(state["drawing"], false, "{state}");
        assert_eq!(failed.as_ref(), Some(&now));
        let mut drawn = false;
        server.tend(Stamps::new(), &mut failed, |_| {
            drawn = true;
            Ok(())
        });
        assert!(drawn, "the next change is drawn");
    }

    #[test]
    fn root_answers_with_progress_before_any_picture_exists() {
        let server = server();
        let request = Request {
            method: "GET".into(),
            path: "/".into(),
            query: HashMap::new(),
            host: "localhost:8797".into(),
            own: false,
            body: vec![],
        };
        let answer = server.respond(&request);
        let page = String::from_utf8(answer.body).unwrap();
        assert_eq!(answer.status, "200 OK");
        assert!(page.contains("The first picture is taking shape") && page.contains("function progressOf"));
        assert!(!page.contains("/*__PROGRESS__*/"), "the loading page carries the measure of progress");
        assert!(!page.contains("http-equiv=\"refresh\""));
    }
}
