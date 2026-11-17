// The ledger's test: a ledger of its own, in the folder the program keeps its data in, flushed with a stamp.
import { test } from "vitest";

import { Ledger } from "../src/store.js";

test("test_flush", () => {
  const ledger = new Ledger("data/blocked");
  ledger.flush("20260823");
});
