//! What can go wrong, by where it goes wrong.

use plotters::drawing::DrawingAreaErrorKind;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("reading or writing a file: {0}")]
    Io(#[from] std::io::Error),
    #[error("asking SILSO: {0}")]
    Request(#[from] reqwest::Error),
    #[error("reading SILSO's file: {0}")]
    Csv(#[from] csv::Error),
    #[error("SILSO's file has no month {month} in {year}")]
    Date { year: i32, month: u32 },
    #[error("no months to draw")]
    Empty,
    #[error("drawing the figure: {0}")]
    Plot(String),
}

/// What goes wrong on a drawing area, whatever backend it draws on.
impl<E: std::error::Error + Send + Sync> From<DrawingAreaErrorKind<E>> for Error {
    fn from(error: DrawingAreaErrorKind<E>) -> Error {
        Error::Plot(error.to_string())
    }
}
