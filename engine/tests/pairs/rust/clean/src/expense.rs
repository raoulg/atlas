//! An expense, and the CSV file expenses are imported from.

use std::fmt;
use std::fs;
use std::path::Path;

use crate::error::Error;

/// An amount in a currency, in cents.
#[derive(Clone, Debug, PartialEq)]
pub struct Money {
    pub cents: i64,
    pub currency: String,
}

impl Money {
    /// Reads `12.50 EUR`.
    pub fn parse(text: &str) -> Result<Money, Error> {
        let wrong = || Error::Amount(text.to_string());
        let (amount, currency) = text.trim().split_once(' ').ok_or_else(wrong)?;
        let value: f64 = amount.parse().map_err(|_| wrong())?;
        Ok(Money { cents: (value * 100.0).round() as i64, currency: currency.to_string() })
    }
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}.{:02} {}", self.cents / 100, self.cents.abs() % 100, self.currency)
    }
}

/// One expense in the ledger.
#[derive(Clone, Debug)]
pub struct Expense {
    pub date: String,
    pub description: String,
    pub amount: Money,
}

impl Expense {
    /// Reads a line `date,description,amount`.
    pub fn parse(line: &str) -> Result<Expense, Error> {
        let mut fields = line.splitn(3, ',');
        let (Some(date), Some(description), Some(amount)) = (fields.next(), fields.next(), fields.next()) else {
            return Err(Error::Line(line.to_string()));
        };
        let amount = Money::parse(amount)?;
        Ok(Expense { date: date.to_string(), description: description.to_string(), amount })
    }
}

/// The expenses in a CSV file, after its header.
pub fn read_csv(path: &Path) -> Result<Vec<Expense>, Error> {
    let text = fs::read_to_string(path)?;
    text.lines().skip(1).filter(|l| !l.trim().is_empty()).map(Expense::parse).collect()
}
