//! A ledger that posts and saves, a shelf that saves and keeps, and what will replace them.

pub mod ledger;
pub mod shelf;

#[doc = "prophecy: supersedes=crate::ledger::post"]
pub fn post_v2(_n: u32) -> u32 {
    todo!()
}

#[doc = "prophecy: supersedes=archive::ledger::post"]
pub fn post_v3(_n: u32) -> u32 {
    todo!()
}

#[doc = "prophecy: supersedes=shelf::save"]
pub fn save_v2(_n: u32) {
    todo!()
}
