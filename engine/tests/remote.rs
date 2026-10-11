//! A repository read from its URL: one folder of it, at one revision, as a local one is read.

use std::path::{Path, PathBuf};
use std::process::Command;

use atlas_engine::{parts, remote, Options};

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git").arg("-C").arg(dir).args(["-c", "user.name=atlas", "-c", "user.email=atlas@example.org", "-c", "commit.gpgsign=false"]).args(args).status().expect("git runs").success();
    assert!(ok, "git {args:?}");
}

#[test]
fn a_tree_url_reads_one_folder() {
    let repo = std::env::temp_dir().join(format!("atlas-remote-{}", std::process::id())).join("tiny-repo");
    let _ = std::fs::remove_dir_all(&repo);
    std::fs::create_dir_all(repo.join("src/tiny")).unwrap();
    std::fs::write(repo.join("pyproject.toml"), "[project]\nname = \"tiny\"\nversion = \"0\"\n").unwrap();
    std::fs::write(repo.join("src/tiny/__init__.py"), "").unwrap();
    std::fs::write(repo.join("src/tiny/core.py"), "def double(x: int) -> int:\n    return 2 * x\n").unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "tiny"]);
    let path = atlas_engine::tree::path_str(&repo);
    let url = if path.starts_with('/') { format!("file://{path}/tree/main/src/tiny") } else { format!("file:///{path}/tree/main/src/tiny") };
    assert!(remote::is_url(&url));
    let cloned = remote::clone(&url).expect("the folder is cloned");
    let g = parts::analyze_repo(&[cloned.dir.clone()], &Options::default()).unwrap();
    assert_eq!(g.root, "tiny");
    assert!(g.node("tiny.core.double").is_some(), "{:?}", g.nodes.iter().map(|n| &n.id).collect::<Vec<_>>());
    let base: PathBuf = cloned.dir.clone();
    drop(cloned);
    assert!(!base.exists(), "the clone is gone once read");
    let _ = std::fs::remove_dir_all(repo.parent().unwrap());
}
