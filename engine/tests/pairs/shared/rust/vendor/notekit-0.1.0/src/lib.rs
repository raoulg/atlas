use std::fs;

pub mod disk;

/// Saves `text` at `path`, and waits until it is on the disk.
pub fn save(path: &str, text: &str) {
    write(path, text);
    disk::sync(path);
}

/// Writes `text` to `path`.
pub fn write(path: &str, text: &str) {
    fs::write(path, text).unwrap();
}
