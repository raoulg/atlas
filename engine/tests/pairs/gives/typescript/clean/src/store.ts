// Each function says what it gives back, so its callers' values have a type.
import { readFileSync } from "node:fs";

export class Store {
  constructor(readonly path: string) {}

  history(): string {
    return readFileSync(this.path, "utf8");
  }
}

function getStore(path: string): Store {
  return new Store(path);
}

function label(n: number): string {
  return `n=${n}`;
}

export function main(path: string): string {
  const store = getStore(path);
  return store.history() + label(1);
}
