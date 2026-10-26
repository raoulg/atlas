mod files;
mod lesson;
mod terminal;

use std::{path::PathBuf, sync::Mutex, time::Duration};
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

#[derive(Default)]
struct StudioState(Mutex<Option<Session>>);

#[tauri::command]
async fn choose_workspace(app: tauri::AppHandle, state: tauri::State<'_, StudioState>) -> Result<Option<Workspace>, String> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.dialog().file().pick_folder(move |folder| {
        let _ = sender.send(folder);
    });
    let Some(folder) = receiver.await.map_err(|e| e.to_string())? else {
        return Ok(None);
    };
    open_workspace(&app, &state, folder.into_path().map_err(|e| e.to_string())?).await.map(Some)
}

/// The workspace Studio opens as it starts: the folder named on its command line
/// (`open -a "Atlas Studio" --args ~/code/x`), else the one open when it last closed.
#[tauri::command]
async fn first_workspace(app: tauri::AppHandle, state: tauri::State<'_, StudioState>) -> Result<Option<Workspace>, String> {
    if let Some(current) = state.0.lock().map_err(|e| e.to_string())?.as_ref() {
        return Ok(Some(current.workspace.clone()));
    }
    let named = std::env::args_os().skip(1).map(PathBuf::from).find(|path| path.is_dir());
    let Some(path) = named.or_else(|| last_workspace(&app).filter(|path| path.is_dir())) else {
        return Ok(None);
    };
    open_workspace(&app, &state, path).await.map(Some)
}

/// The file that keeps the workspace open last, in the app's own configuration folder.
fn last_workspace_file(app: &tauri::AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|dir| dir.join("last-workspace"))
}

fn last_workspace(app: &tauri::AppHandle) -> Option<PathBuf> {
    std::fs::read_to_string(last_workspace_file(app)?).ok().map(|path| PathBuf::from(path.trim()))
}

async fn open_workspace(app: &tauri::AppHandle, state: &tauri::State<'_, StudioState>, path: PathBuf) -> Result<Workspace, String> {
    let path = path.canonicalize().map_err(|e| e.to_string())?;
    let started = start_workspace(app, &path).await?;
    let workspace = started.workspace.clone();
    if let Some(previous) = state.0.lock().map_err(|e| e.to_string())?.replace(started) {
        let _ = previous.child.kill();
    }
    if let Some(file) = last_workspace_file(app) {
        let _ = file.parent().map(std::fs::create_dir_all);
        let _ = std::fs::write(file, path.to_string_lossy().as_bytes());
    }
    Ok(workspace)
}

async fn start_workspace(app: &tauri::AppHandle, path: &PathBuf) -> Result<Session, String> {
    let command = app
        .shell()
        .sidecar("atlas")
        .map_err(|e| e.to_string())?
        .args(["serve", &path.to_string_lossy(), "--port", "0", "--no-open"])
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
    tauri::async_runtime::spawn(async move {
        while let Some(event) = events.recv().await {
            if let CommandEvent::Terminated(status) = event {
                let current = monitor.try_state::<StudioState>().is_some_and(|state| {
                    state.0.lock().is_ok_and(|mut session| {
                        if session.as_ref().is_some_and(|session| session.workspace.url == monitor_url) {
                            session.take();
                            true
                        } else {
                            false
                        }
                    })
                });
                if current {
                    let _ = monitor.emit("studio-backend-stopped", status.code);
                }
                break;
            }
        }
    });
    Ok(Session { workspace, child: child.expect("the sidecar is available after startup") })
}

fn workspace_path(state: &tauri::State<'_, StudioState>) -> Result<PathBuf, String> {
    let state = state.0.lock().map_err(|error| error.to_string())?;
    let session = state.as_ref().ok_or("Open a workspace first")?;
    Ok(PathBuf::from(&session.workspace.path))
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
fn list_lessons(state: tauri::State<'_, StudioState>) -> Result<Vec<String>, String> {
    lesson::list(&workspace_path(&state)?).map(|lessons| lessons.into_iter().map(|path| path.display().to_string()).collect())
}

#[tauri::command]
fn open_lesson(state: tauri::State<'_, StudioState>, path: String) -> Result<LessonData, String> {
    let found = lesson::open(&workspace_path(&state)?, PathBuf::from(path).as_path())?;
    let script = found.script.strip_prefix(found.path.parent().ok_or("Lesson has no folder")?).map_err(|error| error.to_string())?;
    Ok(LessonData { text: found.text, script: script.display().to_string() })
}

#[tauri::command]
fn read_lesson_reference(state: tauri::State<'_, StudioState>, path: String, reference: String) -> Result<String, String> {
    lesson::read_reference(&workspace_path(&state)?, PathBuf::from(path).as_path(), &reference)
}

#[tauri::command]
fn read_lesson_asset(state: tauri::State<'_, StudioState>, path: String, reference: String) -> Result<Vec<u8>, String> {
    lesson::read_asset(&workspace_path(&state)?, PathBuf::from(path).as_path(), PathBuf::from(reference).as_path())
}

#[tauri::command]
fn save_lesson(state: tauri::State<'_, StudioState>, path: String, text: String) -> Result<(), String> {
    lesson::save_document(&workspace_path(&state)?, PathBuf::from(path).as_path(), &text)
}

#[tauri::command]
fn current_atlas_view(state: tauri::State<'_, StudioState>) -> Result<String, String> {
    lesson::current_view(&workspace_path(&state)?)
}

#[tauri::command]
fn save_lesson_view(state: tauri::State<'_, StudioState>, path: String, name: String, view: String) -> Result<String, String> {
    lesson::save_view(&workspace_path(&state)?, PathBuf::from(path).as_path(), &name, &view)
}

#[tauri::command]
async fn run_lesson(state: tauri::State<'_, StudioState>, path: String) -> Result<LessonRun, String> {
    let workspace = workspace_path(&state)?;
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
async fn workspace_files(state: tauri::State<'_, StudioState>) -> Result<String, String> {
    let url =
        state.0.lock().map_err(|e| e.to_string())?.as_ref().map(|session| session.workspace.url.clone()).ok_or("Open a workspace first")?;
    tauri::async_runtime::spawn_blocking(move || files::listing(&url)).await.map_err(|error| error.to_string())?
}

#[tauri::command]
fn read_workspace_file(state: tauri::State<'_, StudioState>, path: String) -> Result<tauri::ipc::Response, String> {
    files::read(&workspace_path(&state)?, &path).map(tauri::ipc::Response::new)
}

#[tauri::command]
fn terminal_open(
    state: tauri::State<'_, StudioState>,
    terminals: tauri::State<'_, Terminals>,
    cols: u16,
    rows: u16,
    on_event: Channel<terminal::Event>,
) -> Result<u32, String> {
    terminals.open(&workspace_path(&state)?, cols, rows, move |event| on_event.send(event).is_ok())
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
        .manage(StudioState::default())
        .manage(Terminals::default())
        .invoke_handler(tauri::generate_handler![
            choose_workspace,
            first_workspace,
            list_lessons,
            open_lesson,
            read_lesson_reference,
            read_lesson_asset,
            save_lesson,
            current_atlas_view,
            save_lesson_view,
            run_lesson,
            workspace_files,
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
                    terminals.close_all();
                }
                if let Some(state) = app.try_state::<StudioState>() {
                    if let Ok(mut session) = state.0.lock() {
                        if let Some(session) = session.take() {
                            let _ = session.child.kill();
                        }
                    }
                }
            }
        });
}
