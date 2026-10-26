import { readFileSync } from "node:fs";

export class Store {
  constructor(readonly root: string) {}

  get(key: string): string {
    return readFileSync(`${this.root}/${key}`, "utf8");
  }
}
