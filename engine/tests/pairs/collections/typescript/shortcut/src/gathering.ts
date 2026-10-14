// Readings kept in a file, one per line; read back as the lines.
import { readFileSync, writeFileSync } from "node:fs";

export class Reading {
  constructor(readonly text: string) {}
}

export class Row {
  constructor(readonly reading: Reading) {}
}

function writeRows(path: string, rows: Row[]): void {
  writeFileSync(path, rows.map((row) => row.reading.text).join("\n"));
}

export class Store {
  constructor(private readonly path: string) {}

  put(reading: Reading): void {
    const rows: Row[] = this.rows();
    rows.push(new Row(reading));
    writeRows(this.path, rows);
  }

  putAll(readings: Reading[]): void {
    const rows: Row[] = this.rows();
    for (const reading of readings) {
      rows.push(new Row(reading));
    }
    writeRows(this.path, rows);
  }

  get(): string[] {
    return readFileSync(this.path, "utf8").split("\n");
  }

  private rows(): Row[] {
    return readFileSync(this.path, "utf8").split("\n").map((line) => new Row(new Reading(line)));
  }
}

export class DroppedStore {
  constructor(private readonly path: string) {}

  put(reading: Reading): void {
    const rows: Row[] = this.rows();
    const added: Row[] = [];
    added.push(new Row(reading));
    writeRows(this.path, rows);
  }

  putAll(readings: Reading[]): void {
    const rows: Row[] = this.rows();
    const added: Row[] = [];
    for (const reading of readings) {
      added.push(new Row(reading));
    }
    writeRows(this.path, rows);
  }

  get(): string[] {
    return readFileSync(this.path, "utf8").split("\n");
  }

  private rows(): Row[] {
    return readFileSync(this.path, "utf8").split("\n").map((line) => new Row(new Reading(line)));
  }
}

export class Frozen {
  push(row: Row): Frozen {
    return new Frozen();
  }
}

export function observe(frozen: Frozen, row: Row): void {
  frozen.push(row);
}
