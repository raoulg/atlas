//! A file named on the command line is read as part of its project even where git ignores it,
//! and a file outside any project is a project of its own.

use std::path::{Path, PathBuf};

use atlas_engine::{analyze, EntryKind, Options};

fn named(files: &[&Path]) -> Options {
    Options { named: files.iter().map(|f| std::fs::canonicalize(f).unwrap()).collect(), ..Options::default() }
}

#[test]
fn a_named_file_is_read_where_git_ignores_it() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixture-lib");
    let has = |g: &atlas_engine::Graph| g.nodes.iter().any(|n| n.id == "drafts.scratch" && n.entry.as_ref().map(|e| e.kind) == Some(EntryKind::Script));
    assert!(!has(&analyze(&repo, &named(&[])).unwrap()), "an ignored file is not read unless it is named");
    assert!(has(&analyze(&repo, &named(&[&repo.join("drafts/scratch.py")])).unwrap()), "a named file is read as one of its project's scripts");

    // outside any project, the project is the files named, not their folder
    let dir = std::env::temp_dir().join(format!("atlas-named-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("named.py"), "def go() -> None:\n    print('go')\n\ngo()\n").unwrap();
    std::fs::write(dir.join("beside.py"), "print('beside')\n").unwrap();
    let g = analyze(&dir, &named(&[&dir.join("named.py")])).expect("a lone file is a project");
    let _ = std::fs::remove_dir_all(&dir);
    let modules: Vec<&str> = g.nodes.iter().filter(|n| n.kind == atlas_engine::NodeKind::Module).map(|n| n.name.as_str()).collect();
    assert_eq!(modules, ["named"], "only the named file is read");
}

/// A package named on its own is read with the project it is in, once: the same graph as the
/// project's, whether the path is written relative or absolute.
#[test]
fn a_package_named_on_its_own_is_its_project() {
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let project = serde_json::to_string(&analyze(&here.join("tests/fixture"), &Options::default()).unwrap()).unwrap();
    let package = serde_json::to_string(&analyze(&here.join("tests/fixture/src/fixture"), &Options::default()).unwrap()).unwrap();
    assert!(project == package, "the package named on its own reads as its project");
    let g = analyze(Path::new("tests/fixture/src/fixture"), &Options::default()).unwrap();
    assert!(serde_json::to_string(&g).unwrap() == project, "a relative path finds the project above it too, and reads it the same");
    assert!(!g.make.is_empty(), "the project's Makefile is read with it");
}
