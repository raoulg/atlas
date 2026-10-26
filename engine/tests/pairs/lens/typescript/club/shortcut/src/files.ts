// What the club keeps in files: a log of visits, its roster and its board.
import { readFileSync, writeFileSync } from "node:fs";

import { Member, Notice, Visit } from "./model.js";

/** Appends each visit to the file and gives back the lines it is kept as. */
export class Log {
  constructor(private readonly path: string) {}

  add(visit: Visit): void {
    writeFileSync(this.path, readFileSync(this.path, "utf8") + `${visit.member} ${visit.day}\n`);
  }

  lines(): string[] {
    return readFileSync(this.path, "utf8").split("\n");
  }
}

/** The members, kept in a file, a name a line: `load` gives back a Roster that holds the lines. */
export class Roster {
  constructor(private readonly path: string, readonly lines: string[]) {}

  static load(path: string): Roster {
    return new Roster(path, readFileSync(path, "utf8").split("\n"));
  }

  add(who: Member): void {
    writeFileSync(this.path, [...this.lines, who.name].join("\n"));
  }
}

/** Pins a Notice and gives back what the file holds. */
export class Board {
  constructor(private readonly path: string) {}

  pin(notice: Notice): void {
    writeFileSync(this.path, JSON.stringify(notice));
  }

  posts(): Record<string, unknown>[] {
    return [JSON.parse(readFileSync(this.path, "utf8")) as Record<string, unknown>];
  }
}
