//! The ledger's test: a ledger of its own, in the folder the system keeps for temporary files,
//! flushed with a stamp.

use ledger::Ledger;

#[test]
fn test_flush() {
    let ledger = Ledger::new(std::env::temp_dir());
    ledger.flush("20260823");
}
