// A store of notes and the line a report makes of it: each value goes to the step that uses it, and what a step gives
// back the route uses.

import { appendFileSync } from "node:fs";

export class Store {
  constructor(
    readonly path: string,
    readonly count: number,
  ) {}
}

function describe(store: Store): string {
  return `${store.path}: ${store.count} notes`;
}

function save(store: Store, line: string): string {
  appendFileSync(store.path, line + "\n");
  return line;
}

export function main(): void {
  const store = new Store("notes.txt", 3);
  const line = describe(store);
  const kept = save(store, line);
  console.log(kept);
}

main();
