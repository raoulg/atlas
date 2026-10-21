//! Rows read and kept where whoever calls says; where nobody says, the code names its default. A
//! page is made from a template compiled into the program.

use std::env;
use std::fs;
use std::io;
use std::path::Path;

pub fn read_rows(path: &Path) -> io::Result<Vec<String>> {
    Ok(fs::read_to_string(path)?.lines().map(str::to_string).collect())
}

pub fn save_where_told(rows: &[String]) -> io::Result<()> {
    let path = env::var("STOCK_PATH").unwrap_or_else(|_| "out/rows.csv".to_string());
    fs::write(path, rows.join("\n"))
}

pub fn page(title: &str) -> String {
    include_str!("template.html").replace("{title}", title)
}
