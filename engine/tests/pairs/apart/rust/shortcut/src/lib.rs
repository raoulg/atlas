//! A ledger of blocked rows, flushed to a file in the folder it is handed.

pub mod store;

pub use store::Ledger;
