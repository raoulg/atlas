import { expect, test } from "vitest";

import { post } from "../ledger/post.js";
import { round } from "./round.js";

function posted(n: number): number {
  return post(n);
}

test("rounds what is posted", () => {
  expect(round(posted(25))).toBe(20);
});
