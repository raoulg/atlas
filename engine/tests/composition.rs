//! Ordered compositions in two committed readings of the same typed program.

use std::path::Path;
use std::process::Command;

use atlas_engine::changes;
use atlas_engine::diff::{self, Status};
use atlas_engine::{Graph, Options};

const PROGRAM: &str = "def add_one(value: int) -> int:\n    return value + 1\n\ndef double(value: int) -> int:\n    return value * 2\n\ndef run() -> None:\n    print(EXPRESSION)\n";

fn git(repo: &Path, args: &[&str]) {
    let result = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["-c", "user.name=atlas", "-c", "user.email=atlas@example.org", "-c", "commit.gpgsign=false"])
        .args(args)
        .output()
        .unwrap();
    assert!(result.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&result.stderr));
}

fn calls(g: &Graph) -> Vec<&str> {
    g.node("composition.run").unwrap().sites.iter().map(|s| s.to.as_str()).collect()
}

#[test]
fn reversing_nested_calls_preserves_order_and_marks_occurrences_not_functions() {
    let base = std::env::temp_dir().join(format!("atlas-composition-{}", std::process::id()));
    let repo = base.join("composition");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::write(
        repo.join("pyproject.toml"),
        "[project]\nname=\"composition\"\nversion=\"0.1.0\"\n[project.scripts]\ncomposition=\"composition:run\"\n",
    )
    .unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    for (expression, label) in [("double(add_one(3))", "first"), ("add_one(double(3))", "second")] {
        std::fs::write(repo.join("composition.py"), PROGRAM.replace("EXPRESSION", expression)).unwrap();
        git(&repo, &["add", "composition.py", "pyproject.toml"]);
        git(&repo, &["commit", "-q", "-m", label]);
    }
    let a = changes::parts_at(&repo, Some("HEAD~1"), &Options::default()).unwrap();
    let b = changes::parts_at(&repo, Some("HEAD"), &Options::default()).unwrap();
    assert_eq!(calls(&a[0]), ["composition.add_one", "composition.double"]);
    assert_eq!(calls(&b[0]), ["composition.double", "composition.add_one"]);
    for g in [&a[0], &b[0]] {
        let sites = &g.node("composition.run").unwrap().sites;
        assert!(!sites[0].gives.is_empty());
        assert_eq!(sites[0].gives[0].name, sites[1].hands[0].name, "the outer call consumes the inner result");
    }
    let changed = changes::between("HEAD~1", Some("HEAD"), &a, &b);
    let route = changed.routes.iter().find(|r| r.id == "composition.run").expect("the route composition differs");
    assert_eq!(route.status, Status::Changed);
    assert!(route.steps_added.is_empty() && route.steps_removed.is_empty());
    assert!(route.places_added.is_empty() && route.places_removed.is_empty());
    assert_eq!(route.compositions_changed.len(), 1);
    let composition = &route.compositions_changed[0];
    assert_eq!(composition.function, "composition.run");
    assert_eq!(composition.before.steps().iter().map(|s| s.to.as_str()).collect::<Vec<_>>(), calls(&a[0]));
    assert_eq!(composition.after.steps().iter().map(|s| s.to.as_str()).collect::<Vec<_>>(), calls(&b[0]));
    assert!(changes::between("same", Some("same"), &a, &a).routes.is_empty());
    std::fs::write(repo.join("composition.py"), format!("# A named composition.\n{}", PROGRAM.replace("EXPRESSION", "add_one(double(3))")))
        .unwrap();
    git(&repo, &["commit", "-q", "-am", "comment"]);
    let commented = changes::parts_at(&repo, Some("HEAD"), &Options::default()).unwrap();
    assert!(changes::between("second", Some("comment"), &b, &commented).routes.is_empty(), "source positions are not composition");
    std::fs::write(repo.join("composition.py"), PROGRAM.replace("EXPRESSION", "add_one(add_one(double(3)))")).unwrap();
    git(&repo, &["commit", "-q", "-am", "repeat"]);
    let repeated = changes::parts_at(&repo, Some("HEAD"), &Options::default()).unwrap();
    let repeat = changes::between("second", Some("repeat"), &b, &repeated);
    let route = repeat.routes.iter().find(|r| r.id == "composition.run").unwrap();
    assert!(route.steps_added.is_empty() && route.steps_removed.is_empty(), "the same functions are reached");
    assert_eq!(route.compositions_changed.len(), 1, "multiplicity belongs to composition");
    let (g, d) = diff::union("composition", a, b);
    assert!(d.nodes.is_empty(), "all own arrows are unchanged: {:?}", d.nodes);
    assert!(d.edges.is_empty() && d.places.is_empty());
    let marks = &d.sites["composition.run"];
    for (excluded, expected) in [
        (Status::Added, vec!["composition.add_one", "composition.double"]),
        (Status::Removed, vec!["composition.double", "composition.add_one"]),
    ] {
        let side: Vec<_> = calls(&g).into_iter().enumerate().filter(|(i, _)| marks.get(i) != Some(&excluded)).map(|(_, to)| to).collect();
        assert_eq!(side, expected, "each diff fiber retains its execution order");
    }
    assert!(marks.values().any(|s| *s == Status::Added) && marks.values().any(|s| *s == Status::Removed));
    std::fs::remove_dir_all(base).unwrap();
}

/// Each reading of the union is its own chain of values. A step put in before another (`insert`)
/// or taken out (`delete`) is the only step marked: the step after it makes the same call on a
/// value of the same type, so it is one step, handed in each reading what that reading hands it.
#[test]
fn each_union_side_keeps_its_own_nested_value_chain() {
    let program = include_str!("composition/number.py");
    for (case, first, second, changed) in [
        ("reverse", "double(add_one(Number(3)))", "add_one(double(Number(3)))", None),
        ("insert", "double(Number(3))", "double(add_one(Number(3)))", Some(Status::Added)),
        ("delete", "double(add_one(Number(3)))", "double(Number(3))", Some(Status::Removed)),
    ] {
        let base = std::env::temp_dir().join(format!("atlas-composition-{case}-{}", std::process::id()));
        let repo = base.join("composition");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::write(
            repo.join("pyproject.toml"),
            "[project]\nname=\"composition\"\nversion=\"0.1.0\"\n[project.scripts]\ncomposition=\"composition:run\"\n",
        )
        .unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        for expression in [first, second] {
            std::fs::write(repo.join("composition.py"), program.replace("EXPRESSION", expression)).unwrap();
            git(&repo, &["add", "composition.py", "pyproject.toml"]);
            git(&repo, &["commit", "-q", "-m", "composition"]);
        }
        let a = changes::parts_at(&repo, Some("HEAD~1"), &Options::default()).unwrap();
        let b = changes::parts_at(&repo, Some("HEAD"), &Options::default()).unwrap();
        let (same, unchanged) = diff::union("composition", a.clone(), a.clone());
        assert!(unchanged.nodes.is_empty() && unchanged.sites.is_empty(), "equal readings share every occurrence");
        assert!(same
            .node("composition.run")
            .unwrap()
            .sites
            .iter()
            .flat_map(|s| s.hands.iter().chain(&s.gives))
            .all(|h| !h.name.starts_with("#a")));
        let (union, d) = diff::union("composition", a.clone(), b.clone());
        for id in ["composition.run", "composition.add_one", "composition.double"] {
            assert!(!d.nodes.contains_key(id), "{id}'s own arrow is unchanged");
        }
        let marks = &d.sites["composition.run"];
        assert_eq!(marks.get(&0), None, "the common Number constructor is one occurrence");
        if let Some(status) = changed {
            let run = union.node("composition.run").unwrap();
            assert_eq!(calls(&union), ["composition.Number", "composition.add_one", "composition.double"], "{case}: double is one step");
            assert_eq!(marks.iter().map(|(i, s)| (*i, *s)).collect::<Vec<_>>(), [(1, status)], "{case}: only add_one is marked");
            assert_eq!(d.hands["composition.run"].keys().collect::<Vec<_>>(), [&2], "{case}: A hands double another value than B");
            assert_eq!(run.sites[2].hands.len(), 1);
            let c = changes::between("HEAD~1", Some("HEAD"), &a, &b);
            let route = c.routes.iter().find(|r| r.id == "composition.run").expect("the route changed");
            let (added, removed) = (route.steps_added.as_slice(), route.steps_removed.as_slice());
            let listed = if status == Status::Added { (added, removed) } else { (removed, added) };
            assert_eq!(listed, (["composition.add_one".to_string()].as_slice(), [].as_slice()), "{case}: {}", changes::report(&c));
            assert!(route.steps_changed.is_empty(), "{case}: {}", changes::report(&c));
        }
        for (original, reading) in [(&a[0], diff::Reading::A), (&b[0], diff::Reading::B)] {
            let side = diff::reading(&union, &d, reading);
            let sites: Vec<_> = side.node("composition.run").unwrap().sites.iter().collect();
            assert_eq!(sites.iter().map(|s| s.to.as_str()).collect::<Vec<_>>(), calls(original));
            let mut produced = std::collections::HashSet::new();
            for s in &sites {
                for input in s.hands.iter().filter(|h| h.name.starts_with('#')) {
                    assert!(produced.contains(&input.name), "{case}: {} consumes a value absent from its side: {}", s.to, input.name);
                }
                produced.extend(s.gives.iter().map(|h| h.name.clone()));
            }
            for pair in sites.windows(2) {
                assert!(
                    pair[1].hands.iter().any(|h| pair[0].gives.iter().any(|g| h.name == g.name)),
                    "{case}: each call consumes its predecessor's result"
                );
            }
        }
        std::fs::remove_dir_all(base).unwrap();
    }
}

#[test]
fn the_clean_and_shortcut_pairs_have_an_empty_diff_against_themselves() {
    let pairs = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/pairs");
    for subject in [
        "storage/clean",
        "storage/shortcut",
        "pipeline/python-clean",
        "pipeline/python-sloppy",
        "pipeline/rust-clean",
        "pipeline/typescript",
        "python/clean",
        "python/shortcut",
    ] {
        let parts = atlas_engine::parts::analyze_parts(&[pairs.join(subject)], &Options::default()).unwrap();
        let (_, d) = diff::union(subject, parts.clone(), parts);
        assert!(
            d.nodes.is_empty() && d.edges.is_empty() && d.sites.is_empty() && d.touches.is_empty() && d.places.is_empty(),
            "{subject}: {d:?}"
        );
    }
}
