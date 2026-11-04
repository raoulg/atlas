//! A report and a log, each kept in the file an environment variable names: `keep` asks for the
//! variable its caller names and stops where it is not set, and `show_report` reads the report back
//! from the file its own lookup names.

use std::env;
use std::fs;
use std::io;

fn setting(name: &str) -> String {
    env::var(name).expect("the variable names a file")
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
    fs::read_to_string(env::var("REPORT").expect("REPORT names the report"))
}
