//! How a report is kept: trimmed now, sealed once sealing is built.

#[cfg(test)]
mod checks;

pub fn run(report: &str) -> String {
    report.trim().to_string()
}

#[doc = "prophecy: supersedes=archive::run"]
pub fn seal(_report: &str) -> String {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sealed_twice(r: &str) -> String {
        seal(&seal(r))
    }

    #[test]
    fn sealing_twice_is_sealing_once() {
        assert_eq!(sealed_twice("r"), seal("r"));
    }
}
