import { closeSync, fsyncSync, openSync } from "node:fs";

/** Waits until what `path` holds is on the disk. */
export function sync(path) {
  const fd = openSync(path, "r");
  fsyncSync(fd);
  closeSync(fd);
}
