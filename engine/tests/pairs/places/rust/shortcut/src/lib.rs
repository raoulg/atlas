//! Rows read and kept where whoever calls says; where nobody says, the code names its default. A
//! page is made from a template read, at run time, from the directory the crate was built in.

use std::env;
use std::fs;
use std::io;
use std::path::Path;

pub fn read_rows(path: &Path) -> io::Result<Vec<String>> {
    Ok(fs::read_to_string(path)?.lines().map(str::to_string).collect())
}

pub fn default_rows() -> io::Result<Vec<String>> {
    read_rows(Path::new("data/rows.csv"))
}

pub fn save_where_told(rows: &[String]) -> io::Result<()> {
    let path = env::var("STOCK_PATH").unwrap_or_else(|_| "out/rows.csv".to_string());
    fs::write(path, rows.join("\n"))
}

pub fn page(title: &str) -> io::Result<String> {
    let template = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("template.html"))?;
    Ok(template.replace("{title}", title))
}
