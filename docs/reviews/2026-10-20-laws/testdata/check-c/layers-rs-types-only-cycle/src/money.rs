pub struct Coin {
    pub cents: u32,
}

pub fn worth(b: &crate::ledger::Book) -> Coin {
    Coin { cents: b.first.cents * 2 }
}
