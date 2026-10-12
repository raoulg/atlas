//! A library function that names its own file, beside a test that names its own; a trait whose
//! implementation spells the same promise another way.

use anyhow::Result;

use crate::world::Page;

pub fn ledger() -> std::io::Result<String> {
    std::fs::read_to_string("ledger.txt")
}

pub trait Source {
    fn page(&self) -> Result<Page>;
}

pub struct Disk;

impl Source for Disk {
    fn page(&self) -> anyhow::Result<Page> {
        Ok(Page { body: ledger()? })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_the_sample() {
        let text = std::fs::read_to_string("tests/sample.txt").unwrap();
        assert!(!text.is_empty());
    }
}
