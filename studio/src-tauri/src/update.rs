//! Whether a newer build of the channel Studio is from is out, asked once a day, as the CLI asks it
//! (`cli/src/update.rs`): a stable build looks for a newer release, a nightly for a newer commit of
//! the nightly. Which build Studio is, its sidecar says (`atlas --version`): it is of the same commit
//! and channel. GitHub is asked through `curl`, so Studio carries no TLS of its own, and the last
//! answer is kept in Studio's configuration folder. Updating runs the install script of the channel
//! into the folder Studio runs from (macOS), or the setup program (Windows).

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

const DAY: u64 = 24 * 60 * 60;
const API: &str = "https://api.github.com/repos/raoulg/atlas/releases";

/// Which build this is: `0.68.0`, nightly or not, and the commit it is made from.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Build {
    pub version: String,
    pub nightly: bool,
    pub commit: String,
}

/// The build `atlas --version` names: `atlas 0.68.0-nightly (9a7ecf0)`, `atlas 0.68.0 (9a7ecf0+)`.
pub fn build_of(said: &str) -> Option<Build> {
    let rest = said.trim().strip_prefix("atlas ")?;
    let (version, commit) = match rest.split_once(" (") {
        Some((v, c)) => (v, c.trim_end_matches(')').trim_end_matches('+')),
        None => (rest, ""),
    };
    let nightly = version.ends_with("-nightly");
    Some(Build { version: version.trim_end_matches("-nightly").to_string(), nightly, commit: commit.to_string() })
}

/// The newest build of the channels: a release's tag (`v0.69.0`), and the nightly's commit.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Latest {
    pub release: String,
    pub nightly: String,
}

/// A newer build than this one, or this one itself (`current`), with what changed in it, as the window
/// shows it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Newer {
    /// `0.69.0`, or the nightly's commit, short.
    pub version: String,
    pub nightly: bool,
    /// What changed: the release's page of notes, or the commits between the two nightlies.
    pub notes: String,
    /// Whether `notes` is a page of release notes, shown in the window; else a page for the browser.
    pub page: bool,
    /// Whether Studio can install it in place here.
    pub installs: bool,
    /// Whether it is the build that runs, whose changes are asked for.
    pub current: bool,
}

/// A version as numbers: `v0.66.3` and `0.66.3` are `(0, 66, 3)`.
fn numbers(v: &str) -> Option<(u64, u64, u64)> {
    let mut it = v.trim_start_matches('v').split('-').next()?.split('.').map(|n| n.parse::<u64>().ok());
    Some((it.next()??, it.next()??, it.next()??))
}

/// The newer build of this one's channel; none when this is the newest, or when it cannot be told.
pub fn newer(this: &Build, latest: &Latest) -> Option<Newer> {
    let installs = cfg!(any(target_os = "macos", target_os = "windows"));
    if this.nightly {
        let sha = latest.nightly.trim();
        if sha.is_empty() || this.commit.is_empty() || sha.starts_with(&this.commit) || this.commit.starts_with(sha) {
            return None;
        }
        let short = &sha[..sha.len().min(7)];
        let notes = format!("https://github.com/raoulg/atlas/compare/{}...{short}", this.commit);
        return Some(Newer { version: short.to_string(), nightly: true, notes, page: false, installs, current: false });
    }
    let (theirs, ours) = (numbers(&latest.release)?, numbers(&this.version)?);
    (theirs > ours).then(|| {
        let version = latest.release.trim_start_matches('v').to_string();
        let notes = format!("https://raoulg.github.io/atlas/releases/{version}/");
        Newer { version, nightly: false, notes, page: true, installs, current: false }
    })
}

/// `newer` as it can be shown: a release without its page of notes (one made before the pages, or
/// whose page is not up yet) shows its release on GitHub, in the browser.
pub fn shown(mut newer: Newer) -> Newer {
    if newer.page && !answers(&newer.notes) {
        newer.notes = format!("https://github.com/raoulg/atlas/releases/tag/v{}", newer.version);
        newer.page = false;
    }
    newer
}

/// What changed in this build: its release's page of notes, in the window; else, in the browser, its
/// release on GitHub, or for a build that is no release (a nightly, or one built here before its release)
/// the commits it is made of.
pub fn changes(this: &Build) -> Newer {
    let version = this.version.clone();
    let at =
        |notes: String, page: bool| Newer { version: version.clone(), nightly: this.nightly, notes, page, installs: false, current: true };
    if !this.nightly {
        let page = format!("https://raoulg.github.io/atlas/releases/{version}/");
        if answers(&page) {
            return at(page, true);
        }
        let release = format!("https://github.com/raoulg/atlas/releases/tag/v{version}");
        if answers(&release) {
            return at(release, false);
        }
    }
    let commit = if this.commit.is_empty() { "next" } else { this.commit.as_str() };
    at(format!("https://github.com/raoulg/atlas/commits/{commit}"), false)
}

/// Whether a page is there.
fn answers(url: &str) -> bool {
    Command::new("curl")
        .args(["-fsSI", "--max-time", "10", url])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// What GitHub says one field of a release is.
fn get(path: &str, field: &str) -> Option<String> {
    let out = Command::new("curl")
        .args(["-fsS", "--max-time", "10", "-H", "Accept: application/vnd.github+json", &format!("{API}/{path}")])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    value[field].as_str().map(str::to_string)
}

/// The newest build of this one's channel, asked of GitHub now.
pub fn ask(this: &Build) -> Option<Latest> {
    Some(if this.nightly {
        Latest { nightly: get("tags/nightly", "target_commitish")?, ..Latest::default() }
    } else {
        Latest { release: get("latest", "tag_name")?, ..Latest::default() }
    })
}

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_secs())
}

/// The last answer, and when it was asked.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Asked {
    pub checked: u64,
    pub latest: Latest,
}

impl Asked {
    /// Whether the answer is a day old, or none was asked yet.
    pub fn stale(&self, now: u64) -> bool {
        now.saturating_sub(self.checked) >= DAY
    }
}

pub fn read(file: &Path) -> Asked {
    std::fs::read(file).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
}

pub fn keep(file: &Path, asked: &Asked) {
    if let Some(dir) = file.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = serde_json::to_vec(asked).map(|bytes| std::fs::write(file, bytes));
}

/// The folder the running app lives in, where an update installs it: `~/Applications` for
/// `~/Applications/Atlas Studio.app/Contents/MacOS/atlas-studio`.
#[cfg(target_os = "macos")]
fn installed_in(exe: &Path) -> Option<std::path::PathBuf> {
    let app = exe.ancestors().find(|a| a.extension().is_some_and(|e| e == "app"))?;
    app.parent().map(Path::to_path_buf)
}

/// Install the newest build of this one's channel. On macOS the install script puts it where this
/// app is, for Studio to start again on it; on Windows the setup program starts, passive, for Studio
/// to close and be started again by it.
pub fn install(this: &Build) -> Result<(), String> {
    let branch = if this.nightly { "next" } else { "main" };
    let channel = if this.nightly { "nightly" } else { "" };
    #[cfg(target_os = "macos")]
    {
        let exe = std::env::current_exe().map_err(|error| error.to_string())?;
        let dir = installed_in(&exe).ok_or("Studio does not run from an app bundle")?;
        let script = format!("curl -fsSL https://raw.githubusercontent.com/raoulg/atlas/{branch}/install-studio.sh | sh");
        let out = Command::new("sh")
            .args(["-c", &script])
            .env("ATLAS_STUDIO_DIR", &dir)
            .env("ATLAS_CHANNEL", channel)
            .stdin(Stdio::null())
            .output()
            .map_err(|error| error.to_string())?;
        if !out.status.success() {
            return Err(format!("The update did not install: {}", String::from_utf8_lossy(&out.stderr).trim()));
        }
        Ok(())
    }
    #[cfg(target_os = "windows")]
    {
        let from = if this.nightly { "download/nightly" } else { "latest/download" };
        let setup = std::env::temp_dir().join("atlas-studio-x86_64-pc-windows-msvc-setup.exe");
        let url = format!("https://github.com/raoulg/atlas/releases/{from}/atlas-studio-x86_64-pc-windows-msvc-setup.exe");
        let got = Command::new("curl").args(["-fsSL", "-o"]).arg(&setup).arg(&url).status().map_err(|error| error.to_string())?;
        if !got.success() {
            return Err("The update could not be downloaded".into());
        }
        // passive: a progress bar and no questions; /R starts Studio again when it is done
        Command::new(&setup).args(["/P", "/R"]).spawn().map_err(|error| error.to_string())?;
        let _ = (branch, channel);
        Ok(())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (branch, channel);
        Err("Studio installs updates itself on macOS and Windows only".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sidecar_says_which_build_it_is() {
        assert_eq!(
            build_of("atlas 0.68.0-nightly (9a7ecf0)\n"),
            Some(Build { version: "0.68.0".into(), nightly: true, commit: "9a7ecf0".into() })
        );
        assert_eq!(build_of("atlas 0.68.0 (9a7ecf0+)"), Some(Build { version: "0.68.0".into(), nightly: false, commit: "9a7ecf0".into() }));
        assert_eq!(build_of("something else"), None);
    }

    #[test]
    fn a_release_is_newer_by_its_number_and_shows_its_page() {
        let this = Build { version: "0.68.0".into(), nightly: false, commit: "abc".into() };
        let newer = newer(&this, &Latest { release: "v0.69.0".into(), ..Latest::default() }).unwrap();
        assert_eq!(newer.version, "0.69.0");
        assert_eq!(newer.notes, "https://raoulg.github.io/atlas/releases/0.69.0/");
        assert!(newer.page);
        assert_eq!(super::newer(&this, &Latest { release: "v0.68.0".into(), ..Latest::default() }), None);
        assert_eq!(super::newer(&this, &Latest { release: "v0.67.9".into(), ..Latest::default() }), None);
    }

    #[test]
    fn a_nightly_is_newer_by_its_commit() {
        let this = Build { version: "0.68.0".into(), nightly: true, commit: "9a7ecf0".into() };
        assert_eq!(newer(&this, &Latest { nightly: "9a7ecf0d5e".into(), ..Latest::default() }), None);
        let newer = newer(&this, &Latest { nightly: "19ec16bb11".into(), ..Latest::default() }).unwrap();
        assert_eq!((newer.version.as_str(), newer.nightly), ("19ec16b", true));
        assert!(newer.notes.ends_with("compare/9a7ecf0...19ec16b"));
    }

    #[test]
    fn an_answer_is_asked_again_after_a_day() {
        let asked = Asked { checked: 1_000, latest: Latest::default() };
        assert!(!asked.stale(1_000 + DAY - 1));
        assert!(asked.stale(1_000 + DAY));
        assert!(Asked::default().stale(now()));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn an_update_installs_where_the_app_is() {
        let exe = Path::new("/Users/me/Applications/Atlas Studio.app/Contents/MacOS/atlas-studio");
        assert_eq!(installed_in(exe), Some(std::path::PathBuf::from("/Users/me/Applications")));
        assert_eq!(installed_in(Path::new("/tmp/target/debug/atlas-studio")), None);
    }
}
