//! Amounts kept in a file, each read back inside an Entry.

use std::fs;
use std::io;
use std::path::PathBuf;
use std::str::FromStr;

use rust_decimal::Decimal;

pub struct Entry {
    pub amount: Decimal,
}

/// Puts a Decimal and gets it back inside an Entry: a library's type as a field.
pub struct Ledger {
    path: PathBuf,
}

impl Ledger {
    pub fn new(path: PathBuf) -> Ledger {
        Ledger { path }
    }

    pub fn put(&self, amount: &Decimal) -> io::Result<()> {
        fs::write(&self.path, amount.to_string())
    }

    pub fn get(&self) -> io::Result<Entry> {
        let text = fs::read_to_string(&self.path)?;
        let amount = Decimal::from_str(text.trim()).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        Ok(Entry { amount })
    }
}
