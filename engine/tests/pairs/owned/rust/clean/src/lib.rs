//! A report and a log, each kept where the one who runs the program says: two files the code does
//! not decide, each held by a name that says which.

use std::fs;
use std::io;

pub fn save_report(text: &str) -> io::Result<()> {
    let Some(report) = std::env::args().nth(1) else { return Ok(()) };
    fs::write(report, text)
}

pub fn save_log(text: &str) -> io::Result<()> {
    let Some(log) = std::env::args().nth(2) else { return Ok(()) };
    fs::write(log, text)
}
