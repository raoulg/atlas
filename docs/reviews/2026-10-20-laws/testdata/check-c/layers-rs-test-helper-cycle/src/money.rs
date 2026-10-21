pub fn round(n: u32) -> u32 {
    n / 10 * 10
}

#[cfg(test)]
mod tests {
    fn posted(n: u32) -> u32 {
        crate::ledger::post(n)
    }

    #[test]
    fn rounds_what_is_posted() {
        assert_eq!(posted(25), 21);
    }
}
