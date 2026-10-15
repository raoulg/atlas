// Readings kept in files, with the outages of a harvest and the summaries of a day.
import { readFileSync, writeFileSync } from "node:fs";

export class Reading {
  constructor(readonly station: string, readonly date: string, readonly temperature: number) {}
}

/** What the service said when it gave a station no reading. */
export class Outage {
  constructor(readonly station: string, readonly reason: string) {}
}

/** What was kept of a station: its readings, and what the service said when it gave none. */
export class Harvest {
  constructor(readonly readings: Reading[], readonly outages: Outage[]) {}
}

/** A station on a date: what a summary is of. */
export class Day {
  constructor(readonly station: string, readonly date: string) {}
}

/** The lowest and highest temperature a station had on a date. */
export class Summary {
  constructor(readonly station: string, readonly date: string, readonly low: number, readonly high: number) {}
}

/** The summary of the readings taken on the day. */
export function daySummary(day: Day, readings: Reading[]): Summary {
  const temperatures = readings.filter((r) => r.date === day.date).map((r) => r.temperature);
  return new Summary(day.station, day.date, Math.min(...temperatures), Math.max(...temperatures));
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

  /** What was kept of a station: its readings, and what the service said when it gave none. */
  kept(station: string): Harvest {
    const outages = readFileSync(this.outages, "utf8")
      .split("\n")
      .map((line) => line.split(" "))
      .filter(([s]) => s === station)
      .map(([s = "", reason = ""]) => new Outage(s, reason));
    return new Harvest(this.history(station), outages);
  }

  /** The summary of a day: the one kept, else one made from the day's readings and kept. */
  summary(day: Day): Summary {
    return this.keptSummary(day) ?? this.keepSummary(daySummary(day, this.history(day.station)));
  }

  keptSummary(day: Day): Summary | undefined {
    const line = readFileSync(this.summaries, "utf8")
      .split("\n")
      .map((l) => l.split(" "))
      .find(([s, date]) => s === day.station && date === day.date);
    return line && new Summary(day.station, day.date, Number(line[2]), Number(line[3]));
  }

  keepSummary(summary: Summary): Summary {
    writeFileSync(this.summaries, readFileSync(this.summaries, "utf8") + `${summary.station} ${summary.date} ${summary.low} ${summary.high}\n`);
    return summary;
  }
}
