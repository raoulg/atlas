fn main() -> std::io::Result<()> {
    let folder = kept::notes_folder()?;
    let path = kept::note_file(&folder);
    kept::keep(&path, "{\"text\": \"hello\"}")?;
    println!("{}", kept::read(&path)?);
    println!("{}", kept::take(&path)?);
    Ok(())
}
