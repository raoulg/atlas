//! `atlas-code mcp` spoken to as a client would: one JSON-RPC message per line.

use std::io::Write;
use std::process::{Command, Stdio};

use serde_json::{json, Value};

/// Send `msgs` to a fresh server in the workspace root, keeping what it keeps in a cache of the
/// tests' own, and collect its replies.
fn session(msgs: &[Value]) -> Vec<Value> {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/..");
    let cache = std::env::temp_dir().join(format!("atlas-mcp-cache-{}", std::process::id()));
    let mut child = Command::new(env!("CARGO_BIN_EXE_atlas-code"))
        .arg("mcp")
        .current_dir(root)
        .env("XDG_CACHE_HOME", cache)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("atlas-code mcp starts");
    let mut stdin = child.stdin.take().unwrap();
    for m in msgs {
        writeln!(stdin, "{m}").unwrap();
    }
    drop(stdin);
    let out = child.wait_with_output().unwrap();
    String::from_utf8(out.stdout).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect()
}

fn call(id: u64, name: &str, args: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": args } })
}

fn text(reply: &Value) -> &str {
    reply["result"]["content"][0]["text"].as_str().unwrap_or("")
}

#[test]
fn a_client_initializes_lists_and_calls() {
    let replies = session(&[
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } } }),
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
        call(3, "check", json!({ "path": "engine/tests/fixture", "principle": "choices" })),
        call(4, "explain", json!({ "principle": "sums" })),
        call(5, "route", json!({ "path": "engine/tests/fixture", "entry": "build" })),
        call(6, "explain", json!({ "principle": "kiss" })),
        call(7, "nothing", json!({})),
    ]);
    assert_eq!(replies.len(), 7, "a notification gets no answer");
    assert_eq!(replies[0]["result"]["protocolVersion"], "2025-06-18", "the client's version, when spoken");
    assert!(replies[0]["result"]["instructions"].as_str().unwrap().contains("coaches"));
    let tools: Vec<&str> = replies[1]["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(
        tools,
        [
            "principles",
            "check",
            "revelation",
            "seals",
            "unseal",
            "explain",
            "routes",
            "route",
            "choices",
            "view",
            "changes",
            "shown",
            "look",
            "vision",
            "prophetize",
            "annotate",
            "notes",
            "resolve"
        ]
    );
    assert!(text(&replies[2]).contains("prepare tests `mode`"), "{}", text(&replies[2]));
    assert!(!text(&replies[2]).contains("\ndoors ·"), "only the principle asked for");
    assert!(text(&replies[3]).starts_with("A sum type is unwrapped where it arises."));
    assert_ne!(replies[4]["result"]["isError"], true, "{}", text(&replies[4]));
    assert!(text(&replies[4]).contains("fixture.cli.build"), "the route tool returns the requested entry: {}", text(&replies[4]));
    assert_eq!(replies[5]["result"]["isError"], true, "an unknown principle is the tool's error, said in words");
    assert_eq!(replies[6]["error"]["code"], -32602, "an unknown tool is a protocol error");
}

/// `check` with `json: true` gives the findings as data, the document `atlas-code check --json` prints: per
/// part its findings, each with what it is about, by role and id.
#[test]
fn check_gives_the_findings_as_data() {
    let replies = session(&[call(1, "check", json!({ "path": "engine/tests/fixture", "json": true }))]);
    assert_ne!(replies[0]["result"]["isError"], true, "{}", text(&replies[0]));
    let doc: Value = serde_json::from_str(text(&replies[0])).unwrap_or_else(|e| panic!("{e}: {}", text(&replies[0])));
    let findings: Vec<&Value> =
        doc["parts"].as_array().expect("parts").iter().flat_map(|p| p["findings"].as_array().expect("findings")).collect();
    let about: Vec<&Value> = findings.iter().filter_map(|f| f["about"].as_array()).flatten().collect();
    assert!(!about.is_empty(), "some finding says what it is about: {doc:#}");
    assert!(about.iter().all(|a| a["role"].is_string() && a["id"].is_string()), "{about:?}");
}

/// An agent can request a route by its entry's id, and a route that is not there is the tool's
/// error, said in words.
#[test]
fn an_agent_can_read_a_route_by_its_entry() {
    let replies = session(&[
        call(1, "route", json!({ "path": "engine/tests/fixture", "entry": "fixture.cli.build" })),
        call(2, "route", json!({ "path": "engine/tests/fixture", "entry": "fixture.cli.nowhere" })),
    ]);
    assert_ne!(replies[0]["result"]["isError"], true, "{}", text(&replies[0]));
    assert!(text(&replies[0]).contains("fixture.cli.build"), "{}", text(&replies[0]));
    assert_eq!(replies[1]["result"]["isError"], true, "{}", text(&replies[1]));
}

/// `changes` over MCP: an agent sees what its edit did to the routes.
#[test]
fn changes_shows_an_edit() {
    let from = std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../engine/tests/fixture-ts"));
    let repo = std::env::temp_dir().join(format!("atlas-mcp-changes-{}", std::process::id())).join("fixture-ts");
    let _ = std::fs::remove_dir_all(&repo);
    let mut stack = vec![from.clone()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            let dest = repo.join(p.strip_prefix(&from).unwrap());
            if p.is_dir() {
                stack.push(p);
            } else {
                std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
                std::fs::copy(&p, dest).unwrap();
            }
        }
    }
    let git = |args: &[&str]| {
        assert!(Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["-c", "user.name=atlas", "-c", "user.email=atlas@example.org", "-c", "commit.gpgsign=false"])
            .args(args)
            .status()
            .unwrap()
            .success())
    };
    git(&["init", "-q"]);
    git(&["add", "."]);
    git(&["commit", "-q", "-m", "fixture"]);
    let regels = repo.join("src/server/regels.ts");
    let src = std::fs::read_to_string(&regels).unwrap().replace("[await vat(\"x\")]", "[\"x\"]");
    std::fs::write(&regels, src).unwrap();
    let replies = session(&[call(1, "changes", json!({ "path": repo.to_string_lossy() }))]);
    let t = text(&replies[0]);
    assert!(t.contains("~ API · POST /api/regels\n    - step server.regels.vat\n"), "{t}");
    let _ = std::fs::remove_dir_all(repo.parent().unwrap());
}

/// An agent draws the picture it means: a view, written as a page the human opens.
#[test]
fn view_writes_the_page_in_a_view() {
    let out = std::env::temp_dir().join(format!("atlas-mcp-view-{}.html", std::process::id()));
    let shown = json!({ "route": "fixture.cli.build", "open": { "functions": ["fixture.build.Pipeline.run"] } });
    let replies = session(&[call(1, "view", json!({ "path": "engine/tests/fixture", "view": shown, "out": out.to_string_lossy() }))]);
    assert!(text(&replies[0]).starts_with("Wrote "), "{}", text(&replies[0]));
    let page = std::fs::read_to_string(&out).unwrap();
    let _ = std::fs::remove_file(&out);
    let data = &page[page.find("window.DATA = ").expect("the page holds its data") + "window.DATA = ".len()..];
    let data: Value = serde_json::Deserializer::from_str(data).into_iter().next().unwrap().unwrap();
    assert_eq!(data["projects"]["fixture"]["view"], shown, "the page opens in the view");
    // a view the page does not know is said, not drawn as no view at all
    let replies = session(&[call(1, "view", json!({ "path": "engine/tests/fixture", "view": { "rout": "fixture.cli.build" } }))]);
    assert_eq!(replies[0]["result"]["isError"], true);
    assert!(text(&replies[0]).contains("unknown field `rout`"), "{}", text(&replies[0]));
}

#[test]
fn invalid_utf8_line_gets_a_parse_error_and_the_next_line_is_read() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/..");
    let cache = std::env::temp_dir().join(format!("atlas-mcp-cache-{}", std::process::id()));
    let mut child = Command::new(env!("CARGO_BIN_EXE_atlas"))
        .arg("mcp")
        .current_dir(root)
        .env("XDG_CACHE_HOME", cache)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("atlas mcp starts");
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(b"\xff\n{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n").unwrap();
    drop(stdin);
    let out = child.wait_with_output().unwrap();
    let replies: Vec<Value> = String::from_utf8(out.stdout).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(replies.len(), 2, "a malformed line gets an error and does not end the session");
    assert_eq!(replies[0]["error"]["code"], -32700);
    assert_eq!(replies[1], json!({ "jsonrpc": "2.0", "id": 1, "result": {} }));
}
