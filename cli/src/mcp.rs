//! `atlas mcp`: the engine as an MCP server over stdio, so a coding agent can check a
//! repository against Atlas's principles, ask why a principle matters, and read a route.
//!
//! The protocol is JSON-RPC 2.0, one message per line on stdin and stdout: `initialize`,
//! `ping`, `tools/list` and `tools/call`. A notification gets no answer.

use std::io::{BufRead, Write};
use std::path::PathBuf;

use anyhow::Result;
use atlas_engine::{principles, remote, story, Graph, Options};
use serde_json::{json, Value};

/// Protocol versions this server speaks; a client asking for one gets it, else the newest.
const VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

const INSTRUCTIONS: &str = "Atlas reads a repository as the places where data rests (files, \
tables, types in memory, the screen) and the functions between them, and checks it against a \
handful of design principles. `check` finds where code and principle disagree, `explain` says \
why a principle matters and how it looks in Python and Rust, `routes` and `route` read what an \
entry point does, step by step, `view` writes the picture opened where you point (a route, \
the functions in it to open) as a page for the human, and `changes` shows what the working tree did to the routes, \
their places and the findings since a commit, or what one commit did against another: check a change against what it was meant to do. Atlas coaches: every finding ends in a question for the author. \
When you help a student, put that question to them and help them reason about the answer; let \
them make the change, then run `check` again to see the finding go. Python's types come from \
ty: a value whose type ty does not know (a function without a return annotation, a library \
without an installed environment) is not followed, and `check` asks for the annotation. Atlas \
sees Python best with the environment synced (`uv sync`) and Ruff's `ANN` rules on.";

/// Answer every message on stdin until it closes, reading repositories with `opts`.
pub fn serve(opts: &Options) -> Result<()> {
    let mut stdout = std::io::stdout().lock();
    for line in std::io::stdin().lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let replies: Vec<Value> = match serde_json::from_str::<Value>(&line) {
            Ok(Value::Array(batch)) => batch.iter().filter_map(|m| answer(m, opts)).collect(),
            Ok(msg) => answer(&msg, opts).into_iter().collect(),
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
fn answer(msg: &Value, opts: &Options) -> Option<Value> {
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
            match call(name, &args, opts) {
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
        "serverInfo": { "name": "atlas", "version": crate::VERSION.as_str() },
        "instructions": INSTRUCTIONS,
    })
}

fn tools() -> Value {
    let path = json!({ "type": "string", "description": "The repository: its root, a package directory, or a git URL (a `…/tree/<ref>/<folder>` URL names one folder). Default: the directory the server runs in." });
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
        {
            "name": "view",
            "description": "Draw a repository's Flowchart in a view of your choosing and write it as a page the human can open: the same picture `atlas view` draws, opened where you point. A view is JSON: `route` (an entry id from `routes`), and `open.functions` (the steps that run these functions are opened, as `route` lists them); also `kind` (all routes of one kind: cli, export, script, notebook, route, mcp, make), `part`, `focus` (package ids), and the opened `parts`, `units` (packages), `families` (type ids) and `places`. Gives the file the page is in.",
            "inputSchema": { "type": "object", "properties": {
                "path": path,
                "view": { "type": "object", "description": "The view, e.g. `{\"route\": \"pkg.cli.build\", \"open\": {\"functions\": [\"pkg.build.Pipeline.run\"]}}`." },
                "out": { "type": "string", "description": "Where to write the page. Default: `.atlas/<project>.view.html` at the top of the git repository it is in, where you and the person you work with can open it." },
            }, "required": ["view"] },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "openWorldHint": false },
        },
        {
            "name": "changes",
            "description": "What the working tree changes against a git revision, or what one revision changes against another: the routes added and removed, the steps and places each route gains and loses, and the findings raised and resolved. Use it after an edit to see whether the change does what it was meant to, or on a commit range to see what it did.",
            "inputSchema": { "type": "object", "properties": {
                "path": path,
                "rev": { "type": "string", "description": "The revision to compare from. Default: HEAD." },
                "until": { "type": "string", "description": "The revision to compare to. Default: the working tree." },
            } },
            "annotations": read_only,
        },
    ])
}

/// A tool's text, or what went wrong; `None` for a tool that does not exist.
fn call(name: &str, args: &Value, opts: &Options) -> Option<Result<String, String>> {
    let text = |k: &str| args.get(k).and_then(Value::as_str);
    Some(match name {
        "principles" => Ok(principles::PRINCIPLES.iter().map(|p| format!("{} · {}", p.id, p.asks)).collect::<Vec<_>>().join("\n")),
        "explain" => {
            let id = text("principle").unwrap_or("");
            principles::explain(id).map(str::to_string).ok_or_else(|| unknown_principle(id))
        }
        "check" => parts_of(text("path"), opts).and_then(|gs| {
            if let Some(p) = text("principle").filter(|p| !principles::PRINCIPLES.iter().any(|x| x.id == *p)) {
                return Err(unknown_principle(p));
            }
            let asked: Vec<&principles::Principle> =
                principles::PRINCIPLES.iter().filter(|p| text("principle").map_or(true, |id| p.id == id)).collect();
            let parts: Vec<(String, Vec<principles::Finding>)> = gs
                .iter()
                .map(|g| (g.root.clone(), principles::check(g).into_iter().filter(|f| asked.iter().any(|p| p.id == f.principle)).collect()))
                .collect();
            let look = principles::Look::plain();
            let reports: Vec<String> = parts.iter().map(|(root, findings)| principles::report(root, findings, &look)).collect();
            let counted: Vec<(&str, &[principles::Finding])> = parts.iter().map(|(r, f)| (r.as_str(), f.as_slice())).collect();
            Ok(reports.join("\n") + &principles::summary(&counted, &asked, &look))
        }),
        "routes" => graph(text("path"), opts).map(|g| {
            let rows: Vec<String> =
                story::entries(&g).into_iter().map(|(k, label, id)| format!("{:<8} {label}  ({id})", story::kind_label(k))).collect();
            if rows.is_empty() {
                "No entry points found: no CLI command, script, export, MCP tool or test.".into()
            } else {
                rows.join("\n")
            }
        }),
        "changes" => {
            let repo = PathBuf::from(text("path").unwrap_or("."));
            let rev = text("rev").unwrap_or("HEAD");
            atlas_engine::changes::compare(&repo, rev, text("until"), opts)
                .map(|c| atlas_engine::changes::report(&c))
                .map_err(|e| format!("Could not compare {} with {rev}: {e}", repo.display()))
        }
        "view" => {
            let (repo, clone) = match located(text("path")) {
                Ok(r) => r,
                Err(e) => return Some(Err(e)),
            };
            let shown = args.get("view").cloned().filter(Value::is_object);
            crate::page(&[repo.clone()], opts, None, None, shown).map_err(|e| format!("Could not read {}: {e}", repo.display())).and_then(
                |(name, html)| {
                    let home = clone.is_none().then_some(repo.as_path());
                    let out = text("out").map(PathBuf::from).unwrap_or_else(|| crate::made(home, &format!("{name}.view.html")));
                    std::fs::write(&out, html).map_err(|e| format!("Could not write {}: {e}", out.display()))?;
                    Ok(format!(
                        "Wrote {}. Open it in a browser, or hand the file to the person you work with; once open, its address holds the view, to share as a link.",
                        out.display()
                    ))
                },
            )
        }
        "route" => graph(text("path"), opts).and_then(|g| {
            let entry = text("entry").unwrap_or("");
            let depth = args.get("depth").and_then(Value::as_u64).unwrap_or(2) as usize;
            story::route(&g, entry, depth).ok_or_else(|| format!("No route `{entry}` in {}; `routes` lists them.", g.root))
        }),
        _ => return None,
    })
}

/// The repository a tool is handed: a path, or a URL cloned for as long as the tool runs.
fn located(path: Option<&str>) -> Result<(PathBuf, Option<remote::Clone>), String> {
    match path {
        Some(p) if remote::is_url(p) => remote::clone(p).map(|c| (c.dir.clone(), Some(c))).map_err(|e| format!("Could not fetch {p}: {e}")),
        p => Ok((PathBuf::from(p.unwrap_or(".")), None)),
    }
}

fn graph(path: Option<&str>, opts: &Options) -> Result<Graph, String> {
    let (repo, _clone) = located(path)?;
    atlas_engine::parts::analyze_repo(&[repo.clone()], opts).map_err(|e| format!("Could not read {}: {e}", repo.display()))
}

/// Each part's own graph, checked in its own language.
fn parts_of(path: Option<&str>, opts: &Options) -> Result<Vec<Graph>, String> {
    let (repo, _clone) = located(path)?;
    atlas_engine::parts::analyze_parts(&[repo.clone()], opts).map_err(|e| format!("Could not read {}: {e}", repo.display()))
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
