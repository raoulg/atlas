//! The shelf on disk: one title on each line, and its tests in the same file.

pub fn load(path: &str) -> Vec<String> {
    std::fs::read_to_string(path).unwrap().lines().map(String::from).collect()
}

#[cfg(test)]
fn sample() -> String {
    String::from("fixtures/shelf.txt")
}

#[test]
fn test_loads() {
    assert!(!crate::load(&sample()).is_empty());
}
