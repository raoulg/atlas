import { renameSync, writeFileSync } from "node:fs";

/** Writes `text` beside `path` and renames it into place, so that a reader never finds half of it. */
export function save(path, text) {
  const partial = `${path}.part`;
  writeFileSync(partial, text);
  renameSync(partial, path);
}
