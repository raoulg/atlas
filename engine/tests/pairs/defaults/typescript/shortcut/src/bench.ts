import { join } from "node:path";

/** A file of the workbench in `home`. */
export function made(home: string, name: string): string {
  return join(home, ".made", name);
}
