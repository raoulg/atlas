//! A report and a log, each kept where the one who runs the program says, both held by the same
//! name.

use std::fs;
use std::io;

pub fn save_report(text: &str) -> io::Result<()> {
    let Some(file) = std::env::args().nth(1) else { return Ok(()) };
    fs::write(file, text)
}

pub fn save_log(text: &str) -> io::Result<()> {
    let Some(file) = std::env::args().nth(2) else { return Ok(()) };
    fs::write(file, text)
}
