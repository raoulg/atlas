//! The settings of a project, in its own settings file or in its manifest, and its lock.

use std::path::Path;

/// The text of the file `name` in the project at `root`.
fn text(root: &Path, name: &str) -> Option<String> {
    std::fs::read_to_string(root.join(name)).ok()
}

/// The project's settings: what its `knot.toml` says, else its `Cargo.toml`.
pub fn settings(root: &Path) -> Option<String> {
    text(root, "knot.toml").or_else(|| text(root, "Cargo.toml"))
}

/// The versions the project is locked to.
pub fn lock(root: &Path) -> Option<String> {
    text(root, "knot.lock")
}
