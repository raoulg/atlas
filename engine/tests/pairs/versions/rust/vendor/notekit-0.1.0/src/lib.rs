use std::fs;

/// Writes `text` to `path`.
pub fn save(path: &str, text: &str) {
    fs::write(path, text).unwrap();
}
