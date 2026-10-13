//! A report of the ledger in one currency, in the format its reader wants.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::Error;
use crate::expense::{Expense, Money};
use crate::rates::Rates;

/// An expense with its amount in the base currency.
pub struct Line {
    pub expense: Expense,
    pub base: Money,
}

/// A way to write a report down.
pub trait Render {
    /// The extension of a file in this format.
    fn extension(&self) -> &'static str;
    fn render(&self, title: &str, lines: &[Line]) -> String;
}

pub struct Markdown;

pub struct Csv;

impl Render for Markdown {
    fn extension(&self) -> &'static str {
        "md"
    }

    fn render(&self, title: &str, lines: &[Line]) -> String {
        let mut out = format!("# {title}\n\n| date | description | amount | in base |\n|---|---|---|---|\n");
        for l in lines {
            out += &format!("| {} | {} | {} | {} |\n", l.expense.date, l.expense.description, l.expense.amount, l.base);
        }
        out
    }
}

impl Render for Csv {
    fn extension(&self) -> &'static str {
        "csv"
    }

    fn render(&self, _title: &str, lines: &[Line]) -> String {
        let mut out = String::from("date,description,amount,in base\n");
        for l in lines {
            out += &format!("{},{},{},{}\n", l.expense.date, l.expense.description, l.expense.amount, l.base);
        }
        out
    }
}

/// Each expense with its amount in the base currency of `rates`.
pub fn lines(expenses: &[Expense], rates: &Rates) -> Result<Vec<Line>, Error> {
    let mut out = vec![];
    for e in expenses {
        let rate = rates.to_base(&e.amount.currency).ok_or_else(|| Error::NoRate(e.amount.currency.clone()))?;
        let base = Money { cents: (e.amount.cents as f64 * rate).round() as i64, currency: rates.base.clone() };
        out.push(Line { expense: e.clone(), base });
    }
    Ok(out)
}

/// Writes the report into `dir`, named after its format.
pub fn write(dir: &Path, render: &dyn Render, title: &str, lines: &[Line]) -> Result<PathBuf, Error> {
    fs::create_dir_all(dir)?;
    let path = dir.join(format!("report.{}", render.extension()));
    fs::write(&path, render.render(title, lines))?;
    Ok(path)
}
