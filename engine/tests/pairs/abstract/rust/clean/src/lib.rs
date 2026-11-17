//! A report written out in the format asked for, and kept in the place asked for.

use std::fs;
use std::path::PathBuf;

pub struct Report {
    pub title: String,
    pub rows: Vec<String>,
}

pub struct Receipt {
    pub path: PathBuf,
}

/// A way to write a report out.
pub trait Exporter {
    /// Writes the report out, and says where.
    fn export(&self, report: &Report) -> Receipt;
}

pub struct CsvExporter;

pub struct JsonExporter;

impl Exporter for CsvExporter {
    fn export(&self, report: &Report) -> Receipt {
        let path = PathBuf::from("out/report.csv");
        fs::write(&path, report.rows.join("\n")).expect("writes the csv");
        Receipt { path }
    }
}

impl Exporter for JsonExporter {
    fn export(&self, report: &Report) -> Receipt {
        let path = PathBuf::from("out/report.json");
        fs::write(&path, format!("{{\"title\": {:?}, \"rows\": {:?}}}", report.title, report.rows)).expect("writes the json");
        Receipt { path }
    }
}

/// A place a report is kept.
pub trait Store {
    fn keep(&self, report: &Report) -> Receipt;
}

pub struct Shelf;

pub struct Vault;

impl Store for Shelf {
    fn keep(&self, report: &Report) -> Receipt {
        let path = PathBuf::from("shelf/report.txt");
        fs::write(&path, &report.title).expect("writes the shelf");
        Receipt { path }
    }
}

impl Store for Vault {
    fn keep(&self, report: &Report) -> Receipt {
        let path = PathBuf::from("vault/report.txt");
        fs::write(&path, &report.title).expect("writes the vault");
        Receipt { path }
    }
}

fn exporter_for(format: &str) -> Box<dyn Exporter> {
    if format == "csv" {
        Box::new(CsvExporter)
    } else {
        Box::new(JsonExporter)
    }
}

fn store_for(place: &str) -> Box<dyn Store> {
    if place == "vault" {
        Box::new(Vault)
    } else {
        Box::new(Shelf)
    }
}

pub fn publish(report: &Report, format: &str) -> Receipt {
    exporter_for(format).export(report)
}

pub fn archive(report: &Report, place: &str) -> Receipt {
    store_for(place).keep(report)
}
