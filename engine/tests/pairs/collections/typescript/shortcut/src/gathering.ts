// Readings kept in a file, one per line; read back as the lines.
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

  get(): string[] {
    return readFileSync(this.path, "utf8").split("\n");
  }

  private rows(): Row[] {
    return readFileSync(this.path, "utf8").split("\n").map((line) => new Row(new Reading(line)));
  }

  private write(rows: Row[]): void {
    writeFileSync(this.path, rows.map((row) => row.reading.text).join("\n"));
  }
}

export class DroppedStore {
  constructor(private readonly path: string) {}

  put(reading: Reading): void {
    const rows: Row[] = this.rows();
    const added: Row[] = [];
    added.push(new Row(reading));
    this.write(rows);
  }

  get(): string[] {
    return readFileSync(this.path, "utf8").split("\n");
  }

  private rows(): Row[] {
    return readFileSync(this.path, "utf8").split("\n").map((line) => new Row(new Reading(line)));
  }

  private write(rows: Row[]): void {
    writeFileSync(this.path, rows.map((row) => row.reading.text).join("\n"));
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
