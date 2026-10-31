//! `atlas-code mcp`: the engine as an MCP server over stdio, so a coding agent can check a
//! repository against Atlas's principles, ask why a principle matters, and read a route.
//!
//! The protocol is JSON-RPC 2.0, one message per line on stdin and stdout: `initialize`,
//! `ping`, `tools/list` and `tools/call`. A notification gets no answer.

use std::io::{BufRead, Write};
use std::path::PathBuf;

use anyhow::Result;
use atlas_engine::principles::Principle;
use atlas_engine::sight::Sight;
use atlas_engine::{principles, remote, story, Graph, Options};
use serde::Deserialize;
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
person you work with runs `atlas-code serve`, their page and you share one view: `shown` says what \
they have open (read it when they say \"this box\" or \"here\"), `look` takes their page to a \
view or points at a box or a line (\"here, this one\"), and `notes` lists what they wrote on boxes \
and lines, each with a link to its place. Handle a note where you work, by a fix or by explaining \
it to them there (Atlas is no chat), then `resolve` it, with the commit if one fixed it; reading \
a note does not resolve it. Put a note worth keeping (a real bug) into an issue with its link, \
and give the issue to `resolve`. To ask whether to make a change, show it rather than describe it: \
`vision` the files the change would write, as sketch code (signatures, types, calls in order; \
bodies may stay empty), and the page draws the working tree and the vision as one picture, \
what you add, remove and change marked; `look` at the part you mean; once it is decided, `prophetize` \
writes it into the tree as prophecy, or `retract` takes it away. To say what the code does not (what a condition of `choices` means, what a step is \
for), `annotate` a box or a line; it shows on the page in your words. Sketch code kept in the tree, marked \
as not built yet, is a prophecy (`@prophecy(supersedes=..)` in Python, `#[doc = \"prophecy: ..\"]` in Rust, \
`@prophecy` in a TypeScript JSDoc): `revelation` lists them with what is left to build, and `check` asks \
about built code that leans on one.";

/// Answer every message on stdin until it closes, reading repositories with `opts`.
pub fn serve(opts: &Options) -> Result<()> {
    let mut stdout = std::io::stdout().lock();
    for line in std::io::stdin().lock().split(b'\n') {
        let line = line?;
        let line = match String::from_utf8(line) {
            Ok(line) => line,
            Err(e) => {
                writeln!(stdout, "{}", error(Value::Null, -32700, &format!("not UTF-8: {e}")))?;
                stdout.flush()?;
                continue;
            }
        };
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
            // a tool that panics fails its own call; the server answers the next
            let called = crate::http::guarded(|| call(name, &args, opts))
                .unwrap_or_else(|said| Some(Err(format!("Atlas failed inside `{name}`: {said}. The other tools still answer."))));
            match called {
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
        "serverInfo": { "name": "atlas", "version": crate::version::VERSION.as_str() },
        "instructions": INSTRUCTIONS,
    })
}

fn tools() -> Value {
    let path = json!({ "type": "string", "description": "The repository: its root, a package directory, or a git URL (a `…/tree/<ref>/<folder>` URL names one folder). Default: the directory the server runs in." });
    let read_only = json!({ "readOnlyHint": true, "openWorldHint": false });
    let local = json!({ "type": "string", "description": "The repository `atlas-code serve` serves: its root or a directory in it. Default: the directory the server runs in." });
    json!([
        {
            "name": "principles",
            "description": "The design principles Atlas checks, each with its id and what it asks of code.",
            "inputSchema": { "type": "object", "properties": {} },
            "annotations": read_only,
        },
        {
            "name": "check",
            "description": "Check a repository against Atlas's principles. Each finding gives the file and line, what the graph shows, the evidence, and a question for the author. A finding the author has answered with a decision (`# atlas: ignore[changes] reason` in the code, or an entry of `.atlasignore`) is set aside and counted; a decision that sets nothing aside is listed as a question of its own.",
            "inputSchema": { "type": "object", "properties": {
                "path": path,
                "principle": { "type": "string", "description": "Only this principle's findings: an id from `principles`." },
                "json": { "type": "boolean", "description": "The findings as data, as `atlas-code check --json` prints them: each with `about`, the ids it is about by the part each plays (`breaks`, `variant`, `put`, `lost`), to read without the sentences, and one set aside with its `aside`, the decision that answers it." },
                "all": { "type": "boolean", "description": "List the findings set aside too, each with the decision that answers it." },
            } },
            "annotations": read_only,
        },
        {
            "name": "revelation",
            "description": "Every prophecy in the tree: sketch code marked as not built yet. For each, what it supersedes, how many bodies are left to write, whether it is veiled or sealed, and the built code that leans on it. The epic is done when nothing is left.",
            "inputSchema": { "type": "object", "properties": {
                "path": path,
            } },
            "annotations": read_only,
        },
        {
            "name": "seals",
            "description": "The sealed prophecies: sketch code whose shape waits on an open question (`sealed=\"#144\"`), each with its issue and whether GitHub says it is open or closed (asked with `gh`; unknown where that cannot be asked). A seal whose issue is closed can be broken with `unseal`.",
            "inputSchema": { "type": "object", "properties": {
                "path": path,
            } },
            "annotations": read_only,
        },
        {
            "name": "unseal",
            "description": "Break seals: take `sealed=..` out of the marker of each named prophecy, or of every sealed prophecy whose issue is closed, and nothing else. Do it when the question is answered, then sketch the shape. The files are left for the person to commit.",
            "inputSchema": { "type": "object", "properties": {
                "path": local,
                "ids": { "type": "array", "items": { "type": "string" }, "description": "The prophecies whose seals to break, by the ids `seals` lists." },
                "closed": { "type": "boolean", "description": "Break every seal whose issue is closed." },
            } },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "openWorldHint": false },
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
            "description": "What one route does, step by step in call order: each step's parameters, what it hands on, what it changes (↻), and what it reads, writes or prints. Alternatives share a number, lettered by branch (4.1a | 4.1b; past z: 4.1aa), and the overrides of a lettered step after a dash (2a-a).",
            "inputSchema": { "type": "object", "properties": {
                "path": path,
                "entry": { "type": "string", "description": "An entry's label or id, from `routes`." },
                "depth": { "type": "integer", "minimum": 1, "description": "How many levels of steps to open. Default: 2." },
            }, "required": ["entry"] },
            "annotations": read_only,
        },
        {
            "name": "choices",
            "description": "The choices a route makes, each where it is made, in call order: what decides each branch as the code writes it (`when not raw.exists()`, a `match` pattern, `otherwise`), and the steps each branch runs with the places they touch; a choice of one branch runs nothing otherwise. Atlas does not say what a condition means: read the conditions and say it, e.g. \"only on the first run\", or \"never true here\". The page draws each choice as a frame titled with its conditions.",
            "inputSchema": { "type": "object", "properties": {
                "path": path,
                "entry": { "type": "string", "description": "The route, by an entry's id or label as `routes` lists them." },
            }, "required": ["entry"] },
            "annotations": read_only,
        },
        {
            "name": "view",
            "description": "Draw a repository's Flowchart in a view of your choosing and write it as a page the human can open: the same picture `atlas-code view` draws, opened where you point. A view is JSON: `route` (an entry id from `routes`), and `open.functions` (the steps that run these functions are opened, as `route` lists them); also `kind` (all routes of one kind: cli, export, script, notebook, route, mcp, make), `part`, `focus` (package ids), and the opened `parts`, `units` (packages), `families` (type ids) and `places`. Gives the file the page is in.",
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
            "description": "What the person you work with has open in `atlas-code serve`: the view (route, opened sets, focus), the boxes and lines it draws, and the box they selected. Read it when they say \"this box\" or \"here\".",
            "inputSchema": { "type": "object", "properties": { "path": local } },
            "annotations": read_only,
        },
        {
            "name": "look",
            "description": "Point the page of `atlas-code serve` at something: \"look here, this box\". Give a view to go to (JSON as `view` takes it: `route`, `open.functions`, `kind`, `part`, `focus`, the opened `parts`, `units`, `families`, `places`), a box or a line to point at, or both. The open page goes there within a second, selects what it points at and makes it light up.",
            "inputSchema": { "type": "object", "properties": {
                "path": local,
                "view": { "type": "object", "description": "The view, e.g. `{\"route\": \"pkg.cli.build\", \"open\": {\"functions\": [\"pkg.build.Pipeline.run\"]}}`. Without it the page keeps its view." },
                "at": { "type": "string", "description": "A box by its id, or a line as `from>to`, as `shown` lists them." },
            } },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "openWorldHint": false },
        },
        {
            "name": "vision",
            "description": "Show the person you work with what you plan, drawn beside what is: give the files your change would write, as sketch code (the signatures with their types and the calls in their order; bodies may be `...`, `todo!()` or `throw` — a sketch has to parse, not to build), and `atlas-code serve` draws the working tree and your vision as one picture, with what you add, remove and change marked, and a switch between the two. Use it to ask \"shall I do this?\" with a picture rather than in words, then `look` to point at the part you mean. It answers with what the vision does to the routes. A new vision replaces the last; `prophetize` writes it into the tree as prophecy once it is decided, `retract` takes it away.",
            "inputSchema": { "type": "object", "properties": {
                "path": local,
                "title": { "type": "string", "description": "What the vision is, in a line: shown beside it on the page." },
                "files": { "type": "object", "description": "Each file the vision writes, by its path from the repository's root, with its whole text; `null` for a file it removes. Files left out stay as they are.", "additionalProperties": { "type": ["string", "null"] } },
                "retract": { "type": "boolean", "description": "Take the vision away: the page draws the working tree alone again." },
            } },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "openWorldHint": false },
        },
        {
            "name": "prophetize",
            "description": "Decide the vision: write its files into the tree, every item the vision adds marked as a prophecy (sketch code kept in the tree, not built yet), and take the vision away. The files are left for the person to commit; no branch is opened. A vision that changes or removes something that exists is refused: the new item takes a new name and `supersedes` the old one. Answers with what was marked and what the person still has to do.",
            "inputSchema": { "type": "object", "properties": {
                "path": local,
                "supersedes": { "type": "object", "description": "For an added item by its id, the existing item it replaces: `{\"pkg.store.RasterStore\": \"pkg.store.RasterSink\"}`.", "additionalProperties": { "type": "string" } },
            } },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "openWorldHint": false },
        },
        {
            "name": "annotate",
            "description": "Say something on a box or a line of `atlas-code serve`'s page, in your own words: what a condition of `choices` means (\"only on the first run\"), what a step is for, why a place stands where it does. It shows as a mark on what it is on, with your words on hover, apart from what Atlas traces. Name what it is on as `shown` lists the boxes and lines (a choice's frame is a box). A new annotation on the same box replaces the last; an empty `text` takes it away.",
            "inputSchema": { "type": "object", "properties": {
                "path": local,
                "on": { "type": "string", "description": "A box by its id, or a line as `from>to`, as `shown` lists them." },
                "text": { "type": "string", "description": "What you say of it. Empty takes the annotation away." },
            }, "required": ["on"] },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "openWorldHint": false },
        },
        {
            "name": "notes",
            "description": "The notes the person you work with wrote on boxes and lines of `atlas-code serve`'s page that no agent has resolved: what each is on, the view and commit it was written at, its text (a line for each time it was opened), and a link to its place. Reading marks them seen, not resolved. Handle each where you work, by a fix or by explaining, then `resolve` it; a note worth keeping (a real bug) goes into an issue with its link first. Also lists the notes resolved while this page is served.",
            "inputSchema": { "type": "object", "properties": { "path": local } },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "openWorldHint": false },
        },
        {
            "name": "resolve",
            "description": "Resolve a note from `notes`, once you have handled it: fixed it, or explained it to the person you work with where you work (Atlas is no chat). The page marks it resolved while it is served; they can reopen it with a new line. Give the commit that fixed it, if one did, and the issue you kept it in, if you made one.",
            "inputSchema": { "type": "object", "properties": {
                "path": local,
                "id": { "type": "integer", "description": "The note's number, from `notes`." },
                "commit": { "type": "string", "description": "The commit that fixed it, if one did." },
                "issue": { "type": "string", "description": "The link of the issue that keeps it, if you made one." },
            }, "required": ["id"] },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "openWorldHint": false },
        },
    ])
}

/// A tool's text, or what went wrong; `None` for a tool that does not exist.
fn call(name: &str, args: &Value, opts: &Options) -> Option<Result<String, String>> {
    let text = |k: &str| args.get(k).and_then(Value::as_str);
    Some(match name {
        "principles" => Ok(Principle::ALL.iter().map(|p| format!("{} · {}", p.id(), p.asks())).collect::<Vec<_>>().join("\n")),
        "explain" => {
            let id = text("principle").unwrap_or("");
            Principle::parse(id).and_then(principles::explain).map(str::to_string).ok_or_else(|| unknown_principle(id))
        }
        "check" => parts_of(text("path"), opts).and_then(|gs| {
            let asked: Vec<Principle> = match text("principle") {
                None => Principle::ALL.to_vec(),
                Some(id) => vec![Principle::parse(id).ok_or_else(|| unknown_principle(id))?],
            };
            let found: Vec<Vec<principles::Finding>> = gs.iter().map(atlas_engine::aside::check).collect();
            // whether a decision sets anything aside is asked of every finding, whichever are shown
            let stale = atlas_engine::aside::stale(&gs.iter().zip(&found).map(|(g, f)| (g, f.as_slice())).collect::<Vec<_>>());
            let parts: Vec<(String, Vec<principles::Finding>)> = gs
                .iter()
                .zip(found)
                .map(|(g, f)| (g.root.clone(), f.into_iter().filter(|f| asked.contains(&f.principle)).collect()))
                .collect();
            if args.get("json").and_then(Value::as_bool) == Some(true) {
                let reports: Vec<Value> = parts.iter().map(|(root, findings)| json!({ "root": root, "findings": findings })).collect();
                return Ok(serde_json::to_string_pretty(&json!({ "parts": reports, "stale": stale })).unwrap_or_default());
            }
            let look = principles::Look::plain();
            let mut reports: Vec<String> = parts.iter().map(|(root, findings)| principles::report(root, findings, &look)).collect();
            if args.get("all").and_then(Value::as_bool) == Some(true) {
                reports.extend(parts.iter().map(|(root, findings)| principles::set_aside(root, findings, &look)).filter(|r| !r.is_empty()));
            }
            let counted: Vec<(&str, &[principles::Finding])> = parts.iter().map(|(r, f)| (r.as_str(), f.as_slice())).collect();
            Ok(reports.join("\n") + &principles::summary(&counted, &asked, &look) + &atlas_engine::aside::report(&stale, &look))
        }),
        "revelation" => parts_of(text("path"), opts).map(|gs| {
            let repo = here(text("path"));
            gs.iter()
                .map(|g| {
                    let stands = atlas_engine::revelation::revelation(g);
                    let sealed = atlas_engine::revelation::seals(&stands, &|i| crate::shared::issue_state(&repo, i));
                    atlas_engine::revelation::report(&g.root, &stands, &sealed)
                })
                .collect::<Vec<_>>()
                .join("\n")
        }),
        "seals" => parts_of(text("path"), opts).map(|gs| {
            let repo = here(text("path"));
            gs.iter()
                .map(|g| {
                    let stands = atlas_engine::revelation::revelation(g);
                    atlas_engine::revelation::seals_report(
                        &g.root,
                        &atlas_engine::revelation::seals(&stands, &|i| crate::shared::issue_state(&repo, i)),
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        }),
        "unseal" => {
            let repo = here(text("path"));
            let named: Vec<String> = args
                .get("ids")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
                .unwrap_or_default();
            let closed = args.get("closed").and_then(Value::as_bool) == Some(true);
            if named.is_empty() && !closed {
                return Some(Err(
                    "`unseal` takes the `ids` of the prophecies to break, or `closed` for every seal whose issue is closed.".into()
                ));
            }
            let gs = match parts_of(text("path"), opts) {
                Ok(gs) => gs,
                Err(e) => return Some(Err(e)),
            };
            let mut broken = vec![];
            for g in &gs {
                let stands = atlas_engine::revelation::revelation(g);
                let mut ids: Vec<String> = named.iter().filter(|n| g.node(n).is_some()).cloned().collect();
                if closed {
                    let sealed = atlas_engine::revelation::seals(&stands, &|i| crate::shared::issue_state(&repo, i));
                    ids.extend(sealed.iter().filter(|s| s.can_be_broken()).map(|s| s.id.clone()));
                }
                if ids.is_empty() {
                    continue;
                }
                let files = match atlas_engine::decide::unseal(g, &ids) {
                    Ok(f) => f,
                    Err(e) => return Some(Err(format!("{e}"))),
                };
                if let Err(e) = crate::shared::write_files(std::path::Path::new(&g.project_root), &files) {
                    return Some(Err(format!("Could not write the files: {e}")));
                }
                broken.extend(ids);
            }
            if broken.is_empty() {
                Ok("No seal to break.".into())
            } else {
                Ok(format!("Seals broken: {}. The files are left for you to commit; sketch the shape now.", broken.join(", ")))
            }
        }
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
        "view" => sight(args).and_then(|sight| view(text("path"), sight, text("out"), opts)),
        "shown" => Ok(shown(&here(text("path")))),
        "look" => sight(args).and_then(|view| {
            let at = text("at").filter(|a| !a.is_empty()).map(str::to_string);
            if view.is_none() && at.is_none() {
                return Err("`look` takes a `view` (`{\"route\": …, \"open\": {…}}`), a box or line to point `at`, or both.".into());
            }
            crate::shared::look(&here(text("path")), &crate::shared::Look { view, at })
                .map(|_| "The page of `atlas-code serve` goes there when it next asks, within a second while it is open.".to_string())
                .map_err(|e| format!("Could not keep where to look: {e}"))
        }),
        "vision" => {
            let repo = here(text("path"));
            if args.get("retract").and_then(Value::as_bool) == Some(true) {
                return Some(
                    crate::shared::retract(&repo)
                        .map(|was| {
                            if was { "The vision is retracted; the page draws the working tree alone." } else { "There was no vision." }
                                .to_string()
                        })
                        .map_err(|e| format!("Could not retract the vision: {e}")),
                );
            }
            let files: std::collections::BTreeMap<PathBuf, Option<String>> = match args.get("files").and_then(Value::as_object) {
                Some(fs) if !fs.is_empty() => fs.iter().map(|(k, v)| (PathBuf::from(k), v.as_str().map(str::to_string))).collect(),
                _ => return Some(Err("`vision` takes the `files` the vision writes (a path from the repository's root to its text, or null to remove it), or `retract`.".into())),
            };
            let vision = crate::shared::Vision { title: text("title").unwrap_or("a vision").to_string(), files };
            let before = atlas_engine::parts::analyze_parts(&[repo.clone()], opts);
            let after = atlas_engine::changes::envisioned(&repo, &vision.files, opts).map(|v| v.parts);
            match (before, after) {
                (Ok(before), Ok(after)) => crate::shared::envision(&repo, &vision).map_err(|e| format!("Could not keep the vision: {e}")).map(|_| {
                    let c = atlas_engine::changes::between("working tree", Some("vision"), &before, &after);
                    format!(
                        "Envisioned: {}. The page of `atlas-code serve` draws it beside the working tree within a second; `look` points at what you mean.\n\n{}",
                        vision.title,
                        atlas_engine::changes::report(&c)
                    )
                }),
                (Err(e), _) | (_, Err(e)) => Err(format!("Could not read the vision: {e}")),
            }
        }
        "prophetize" => {
            let repo = here(text("path"));
            let Some(vision) = crate::shared::vision(&repo) else {
                return Some(Err("There is no vision to decide: show one with `vision` first.".into()));
            };
            let supersedes: std::collections::BTreeMap<String, String> = args
                .get("supersedes")
                .and_then(Value::as_object)
                .map(|m| m.iter().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string()))).collect())
                .unwrap_or_default();
            let prophetized = match atlas_engine::decide::prophetize(&repo, &vision.files, &supersedes, opts) {
                Ok(p) => p,
                Err(e) => return Some(Err(format!("{e}"))),
            };
            if let Err(e) = crate::shared::write_files(&repo, &prophetized.files) {
                return Some(Err(format!("Could not write the files: {e}")));
            }
            let _ = crate::shared::retract(&repo);
            let files: Vec<String> = prophetized.files.keys().map(|f| f.display().to_string()).collect();
            let mut said = format!(
                "Decided: {}. Written: {}. Marked as prophecies: {}. The files are left for you to commit; the vision is retracted.",
                vision.title,
                files.join(", "),
                if prophetized.marked.is_empty() { "nothing".to_string() } else { prophetized.marked.join(", ") }
            );
            for note in &prophetized.notes {
                said += &format!("\nNote: {note}");
            }
            Ok(said)
        }
        "annotate" => match text("on").filter(|o| !o.is_empty()) {
            Some(on) => crate::shared::annotate(&here(text("path")), on, text("text"))
                .map(|all| {
                    format!(
                        "The page of `atlas-code serve` shows {} annotation{} within a few seconds.",
                        all.len(),
                        if all.len() == 1 { "" } else { "s" }
                    )
                })
                .map_err(|e| format!("Could not keep the annotation: {e}")),
            None => {
                Err("`annotate` takes what it is `on` (a box's id or a line as `from>to`, as `shown` lists them) and the `text`.".into())
            }
        },
        "notes" => {
            let repo = here(text("path"));
            crate::shared::read_notes(&repo).map(|ns| notes(&repo, &ns)).map_err(|e| format!("Could not read the notes: {e:#}"))
        }
        "resolve" => match args.get("id").and_then(Value::as_u64).and_then(|id| u32::try_from(id).ok()) {
            Some(id) => {
                let given = |k: &str| text(k).map(str::trim).filter(|v| !v.is_empty()).map(str::to_string);
                crate::shared::resolve(&here(text("path")), id, given("commit"), given("issue"))
                    .map_err(|e| format!("Could not resolve note {id}: {e}"))
                    .and_then(|n| {
                        n.map(|n| format!("Note {} on {} is resolved; the page marks it so while it is served. Tell the person you work with what you did.", n.id, n.label))
                            .ok_or_else(|| format!("No note {id}; `notes` lists them."))
                    })
            }
            None => Err("`resolve` takes a note's `id`, a number from `notes`, and the `commit` and `issue`, if any.".into()),
        },
        #[cfg(test)]
        "panic" => panic!("a tool made to panic"),
        "choices" => graph(text("path"), opts).and_then(|g| {
            let entry = text("entry").unwrap_or("");
            story::choices(&g, entry).ok_or_else(|| format!("No route `{entry}` in {}; `routes` lists them.", g.root))
        }),
        "route" => graph(text("path"), opts).and_then(|g| {
            let entry = text("entry").unwrap_or("");
            let depth = args.get("depth").and_then(Value::as_u64).unwrap_or(2) as usize;
            story::route(&g, entry, depth).ok_or_else(|| format!("No route `{entry}` in {}; `routes` lists them.", g.root))
        }),
        _ => return None,
    })
}

/// The sight a tool is handed as its `view`, if any.
fn sight(args: &Value) -> Result<Option<Sight>, String> {
    args.get("view")
        .filter(|v| !v.is_null())
        .map(Sight::deserialize)
        .transpose()
        .map_err(|e| format!("`view` is not a view the page knows ({e}): it holds `route`, `kind`, `part`, `focus` and `open`."))
}

/// The page of the repository at `path` in `sight`, written to `out`, else to `.atlas/<project>.view.html`
/// at the top of the git repository it is in.
fn view(path: Option<&str>, sight: Option<Sight>, out: Option<&str>, opts: &Options) -> Result<String, String> {
    let (repo, clone) = located(path)?;
    let read = crate::page::reading(&[repo.clone()], opts, None, None).map_err(|e| format!("Could not read {}: {e}", repo.display()))?;
    let findings = read.findings();
    let (graph, compared) = read.drawn();
    let flow = atlas_engine::flow::of(&graph);
    let opens = crate::page::Opens { route: None, sight, served: None };
    let html =
        crate::page::Html::of(&graph, &flow, &findings, compared.as_ref(), &opens).map_err(|e| format!("Could not draw the page: {e}"))?;
    let home = clone.is_none().then_some(repo.as_path());
    let out = out.map(PathBuf::from).unwrap_or_else(|| crate::home::made(home, &format!("{}.view.html", graph.root)));
    html.write(&out).map_err(|e| format!("Could not write the page: {e:#}"))?;
    Ok(format!(
        "Wrote {}. Open it in a browser, or hand the file to the person you work with; once open, its address holds the view, to share as a link.",
        out.display()
    ))
}

/// The repository `atlas-code serve` serves, as a tool names it: a directory, else where the server runs.
fn here(path: Option<&str>) -> PathBuf {
    PathBuf::from(path.unwrap_or("."))
}

/// What the page says it shows, as text: the view, the selected box, the boxes and the lines.
fn shown(repo: &std::path::Path) -> String {
    let Some(s) = crate::shared::shown(repo) else {
        return "No page has said what it shows: run `atlas-code serve` in the repository and open it.".into();
    };
    let list = |k: &str, f: &dyn Fn(&Value) -> String| {
        s.get(k).and_then(Value::as_array).map(|xs| xs.iter().map(|x| f(x)).collect::<Vec<_>>()).unwrap_or_default()
    };
    let str_of = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let boxes = list("boxes", &|b| format!("  {} ({})", str_of(b, "label"), str_of(b, "id")));
    let lines = list("lines", &|l| {
        format!(
            "  {} → {}{} [{}]",
            str_of(l, "from"),
            str_of(l, "to"),
            Some(str_of(l, "kind")).filter(|k| !k.is_empty()).map_or(String::new(), |k| format!(" ({k})")),
            str_of(l, "on")
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

/// The notes an agent has not resolved, each with the link to its place, then those resolved while
/// the page is served, as text.
fn notes(repo: &std::path::Path, all: &[crate::shared::Note]) -> String {
    use crate::shared::State;
    let url = crate::shared::serving(repo);
    let place = |n: &crate::shared::Note| url.as_deref().map_or(String::new(), |u| format!("\n  Its place: {}", crate::shared::link(u, n)));
    let open: Vec<&crate::shared::Note> = all.iter().filter(|n| n.state != State::Resolved).collect();
    let mut out = if open.is_empty() {
        "No open notes.".to_string()
    } else {
        open.iter()
            .map(|n| {
                let lines: Vec<String> = n.text.lines().map(|l| format!("  {l}")).collect();
                format!(
                    "Note {} on {} ({}), written at {} in the view {}:\n{}{}",
                    n.id,
                    n.label,
                    n.on,
                    crate::shared::short(&n.rev),
                    n.view,
                    lines.join("\n"),
                    place(n)
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    };
    let resolved: Vec<String> = all
        .iter()
        .filter(|n| n.state == State::Resolved)
        .map(|n| format!("  {} on {}: {}", n.id, n.label, n.text.lines().last().unwrap_or("")))
        .collect();
    if !resolved.is_empty() {
        out += &format!("\n\nResolved while the page is served:\n{}", resolved.join("\n"));
    }
    out
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
    let ids: Vec<&str> = Principle::ALL.iter().map(|p| p.id()).collect();
    format!("No principle `{id}`; the principles are {}.", ids.join(", "))
}

fn ok(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn called(id: u64, name: &str, args: Value) -> Value {
        let msg = json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {"name": name, "arguments": args}});
        answer(&msg, &Options::default()).expect("a call is answered")
    }

    /// A tool that panics fails its own call, with what the panic said, as any failed call does
    /// (`isError`); the server answers the next message as before.
    #[test]
    fn a_tool_that_panics_fails_its_call_and_the_server_goes_on() {
        let reply = called(1, "panic", json!({}));
        assert_eq!(reply["id"], 1);
        assert_eq!(reply["result"]["isError"], true, "{reply}");
        assert!(reply["result"]["content"][0]["text"].as_str().unwrap().contains("a tool made to panic"), "{reply}");
        let next = called(2, "principles", json!({}));
        assert_eq!((&next["id"], &next["result"]["isError"]), (&json!(2), &json!(false)), "{next}");
        let ping = answer(&json!({"jsonrpc": "2.0", "id": 3, "method": "ping"}), &Options::default()).unwrap();
        assert_eq!(ping["result"], json!({}));
    }

    /// A note's number is a `u32`: one beyond it names no note, and is refused rather than read as
    /// another (2³² + 1 as note 1).
    #[test]
    fn a_note_number_beyond_u32_is_refused() {
        let repo = std::env::temp_dir().join(format!("atlas-mcp-{}-beyond", std::process::id()));
        let _ = std::fs::remove_dir_all(&repo);
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        let note: crate::shared::Note = serde_json::from_value(json!({"on": "spots.count", "label": "count", "text": "why?"})).unwrap();
        let id = crate::shared::add(&repo, note).unwrap().id;
        let reply = called(1, "resolve", json!({"id": (1u64 << 32) + u64::from(id), "path": repo}));
        assert_eq!(reply["result"]["isError"], true, "{reply}");
        let notes = crate::shared::notes(&repo).unwrap();
        assert_eq!(notes[0].state, crate::shared::State::Open, "note {id} is left as it was");
        let _ = std::fs::remove_dir_all(&repo);
    }
}
