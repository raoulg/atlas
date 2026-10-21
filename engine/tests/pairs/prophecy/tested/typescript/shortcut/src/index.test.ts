import { expect, test } from "vitest";

import { seal } from "./index.js";

function sealedTwice(r: string): string {
  return seal(seal(r));
}

test("sealing twice is sealing once", () => {
  expect(sealedTwice("r")).toBe(seal("r"));
});
