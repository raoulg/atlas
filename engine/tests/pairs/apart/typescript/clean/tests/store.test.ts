// The ledger's test: a ledger of its own, in the folder the system keeps for temporary files, flushed with a stamp.
import { tmpdir } from "node:os";
import { test } from "vitest";

import { Ledger } from "../src/store.js";

test("test_flush", () => {
  const ledger = new Ledger(tmpdir());
  ledger.flush("20260823");
});
