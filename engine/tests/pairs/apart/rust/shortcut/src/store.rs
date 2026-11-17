//! A ledger kept in the folder it is handed: each flush writes one file, stamped with what its
//! caller hands.

use std::fs;
use std::path::PathBuf;

/// The rows a run blocked, kept in one folder.
pub struct Ledger {
    folder: PathBuf,
    rows: Vec<String>,
}

impl Ledger {
    /// A ledger in the folder `folder`.
    pub fn new(folder: PathBuf) -> Ledger {
        Ledger { folder, rows: vec![] }
    }

    /// Writes the rows to a file of the folder, stamped `stamp`.
    pub fn flush(&self, stamp: &str) {
        fs::write(self.folder.join(format!("blocked_{stamp}.jsonl")), self.rows.join("\n")).unwrap();
    }
}
