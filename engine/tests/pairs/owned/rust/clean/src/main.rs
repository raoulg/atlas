fn main() -> std::io::Result<()> {
    notes::save_report("all well")?;
    notes::save_log("ran")
}
