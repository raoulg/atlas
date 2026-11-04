//! A shelf of recipes, one note per dish: the program shows one recipe and how many there are.

use std::fs;
use std::path::Path;

/// Where the shelf stands.
const SHELF: &str = "shelf";

/// The recipe for `dish`, from its note on `shelf`.
fn recipe(shelf: &Path, dish: &str) -> std::io::Result<String> {
    let mut note = shelf.to_path_buf();
    note.push("recipes");
    note.push(dish);
    note.set_extension("md");
    fs::read_to_string(note)
}

/// Every recipe on `shelf`.
fn recipes(shelf: &Path) -> std::io::Result<Vec<String>> {
    let mut out = vec![];
    for entry in Path::join(shelf, "recipes").read_dir()? {
        out.push(fs::read_to_string(entry?.path())?);
    }
    Ok(out)
}

fn main() -> std::io::Result<()> {
    let shelf = Path::new(SHELF);
    println!("{}", recipe(shelf, "bread")?);
    println!("{} recipes", recipes(shelf)?.len());
    Ok(())
}
