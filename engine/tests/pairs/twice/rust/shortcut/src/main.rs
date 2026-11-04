//! Each kind of the shop's stock written to a folder of its own, in a file named after it.

use std::fs;
use std::path::Path;

const FOLDERS: [&str; 2] = ["books", "games"];
const NAMES: [&str; 2] = ["books.csv", "games.csv"];

/// Write each kind into its folder below `root`: `books/books.csv`, `games/games.csv`.
fn export(root: &Path) -> std::io::Result<()> {
    for i in 0..FOLDERS.len() {
        fs::create_dir_all(root.join(FOLDERS[i]))?;
        fs::write(root.join(FOLDERS[i]).join(NAMES[i]), FOLDERS[i])?;
    }
    Ok(())
}

/// Export the stock into `out`.
fn main() -> std::io::Result<()> {
    export(Path::new("out"))
}
