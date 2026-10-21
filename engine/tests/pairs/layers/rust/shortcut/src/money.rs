#[cfg(test)]
mod checks;

pub struct Coin {
    pub cents: u32,
}

pub fn round(n: u32) -> u32 {
    n / 10 * 10
}

pub fn worth(b: &crate::ledger::Book) -> Coin {
    Coin { cents: b.first.cents * 2 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn posted(n: u32) -> u32 {
        crate::ledger::post(n)
    }

    #[test]
    fn rounds_what_is_posted() {
        assert_eq!(round(posted(25)), 20);
    }
}
