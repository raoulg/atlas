//! `atlas-code history`: a repository's history as a page in the browser, served on this machine. The
//! page lists the commits (`/api/log`) and, beside them, draws one revision or the union of two
//! (`/view`) in the viewer `atlas-code view` writes. `atlas-code history` answers it at its root, `atlas-code serve`
//! beside its own page, at `/history`.
//!
//! The server answers GET requests over HTTP/1.1, one thread per connection, from the browser of
//! this machine only. A revision is read once, by its commit, and kept in the cache
//! (`changes::parts_at`); the working tree is read anew on every request, since it changes under
//! the server.

use std::net::{SocketAddr, TcpListener};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use atlas_engine::changes::{self, Named};
use atlas_engine::{Graph, Options};

use crate::http::{self, Answer, Request};
use crate::page::{Html, Opens, Reading};

/// The page of the history: the commits beside the viewer.
const PAGE: &str = include_str!("../../viewer/history.html");

/// The server of `atlas-code history`: the repository, how to read it, and the addresses the browser
/// reaches it by.
struct Server {
    repo: PathBuf,
    opts: Options,
    hosts: Vec<String>,
}

/// Serve the history of `repo` at `addr` until the process stops, opening it in the browser when
/// `shown`.
pub fn serve(repo: &Path, addr: SocketAddr, opts: &Options, shown: bool) -> Result<()> {
    let listener = TcpListener::bind(addr).with_context(|| format!("serving at {addr}"))?;
    let at = listener.local_addr()?;
    let server = Server { repo: repo.to_path_buf(), opts: opts.clone(), hosts: vec![at.to_string(), format!("localhost:{}", at.port())] };
    let url = format!("http://{at}/");
    eprintln!("atlas-code history of {} at {url}", repo.display());
    if shown {
        crate::page::open(url.as_ref());
    }
    http::listen(listener, |e| eprintln!("atlas-code history: {e}"), move |req| server.respond(req));
    Ok(())
}

impl Server {
    /// The answer to a request. A request sent to another address (a page of another site that
    /// renamed itself to this machine) is refused.
    fn respond(&self, req: &Request) -> Answer {
        if !self.hosts.contains(&req.host) {
            return Answer::text("403 Forbidden", "atlas-code history answers only at the address it serves".into());
        }
        if req.method != "GET" {
            return Answer::text("405 Method Not Allowed", "atlas-code history only reads".into());
        }
        respond(&self.repo, &self.opts, req, "/").unwrap_or_else(|| Answer::text("404 Not Found", format!("nothing at {}", req.path)))
    }
}

/// The answer to a GET of the history of `repo`: its page at `at`, the commits (`/api/log`), a
/// viewer page (`/view`); none for any other request. A viewer page reads a revision, an analysis,
/// and is drawn only for the history's own page or its reader (`Request::own`): the frame the page
/// opens it in, and the link it opens it by, carry the browser's word for it.
pub fn respond(repo: &Path, opts: &Options, req: &Request, at: &str) -> Option<Answer> {
    if req.method != "GET" {
        return None;
    }
    Some(match req.path.as_str() {
        p if p == at => {
            Answer::page(PAGE.replacen("/*__FONTS__*/", crate::page::FONTS, 1).replacen("/*__PALETTE__*/", crate::page::PALETTE, 1))
        }
        "/api/log" => log(repo, req).map_or_else(Answer::failed, Answer::json),
        "/view" if !req.own => Answer::text("403 Forbidden", "a revision is drawn for the history's own page only".into()),
        "/view" => view(repo, opts, req).map_or_else(Answer::failed, Answer::page),
        _ => return None,
    })
}

/// The history as JSON: of every branch and tag unless `all=0`, `n` commits (300) after the newest
/// `skip` (none).
fn log(repo: &Path, req: &Request) -> Result<String> {
    let all = req.query.get("all").map_or(true, |v| v != "0");
    let count = |k: &str, or: usize| req.query.get(k).and_then(|n| n.parse().ok()).unwrap_or(or);
    Ok(serde_json::to_string(&changes::log(repo, all, count("n", 300), count("skip", 0))?)?)
}

/// The viewer page of revision `b` (the working tree by default), or of the union of `a` and `b`.
fn view(repo: &Path, opts: &Options, req: &Request) -> Result<String> {
    let b = reading(repo, opts, req.query.get("b").map_or("worktree", String::as_str))?;
    let read = beside(repo, opts, req.query.get("a").map(String::as_str), b)?;
    let findings = read.findings();
    let (graph, compared) = read.drawn();
    let flow = atlas_engine::flow::of(&graph);
    Ok(Html::of(&graph, &flow, &findings, compared.as_ref(), &Opens::default())?.into())
}

/// One reading, or revision `a` beside it, to be drawn as one.
fn beside(repo: &Path, opts: &Options, a: Option<&str>, b: (Named, Vec<Graph>)) -> Result<Reading> {
    match a {
        None => Ok(Reading::of(&[repo.to_path_buf()], b.1)),
        Some(a) => Ok(Reading::compared(repo, reading(repo, opts, a)?, b)),
    }
}

/// The parts of a revision, read once per commit and kept; of the working tree (`worktree`), read
/// now.
fn reading(repo: &Path, opts: &Options, rev: &str) -> Result<(Named, Vec<Graph>)> {
    if rev == "worktree" {
        return Ok((changes::worktree(), changes::parts_at(repo, None, opts)?));
    }
    let (commit, named) = changes::resolve(repo, rev)?;
    Ok((named, changes::parts_at(repo, Some(&commit), opts)?))
}
