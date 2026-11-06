//! Levels measured in decibels, and the loudest of them, given to the caller.

/// One level, as it was measured.
pub struct Level {
    pub db: f64,
}

/// The loudest of the levels, in decibels.
pub fn loudest(levels: &[Level]) -> f64 {
    levels.iter().map(|level| level.db).fold(f64::MIN, f64::max)
}
