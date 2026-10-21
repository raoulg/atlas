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
    fn seal_keeps_the_report() {
        assert_eq!(seal("r"), "r");
        assert_eq!(sealed_twice("r"), "r");
    }
}
