//! Lesson files, saved Atlas sights, and script output within an open workspace.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::SystemTime,
};

use crate::workspace::Workspace;

const MAX_TEXT_BYTES: u64 = 2 * 1024 * 1024;
const MAX_ARTIFACT_BYTES: u64 = 16 * 1024 * 1024;
const MAX_TERMINAL_BYTES: usize = 1024 * 1024;

pub struct Lesson {
    pub path: PathBuf,
    pub script: PathBuf,
    pub text: String,
}

pub struct Run {
    pub stdout: String,
    pub stderr: String,
    pub code: Option<i32>,
    pub artifacts: Vec<PathBuf>,
}

/// A path of a lesson, resolved, where it stays in the open workspace.
fn inside(workspace: &Workspace, path: &Path) -> Result<PathBuf, String> {
    workspace.inside(path, &path.display().to_string(), "Lesson references must stay in the open workspace")
}

/// A lesson file's text, small enough to show.
fn text(workspace: &Workspace, path: &Path) -> Result<String, String> {
    workspace.text(path, MAX_TEXT_BYTES, "This lesson file is too large to show in Studio")
}

fn script_name(document: &str) -> Option<&str> {
    let mut lines = document.lines();
    let first = lines.next()?;
    if let Some(value) = first.strip_prefix("<!-- atlas-script:") {
        return value.trim().strip_suffix("-->").map(str::trim);
    }
    if first != "---" {
        return None;
    }
    for line in lines {
        if line == "---" {
            break;
        }
        if let Some(value) = line.strip_prefix("atlas-script:") {
            return Some(value.trim());
        }
    }
    None
}

pub fn open(folder: &Path, filename: &Path) -> Result<Lesson, String> {
    let workspace = Workspace::open(folder)?;
    let path = inside(&workspace, &folder.join(filename))?;
    if !matches!(path.extension().and_then(|extension| extension.to_str()), Some("md" | "html")) {
        return Err("A lesson is a Markdown or HTML file".into());
    }
    let markdown = text(&workspace, &path)?;
    let name = script_name(&markdown).ok_or("The lesson needs an atlas-script header")?;
    if name.is_empty() {
        return Err("The atlas-script header needs a Python filename".into());
    }
    let script = inside(&workspace, &path.parent().ok_or("Lesson has no parent folder")?.join(name))?;
    if script.extension().and_then(|extension| extension.to_str()) != Some("py") {
        return Err("The first lesson runner supports Python scripts".into());
    }
    Ok(Lesson { path, script, text: markdown })
}

pub fn read_reference(folder: &Path, lesson: &Path, reference: &str) -> Result<String, String> {
    let workspace = Workspace::open(folder)?;
    let base = inside(&workspace, &folder.join(lesson))?;
    let path = inside(&workspace, &base.parent().ok_or("Lesson has no parent folder")?.join(reference))?;
    text(&workspace, &path)
}

pub fn read_asset(folder: &Path, lesson: &Path, reference: &Path) -> Result<Vec<u8>, String> {
    let workspace = Workspace::open(folder)?;
    let base = inside(&workspace, &folder.join(lesson))?;
    let path = inside(&workspace, &base.parent().ok_or("Lesson has no parent folder")?.join(reference))?;
    workspace.bytes(&path, MAX_ARTIFACT_BYTES, "This artifact is too large to show in Studio".into(), &path.display().to_string())
}

pub fn save_document(folder: &Path, filename: &Path, markdown: &str) -> Result<(), String> {
    if markdown.len() as u64 > MAX_TEXT_BYTES {
        return Err("This lesson is too large to save in Studio".into());
    }
    let workspace = Workspace::open(folder)?;
    let path = inside(&workspace, &folder.join(filename))?;
    if !matches!(path.extension().and_then(|extension| extension.to_str()), Some("md" | "html")) {
        return Err("A lesson is a Markdown or HTML file".into());
    }
    workspace.write(&path, markdown)
}

pub fn current_view(folder: &Path) -> Result<String, String> {
    let workspace = Workspace::open(folder)?;
    let path = inside(&workspace, &workspace.root().join(".atlas").join("shown.json"))?;
    text(&workspace, &path)
}

pub fn save_view(folder: &Path, lesson: &Path, name: &str, view: &str) -> Result<String, String> {
    if name.is_empty() || !name.chars().all(|character| character.is_ascii_alphanumeric() || character == '-' || character == '_') {
        return Err("A view name contains only letters, digits, '-' or '_'".into());
    }
    if view.len() > 65536 || !view.trim_start().starts_with('{') {
        return Err("The selected Atlas view is not valid JSON".into());
    }
    let workspace = Workspace::open(folder)?;
    let lesson = inside(&workspace, &folder.join(lesson))?;
    let parent = lesson.parent().ok_or("Lesson has no parent folder")?;
    let views = parent.join("views");
    workspace.folder(&views)?;
    let views = inside(&workspace, &views)?;
    let path = views.join(format!("{name}.json"));
    if workspace.has(&path) {
        return Err(format!("A view named {name} already exists"));
    }
    workspace.write(&path, view)?;
    Ok(format!("views/{name}.json"))
}

fn pictures(workspace: &Workspace, folder: &Path, depth: usize, found: &mut BTreeMap<PathBuf, (u64, Option<SystemTime>)>) {
    if depth == 0 {
        return;
    }
    for (path, kind) in workspace.entries(folder) {
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            if !path.file_name().is_some_and(|name| name.to_string_lossy().starts_with('.')) {
                pictures(workspace, &path, depth - 1, found);
            }
        } else if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| matches!(extension.to_ascii_lowercase().as_str(), "png" | "jpg" | "jpeg" | "svg" | "webp" | "gif"))
        {
            if let Some(stamp) = workspace.stamp(&path) {
                found.insert(path, stamp);
            }
        }
    }
}

fn uv_command() -> PathBuf {
    if let Some(command) = std::env::var_os("ATLAS_UV") {
        return PathBuf::from(command);
    }
    let mut candidates = vec![PathBuf::from("/opt/homebrew/bin/uv"), PathBuf::from("/usr/local/bin/uv")];
    if let Some(home) = std::env::var_os("HOME") {
        candidates.push(PathBuf::from(home).join(".local/bin/uv"));
    }
    candidates.into_iter().find(|path| path.is_file()).unwrap_or_else(|| PathBuf::from("uv"))
}

pub fn run_python(folder: &Path, filename: &Path) -> Result<Run, String> {
    use std::{
        io::Read,
        process::{Command, Stdio},
    };

    let lesson = open(folder, filename)?;
    let workspace = Workspace::open(folder)?;
    let root = workspace.root();
    let python = if cfg!(windows) { ".venv/Scripts/python.exe" } else { ".venv/bin/python" };
    let has_virtual_python = workspace.holds(python);
    let mut command = if has_virtual_python { Command::new(root.join(python)) } else { Command::new(uv_command()) };
    if !has_virtual_python {
        command.arg("run");
        if workspace.holds("pyproject.toml") {
            if workspace.holds("uv.lock") {
                command.arg("--frozen");
            }
            command.arg("--project").arg(&root);
        } else {
            command.arg("--no-project");
        }
    }
    let script_dir = lesson.script.parent().ok_or("Script has no parent folder")?;
    command.arg(&lesson.script).current_dir(script_dir).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut before = BTreeMap::new();
    pictures(&workspace, script_dir, 3, &mut before);
    let mut process = command.spawn().map_err(|error| format!("Cannot start Python: {error}"))?;
    let stdout = process.stdout.take().ok_or("Python has no stdout")?;
    let stderr = process.stderr.take().ok_or("Python has no stderr")?;
    let collect = |mut stream: Box<dyn Read + Send>| {
        std::thread::spawn(move || {
            let mut kept = Vec::new();
            let mut chunk = [0; 8192];
            while let Ok(size) = stream.read(&mut chunk) {
                if size == 0 {
                    break;
                }
                let room = MAX_TERMINAL_BYTES.saturating_sub(kept.len());
                kept.extend_from_slice(&chunk[..size.min(room)]);
            }
            String::from_utf8_lossy(&kept).into_owned()
        })
    };
    let out = collect(Box::new(stdout));
    let err = collect(Box::new(stderr));
    let status = process.wait().map_err(|error| error.to_string())?;
    let stdout = out.join().map_err(|_| "Cannot read Python stdout")?;
    let stderr = err.join().map_err(|_| "Cannot read Python stderr")?;
    let mut after = BTreeMap::new();
    pictures(&workspace, script_dir, 3, &mut after);
    let artifacts = after.into_iter().filter(|(path, stamp)| before.get(path) != Some(stamp)).map(|(path, _)| path).collect();
    Ok(Run { stdout, stderr, code: status.code(), artifacts })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn opens_markdown_and_saves_a_view() {
        let workspace = std::env::temp_dir().join(format!("atlas-lesson-open-{}", std::process::id()));
        fs::create_dir_all(workspace.join("chapter")).unwrap();
        fs::write(workspace.join("chapter/lesson.md"), "---\natlas-script: demo.py\n---\n# Lesson\n").unwrap();
        fs::write(workspace.join("chapter/demo.py"), "print('hello')\n").unwrap();
        let lesson = open(&workspace, Path::new("chapter/lesson.md")).unwrap();
        assert_eq!(lesson.script, workspace.canonicalize().unwrap().join("chapter/demo.py"));
        fs::write(workspace.join("chapter/lesson.html"), "<!-- atlas-script: demo.py -->\n<h1>Lesson</h1>").unwrap();
        let html = open(&workspace, Path::new("chapter/lesson.html")).unwrap();
        assert!(html.text.contains("<h1>Lesson</h1>"));
        assert_eq!(list(&workspace).unwrap(), vec![PathBuf::from("chapter/lesson.html"), PathBuf::from("chapter/lesson.md")]);
        let reference = save_view(&workspace, Path::new("chapter/lesson.md"), "predict", "{\"route\":\"demo\"}").unwrap();
        assert_eq!(read_reference(&workspace, Path::new("chapter/lesson.md"), &reference).unwrap(), "{\"route\":\"demo\"}");
        fs::remove_dir_all(&workspace).unwrap();
    }

    #[test]
    fn rejects_references_outside_workspace() {
        let workspace = std::env::temp_dir().join(format!("atlas-lesson-boundary-{}", std::process::id()));
        fs::create_dir_all(&workspace).unwrap();
        fs::write(workspace.join("lesson.md"), "---\natlas-script: demo.py\n---\n").unwrap();
        fs::write(workspace.join("demo.py"), "print('hello')").unwrap();
        let outside = workspace.with_extension("outside");
        fs::write(&outside, "outside the workspace").unwrap();
        let reference = format!("../{}", outside.file_name().unwrap().to_string_lossy());
        let error = read_reference(&workspace, Path::new("lesson.md"), &reference).unwrap_err();
        assert!(error.contains("must stay in the open workspace"));
        fs::remove_file(outside).unwrap();
        fs::remove_dir_all(&workspace).unwrap();
    }
}

pub fn list(folder: &Path) -> Result<Vec<PathBuf>, String> {
    let workspace = Workspace::open(folder)?;
    let mut lessons = Vec::new();
    fn visit(workspace: &Workspace, folder: &Path, depth: usize, lessons: &mut Vec<PathBuf>) {
        if depth == 0 || lessons.len() >= 200 {
            return;
        }
        for (path, kind) in workspace.entries(folder) {
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                if !name.starts_with('.') && !matches!(name.as_str(), "target" | "node_modules" | "dist" | "build") {
                    visit(workspace, &path, depth - 1, lessons);
                }
            } else if matches!(path.extension().and_then(|extension| extension.to_str()), Some("md" | "html")) {
                if let Ok(markdown) = text(workspace, &path) {
                    if script_name(&markdown).is_some() {
                        if let Ok(relative) = path.strip_prefix(workspace.root()) {
                            lessons.push(relative.to_path_buf());
                        }
                    }
                }
            }
            if lessons.len() >= 200 {
                break;
            }
        }
    }
    visit(&workspace, workspace.root(), 8, &mut lessons);
    lessons.sort();
    Ok(lessons)
}
