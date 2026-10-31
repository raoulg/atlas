//! A report and a log, each kept in the file the environment names: the program asks for each and
//! stops where it is not set.

use std::env;
use std::fs;
use std::io;

pub fn save_report(text: &str) -> io::Result<()> {
    let report = env::var("REPORT").expect("REPORT names the report");
    fs::write(report, text)
}

pub fn save_log(text: &str) -> io::Result<()> {
    let log = env::var("LOG").expect("LOG names the log");
    fs::write(log, text)
}
