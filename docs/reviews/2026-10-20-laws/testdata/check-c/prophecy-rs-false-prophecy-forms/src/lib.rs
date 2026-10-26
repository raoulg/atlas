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

#[doc = "prophecy: supersedes=save"]
pub fn save_v2(_n: u32) {
    todo!()
}

#[doc = "prophecy: supersedes=std::fs::write"]
pub fn write_v2(_n: u32) {
    todo!()
}
