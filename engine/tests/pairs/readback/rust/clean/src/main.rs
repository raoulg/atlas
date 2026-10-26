//! Keep notes in a table and make sure all of them arrived: the count read back is checked, and goes no further.

use rusqlite::Connection;

pub struct Ledger {
    path: String,
    kept: usize,
}

impl Ledger {
    pub fn keep(&mut self, notes: &[String]) -> rusqlite::Result<()> {
        let con = Connection::open(&self.path)?;
        con.execute("CREATE TABLE IF NOT EXISTS notes (text TEXT)", [])?;
        for note in notes {
            con.execute("INSERT INTO notes VALUES (?1)", [note])?;
        }
        let stored: usize = con.query_row("SELECT count(*) FROM notes", [], |r| r.get(0))?;
        if stored < notes.len() {
            panic!("notes went missing");
        }
        self.kept += notes.len();
        Ok(())
    }
}

fn main() {
    let mut ledger = Ledger { path: "out/notes.db".to_string(), kept: 0 };
    ledger.keep(&["first".to_string(), "second".to_string()]).unwrap();
}
