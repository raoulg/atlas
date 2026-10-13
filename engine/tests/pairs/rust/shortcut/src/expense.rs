//! An expense, and the CSV file expenses are imported from.

use std::fs;

/// One expense in the ledger.
pub struct Expense {
    pub date: String,
    pub description: String,
    pub amount: f64,
    pub currency: String,
}

impl Expense {
    /// Reads a line `date,description,amount`.
    pub fn parse(line: &str) -> Expense {
        let fields: Vec<&str> = line.splitn(3, ',').collect();
        let (amount, currency) = split_amount(fields[2]);
        Expense { date: fields[0].to_string(), description: fields[1].to_string(), amount: amount.parse().unwrap(), currency }
    }
}

/// Splits `12.50 EUR` into its amount and its currency.
pub fn split_amount(text: &str) -> (String, String) {
    let (amount, currency) = text.trim().split_once(' ').unwrap_or((text, "EUR"));
    (amount.to_string(), currency.to_string())
}

/// The expenses in a CSV file, after its header.
pub fn read_csv(path: &str) -> Vec<Expense> {
    let text = fs::read_to_string(path).unwrap();
    text.lines().skip(1).filter(|l| !l.trim().is_empty()).map(Expense::parse).collect()
}
