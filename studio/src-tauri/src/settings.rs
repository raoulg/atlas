//! Studio's settings: its theme and where its terminal and its preview dock, kept in its configuration folder and set on
//! every window, and
//! whether the coding agents on this machine have Atlas's MCP server, registered for every project as
//! `docs/reference/install.md` says (`claude mcp add --scope user atlas -- atlas-code mcp`, and Codex's
//! equivalent). Registering runs that same command; connecting to it again is the agent's own.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use tauri::{AppHandle, Emitter, Manager, Runtime, Theme};

/// Studio's configuration folder: the user's choices, `settings.json`, and the workspaces opened,
/// `workspaces.json`.
pub struct Settings {
    dir: PathBuf,
}

impl Settings {
    /// The configuration folder of the app that runs; none where the platform has none.
    pub fn of<R: Runtime>(app: &AppHandle<R>) -> Option<Settings> {
        app.path().app_config_dir().ok().map(|dir| Settings { dir })
    }

    /// One file kept there.
    pub fn file(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    /// What one of its files keeps, as it was kept.
    pub fn read(&self, name: &str) -> Option<Vec<u8>> {
        std::fs::read(self.file(name)).ok()
    }

    /// Keep `bytes` in one of its files, in place of what it kept.
    pub fn keep(&self, name: &str, bytes: &[u8]) -> Result<(), String> {
        std::fs::create_dir_all(&self.dir).map_err(|error| error.to_string())?;
        std::fs::write(self.file(name), bytes).map_err(|error| error.to_string())
    }

    /// A coding agent's own settings file, as it is: Claude Code's `~/.claude.json`, a project's
    /// `.mcp.json`, Codex's `config.toml`.
    pub fn agent(config: &Path) -> Option<String> {
        std::fs::read_to_string(config).ok()
    }
}

/// The file Studio keeps the user's choices in.
const FILE: &str = "settings.json";

/// The settings kept: one object, each setting a field of it.
fn read<R: Runtime>(app: &AppHandle<R>) -> serde_json::Value {
    Settings::of(app)
        .and_then(|settings| settings.read(FILE))
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .unwrap_or_default()
}

/// The settings kept with `fields` set and every other field as it was.
fn merged(kept: serde_json::Value, fields: serde_json::Value) -> serde_json::Value {
    let mut object = match kept {
        serde_json::Value::Object(object) => object,
        _ => serde_json::Map::new(),
    };
    if let serde_json::Value::Object(fields) = fields {
        object.extend(fields);
    }
    serde_json::Value::Object(object)
}

/// Keep `fields` in the settings, beside the settings they do not name.
fn keep<R: Runtime>(app: &AppHandle<R>, fields: serde_json::Value) -> Result<(), String> {
    let settings = Settings::of(app).ok_or("Studio has no configuration folder")?;
    let kept = merged(read(app), fields);
    settings.keep(FILE, serde_json::to_string(&kept).map_err(|error| error.to_string())?.as_bytes())
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
    let read = read(app);
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
    keep(app, serde_json::to_value(&chosen).map_err(|error| error.to_string())?)
}

/// Where the terminal stands: under the main view, or beside it on the left or the right.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Dock {
    #[default]
    Bottom,
    Left,
    Right,
}

/// The dock chosen; one Studio does not know is the bottom.
pub fn terminal_dock<R: Runtime>(app: &AppHandle<R>) -> Dock {
    dock_of(&read(app))
}

fn dock_of(kept: &serde_json::Value) -> Dock {
    chosen(kept, "terminal")
}

/// A choice kept under `key`; one Studio does not know is its default.
fn chosen<T: serde::de::DeserializeOwned + Default>(kept: &serde_json::Value, key: &str) -> T {
    serde_json::from_value(kept[key].clone()).unwrap_or_default()
}

/// Dock the terminal of every window, and keep it.
pub fn set_terminal_dock<R: Runtime>(app: &AppHandle<R>, dock: Dock) -> Result<(), String> {
    let _ = app.emit("terminal-dock", dock);
    keep(app, serde_json::json!({ "terminal": dock }))
}

/// Where the preview stands: beside the main view, on the left or the right.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Left,
    #[default]
    Right,
}

/// The side chosen; one Studio does not know is the right.
pub fn preview_side<R: Runtime>(app: &AppHandle<R>) -> Side {
    chosen(&read(app), "preview")
}

/// Dock the preview of every window, and keep it.
pub fn set_preview_side<R: Runtime>(app: &AppHandle<R>, side: Side) -> Result<(), String> {
    let _ = app.emit("preview-dock", side);
    keep(app, serde_json::json!({ "preview": side }))
}

/// The mode on every window: the system's, or light or dark.
pub fn apply<R: Runtime>(app: &AppHandle<R>, theme: &str) {
    app.set_theme(match theme {
        "light" => Some(Theme::Light),
        "dark" => Some(Theme::Dark),
        _ => None,
    });
}

/// The command the CLI of a channel is installed as: the nightly's `atlas-code-nightly`, beside the release's `atlas-code`.
fn command_of(channel: &str) -> &'static str {
    if channel == "nightly" {
        "atlas-code-nightly"
    } else {
        "atlas-code"
    }
}

/// The `atlas-code` an agent starts: the CLI of Studio's own channel on the `PATH`, else Studio's own, so that
/// the agent's `atlas-code mcp` is of the channel of the `atlas-code serve` it shares a page with.
fn atlas() -> Option<PathBuf> {
    which::which(command_of(env!("ATLAS_CHANNEL"))).ok().or_else(|| {
        let own = std::env::current_exe().ok()?.with_file_name(if cfg!(windows) { "atlas-code.exe" } else { "atlas-code" });
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
    Settings::agent(config)
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .is_some_and(|value| value["mcpServers"].get("atlas").is_some())
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from)
}

/// The agents Studio knows, for the workspace at `workspace`.
pub fn agents(workspace: Option<&Path>) -> Vec<Agent> {
    let atlas = atlas().map(|path| path.display().to_string()).unwrap_or_else(|| "atlas-code".into());
    let home = home();
    let claude = home.as_ref().is_some_and(|home| names_atlas(&home.join(".claude.json")))
        || workspace.is_some_and(|w| names_atlas(&w.join(".mcp.json")));
    let codex = home
        .as_ref()
        .and_then(|home| Settings::agent(&home.join(".codex").join("config.toml")))
        .is_some_and(|text| text.lines().any(|line| line.trim() == "[mcp_servers.atlas]"));
    vec![
        Agent {
            id: "claude",
            name: "Claude Code",
            installed: which::which("claude").is_ok(),
            registered: claude,
            command: format!("claude mcp add --scope user atlas -- {} mcp", quoted(&atlas)),
            reconnect: "Start a new Claude Code session, or run /mcp in the one you have.",
        },
        Agent {
            id: "codex",
            name: "Codex",
            installed: which::which("codex").is_ok(),
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
    let atlas = atlas().ok_or("No atlas-code to register: install the atlas-code CLI, or run Studio from its app")?;
    let (program, args): (&str, Vec<String>) = match id {
        "claude" => ("claude", vec!["mcp".into(), "add".into(), "--scope".into(), "user".into(), "atlas".into(), "--".into()]),
        "codex" => ("codex", vec!["mcp".into(), "add".into(), "atlas".into(), "--".into()]),
        _ => return Err(format!("Studio does not know the agent {id}")),
    };
    let program = which::which(program).map_err(|_| format!("{program} is not on the PATH"))?;
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
        std::fs::write(&config, r#"{"mcpServers": {"atlas": {"command": "atlas-code", "args": ["mcp"]}}}"#).unwrap();
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

    /// Each setting is kept beside the others: setting one leaves every other field as it was, and a file
    /// that holds no object is read as none.
    #[test]
    fn a_setting_is_kept_beside_the_others() {
        let appearance =
            serde_json::to_value(Appearance { theme: "dark".into(), light: "github-light".into(), dark: "nord".into() }).unwrap();
        let docked = serde_json::json!({ "terminal": Dock::Right });
        let both = merged(merged(serde_json::Value::Null, appearance.clone()), docked.clone());
        assert_eq!(both, merged(merged(serde_json::json!([1]), docked), appearance.clone()));
        assert_eq!(dock_of(&both), Dock::Right);
        assert_eq!(merged(both, appearance)["dark"], "nord");
    }

    /// A dock read from the file is one Studio can show: the bottom where it names none or one it does not know.
    #[test]
    fn a_dock_studio_does_not_know_is_the_bottom() {
        for (kept, dock) in [("{}", Dock::Bottom), (r#"{"terminal":"left"}"#, Dock::Left), (r#"{"terminal":"top"}"#, Dock::Bottom)] {
            assert_eq!(dock_of(&serde_json::from_str(kept).unwrap()), dock, "{kept}");
        }
    }

    /// A preview's side read from the file is one Studio can show: the right where it names none or one it does not know,
    /// and the terminal's dock is no side of it.
    #[test]
    fn a_side_studio_does_not_know_is_the_right() {
        for (kept, side) in [
            ("{}", Side::Right),
            (r#"{"preview":"left"}"#, Side::Left),
            (r#"{"preview":"bottom"}"#, Side::Right),
            (r#"{"terminal":"left"}"#, Side::Right),
        ] {
            assert_eq!(chosen::<Side>(&serde_json::from_str(kept).unwrap(), "preview"), side, "{kept}");
        }
    }

    /// The release's and the nightly's CLI stand side by side on one `PATH`: each Studio looks for its own.
    #[test]
    fn each_channel_looks_for_its_own_cli() {
        assert_eq!(command_of(""), "atlas-code");
        assert_ne!(command_of("nightly"), command_of(""));
    }

    #[test]
    fn a_path_with_a_space_is_one_word() {
        assert_eq!(
            quoted("/Applications/Atlas Studio.app/Contents/MacOS/atlas-code"),
            "\"/Applications/Atlas Studio.app/Contents/MacOS/atlas-code\""
        );
        assert_eq!(quoted("/opt/homebrew/bin/atlas-code"), "/opt/homebrew/bin/atlas-code");
    }
}
