// Each kind of the shop's stock written to a folder of its own, in a file named after it.
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const FOLDERS = ["books", "games"];
const NAMES = ["books.csv", "games.csv"];

/** Write each kind into its folder below `root`: `books/books.csv`, `games/games.csv`. */
export function exportStock(root: string): void {
  for (let i = 0; i < FOLDERS.length; i++) {
    mkdirSync(join(root, FOLDERS[i]!), { recursive: true });
    writeFileSync(join(root, FOLDERS[i]!, NAMES[i]!), FOLDERS[i]!);
  }
}

/** Export the stock into `out`. */
export function main(): void {
  exportStock("out");
}
