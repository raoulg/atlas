//! The choices of a run that are not places: how far to smooth and how the figure looks.

use std::num::NonZeroUsize;

#[derive(Clone, Debug)]
pub struct Settings {
    /// How many months the smoothed mean is taken over, centred on each.
    pub months: NonZeroUsize,
    pub title: String,
    /// The figure's size, in pixels.
    pub size: (u32, u32),
}

impl Default for Settings {
    fn default() -> Settings {
        Settings { months: NonZeroUsize::new(13).expect("13 is not zero"), title: "Monthly mean total sunspot number".to_string(), size: (1500, 600) }
    }
}
