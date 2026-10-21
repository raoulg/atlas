pub struct Book {
    pub first: crate::money::Coin,
}

pub fn total(b: &Book) -> u32 {
    b.first.cents
}
