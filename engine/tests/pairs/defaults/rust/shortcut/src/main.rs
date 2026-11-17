use std::fs;
use std::io;
use std::path::Path;

/// A note kept straight in the workbench of `work`.
fn jot(text: &str) -> io::Result<()> {
    fs::write(downloads::made(Path::new("work"), "kept.txt"), text)
}

fn main() -> io::Result<()> {
    downloads::save_report("draft", None)?;
    downloads::save_report("final", Some(Path::new("out/report.txt")))?;
    downloads::fetch("rates", "{}")?;
    downloads::keep("noted", Path::new("work"), None)?;
    jot("noted")
}
