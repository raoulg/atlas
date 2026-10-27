use std::path::Path;

fn main() -> std::io::Result<()> {
    bench::save_report(Path::new("."), "all well")?;
    bench::save_log(Path::new("."), "ran")?;
    bench::clear(Path::new("."))
}
