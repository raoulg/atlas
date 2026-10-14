//! `atlas serve`: the page of a repository, served on this machine and shared with an agent. The page
//! says what it shows, goes where an agent points it, and keeps the notes its reader writes on boxes
//! and lines; all three are files of `.atlas/` (`shared`), which `atlas mcp` reads and writes too.
//!
//! The page is the working tree's, read anew on every load; beside it, at `/history`, the history of
//! `atlas history`, so that two commits are a click away.

use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use atlas_engine::{changes, Options};
use serde_json::Value;

use crate::http::{reply, request, Answer, Request};
use crate::shared::{self, Note, State};

struct Server {
    repo: PathBuf,
    opts: Options,
    hosts: Vec<String>,
}

/// Serve the page of `repo` at `addr` until the process stops, opening it in the browser when
/// `shown`.
pub fn serve(repo: &Path, addr: SocketAddr, opts: &Options, shown: bool) -> Result<()> {
    let listener = TcpListener::bind(addr).with_context(|| format!("serving at {addr}"))?;
    let at = listener.local_addr()?;
    let server =
        Arc::new(Server { repo: repo.to_path_buf(), opts: opts.clone(), hosts: vec![at.to_string(), format!("localhost:{}", at.port())] });
    let url = format!("http://{at}/");
    eprintln!("atlas serve of {} at {url} · an agent shares it through `atlas mcp`: shown, look, notes, answer", repo.display());
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
            ("GET", "/") => crate::served_page(&self.repo, &self.opts).map_or_else(Answer::failed, Answer::page),
            ("POST", "/api/shown") => json(self.body(req).and_then(|v| shared::set_shown(&self.repo, &v).map(|_| Value::Null))),
            ("GET", "/api/look") => shared::take_look(&self.repo).map_or_else(Answer::none, |v| Answer::json(v.to_string())),
            ("GET", "/api/notes") => json(Ok(serde_json::to_value(shared::notes(&self.repo)).unwrap_or_default())),
            ("POST", "/api/notes") => json(self.note(req)),
            ("POST", "/api/notes/read") => json(self.read_answer(req)),
            _ => Answer::text("404 Not Found", format!("nothing at {} {}", req.method, req.path)),
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
