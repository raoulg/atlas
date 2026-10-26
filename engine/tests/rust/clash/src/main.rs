//! A binary with a function of the crate's own name beside one of another.

use std::path::PathBuf;

fn clash() -> PathBuf {
    std::env::temp_dir().join("clash.txt")
}

fn other() -> PathBuf {
    std::env::temp_dir().join("other.txt")
}

fn main() {
    std::fs::write(clash(), "").unwrap();
    std::fs::write(other(), "").unwrap();
}
