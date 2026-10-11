//! A file named on the command line is read as part of its project even where git ignores it,
//! and a file outside any project is a project of its own. The files named are a setting for
//! the whole process, so these tests run in a binary of their own.

use std::path::PathBuf;

use atlas_engine::{analyze, python, EntryKind};

#[test]
fn a_named_file_is_read_where_git_ignores_it() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixture-lib");
    let has = |g: &atlas_engine::Graph| g.nodes.iter().any(|n| n.id == "drafts.scratch" && n.entry.as_ref().map(|e| e.kind) == Some(EntryKind::Script));
    python::read_named(&[]);
    assert!(!has(&analyze(&repo, None, None).unwrap()), "an ignored file is not read unless it is named");
    python::read_named(&[repo.join("drafts/scratch.py")]);
    assert!(has(&analyze(&repo, None, None).unwrap()), "a named file is read as one of its project's scripts");

    // outside any project, the project is the files named, not their folder
    let dir = std::env::temp_dir().join(format!("atlas-named-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("named.py"), "def go() -> None:\n    print('go')\n\ngo()\n").unwrap();
    std::fs::write(dir.join("beside.py"), "print('beside')\n").unwrap();
    python::read_named(&[dir.join("named.py")]);
    let g = analyze(&dir, None, None).expect("a lone file is a project");
    let _ = std::fs::remove_dir_all(&dir);
    let modules: Vec<&str> = g.nodes.iter().filter(|n| n.kind == atlas_engine::NodeKind::Module).map(|n| n.name.as_str()).collect();
    assert_eq!(modules, ["named"], "only the named file is read");
}
