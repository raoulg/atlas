// Readings kept in files, with the outages of a harvest and the summaries of a day; outages and
// summaries read back as the lines they are kept as.
import { readFileSync, writeFileSync } from "node:fs";

export class Reading {
  constructor(readonly station: string, readonly date: string, readonly temperature: number) {}
}

/** What the service said when it gave a station no reading. */
export class Outage {
  constructor(readonly station: string, readonly reason: string) {}
}

/** A station on a date: what a summary is of. */
export class Day {
  constructor(readonly station: string, readonly date: string) {}
}

/** Puts readings, outages and summaries into their files and gets them back. */
export class ReadingStore {
  constructor(
    private readonly readings: string,
    private readonly outages: string,
    private readonly summaries: string,
  ) {}

  save(reading: Reading): void {
    writeFileSync(this.readings, readFileSync(this.readings, "utf8") + `${reading.station} ${reading.date} ${reading.temperature}\n`);
  }

  history(station: string): Reading[] {
    return readFileSync(this.readings, "utf8")
      .split("\n")
      .map((line) => line.split(" "))
      .filter(([s]) => s === station)
      .map(([s = "", date = "", t = "0"]) => new Reading(s, date, Number(t)));
  }

  saveOutage(outage: Outage): void {
    writeFileSync(this.outages, readFileSync(this.outages, "utf8") + `${outage.station} ${outage.reason}\n`);
  }

  outages(station: string): string[] {
    return readFileSync(this.outages, "utf8").split("\n").filter((line) => line.startsWith(station));
  }

  summary(day: Day): string {
    const text = readFileSync(this.summaries, "utf8");
    const key = `${day.station} ${day.date}`;
    const kept = text.split("\n").find((line) => line.startsWith(key));
    if (kept) {
      return kept;
    }
    const temperatures = this.history(day.station).filter((r) => r.date === day.date).map((r) => r.temperature);
    const line = `${key} ${Math.min(...temperatures)} ${Math.max(...temperatures)}`;
    writeFileSync(this.summaries, text + line + "\n");
    return line;
  }
}
