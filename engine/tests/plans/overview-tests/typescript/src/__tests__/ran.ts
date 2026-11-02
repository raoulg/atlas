// The command's test: it keeps a mark that it ran.
import { writeFileSync } from "node:fs";
import { test } from "vitest";

test("ran", () => {
  writeFileSync("out/ran.txt", "ok");
});
