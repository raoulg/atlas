// The club's desk, in SQLite: a table per thing it keeps.
import Database from "better-sqlite3";

import { Member, Report, Visit } from "./model.js";

/** Logs visits and lists the members' names: it only writes `visits` and only reads `members`. */
export class Desk {
  constructor(private readonly db: Database.Database) {}

  log(visit: Visit): void {
    this.db.prepare("INSERT INTO visits (member, day) VALUES (?, ?)").run(visit.member, visit.day);
  }

  members(): string[] {
    return this.db.prepare("SELECT name FROM members").pluck().all() as string[];
  }
}

/** Puts visits and gets them back; `forget` names the rows it deletes by their Member. */
export class Visits {
  constructor(private readonly db: Database.Database) {}

  log(visit: Visit): void {
    this.db.prepare("INSERT INTO visits (member, day) VALUES (?, ?)").run(visit.member, visit.day);
  }

  visits(): Visit[] {
    const rows = this.db.prepare("SELECT member, day FROM visits").all() as { member: string; day: string }[];
    return rows.map((r) => new Visit(r.member, r.day));
  }

  forget(who: Member): void {
    this.db.prepare("DELETE FROM visits WHERE member = ?").run(who.name);
  }
}

/** Puts members and gets them back from `members`; gets reports, which name a Member, from `reports`. */
export class Register {
  constructor(private readonly db: Database.Database) {}

  join(who: Member): void {
    this.db.prepare("INSERT INTO members (name) VALUES (?)").run(who.name);
  }

  member(name: string): Member {
    const found = this.db.prepare("SELECT name FROM members WHERE name = ?").pluck().get(name) as string;
    return new Member(found);
  }

  latest(): Report {
    const row = this.db.prepare("SELECT title, author FROM reports").get() as { title: string; author: string };
    return new Report(row.title, new Member(row.author));
  }
}
