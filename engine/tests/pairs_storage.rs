//! A file join with caller-named paths and the same operation with hidden environment paths.
//! The viewer can refine symbolic places without inventing concrete file names. Beside it, a graph
//! read and changed through transactions that say which way, and through bare queries that do not.

use atlas_engine::{analyze, flow, Options};
use std::collections::BTreeSet;
use std::path::PathBuf;

#[test]
fn storage_places_keep_the_evidence_for_their_quotient() {
    for side in ["clean", "shortcut"] {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/storage").join(side);
        let graph = analyze(&repo, &Options::default()).unwrap();
        let joining = graph.node("storage.join").unwrap();
        let reading = graph.node("storage.read_pair").unwrap();
        let drawn = flow::of(&graph);
        let reads: Vec<_> = reading.touches.iter().zip(&drawn.drawn[&reading.id]).filter(|(t, _)| t.kind_name() == "disk_read").collect();
        let writes: Vec<_> = joining.touches.iter().zip(&drawn.drawn[&joining.id]).filter(|(t, _)| t.kind_name() == "disk_write").collect();
        assert_eq!(reads.len(), 2, "{side}: {reads:?}");
        assert_eq!(writes.len(), 1, "{side}: {writes:?}");
        assert_eq!(reads.iter().map(|(_, p)| &p.place.id).collect::<BTreeSet<_>>().len(), 2);
        for (touch, drawn) in reads.into_iter().chain(writes) {
            if side == "clean" {
                assert!(!touch.artifact.is_empty(), "{touch:?}");
                assert!(drawn.place.coarse.is_none());
                assert!(drawn.place.id.starts_with("w:File:"));
            } else {
                assert!(touch.artifact.is_empty());
                assert!(!touch.what.is_empty());
                assert_eq!(drawn.place.group, "w:Disk");
                assert!(drawn.place.coarse.is_some());
            }
        }
    }
}

/// `overwrite(path)` reads the file its caller names and writes it back: one place, read and
/// written. A vertex of the finest picture is a place at a moment, the place as the step finds it
/// and as it leaves it, so the read and the write are two vertices of one place. The fold of the
/// disk is its quotient by directory met with the side ("files fold into the directories that hold
/// them, one box per side their files stand on", mathematics.md): a path no directory names folds
/// into the disk itself, once on each side.
#[test]
fn reading_and_writing_a_symbolic_path_touch_the_same_place() {
    for side in ["clean", "shortcut"] {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/storage").join(side);
        let graph = analyze(&repo, &Options::default()).unwrap();
        let flow = flow::of(&graph);
        let places = &flow.drawn["storage.overwrite"];
        let reads: Vec<_> = places.iter().filter(|p| p.side == flow::Side::In).collect();
        let writes: Vec<_> = places.iter().filter(|p| p.side == flow::Side::Out).collect();
        assert_eq!(reads.len(), 1);
        assert_eq!(writes.len(), 1);
        assert_eq!(reads[0].place.id, writes[0].place.id, "one place");
        for drawn in [reads[0], writes[0]] {
            assert_eq!(drawn.place.coarse.as_ref().unwrap().id, drawn.side.disk().id, "the fold of the disk met with the side");
        }
    }
}

/// A graph database goes the way the code says: a managed transaction reads (`execute_read`) or
/// writes (`execute_write`). A query run on a bare session says neither, and Atlas does not guess
/// it from the query's text: the way is open, and the picture draws the place on neither side.
#[test]
fn a_graph_call_goes_the_way_the_code_says() {
    for (side, reading, renaming) in [("clean", flow::Side::In, flow::Side::Out), ("shortcut", flow::Side::Open, flow::Side::Open)] {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/storage").join(side);
        let drawn = flow::of(&analyze(&repo, &Options::default()).unwrap());
        for (f, way) in [("storage.names", reading), ("storage.rename", renaming)] {
            let ways: Vec<flow::Side> = drawn.drawn[f].iter().filter(|w| w.place.id == "w:GraphDB").map(|w| w.side).collect();
            assert!(!ways.is_empty() && ways.iter().all(|w| *w == way), "{side} {f}: {ways:?}");
        }
    }
}

/// A connection opens the file a database is, and the directory made for that file is the
/// database's: made where the connection opens it, it is no place of its own. Made by a function
/// of its own, apart from the connection, it stands as a directory nothing in the picture uses.
/// Python's `duckdb.connect(path)` and Rust's `Connection::open(path)` are the same open.
#[test]
fn a_directory_made_for_a_database_file_belongs_to_the_database() {
    for (language, connect, file) in
        [("python", "_root.warehouse.connect", "warehouse/rows.duckdb"), ("rust", "warehouse.connect", "warehouse/rows.db")]
    {
        for (side, folded) in [("clean", true), ("shortcut", false)] {
            let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/storage");
            let repo = if language == "rust" { base.join("rust").join(side) } else { base.join(side) };
            let graph = analyze(&repo, &Options::default()).unwrap();
            let opening = graph.node(connect).unwrap();
            let named: Vec<_> = opening.touches.iter().map(|t| t.file.as_str()).filter(|f| !f.is_empty()).collect();
            assert_eq!(named, [file], "{language} {side}: the connection names its file");
            let made: Vec<_> = graph
                .nodes
                .iter()
                .flat_map(|n| &n.touches)
                .filter(|t| t.kind_name().starts_with("disk") && t.artifact == "warehouse/")
                .collect();
            assert_eq!(made.is_empty(), folded, "{language} {side}: {made:?}");
        }
    }
}
