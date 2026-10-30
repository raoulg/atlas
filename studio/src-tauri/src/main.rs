mod files;
mod lesson;
mod settings;
mod terminal;
mod update;
mod workspace;
mod workspaces;

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicU32, Ordering},
        Mutex,
    },
    time::Duration,
};
use tauri::{ipc::Channel, Emitter, Manager};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_shell::{
    process::{CommandChild, CommandEvent},
    ShellExt,
};
use terminal::Terminals;

#[derive(Clone, serde::Serialize)]
struct Workspace {
    path: String,
    name: String,
    url: String,
}

struct Session {
    workspace: Workspace,
    child: CommandChild,
}

/// The workspace open in each window, by the window's label: every window is one workspace, with its
/// own `atlas-code serve`.
#[derive(Default)]
struct StudioState(Mutex<HashMap<String, Session>>);

#[tauri::command]
async fn choose_workspace(
    window: tauri::Window,
    app: tauri::AppHandle,
    state: tauri::State<'_, StudioState>,
) -> Result<Option<Workspace>, String> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.dialog().file().pick_folder(move |folder| {
        let _ = sender.send(folder);
    });
    let Some(folder) = receiver.await.map_err(|e| e.to_string())? else {
        return Ok(None);
    };
    open_workspace(&app, &state, window.label(), folder.into_path().map_err(|e| e.to_string())?).await
}

/// The workspace a window opens on: the one it has; for the window Studio starts with, the folder named
/// on its command line (`open -a "Atlas Studio" --args ~/code/x`), else the one opened last; a new
/// window starts on the page of workspaces.
#[tauri::command]
async fn first_workspace(
    window: tauri::Window,
    app: tauri::AppHandle,
    state: tauri::State<'_, StudioState>,
) -> Result<Option<Workspace>, String> {
    if let Some(current) = state.0.lock().map_err(|e| e.to_string())?.get(window.label()) {
        return Ok(Some(current.workspace.clone()));
    }
    if window.label() != "main" {
        return Ok(None);
    }
    let named = std::env::args_os().skip(1).map(PathBuf::from).find(|path| workspace::Workspace::is(path));
    let last = workspaces::recent(&app).into_iter().map(|recent| PathBuf::from(recent.path)).find(|path| workspace::Workspace::is(path));
    let Some(path) = named.or(last) else {
        return Ok(None);
    };
    open_workspace(&app, &state, window.label(), path).await
}

#[tauri::command]
fn recent_workspaces(app: tauri::AppHandle) -> Vec<workspaces::Recent> {
    workspaces::recent(&app)
}

#[tauri::command]
async fn open_workspace_at(
    window: tauri::Window,
    app: tauri::AppHandle,
    state: tauri::State<'_, StudioState>,
    path: String,
) -> Result<Option<Workspace>, String> {
    open_workspace(&app, &state, window.label(), PathBuf::from(path)).await
}

/// The workspaces open in the other windows, by their paths.
#[tauri::command]
fn open_elsewhere(window: tauri::Window, state: tauri::State<'_, StudioState>) -> Result<Vec<String>, String> {
    let sessions = state.0.lock().map_err(|e| e.to_string())?;
    Ok(sessions.iter().filter(|(label, _)| *label != window.label()).map(|(_, session)| session.workspace.path.clone()).collect())
}

#[tauri::command]
fn forget_workspace(app: tauri::AppHandle, path: String) -> Result<(), String> {
    workspaces::forget(&app, &path)
}

/// Open `path` in the window `label`, in place of what it had open. A workspace open in another window
/// is that window's: it comes to the front, and nothing is opened here (two servers on one repository
/// would share what it keeps in `.atlas/`).
async fn open_workspace(
    app: &tauri::AppHandle,
    state: &tauri::State<'_, StudioState>,
    label: &str,
    path: PathBuf,
) -> Result<Option<Workspace>, String> {
    let path = workspace::Workspace::open(&path)?.root().to_path_buf();
    let shown = path.display().to_string();
    let elsewhere =
        state.0.lock().map_err(|e| e.to_string())?.iter().find(|(l, s)| *l != label && s.workspace.path == shown).map(|(l, _)| l.clone());
    if let Some(window) = elsewhere.and_then(|other| app.get_webview_window(&other)) {
        let _ = window.unminimize();
        let _ = window.set_focus();
        return Ok(None);
    }
    let started = start_workspace(app, &path, label).await?;
    let workspace = started.workspace.clone();
    if let Some(previous) = state.0.lock().map_err(|e| e.to_string())?.insert(label.to_string(), started) {
        let _ = previous.child.kill();
    }
    if let Some(window) = app.get_webview_window(label) {
        let _ = window.set_title(&format!("{} — {}", workspace.name, app.package_info().name));
    }
    let _ = workspaces::remember(app, &path);
    Ok(Some(workspace))
}

/// Start `atlas-code serve` on the workspace. Its standard input is a pipe Studio holds and never writes:
/// the system closes it when Studio ends, by Quit or by a signal, and the server ends with it.
async fn start_workspace(app: &tauri::AppHandle, path: &PathBuf, label: &str) -> Result<Session, String> {
    let command = app
        .shell()
        .sidecar("atlas-code")
        .map_err(|e| e.to_string())?
        .args(["serve", &path.to_string_lossy(), "--port", "0", "--no-open", "--exit-on-stdin-close"])
        .env("NO_COLOR", "1")
        .env("ATLAS_NO_UPDATE_CHECK", "1");
    let (mut events, child) = command.spawn().map_err(|e| format!("Atlas kon niet starten: {e}"))?;
    let mut child = Some(child);
    let started = tokio::time::timeout(Duration::from_secs(20), async {
        let mut last_error = String::new();
        while let Some(event) = events.recv().await {
            match event {
                CommandEvent::Stderr(bytes) => {
                    let line = String::from_utf8_lossy(&bytes);
                    if let Some(url) = line.split_whitespace().find(|part| part.starts_with("http://127.0.0.1:")) {
                        return Ok(url.trim_end_matches('/').to_string());
                    }
                    last_error = line.trim().to_string();
                }
                CommandEvent::Error(error) => return Err(error),
                CommandEvent::Terminated(status) => {
                    return Err(format!("Atlas stopte bij het openen van deze map ({:?}): {last_error}", status.code))
                }
                _ => {}
            }
        }
        Err(format!("Atlas kon deze map niet openen: {last_error}"))
    })
    .await;
    let url = match started {
        Ok(Ok(url)) => url,
        Ok(Err(error)) => {
            if let Some(child) = child.take() {
                let _ = child.kill();
            }
            return Err(error);
        }
        Err(_) => {
            if let Some(child) = child.take() {
                let _ = child.kill();
            }
            return Err("Atlas gaf geen serveradres terug.".into());
        }
    };
    let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string());
    let workspace = Workspace { path: path.display().to_string(), name, url };
    let monitor = app.clone();
    let monitor_url = workspace.url.clone();
    let monitor_label = label.to_string();
    tauri::async_runtime::spawn(async move {
        while let Some(event) = events.recv().await {
            if let CommandEvent::Terminated(status) = event {
                let current = monitor.try_state::<StudioState>().is_some_and(|state| {
                    state.0.lock().is_ok_and(|mut sessions| {
                        if sessions.get(&monitor_label).is_some_and(|session| session.workspace.url == monitor_url) {
                            sessions.remove(&monitor_label);
                            true
                        } else {
                            false
                        }
                    })
                });
                if current {
                    let _ = monitor.emit_to(monitor_label.as_str(), "studio-backend-stopped", status.code);
                }
                break;
            }
        }
    });
    Ok(Session { workspace, child: child.expect("the sidecar is available after startup") })
}

/// The workspace open in the window `label`.
fn workspace_path(state: &tauri::State<'_, StudioState>, label: &str) -> Result<PathBuf, String> {
    let state = state.0.lock().map_err(|error| error.to_string())?;
    let session = state.get(label).ok_or("Open a workspace first")?;
    Ok(PathBuf::from(&session.workspace.path))
}

/// Where the window `label`'s `atlas-code serve` answers.
fn workspace_url(state: &tauri::State<'_, StudioState>, label: &str) -> Result<String, String> {
    let state = state.0.lock().map_err(|error| error.to_string())?;
    Ok(state.get(label).ok_or("Open a workspace first")?.workspace.url.clone())
}

#[derive(serde::Serialize)]
struct LessonData {
    text: String,
    script: String,
}

#[derive(serde::Serialize)]
struct LessonRun {
    stdout: String,
    stderr: String,
    code: Option<i32>,
    artifacts: Vec<String>,
}

#[tauri::command]
fn list_lessons(window: tauri::Window, state: tauri::State<'_, StudioState>) -> Result<Vec<String>, String> {
    lesson::list(&workspace_path(&state, window.label())?)
        .map(|lessons| lessons.into_iter().map(|path| path.display().to_string()).collect())
}

#[tauri::command]
fn open_lesson(window: tauri::Window, state: tauri::State<'_, StudioState>, path: String) -> Result<LessonData, String> {
    let found = lesson::open(&workspace_path(&state, window.label())?, PathBuf::from(path).as_path())?;
    let script = found.script.strip_prefix(found.path.parent().ok_or("Lesson has no folder")?).map_err(|error| error.to_string())?;
    Ok(LessonData { text: found.text, script: script.display().to_string() })
}

#[tauri::command]
fn read_lesson_reference(
    window: tauri::Window,
    state: tauri::State<'_, StudioState>,
    path: String,
    reference: String,
) -> Result<String, String> {
    lesson::read_reference(&workspace_path(&state, window.label())?, PathBuf::from(path).as_path(), &reference)
}

#[tauri::command]
fn read_lesson_asset(
    window: tauri::Window,
    state: tauri::State<'_, StudioState>,
    path: String,
    reference: String,
) -> Result<Vec<u8>, String> {
    lesson::read_asset(&workspace_path(&state, window.label())?, PathBuf::from(path).as_path(), PathBuf::from(reference).as_path())
}

#[tauri::command]
fn save_lesson(window: tauri::Window, state: tauri::State<'_, StudioState>, path: String, text: String) -> Result<(), String> {
    lesson::save_document(&workspace_path(&state, window.label())?, PathBuf::from(path).as_path(), &text)
}

#[tauri::command]
fn current_atlas_view(window: tauri::Window, state: tauri::State<'_, StudioState>) -> Result<String, String> {
    lesson::current_view(&workspace_path(&state, window.label())?)
}

#[tauri::command]
fn save_lesson_view(
    window: tauri::Window,
    state: tauri::State<'_, StudioState>,
    path: String,
    name: String,
    view: String,
) -> Result<String, String> {
    lesson::save_view(&workspace_path(&state, window.label())?, PathBuf::from(path).as_path(), &name, &view)
}

#[tauri::command]
async fn run_lesson(window: tauri::Window, state: tauri::State<'_, StudioState>, path: String) -> Result<LessonRun, String> {
    let workspace = workspace_path(&state, window.label())?;
    let result = tauri::async_runtime::spawn_blocking(move || lesson::run_python(&workspace, PathBuf::from(path).as_path()))
        .await
        .map_err(|error| error.to_string())??;
    Ok(LessonRun {
        stdout: result.stdout,
        stderr: result.stderr,
        code: result.code,
        artifacts: result.artifacts.into_iter().map(|path| path.display().to_string()).collect(),
    })
}

#[tauri::command]
async fn workspace_files(window: tauri::Window, state: tauri::State<'_, StudioState>) -> Result<String, String> {
    let url = workspace_url(&state, window.label())?;
    tauri::async_runtime::spawn_blocking(move || files::listing(&url)).await.map_err(|error| error.to_string())?
}

/// The Atlas view as one HTML file, opened in `view`, saved where the user chooses; the name it was
/// saved as, or nothing when the user cancelled.
#[tauri::command]
async fn export_view(
    window: tauri::Window,
    app: tauri::AppHandle,
    state: tauri::State<'_, StudioState>,
    view: String,
) -> Result<Option<String>, String> {
    let url = workspace_url(&state, window.label())?;
    let path = format!("/export?v={}", files::query(&view));
    let (html, name) = tauri::async_runtime::spawn_blocking(move || files::get(&url, &path)).await.map_err(|error| error.to_string())??;
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.dialog().file().set_file_name(name.unwrap_or_else(|| "atlas.html".into())).add_filter("HTML", &["html"]).save_file(move |file| {
        let _ = sender.send(file);
    });
    let Some(file) = receiver.await.map_err(|error| error.to_string())? else {
        return Ok(None);
    };
    let file = file.into_path().map_err(|error| error.to_string())?;
    std::fs::write(&file, html).map_err(|error| format!("Cannot write {}: {error}", file.display()))?;
    Ok(Some(file.display().to_string()))
}

#[tauri::command]
fn read_workspace_file(window: tauri::Window, state: tauri::State<'_, StudioState>, path: String) -> Result<tauri::ipc::Response, String> {
    files::read(&workspace_path(&state, window.label())?, &path).map(tauri::ipc::Response::new)
}

#[tauri::command]
fn terminal_open(
    window: tauri::Window,
    state: tauri::State<'_, StudioState>,
    terminals: tauri::State<'_, Terminals>,
    cols: u16,
    rows: u16,
    on_event: Channel<terminal::Event>,
) -> Result<u32, String> {
    terminals.open(window.label(), &workspace_path(&state, window.label())?, cols, rows, move |event| on_event.send(event).is_ok())
}

#[tauri::command]
fn terminal_write(terminals: tauri::State<'_, Terminals>, id: u32, data: String) -> Result<(), String> {
    terminals.write(id, &data)
}

#[tauri::command]
fn terminal_resize(terminals: tauri::State<'_, Terminals>, id: u32, cols: u16, rows: u16) -> Result<(), String> {
    terminals.resize(id, cols, rows)
}

#[tauri::command]
fn terminal_close(terminals: tauri::State<'_, Terminals>, id: u32) -> Result<(), String> {
    terminals.close(id)
}

/// Which build Studio is, as its sidecar says: the same commit and channel.
async fn this_build(app: &tauri::AppHandle) -> Result<update::Build, String> {
    let out =
        app.shell().sidecar("atlas-code").map_err(|e| e.to_string())?.args(["--version"]).output().await.map_err(|e| e.to_string())?;
    update::build_of(&String::from_utf8_lossy(&out.stdout)).ok_or_else(|| "Studio's atlas does not say which build it is".into())
}

/// Whether a newer build of Studio's channel is out: from the answer kept, asked of GitHub again when
/// it is a day old, another build asked it, or `now` asks for it.
async fn newer_build(app: &tauri::AppHandle, now: bool) -> Result<Option<update::Newer>, String> {
    let build = this_build(app).await?;
    let cache = update::Cache::of(app).ok_or("Studio has no cache folder")?;
    let mut asked = cache.asked();
    if now || asked.stale(&build, update::now()) {
        let asking = build.clone();
        let latest = tauri::async_runtime::spawn_blocking(move || update::ask(&asking)).await.map_err(|e| e.to_string())?;
        if let Some(latest) = latest {
            asked = update::Asked { checked: update::now(), by: update::key(&build), latest };
            cache.keep(&asked);
        } else if now {
            return Err("GitHub did not answer; try again later".into());
        }
    }
    let newer = update::newer(&build, &asked.latest);
    tauri::async_runtime::spawn_blocking(move || newer.map(update::shown)).await.map_err(|e| e.to_string())
}

/// Studio's settings, for the window that asks: its appearance, where its terminal and its preview dock, and the coding agents
/// with Atlas or without.
#[tauri::command]
fn get_settings(window: tauri::Window, app: tauri::AppHandle, state: tauri::State<'_, StudioState>) -> serde_json::Value {
    let workspace = workspace_path(&state, window.label()).ok();
    serde_json::json!({
        "appearance": settings::appearance(&app),
        "terminal": settings::terminal_dock(&app),
        "preview": settings::preview_side(&app),
        "agents": settings::agents(workspace.as_deref()),
    })
}

#[tauri::command]
fn set_terminal_dock(app: tauri::AppHandle, dock: settings::Dock) -> Result<(), String> {
    settings::set_terminal_dock(&app, dock)
}

#[tauri::command]
fn set_preview_dock(app: tauri::AppHandle, side: settings::Side) -> Result<(), String> {
    settings::set_preview_side(&app, side)
}

#[tauri::command]
fn set_appearance(app: tauri::AppHandle, appearance: settings::Appearance) -> Result<(), String> {
    settings::set_appearance(&app, appearance)
}

#[tauri::command]
async fn register_agent(id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || settings::register(&id)).await.map_err(|e| e.to_string())?
}

/// Which build Studio is, for the status bar.
#[tauri::command]
async fn studio_build(app: tauri::AppHandle) -> Result<update::Build, String> {
    this_build(&app).await
}

/// What changed in the build that runs, as the window shows it.
#[tauri::command]
async fn whats_new(app: tauri::AppHandle) -> Result<update::Newer, String> {
    let build = this_build(&app).await?;
    tauri::async_runtime::spawn_blocking(move || update::changes(&build)).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn check_update(app: tauri::AppHandle, now: bool) -> Result<Option<update::Newer>, String> {
    newer_build(&app, now).await
}

/// Install the newer build, then start Studio on it: again here on macOS, by the setup program on
/// Windows, which Studio closes for.
#[tauri::command]
async fn install_update(app: tauri::AppHandle) -> Result<(), String> {
    let build = this_build(&app).await?;
    tauri::async_runtime::spawn_blocking(move || update::install(&build)).await.map_err(|e| e.to_string())??;
    if cfg!(target_os = "windows") {
        app.exit(0);
    } else {
        app.restart();
    }
    Ok(())
}

/// Once an hour Studio looks whether its answer is a day old, and says so when a newer build is out:
/// a day after it was last asked, whether Studio ran in between or not.
fn watch_updates(app: tauri::AppHandle) {
    if std::env::var_os("ATLAS_NO_UPDATE_CHECK").is_some() {
        return;
    }
    tauri::async_runtime::spawn(async move {
        loop {
            if let Ok(Some(newer)) = newer_build(&app, false).await {
                let _ = app.emit("studio-update", newer);
            }
            tokio::time::sleep(Duration::from_secs(60 * 60)).await;
        }
    });
}

/// A new window, for a workspace of its own; on macOS in the same group of tabs as the others.
fn new_window(app: &tauri::AppHandle) -> tauri::Result<()> {
    static NEXT: AtomicU32 = AtomicU32::new(1);
    let label = format!("workspace-{}", NEXT.fetch_add(1, Ordering::Relaxed));
    let builder = tauri::WebviewWindowBuilder::new(app, label, tauri::WebviewUrl::App("index.html".into()))
        .title(&app.package_info().name)
        .inner_size(1320.0, 820.0)
        .min_inner_size(850.0, 560.0);
    #[cfg(target_os = "macos")]
    let builder = builder.tabbing_identifier("atlas-studio");
    builder.build().map(|_| ())
}

/// The window the user works in, for a menu item to act on.
fn focused(app: &tauri::AppHandle) -> Option<String> {
    let windows = app.webview_windows();
    windows
        .iter()
        .find(|(_, window)| window.is_focused().unwrap_or(false))
        .or_else(|| windows.iter().next())
        .map(|(label, _)| label.clone())
}

/// End what the window `label` had open: its server and its shells.
fn close_window(app: &tauri::AppHandle, label: &str) {
    if let Some(terminals) = app.try_state::<Terminals>() {
        terminals.close_all(Some(label));
    }
    if let Some(state) = app.try_state::<StudioState>() {
        if let Some(session) = state.0.lock().ok().and_then(|mut sessions| sessions.remove(label)) {
            let _ = session.child.kill();
        }
    }
}

/// An app started from the Finder or the Dock gets the system's bare `PATH`, without what the
/// user's shell profile adds (Homebrew, `uv`, `cargo`). The sidecar, the lessons' Python and the
/// terminals take the `PATH` the user's login shell has, as a terminal application does.
#[cfg(unix)]
fn take_login_path() {
    use std::process::{Command, Stdio};
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let mark = "__ATLAS_STUDIO_PATH__";
    let Ok(child) = Command::new(&shell)
        .args(["-ilc", &format!("printf '{mark}%s{mark}' \"$PATH\"")])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return;
    };
    let id = child.id();
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(child.wait_with_output());
    });
    let Ok(Ok(output)) = receiver.recv_timeout(Duration::from_secs(5)) else {
        let _ = Command::new("kill").arg(id.to_string()).status();
        return;
    };
    let text = String::from_utf8_lossy(&output.stdout);
    if let Some(path) = text.split(mark).nth(1).filter(|path| !path.is_empty()) {
        std::env::set_var("PATH", path);
    }
}

fn main() {
    #[cfg(unix)]
    take_login_path();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .manage(StudioState::default())
        .manage(Terminals::default())
        .register_uri_scheme_protocol("preview", |context, request| {
            // the files of the workspace of the window that asks
            let state = context.app_handle().state::<StudioState>();
            let workspace = state
                .0
                .lock()
                .ok()
                .and_then(|sessions| sessions.get(context.webview_label()).map(|session| PathBuf::from(&session.workspace.path)));
            files::preview(workspace.as_deref(), request.uri().path())
        })
        .menu(|app| workspaces::menu(app, &workspaces::recent(app)))
        .setup(|app| {
            settings::apply(app.handle(), &settings::appearance(app.handle()).theme);
            watch_updates(app.handle().clone());
            Ok(())
        })
        .on_menu_event(|app, event| {
            // what a window does for a menu item, said to the window in front by name; Clear Recent
            // and New Window are the app's
            let id = event.id().as_ref();
            let to = focused(app).unwrap_or_else(|| "main".into());
            if id == "workspace.clear" {
                let _ = workspaces::clear(app);
                let _ = app.emit("studio-menu", serde_json::json!({"action": "recents"}));
            } else if id == "window.new" {
                let _ = new_window(app);
            } else if let Some(index) = id.strip_prefix(workspaces::RECENT).and_then(|index| index.parse::<usize>().ok()) {
                if let Some(recent) = workspaces::recent(app).get(index) {
                    let _ = app.emit_to(to.as_str(), "studio-menu", serde_json::json!({"action": "open", "path": recent.path}));
                }
            } else {
                let _ = app.emit_to(to.as_str(), "studio-menu", serde_json::json!({"action": id}));
            }
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                close_window(window.app_handle(), window.label());
            }
        })
        .invoke_handler(tauri::generate_handler![
            choose_workspace,
            first_workspace,
            recent_workspaces,
            open_workspace_at,
            forget_workspace,
            open_elsewhere,
            list_lessons,
            open_lesson,
            read_lesson_reference,
            read_lesson_asset,
            save_lesson,
            current_atlas_view,
            save_lesson_view,
            run_lesson,
            workspace_files,
            export_view,
            check_update,
            whats_new,
            studio_build,
            get_settings,
            set_appearance,
            set_terminal_dock,
            set_preview_dock,
            register_agent,
            install_update,
            read_workspace_file,
            terminal_open,
            terminal_write,
            terminal_resize,
            terminal_close
        ])
        .build(tauri::generate_context!())
        .expect("Atlas Studio could not start")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                if let Some(terminals) = app.try_state::<Terminals>() {
                    terminals.close_all(None);
                }
                if let Some(state) = app.try_state::<StudioState>() {
                    if let Ok(mut sessions) = state.0.lock() {
                        for (_, session) in sessions.drain() {
                            let _ = session.child.kill();
                        }
                    }
                }
            }
        });
}
