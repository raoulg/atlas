//! Lesson files, saved Atlas sights, and script output within an open workspace.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

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

fn root(workspace: &Path) -> Result<PathBuf, String> {
    workspace.canonicalize().map_err(|error| format!("Cannot open workspace: {error}"))
}

fn inside(workspace: &Path, path: &Path) -> Result<PathBuf, String> {
    let root = root(workspace)?;
    let path = path.canonicalize().map_err(|error| format!("Cannot open {}: {error}", path.display()))?;
    if !path.starts_with(&root) {
        return Err("Lesson references must stay in the open workspace".into());
    }
    Ok(path)
}

fn text(path: &Path) -> Result<String, String> {
    let size = path.metadata().map_err(|error| error.to_string())?.len();
    if size > MAX_TEXT_BYTES {
        return Err("This lesson file is too large to show in Studio".into());
    }
    fs::read_to_string(path).map_err(|error| format!("Cannot read {}: {error}", path.display()))
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

pub fn open(workspace: &Path, filename: &Path) -> Result<Lesson, String> {
    let path = inside(workspace, &workspace.join(filename))?;
    if !matches!(path.extension().and_then(|extension| extension.to_str()), Some("md" | "html")) {
        return Err("A lesson is a Markdown or HTML file".into());
    }
    let markdown = text(&path)?;
    let name = script_name(&markdown).ok_or("The lesson needs an atlas-script header")?;
    if name.is_empty() {
        return Err("The atlas-script header needs a Python filename".into());
    }
    let script = inside(workspace, &path.parent().ok_or("Lesson has no parent folder")?.join(name))?;
    if script.extension().and_then(|extension| extension.to_str()) != Some("py") {
        return Err("The first lesson runner supports Python scripts".into());
    }
    Ok(Lesson { path, script, text: markdown })
}

pub fn read_reference(workspace: &Path, lesson: &Path, reference: &str) -> Result<String, String> {
    let base = inside(workspace, &workspace.join(lesson))?;
    let path = inside(workspace, &base.parent().ok_or("Lesson has no parent folder")?.join(reference))?;
    text(&path)
}

pub fn read_asset(workspace: &Path, lesson: &Path, reference: &Path) -> Result<Vec<u8>, String> {
    let base = inside(workspace, &workspace.join(lesson))?;
    let path = inside(workspace, &base.parent().ok_or("Lesson has no parent folder")?.join(reference))?;
    if path.metadata().map_err(|error| error.to_string())?.len() > MAX_ARTIFACT_BYTES {
        return Err("This artifact is too large to show in Studio".into());
    }
    fs::read(&path).map_err(|error| format!("Cannot read {}: {error}", path.display()))
}

pub fn save_document(workspace: &Path, filename: &Path, markdown: &str) -> Result<(), String> {
    if markdown.len() as u64 > MAX_TEXT_BYTES {
        return Err("This lesson is too large to save in Studio".into());
    }
    let path = inside(workspace, &workspace.join(filename))?;
    if !matches!(path.extension().and_then(|extension| extension.to_str()), Some("md" | "html")) {
        return Err("A lesson is a Markdown or HTML file".into());
    }
    fs::write(&path, markdown).map_err(|error| format!("Cannot save {}: {error}", path.display()))
}

pub fn current_view(workspace: &Path) -> Result<String, String> {
    let path = inside(workspace, &root(workspace)?.join(".atlas").join("shown.json"))?;
    text(&path)
}

pub fn save_view(workspace: &Path, lesson: &Path, name: &str, view: &str) -> Result<String, String> {
    if name.is_empty() || !name.chars().all(|character| character.is_ascii_alphanumeric() || character == '-' || character == '_') {
        return Err("A view name contains only letters, digits, '-' or '_'".into());
    }
    if view.len() > 65536 || !view.trim_start().starts_with('{') {
        return Err("The selected Atlas view is not valid JSON".into());
    }
    let lesson = inside(workspace, &workspace.join(lesson))?;
    let parent = lesson.parent().ok_or("Lesson has no parent folder")?;
    let folder = parent.join("views");
    fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
    let folder = inside(workspace, &folder)?;
    let path = folder.join(format!("{name}.json"));
    if path.exists() {
        return Err(format!("A view named {name} already exists"));
    }
    fs::write(&path, view).map_err(|error| format!("Cannot save {}: {error}", path.display()))?;
    Ok(format!("views/{name}.json"))
}

fn pictures(folder: &Path, depth: usize, found: &mut BTreeMap<PathBuf, (u64, Option<SystemTime>)>) {
    if depth == 0 {
        return;
    }
    let Ok(entries) = fs::read_dir(folder) else { return };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_symlink() {
            continue;
        }
        let path = entry.path();
        if kind.is_dir() {
            if !path.file_name().is_some_and(|name| name.to_string_lossy().starts_with('.')) {
                pictures(&path, depth - 1, found);
            }
        } else if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| matches!(extension.to_ascii_lowercase().as_str(), "png" | "jpg" | "jpeg" | "svg" | "webp" | "gif"))
        {
            if let Ok(metadata) = path.metadata() {
                found.insert(path, (metadata.len(), metadata.modified().ok()));
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

pub fn run_python(workspace: &Path, filename: &Path) -> Result<Run, String> {
    use std::{
        io::Read,
        process::{Command, Stdio},
    };

    let lesson = open(workspace, filename)?;
    let root = root(workspace)?;
    let virtual_python = if cfg!(windows) { root.join(".venv/Scripts/python.exe") } else { root.join(".venv/bin/python") };
    let has_virtual_python = virtual_python.is_file();
    let mut command = if has_virtual_python { Command::new(virtual_python) } else { Command::new(uv_command()) };
    if !has_virtual_python {
        command.arg("run");
        if root.join("pyproject.toml").is_file() {
            if root.join("uv.lock").is_file() {
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
    pictures(script_dir, 3, &mut before);
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
    pictures(script_dir, 3, &mut after);
    let artifacts = after.into_iter().filter(|(path, stamp)| before.get(path) != Some(stamp)).map(|(path, _)| path).collect();
    Ok(Run { stdout, stderr, code: status.code(), artifacts })
}

#[cfg(test)]
mod tests {
    use super::*;

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

pub fn list(workspace: &Path) -> Result<Vec<PathBuf>, String> {
    let root = root(workspace)?;
    let mut lessons = Vec::new();
    fn visit(root: &Path, folder: &Path, depth: usize, lessons: &mut Vec<PathBuf>) {
        if depth == 0 || lessons.len() >= 200 {
            return;
        }
        let Ok(entries) = fs::read_dir(folder) else { return };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_symlink() {
                continue;
            }
            let path = entry.path();
            if kind.is_dir() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if !name.starts_with('.') && !matches!(name.as_str(), "target" | "node_modules" | "dist" | "build") {
                    visit(root, &path, depth - 1, lessons);
                }
            } else if matches!(path.extension().and_then(|extension| extension.to_str()), Some("md" | "html")) {
                if let Ok(markdown) = text(&path) {
                    if script_name(&markdown).is_some() {
                        if let Ok(relative) = path.strip_prefix(root) {
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
    visit(&root, &root, 8, &mut lessons);
    lessons.sort();
    Ok(lessons)
}
