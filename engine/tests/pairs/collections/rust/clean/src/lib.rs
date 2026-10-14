//! Readings kept in a file, one per line, and read back as readings.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct Reading {
    pub text: String,
}

pub struct Row {
    pub reading: Reading,
}

fn write_rows(path: &Path, rows: &[Row]) -> io::Result<()> {
    let lines: Vec<&str> = rows.iter().map(|row| row.reading.text.as_str()).collect();
    fs::write(path, lines.join("\n"))
}

pub struct Store {
    path: PathBuf,
}

impl Store {
    pub fn new(path: PathBuf) -> Store {
        Store { path }
    }

    pub fn put(&self, reading: &Reading) -> io::Result<()> {
        let mut rows: Vec<Row> = self.rows()?;
        rows.push(Row { reading: reading.clone() });
        write_rows(&self.path, &rows)
    }

    pub fn put_all(&self, readings: &[Reading]) -> io::Result<()> {
        let mut rows: Vec<Row> = self.rows()?;
        for reading in readings {
            rows.push(Row { reading: reading.clone() });
        }
        write_rows(&self.path, &rows)
    }

    pub fn get(&self) -> io::Result<Vec<Reading>> {
        Ok(self.rows()?.into_iter().map(|row| row.reading).collect())
    }

    fn rows(&self) -> io::Result<Vec<Row>> {
        Ok(fs::read_to_string(&self.path)?.lines().map(|line| Row { reading: Reading { text: line.to_string() } }).collect())
    }
}
