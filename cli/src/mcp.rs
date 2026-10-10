//! `atlas mcp`: the engine as an MCP server over stdio, so a coding agent can check a
//! repository against Atlas's principles, ask why a principle matters, and read a route.
//!
//! The protocol is JSON-RPC 2.0, one message per line on stdin and stdout: `initialize`,
//! `ping`, `tools/list` and `tools/call`. A notification gets no answer.

use std::io::{BufRead, Write};
use std::path::PathBuf;

use anyhow::Result;
use atlas_engine::{principles, story, Graph};
use serde_json::{json, Value};

/// Protocol versions this server speaks; a client asking for one gets it, else the newest.
const VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

const INSTRUCTIONS: &str = "Atlas reads a repository as the places where data rests (files, \
tables, types in memory, the screen) and the functions between them, and checks it against a \
handful of design principles. `check` finds where code and principle disagree, `explain` says \
why a principle matters and how it looks in Python and Rust, `routes` and `route` read what an \
entry point does, step by step. Atlas coaches: every finding ends in a question for the author. \
When you help a student, put that question to them and help them reason about the answer; let \
them make the change, then run `check` again to see the finding go.";

/// Answer every message on stdin until it closes.
pub fn serve() -> Result<()> {
    let mut stdout = std::io::stdout().lock();
    for line in std::io::stdin().lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let replies: Vec<Value> = match serde_json::from_str::<Value>(&line) {
            Ok(Value::Array(batch)) => batch.iter().filter_map(answer).collect(),
            Ok(msg) => answer(&msg).into_iter().collect(),
            Err(e) => vec![error(Value::Null, -32700, &format!("not JSON: {e}"))],
        };
        for r in replies {
            writeln!(stdout, "{r}")?;
            stdout.flush()?;
        }
    }
    Ok(())
}

/// The reply to one request; a notification, or a client's own reply, has none.
fn answer(msg: &Value) -> Option<Value> {
    let method = msg.get("method").and_then(Value::as_str)?;
    let id = msg.get("id")?.clone();
    let params = msg.get("params").cloned().unwrap_or_else(|| json!({}));
    Some(match method {
        "initialize" => ok(id, initialize(&params)),
        "ping" => ok(id, json!({})),
        "tools/list" => ok(id, json!({ "tools": tools() })),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
            match call(name, &args) {
                Some(Ok(text)) => ok(id, json!({ "content": [{ "type": "text", "text": text }], "isError": false })),
                Some(Err(e)) => ok(id, json!({ "content": [{ "type": "text", "text": e }], "isError": true })),
                None => error(id, -32602, &format!("no tool called `{name}`")),
            }
        }
        _ => error(id, -32601, &format!("no method `{method}`")),
    })
}

fn initialize(params: &Value) -> Value {
    let asked = params.get("protocolVersion").and_then(Value::as_str).unwrap_or("");
    let version = VERSIONS.iter().find(|v| **v == asked).unwrap_or(&VERSIONS[0]);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "atlas", "version": env!("CARGO_PKG_VERSION") },
        "instructions": INSTRUCTIONS,
    })
}

fn tools() -> Value {
    let path = json!({ "type": "string", "description": "The repository: its root, or a package directory. Default: the directory the server runs in." });
    let read_only = json!({ "readOnlyHint": true, "openWorldHint": false });
    json!([
        {
            "name": "principles",
            "description": "The design principles Atlas checks, each with its id and what it asks of code.",
            "inputSchema": { "type": "object", "properties": {} },
            "annotations": read_only,
        },
        {
            "name": "check",
            "description": "Check a repository against Atlas's principles. Each finding gives the file and line, what the graph shows, the evidence, and a question for the author.",
            "inputSchema": { "type": "object", "properties": {
                "path": path,
                "principle": { "type": "string", "description": "Only this principle's findings: an id from `principles`." },
            } },
            "annotations": read_only,
        },
        {
            "name": "explain",
            "description": "Why a principle matters: what it asks, its PEP or Rust idiom, how Atlas sees it, and an example.",
            "inputSchema": { "type": "object", "properties": {
                "principle": { "type": "string", "description": "An id from `principles`." },
            }, "required": ["principle"] },
            "annotations": read_only,
        },
        {
            "name": "routes",
            "description": "The routes into a repository: its CLI commands, scripts, library exports, MCP tools and tests.",
            "inputSchema": { "type": "object", "properties": { "path": path } },
            "annotations": read_only,
        },
        {
            "name": "route",
            "description": "What one route does, step by step in call order: each step's parameters, what it hands on, what it changes (↻), and what it reads, writes or prints. Alternatives share a number, lettered by branch (4.1a | 4.1b).",
            "inputSchema": { "type": "object", "properties": {
                "path": path,
                "entry": { "type": "string", "description": "An entry's label or id, from `routes`." },
                "depth": { "type": "integer", "minimum": 1, "description": "How many levels of steps to open. Default: 2." },
            }, "required": ["entry"] },
            "annotations": read_only,
        },
    ])
}

/// A tool's text, or what went wrong; `None` for a tool that does not exist.
fn call(name: &str, args: &Value) -> Option<Result<String, String>> {
    let text = |k: &str| args.get(k).and_then(Value::as_str);
    Some(match name {
        "principles" => Ok(principles::PRINCIPLES.iter().map(|p| format!("{} · {}", p.id, p.asks)).collect::<Vec<_>>().join("\n")),
        "explain" => {
            let id = text("principle").unwrap_or("");
            principles::explain(id).map(str::to_string).ok_or_else(|| unknown_principle(id))
        }
        "check" => parts_of(text("path")).and_then(|gs| {
            if let Some(p) = text("principle").filter(|p| !principles::PRINCIPLES.iter().any(|x| x.id == *p)) {
                return Err(unknown_principle(p));
            }
            Ok(gs
                .iter()
                .map(|g| {
                    let mut findings = principles::check(g);
                    findings.retain(|f| text("principle").map_or(true, |p| f.principle == p));
                    principles::report(&g.root, &findings)
                })
                .collect::<Vec<_>>()
                .join("\n"))
        }),
        "routes" => graph(text("path")).map(|g| {
            let rows: Vec<String> = story::entries(&g).into_iter().map(|(k, label, id)| format!("{:<8} {label}  ({id})", story::kind_label(k))).collect();
            if rows.is_empty() { "No entry points found: no CLI command, script, export, MCP tool or test.".into() } else { rows.join("\n") }
        }),
        "route" => graph(text("path")).and_then(|g| {
            let entry = text("entry").unwrap_or("");
            let depth = args.get("depth").and_then(Value::as_u64).unwrap_or(2) as usize;
            story::route(&g, entry, depth).ok_or_else(|| format!("No route `{entry}` in {}; `routes` lists them.", g.root))
        }),
        _ => return None,
    })
}

fn graph(path: Option<&str>) -> Result<Graph, String> {
    let repo = path.map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    atlas_engine::parts::analyze_repo(&[repo.clone()], None).map_err(|e| format!("Could not read {}: {e}", repo.display()))
}

/// Each part's own graph, checked in its own language.
fn parts_of(path: Option<&str>) -> Result<Vec<Graph>, String> {
    let repo = path.map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    atlas_engine::parts::analyze_parts(&[repo.clone()], None).map_err(|e| format!("Could not read {}: {e}", repo.display()))
}

fn unknown_principle(id: &str) -> String {
    let ids: Vec<&str> = principles::PRINCIPLES.iter().map(|p| p.id).collect();
    format!("No principle `{id}`; the principles are {}.", ids.join(", "))
}

fn ok(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}
