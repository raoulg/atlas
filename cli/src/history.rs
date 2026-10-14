//! `atlas history`: a repository's history as a page in the browser, served on this machine. The
//! page lists the commits (`/api/log`) and, beside them, draws one revision or the union of two
//! (`/view`) in the viewer `atlas view` writes.
//!
//! The server answers GET requests over HTTP/1.1, one thread per connection, from the browser of
//! this machine only. Each revision is read once, by its commit, for as long as the server runs,
//! and from one run to the next it is kept in the cache (`changes::parts_at`); the working tree is
//! read anew on every request, since it changes under the server.

use std::collections::HashMap;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use atlas_engine::changes::{self, Named};
use atlas_engine::{Graph, Options};

use crate::http::{reply, request, Answer, Request};

/// The page the server answers at its root: the history beside the viewer.
const PAGE: &str = include_str!("../../viewer/history.html");

/// What the server holds while it runs: the repository, how to read it, the addresses the browser
/// reaches it by, and every revision it has read, by commit.
struct Server {
    repo: PathBuf,
    opts: Options,
    hosts: Vec<String>,
    read: Mutex<HashMap<String, Arc<Vec<Graph>>>>,
}

/// Serve the history of `repo` at `addr` until the process stops, opening it in the browser when
/// `shown`.
pub fn serve(repo: &Path, addr: SocketAddr, opts: &Options, shown: bool) -> Result<()> {
    let listener = TcpListener::bind(addr).with_context(|| format!("serving at {addr}"))?;
    let at = listener.local_addr()?;
    let server = Arc::new(Server {
        repo: repo.to_path_buf(),
        opts: opts.clone(),
        hosts: vec![at.to_string(), format!("localhost:{}", at.port())],
        read: Mutex::new(HashMap::new()),
    });
    let url = format!("http://{at}/");
    eprintln!("atlas history of {} at {url}", repo.display());
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
    /// Read one request from `conn` and write its answer.
    fn answer(&self, conn: TcpStream) -> Result<()> {
        let req = request(&conn)?;
        reply(conn, self.respond(&req))
    }

    /// The answer to a request: the page, the history, a viewer page. A request sent to another
    /// address (a page of another site that renamed itself to this machine) is refused.
    fn respond(&self, req: &Request) -> Answer {
        if !self.hosts.contains(&req.host) {
            return Answer::text("403 Forbidden", "atlas history answers only at the address it serves".into());
        }
        if req.method != "GET" {
            return Answer::text("405 Method Not Allowed", "atlas history only reads".into());
        }
        match req.path.as_str() {
            "/" => Answer::page(PAGE.replacen("/*__FONTS__*/", crate::FONTS, 1)),
            "/api/log" => self.log(req).map_or_else(Answer::failed, Answer::json),
            "/view" => self.view(req).map_or_else(Answer::failed, Answer::page),
            _ => Answer::text("404 Not Found", format!("nothing at {}", req.path)),
        }
    }

    /// The history as JSON: of every branch and tag unless `all=0`, `n` commits (300) after the
    /// newest `skip` (none).
    fn log(&self, req: &Request) -> Result<String> {
        let all = req.query.get("all").map_or(true, |v| v != "0");
        let count = |k: &str, or: usize| req.query.get(k).and_then(|n| n.parse().ok()).unwrap_or(or);
        Ok(serde_json::to_string(&changes::log(&self.repo, all, count("n", 300), count("skip", 0))?)?)
    }

    /// The viewer page of revision `b` (the working tree by default), or of the union of `a` and `b`.
    fn view(&self, req: &Request) -> Result<String> {
        let b = self.reading(req.query.get("b").map_or("worktree", String::as_str))?;
        let d = self.drawing(req.query.get("a").map(String::as_str), b)?;
        crate::viewer(&d, serde_json::Map::new())
    }

    /// One reading drawn, or the union of revision `a` and that reading.
    fn drawing(&self, a: Option<&str>, b: (Named, Vec<Graph>)) -> Result<crate::Drawing> {
        match a {
            None => Ok(crate::single(&[self.repo.clone()], b.1)),
            Some(a) => crate::union(&self.repo, self.reading(a)?, b),
        }
    }

    /// The parts of a revision, read once per commit, or kept from a run before; of the working
    /// tree (`worktree`), read now.
    fn reading(&self, rev: &str) -> Result<(Named, Vec<Graph>)> {
        if rev == "worktree" {
            return Ok((changes::worktree(), changes::parts_at(&self.repo, None, &self.opts)?));
        }
        let (commit, named) = changes::resolve(&self.repo, rev)?;
        let known = self.read.lock().map_err(|_| anyhow::anyhow!("a reading failed"))?.get(&commit).cloned();
        let parts = match known {
            Some(p) => p,
            None => {
                let p = Arc::new(changes::parts_at(&self.repo, Some(&commit), &self.opts)?);
                self.read.lock().map_err(|_| anyhow::anyhow!("a reading failed"))?.insert(commit, Arc::clone(&p));
                p
            }
        };
        Ok((named, parts.as_ref().clone()))
    }
}
