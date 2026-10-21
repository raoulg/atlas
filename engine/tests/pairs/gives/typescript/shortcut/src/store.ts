// The same store, with what its functions give back left to the compiler.
import { readFileSync } from "node:fs";

export class Store {
  constructor(readonly path: string) {}

  history(): string {
    return readFileSync(this.path, "utf8");
  }
}

function getStore(path: string) {
  return new Store(path);
}

function label(n: number) {
  return `n=${n}`;
}

export function main(path: string): string {
  const store = getStore(path);
  return store.history() + label(1);
}
