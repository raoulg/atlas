use std::env;
use std::fs;
use std::io;

pub fn save(rows: &[String]) -> io::Result<()> {
    let path = env::var("STORE_PATH").unwrap_or_else(|_| "out.csv".to_string());
    fs::write(path, rows.join("\n"))
}
