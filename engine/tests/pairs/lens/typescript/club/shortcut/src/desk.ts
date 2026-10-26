// The club's desk, in SQLite: a table per thing it keeps.
import Database from "better-sqlite3";

import { Member, Report, Visit } from "./model.js";

/** Logs visits and reads them back as the rows they are kept as. */
export class Desk {
  constructor(private readonly db: Database.Database) {}

  log(visit: Visit): void {
    this.db.prepare("INSERT INTO visits (member, day) VALUES (?, ?)").run(visit.member, visit.day);
  }

  visits(): Record<string, unknown>[] {
    return this.db.prepare("SELECT member, day FROM visits").all() as Record<string, unknown>[];
  }
}

/** Puts visits and gets them back; `come` puts a Member's name into a visit. */
export class Visits {
  constructor(private readonly db: Database.Database) {}

  log(visit: Visit): void {
    this.db.prepare("INSERT INTO visits (member, day) VALUES (?, ?)").run(visit.member, visit.day);
  }

  visits(): Visit[] {
    const rows = this.db.prepare("SELECT member, day FROM visits").all() as { member: string; day: string }[];
    return rows.map((r) => new Visit(r.member, r.day));
  }

  come(who: Member, day: string): void {
    this.db.prepare("INSERT INTO visits (member, day) VALUES (?, ?)").run(who.name, day);
  }
}

/** Puts members into `members` and reads back their names; a Member comes back only inside a Report, from `reports`. */
export class Register {
  constructor(private readonly db: Database.Database) {}

  join(who: Member): void {
    this.db.prepare("INSERT INTO members (name) VALUES (?)").run(who.name);
  }

  names(): string[] {
    return this.db.prepare("SELECT name FROM members").pluck().all() as string[];
  }

  latest(): Report {
    const row = this.db.prepare("SELECT title, author FROM reports").get() as { title: string; author: string };
    return new Report(row.title, new Member(row.author));
  }
}
