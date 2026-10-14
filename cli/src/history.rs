//! `atlas history`: a repository's history as a page in the browser, served on this machine. The
//! page lists the commits (`/api/log`) and, beside them, draws one revision or the union of two
//! (`/view`) in the viewer `atlas view` writes.
//!
//! The server answers GET requests over HTTP/1.1, one thread per connection, from the browser of
//! this machine only. Each revision is read once, by its commit, for as long as the server runs,
//! and from one run to the next it is kept in the cache (`changes::parts_at`); the working tree is
//! read anew on every request, since it changes under the server.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use atlas_engine::changes::{self, Named};
use atlas_engine::{Graph, Options};

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

/// A request as the server reads it: what it asks, where, with what in its query, and the address
/// the browser sent it to.
struct Request {
    method: String,
    path: String,
    query: HashMap<String, String>,
    host: String,
}

/// An answer: its status line, the type of its body, and the body.
struct Answer {
    status: &'static str,
    kind: &'static str,
    body: Vec<u8>,
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
            "/api/log" => self.log(req).map_or_else(Answer::failed, |json| Answer {
                status: "200 OK",
                kind: "application/json",
                body: json.into_bytes(),
            }),
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

impl Answer {
    fn page(html: String) -> Answer {
        Answer { status: "200 OK", kind: "text/html; charset=utf-8", body: html.into_bytes() }
    }

    fn text(status: &'static str, text: String) -> Answer {
        Answer { status, kind: "text/plain; charset=utf-8", body: text.into_bytes() }
    }

    /// What went wrong, in words, where the page would have been.
    fn failed(e: anyhow::Error) -> Answer {
        Answer::text("500 Internal Server Error", format!("{e:#}"))
    }
}

/// The request line and headers of one request; a body is not read.
fn request(conn: &TcpStream) -> Result<Request> {
    let mut reader = BufReader::new(conn);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut words = line.split_whitespace();
    let (method, target) = (words.next().unwrap_or("").to_string(), words.next().unwrap_or("/").to_string());
    let mut host = String::new();
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 || header.trim().is_empty() {
            break;
        }
        if let Some((_, value)) = header.split_once(':').filter(|(n, _)| n.trim().eq_ignore_ascii_case("host")) {
            host = value.trim().to_string();
        }
    }
    let (path, query) = target.split_once('?').unwrap_or((&target, ""));
    let query = query
        .split('&')
        .filter(|kv| !kv.is_empty())
        .map(|kv| kv.split_once('=').unwrap_or((kv, "")))
        .map(|(k, v)| (decoded(k), decoded(v)))
        .collect();
    Ok(Request { method, path: decoded(path), query, host })
}

/// A part of an address with its escapes (`%2F`, `+`) read back.
fn decoded(s: &str) -> String {
    let (bytes, mut out, mut i) = (s.as_bytes(), Vec::with_capacity(s.len()), 0);
    while i < bytes.len() {
        let hex = bytes.get(i + 1..i + 3).and_then(|h| std::str::from_utf8(h).ok()).and_then(|h| u8::from_str_radix(h, 16).ok());
        let (byte, used) = match (bytes[i], hex) {
            (b'%', Some(b)) => (b, 3),
            (b'+', _) => (b' ', 1),
            (b, _) => (b, 1),
        };
        out.push(byte);
        i += used;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Write an answer and close the connection.
fn reply(mut conn: TcpStream, a: Answer) -> Result<()> {
    write!(
        conn,
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        a.status,
        a.kind,
        a.body.len()
    )?;
    conn.write_all(&a.body)?;
    Ok(conn.flush()?)
}
