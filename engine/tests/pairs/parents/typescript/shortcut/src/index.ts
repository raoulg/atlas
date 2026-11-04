// A data file's line count, logged in the directory the data files lie in.
import { readFileSync, appendFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

/** The directory the data files lie in: `data` in the user's home. */
function dataDir(): string {
  return join(homedir(), "data");
}

/** Count the lines of the data file `name` and log the count beside it. */
export function summarise(name: string): number {
  const source = join(dataDir(), name);
  const count = readFileSync(source, "utf8").split("\n").length;
  log(source, count);
  return count;
}

/** Add the count of the data file `source` to the log in its directory. */
function log(source: string, count: number): void {
  const folder = source.slice(0, source.lastIndexOf("/"));
  appendFileSync(folder + "/shelf.log", `${source} ${count}\n`);
}
