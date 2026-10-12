//! A change shows as itself: the TypeScript fixture in a repository of its own, committed,
//! then edited.

use std::path::{Path, PathBuf};
use std::process::Command;

use atlas_engine::changes;

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git").arg("-C").arg(dir).args(["-c", "user.name=atlas", "-c", "user.email=atlas@example.org", "-c", "commit.gpgsign=false"]).args(args).status().expect("git runs").success();
    assert!(ok, "git {args:?}");
}

/// The fixture copied into a fresh repository, committed: its root keeps the fixture's name.
fn repository(name: &str) -> PathBuf {
    let from = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixture-ts");
    let repo = std::env::temp_dir().join(format!("atlas-test-{name}-{}", std::process::id())).join("fixture-ts");
    let _ = std::fs::remove_dir_all(&repo);
    for e in walkdir::WalkDir::new(&from).into_iter().flatten().filter(|e| e.file_type().is_file()) {
        let dest = repo.join(e.path().strip_prefix(&from).unwrap());
        std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
        std::fs::copy(e.path(), dest).unwrap();
    }
    git(&repo, &["init", "-q"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "fixture"]);
    repo
}

/// A handler that no longer asks the model and one that writes a log: two routes changed, each
/// by exactly its step and its place.
#[test]
fn a_new_write_and_a_removed_call_show_as_themselves() {
    let repo = repository("changes");
    let regels = repo.join("src/server/regels.ts");
    let src = std::fs::read_to_string(&regels).unwrap();
    let src = src.replace("[await vat(\"x\")]", "[\"x\"]");
    let src = src.replace("import { Hono } from \"hono\";", "import fs from \"node:fs\";\nimport { Hono } from \"hono\";");
    let src = src.replace("    await pool.query(BEZOEK);", "    fs.appendFileSync(\"logs/regels.log\", c.req.param(\"id\"));\n    await pool.query(BEZOEK);");
    std::fs::write(&regels, src).unwrap();
    let c = changes::since(&repo, "HEAD", &atlas_engine::Options::default()).expect("the working tree against HEAD");
    let routes: Vec<&str> = c.routes.iter().map(|r| r.route.as_str()).collect();
    assert_eq!(routes, ["API · GET /api/regels/{id}", "API · POST /api/regels"], "{}", changes::report(&c));
    let (get, post) = (&c.routes[0], &c.routes[1]);
    assert_eq!(get.places_added, ["writes logs/regels.log"]);
    assert!(get.places_removed.is_empty() && get.steps_added.is_empty() && get.steps_removed.is_empty());
    assert_eq!(post.steps_removed, ["fixture-ts.server.regels.vat"]);
    assert_eq!((post.id.as_str(), post.root.as_str()), ("fixture-ts.server.regels.regels.POST /", "fixture-ts"));
    assert_eq!(get.touches_added.iter().map(|t| (t.touch.kind_name(), t.touch.artifact.as_str())).collect::<Vec<_>>(), [("disk_write", "logs/regels.log")]);
    assert!(post.places_removed.iter().all(|p| p.starts_with("asks an LLM")) && post.places_removed.len() == 1, "{:?}", post.places_removed);
    assert!(post.places_added.is_empty() && post.steps_added.is_empty());
    let report = changes::report(&c);
    assert!(report.contains("~ API · POST /api/regels\n    - step server.regels.vat\n"), "{report}");
    let _ = std::fs::remove_dir_all(repo.parent().unwrap());
}

/// Nothing changed is said in words.
#[test]
fn an_unchanged_tree_says_so() {
    let repo = repository("same");
    let c = changes::since(&repo, "HEAD", &atlas_engine::Options::default()).unwrap();
    assert!(c.routes.is_empty() && c.findings_added.is_empty() && c.findings_resolved.is_empty(), "{}", changes::report(&c));
    assert!(changes::report(&c).starts_with("nothing changed"));
    let _ = std::fs::remove_dir_all(repo.parent().unwrap());
}
