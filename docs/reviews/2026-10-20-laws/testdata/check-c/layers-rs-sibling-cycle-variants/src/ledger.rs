use crate::money::Coin;

pub struct Book {
    pub first: Coin,
}

pub fn post(n: u32) -> u32 {
    n + crate::rate()
}

pub fn open(c: Coin) -> Book {
    Book { first: c }
}
