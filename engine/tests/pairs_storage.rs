//! A file join with caller-named paths and the same operation with hidden environment paths.
//! The viewer can refine symbolic places without inventing concrete file names.

use atlas_engine::{analyze, flow, Options};
use std::collections::BTreeSet;
use std::path::PathBuf;

#[test]
fn storage_places_keep_the_evidence_for_their_quotient() {
    for side in ["clean", "shortcut"] {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs/storage").join(side);
        let graph = analyze(&repo, &Options::default()).unwrap();
        let joining = graph.node("storage.join").unwrap();
        let drawn = flow::of(&graph);
        let places = &drawn.drawn[&joining.id];
        let reads: Vec<_> = joining.touches.iter().zip(places).filter(|(t, _)| t.kind_name() == "disk_read").collect();
        let writes: Vec<_> = joining.touches.iter().zip(places).filter(|(t, _)| t.kind_name() == "disk_write").collect();
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
