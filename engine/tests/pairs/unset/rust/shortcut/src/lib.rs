//! A report and a log, each kept in the file an environment variable names, or in the empty path
//! where it names none: `keep` falls back for the variable its caller names, and `show_report`
//! reads the report back from the file its own lookup names.

use std::env;
use std::fs;
use std::io;

fn setting(name: &str) -> String {
    env::var(name).unwrap_or(String::new())
}

fn keep(name: &str, text: &str) -> io::Result<()> {
    fs::write(setting(name), text)
}

pub fn save_report(text: &str) -> io::Result<()> {
    keep("REPORT", text)
}

pub fn save_log(text: &str) -> io::Result<()> {
    keep("LOG", text)
}

pub fn show_report() -> io::Result<String> {
    fs::read_to_string(env::var("REPORT").unwrap_or_default())
}
