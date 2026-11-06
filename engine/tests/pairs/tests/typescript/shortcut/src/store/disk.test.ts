// The shelf's tests, inside the package beside its store: a sample shelf on disk, loaded through the package's door.
import { expect, test } from "vitest";

import { load } from "../index.js";

function sample(): string {
  return "fixtures/shelf.txt";
}

test("test_loads", () => {
  expect(load(sample()).length).toBeGreaterThan(0);
});
