//! `atlas-code history`: a repository's history as a page in the browser, served on this machine. The
//! page lists the commits (`/api/log`) and, beside them, draws one revision or the union of two
//! (`/view`) in the viewer `atlas-code view` writes, or shows a revision's files (`/api/tree`, `/file`),
//! read from its commit and never from the working tree unless it is the working tree that is asked.
//! A few git commands the page offers on a commit or on HEAD's branch (`/api/git`) run once the reader
//! has confirmed them in the page. `atlas-code history` answers it at its root, `atlas-code serve` beside
//! its own page, at `/history`.
//!
//! The server answers over HTTP/1.1, one thread per connection, from the browser of this machine
//! only. A revision is read once, by its commit, and kept in the cache (`changes::parts_at`); the
//! working tree is read anew on every request, since it changes under the server.

use std::net::{SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, ensure, Context, Result};
use atlas_engine::changes::{self, Named};
use atlas_engine::{Graph, Options};
use base64::Engine as _;
use serde_json::{json, Value};

use crate::http::{self, Answer, Request};
use crate::page::{Html, Opens, Reading};

/// The page of the history: the commits beside the viewer.
const PAGE: &str = include_str!("../../viewer/history.html");

/// The page one file of a revision is shown in.
const FILE: &str = include_str!("../../viewer/file.html");

/// The largest file shown as text.
const TEXT: usize = 2 << 20;

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
        respond(&self.repo, &self.opts, req, "/").unwrap_or_else(|| Answer::text("404 Not Found", format!("nothing at {}", req.path)))
    }
}

/// The answer to a request of the history of `repo`: its page at `at`, the commits (`/api/log`), a
/// viewer page (`/view`), a revision's files (`/api/tree`) and one of them (`/file`), and a git command
/// the reader confirmed (`POST /api/git`); none for any other request. What reads a revision or runs
/// git is answered only for the history's own page or its reader (`Request::own`): the frame the page
/// opens it in, and the link it opens it by, carry the browser's word for it.
pub fn respond(repo: &Path, opts: &Options, req: &Request, at: &str) -> Option<Answer> {
    Some(match (req.method.as_str(), req.path.as_str()) {
        ("GET", p) if p == at => {
            Answer::page(PAGE.replacen("/*__FONTS__*/", crate::page::FONTS, 1).replacen("/*__PALETTE__*/", crate::page::PALETTE, 1))
        }
        ("GET", "/api/log") => log(repo, req).map_or_else(Answer::failed, Answer::json),
        ("GET", "/view" | "/api/tree" | "/file") | ("POST", "/api/git") if !req.own => {
            Answer::text("403 Forbidden", "a revision is read, and git is run, for the history's own page only".into())
        }
        ("GET", "/view") => view(repo, opts, req).map_or_else(Answer::failed, Answer::page),
        ("GET", "/api/tree") => tree(repo, req).map_or_else(Answer::failed, |t| Answer::json(t.to_string())),
        ("GET", "/file") => file(repo, req).map_or_else(Answer::failed, Answer::page),
        ("POST", "/api/git") => run(repo, req).map_or_else(Answer::failed, |t| Answer::json(t.to_string())),
        _ => return None,
    })
}

/// The history as JSON: of every branch and tag unless `all=0`, `n` commits (300) after the newest
/// `skip` (none), with the branch HEAD is on and what pushing and pulling it run.
fn log(repo: &Path, req: &Request) -> Result<String> {
    let all = req.query.get("all").map_or(true, |v| v != "0");
    let count = |k: &str, or: usize| req.query.get(k).and_then(|n| n.parse().ok()).unwrap_or(or);
    let mut log = serde_json::to_value(changes::log(repo, all, count("n", 300), count("skip", 0))?)?;
    log["branch"] = Branch::of(repo).json();
    Ok(log.to_string())
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
    Ok(match At::of(repo, rev)? {
        At::Tree => (changes::worktree(), changes::parts_at(repo, None, opts)?),
        At::Commit(commit, named) => {
            let parts = changes::parts_at(repo, Some(&commit), opts)?;
            (named, parts)
        }
    })
}

/// Where a revision's files are read: the working tree as it is now, or a commit's tree.
enum At {
    Tree,
    /// The commit by its full hash, and as the page names it.
    Commit(String, Named),
}

impl At {
    fn of(repo: &Path, rev: &str) -> Result<At> {
        if rev == "worktree" {
            return Ok(At::Tree);
        }
        let (commit, named) = changes::resolve(repo, rev)?;
        Ok(At::Commit(commit, named))
    }

    /// The files of the revision below `repo`, by their paths from it with `/` between the parts: of
    /// a commit, the files of its tree; of the working tree, the files git keeps or would keep.
    fn files(&self, repo: &Path) -> Result<Vec<String>> {
        let listed = match self {
            At::Tree => git(repo, &["ls-files", "-z", "--cached", "--others", "--exclude-standard"])?,
            At::Commit(c, _) => git(repo, &["ls-tree", "-r", "-z", "--name-only", c])?,
        };
        let mut files: Vec<String> = listed
            .split(|b| *b == 0)
            .filter(|p| !p.is_empty())
            .map(|p| String::from_utf8_lossy(p).into_owned())
            .filter(|p| matches!(self, At::Commit(..)) || repo.join(p).is_file())
            .collect();
        files.sort();
        files.dedup();
        Ok(files)
    }

    /// The content of `path`, one of `files`.
    fn read(&self, repo: &Path, path: &str) -> Result<Vec<u8>> {
        match self {
            At::Tree => std::fs::read(repo.join(path)).with_context(|| format!("reading {path}")),
            At::Commit(c, _) => git(repo, &["cat-file", "blob", &format!("{c}:./{path}")]),
        }
    }
}

/// The files of revision `rev` (the working tree by default).
fn tree(repo: &Path, req: &Request) -> Result<Value> {
    let rev = req.query.get("rev").map_or("worktree", String::as_str);
    Ok(json!({"files": At::of(repo, rev)?.files(repo)?}))
}

/// The page of one file of revision `rev`: markdown rendered, with the pictures it shows and links to
/// the revision's other files; a picture; other text as it is.
fn file(repo: &Path, req: &Request) -> Result<String> {
    let rev = req.query.get("rev").map_or("worktree", String::as_str);
    let path = req.query.get("path").context("which file: `path`")?;
    let at = At::of(repo, rev)?;
    let files = at.files(repo)?;
    ensure!(files.binary_search(path).is_ok(), "{rev} has no file {path}");
    let bytes = at.read(repo, path)?;
    let text = std::str::from_utf8(&bytes).ok().filter(|t| !t.contains('\0'));
    let body = match (picture(path), text) {
        (Some(kind), _) => format!(r#"<div class="picture"><img alt="{}" src="{}"></div>"#, escape(path), data(kind, &bytes)),
        (None, Some(t)) if markdown(path) => format!("<article>{}</article>", rendered(t, path, rev, &files, &|p| at.read(repo, p).ok())),
        (None, Some(t)) if t.len() <= TEXT => format!(r#"<article class="text"><pre>{}</pre></article>"#, escape(t)),
        (None, Some(_)) => format!(r#"<p class="none">{} is too long to show here.</p>"#, escape(path)),
        (None, None) => format!(r#"<p class="none">{} is not text; the history shows no preview of it.</p>"#, escape(path)),
    };
    Ok(FILE.replacen("/*__TITLE__*/", &escape(path), 1).replacen("/*__FONTS__*/", crate::page::FONTS, 1).replacen("/*__BODY__*/", &body, 1))
}

fn markdown(path: &str) -> bool {
    path.ends_with(".md") || path.ends_with(".markdown")
}

/// The media type of a picture the page shows, by the file's extension.
fn picture(path: &str) -> Option<&'static str> {
    let ext = path.rsplit_once('.')?.1.to_ascii_lowercase();
    Some(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "svg" => "image/svg+xml",
        _ => return None,
    })
}

fn data(kind: &str, bytes: &[u8]) -> String {
    format!("data:{kind};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes))
}

/// Markdown `text` of the file `path` of `rev` as HTML. A picture it shows from the revision is
/// carried in the page (`read`); a link to another of its `files` opens that file of the same
/// revision; both in markdown and in the HTML it holds. A head of `---` lines is metadata, not text.
fn rendered(text: &str, path: &str, rev: &str, files: &[String], read: &dyn Fn(&str) -> Option<Vec<u8>>) -> String {
    use pulldown_cmark::{CowStr, Event, Options as Md, Parser, Tag};
    let text = text.strip_prefix("---\n").and_then(|rest| rest.split_once("\n---\n")).map_or(text, |(_, after)| after);
    let found = |target: &str| -> Option<String> {
        let target = [target.to_string(), http::decoded(target)].into_iter().find_map(|t| linked(path, &t))?;
        files.binary_search(&target).is_ok().then_some(target)
    };
    let image = |target: &str| found(target).and_then(|p| Some(data(picture(&p)?, &read(&p)?)));
    let link = |target: &str| {
        let (to, anchor) = target.split_once('#').map_or((target, None), |(t, a)| (t, Some(a)));
        let p = (!to.is_empty()).then(|| found(to)).flatten()?;
        Some(format!("/file?rev={}&path={}{}", encoded(rev), encoded(&p), anchor.map(|a| format!("#{a}")).unwrap_or_default()))
    };
    let events = Parser::new_ext(text, Md::ENABLE_TABLES | Md::ENABLE_STRIKETHROUGH | Md::ENABLE_TASKLISTS | Md::ENABLE_FOOTNOTES).map(
        |e| match e {
            Event::Start(Tag::Image { link_type, dest_url, title, id }) => {
                Event::Start(Tag::Image { link_type, dest_url: image(&dest_url).map_or(dest_url, CowStr::from), title, id })
            }
            Event::Start(Tag::Link { link_type, dest_url, title, id }) => {
                Event::Start(Tag::Link { link_type, dest_url: link(&dest_url).map_or(dest_url, CowStr::from), title, id })
            }
            Event::Html(h) => Event::Html(attributes(&h, &image, &link).into()),
            Event::InlineHtml(h) => Event::InlineHtml(attributes(&h, &image, &link).into()),
            e => e,
        },
    );
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, events);
    html
}

/// `html` with each quoted `src`, `srcset` and `href` that `image` or `link` places put where they
/// place it; every other attribute as it is.
fn attributes(html: &str, image: &dyn Fn(&str) -> Option<String>, link: &dyn Fn(&str) -> Option<String>) -> String {
    let (mut out, mut rest) = (String::with_capacity(html.len()), html);
    while let Some(at) = rest.find('=') {
        let (before, after) = rest.split_at(at);
        let name = before.rsplit(|c: char| c.is_whitespace()).next().unwrap_or("").to_ascii_lowercase();
        let quote = after[1..].chars().next().filter(|q| matches!(q, '"' | '\''));
        let value = quote.and_then(|q| after[2..].find(q).map(|end| (q, &after[2..2 + end])));
        let placed = match (name.as_str(), value) {
            ("src", Some((_, v))) => image(v),
            ("href", Some((_, v))) => link(v),
            ("srcset", Some((_, v))) => {
                let each = v.split(',').map(|s| {
                    let s = s.trim();
                    let (url, size) = s.split_once(char::is_whitespace).unwrap_or((s, ""));
                    format!("{} {size}", image(url).unwrap_or_else(|| url.to_string())).trim_end().to_string()
                });
                Some(each.collect::<Vec<_>>().join(", "))
            }
            _ => None,
        };
        match (placed, value) {
            (Some(p), Some((q, v))) => {
                out.push_str(before);
                out.push('=');
                out.push(q);
                out.push_str(&p.replace('&', "&amp;"));
                out.push(q);
                rest = &after[2 + v.len() + 1..];
            }
            _ => {
                out.push_str(&rest[..=at]);
                rest = &rest[at + 1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// The path from the repository that `target`, a link in the file `from`, names; none for an
/// address of its own (`https:`, `mailto:`) or one that leaves the repository.
fn linked(from: &str, target: &str) -> Option<String> {
    if target.contains(':') || target.starts_with('/') || target.is_empty() {
        return None;
    }
    let mut parts: Vec<&str> = from.split('/').collect();
    parts.pop();
    for part in target.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            p => parts.push(p),
        }
    }
    Some(parts.join("/"))
}

/// A value as it stands in an address's query.
fn encoded(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => (b as char).to_string(),
            b => format!("%{b:02X}"),
        })
        .collect()
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// The branch HEAD is on, the branch it follows and how far the two lie apart, and the git commands
/// that push and pull it: none where there is no branch (a detached HEAD), nothing to pull from, or no
/// remote to push to.
struct Branch {
    name: Option<String>,
    upstream: Option<String>,
    ahead: usize,
    behind: usize,
    push: Option<Vec<String>>,
    pull: Option<Vec<String>>,
}

impl Branch {
    fn of(repo: &Path) -> Branch {
        let said = |args: &[&str]| git(repo, args).ok().map(|o| String::from_utf8_lossy(&o).trim().to_string()).filter(|s| !s.is_empty());
        let name = said(&["symbolic-ref", "--short", "-q", "HEAD"]);
        let upstream = name.as_ref().and_then(|_| said(&["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{upstream}"]));
        let (behind, ahead) = upstream
            .as_ref()
            .and_then(|_| said(&["rev-list", "--left-right", "--count", "@{upstream}...HEAD"]))
            .and_then(|c| c.split_once('\t').map(|(b, a)| (b.parse().unwrap_or(0), a.parse().unwrap_or(0))))
            .unwrap_or((0, 0));
        let remotes: Vec<String> = said(&["remote"]).map(|r| r.lines().map(str::to_string).collect()).unwrap_or_default();
        let remote = remotes.iter().find(|r| *r == "origin").or(remotes.first());
        let push = match (&name, &upstream, remote) {
            (Some(_), Some(_), _) => Some(vec!["push".into()]),
            (Some(n), None, Some(r)) => Some(vec!["push".into(), "-u".into(), r.clone(), n.clone()]),
            _ => None,
        };
        let pull = upstream.as_ref().map(|_| vec!["pull".into(), "--ff-only".into()]);
        Branch { name, upstream, ahead, behind, push, pull }
    }

    fn json(&self) -> Value {
        json!({"name": self.name, "upstream": self.upstream, "ahead": self.ahead, "behind": self.behind, "push": self.push, "pull": self.pull})
    }
}

/// Run the git command the page sent (`{"run": [args]}`), once the reader confirmed it there, and say
/// what git said. Only the commands the page offers run: a tag on a commit, a switch to a branch at a
/// commit or to the commit itself, and the push and pull of HEAD's branch as they are now
/// (`Branch::of`); a command for a branch that has moved since the page asked is refused.
fn run(repo: &Path, req: &Request) -> Result<Value> {
    let body: Value = serde_json::from_slice(&req.body).context("the page sent no JSON")?;
    let args: Vec<String> = serde_json::from_value(body.get("run").cloned().unwrap_or_default()).context("`run` is a list of words")?;
    allowed(repo, &args)?;
    let out = Command::new("git").arg("-C").arg(repo).args(&args).env("GIT_TERMINAL_PROMPT", "0").output().context("running git")?;
    let said = [out.stdout, out.stderr]
        .iter()
        .map(|o| String::from_utf8_lossy(o).trim().to_string())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    if !out.status.success() {
        bail!("git {}: {}", args.join(" "), said.join("\n"));
    }
    Ok(json!({"said": said.join("\n")}))
}

/// Whether `args` is a git command the page offers, with names and commits that are what they claim.
fn allowed(repo: &Path, args: &[String]) -> Result<()> {
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    let is = |r: &str| git(repo, &["show-ref", "--verify", "--quiet", r]).is_ok();
    let commit = |c: &str| changes::resolve(repo, c).is_ok_and(|(full, _)| full == c);
    let name = |n: &str| !n.starts_with('-') && git(repo, &["check-ref-format", &format!("refs/tags/{n}")]).is_ok();
    let branch = Branch::of(repo);
    let ok = match words.as_slice() {
        ["tag", n, c] => name(n) && commit(c),
        ["tag", "-a", n, "-m", _, c] => name(n) && commit(c),
        ["switch", "--detach", c] => commit(c),
        ["switch", "--track", r] => !r.starts_with('-') && is(&format!("refs/remotes/{r}")),
        ["switch", b] => !b.starts_with('-') && is(&format!("refs/heads/{b}")),
        ["push", ..] => branch.push.as_deref() == Some(args),
        ["pull", ..] => branch.pull.as_deref() == Some(args),
        _ => false,
    };
    ensure!(ok, "the history does not run `git {}`; if the branch moved, look again", args.join(" "));
    Ok(())
}

fn git(repo: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let out = Command::new("git").arg("-C").arg(repo).args(args).output().context("running git")?;
    if !out.status.success() {
        bail!("git {}: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(out.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A link in a file names a path from the repository by the file's own directory; one that leaves
    /// the repository, or is an address of its own, names none.
    #[test]
    fn a_link_names_the_path_from_its_file() {
        assert_eq!(linked("docs/changes/307.md", "307.png").as_deref(), Some("docs/changes/307.png"));
        assert_eq!(linked("docs/changes/307.md", "../img/a.png").as_deref(), Some("docs/img/a.png"));
        assert_eq!(linked("README.md", "./docs/x.md").as_deref(), Some("docs/x.md"));
        assert_eq!(linked("README.md", "../outside.md"), None);
        assert_eq!(linked("README.md", "https://atlas.pttrn.io"), None);
        assert_eq!(linked("README.md", "/etc/passwd"), None);
    }

    /// The pictures and links of HTML in markdown are placed as the markdown's own are; any other
    /// attribute, and a target nothing places, stays as it was written.
    #[test]
    fn html_in_markdown_places_its_pictures_and_links() {
        let image = |t: &str| (t == "a.png").then(|| "data:image/png;base64,AA".to_string());
        let link = |t: &str| (t == "b.md").then(|| "/file?rev=x&path=b.md".to_string());
        let html = r#"<img width="60%" src="a.png" alt="a = b"><source srcset="a.png 2x"><a href="b.md">b</a><a href="https://x.io">x</a>"#;
        assert_eq!(
            attributes(html, &image, &link),
            r#"<img width="60%" src="data:image/png;base64,AA" alt="a = b"><source srcset="data:image/png;base64,AA 2x"><a href="/file?rev=x&amp;path=b.md">b</a><a href="https://x.io">x</a>"#
        );
    }
}
