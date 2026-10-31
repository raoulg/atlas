//! Each kind of the shop's stock written to a folder of its own, in a file named after it.

use std::fs;
use std::path::Path;

const KINDS: [&str; 2] = ["books", "games"];

/// Write each kind into its folder below `root`: `books/books.csv`, `games/games.csv`.
fn export(root: &Path) -> std::io::Result<()> {
    for kind in KINDS {
        fs::create_dir_all(root.join(kind))?;
        fs::write(root.join(kind).join(format!("{kind}.csv")), kind)?;
    }
    Ok(())
}

/// Export the stock into `out`.
fn main() -> std::io::Result<()> {
    export(Path::new("out"))
}
