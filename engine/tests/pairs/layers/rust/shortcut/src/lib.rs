//! A ledger of books in coins, and the stamp an audit puts on a sum.

pub mod ledger;
pub mod money;
pub mod stamp;

pub fn audit(n: u32) -> u32 {
    stamp::stamp(money::round(n))
}

pub fn rate() -> u32 {
    7
}
