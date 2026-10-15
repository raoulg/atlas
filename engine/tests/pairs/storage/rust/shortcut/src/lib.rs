//! Rows kept in a SQLite file: the directory is made by a function of its own, apart from the
//! connection that opens the file in it.

use rusqlite::Connection;
use std::fs;
use std::path::Path;

const DATABASE: &str = "warehouse/rows.db";

pub fn prepare() {
    fs::create_dir_all(Path::new(DATABASE).parent().unwrap()).unwrap();
}

pub fn connect() -> rusqlite::Result<Connection> {
    Connection::open(DATABASE)
}

pub fn count(db: &Connection) -> rusqlite::Result<i64> {
    db.query_row("SELECT count(*) FROM rows", [], |r| r.get(0))
}
