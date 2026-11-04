//! Counts kept per kind, each in a database file of its own in the folder `data`, its path glued
//! together as text.

use rusqlite::Connection;

pub fn counts(kind: &str) -> rusqlite::Result<Connection> {
    Connection::open(format!("data/{kind}/counts.db"))
}

pub fn both(kind: &str) -> rusqlite::Result<(Connection, Connection)> {
    Ok((Connection::open(format!("data/{kind}/counts.db"))?, Connection::open(format!("data/{}", "totals.db"))?))
}
