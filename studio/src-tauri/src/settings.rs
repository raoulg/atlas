//! Studio's settings: its theme, kept in its configuration folder and set on every window, and
//! whether the coding agents on this machine have Atlas's MCP server, registered for every project as
//! `docs/reference/install.md` says (`claude mcp add --scope user atlas -- atlas mcp`, and Codex's
//! equivalent). Registering runs that same command; connecting to it again is the agent's own.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use tauri::{AppHandle, Manager, Runtime, Theme};

fn file<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|dir| dir.join("settings.json"))
}

/// The theme chosen: `system`, `light` or `dark`.
pub fn theme<R: Runtime>(app: &AppHandle<R>) -> String {
    file(app)
        .and_then(|file| std::fs::read(file).ok())
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|value| value["theme"].as_str().map(str::to_string))
        .filter(|theme| ["system", "light", "dark"].contains(&theme.as_str()))
        .unwrap_or_else(|| "system".into())
}

/// Set the theme on every window, the Atlas views in them too, and keep it.
pub fn set_theme<R: Runtime>(app: &AppHandle<R>, theme: &str) -> Result<(), String> {
    apply(app, theme);
    let file = file(app).ok_or("Studio has no configuration folder")?;
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    }
    std::fs::write(file, serde_json::json!({ "theme": theme }).to_string()).map_err(|error| error.to_string())
}

pub fn apply<R: Runtime>(app: &AppHandle<R>, theme: &str) {
    app.set_theme(match theme {
        "light" => Some(Theme::Light),
        "dark" => Some(Theme::Dark),
        _ => None,
    });
}

/// A program on the `PATH`.
fn which(program: &str) -> Option<PathBuf> {
    let names: Vec<String> = if cfg!(windows) { vec![format!("{program}.exe"), format!("{program}.cmd")] } else { vec![program.into()] };
    std::env::split_paths(&std::env::var_os("PATH")?)
        .flat_map(|dir| names.iter().map(move |name| dir.join(name)))
        .find(|path| path.is_file())
}

/// The `atlas` an agent starts: the one on the `PATH`, else Studio's own.
fn atlas() -> Option<PathBuf> {
    which("atlas").or_else(|| {
        let own = std::env::current_exe().ok()?.with_file_name(if cfg!(windows) { "atlas.exe" } else { "atlas" });
        own.is_file().then_some(own)
    })
}

/// A coding agent, and whether it has Atlas.
#[derive(serde::Serialize)]
pub struct Agent {
    pub id: &'static str,
    pub name: &'static str,
    /// Whether the agent's command line is on this machine.
    pub installed: bool,
    /// Whether it has Atlas's MCP server, for every project or for the workspace.
    pub registered: bool,
    /// The command that registers it.
    pub command: String,
    /// How the agent connects to it once it is registered.
    pub reconnect: &'static str,
}

/// Whether `config` (Claude Code's `~/.claude.json`, or a project's `.mcp.json`) names a server `atlas`.
fn names_atlas(config: &Path) -> bool {
    std::fs::read(config)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .is_some_and(|value| value["mcpServers"].get("atlas").is_some())
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from)
}

/// The agents Studio knows, for the workspace at `workspace`.
pub fn agents(workspace: Option<&Path>) -> Vec<Agent> {
    let atlas = atlas().map(|path| path.display().to_string()).unwrap_or_else(|| "atlas".into());
    let home = home();
    let claude = home.as_ref().is_some_and(|home| names_atlas(&home.join(".claude.json")))
        || workspace.is_some_and(|w| names_atlas(&w.join(".mcp.json")));
    let codex = home
        .as_ref()
        .and_then(|home| std::fs::read_to_string(home.join(".codex").join("config.toml")).ok())
        .is_some_and(|text| text.lines().any(|line| line.trim() == "[mcp_servers.atlas]"));
    vec![
        Agent {
            id: "claude",
            name: "Claude Code",
            installed: which("claude").is_some(),
            registered: claude,
            command: format!("claude mcp add --scope user atlas -- {} mcp", quoted(&atlas)),
            reconnect: "Start a new Claude Code session, or run /mcp in the one you have.",
        },
        Agent {
            id: "codex",
            name: "Codex",
            installed: which("codex").is_some(),
            registered: codex,
            command: format!("codex mcp add atlas -- {} mcp", quoted(&atlas)),
            reconnect: "Start a new Codex session.",
        },
    ]
}

/// A path as one word of a command line.
fn quoted(path: &str) -> String {
    if path.contains(' ') {
        format!("\"{path}\"")
    } else {
        path.to_string()
    }
}

/// Register Atlas with the agent `id`, by its own command line.
pub fn register(id: &str) -> Result<(), String> {
    let atlas = atlas().ok_or("No atlas to register: install the atlas CLI, or run Studio from its app")?;
    let (program, args): (&str, Vec<String>) = match id {
        "claude" => ("claude", vec!["mcp".into(), "add".into(), "--scope".into(), "user".into(), "atlas".into(), "--".into()]),
        "codex" => ("codex", vec!["mcp".into(), "add".into(), "atlas".into(), "--".into()]),
        _ => return Err(format!("Studio does not know the agent {id}")),
    };
    let program = which(program).ok_or(format!("{program} is not on the PATH"))?;
    let out = Command::new(program).args(args).arg(atlas).arg("mcp").stdin(Stdio::null()).output().map_err(|error| error.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_config_that_names_atlas_has_it() {
        let dir = std::env::temp_dir().join(format!("atlas-studio-settings-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let config = dir.join(".mcp.json");
        std::fs::write(&config, r#"{"mcpServers": {"atlas": {"command": "atlas", "args": ["mcp"]}}}"#).unwrap();
        assert!(names_atlas(&config));
        std::fs::write(&config, r#"{"mcpServers": {"other": {}}}"#).unwrap();
        assert!(!names_atlas(&config));
        assert!(!names_atlas(&dir.join("missing.json")));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_path_with_a_space_is_one_word() {
        assert_eq!(
            quoted("/Applications/Atlas Studio.app/Contents/MacOS/atlas"),
            "\"/Applications/Atlas Studio.app/Contents/MacOS/atlas\""
        );
        assert_eq!(quoted("/opt/homebrew/bin/atlas"), "/opt/homebrew/bin/atlas");
    }
}
