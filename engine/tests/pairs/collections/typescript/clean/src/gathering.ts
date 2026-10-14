// Readings kept in a file, one per line, and read back as readings.
import { readFileSync, writeFileSync } from "node:fs";

export class Reading {
  constructor(readonly text: string) {}
}

export class Row {
  constructor(readonly reading: Reading) {}
}

export class Store {
  constructor(private readonly path: string) {}

  put(reading: Reading): void {
    const rows: Row[] = this.rows();
    rows.push(new Row(reading));
    this.write(rows);
  }

  putAll(readings: Reading[]): void {
    const rows: Row[] = this.rows();
    for (const reading of readings) {
      rows.push(new Row(reading));
    }
    this.write(rows);
  }

  get(): Reading[] {
    return this.rows().map((row) => row.reading);
  }

  private rows(): Row[] {
    return readFileSync(this.path, "utf8").split("\n").map((line) => new Row(new Reading(line)));
  }

  private write(rows: Row[]): void {
    writeFileSync(this.path, rows.map((row) => row.reading.text).join("\n"));
  }
}
