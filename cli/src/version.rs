//! Which build of Atlas this is.

/// The version and the commit this build is from: `0.1.0 (a5e777d)`, with `+` after a commit
/// whose tree had edits.
pub(crate) static VERSION: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    let version = match env!("ATLAS_CHANNEL") {
        "" => env!("CARGO_PKG_VERSION").to_string(),
        channel => format!("{}-{channel}", env!("CARGO_PKG_VERSION")),
    };
    match env!("ATLAS_COMMIT") {
        "" => version,
        c => format!("{version} ({c})"),
    }
});
