//! Keep notes in a table, then read them back to carry on with them: the table is where one step leaves them for the next.

use rusqlite::Connection;

pub struct Ledger {
    path: String,
    kept: usize,
}

impl Ledger {
    pub fn keep(&mut self, notes: &[String]) -> rusqlite::Result<Vec<String>> {
        let con = Connection::open(&self.path)?;
        con.execute("CREATE TABLE IF NOT EXISTS notes (text TEXT)", [])?;
        for note in notes {
            con.execute("INSERT INTO notes VALUES (?1)", [note])?;
        }
        let stored: Vec<String> = con.prepare("SELECT text FROM notes")?.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
        self.kept += notes.len();
        Ok(stored)
    }
}

fn main() {
    let mut ledger = Ledger { path: "out/notes.db".to_string(), kept: 0 };
    println!("{:?}", ledger.keep(&["first".to_string(), "second".to_string()]).unwrap());
}
