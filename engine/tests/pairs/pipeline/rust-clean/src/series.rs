//! The monthly mean sunspot number, a month at a time.
//!
//! SILSO's file has no header; each line holds seven numbers separated by semicolons and padded
//! with spaces (year, month, decimal date, mean, standard deviation, observations, definitive),
//! and a month without an observation has `-1` for its mean.

use std::num::NonZeroUsize;
use std::path::Path;

use chrono::NaiveDate;
use csv::{ReaderBuilder, Trim};
use serde::Deserialize;

use crate::error::Error;

/// The mean of a month without an observation.
const MISSING: f64 = -1.0;

/// A month of SILSO's series that has a mean.
#[derive(Clone, Debug, PartialEq)]
pub struct Month {
    /// The first day of the month.
    pub date: NaiveDate,
    pub mean: f64,
    /// The standard deviation of the stations' daily numbers.
    pub std: f64,
    pub observations: i32,
}

/// The mean sunspot number over the months centred on `date`.
#[derive(Clone, Debug, PartialEq)]
pub struct Smoothed {
    pub date: NaiveDate,
    pub mean: f64,
}

/// A line of SILSO's file, column by column.
#[derive(Deserialize)]
struct Record {
    year: i32,
    month: u32,
    _decimal_date: f64,
    mean: f64,
    std: f64,
    observations: i32,
    _definitive: u8,
}

impl Record {
    fn into_month(self) -> Result<Month, Error> {
        let date = NaiveDate::from_ymd_opt(self.year, self.month, 1).ok_or(Error::Date { year: self.year, month: self.month })?;
        Ok(Month { date, mean: self.mean, std: self.std, observations: self.observations })
    }
}

/// The months in SILSO's file at `path` that have a mean, in order.
pub fn load(path: &Path) -> Result<Vec<Month>, Error> {
    let mut reader = ReaderBuilder::new().delimiter(b';').has_headers(false).trim(Trim::All).from_path(path)?;
    let mut series = Vec::new();
    for record in reader.deserialize() {
        let record: Record = record?;
        if record.mean != MISSING {
            series.push(record.into_month()?);
        }
    }
    Ok(series)
}

/// The mean over each run of `months` consecutive months, dated by the month at its centre.
pub fn smooth(series: &[Month], months: NonZeroUsize) -> Vec<Smoothed> {
    let months = months.get();
    series
        .windows(months)
        .map(|run| Smoothed { date: run[months / 2].date, mean: run.iter().map(|month| month.mean).sum::<f64>() / months as f64 })
        .collect()
}
