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
sees Python best with the environment synced (`uv sync`) and Ruff's `ANN` rules on. When the \
person you work with runs `atlas serve`, their page and you share one view: `shown` says what \
they have open (read it when they say \"this box\" or \"here\"), `look` takes their page to a \
view, and `notes` lists what they wrote on boxes and lines. Answer every note with `answer`: \
reading a note does not resolve it.";

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
    let local = json!({ "type": "string", "description": "The repository `atlas serve` serves: its root or a directory in it. Default: the directory the server runs in." });
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
        {
            "name": "shown",
            "description": "What the person you work with has open in `atlas serve`: the view (route, opened sets, focus), the boxes and lines it draws, and the box they selected. Read it when they say \"this box\" or \"here\".",
            "inputSchema": { "type": "object", "properties": { "path": local } },
            "annotations": read_only,
        },
        {
            "name": "look",
            "description": "Take the page of `atlas serve` to a view: \"look here\". The view is JSON as `view` takes it (`route`, `open.functions`, `kind`, `part`, `focus`, the opened `parts`, `units`, `families`, `places`). The open page goes there within a second.",
            "inputSchema": { "type": "object", "properties": {
                "path": local,
                "view": { "type": "object", "description": "The view, e.g. `{\"route\": \"pkg.cli.build\", \"open\": {\"functions\": [\"pkg.build.Pipeline.run\"]}}`." },
            }, "required": ["view"] },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "openWorldHint": false },
        },
        {
            "name": "notes",
            "description": "The notes the person you work with wrote on boxes and lines of `atlas serve`'s page that no agent has answered yet: what each is on, the view it was written in, the commit, and the text. Reading marks them seen, not resolved: answer each with `answer`.",
            "inputSchema": { "type": "object", "properties": { "path": local } },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "openWorldHint": false },
        },
        {
            "name": "answer",
            "description": "Answer a note from `notes`, which resolves it: the page shows the answer on the box or line the note is on, once.",
            "inputSchema": { "type": "object", "properties": {
                "path": local,
                "id": { "type": "integer", "description": "The note's number, from `notes`." },
                "text": { "type": "string", "description": "The answer." },
            }, "required": ["id", "text"] },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "openWorldHint": false },
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
                |(name, html, _)| {
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
        "shown" => Ok(shown(&here(text("path")))),
        "look" => match args.get("view").filter(|v| v.is_object()) {
            Some(view) => crate::shared::look(&here(text("path")), view)
                .map(|_| "The page of `atlas serve` goes to this view when it next asks, within a second while it is open.".to_string())
                .map_err(|e| format!("Could not keep the view: {e}")),
            None => Err("`view` is a JSON object: `{\"route\": …, \"open\": {…}}`.".into()),
        },
        "notes" => crate::shared::read_open(&here(text("path"))).map(|ns| notes(&ns)).map_err(|e| format!("Could not read the notes: {e}")),
        "answer" => {
            let (id, said) = (args.get("id").and_then(Value::as_u64), text("text").filter(|t| !t.trim().is_empty()));
            match (id, said) {
                (Some(id), Some(said)) => crate::shared::change(&here(text("path")), id as u32, |n| {
                    n.state = crate::shared::State::Answered;
                    n.answer = Some(said.to_string());
                })
                .map_err(|e| format!("Could not keep the answer: {e}"))
                .and_then(|n| {
                    n.map(|n| format!("Note {} on {} is answered; the page shows the answer once.", n.id, n.label))
                        .ok_or_else(|| format!("No note {id}; `notes` lists them."))
                }),
                _ => Err("`answer` takes a note's `id` and the `text` of the answer.".into()),
            }
        }
        "route" => graph(text("path"), opts).and_then(|g| {
            let entry = text("entry").unwrap_or("");
            let depth = args.get("depth").and_then(Value::as_u64).unwrap_or(2) as usize;
            story::route(&g, entry, depth).ok_or_else(|| format!("No route `{entry}` in {}; `routes` lists them.", g.root))
        }),
        _ => return None,
    })
}

/// The repository `atlas serve` serves, as a tool names it: a directory, else where the server runs.
fn here(path: Option<&str>) -> PathBuf {
    PathBuf::from(path.unwrap_or("."))
}

/// What the page says it shows, as text: the view, the selected box, the boxes and the lines.
fn shown(repo: &std::path::Path) -> String {
    let Some(s) = crate::shared::shown(repo) else {
        return "No page has said what it shows: run `atlas serve` in the repository and open it.".into();
    };
    let list = |k: &str, f: &dyn Fn(&Value) -> String| {
        s.get(k).and_then(Value::as_array).map(|xs| xs.iter().map(|x| f(x)).collect::<Vec<_>>()).unwrap_or_default()
    };
    let str_of = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let boxes = list("boxes", &|b| format!("  {} ({})", str_of(b, "label"), str_of(b, "id")));
    let lines = list("lines", &|l| {
        format!(
            "  {} → {}{}",
            str_of(l, "from"),
            str_of(l, "to"),
            Some(str_of(l, "kind")).filter(|k| !k.is_empty()).map_or(String::new(), |k| format!(" ({k})"))
        )
    });
    let mut out = format!("View: {}\n", s.get("view").map_or("{}".into(), Value::to_string));
    if let Some(sel) = s.get("selected").and_then(Value::as_str).filter(|x| !x.is_empty()) {
        out += &format!("Selected: {sel}\n");
    }
    out += &format!("{} boxes:\n{}\n", boxes.len(), boxes.join("\n"));
    out += &format!("{} lines:\n{}", lines.len(), lines.join("\n"));
    out
}

/// The notes an agent has not answered, as text.
fn notes(ns: &[crate::shared::Note]) -> String {
    if ns.is_empty() {
        return "No open notes.".into();
    }
    ns.iter()
        .map(|n| {
            format!(
                "Note {} on {} ({}), written at {} in the view {}:\n  {}",
                n.id,
                n.label,
                n.on,
                &n.rev[..n.rev.len().min(7)],
                n.view,
                n.text
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
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
