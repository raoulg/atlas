// A store of notes and the line a report makes of it: the route makes the line itself, and throws away what the step
// that saves it gives back.

import { appendFileSync } from "node:fs";

export class Store {
  constructor(
    readonly path: string,
    readonly count: number,
  ) {}
}

function save(store: Store, line: string): string {
  appendFileSync(store.path, line + "\n");
  return line;
}

export function main(): void {
  const store = new Store("notes.txt", 3);
  const line = `${store.path}: ${store.count} notes`;
  void save(store, line);
  console.log(line);
}

main();
