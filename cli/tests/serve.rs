//! `atlas serve` and `atlas mcp` sharing one page: the page says what it shows and an agent reads
//! it, an agent points and the page goes there once, and a note the reader writes is resolved by an
//! agent's answer, never by its reading alone.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use serde_json::{json, Value};

/// A Python program in a repository of its own.
fn repository() -> PathBuf {
    let repo = std::env::temp_dir().join(format!("atlas-serve-{}", std::process::id())).join("spots");
    let _ = std::fs::remove_dir_all(&repo);
    std::fs::create_dir_all(repo.join("src/spots")).unwrap();
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    std::fs::write(repo.join("pyproject.toml"), "[project]\nname = \"spots\"\nversion = \"0.1.0\"\n").unwrap();
    std::fs::write(repo.join("src/spots/__init__.py"), "def count(n: int) -> int:\n    return n + 1\n").unwrap();
    repo
}

/// The server, and the address it serves at, read from what it says when it starts.
fn serve(repo: &Path) -> (Child, String) {
    let cache = std::env::temp_dir().join(format!("atlas-serve-cache-{}", std::process::id()));
    let mut child = Command::new(env!("CARGO_BIN_EXE_atlas"))
        .args(["serve", "--no-open", "--port", "0"])
        .arg(repo)
        .env("XDG_CACHE_HOME", cache)
        .stderr(Stdio::piped())
        .spawn()
        .expect("atlas serve starts");
    let lines = BufReader::new(child.stderr.take().unwrap()).lines().map_while(Result::ok);
    let line = lines.take(8).find(|l| l.contains("http://")).expect("the server says where it serves");
    let at = line.split("http://").nth(1).and_then(|r| r.split('/').next()).unwrap_or_else(|| panic!("an address: {line}")).to_string();
    (child, at)
}

/// A request as the page sends it, from the page itself when `own`: the status line and the body.
fn send(at: &str, method: &str, path: &str, body: Option<&Value>, own: bool) -> (String, String) {
    let mut conn = TcpStream::connect(at).unwrap();
    let body = body.map(Value::to_string).unwrap_or_default();
    let mark = if own { "X-Atlas: 1\r\n" } else { "" };
    write!(
        conn,
        "{method} {path} HTTP/1.1\r\nHost: {at}\r\n{mark}Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
    let mut text = String::new();
    conn.read_to_string(&mut text).unwrap();
    let (head, body) = text.split_once("\r\n\r\n").unwrap();
    (head.lines().next().unwrap().to_string(), body.to_string())
}

/// One tool of `atlas mcp`, called on `repo`: its text.
fn tool(repo: &Path, name: &str, mut args: Value) -> String {
    args["path"] = json!(repo.to_str().unwrap());
    let call = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": name, "arguments": args}});
    let mut child = Command::new(env!("CARGO_BIN_EXE_atlas")).arg("mcp").stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
    writeln!(child.stdin.take().unwrap(), "{call}").unwrap();
    let out = child.wait_with_output().unwrap();
    let reply: Value = serde_json::from_slice(&out.stdout).unwrap();
    reply["result"]["content"][0]["text"].as_str().unwrap_or_else(|| panic!("{reply}")).to_string()
}

#[test]
fn the_page_and_the_agent_share_one_view() {
    let repo = repository();
    let (mut server, at) = serve(&repo);

    let (status, page) = send(&at, "GET", "/", None, false);
    assert!(status.contains("200") && page.contains("\"served\":true"), "the page knows it is served: {status}");
    let (history, page) = send(&at, "GET", "/history", None, false);
    assert!(history.contains("200") && page.contains("Atlas history"), "the history beside the page: {history}");
    let mut conn = TcpStream::connect(&at).unwrap();
    write!(conn, "GET /export?v=%7B%22route%22%3A%22spots.count%22%7D HTTP/1.1\r\nHost: {at}\r\n\r\n").unwrap();
    let mut export = String::new();
    conn.read_to_string(&mut export).unwrap();
    assert!(export.contains("Content-Disposition: attachment; filename=\"spots.html\""), "the page to keep as a file");
    assert!(
        export.contains("\"view\":{\"route\":\"spots.count\"}") && !export.contains("\"served\":true"),
        "opened in the reader's view, and needing no server"
    );
    let shown = json!({"view": {"route": "spots.count"}, "selected": "spots.count", "boxes": [{"id": "spots.count", "label": "count"}], "lines": []});
    let (refused, _) = send(&at, "POST", "/api/shown", Some(&shown), false);
    assert!(refused.contains("403"), "a page of another site changes nothing: {refused}");
    assert!(send(&at, "POST", "/api/shown", Some(&shown), true).0.contains("200"));
    let read = tool(&repo, "shown", json!({}));
    assert!(
        read.contains("\"route\":\"spots.count\"") && read.contains("count (spots.count)") && read.contains("Selected: spots.count"),
        "{read}"
    );

    tool(&repo, "look", json!({"view": {"route": "spots.count"}}));
    let (_, look) = send(&at, "GET", "/api/look", None, false);
    assert_eq!(serde_json::from_str::<Value>(&look).unwrap(), json!({"route": "spots.count"}));
    assert!(send(&at, "GET", "/api/look", None, false).0.contains("204"), "the page goes there once");

    let note = json!({"on": "spots.count", "label": "count", "view": {"route": "spots.count"}, "text": "why one more?"});
    let (_, kept) = send(&at, "POST", "/api/notes", Some(&note), true);
    let id = serde_json::from_str::<Value>(&kept).unwrap()["id"].as_u64().unwrap();
    let state = || serde_json::from_str::<Value>(&send(&at, "GET", "/api/notes", None, false).1).unwrap()[0]["state"].clone();
    assert_eq!(state(), "open");
    assert!(tool(&repo, "notes", json!({})).contains("why one more?"));
    assert_eq!(state(), "seen", "reading a note marks it seen, and leaves it unresolved");
    assert!(tool(&repo, "notes", json!({})).contains("why one more?"), "a seen note is still open");
    tool(&repo, "answer", json!({"id": id, "text": "a count starts at one"}));
    assert_eq!(state(), "answered");
    assert!(tool(&repo, "notes", json!({})).contains("No open notes"));
    send(&at, "POST", "/api/notes/read", Some(&json!({"id": id})), true);
    assert_eq!(state(), "closed", "the reader has read the answer");

    let ignored = std::fs::read_to_string(repo.join(".atlas/.gitignore")).unwrap_or_default();
    server.kill().ok();
    let _ = std::fs::remove_dir_all(repo.parent().unwrap());
    assert!(ignored.lines().any(|l| l == "*"), "what the page and the agent share stays out of git");
}
