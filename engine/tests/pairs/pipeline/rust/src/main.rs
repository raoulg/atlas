use std::error::Error;
use std::fs;

use chrono::NaiveDate;
use plotters::prelude::*;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Record {
    year: i32,
    month: u32,
    decimal_date: f64,
    mean: f64,
    std: f64,
    observations: i32,
    definitive: u8,
}

fn download() -> Result<(), Box<dyn Error>> {
    let body = reqwest::blocking::get("https://www.sidc.be/SILSO/DATA/SN_m_tot_V2.0.csv")?.text()?;
    fs::create_dir_all("data/raw")?;
    fs::write("data/raw/sunspots.csv", body)?;
    Ok(())
}

fn load() -> Result<Vec<(NaiveDate, f64)>, Box<dyn Error>> {
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(b';')
        .has_headers(false)
        .trim(csv::Trim::All)
        .from_path("data/raw/sunspots.csv")?;
    let mut rows = Vec::new();
    for result in reader.deserialize() {
        let record: Record = result?;
        if record.mean == -1.0 {
            continue;
        }
        let date = NaiveDate::from_ymd_opt(record.year, record.month, 1).ok_or("bad date")?;
        rows.push((date, record.mean));
    }
    Ok(rows)
}

fn smooth(rows: &[(NaiveDate, f64)]) -> Vec<(NaiveDate, f64)> {
    rows.windows(13)
        .map(|w| (w[6].0, w.iter().map(|(_, mean)| mean).sum::<f64>() / 13.0))
        .collect()
}

fn plot(rows: &[(NaiveDate, f64)], smoothed: &[(NaiveDate, f64)]) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all("figures")?;
    let root = BitMapBackend::new("figures/sunspots.png", (1200, 500)).into_drawing_area();
    root.fill(&WHITE)?;
    let top = rows.iter().map(|(_, mean)| *mean).fold(0.0, f64::max);
    let mut chart = ChartBuilder::on(&root)
        .caption("Monthly mean total sunspot number", ("sans-serif", 24))
        .margin(10)
        .x_label_area_size(30)
        .y_label_area_size(40)
        .build_cartesian_2d(rows[0].0..rows[rows.len() - 1].0, 0.0..top)?;
    chart.configure_mesh().draw()?;
    chart.draw_series(LineSeries::new(rows.iter().copied(), &RGBColor(170, 170, 170)))?;
    chart.draw_series(LineSeries::new(smoothed.iter().copied(), &RED))?;
    root.present()?;
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    download()?;
    let rows = load()?;
    let smoothed = smooth(&rows);
    plot(&rows, &smoothed)?;
    Ok(())
}
