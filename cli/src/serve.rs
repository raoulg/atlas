//! `atlas serve`: the page of a repository, served on this machine and shared with an agent. The page
//! says what it shows, goes where an agent points it, and keeps the notes its reader writes on boxes
//! and lines; all three are files of `.atlas/` (`shared`), which `atlas mcp` reads and writes too.
//!
//! The page is the working tree's, and follows it: when a file Atlas reads changes, the server publishes
//! a complete snapshot and the open page draws it in the view it was in. Beside it, at `/history`, the
//! history of `atlas history`, so that two commits are a click away. At `/api/files`, the files Atlas
//! reads, each with what the graph defines in it: the tree a file browser shows beside the page.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::{Hash, Hasher};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use atlas_engine::changes::{self, Named};
use atlas_engine::repo::Stamp;
use atlas_engine::sight::Sight;
use atlas_engine::Options;
use serde_json::{json, Value};

use crate::http::{reply, request, Answer, Request};
use crate::log::{Log, Who};
use crate::page::{self, Html, Opens, Reading};
use crate::shared::{self, Note, State};

const LOADING: &str = include_str!("../../viewer/loading.html");

/// The loading page, with the measure of progress the page shares.
fn loading() -> String {
    LOADING.replacen("/*__PROGRESS__*/", page::PROGRESS, 1).replacen("/*__PALETTE__*/", page::PALETTE, 1)
}

/// The working tree as far as Atlas reads it: each file by its size and the time it was written.
type Stamps = BTreeMap<PathBuf, Stamp>;

/// A page of the working tree, with the files it was drawn from, the name the page knows it by, and
/// what its graph defines in each file.
struct Drawn {
    stamps: Stamps,
    id: String,
    data: String,
    complete: bool,
    facts: Facts,
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
        progress: Mutex::new(Progress { phase: "code", error: None, since: Instant::now() }),
        drawing: Mutex::new(()),
        timings: Mutex::new(shared::timings(repo)),
        warm: opts.cache.as_ref().and_then(|c| c.workspace(repo)).is_some_and(|c| c.holds_any()),
    });
    let url = format!("http://{at}/");
    // a note is for the work at hand: this page starts without the ones resolved before it
    shared::forget_resolved(repo)?;
    shared::set_serving(repo, &url)?;
    log.start(&repo.display().to_string(), &url);
    let (watched, told) = (repo.to_path_buf(), Arc::clone(&log));
    std::thread::spawn(move || watch_notes(&watched, &told));
    let code = Arc::clone(&server);
    std::thread::spawn(move || code.watch());
    if shown {
        page::open(url.as_ref());
    }
    for conn in listener.incoming().flatten() {
        let server = Arc::clone(&server);
        std::thread::spawn(move || server.answer(conn));
    }
    Ok(())
}

impl Server {
    fn answer(&self, conn: TcpStream) -> Result<()> {
        let req = request(&conn)?;
        reply(conn, self.respond(&req))
    }

    /// The page, and what it and an agent share. A request that changes something comes from the
    /// page itself, which says so (`X-Atlas`).
    fn respond(&self, req: &Request) -> Answer {
        if !self.hosts.contains(&req.host) {
            return Answer::text("403 Forbidden", "atlas serve answers only at the address it serves".into());
        }
        if req.method == "POST" && !req.own {
            return Answer::text("403 Forbidden", "atlas serve takes changes from its own page only".into());
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
            ("GET", "/api/look") => shared::take_look(&self.repo).map_or_else(Answer::none, |look| {
                let said = serde_json::to_string(&look).unwrap_or_default();
                self.log.say(Who::Agent, &format!("look here: {said}"));
                Answer::json(said)
            }),
            ("GET", "/api/notes") => json(Ok(serde_json::to_value(shared::notes(&self.repo)).unwrap_or_default())),
            ("GET", "/api/annotations") => json(Ok(serde_json::to_value(shared::annotations(&self.repo)).unwrap_or_default())),
            ("POST", "/api/notes") => json(self.note(req)),
            ("POST", "/api/notes/reopen") => json(self.reopen(req)),
            _ => Answer::text("404 Not Found", format!("nothing at {} {}", req.method, req.path)),
        }
    }

    /// The page as `atlas view` writes it, opened in the sight the reader has (`v`): a file to keep
    /// or send, that needs no server.
    fn export(&self, req: &Request) -> Result<(String, String)> {
        let sight = req.query.get("v").map(|v| v.parse::<Sight>()).transpose().context("the view is not one the page writes")?;
        let said = serde_json::to_string(sight.as_ref().unwrap_or(&Sight::default()))?;
        let read = page::reading(&[self.repo.clone()], &self.opts, None, None)?;
        let findings = read.findings();
        let (graph, compared) = read.drawn();
        let flow = atlas_engine::flow::of(&graph);
        let html = Html::of(&graph, &flow, &findings, compared.as_ref(), &Opens { route: None, sight, served: None, preliminary: false })?;
        self.log.say(Who::You, &format!("exported {}.html, in the view {said}", graph.root));
        Ok((graph.root, html.into()))
    }

    /// Draw the repository's own code first, then the complete reading. Each page is available as
    /// soon as it is drawn; the open browser follows the next one by its drawing id.
    fn page(&self, now: Stamps) -> Result<()> {
        let _drawing = self.drawing.lock().unwrap_or_else(|e| e.into_inner());
        let kept = self.kept();
        match &kept {
            Some(d) if d.stamps == now && d.complete => return Ok(()),
            Some(d) => self.log.say(Who::Atlas, &format!("changed {}", changed(&d.stamps, &now))),
            None => {}
        }
        let id = name(&now);
        self.progress("code", None);
        let mut took = BTreeMap::new();
        if self.opts.libraries > 0 {
            let mut local = self.opts.clone();
            local.libraries = 0;
            match self.draw(&now, &format!("{id}-local"), &local, true) {
                Ok(d) => *self.kept.lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(d)),
                Err(e) => self.log.say(Who::Atlas, &format!("could not draw local code: {e:#}")),
            }
            took.insert("code", self.since());
            self.progress("libraries", None);
        }
        let drawn = self.draw(&now, &id, &self.opts, false)?;
        took.insert(if self.opts.libraries > 0 { "libraries" } else { "code" }, self.since());
        *self.kept.lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(drawn));
        self.progress("ready", None);
        // what a start takes is what the bars are shown for: a drawing after a change reads most of
        // it from the cache, and would make the next start look quick
        if kept.is_some() {
            return Ok(());
        }
        let mut timings = self.timings.lock().unwrap_or_else(|e| e.into_inner());
        for (phase, secs) in took {
            let times = timings.of_mut(self.warm).entry(phase.to_string()).or_default();
            times.push(secs);
            let gone = times.len().saturating_sub(STARTS);
            times.drain(..gone);
        }
        if let Err(e) = shared::set_timings(&self.repo, &timings) {
            self.log.say(Who::Atlas, &format!("could not keep how long the drawing took: {e:#}"));
        }
        Ok(())
    }

    fn draw(&self, stamps: &Stamps, id: &str, opts: &Options, preliminary: bool) -> Result<Drawn> {
        let read = reading(&self.repo, opts)?;
        let findings = read.findings();
        let (graph, compared) = read.drawn();
        let flow = atlas_engine::flow::of(&graph);
        let opens = Opens { route: None, sight: None, served: Some(id.to_string()), preliminary };
        let data = Html::payload(&graph, &flow, &findings, compared.as_ref(), &opens)?;
        self.log.say(Who::Atlas, &format!("drew {}{}", if preliminary { "local code · " } else { "" }, page::summary(&graph)));
        let facts = facts(&atlas_engine::language::read(&self.repo, opts)?, &graph);
        Ok(Drawn { stamps: stamps.clone(), id: id.to_string(), data, complete: !preliminary, facts })
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
        let phases: &[&str] = if self.opts.libraries > 0 { &["code", "libraries"] } else { &["code"] };
        let expected = expected(&self.timings.lock().unwrap_or_else(|e| e.into_inner()), self.warm);
        json!({"drawn": self.kept().map(|d| d.id.clone()), "drawing": self.drawing.try_lock().is_err(),
            "phase": progress.phase, "error": progress.error, "phases": phases,
            "elapsed": progress.since.elapsed().as_secs_f64(), "expected": expected})
    }

    /// The working tree, looked at every second: when a file Atlas reads has changed and has stayed
    /// so for a second, the page is drawn anew, for the open page to follow. A drawing that fails is
    /// said once, and tried again when the files change again.
    fn watch(&self) {
        let (mut last, mut failed): (Option<Stamps>, Option<Stamps>) = (None, None);
        loop {
            if let Ok(now) = stamps(&self.repo, &self.opts) {
                let kept = self.kept().map(|d| d.stamps == now && d.complete);
                if kept != Some(true) && (kept.is_none() || last.as_ref() == Some(&now)) && failed.as_ref() != Some(&now) {
                    if let Err(e) = self.page(now.clone()) {
                        self.log.say(Who::Atlas, &format!("could not draw: {e:#}"));
                        self.progress("failed", Some(format!("{e:#}")));
                        failed = Some(now.clone());
                    }
                }
                last = Some(now);
            }
            std::thread::sleep(Duration::from_secs(1));
        }
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
        let id = body.get("id").and_then(Value::as_u64).context("which note")? as u32;
        let line = body.get("text").and_then(Value::as_str).filter(|t| !t.trim().is_empty()).context("a reopened note has a new line")?;
        Ok(serde_json::to_value(shared::reopen(&self.repo, id, line)?)?)
    }
}

/// What happened to the notes, said in the log as it happens: a note written or reopened, read by
/// an agent, resolved. The agent writes through `atlas mcp`, another process, so the notes are
/// looked at every second.
fn watch_notes(repo: &Path, log: &Log) {
    let mut known: HashMap<u32, State> = shared::notes(repo).into_iter().map(|n| (n.id, n.state)).collect();
    loop {
        std::thread::sleep(Duration::from_secs(1));
        for n in shared::notes(repo) {
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
                    let by: Vec<String> = [n.commit.as_ref().map(|c| format!("in {}", &c[..c.len().min(7)])), n.issue.clone()]
                        .into_iter()
                        .flatten()
                        .collect();
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

/// The working tree as the page draws it: beside what an agent has in view, as two readings are
/// drawn (A the working tree, B the vision), and alone when no vision is kept.
fn reading(repo: &Path, opts: &Options) -> Result<Reading> {
    let Some(p) = shared::vision(repo) else { return page::reading(&[repo.to_path_buf()], opts, None, None) };
    let envisioned = Named { rev: "vision".into(), short: "vision".into(), subject: p.title.clone() };
    let now = atlas_engine::parts::analyze_parts(&[repo.to_path_buf()], opts)?;
    Ok(Reading::compared(repo, (changes::worktree(), now), (envisioned, changes::envisioned(repo, &p.files, opts)?.parts)))
}

/// The working tree as far as Atlas reads it, and what an agent has in view beside it: equal stamps
/// are one page.
fn stamps(repo: &Path, opts: &Options) -> Result<Stamps> {
    let mut now = atlas_engine::language::read(repo, opts)?.stamps();
    let vision = shared::vision_file(repo);
    if let Ok(m) = std::fs::metadata(&vision) {
        now.insert(vision, Some((m.len(), m.modified().ok())));
    }
    Ok(now)
}

/// What `graph` defines in the files of `read`, by their paths from its root: a library's code is
/// not the repository's, and has no file in it.
fn facts(read: &atlas_engine::repo::Repo, graph: &atlas_engine::model::Graph) -> Facts {
    let mut facts = Facts::new();
    for n in graph.nodes.iter().filter(|n| n.kind != atlas_engine::model::NodeKind::External) {
        let Ok(rel) = Path::new(&n.file).strip_prefix(read.root()) else { continue };
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

    #[test]
    fn root_answers_with_progress_before_any_picture_exists() {
        let server = Server {
            repo: PathBuf::new(),
            opts: Options::default(),
            hosts: vec!["localhost:8797".into()],
            log: Arc::new(Log::new()),
            kept: Mutex::new(None),
            progress: Mutex::new(Progress { phase: "code", error: None, since: Instant::now() }),
            drawing: Mutex::new(()),
            timings: Mutex::new(shared::Timings::default()),
            warm: false,
        };
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
