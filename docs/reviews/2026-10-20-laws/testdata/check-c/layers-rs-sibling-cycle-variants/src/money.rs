use crate::ledger::Book;

pub struct Coin {
    pub cents: u32,
}

pub fn worth(b: &Book) -> u32 {
    b.first.cents
}
