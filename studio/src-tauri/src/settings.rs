//! Studio's settings: its theme, kept in its configuration folder and set on every window, and
//! whether the coding agents on this machine have Atlas's MCP server, registered for every project as
//! `docs/reference/install.md` says (`claude mcp add --scope user atlas -- atlas mcp`, and Codex's
//! equivalent). Registering runs that same command; connecting to it again is the agent's own.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use tauri::{AppHandle, Emitter, Manager, Runtime, Theme};

fn file<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|dir| dir.join("settings.json"))
}

/// How Studio looks: the mode (`system`, `light` or `dark`) and the theme it shows in each of light and
/// dark, by the id `ui/themes.js` gives it; a theme Studio does not know is shown as Atlas's own.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct Appearance {
    pub theme: String,
    pub light: String,
    pub dark: String,
}

impl Default for Appearance {
    fn default() -> Appearance {
        Appearance { theme: "system".into(), light: "atlas-light".into(), dark: "atlas-dark".into() }
    }
}

impl Appearance {
    /// The appearance with each field as Studio knows it: a mode of the three, a theme's id a name of letters, digits
    /// and dashes; anything else as it was by default.
    fn checked(self) -> Appearance {
        let id = |s: &str| !s.is_empty() && s.len() <= 40 && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        let fallback = Appearance::default();
        Appearance {
            theme: if ["system", "light", "dark"].contains(&self.theme.as_str()) { self.theme } else { fallback.theme },
            light: if id(&self.light) { self.light } else { fallback.light },
            dark: if id(&self.dark) { self.dark } else { fallback.dark },
        }
    }
}

/// The appearance chosen.
pub fn appearance<R: Runtime>(app: &AppHandle<R>) -> Appearance {
    let read = file(app)
        .and_then(|file| std::fs::read(file).ok())
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .unwrap_or_default();
    let field = |key: &str, fallback: String| read[key].as_str().map_or(fallback, str::to_string);
    let fallback = Appearance::default();
    Appearance { theme: field("theme", fallback.theme), light: field("light", fallback.light), dark: field("dark", fallback.dark) }
        .checked()
}

/// Set the appearance on every window, the Atlas views in them too, and keep it.
pub fn set_appearance<R: Runtime>(app: &AppHandle<R>, chosen: Appearance) -> Result<(), String> {
    let chosen = chosen.checked();
    apply(app, &chosen.theme);
    let _ = app.emit("appearance", &chosen);
    let file = file(app).ok_or("Studio has no configuration folder")?;
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    }
    std::fs::write(file, serde_json::to_string(&chosen).map_err(|error| error.to_string())?).map_err(|error| error.to_string())
}

/// The mode on every window: the system's, or light or dark.
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

    /// An appearance read from the file is one Studio can show: a mode it does not know is the system's, a
    /// theme's id that is no name is Atlas's own; a known one is kept as it was written.
    #[test]
    fn an_appearance_is_checked_field_by_field() {
        let read =
            |theme: &str, light: &str, dark: &str| Appearance { theme: theme.into(), light: light.into(), dark: dark.into() }.checked();
        assert_eq!(
            read("dark", "catppuccin-latte", "nord"),
            Appearance { theme: "dark".into(), light: "catppuccin-latte".into(), dark: "nord".into() }
        );
        assert_eq!(read("sepia", "../x", ""), Appearance::default());
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
