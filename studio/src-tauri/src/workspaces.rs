//! The workspaces Studio has opened, most recent first, kept in the app's configuration folder,
//! and the menu bar that offers them: File › Open Workspace… (the page of them), Open Folder…,
//! Open Recent.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::menu::{AboutMetadata, Menu, MenuItemBuilder, SubmenuBuilder};
use tauri::{AppHandle, Manager, Runtime};

/// How many workspaces Studio keeps.
const MOST: usize = 20;

/// A workspace opened before: where it is, its folder's name, and when it was last opened, in
/// seconds since 1970.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Recent {
    pub path: String,
    pub name: String,
    pub opened: u64,
}

fn file<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|dir| dir.join("workspaces.json"))
}

/// The workspaces opened before, most recent first.
pub fn recent<R: Runtime>(app: &AppHandle<R>) -> Vec<Recent> {
    file(app).and_then(|file| std::fs::read(file).ok()).and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
}

fn keep<R: Runtime>(app: &AppHandle<R>, recent: &[Recent]) -> Result<(), String> {
    let file = file(app).ok_or("Studio has no configuration folder")?;
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    }
    std::fs::write(file, serde_json::to_vec_pretty(recent).map_err(|error| error.to_string())?).map_err(|error| error.to_string())?;
    let _ = app.set_menu(menu(app, recent).map_err(|error| error.to_string())?);
    Ok(())
}

/// `list` with `path` first, opened now; the oldest beyond the most Studio keeps fall off.
pub fn opened(list: &[Recent], path: &Path, now: u64) -> Vec<Recent> {
    let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string());
    let path = path.display().to_string();
    let mut list: Vec<Recent> = list.iter().filter(|recent| recent.path != path).cloned().collect();
    list.insert(0, Recent { path, name, opened: now });
    list.truncate(MOST);
    list
}

pub fn remember<R: Runtime>(app: &AppHandle<R>, path: &Path) -> Result<(), String> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|since| since.as_secs()).unwrap_or_default();
    keep(app, &opened(&recent(app), path, now))
}

pub fn forget<R: Runtime>(app: &AppHandle<R>, path: &str) -> Result<(), String> {
    keep(app, &recent(app).into_iter().filter(|recent| recent.path != path).collect::<Vec<_>>())
}

pub fn clear<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    keep(app, &[])
}

/// What the About panel says beside the logo.
const ABOUT: &str = "31 mathematical principles in action, to visualise your code.\n\nThe logo is Rule 30: wholly deterministic, and still a source of real randomness. Much like working with an agent.";

fn about() -> AboutMetadata<'static> {
    AboutMetadata {
        credits: Some(ABOUT.into()),
        comments: Some(ABOUT.into()),
        website: Some("https://raoulg.github.io/atlas/".into()),
        website_label: Some("raoulg.github.io/atlas".into()),
        ..Default::default()
    }
}

/// The id of the menu item that opens the recent workspace at `index`.
pub const RECENT: &str = "workspace.recent.";

/// The menu bar: the app's own menu on macOS, File with the workspaces, Edit, View with the
/// terminal (a menu's shortcut works wherever the focus is, also inside the Atlas view), Window.
pub fn menu<R: Runtime>(app: &AppHandle<R>, recent: &[Recent]) -> tauri::Result<Menu<R>> {
    let mut opened = SubmenuBuilder::new(app, "Open Recent");
    for (index, workspace) in recent.iter().enumerate() {
        opened = opened
            .item(&MenuItemBuilder::with_id(format!("{RECENT}{index}"), format!("{}  —  {}", workspace.name, workspace.path)).build(app)?);
    }
    if !recent.is_empty() {
        opened = opened.separator();
    }
    let clear = MenuItemBuilder::with_id("workspace.clear", "Clear Recent").enabled(!recent.is_empty()).build(app)?;
    let file = SubmenuBuilder::new(app, "File")
        .item(&MenuItemBuilder::with_id("workspace.page", "Open Workspace…").accelerator("CmdOrCtrl+O").build(app)?)
        .item(&MenuItemBuilder::with_id("workspace.folder", "Open Folder…").accelerator("CmdOrCtrl+Shift+O").build(app)?)
        .item(&opened.item(&clear).build()?)
        .separator()
        .close_window();
    #[cfg(not(target_os = "macos"))]
    let file = file.separator().quit();
    let edit = SubmenuBuilder::new(app, "Edit").undo().redo().separator().cut().copy().paste().select_all().build()?;
    let view = SubmenuBuilder::new(app, "View")
        .item(&MenuItemBuilder::with_id("view.atlas", "Atlas View").accelerator("CmdOrCtrl+1").build(app)?)
        .item(&MenuItemBuilder::with_id("view.history", "History").accelerator("CmdOrCtrl+2").build(app)?)
        .separator()
        .item(&MenuItemBuilder::with_id("view.terminal", "Terminal").accelerator("Ctrl+`").build(app)?);
    #[cfg(target_os = "macos")]
    let view = view.separator().fullscreen();
    let window = SubmenuBuilder::new(app, "Window").minimize().maximize().separator().close_window().build()?;
    let bar = Menu::new(app)?;
    #[cfg(target_os = "macos")]
    {
        let name = app.package_info().name.clone();
        let own = SubmenuBuilder::new(app, name)
            .about(Some(about()))
            .item(&MenuItemBuilder::with_id("update.check", "Check for Updates…").build(app)?)
            .separator()
            .services()
            .separator()
            .hide()
            .hide_others()
            .show_all()
            .separator()
            .quit()
            .build()?;
        bar.append(&own)?;
    }
    bar.append(&file.build()?)?;
    bar.append(&edit)?;
    bar.append(&view.build()?)?;
    bar.append(&window)?;
    #[cfg(not(target_os = "macos"))]
    bar.append(
        &SubmenuBuilder::new(app, "Help").item(&MenuItemBuilder::with_id("update.check", "Check for Updates…").build(app)?).build()?,
    )?;
    Ok(bar)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(path: &str, opened: u64) -> Recent {
        Recent { path: path.into(), name: Path::new(path).file_name().unwrap().to_string_lossy().into(), opened }
    }

    #[test]
    fn a_workspace_opened_again_moves_to_the_top_once() {
        let list = vec![at("/code/a", 1), at("/code/b", 2)];
        let now = opened(&list, Path::new("/code/b"), 9);
        assert_eq!(now, vec![at("/code/b", 9), at("/code/a", 1)]);
    }

    #[test]
    fn the_oldest_falls_off() {
        let list: Vec<Recent> = (0..MOST).map(|n| at(&format!("/code/{n}"), n as u64)).collect();
        let now = opened(&list, Path::new("/code/new"), 99);
        assert_eq!(now.len(), MOST);
        assert_eq!(now[0].path, "/code/new");
        assert_eq!(now.last().unwrap().path, format!("/code/{}", MOST - 2));
    }
}
