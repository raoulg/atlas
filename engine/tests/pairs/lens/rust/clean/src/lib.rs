//! Readings kept in files, with the outages of a harvest and the summaries of a day.

use std::fs;
use std::io;
use std::path::PathBuf;

pub struct Reading {
    pub station: String,
    pub date: String,
    pub temperature: f64,
}

/// What the service said when it gave a station no reading.
pub struct Outage {
    pub station: String,
    pub reason: String,
}

/// What was kept of a station: its readings, and what the service said when it gave none.
pub struct Harvest {
    pub readings: Vec<Reading>,
    pub outages: Vec<Outage>,
}

/// A station on a date: what a summary is of.
pub struct Day {
    pub station: String,
    pub date: String,
}

/// The lowest and highest temperature a station had on a date.
pub struct Summary {
    pub station: String,
    pub date: String,
    pub low: f64,
    pub high: f64,
}

/// The summary of the readings taken on the day.
pub fn day_summary(day: &Day, readings: &[Reading]) -> Summary {
    let temperatures: Vec<f64> = readings.iter().filter(|r| r.date == day.date).map(|r| r.temperature).collect();
    let low = temperatures.iter().copied().fold(f64::INFINITY, f64::min);
    let high = temperatures.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    Summary { station: day.station.clone(), date: day.date.clone(), low, high }
}

/// Puts readings, outages and summaries into their files and gets them back.
pub struct ReadingStore {
    readings: PathBuf,
    outages: PathBuf,
    summaries: PathBuf,
}

impl ReadingStore {
    pub fn new(readings: PathBuf, outages: PathBuf, summaries: PathBuf) -> ReadingStore {
        ReadingStore { readings, outages, summaries }
    }

    pub fn save(&self, reading: &Reading) -> io::Result<()> {
        let mut text = fs::read_to_string(&self.readings)?;
        text.push_str(&format!("{} {} {}\n", reading.station, reading.date, reading.temperature));
        fs::write(&self.readings, text)
    }

    pub fn history(&self, station: &str) -> io::Result<Vec<Reading>> {
        let text = fs::read_to_string(&self.readings)?;
        Ok(text
            .lines()
            .filter_map(|line| {
                let mut parts = line.split(' ');
                let (s, date, t) = (parts.next()?, parts.next()?, parts.next()?);
                Some(Reading { station: s.to_string(), date: date.to_string(), temperature: t.parse().ok()? })
            })
            .filter(|r| r.station == station)
            .collect())
    }

    pub fn save_outage(&self, outage: &Outage) -> io::Result<()> {
        let mut text = fs::read_to_string(&self.outages)?;
        text.push_str(&format!("{} {}\n", outage.station, outage.reason));
        fs::write(&self.outages, text)
    }

    /// What was kept of a station: its readings, and what the service said when it gave none.
    pub fn kept(&self, station: &str) -> io::Result<Harvest> {
        let text = fs::read_to_string(&self.outages)?;
        let outages = text
            .lines()
            .filter_map(|line| line.split_once(' '))
            .filter(|(s, _)| *s == station)
            .map(|(s, reason)| Outage { station: s.to_string(), reason: reason.to_string() })
            .collect();
        Ok(Harvest { readings: self.history(station)?, outages })
    }

    /// The summary of a day: the one kept, else one made from the day's readings and kept.
    pub fn summary(&self, day: &Day) -> io::Result<Summary> {
        match self.kept_summary(day)? {
            Some(summary) => Ok(summary),
            None => self.keep_summary(day_summary(day, &self.history(&day.station)?)),
        }
    }

    pub fn kept_summary(&self, day: &Day) -> io::Result<Option<Summary>> {
        let text = fs::read_to_string(&self.summaries)?;
        Ok(text.lines().find_map(|line| {
            let mut parts = line.split(' ');
            let (s, date, low, high) = (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
            (s == day.station && date == day.date)
                .then(|| Some(Summary { station: s.to_string(), date: date.to_string(), low: low.parse().ok()?, high: high.parse().ok()? }))
                .flatten()
        }))
    }

    pub fn keep_summary(&self, summary: Summary) -> io::Result<Summary> {
        let mut text = fs::read_to_string(&self.summaries)?;
        text.push_str(&format!("{} {} {} {}\n", summary.station, summary.date, summary.low, summary.high));
        fs::write(&self.summaries, text)?;
        Ok(summary)
    }
}
