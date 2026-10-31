//! What the exit status says, as `atlas-code --help` writes it: 0 when `atlas-code check` has nothing to
//! ask, 1 when it has, 2 when Atlas could not do what was asked; and whatever it says, the process
//! ends with what it holds dropped, the temporary clone of a git URL among it.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

fn root() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
}

/// A directory of the test's own, empty.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("atlas-exit-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Atlas run with `tmp` as the place it lays out what it removes again, keeping nothing.
fn atlas(args: &[&str], tmp: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_atlas-code"))
        .arg("--no-cache")
        .args(args)
        .current_dir(root())
        .env("ATLAS_NO_UPDATE_CHECK", "1")
        .env("TMPDIR", tmp)
        .env("TMP", tmp)
        .env("TEMP", tmp)
        .output()
        .expect("atlas runs")
}

/// Whether `atlas-code check --json` asks anything: a finding no decision sets aside, or a decision that
/// sets nothing aside.
fn asks(report: &Value) -> bool {
    let open = report["parts"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|p| p["findings"].as_array().unwrap())
        .any(|f| f.get("aside").map_or(true, Value::is_null));
    open || !report["stale"].as_array().unwrap().is_empty()
}

/// The status of `atlas-code check` says whether it has something to ask, as its report does: on the
/// pair of a choice, the clean side asks nothing and the shortcut asks.
#[test]
fn the_status_says_whether_check_asks() {
    let tmp = scratch("asks");
    let mut said = vec![];
    for side in ["clean", "shortcut"] {
        let repo = format!("engine/tests/pairs/choices/rust/{side}");
        let report: Value = serde_json::from_slice(&atlas(&["check", &repo, "--json"], &tmp).stdout).expect("one JSON document");
        let status = atlas(&["check", &repo], &tmp).status.code();
        assert_eq!(status, Some(i32::from(asks(&report))), "{side}: the status and the report disagree");
        said.push(asks(&report));
    }
    assert_eq!(said, [false, true], "the clean side asks nothing, the shortcut asks");
    let _ = std::fs::remove_dir_all(&tmp);
}

/// What Atlas cannot read, it says on standard error and exits with 2, as it does for a command line
/// it does not take: a failure is never read as a finding.
#[test]
fn a_failure_is_2() {
    let tmp = scratch("fails");
    let missing = tmp.join("no-such-repository");
    for args in [vec!["check", missing.to_str().unwrap()], vec!["check", "--no-such-flag"]] {
        let out = atlas(&args, &tmp);
        assert_eq!(out.status.code(), Some(2), "{args:?}: {}", String::from_utf8_lossy(&out.stderr));
        assert!(!out.stderr.is_empty(), "{args:?}: the failure is said");
    }
    let _ = std::fs::remove_dir_all(&tmp);
}

/// The clone of a git URL is removed when Atlas ends, whatever its status: with something to ask,
/// and when what the URL names cannot be read.
#[test]
fn a_clone_is_removed_whatever_the_status() {
    let tmp = scratch("clone");
    let repo = scratch("origin");
    let pair = root().join("engine/tests/pairs/choices/rust/shortcut");
    for file in ["Cargo.toml", "src/lib.rs"] {
        std::fs::create_dir_all(repo.join(file).parent().unwrap()).unwrap();
        std::fs::copy(pair.join(file), repo.join(file)).unwrap();
    }
    let git = |args: &[&str]| {
        let ok = Command::new("git")
            .args(["-c", "user.name=atlas", "-c", "user.email=atlas@example.com", "-c", "commit.gpgsign=false"])
            .args(args)
            .current_dir(&repo)
            .output()
            .expect("git runs")
            .status
            .success();
        assert!(ok, "git {args:?}");
    };
    git(&["init", "--quiet"]);
    git(&["add", "."]);
    git(&["commit", "--quiet", "-m", "the shortcut"]);
    let url = |dir: &Path| format!("file:///{}", dir.to_str().unwrap().replace('\\', "/").trim_start_matches('/'));
    let clones = || {
        std::fs::read_dir(&tmp)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("atlas-clone-"))
            .count()
    };
    for (at, status) in [(url(&repo), 1), (url(&repo.join("not-there")), 2)] {
        let out = atlas(&["check", &at], &tmp);
        assert_eq!(out.status.code(), Some(status), "{at}: {}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(clones(), 0, "{at}: the clone is left behind");
    }
    let _ = std::fs::remove_dir_all(&tmp);
    let _ = std::fs::remove_dir_all(&repo);
}

/// The help says what each status means.
#[test]
fn the_help_says_the_statuses() {
    let tmp = scratch("help");
    let help = String::from_utf8(atlas(&["--help"], &tmp).stdout).unwrap();
    let section = &help[help.find("Exit status:").expect("an exit status section")..];
    for code in ["0", "1", "2"] {
        assert!(section.lines().any(|l| l.trim_start().split_whitespace().next() == Some(code)), "status {code} is named:\n{section}");
    }
    let _ = std::fs::remove_dir_all(&tmp);
}
