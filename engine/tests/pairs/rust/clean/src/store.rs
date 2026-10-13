//! The ledger: expenses and exchange rates in SQLite.

use std::collections::HashMap;
use std::path::Path;

use rusqlite::{params, Connection};

use crate::error::Error;
use crate::expense::{Expense, Money};
use crate::rates::Rates;

const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS expenses (date TEXT NOT NULL, description TEXT NOT NULL, cents INTEGER NOT NULL, currency TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS rates (base TEXT NOT NULL, date TEXT NOT NULL, currency TEXT NOT NULL, rate REAL NOT NULL);";
const INSERT_EXPENSE: &str = "INSERT INTO expenses (date, description, cents, currency) VALUES (?1, ?2, ?3, ?4)";
const SELECT_EXPENSES: &str = "SELECT date, description, cents, currency FROM expenses ORDER BY date";
const INSERT_RATE: &str = "INSERT INTO rates (base, date, currency, rate) VALUES (?1, ?2, ?3, ?4)";
const SELECT_RATES: &str = "SELECT date, currency, rate FROM rates WHERE base = ?1 AND date = (SELECT max(date) FROM rates WHERE base = ?1)";

/// The ledger, in the database it is opened at.
pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Store, Error> {
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        Ok(Store { conn })
    }

    pub fn save(&self, expenses: &[Expense]) -> Result<(), Error> {
        for e in expenses {
            self.conn.execute(INSERT_EXPENSE, params![e.date, e.description, e.amount.cents, e.amount.currency])?;
        }
        Ok(())
    }

    pub fn expenses(&self) -> Result<Vec<Expense>, Error> {
        let mut select = self.conn.prepare(SELECT_EXPENSES)?;
        let rows = select.query_map([], |row| {
            let amount = Money { cents: row.get(2)?, currency: row.get(3)? };
            Ok(Expense { date: row.get(0)?, description: row.get(1)?, amount })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn save_rates(&self, rates: &Rates) -> Result<(), Error> {
        for (currency, rate) in &rates.rates {
            self.conn.execute(INSERT_RATE, params![rates.base, rates.date, currency, rate])?;
        }
        Ok(())
    }

    /// The latest rates against `base`.
    pub fn rates(&self, base: &str) -> Result<Rates, Error> {
        let mut select = self.conn.prepare(SELECT_RATES)?;
        let rows = select.query_map([base], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, f64>(2)?)))?;
        let mut rates = Rates { base: base.to_string(), date: String::new(), rates: HashMap::new() };
        for row in rows {
            let (date, currency, rate) = row?;
            rates.date = date;
            rates.rates.insert(currency, rate);
        }
        if rates.rates.is_empty() {
            return Err(Error::NoRates(base.to_string()));
        }
        Ok(rates)
    }
}
