//! A ledger that posts and saves, a shelf that saves and keeps, and what will replace them.

pub mod ledger;
pub mod shelf;

#[doc = "prophecy: supersedes=save"]
pub fn save_v2(_n: u32) {
    todo!()
}

#[doc = "prophecy: supersedes=crate::ledger::store"]
pub fn store_v2(_n: u32) {
    todo!()
}
