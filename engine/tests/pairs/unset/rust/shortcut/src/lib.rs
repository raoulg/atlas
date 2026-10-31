//! A report and a log, each kept in the file the environment names, or in the empty path where it
//! names none.

use std::env;
use std::fs;
use std::io;

pub fn save_report(text: &str) -> io::Result<()> {
    let report = env::var("REPORT").unwrap_or_default();
    fs::write(report, text)
}

pub fn save_log(text: &str) -> io::Result<()> {
    let log = env::var("LOG").unwrap_or_default();
    fs::write(log, text)
}
