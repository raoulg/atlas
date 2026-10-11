//! How deep Atlas reads into a library is a setting for the whole process, so these tests run
//! in a binary of their own.

use std::path::PathBuf;

use atlas_engine::{analyze, python};

/// One round reads the library modules the project's calls go into; a module of the library
/// that only those call is read in the second.
#[test]
fn each_round_reads_one_step_further_into_a_library() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixture-lib");
    let has = |depth: usize, id: &str| {
        python::read_libraries_to(depth);
        analyze(&repo, None, None).expect("fixture parses").nodes.iter().any(|n| n.id == id)
    };
    assert!(!has(0, "plotkit.data.load"), "no round reads no library");
    assert!(has(1, "plotkit.data.lines"), "the module the app calls is read");
    assert!(!has(1, "plotkit.rows.split"), "a module only the library calls waits for the next round");
    assert!(has(2, "plotkit.rows.split"));
}
