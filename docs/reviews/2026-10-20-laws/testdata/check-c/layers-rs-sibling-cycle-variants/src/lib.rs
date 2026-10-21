pub mod ledger;
pub mod money;

pub fn audit(n: u32) -> u32 {
    ledger::post(n)
}

pub fn rate() -> u32 {
    7
}
