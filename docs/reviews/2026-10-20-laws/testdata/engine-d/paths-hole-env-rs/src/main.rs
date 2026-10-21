//! The data directory comes from the environment: the code does not decide it, so it is a hole.

use std::fs;
use std::path::PathBuf;

fn main() {
    let root = PathBuf::from(std::env::var("DATA_DIR").unwrap());
    println!("{}", fs::read_to_string(root.join("sub").join("report.txt")).unwrap());
}
