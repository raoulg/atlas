mod lesson;

use std::{path::PathBuf, sync::Mutex, time::Duration};
use tauri::{Emitter, Manager};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_shell::{
    process::{CommandChild, CommandEvent},
    ShellExt,
};

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
fn current_workspace(state: tauri::State<'_, StudioState>) -> Result<Option<Workspace>, String> {
    Ok(state.0.lock().map_err(|e| e.to_string())?.as_ref().map(|session| session.workspace.clone()))
}

#[tauri::command]
async fn choose_workspace(app: tauri::AppHandle, state: tauri::State<'_, StudioState>) -> Result<Option<Workspace>, String> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.dialog().file().pick_folder(move |folder| {
        let _ = sender.send(folder);
    });
    let Some(folder) = receiver.await.map_err(|e| e.to_string())? else {
        return Ok(None);
    };
    let path = folder.into_path().map_err(|e| e.to_string())?.canonicalize().map_err(|e| e.to_string())?;
    let workspace = start_workspace(&app, &path).await?;
    let mut session = state.0.lock().map_err(|e| e.to_string())?;
    if let Some(previous) = session.take() {
        let _ = previous.child.kill();
    }
    *session = Some(workspace);
    Ok(session.as_ref().map(|session| session.workspace.clone()))
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

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .manage(StudioState::default())
        .invoke_handler(tauri::generate_handler![
            current_workspace,
            choose_workspace,
            list_lessons,
            open_lesson,
            read_lesson_reference,
            read_lesson_asset,
            save_lesson,
            current_atlas_view,
            save_lesson_view,
            run_lesson
        ])
        .build(tauri::generate_context!())
        .expect("Atlas Studio could not start")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
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
