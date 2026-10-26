import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

export class Store {
  constructor(readonly root: string) {}

  has(key: string): boolean {
    return existsSync(join(this.root, key));
  }

  get(key: string): string {
    return readFileSync(join(this.root, key), "utf8");
  }

  put(key: string, text: string): void {
    writeFileSync(join(this.root, key), text);
  }
}
