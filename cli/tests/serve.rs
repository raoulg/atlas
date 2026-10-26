//! `atlas serve` and `atlas mcp` sharing one page: the page says what it shows and an agent reads
//! it, an agent points at a box and the page goes there once, and a note the reader writes is
//! resolved by an agent, never by its reading alone, and opened again by the reader. The page follows the code: a changed file is drawn
//! anew, and the page knows which drawing it is. what an agent has in view is drawn beside the
//! working tree, and what it says of a box is shown on it.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use serde_json::{json, Value};

/// A Python program in a repository of its own, one per test.
fn repository(test: &str) -> PathBuf {
    let repo = std::env::temp_dir().join(format!("atlas-serve-{}-{test}", std::process::id())).join("spots");
    let _ = std::fs::remove_dir_all(&repo);
    std::fs::create_dir_all(repo.join("src/spots")).unwrap();
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    std::fs::write(repo.join("pyproject.toml"), "[project]\nname = \"spots\"\nversion = \"0.1.0\"\n").unwrap();
    std::fs::write(repo.join("src/spots/__init__.py"), "def count(n: int) -> int:\n    return n + 1\n").unwrap();
    repo
}

/// The server, and the address it serves at, read from what it says when it starts.
fn serve(repo: &Path) -> (Child, String) {
    serve_with(repo, &[])
}

/// The server, started with `more` arguments; its standard input is a pipe the test holds.
fn serve_with(repo: &Path, more: &[&str]) -> (Child, String) {
    let cache = std::env::temp_dir().join(format!("atlas-serve-cache-{}", std::process::id()));
    let mut child = Command::new(env!("CARGO_BIN_EXE_atlas"))
        .args(["serve", "--no-open", "--port", "0"])
        .args(more)
        .arg(repo)
        .env("XDG_CACHE_HOME", cache)
        .stdin(Stdio::piped())
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

fn complete(at: &str) -> (String, String) {
    for _ in 0..300 {
        let (_, body) = send(at, "GET", "/api/drawn", None, false);
        let state: Value = serde_json::from_str(&body).unwrap();
        if state["phase"] == "ready" {
            let (_, page) = send(at, "GET", "/", None, false);
            return (state["drawn"].as_str().unwrap().to_string(), page);
        }
        assert_ne!(state["phase"], "failed", "{state}");
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    panic!("the complete page was not served");
}

fn snapshot(at: &str) -> Value {
    serde_json::from_str(&send(at, "GET", "/api/snapshot", None, false).1).unwrap()
}

#[test]
fn the_page_and_the_agent_share_one_view() {
    let repo = repository("shared");
    let (mut server, at) = serve(&repo);

    let (_, page) = complete(&at);
    assert!(page.contains("/api/snapshot"), "the viewer requests its current reading");
    assert_eq!(snapshot(&at)["projects"]["spots"]["served"], true);
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

    tool(&repo, "look", json!({"view": {"route": "spots.count"}, "at": "spots.count"}));
    let (_, look) = send(&at, "GET", "/api/look", None, false);
    assert_eq!(serde_json::from_str::<Value>(&look).unwrap(), json!({"view": {"route": "spots.count"}, "at": "spots.count"}));
    assert!(send(&at, "GET", "/api/look", None, false).0.contains("204"), "the page goes there once");

    let note = json!({"on": "spots.count", "label": "count", "view": {"route": "spots.count"}, "text": "why one more?"});
    let (_, kept) = send(&at, "POST", "/api/notes", Some(&note), true);
    let id = serde_json::from_str::<Value>(&kept).unwrap()["id"].as_u64().unwrap();
    let state = || serde_json::from_str::<Value>(&send(&at, "GET", "/api/notes", None, false).1).unwrap()[0]["state"].clone();
    assert_eq!(state(), "open");
    let listed = tool(&repo, "notes", json!({}));
    assert!(listed.contains("why one more?"), "{listed}");
    assert!(listed.contains(&format!("Its place: http://{at}/#v=")) && listed.contains("at=spots.count"), "a link to its place: {listed}");
    assert_eq!(state(), "seen", "reading a note marks it seen, and leaves it unresolved");
    assert!(tool(&repo, "notes", json!({})).contains("why one more?"), "a seen note is still open");
    tool(&repo, "resolve", json!({"id": id, "commit": "3a2b1c9"}));
    assert_eq!(state(), "resolved");
    let after = tool(&repo, "notes", json!({}));
    assert!(after.contains("No open notes") && after.contains("Resolved while the page is served"), "{after}");
    let (_, again) = send(&at, "POST", "/api/notes/reopen", Some(&json!({"id": id, "text": "how does that make it one more?"})), true);
    assert_eq!(serde_json::from_str::<Value>(&again).unwrap()["text"], "why one more?\nhow does that make it one more?");
    assert_eq!(state(), "open", "the reader opened it again, for the agent to read anew");
    tool(&repo, "resolve", json!({"id": id}));

    let ignored = std::fs::read_to_string(repo.join(".atlas/.gitignore")).unwrap_or_default();
    server.kill().ok();
    // the next page starts without the notes resolved before it
    let (mut next, at) = serve(&repo);
    assert_eq!(send(&at, "GET", "/api/notes", None, false).1, "[]", "no graveyard");
    next.kill().ok();
    let _ = std::fs::remove_dir_all(repo.parent().unwrap());
    assert!(ignored.lines().any(|l| l == "*"), "what the page and the agent share stays out of git");
}

/// What the server has drawn: the name of the drawing a page shows.
/// Whether `child` has ended within `seconds`.
fn ends_within(child: &mut Child, seconds: u64) -> bool {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(seconds);
    while std::time::Instant::now() < until {
        if child.try_wait().unwrap().is_some() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    false
}

#[test]
fn a_server_started_by_a_program_ends_with_it() {
    let repo = repository("tethered");
    let (mut tethered, _) = serve_with(&repo, &["--exit-on-stdin-close"]);
    let (mut free, _) = serve(&repo);
    // the system closes a program's end of the pipe when the program ends, however it ends
    drop(tethered.stdin.take());
    drop(free.stdin.take());
    assert!(ends_within(&mut tethered, 10), "the server ends once its standard input closes");
    assert!(tethered.wait().unwrap().success());
    assert!(!ends_within(&mut free, 1), "a server started without the flag serves on, as from a terminal");
    free.kill().ok();
}

fn drawn(at: &str) -> Option<String> {
    let (_, body) = send(at, "GET", "/api/drawn", None, false);
    let state = serde_json::from_str::<Value>(&body).ok()?;
    (state["phase"] == "ready").then(|| state["drawn"].as_str().map(String::from)).flatten()
}

#[test]
fn the_page_follows_the_code() {
    let repo = repository("follows");
    let (mut server, at) = serve(&repo);

    let (first, page) = complete(&at);
    assert!(page.contains("/api/snapshot"), "the served viewer requests complete readings");
    assert_eq!(snapshot(&at)["projects"]["spots"]["drawn"], first);

    std::fs::write(
        repo.join("src/spots/__init__.py"),
        "def count(n: int) -> int:\n    return n + 1\n\n\ndef twice(n: int) -> int:\n    return count(count(n))\n",
    )
    .unwrap();
    let next = (0..60)
        .find_map(|_| {
            std::thread::sleep(std::time::Duration::from_millis(500));
            drawn(&at).filter(|d| *d != first)
        })
        .expect("the server draws the changed code by itself");
    let updated = snapshot(&at);
    assert_eq!(updated["projects"]["spots"]["drawn"], next);
    assert!(updated["projects"]["spots"]["nodes"].to_string().contains("spots.twice"), "the snapshot holds the changed code");

    server.kill().ok();
    let _ = std::fs::remove_dir_all(repo.parent().unwrap());
}

/// Beside the page, the files Atlas reads, each with what the graph defines in it: the code's file
/// holds its function, a file the graph defines nothing in is listed with none, and what git keeps
/// or ignores is not listed.
#[test]
fn the_files_are_listed_with_what_the_graph_defines_in_them() {
    let repo = repository("files");
    std::fs::write(repo.join("README.md"), "# spots\n").unwrap();
    std::fs::write(repo.join(".gitignore"), "out/\n").unwrap();
    std::fs::create_dir_all(repo.join("out")).unwrap();
    std::fs::write(repo.join("out/counts.csv"), "n\n1\n").unwrap();
    let (mut server, at) = serve(&repo);

    complete(&at);
    let (status, body) = send(&at, "GET", "/api/files", None, false);
    assert_eq!(status, "HTTP/1.1 200 OK");
    let listed: Value = serde_json::from_str(&body).unwrap();
    let files: Vec<&Value> = listed["files"].as_array().unwrap().iter().collect();
    let file = |path: &str| files.iter().find(|f| f["path"] == path).copied();
    let code = file("src/spots/__init__.py").expect("the code's file is listed");
    let named: Vec<&str> = code["facts"].as_array().unwrap().iter().filter_map(|f| f["name"].as_str()).collect();
    assert!(named.contains(&"count"), "the function is a fact of its file: {code}");
    assert_eq!(file("README.md").expect("a file without code is listed")["facts"], json!([]));
    assert!(file("out/counts.csv").is_none(), "what git ignores is not listed");
    assert!(files.iter().all(|f| !f["path"].as_str().unwrap().starts_with(".git/")), "what git keeps is not listed");

    server.kill().ok();
    let _ = std::fs::remove_dir_all(repo.parent().unwrap());
}

/// An agent shows a change as the files it would write, and the page draws the working tree
/// and the vision as one picture, the vision's step marked; the working tree stays as it was,
/// and once the vision is retracted the page draws the working tree alone.
#[test]
fn a_vision_is_drawn_beside_the_working_tree() {
    let repo = repository("vision");
    let (mut server, at) = serve(&repo);
    let (first, _) = complete(&at);

    let sketch = "def count(n: int) -> int:\n    return n + 1\n\n\ndef twice(n: int) -> int:\n    return count(count(n))\n";
    let told = tool(&repo, "vision", json!({"title": "count twice", "files": {"src/spots/__init__.py": sketch}}));
    assert!(told.starts_with("Envisioned: count twice."), "{told}");
    assert!(told.contains("+ Library · twice"), "what the vision does to the routes: {told}");
    assert!(!std::fs::read_to_string(repo.join("src/spots/__init__.py")).unwrap().contains("twice"), "the working tree stays");

    let next = (0..60)
        .find_map(|_| {
            std::thread::sleep(std::time::Duration::from_millis(500));
            drawn(&at).filter(|d| *d != first)
        })
        .expect("the server draws the vision by itself");
    let page = snapshot(&at).to_string();
    assert!(page.contains(&format!("\"drawn\":\"{next}\"")), "the snapshot is the vision");
    assert!(
        page.contains("\"diff\":") && page.contains("\"subject\":\"count twice\"") && page.contains("spots.twice"),
        "A and B as one picture"
    );

    assert!(tool(&repo, "vision", json!({"retract": true})).contains("retracted"));
    let alone = (0..60)
        .find_map(|_| {
            std::thread::sleep(std::time::Duration::from_millis(500));
            drawn(&at).filter(|d| *d != next)
        })
        .expect("the server draws the working tree again");
    let page = snapshot(&at).to_string();
    assert!(page.contains(&format!("\"drawn\":\"{alone}\"")) && !page.contains("\"diff\":"), "the working tree alone");

    server.kill().ok();
    let _ = std::fs::remove_dir_all(repo.parent().unwrap());
}

/// An agent says what the code does not, on a box or a line, and the page shows it in the agent's
/// words; a new annotation on the same box replaces the last, and an empty one takes it away.
#[test]
fn an_agent_annotates_a_box() {
    let repo = repository("annotate");
    let (mut server, at) = serve(&repo);
    let annotations = || serde_json::from_str::<Value>(&send(&at, "GET", "/api/annotations", None, false).1).unwrap();

    assert!(tool(&repo, "annotate", json!({"on": "spots.count", "text": "counts one up"})).contains("1 annotation"));
    assert_eq!(annotations(), json!([{"on": "spots.count", "text": "counts one up"}]));
    tool(&repo, "annotate", json!({"on": "spots.count", "text": "adds one"}));
    assert_eq!(annotations(), json!([{"on": "spots.count", "text": "adds one"}]), "the last word replaces the one before");
    tool(&repo, "annotate", json!({"on": "spots.count", "text": ""}));
    assert_eq!(annotations(), json!([]));

    server.kill().ok();
    let _ = std::fs::remove_dir_all(repo.parent().unwrap());
}

/// An agent decides its vision: the files are written into the tree with what the vision adds
/// marked as prophecy, the vision is retracted, and the revelation lists what was decided. A
/// vision that replaces what exists is refused, and is left as it was.
#[test]
fn a_decided_vision_is_written_as_prophecy() {
    let repo = repository("prophetize");
    let init = repo.join("src/spots/__init__.py");
    let sketch = |body: &str| format!("def count(n: int) -> int:\n    {body}\n\n\ndef twice(n: int) -> int: ...\n");

    tool(&repo, "vision", json!({"title": "count twice", "files": {"src/spots/__init__.py": sketch("return n")}}));
    let refused = tool(&repo, "prophetize", json!({}));
    assert!(refused.contains("changes `spots.count`") && refused.contains("supersedes"), "{refused}");
    assert!(!std::fs::read_to_string(&init).unwrap().contains("twice"), "a refused vision writes nothing");
    assert!(repo.join(".atlas/vision.json").exists(), "and stays as it was");

    tool(&repo, "vision", json!({"title": "count twice", "files": {"src/spots/__init__.py": sketch("return n + 1")}}));
    let told = tool(&repo, "prophetize", json!({"supersedes": {"spots.twice": "spots.count"}}));
    assert!(told.contains("Marked as prophecies: spots.twice") && told.contains("no `prophecy` function"), "{told}");
    let written = std::fs::read_to_string(&init).unwrap();
    assert!(written.contains("@prophecy(supersedes=\"spots.count\")\ndef twice"), "{written}");
    assert!(!repo.join(".atlas/vision.json").exists(), "the vision is retracted");
    let revelation = tool(&repo, "revelation", json!({}));
    assert!(revelation.contains("twice  supersedes `spots.count`"), "{revelation}");
}

/// A seal is broken by name: `sealed=..` leaves the marker and nothing else changes. Which seals
/// can be broken is the issues' to say (`closed`, asked of GitHub with `gh`), so with none to ask
/// nothing is broken, and the seals are still listed.
#[test]
fn a_seal_is_broken_by_naming_it() {
    let repo = repository("unseal");
    let init = repo.join("src/spots/__init__.py");
    std::fs::write(
        &init,
        "def prophecy(**_: str):\n    return lambda item: item\n\n\n@prophecy(sealed=\"#7\", supersedes=\"spots.prophecy\")\ndef twice(n: int) -> int: ...\n",
    )
    .unwrap();
    let listed = tool(&repo, "seals", json!({}));
    assert!(listed.contains("1 sealed, 0 to break") && listed.contains("twice  sealed on #7"), "{listed}");
    assert_eq!(tool(&repo, "unseal", json!({"closed": true})), "No seal to break.");
    assert!(tool(&repo, "unseal", json!({})).contains("takes the `ids`"));
    let told = tool(&repo, "unseal", json!({"ids": ["spots.twice"]}));
    assert!(told.contains("Seals broken: spots.twice"), "{told}");
    let written = std::fs::read_to_string(&init).unwrap();
    assert!(written.contains("@prophecy(supersedes=\"spots.prophecy\")\ndef twice"), "{written}");
    assert!(tool(&repo, "seals", json!({})).contains("0 sealed"));
}
