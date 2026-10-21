// What the club keeps in files: a log of visits, its roster and its board.
import { readFileSync, writeFileSync } from "node:fs";

import { Member, Notice, Post, Visit } from "./model.js";

/** An end product: each visit is appended to the file; nothing gives them back. */
export class Log {
  constructor(private readonly path: string) {}

  add(visit: Visit): void {
    writeFileSync(this.path, readFileSync(this.path, "utf8") + `${visit.member} ${visit.day}\n`);
  }
}

/** The members, kept in a file, a name a line: `load` and `reread` give back a Roster that holds them. */
export class Roster {
  constructor(private readonly path: string, readonly members: Member[]) {}

  static load(path: string): Roster {
    return new Roster(path, readFileSync(path, "utf8").split("\n").map((name) => new Member(name)));
  }

  reread(): Roster {
    return new Roster(this.path, readFileSync(this.path, "utf8").split("\n").map((name) => new Member(name)));
  }

  add(who: Member): void {
    writeFileSync(this.path, [...this.members, who].map((m) => m.name).join("\n"));
  }
}

/** Pins a Notice and gives back the Posts: a Notice is a Post. */
export class Board {
  constructor(private readonly path: string) {}

  pin(notice: Notice): void {
    writeFileSync(this.path, JSON.stringify(notice));
  }

  posts(): Post[] {
    const kept = JSON.parse(readFileSync(this.path, "utf8")) as { text: string; until: string };
    return [new Notice(kept.text, kept.until)];
  }
}
