import { writeFileSync } from "node:fs";

/** Writes `text` to `path`. */
export function save(path, text) {
  writeFileSync(path, text);
}
