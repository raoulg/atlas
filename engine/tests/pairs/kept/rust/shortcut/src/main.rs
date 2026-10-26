fn main() -> std::io::Result<()> {
    let path = kept::note_file(&kept::notes_folder()?);
    kept::keep(&path, "{\"text\": \"hello\"}")?;
    println!("{}", kept::read(&path)?);
    println!("{}", kept::take(&path)?);
    Ok(())
}
