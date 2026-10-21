use crate::money::Coin;

pub struct Book {
    pub first: Coin,
}

pub fn post(n: u32) -> u32 {
    n + 1
}

pub fn total(b: &Book) -> u32 {
    b.first.cents
}
