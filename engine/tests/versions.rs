//! Two parts of one repository that depend on two versions of one library read two libraries, one
//! per directory its code is read from, and a library both read from one directory is one
//! (`mathematics.md`, Coproducts; #372). The repository is written to a directory of its own, with
//! the crates its parts depend on where Cargo keeps them: `orders` locks `notekit` 0.1.0, `billing`
//! locks 0.2.0, and both lock `common` 1.0.0. Nothing is fetched.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;

use atlas_engine::{changes, parts, Graph, Options};

/// Where Cargo keeps a crate fetched from the registry, below its home.
const REGISTRY: &str = "cargo/registry/src/index.example.org-0000000000000000";

/// The crates the parts depend on, by name and version, as Cargo keeps each: `save` writes its file
/// whole in 0.1.0, and in 0.2.0 beside the file and renames it in.
const CRATES: &[(&str, &str, &str)] = &[
    ("notekit", "0.1.0", "use std::fs;\n\npub fn save(path: &str, text: &str) {\n    fs::write(path, text).unwrap();\n}\n"),
    (
        "notekit",
        "0.2.0",
        "use std::fs;\n\npub fn save(path: &str, text: &str) {\n    let partial = format!(\"{path}.part\");\n    fs::write(&partial, text).unwrap();\n    fs::rename(&partial, path).unwrap();\n}\n",
    ),
    ("common", "1.0.0", "pub fn trim(text: &str) -> String {\n    text.trim().to_string()\n}\n"),
];

/// A part named `name` that depends on `notekit` at `version` and on `common`, with the lock that
/// pins them, and whose `main` saves what `common` trims to `file`.
fn part(name: &str, version: &str, file: &str) -> Vec<(String, String)> {
    let manifest =
        format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nnotekit = \"{version}\"\ncommon = \"1\"\n");
    let source = "source = \"registry+https://example.org/index\"";
    let lock = format!(
        "version = 4\n\n[[package]]\nname = \"{name}\"\nversion = \"0.1.0\"\ndependencies = [\n \"common\",\n \"notekit\",\n]\n\n[[package]]\nname = \"common\"\nversion = \"1.0.0\"\n{source}\n\n[[package]]\nname = \"notekit\"\nversion = \"{version}\"\n{source}\n"
    );
    let main = format!("fn main() {{\n    let text = common::trim(\" {name} \");\n    notekit::save(\"{file}\", &text);\n}}\n");
    [("Cargo.toml", manifest), ("Cargo.lock", lock), ("src/main.rs", main)].into_iter().map(|(p, t)| (format!("{name}/{p}"), t)).collect()
}

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=atlas", "-c", "user.email=atlas@example.org", "-c", "commit.gpgsign=false"])
        .args(args)
        .status()
        .expect("git runs")
        .success();
    assert!(ok, "git {args:?}");
}

/// `then` run on the repository `shop`, committed, with Cargo's home beside it as this process's
/// `CARGO_HOME`; written to a directory of `name`'s own and removed. One runs at a time, since the
/// process has one `CARGO_HOME`.
fn in_shop<T>(name: &str, then: impl FnOnce(&Path) -> T) -> T {
    static ONE: Mutex<()> = Mutex::new(());
    let _one = ONE.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("atlas-versions-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut files: Vec<(String, String)> = vec![];
    for (crate_name, version, lib) in CRATES {
        let manifest = format!("[package]\nname = \"{crate_name}\"\nversion = \"{version}\"\nedition = \"2021\"\n");
        files.push((format!("{REGISTRY}/{crate_name}-{version}/Cargo.toml"), manifest));
        files.push((format!("{REGISTRY}/{crate_name}-{version}/src/lib.rs"), lib.to_string()));
    }
    files.extend(part("orders", "0.1.0", "orders.txt").into_iter().map(|(p, t)| (format!("shop/{p}"), t)));
    files.extend(part("billing", "0.2.0", "bills.txt").into_iter().map(|(p, t)| (format!("shop/{p}"), t)));
    for (path, text) in &files {
        let file = dir.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }
    let shop = dir.join("shop");
    git(&shop, &["init", "-q"]);
    git(&shop, &["add", "."]);
    git(&shop, &["commit", "-q", "-m", "shop"]);
    std::env::set_var("CARGO_HOME", dir.join("cargo"));
    let out = then(&shop);
    std::env::remove_var("CARGO_HOME");
    let _ = std::fs::remove_dir_all(&dir);
    out
}

/// The callee of the step of `from` that the code writes as `written`.
fn calls(g: &Graph, from: &str, written: &str) -> String {
    let f = g.node(from).unwrap_or_else(|| panic!("no node {from}"));
    let s = f.sites.iter().find(|s| s.to.ends_with(written)).unwrap_or_else(|| panic!("{from} has no step to {written}: {:#?}", f.sites));
    s.to.clone()
}

/// Law: an id names one node, so the merge of the parts holds no id twice. A library the parts read
/// from two directories (two versions Cargo keeps apart) is two libraries, each named after its
/// version, and each part's route runs its own; a library both read from one directory keeps its
/// name and is one node both parts call.
#[test]
fn two_versions_are_two_libraries() {
    let g = in_shop("merged", |shop| parts::analyze_repo(&[shop.to_path_buf()], &Options::default())).expect("the parts parse");
    let ids: Vec<&str> = g.nodes.iter().map(|n| n.id.as_str()).collect();
    let unique: BTreeSet<&str> = ids.iter().copied().collect();
    assert_eq!(
        ids.len(),
        unique.len(),
        "an id held twice: {:?}",
        ids.iter().filter(|i| ids.iter().filter(|j| j == i).count() > 1).collect::<Vec<_>>()
    );
    for (part, version) in [("orders", "0.1.0"), ("billing", "0.2.0")] {
        let save = calls(&g, &format!("{part}.main"), ".save");
        assert_eq!(save, format!("notekit-{}.save", version.replace('.', "_")), "{part} runs the version its lock pins");
        let read = g.node(&save).unwrap();
        assert!(read.library && read.file.contains(&format!("notekit-{version}")), "{}", read.file);
        assert_eq!(g.libraries[&format!("notekit-{}", version.replace('.', "_"))].version, version);
        assert_eq!(calls(&g, &format!("{part}.main"), ".trim"), "common.trim", "one directory, one library under its own name");
    }
    assert!(g.node("notekit.save").is_none() && g.nodes.iter().filter(|n| n.id == "common.trim").count() == 1);
    // each version does what its code does: 0.2.0 renames its file in, 0.1.0 does not
    let renames = |id: &str| g.node(id).unwrap().touches.iter().any(|t| t.op == "rename");
    assert!(renames("notekit-0_2_0.save") && !renames("notekit-0_1_0.save"), "{:#?}", g.node("notekit-0_2_0.save").unwrap().touches);
}

/// Law: a reading is a function of its revision, so a revision compared with itself changes
/// nothing, and neither does the tree at that revision. Both readings are made and compared.
#[test]
fn a_revision_against_itself_changes_nothing() {
    let (same, tree) = in_shop("changes", |shop| {
        (changes::compare(shop, "HEAD", Some("HEAD"), &Options::default()), changes::since(shop, "HEAD", &Options::default()))
    });
    for c in [same.expect("HEAD against HEAD"), tree.expect("the tree against HEAD")] {
        let e = &c.elsewhere;
        assert!(
            c.routes.is_empty()
                && c.findings_added.is_empty()
                && c.findings_resolved.is_empty()
                && e.added.is_empty()
                && e.removed.is_empty()
                && e.changed.is_empty()
                && e.places_added.is_empty()
                && e.places_removed.is_empty()
                && e.places_changed.is_empty(),
            "{}",
            changes::report(&c)
        );
    }
}
