//! The settings of a project, in its own settings file or in its manifest, and its lock.

use std::path::Path;

/// The project's settings: what its `knot.toml` says, else its `Cargo.toml`.
pub fn settings(root: &Path) -> Option<String> {
    let text = |name: &str| std::fs::read_to_string(root.join(name)).ok();
    text("knot.toml").or_else(|| text("Cargo.toml"))
}

/// The versions the project is locked to.
pub fn lock(root: &Path) -> Option<String> {
    let text = |name| std::fs::read_to_string(root.join(name)).ok();
    text("knot.lock")
}
