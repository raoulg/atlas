//! `atlas mcp` spoken to as a client would: one JSON-RPC message per line.

use std::io::Write;
use std::process::{Command, Stdio};

use serde_json::{json, Value};

/// Send `msgs` to a fresh server in the workspace root and collect its replies.
fn session(msgs: &[Value]) -> Vec<Value> {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/..");
    let mut child = Command::new(env!("CARGO_BIN_EXE_atlas")).arg("mcp").current_dir(root).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().expect("atlas mcp starts");
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
        call(5, "route", json!({ "entry": "analyze" })),
        call(6, "explain", json!({ "principle": "kiss" })),
        call(7, "nothing", json!({})),
    ]);
    assert_eq!(replies.len(), 7, "a notification gets no answer");
    assert_eq!(replies[0]["result"]["protocolVersion"], "2025-06-18", "the client's version, when spoken");
    assert!(replies[0]["result"]["instructions"].as_str().unwrap().contains("coaches"));
    let tools: Vec<&str> = replies[1]["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(tools, ["principles", "check", "explain", "routes", "route"]);
    assert!(text(&replies[2]).contains("prepare tests `mode`"), "{}", text(&replies[2]));
    assert!(!text(&replies[2]).contains("\ndoors ·"), "only the principle asked for");
    assert!(text(&replies[3]).starts_with("A sum type is unwrapped where it arises."));
    assert!(text(&replies[4]).contains("4.1a  load(path) → Architecture"), "{}", text(&replies[4]));
    assert_eq!(replies[5]["result"]["isError"], true, "an unknown principle is the tool's error, said in words");
    assert_eq!(replies[6]["error"]["code"], -32602, "an unknown tool is a protocol error");
}
