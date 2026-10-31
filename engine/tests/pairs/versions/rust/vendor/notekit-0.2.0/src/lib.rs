use std::fs;

/// Writes `text` beside `path` and renames it into place, so that a reader never finds half of it.
pub fn save(path: &str, text: &str) {
    let partial = format!("{path}.part");
    fs::write(&partial, text).unwrap();
    fs::rename(&partial, path).unwrap();
}
