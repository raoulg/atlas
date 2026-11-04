//! Counts kept per kind, each in a database file of its own in the folder `data`, its path joined
//! part by part.

use rusqlite::Connection;
use std::path::Path;

pub fn counts(kind: &str) -> rusqlite::Result<Connection> {
    Connection::open(Path::new("data").join(kind).join("counts.db"))
}

pub fn both(kind: &str) -> rusqlite::Result<(Connection, Connection)> {
    Ok((Connection::open(Path::new("data").join(kind).join("counts.db"))?, Connection::open(Path::new("data").join("totals.db"))?))
}
