import { writeFileSync } from "node:fs";
import { sync } from "./disk.js";

/** Saves `text` at `path`, and waits until it is on the disk. */
export function save(path, text) {
  write(path, text);
  sync(path);
}

/** Writes `text` to `path`. */
export function write(path, text) {
  writeFileSync(path, text);
}
