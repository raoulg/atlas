// Each kind of the shop's stock written to a folder of its own, in a file named after it.
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const KINDS = ["books", "games"];

/** Write each kind into its folder below `root`: `books/books.csv`, `games/games.csv`. */
export function exportStock(root: string): void {
  for (const kind of KINDS) {
    mkdirSync(join(root, kind), { recursive: true });
    writeFileSync(join(root, kind, `${kind}.csv`), kind);
  }
}

/** Export the stock into `out`. */
export function main(): void {
  exportStock("out");
}
