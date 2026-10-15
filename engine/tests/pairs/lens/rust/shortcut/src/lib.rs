//! Readings kept in files, with the outages of a harvest and the summaries of a day; outages and
//! summaries read back as the lines they are kept as.

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

/// A station on a date: what a summary is of.
pub struct Day {
    pub station: String,
    pub date: String,
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

    pub fn outages(&self, station: &str) -> io::Result<Vec<String>> {
        let text = fs::read_to_string(&self.outages)?;
        Ok(text.lines().filter(|line| line.starts_with(station)).map(str::to_string).collect())
    }

    pub fn summary(&self, day: &Day) -> io::Result<String> {
        let text = fs::read_to_string(&self.summaries)?;
        let key = format!("{} {}", day.station, day.date);
        if let Some(line) = text.lines().find(|line| line.starts_with(&key)) {
            return Ok(line.to_string());
        }
        let temperatures: Vec<f64> = self.history(&day.station)?.iter().filter(|r| r.date == day.date).map(|r| r.temperature).collect();
        let low = temperatures.iter().copied().fold(f64::INFINITY, f64::min);
        let high = temperatures.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let line = format!("{key} {low} {high}");
        fs::write(&self.summaries, format!("{text}{line}\n"))?;
        Ok(line)
    }
}
