//! Whether a newer build of the channel this one is from is out, said once a day on stderr to
//! whoever reads it, with the command that upgrades the way Atlas was installed.
//!
//! The answer comes from a cache file the command before it left; a background thread asks
//! GitHub (through `curl`, so the binary carries no TLS) and writes the cache for the next. A
//! command never waits for it longer than a moment at its end, and a failure says nothing.

use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const DAY: u64 = 24 * 60 * 60;
const API: &str = "https://api.github.com/repos/raoulg/atlas/releases";

/// Which build this is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Build {
    /// `0.66.2`.
    pub version: String,
    /// `nightly`, or none for a release.
    pub channel: String,
    /// The commit it is made from, short, without the `+` of an edited tree.
    pub commit: String,
}

impl Build {
    pub fn this() -> Build {
        Build {
            version: env!("CARGO_PKG_VERSION").to_string(),
            channel: env!("ATLAS_CHANNEL").to_string(),
            commit: env!("ATLAS_COMMIT").trim_end_matches('+').to_string(),
        }
    }

    fn nightly(&self) -> bool {
        self.channel == "nightly"
    }
}

/// What the channel's newest build is: a release's tag (`v0.66.3`), or the nightly's commit.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Latest {
    pub release: String,
    pub nightly: String,
}

/// A version as numbers: `v0.66.3` and `0.66.3-nightly` are `(0, 66, 3)`.
fn numbers(v: &str) -> Option<(u64, u64, u64)> {
    let mut it = v.trim_start_matches('v').split('-').next()?.split('.').map(|n| n.parse::<u64>().ok());
    Some((it.next()??, it.next()??, it.next()??))
}

/// The newer build of this one's channel, as the line that says so (without how to get it); none
/// when this is the newest, or when it cannot be compared.
pub fn newer(this: &Build, latest: &Latest) -> Option<String> {
    if this.nightly() {
        let sha = latest.nightly.trim();
        let same = sha.is_empty() || this.commit.is_empty() || sha.starts_with(&this.commit) || this.commit.starts_with(sha);
        return (!same).then(|| format!("a newer nightly of atlas is out ({}; you have {})", &sha[..sha.len().min(7)], this.commit));
    }
    let (theirs, ours) = (numbers(&latest.release)?, numbers(&this.version)?);
    (theirs > ours).then(|| format!("atlas {} is out (you have {})", latest.release.trim_start_matches('v'), this.version))
}

/// The command that upgrades an install, from where the executable lives and what this build is.
pub fn upgrade(exe: &Path, nightly: bool, windows: bool) -> String {
    let path = exe.to_string_lossy().replace('\\', "/");
    if path.contains("/Cellar/atlas-code/HEAD-") {
        return "brew upgrade --fetch-HEAD atlas-code".into();
    }
    if path.contains("/Cellar/") || path.contains("/homebrew/") || path.contains("/linuxbrew/") {
        return if nightly { "brew upgrade atlas-code-nightly".into() } else { "brew upgrade atlas-code".into() };
    }
    if path.contains("/.cargo/bin/") {
        let branch = if nightly { " --branch next" } else { "" };
        return format!("cargo install --locked --git https://github.com/raoulg/atlas{branch} atlas");
    }
    let (branch, channel) = if nightly { ("next", true) } else { ("main", false) };
    if windows {
        let set = if channel { "$env:ATLAS_CHANNEL = 'nightly'; " } else { "" };
        return format!("{set}irm https://raw.githubusercontent.com/raoulg/atlas/{branch}/install.ps1 | iex");
    }
    let set = if channel { "ATLAS_CHANNEL=nightly " } else { "" };
    format!("curl -fsSL https://raw.githubusercontent.com/raoulg/atlas/{branch}/install.sh | {set}sh")
}

fn cache_file() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("LOCALAPPDATA").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?;
    Some(base.join("atlas").join("update.json"))
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// What the cache holds: when it was made, for which build, and what it found.
fn read(file: &Path) -> Option<(u64, String, Latest)> {
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(file).ok()?).ok()?;
    let text = |k: &str| v[k].as_str().unwrap_or_default().to_string();
    Some((v["checked"].as_u64()?, text("for"), Latest { release: text("release"), nightly: text("nightly") }))
}

/// The build a cached answer was found for: two nightlies share a version, so the commit says which.
fn key(this: &Build) -> String {
    format!("{}/{}/{}", this.channel, this.version, this.commit)
}

/// One GET of the GitHub API, the field `field` of its answer.
fn get(path: &str, field: &str) -> Option<String> {
    let out = Command::new("curl")
        .args(["-fsS", "--max-time", "3", "-H", "Accept: application/vnd.github+json", &format!("{API}/{path}")])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    v[field].as_str().map(str::to_string)
}

fn ask(this: &Build) -> Option<Latest> {
    Some(if this.nightly() {
        Latest { nightly: get("tags/nightly", "target_commitish")?, ..Latest::default() }
    } else {
        Latest { release: get("latest", "tag_name")?, ..Latest::default() }
    })
}

/// A check in progress: the background ask for the next command, waited for a moment at the end.
pub struct Check(Option<mpsc::Receiver<()>>);

impl Check {
    pub fn finish(self) {
        if let Some(done) = self.0 {
            let _ = done.recv_timeout(Duration::from_millis(1500));
        }
    }
}

/// Where this happens at all: a person at a terminal, who has not turned it off.
fn wanted() -> bool {
    std::io::stderr().is_terminal()
        && std::env::var_os("ATLAS_NO_UPDATE_CHECK").is_none()
        && std::env::var_os("CI").is_none()
        && !env!("ATLAS_COMMIT").is_empty()
}

/// Say what the cache knows, and ask again in the background when it is a day old.
pub fn begin() -> Check {
    if !wanted() {
        return Check(None);
    }
    let (this, Some(file)) = (Build::this(), cache_file()) else { return Check(None) };
    let cached = read(&file).filter(|(_, built, _)| *built == key(&this));
    if let Some((_, _, latest)) = &cached {
        if let Some(line) = newer(&this, latest) {
            // the executable as installed, not the link on the PATH that points to it (`/opt/homebrew/bin/atlas`)
            let exe = std::env::current_exe().map(|e| std::fs::canonicalize(&e).unwrap_or(e)).unwrap_or_default();
            let how = upgrade(&exe, this.nightly(), cfg!(windows));
            eprintln!("{line}. Upgrade with: {how}\n(ATLAS_NO_UPDATE_CHECK=1 silences this)");
        }
    }
    if cached.as_ref().map_or(false, |(checked, _, _)| now().saturating_sub(*checked) < DAY) {
        return Check(None);
    }
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        if let Some(latest) = ask(&this) {
            let body = serde_json::json!({ "checked": now(), "for": key(&this), "release": latest.release, "nightly": latest.nightly });
            if let Some(dir) = file.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(&file, body.to_string());
        }
        let _ = tx.send(());
    });
    Check(Some(rx))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(version: &str, channel: &str, commit: &str) -> Build {
        Build { version: version.into(), channel: channel.into(), commit: commit.into() }
    }

    fn release(tag: &str) -> Latest {
        Latest { release: tag.into(), ..Latest::default() }
    }

    #[test]
    fn a_release_is_newer_by_its_numbers() {
        let this = build("0.66.2", "", "a13335f");
        assert!(newer(&this, &release("v0.66.3")).unwrap().contains("0.66.3 is out (you have 0.66.2)"));
        assert!(newer(&this, &release("v0.67.0")).is_some());
        assert!(newer(&this, &release("v0.66.10")).is_some(), "numbers, not text");
        assert!(newer(&this, &release("v0.66.2")).is_none());
        assert!(newer(&this, &release("v0.66.1")).is_none());
        assert!(newer(&this, &release("latest")).is_none(), "what cannot be compared says nothing");
    }

    #[test]
    fn a_nightly_is_newer_by_its_commit() {
        let this = build("0.67.0", "nightly", "b0ead14");
        let nightly = |sha: &str| Latest { nightly: sha.into(), ..Latest::default() };
        assert!(newer(&this, &nightly("b0ead14aa5e777d0000000000000000000000000")).is_none());
        assert!(newer(&this, &nightly("c1f2e3d4aa5e777d000000000000000000000000")).unwrap().contains("c1f2e3d"));
        assert!(newer(&this, &release("v9.9.9")).is_none(), "a nightly is not compared with releases");
        assert!(newer(&this, &nightly("")).is_none());
    }

    #[test]
    fn the_upgrade_follows_the_install() {
        let up = |p: &str, nightly, windows| upgrade(Path::new(p), nightly, windows);
        assert_eq!(up("/opt/homebrew/Cellar/atlas-code/0.66.2/bin/atlas", false, false), "brew upgrade atlas-code");
        assert_eq!(up("/opt/homebrew/Cellar/atlas-code/HEAD-ab/bin/atlas", true, false), "brew upgrade --fetch-HEAD atlas-code");
        assert_eq!(
            up("/opt/homebrew/Cellar/atlas-code-nightly/0.66.2-nightly.202610171105/bin/atlas", true, false),
            "brew upgrade atlas-code-nightly"
        );
        assert_eq!(up("/opt/homebrew/bin/atlas", true, false), "brew upgrade atlas-code-nightly", "a link on the PATH");
        assert_eq!(up("/opt/homebrew/bin/atlas", false, false), "brew upgrade atlas-code");
        assert_eq!(
            up("/opt/homebrew/Cellar/atlas-code-nightly/0.68.0-nightly.202610270300/bin/atlas-nightly", true, false),
            "brew upgrade atlas-code-nightly",
            "the nightly beside the release"
        );
        assert_eq!(up("/opt/homebrew/bin/atlas-nightly", true, false), "brew upgrade atlas-code-nightly");
        assert!(up("/home/x/.local/bin/atlas-nightly", true, false).contains("| ATLAS_CHANNEL=nightly sh"));
        assert!(up("C:\\Users\\x\\AppData\\Local\\Programs\\atlas\\atlas-nightly.exe", true, true)
            .starts_with("$env:ATLAS_CHANNEL = 'nightly'; "));
        assert!(up("/home/x/.cargo/bin/atlas", true, false).contains("--branch next"));
        assert!(up("/home/x/.local/bin/atlas", false, false).ends_with("install.sh | sh"));
        assert!(up("/home/x/.local/bin/atlas", true, false).contains("| ATLAS_CHANNEL=nightly sh"));
        assert!(up("C:\\Users\\x\\AppData\\Local\\Programs\\atlas\\atlas.exe", false, true).ends_with("install.ps1 | iex"));
        assert!(up("C:\\Users\\x\\AppData\\Local\\Programs\\atlas\\atlas.exe", true, true).starts_with("$env:ATLAS_CHANNEL = 'nightly'; "));
    }

    #[test]
    fn a_cached_answer_belongs_to_one_build() {
        assert_ne!(key(&build("0.68.0", "nightly", "edce364")), key(&build("0.68.0", "nightly", "e3e2584")));
    }
}
