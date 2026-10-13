//! A report of the ledger in one currency, in the format its reader wants.

use std::env;
use std::fs;
use std::path::PathBuf;

use crate::rates;
use crate::store::Store;

/// An amount rounded to cents.
pub fn round2(amount: f64) -> f64 {
    (amount * 100.0).round() / 100.0
}

/// Writes the ledger as a report in `format` (`md` or `csv`), and gives back where.
pub fn write(store: &Store, format: &str, title: &str) -> PathBuf {
    let mut out = match format {
        "csv" => String::from("date,description,amount,in base\n"),
        _ => format!("# {title}\n\n| date | description | amount | in base |\n|---|---|---|---|\n"),
    };
    for (date, description, amount, currency) in store.rows() {
        let base = round2(amount * rates::to_base(&currency));
        if format == "csv" {
            out += &format!("{date},{description},{amount} {currency},{base}\n");
        } else {
            out += &format!("| {date} | {description} | {amount} {currency} | {base} |\n");
        }
    }
    let dir = PathBuf::from(env::var("SPEND_REPORTS").unwrap_or_else(|_| "reports".to_string()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("report.{format}"));
    fs::write(&path, out).unwrap();
    path
}
