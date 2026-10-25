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

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .manage(StudioState::default())
        .invoke_handler(tauri::generate_handler![current_workspace, choose_workspace])
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
