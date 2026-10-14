//! Readings kept in a file, one per line; read back as the lines.

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

    pub fn get(&self) -> io::Result<Vec<String>> {
        Ok(fs::read_to_string(&self.path)?.lines().map(str::to_string).collect())
    }

    fn rows(&self) -> io::Result<Vec<Row>> {
        Ok(fs::read_to_string(&self.path)?.lines().map(|line| Row { reading: Reading { text: line.to_string() } }).collect())
    }
}

pub struct DroppedStore {
    path: PathBuf,
}

impl DroppedStore {
    pub fn new(path: PathBuf) -> DroppedStore {
        DroppedStore { path }
    }

    pub fn put(&self, reading: &Reading) -> io::Result<()> {
        let rows: Vec<Row> = self.rows()?;
        let mut added: Vec<Row> = Vec::new();
        added.push(Row { reading: reading.clone() });
        write_rows(&self.path, &rows)
    }

    pub fn put_all(&self, readings: &[Reading]) -> io::Result<()> {
        let rows: Vec<Row> = self.rows()?;
        let mut added: Vec<Row> = Vec::new();
        for reading in readings {
            added.push(Row { reading: reading.clone() });
        }
        write_rows(&self.path, &rows)
    }

    pub fn get(&self) -> io::Result<Vec<String>> {
        Ok(fs::read_to_string(&self.path)?.lines().map(str::to_string).collect())
    }

    fn rows(&self) -> io::Result<Vec<Row>> {
        Ok(fs::read_to_string(&self.path)?.lines().map(|line| Row { reading: Reading { text: line.to_string() } }).collect())
    }
}

pub struct Frozen;

impl Frozen {
    pub fn push(&self, _row: Row) -> Frozen {
        Frozen
    }
}

pub fn observe(path: &Path, frozen: &Frozen, row: Row) {
    path.exists();
    frozen.push(row);
}
