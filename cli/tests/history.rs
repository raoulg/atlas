//! `atlas history` spoken to as the browser speaks to it: its history as JSON, and a viewer page
//! of the union of two commits.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use serde_json::Value;

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=atlas", "-c", "user.email=atlas@example.org", "-c", "commit.gpgsign=false", "-c", "tag.gpgsign=false"])
        .args(args)
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

/// A Python program in a repository of its own, committed twice: `plot` gives way to `draw_trend`.
fn repository() -> PathBuf {
    let repo = std::env::temp_dir().join(format!("atlas-history-{}", std::process::id())).join("spots");
    let _ = std::fs::remove_dir_all(&repo);
    std::fs::create_dir_all(repo.join("src/spots")).unwrap();
    std::fs::write(
        repo.join("pyproject.toml"),
        "[project]\nname = \"spots\"\nversion = \"0.1.0\"\n\n[project.scripts]\nspots = \"spots.cli:main\"\n",
    )
    .unwrap();
    std::fs::write(repo.join("src/spots/__init__.py"), "").unwrap();
    let cli = |draw: &str| {
        format!("from pathlib import Path\n\n\ndef {draw}(out: Path) -> None:\n    out.write_text(\"spots\")\n\n\ndef main() -> None:\n    {draw}(Path(\"figures/spots.txt\"))\n")
    };
    std::fs::write(repo.join("src/spots/cli.py"), cli("plot")).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "plot"]);
    git(&repo, &["tag", "v0.1"]);
    let b = cli("plot").replace("    plot(Path", "    print(\"drawing\")\n    plot(Path");
    std::fs::write(repo.join("src/spots/cli.py"), b).unwrap();
    git(&repo, &["commit", "-q", "-am", "main says it draws"]);
    repo
}

/// The server, keeping what it keeps in a cache of the tests' own, and the address it serves at,
/// read from what it says when it starts.
fn serve(repo: &Path) -> (Served, String) {
    let cache = std::env::temp_dir().join(format!("atlas-history-cache-{}", std::process::id()));
    let mut child = Command::new(env!("CARGO_BIN_EXE_atlas"))
        .args(["history", "--no-open", "--port", "0"])
        .arg(repo)
        .env("XDG_CACHE_HOME", cache)
        .stderr(Stdio::piped())
        .spawn()
        .expect("atlas history starts");
    let stderr = child.stderr.take().unwrap();
    let child = Served(child);
    let mut line = String::new();
    BufReader::new(stderr).read_line(&mut line).unwrap();
    let at = line.split("http://").nth(1).and_then(|r| r.split('/').next()).unwrap_or_else(|| panic!("an address: {line}")).to_string();
    (child, at)
}

/// The server a test started, ended with the test however the test ends.
struct Served(Child);

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// A GET request as a browser sends it from the history's own page, sent to `host`: the status line
/// and the body.
fn get(at: &str, host: &str, path: &str) -> (String, String) {
    get_from(at, host, path, "same-origin")
}

/// A GET request as a browser sends it from a page that is `site` to the server's address.
fn get_from(at: &str, host: &str, path: &str, site: &str) -> (String, String) {
    let mut conn = TcpStream::connect(at).unwrap();
    // an answer that does not come fails the test rather than holding it
    conn.set_read_timeout(Some(std::time::Duration::from_secs(120))).unwrap();
    write!(conn, "GET {path} HTTP/1.1\r\nHost: {host}\r\nAccept: */*\r\nSec-Fetch-Site: {site}\r\n\r\n").unwrap();
    let mut text = String::new();
    conn.read_to_string(&mut text).unwrap();
    let (head, body) = text.split_once("\r\n\r\n").unwrap();
    (head.lines().next().unwrap().to_string(), body.to_string())
}

/// The project a viewer page holds.
fn project(html: &str) -> Value {
    let data = &html[html.find("window.DATA = ").expect("a viewer page") + "window.DATA = ".len()..];
    let data: Value = serde_json::Deserializer::from_str(data).into_iter().next().unwrap().unwrap();
    data["projects"].as_object().unwrap().values().next().unwrap().clone()
}

#[test]
fn the_history_serves_its_log_and_the_union_of_two_commits() {
    let repo = repository();
    let (child, at) = serve(&repo);
    assert!(at.starts_with("127.0.0.1:"), "this machine only: {at}");

    let (status, body) = get(&at, &at, "/api/log?all=1&n=300");
    assert_eq!(status, "HTTP/1.1 200 OK");
    let log: Value = serde_json::from_str(&body).unwrap();
    let commits = log["commits"].as_array().unwrap();
    assert_eq!(commits.iter().map(|c| c["subject"].as_str().unwrap()).collect::<Vec<_>>(), ["main says it draws", "plot"]);
    assert_eq!(log["head"], commits[0]["hash"]);
    assert_eq!((&log["dirty"], &log["changed"]), (&Value::Bool(false), &serde_json::json!(0)));
    let (_, older) = get(&at, &at, "/api/log?all=1&n=300&skip=1");
    let older: Value = serde_json::from_str(&older).unwrap();
    assert_eq!(
        older["commits"].as_array().unwrap().iter().map(|c| &c["hash"]).collect::<Vec<_>>(),
        [&commits[1]["hash"]],
        "skip leaves out the newest"
    );
    assert_eq!(commits[0]["refs"], serde_json::json!([{ "name": "main", "kind": "head" }]));
    assert_eq!(commits[1]["refs"], serde_json::json!([{ "name": "v0.1", "kind": "tag" }]));
    assert_eq!(commits[0]["parents"], serde_json::json!([commits[1]["hash"]]));

    let (status, html) = get(&at, &at, "/view?a=v0.1&b=HEAD");
    assert_eq!(status, "HTTP/1.1 200 OK", "{html}");
    let p = project(&html);
    let diff = &p["diff"];
    assert_eq!((diff["a"]["rev"].as_str(), diff["a"]["subject"].as_str()), (Some("v0.1"), Some("plot")));
    assert_eq!((diff["b"]["rev"].as_str(), diff["b"]["subject"].as_str()), (Some("HEAD"), Some("main says it draws")));
    assert_eq!(diff["nodes"]["spots.cli.main"], "changed", "main prints now: {diff}");
    assert!(diff["was"]["spots.cli.main"].as_str().unwrap().starts_with("main() → None"), "{diff}");
    assert!(p["flow"]["bodies"].is_object(), "the union is drawn as any graph is");

    let (status, html) = get(&at, &format!("localhost:{}", at.split(':').nth(1).unwrap()), "/view?b=worktree");
    assert_eq!(status, "HTTP/1.1 200 OK");
    assert!(project(&html).get("diff").is_none(), "one reading has no diff");

    assert_eq!(get(&at, "example.org", "/api/log").0, "HTTP/1.1 403 Forbidden", "a page of another site is refused");
    assert_eq!(get_from(&at, &at, "/view?b=HEAD", "cross-site").0, "HTTP/1.1 403 Forbidden", "and runs no analysis");
    assert_eq!(get_from(&at, &at, "/view?b=HEAD", "none").0, "HTTP/1.1 200 OK", "a link the reader opens draws");
    assert_eq!(get(&at, &at, "/view?a=nothing-by-this-name").0, "HTTP/1.1 500 Internal Server Error");
    assert_eq!(get(&at, &at, "/nothing").0, "HTTP/1.1 404 Not Found");
    let (status, page) = get(&at, &at, "/");
    assert!(status.ends_with("200 OK") && !page.contains("/*__FONTS__*/"), "the history page, with its fonts");

    drop(child);
    let _ = std::fs::remove_dir_all(repo.parent().unwrap());
}
