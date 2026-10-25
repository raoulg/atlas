use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use atlas_engine::cache::Cache;
use atlas_engine::{flow, parts, principles, Graph, Options};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn workspace() -> PathBuf {
    let root = std::env::temp_dir().join(format!("atlas-workspace-index-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    std::fs::create_dir_all(root.join(".git")).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"index_example\"\nversion = \"0.1.0\"\nedition = \"2021\"\n").unwrap();
    std::fs::write(root.join("src/main.rs"), "fn main() { println!(\"one\"); }\n").unwrap();
    root
}

fn two_parts() -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("atlas-workspace-index-parts-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    std::fs::create_dir_all(root.join(".git")).unwrap();
    for name in ["one", "two"] {
        let part = root.join(name);
        std::fs::create_dir_all(part.join("src")).unwrap();
        std::fs::write(part.join("package.json"), format!(r#"{{"name":"{name}","version":"1.0.0"}}"#)).unwrap();
        std::fs::write(part.join("src/index.ts"), format!("export function {name}(): number {{ return 1; }}\n")).unwrap();
    }
    root
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let dest = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &dest);
        } else {
            std::fs::copy(entry.path(), dest).unwrap();
        }
    }
}

fn read(root: &Path, opts: &Options) -> Vec<Graph> {
    parts::analyze_parts(&[root.to_path_buf()], opts).unwrap()
}

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git").arg("-C").arg(root).args(args).output().unwrap();
    assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
}

fn same_reading(a: &[Graph], b: &[Graph]) {
    assert_eq!(serde_json::to_value(a).unwrap(), serde_json::to_value(b).unwrap());
    for (a, b) in a.iter().zip(b) {
        assert_eq!(serde_json::to_value(flow::of(a)).unwrap(), serde_json::to_value(flow::of(b)).unwrap());
        assert_eq!(principles::check(a), principles::check(b));
    }
}

#[test]
fn cold_warm_and_changed_worktrees_have_the_same_reading() {
    let root = workspace();
    let cold = Options { libraries: 0, ..Options::default() };
    let indexed = Options { cache: Some(Cache::at(&root.join("global"), "index-test")), ..cold.clone() };

    let first = read(&root, &indexed);
    same_reading(&first, &read(&root, &cold));
    let warm = read(&root, &indexed);
    same_reading(&warm, &read(&root, &cold));
    assert!(indexed.cache.as_ref().unwrap().hits() > 0);

    std::fs::write(root.join("src/main.rs"), "fn main() { println!(\"two\"); }\n").unwrap();
    let changed = read(&root, &indexed);
    same_reading(&changed, &read(&root, &cold));
    assert_ne!(serde_json::to_value(&first).unwrap(), serde_json::to_value(&changed).unwrap());

    std::fs::write(root.join("src/helper.rs"), "pub fn helper() {}\n").unwrap();
    same_reading(&read(&root, &indexed), &read(&root, &cold));
    std::fs::remove_file(root.join("src/helper.rs")).unwrap();
    same_reading(&read(&root, &indexed), &read(&root, &cold));

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_damaged_snapshot_falls_back_to_a_fresh_reading() {
    let root = workspace();
    let cold = Options { libraries: 0, ..Options::default() };
    let indexed = Options { cache: Some(Cache::at(&root.join("global"), "index-test")), ..cold.clone() };
    read(&root, &indexed);
    let entries = root.join(".atlas/index/index-test/part");
    let entry = std::fs::read_dir(entries).unwrap().next().unwrap().unwrap().path();
    std::fs::write(entry, b"broken").unwrap();
    same_reading(&read(&root, &indexed), &read(&root, &cold));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn editing_one_part_reuses_the_other() {
    let root = two_parts();
    let cold = Options { libraries: 0, ..Options::default() };
    let indexed = Options { cache: Some(Cache::at(&root.join("global"), "index-test")), ..cold.clone() };
    let cache = indexed.cache.as_ref().unwrap();

    let first = read(&root, &indexed);
    same_reading(&first, &read(&root, &cold));
    assert_eq!(first.len(), 2);
    assert_eq!(cache.writes(), 2);
    same_reading(&read(&root, &indexed), &first);
    assert_eq!(cache.writes(), 2);

    std::fs::write(root.join("one/src/index.ts"), "export function one(): number { return 2; }\n").unwrap();
    same_reading(&read(&root, &indexed), &read(&root, &cold));
    assert_eq!(cache.writes(), 3);

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_python_pair_has_the_same_flow_and_findings_when_warm() {
    let root =
        std::env::temp_dir().join(format!("atlas-workspace-index-python-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    let pair = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/choices/python/clean");
    copy_tree(&pair, &root);
    std::fs::create_dir_all(root.join(".git")).unwrap();

    let cold = Options { libraries: 0, ..Options::default() };
    let indexed = Options { cache: Some(Cache::at(&root.join("global"), "index-test")), ..cold.clone() };
    same_reading(&read(&root, &indexed), &read(&root, &cold));
    same_reading(&read(&root, &indexed), &read(&root, &cold));

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn renamed_sources_and_changed_manifests_match_a_cold_reading() {
    let root = two_parts();
    let cold = Options { libraries: 0, ..Options::default() };
    let indexed = Options { cache: Some(Cache::at(&root.join("global"), "index-test")), ..cold.clone() };
    let cache = indexed.cache.as_ref().unwrap();
    read(&root, &indexed);
    assert_eq!(cache.writes(), 2);

    std::fs::rename(root.join("one/src/index.ts"), root.join("one/src/renamed.ts")).unwrap();
    same_reading(&read(&root, &indexed), &read(&root, &cold));
    assert_eq!(cache.writes(), 3);

    std::fs::write(root.join("two/package.json"), "{\"name\":\"second\",\"version\":\"1.0.0\"}").unwrap();
    same_reading(&read(&root, &indexed), &read(&root, &cold));
    assert_eq!(cache.writes(), 4);

    std::fs::remove_file(root.join("one/package.json")).unwrap();
    same_reading(&read(&root, &indexed), &read(&root, &cold));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn each_worktree_keeps_its_own_snapshot() {
    let first = workspace();
    git(&first, &["init", "--initial-branch=main"]);
    git(&first, &["add", "Cargo.toml", "src/main.rs"]);
    git(&first, &["-c", "user.name=Atlas", "-c", "user.email=atlas@example.invalid", "commit", "-m", "initial"]);
    let second =
        std::env::temp_dir().join(format!("atlas-workspace-index-other-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    git(&first, &["worktree", "add", "-b", "other", second.to_str().unwrap()]);
    let cache = Cache::at(&first.join("global"), "index-test");
    let cold = Options { libraries: 0, ..Options::default() };
    let indexed = Options { cache: Some(cache.clone()), ..cold.clone() };

    std::fs::write(second.join("src/main.rs"), "fn main() { println!(\"other branch\"); }\n").unwrap();
    same_reading(&read(&first, &indexed), &read(&first, &cold));
    same_reading(&read(&second, &indexed), &read(&second, &cold));
    assert_eq!(cache.writes(), 2);
    same_reading(&read(&first, &indexed), &read(&first, &cold));
    same_reading(&read(&second, &indexed), &read(&second, &cold));
    assert_eq!(cache.writes(), 2);

    git(&first, &["switch", "-c", "changed"]);
    std::fs::write(first.join("src/main.rs"), "fn main() { println!(\"switched branch\"); }\n").unwrap();
    git(&first, &["add", "src/main.rs"]);
    git(&first, &["-c", "user.name=Atlas", "-c", "user.email=atlas@example.invalid", "commit", "-m", "changed"]);
    same_reading(&read(&first, &indexed), &read(&first, &cold));
    assert_eq!(cache.writes(), 3);
    git(&first, &["switch", "main"]);
    same_reading(&read(&first, &indexed), &read(&first, &cold));
    assert_eq!(cache.writes(), 4);
    assert!(cache.hits() >= 2);
    git(&first, &["worktree", "remove", "--force", second.to_str().unwrap()]);
    let _ = std::fs::remove_dir_all(first);
}

#[test]
fn changed_options_select_an_independent_reading() {
    let root = workspace();
    let cache = Cache::at(&root.join("global"), "index-test");
    let cold = Options { libraries: 0, ..Options::default() };
    let indexed = Options { cache: Some(cache.clone()), ..cold.clone() };
    read(&root, &indexed);
    let named = Options { named: vec![root.join("src/main.rs")], ..indexed.clone() };
    same_reading(&read(&root, &named), &read(&root, &Options { cache: None, ..named.clone() }));
    assert_eq!(cache.writes(), 2);
    same_reading(&read(&root, &indexed), &read(&root, &cold));
    assert_eq!(cache.writes(), 2);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_shared_library_environment_refreshes_dependent_parts() {
    let root = two_parts();
    let cold = Options { libraries: 1, ..Options::default() };
    let indexed = Options { cache: Some(Cache::at(&root.join("global"), "index-test")), ..cold.clone() };
    let cache = indexed.cache.as_ref().unwrap();
    read(&root, &indexed);
    assert_eq!(cache.writes(), 2);

    let library = root.join("node_modules/shared");
    std::fs::create_dir_all(&library).unwrap();
    std::fs::write(library.join("package.json"), "{\"name\":\"shared\",\"types\":\"index.d.ts\"}").unwrap();
    std::fs::write(library.join("index.d.ts"), "export declare function value(): number;\n").unwrap();
    same_reading(&read(&root, &indexed), &read(&root, &cold));
    assert_eq!(cache.writes(), 4);

    std::fs::write(library.join("index.d.ts"), "export declare function value(): string;\n").unwrap();
    same_reading(&read(&root, &indexed), &read(&root, &cold));
    assert_eq!(cache.writes(), 6);
    let _ = std::fs::remove_dir_all(root);
}
