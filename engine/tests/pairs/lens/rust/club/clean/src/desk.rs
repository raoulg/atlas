//! The club's desk, in SQLite: a table per thing it keeps.

use rusqlite::{params, Connection, Error};

use crate::{Member, Report, Visit};

/// Logs visits and lists the members' names: it only writes `visits` and only reads `members`.
pub struct Desk {
    conn: Connection,
}

impl Desk {
    pub fn new(conn: Connection) -> Desk {
        Desk { conn }
    }

    pub fn log(&self, visit: &Visit) -> Result<(), Error> {
        self.conn.execute("INSERT INTO visits (member, day) VALUES (?1, ?2)", params![visit.member, visit.day])?;
        Ok(())
    }

    pub fn members(&self) -> Result<Vec<String>, Error> {
        let mut select = self.conn.prepare("SELECT name FROM members")?;
        let rows = select.query_map([], |row| row.get(0))?;
        rows.collect()
    }
}

/// Puts visits and gets them back; `forget` names the rows it deletes by their Member.
pub struct Visits {
    conn: Connection,
}

impl Visits {
    pub fn new(conn: Connection) -> Visits {
        Visits { conn }
    }

    pub fn log(&self, visit: &Visit) -> Result<(), Error> {
        self.conn.execute("INSERT INTO visits (member, day) VALUES (?1, ?2)", params![visit.member, visit.day])?;
        Ok(())
    }

    pub fn visits(&self) -> Result<Vec<Visit>, Error> {
        let mut select = self.conn.prepare("SELECT member, day FROM visits")?;
        let rows = select.query_map([], |row| Ok(Visit { member: row.get(0)?, day: row.get(1)? }))?;
        rows.collect()
    }

    pub fn forget(&self, who: &Member) -> Result<(), Error> {
        self.conn.execute("DELETE FROM visits WHERE member = ?1", params![who.name])?;
        Ok(())
    }
}

/// Puts members and gets them back from `members`; gets reports, which name a Member, from `reports`.
pub struct Register {
    conn: Connection,
}

impl Register {
    pub fn new(conn: Connection) -> Register {
        Register { conn }
    }

    pub fn join(&self, who: &Member) -> Result<(), Error> {
        self.conn.execute("INSERT INTO members (name) VALUES (?1)", params![who.name])?;
        Ok(())
    }

    pub fn member(&self, name: &str) -> Result<Member, Error> {
        self.conn.query_row("SELECT name FROM members WHERE name = ?1", params![name], |row| Ok(Member { name: row.get(0)? }))
    }

    pub fn latest(&self) -> Result<Report, Error> {
        self.conn.query_row("SELECT title, author FROM reports", [], |row| {
            Ok(Report { title: row.get(0)?, author: Member { name: row.get(1)? } })
        })
    }
}
