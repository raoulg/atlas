//! The page `atlas view` writes: the graph `atlas analyze` writes, with the findings `atlas check`
//! lists, each at a node the picture knows.

use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

fn root() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
}

/// Atlas run in the workspace root, keeping what it keeps in a cache of the tests' own.
fn atlas(args: &[&str]) -> String {
    let cache = std::env::temp_dir().join(format!("atlas-view-cache-{}", std::process::id()));
    let out = Command::new(env!("CARGO_BIN_EXE_atlas")).args(args).current_dir(root()).env("XDG_CACHE_HOME", cache).output().expect("atlas runs");
    String::from_utf8(out.stdout).unwrap()
}

fn scratch(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("atlas-view-{}-{name}", std::process::id()))
}

/// The project a page draws: its graph, flow and findings.
fn page_of(repo: &str) -> Value {
    let out = scratch(&format!("{}.html", repo.replace('/', "-")));
    atlas(&["view", repo, "--no-open", "--out", out.to_str().unwrap()]);
    let html = std::fs::read_to_string(&out).unwrap();
    let _ = std::fs::remove_file(&out);
    let data = &html[html.find("window.DATA = ").expect("the page holds its data") + "window.DATA = ".len()..];
    let data: Value = serde_json::Deserializer::from_str(data).into_iter().next().unwrap().unwrap();
    data["projects"].as_object().unwrap().values().next().unwrap().clone()
}

/// What `atlas check --json` prints: one report per part, each part's findings in turn.
fn checked(repo: &str) -> Vec<Value> {
    let text = atlas(&["check", repo, "--json"]);
    serde_json::Deserializer::from_str(&text).into_iter::<Value>().flat_map(|r| r.unwrap()["findings"].as_array().unwrap().clone()).collect()
}

fn analyzed(repo: &str) -> Value {
    let out = scratch(&format!("{}.json", repo.replace('/', "-")));
    atlas(&["analyze", repo, "-o", out.to_str().unwrap()]);
    let g = serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
    let _ = std::fs::remove_file(&out);
    g
}

/// A page holds the findings `atlas check` lists, part by part, and each is about a node of the
/// graph it draws; that graph is the one `atlas analyze` reads, its parts merged and joined.
#[test]
fn a_page_holds_what_check_finds() {
    for repo in ["engine/tests/fixture", "engine/tests/fixture-parts"] {
        let mut page = page_of(repo);
        let findings = page["findings"].as_array().cloned().unwrap_or_default();
        assert!(!findings.is_empty(), "{repo} has findings to mark");
        assert_eq!(findings, checked(repo), "{repo}: the page's findings are check's, with the same values");
        let nodes: Vec<&str> = page["nodes"].as_array().unwrap().iter().map(|n| n["id"].as_str().unwrap()).collect();
        for f in &findings {
            assert!(nodes.contains(&f["node"].as_str().unwrap()), "{repo}: {} is a node of the picture", f["node"]);
        }
        let g = page.as_object_mut().unwrap();
        for k in ["flow", "findings", "open", "view"] {
            g.remove(k);
        }
        assert!(Value::Object(g.clone()) == analyzed(repo), "{repo}: the page draws the graph `atlas analyze` reads");
    }
}

/// Atlas asks nothing of itself, so its own picture carries no mark.
#[test]
fn atlas_marks_nothing_in_its_own_picture() {
    assert_eq!(page_of(".")["findings"], serde_json::json!([]));
}
