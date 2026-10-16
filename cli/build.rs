//! The commit Atlas is built from, so that `atlas --version` and the page say which build drew
//! them. Outside a git checkout (a crate from crates.io) there is none.

use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn main() {
    // the build reruns when HEAD moves, a branch it points at moves, the index changes, or the
    // sources do (an edit not yet committed is marked `+`)
    for p in ["src", "../engine/src", "../viewer", "../docs"] {
        println!("cargo:rerun-if-changed={p}");
    }
    for p in ["HEAD", "index", "packed-refs"] {
        if let Some(path) = git(&["rev-parse", "--git-path", p]) {
            println!("cargo:rerun-if-changed={path}");
        }
    }
    if let Some(path) = git(&["symbolic-ref", "-q", "HEAD"]).and_then(|r| git(&["rev-parse", "--git-path", &r])) {
        println!("cargo:rerun-if-changed={path}");
    }
    let commit = git(&["rev-parse", "--short", "HEAD"])
        .map(|c| match git(&["status", "--porcelain", "--untracked-files=no"]) {
            Some(s) if !s.is_empty() => format!("{c}+"),
            _ => c,
        })
        .unwrap_or_default();
    println!("cargo:rustc-env=ATLAS_COMMIT={commit}");
    // the channel a build is made for (`nightly`), so that the version says so; none for a release
    println!("cargo:rerun-if-env-changed=ATLAS_CHANNEL");
    println!("cargo:rustc-env=ATLAS_CHANNEL={}", std::env::var("ATLAS_CHANNEL").unwrap_or_default());
}
