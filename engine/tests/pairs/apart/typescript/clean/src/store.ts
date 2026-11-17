// A ledger kept in the folder it is handed: each flush writes one file, stamped with what its caller hands, `latest`
// unless told.

import { writeFileSync } from "node:fs";
import { join } from "node:path";

/** The rows a run blocked, kept in one folder. */
export class Ledger {
  rows: string[] = [];

  constructor(readonly folder: string) {}

  /** Writes the rows to a file of the folder, stamped `stamp`. */
  flush(stamp: string = "latest"): void {
    writeFileSync(join(this.folder, `blocked_${stamp}.jsonl`), this.rows.join("\n"), "utf8");
  }
}
