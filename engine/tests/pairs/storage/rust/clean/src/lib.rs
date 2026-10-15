//! Rows kept in a SQLite file: opening the database makes the directory its file lives in.

use rusqlite::Connection;
use std::fs;
use std::path::Path;

const DATABASE: &str = "warehouse/rows.db";

pub fn connect() -> rusqlite::Result<Connection> {
    fs::create_dir_all(Path::new(DATABASE).parent().unwrap()).unwrap();
    Connection::open(DATABASE)
}

pub fn count(db: &Connection) -> rusqlite::Result<i64> {
    db.query_row("SELECT count(*) FROM rows", [], |r| r.get(0))
}
