//! Sunspots: SILSO's monthly mean sunspot number, downloaded, smoothed and drawn.

mod download;
mod error;
mod plot;
mod series;
mod settings;

pub use download::download;
pub use error::Error;
pub use plot::{Plot, Trend};
pub use series::{load, smooth, Month, Smoothed};
pub use settings::Settings;
