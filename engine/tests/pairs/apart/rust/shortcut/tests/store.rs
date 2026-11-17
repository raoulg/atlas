//! The ledger's test: a ledger of its own, in the folder the program keeps its data in, flushed
//! with a stamp.

use std::path::PathBuf;

use ledger::Ledger;

#[test]
fn test_flush() {
    let ledger = Ledger::new(PathBuf::from("data/blocked"));
    ledger.flush("20260823");
}
