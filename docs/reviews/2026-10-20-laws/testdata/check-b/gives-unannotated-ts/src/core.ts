import { readFileSync } from "node:fs";

export class Store {
  constructor(readonly path: string) {}

  history(): string {
    return readFileSync(this.path, "utf8");
  }
}

function getStore() {
  return new Store(process.env.STORE ?? "store.txt");
}

function label(n: number) {
  return `n=${n}`;
}

export function main(): string {
  const store = getStore();
  return store.history() + label(1);
}
