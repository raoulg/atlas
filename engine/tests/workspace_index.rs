use std::path::{Path, PathBuf};
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
