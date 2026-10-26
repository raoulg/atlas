//! A type written through an alias is the type it stands for (#216). The clean shop writes `ship(order: Order)`; the
//! shortcut imports the type under a name of its own (`Order as Parcel`, `use crate::model::Order as Parcel`,
//! `import { type Order as Parcel }`) and writes `ship(order: Parcel)`. Isomorphic is the same: the two are one program,
//! and Atlas reads them as one. What the parameter holds is the type by its own name, as the type it takes; the flow,
//! the findings `check` asks and the finest plan the viewer builds of every route (`plans.mjs`) are the same on both
//! sides.

use std::path::PathBuf;
use std::process::Command;

use atlas_engine::principles::check;
use atlas_engine::{analyze, flow, EdgeKind, Graph, Options};

fn side(language: &str, name: &str) -> Graph {
    analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/aliases").join(language).join(name), &Options::default()).unwrap()
}

/// The facts of the finest plan of every route of `g`, as the viewer's model builds them in node.
fn plans(g: &Graph, name: &str) -> serde_json::Value {
    let mut data = serde_json::to_value(g).unwrap();
    data["flow"] = serde_json::to_value(flow::of(g)).unwrap();
    let file = std::env::temp_dir().join(format!("atlas-aliases-{}-{name}.json", std::process::id()));
    std::fs::write(&file, data.to_string()).unwrap();
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out = Command::new("node")
        .arg(here.join("tests/plans.mjs"))
        .arg(here.join("../viewer/plan.js"))
        .arg(&file)
        .output()
        .expect("node runs the viewer's model");
    let _ = std::fs::remove_file(&file);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let mut read: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    for (_, route) in read.as_object_mut().unwrap() {
        route.as_object_mut().unwrap().remove("faults");
    }
    read
}

#[test]
fn a_type_written_through_an_alias_is_the_type_it_stands_for() {
    for language in ["python", "rust", "typescript"] {
        let (clean, shortcut) = (side(language, "clean"), side(language, "shortcut"));
        let ship = |g: &Graph| g.node("shop.post.ship").unwrap_or_else(|| panic!("{language}: ship is read")).clone();
        let holds = |g: &Graph| ship(g).params.iter().map(|p| (p.name.clone(), p.holds.clone())).collect::<Vec<_>>();
        assert_eq!(holds(&clean), [("order".to_string(), vec!["Order".to_string()])], "{language}");
        assert_eq!(holds(&shortcut), holds(&clean), "{language}: the parameter holds the type the alias stands for");
        let takes = |g: &Graph| {
            g.edges.iter().filter(|e| e.src == "shop.post.ship" && e.kind == EdgeKind::Takes).map(|e| e.dst.clone()).collect::<Vec<_>>()
        };
        assert_eq!(takes(&shortcut), takes(&clean), "{language}");
        let drawn = |g: &Graph| serde_json::to_value(flow::of(g)).unwrap();
        assert_eq!(drawn(&shortcut), drawn(&clean), "{language}: the flow is the same");
        let asked = |g: &Graph| check(g).into_iter().map(|f| (f.principle, f.node)).collect::<Vec<_>>();
        assert_eq!(asked(&shortcut), asked(&clean), "{language}: and so are the findings");
        assert_eq!(
            plans(&shortcut, &format!("{language}-shortcut")),
            plans(&clean, &format!("{language}-clean")),
            "{language}: and the plans"
        );
    }
}
