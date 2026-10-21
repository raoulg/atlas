use std::fs;
use std::io;
use std::path::Path;

pub fn render(title: &str) -> io::Result<String> {
    let template = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("template.html"))?;
    Ok(template.replace("{title}", title))
}
