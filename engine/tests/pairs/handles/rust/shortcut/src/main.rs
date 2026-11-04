//! Count the lines of a data file and write the count to a report.

use std::fs::File;
use std::io::{self, Read, Write};

fn main() -> io::Result<()> {
    let n = count()?;
    emit(n)
}

/// The lines of the data file.
fn count() -> io::Result<usize> {
    let mut f = File::open("data.txt")?;
    let mut text = String::new();
    f.read_to_string(&mut text)?;
    Ok(text.lines().count())
}

/// Write the count to the report.
fn emit(n: usize) -> io::Result<()> {
    let mut out = File::create("report.txt")?;
    out.write_all(format!("{n}\n").as_bytes())
}
