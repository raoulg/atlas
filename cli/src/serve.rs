//! `atlas serve`: the page of a repository, served on this machine and shared with an agent. The page
//! says what it shows, goes where an agent points it, and keeps the notes its reader writes on boxes
//! and lines; all three are files of `.atlas/` (`shared`), which `atlas mcp` reads and writes too.
//!
//! The page is the working tree's, and follows it: when a file Atlas reads changes, the server draws
//! the page anew and the open page reloads in the view it was in. Beside it, at `/history`, the
//! history of `atlas history`, so that two commits are a click away.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::{Hash, Hasher};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use atlas_engine::repo::Stamp;
use atlas_engine::{changes, Options};
use serde_json::{json, Value};

use crate::http::{reply, request, Answer, Request};
use crate::log::{Log, Who};
use crate::shared::{self, Note, State};

/// The working tree as far as Atlas reads it: each file by its size and the time it was written.
type Stamps = BTreeMap<PathBuf, Stamp>;

/// A page of the working tree, with the files it was drawn from and the name the page knows it by.
struct Drawn {
    stamps: Stamps,
    id: String,
    html: String,
}

struct Server {
    repo: PathBuf,
    opts: Options,
    hosts: Vec<String>,
    log: Arc<Log>,
    /// The page of the working tree last drawn.
    kept: Mutex<Option<Arc<Drawn>>>,
    /// Held while the page is drawn: it is drawn once, however many ask for it at a time.
    drawing: Mutex<()>,
}

/// Serve the page of `repo` at `addr` until the process stops, opening it in the browser when
/// `shown`.
pub fn serve(repo: &Path, addr: SocketAddr, opts: &Options, shown: bool) -> Result<()> {
    let listener = TcpListener::bind(addr).with_context(|| format!("serving at {addr}"))?;
    let at = listener.local_addr()?;
    let log = Arc::new(Log::new());
    let server = Arc::new(Server {
        repo: repo.to_path_buf(),
        opts: opts.clone(),
        hosts: vec![at.to_string(), format!("localhost:{}", at.port())],
        log: Arc::clone(&log),
        kept: Mutex::new(None),
        drawing: Mutex::new(()),
    });
    let url = format!("http://{at}/");
    log.start(&repo.display().to_string(), &url);
    let (watched, told) = (repo.to_path_buf(), Arc::clone(&log));
    std::thread::spawn(move || watch_notes(&watched, &told));
    let code = Arc::clone(&server);
    std::thread::spawn(move || code.watch());
    if shown {
        crate::open(url.as_ref());
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
            ("GET", "/") => self.page().map_or_else(Answer::failed, |d| Answer::page(d.html.clone())),
            ("GET", "/api/drawn") => Answer::json(self.drawn().to_string()),
            ("GET", "/export") => {
                self.export(req).map_or_else(Answer::failed, |(name, html)| Answer::download(format!("{name}.html"), html))
            }
            ("POST", "/api/shown") => json(self.body(req).and_then(|v| shared::set_shown(&self.repo, &v).map(|_| Value::Null))),
            ("GET", "/api/look") => shared::take_look(&self.repo).map_or_else(Answer::none, |v| {
                self.log.say(Who::Agent, &format!("look here: {v}"));
                Answer::json(v.to_string())
            }),
            ("GET", "/api/notes") => json(Ok(serde_json::to_value(shared::notes(&self.repo)).unwrap_or_default())),
            ("POST", "/api/notes") => json(self.note(req)),
            ("POST", "/api/notes/read") => json(self.read_answer(req)),
            _ => Answer::text("404 Not Found", format!("nothing at {} {}", req.method, req.path)),
        }
    }

    /// The page as `atlas view` writes it, opened in the view the reader has (`v`): a file to keep
    /// or send, that needs no server.
    fn export(&self, req: &Request) -> Result<(String, String)> {
        let view = req.query.get("v").map(|v| serde_json::from_str::<Value>(v)).transpose().context("the view is not JSON")?;
        let (name, html, _) = crate::page(&[self.repo.clone()], &self.opts, None, None, view.clone())?;
        self.log.say(Who::You, &format!("exported {name}.html, in the view {}", view.unwrap_or_default()));
        Ok((name, html))
    }

    /// The page of the working tree as its files are now: the one kept while none has changed, so
    /// that a reload, or a return from the history, is at once; else drawn anew.
    fn page(&self) -> Result<Arc<Drawn>> {
        let now = stamps(&self.repo, &self.opts)?;
        let _drawing = self.drawing.lock().unwrap_or_else(|e| e.into_inner());
        let kept = self.kept();
        match &kept {
            Some(d) if d.stamps == now => return Ok(Arc::clone(d)),
            Some(d) => self.log.say(Who::Atlas, &format!("changed {}", changed(&d.stamps, &now))),
            None => {}
        }
        let id = name(&now);
        let (html, summary) = crate::served_page(&self.repo, &self.opts, &id)?;
        self.log.say(Who::Atlas, &format!("drew {summary}"));
        let drawn = Arc::new(Drawn { stamps: now, id, html });
        *self.kept.lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::clone(&drawn));
        Ok(drawn)
    }

    fn kept(&self) -> Option<Arc<Drawn>> {
        self.kept.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Which page the server has drawn, and whether it is drawing one: the open page compares the
    /// first with its own, and follows.
    fn drawn(&self) -> Value {
        json!({"drawn": self.kept().map(|d| d.id.clone()), "drawing": self.drawing.try_lock().is_err()})
    }

    /// The working tree, looked at every second: when a file Atlas reads has changed and has stayed
    /// so for a second, the page is drawn anew, for the open page to follow. A drawing that fails is
    /// said once, and tried again when the files change again.
    fn watch(&self) {
        let (mut last, mut failed): (Option<Stamps>, Option<Stamps>) = (None, None);
        loop {
            if let Ok(now) = stamps(&self.repo, &self.opts) {
                let kept = self.kept().map(|d| d.stamps == now);
                if kept != Some(true) && (kept.is_none() || last.as_ref() == Some(&now)) && failed.as_ref() != Some(&now) {
                    if let Err(e) = self.page() {
                        self.log.say(Who::Atlas, &format!("could not draw: {e:#}"));
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

    /// The reader has read the answer to a note: it is closed.
    fn read_answer(&self, req: &Request) -> Result<Value> {
        let id = self.body(req)?.get("id").and_then(Value::as_u64).context("which note")? as u32;
        let closed = shared::change(&self.repo, id, |n| {
            if n.state == State::Answered {
                n.state = State::Closed;
            }
        })?;
        Ok(serde_json::to_value(closed)?)
    }
}

/// What happened to the notes, said in the log as it happens: a note written, read by an agent,
/// answered, its answer read. The agent writes through `atlas mcp`, another process, so the notes
/// are looked at every second.
fn watch_notes(repo: &Path, log: &Log) {
    let mut known: HashMap<u32, State> = shared::notes(repo).into_iter().map(|n| (n.id, n.state)).collect();
    loop {
        std::thread::sleep(Duration::from_secs(1));
        for n in shared::notes(repo) {
            if known.insert(n.id, n.state) == Some(n.state) {
                continue;
            }
            match n.state {
                State::Open => log.say(Who::You, &format!("note {} on {}: {}", n.id, n.label, n.text)),
                State::Seen => log.say(Who::Agent, &format!("read note {} on {}", n.id, n.label)),
                State::Answered => log.say(Who::Agent, &format!("answered note {}: {}", n.id, n.answer.as_deref().unwrap_or(""))),
                State::Closed => log.say(Who::You, &format!("read the answer to note {}", n.id)),
            }
        }
    }
}

/// The working tree as far as Atlas reads it: equal stamps are one page.
fn stamps(repo: &Path, opts: &Options) -> Result<Stamps> {
    Ok(atlas_engine::language::read(repo, opts)?.stamps())
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
