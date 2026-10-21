//! Fill a cache where it is missing, then read it only to check it is not empty.

use std::fs;
use std::path::Path;

fn main() {
    if !Path::new("cache/data.txt").exists() {
        fs::write("cache/data.txt", "fresh").unwrap();
    }
    if fs::read_to_string("cache/data.txt").unwrap().is_empty() {
        panic!("empty cache");
    }
}
