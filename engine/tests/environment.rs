//! A Python project's libraries come from the project's own environment: what its configuration
//! names, else its `.venv`. The venv that is active in the shell Atlas runs in is not the
//! project's, so the same files give the same graph wherever Atlas is started.
//!
//! One test in its own file: it sets `VIRTUAL_ENV` for the whole process.

use std::path::Path;

use atlas_engine::{analyze, Options};

/// A virtual environment at `dir` holding one library, `elsewhere`, with a class `Thing`.
fn venv(dir: &Path) {
    let site = dir.join("lib/python3.13/site-packages");
    std::fs::create_dir_all(&site).unwrap();
    std::fs::write(dir.join("pyvenv.cfg"), "home = /usr/bin\nversion_info = 3.13.0\ninclude-system-site-packages = false\n").unwrap();
    std::fs::write(site.join("elsewhere.py"), "class Thing:\n    def size(self) -> int:\n        return 1\n").unwrap();
}

#[test]
fn the_active_venv_is_not_the_projects() {
    let dir = std::env::temp_dir().join(format!("atlas-environment-{}", std::process::id()));
    let (project, other) = (dir.join("project"), dir.join("other-venv"));
    std::fs::create_dir_all(project.join("app")).unwrap();
    std::fs::write(project.join("pyproject.toml"), "[project]\nname = \"app\"\nversion = \"0\"\n").unwrap();
    std::fs::write(project.join("app/__init__.py"), "").unwrap();
    std::fs::write(project.join("app/main.py"), "from elsewhere import Thing\n\n\ndef size(t: Thing) -> int:\n    return t.size()\n")
        .unwrap();
    venv(&other);

    let graph = || serde_json::to_string(&analyze(&project, &Options::default()).unwrap()).unwrap();
    let alone = graph();
    // SAFETY: this file holds one test, so nothing else in the process reads the environment
    unsafe { std::env::set_var("VIRTUAL_ENV", &other) };
    let with_other = graph();
    unsafe { std::env::remove_var("VIRTUAL_ENV") };
    let _ = std::fs::remove_dir_all(&dir);
    assert!(!with_other.contains("elsewhere.Thing.size"), "a library from the shell's venv is read");
    assert_eq!(alone, with_other, "the shell's venv changes the graph");
}
