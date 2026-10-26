use crate::ledger::{post, total, Book};

use super::Coin;

pub fn posted_book(n: u32) -> Book {
    Book { first: Coin { cents: post(n) } }
}

#[test]
fn a_posted_book_totals_what_was_posted() {
    assert_eq!(total(&posted_book(25)), 26);
}
