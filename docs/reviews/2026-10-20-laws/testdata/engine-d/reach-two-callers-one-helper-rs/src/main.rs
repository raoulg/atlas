//! Two steps hand one helper different files: each step writes only its own.

use std::fs;

fn write_to(name: &str) {
    fs::write(name, "x").unwrap();
}

fn alpha() {
    write_to("data/alpha.txt");
}

fn beta() {
    write_to("data/beta.txt");
}

fn main() {
    alpha();
    beta();
}
