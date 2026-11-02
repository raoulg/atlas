// The store's tests, beside the store: a sample shelf on disk, loaded and checked.
import { expect, test } from "vitest";

import { load, type Shelf } from "./disk.js";

interface Sample {
  path: string;
}

function sample(): Sample {
  return { path: "fixtures/shelf.csv" };
}

function check(sample: Sample): Shelf {
  return load(sample.path);
}

test("loads", () => {
  expect(check(sample()).titles.length).toBeGreaterThan(0);
});
