//! The shelf on disk: one title on each line.

pub fn load(path: &str) -> Vec<String> {
    std::fs::read_to_string(path).unwrap().lines().map(String::from).collect()
}
