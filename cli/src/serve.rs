//! `atlas serve`: the page of a repository, served on this machine and shared with an agent. The page
//! says what it shows, goes where an agent points it, and keeps the notes its reader writes on boxes
//! and lines; all three are files of `.atlas/` (`shared`), which `atlas mcp` reads and writes too.
//!
//! The page is the working tree's, read anew on every load; beside it, at `/history`, the history of
//! `atlas history`, so that two commits are a click away.

use std::collections::HashMap;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use atlas_engine::{changes, Options};
use serde_json::Value;

use crate::http::{reply, request, Answer, Request};
use crate::log::{Log, Who};
use crate::shared::{self, Note, State};

struct Server {
    repo: PathBuf,
    opts: Options,
    hosts: Vec<String>,
    log: Arc<Log>,
    /// The page of the working tree last drawn, by the fingerprint of the files it was drawn from.
    kept: Mutex<Option<(u64, String)>>,
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
    });
    let url = format!("http://{at}/");
    log.start(&repo.display().to_string(), &url);
    let (watched, told) = (repo.to_path_buf(), Arc::clone(&log));
    std::thread::spawn(move || watch_notes(&watched, &told));
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
            ("GET", "/") => self.page().map_or_else(Answer::failed, Answer::page),
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

    /// The page of the working tree: the one drawn before while no file Atlas reads has changed, so
    /// that a reload, or a return from the history, is at once; else drawn now.
    fn page(&self) -> Result<String> {
        let now = fingerprint(&self.repo, &self.opts);
        if let Some((fp, html)) = self.kept.lock().ok().and_then(|k| k.clone()) {
            if Some(fp) == now {
                return Ok(html);
            }
        }
        let (html, summary) = crate::served_page(&self.repo, &self.opts)?;
        self.log.say(Who::Atlas, &format!("drew {summary}"));
        if let (Some(fp), Ok(mut kept)) = (now, self.kept.lock()) {
            *kept = Some((fp, html.clone()));
        }
        Ok(html)
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

/// The working tree as far as Atlas reads it, as a fingerprint: one fingerprint is one page.
fn fingerprint(repo: &Path, opts: &Options) -> Option<u64> {
    atlas_engine::language::read(repo, opts).ok().map(|r| r.fingerprint())
}
