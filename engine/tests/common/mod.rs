//! What the pair tests share.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use atlas_engine::decide::{prophesy, Prophesied};
use atlas_engine::revelation::{revelation, Stand};
use atlas_engine::{analyze, Options};

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("makes the directory");
    for e in std::fs::read_dir(from).expect("reads the directory").flatten() {
        let dest = to.join(e.file_name());
        if e.path().is_dir() {
            copy_tree(&e.path(), &dest);
        } else {
            std::fs::copy(e.path(), &dest).expect("copies the file");
        }
    }
}

/// The `clean` side of a prophecy pair with `addition` appended to its file `file`, as a vision
/// that is decided: what `prophesy` writes, and the revelation of the tree it is written into.
/// The pair itself stays as it is; the tree is a copy.
pub fn decided(language: &str, file: &str, addition: &str, supersedes: &[(&str, &str)]) -> (Prophesied, Vec<Stand>) {
    let pair = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/prophecy").join(language).join("clean");
    let copy = std::env::temp_dir().join(format!("atlas-prophesy-{language}-{}", std::process::id())).join("clean");
    let _ = std::fs::remove_dir_all(copy.parent().expect("has a parent"));
    copy_tree(&pair, &copy);
    let now = std::fs::read_to_string(copy.join(file)).expect("reads the file");
    let vision = BTreeMap::from([(PathBuf::from(file), Some(format!("{now}{addition}")))]);
    let supersedes: BTreeMap<String, String> = supersedes.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
    let decided = prophesy(&copy, &vision, &supersedes, &Options::default()).expect("a vision that only adds is decided");
    for (rel, text) in &decided.files {
        std::fs::write(copy.join(rel), text).expect("writes the file");
    }
    let stands = revelation(&analyze(&copy, &Options::default()).expect("reads"));
    let _ = std::fs::remove_dir_all(copy.parent().expect("has a parent"));
    (decided, stands)
}
