// Build a store and never call its methods: building it writes and reads nothing.

import { readFileSync, writeFileSync } from "node:fs";

export class Store {
  constructor(readonly path: string) {}

  save(text: string): void {
    writeFileSync(this.path, text);
  }

  load(): string {
    return readFileSync(this.path, "utf8");
  }
}

export function run(): void {
  const store = new Store("data/store.txt");
  console.log(store.path);
}
