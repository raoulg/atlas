//! Count the lines of a data file and write the count to a report.

use std::fs::File;
use std::io::{self, Read, Write};

/// The data file.
const DATA: &str = "data.txt";
/// The report its count is written to.
const REPORT: &str = "report.txt";

fn main() -> io::Result<()> {
    let mut data = File::open(DATA)?;
    let n = count(&mut data)?;
    let mut report = File::create(REPORT)?;
    emit(&mut report, n)
}

/// The lines of the file handed in.
fn count(f: &mut File) -> io::Result<usize> {
    let mut text = String::new();
    f.read_to_string(&mut text)?;
    Ok(text.lines().count())
}

/// Write the count to the file handed in.
fn emit(out: &mut File, n: usize) -> io::Result<()> {
    out.write_all(format!("{n}\n").as_bytes())
}
