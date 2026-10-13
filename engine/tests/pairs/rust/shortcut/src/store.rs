//! The ledger: expenses and exchange rates in SQLite.

use std::collections::HashMap;
use std::env;

use rusqlite::{params, Connection};
use serde_json::Value;

use crate::expense::Expense;
use crate::report::round2;

/// The ledger, in the database it is opened at.
pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open() -> Store {
        let path = env::var("SPEND_DB").unwrap_or_else(|_| "spend.db".to_string());
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS expenses (date TEXT, description TEXT, amount REAL, currency TEXT);
CREATE TABLE IF NOT EXISTS rates (base TEXT, date TEXT, currency TEXT, rate REAL);",
        )
        .unwrap();
        Store { conn }
    }

    pub fn save(&self, expenses: &[Expense]) {
        for e in expenses {
            self.conn.execute("INSERT INTO expenses VALUES (?1, ?2, ?3, ?4)", params![e.date, e.description, round2(e.amount), e.currency]).unwrap();
        }
    }

    /// Every expense, as (date, description, amount, currency).
    pub fn rows(&self) -> Vec<(String, String, f64, String)> {
        let mut select = self.conn.prepare("SELECT date, description, amount, currency FROM expenses ORDER BY date").unwrap();
        select.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).unwrap().map(Result::unwrap).collect()
    }

    pub fn save_rates(&self, base: &str, fetched: Result<Value, reqwest::Error>) {
        match fetched {
            Ok(body) => {
                let date = body["date"].as_str().unwrap_or_default();
                for (currency, rate) in body["rates"].as_object().into_iter().flatten() {
                    self.conn.execute("INSERT INTO rates VALUES (?1, ?2, ?3, ?4)", params![base, date, currency, rate.as_f64()]).unwrap();
                }
            }
            Err(e) => eprintln!("could not fetch the rates: {e}"),
        }
    }

    /// The latest rates against `base`, by currency.
    pub fn rates(&self, base: &str) -> HashMap<String, f64> {
        let mut select = self.conn.prepare("SELECT currency, rate FROM rates WHERE base = ?1 AND date = (SELECT max(date) FROM rates WHERE base = ?1)").unwrap();
        select.query_map([base], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect()
    }
}
