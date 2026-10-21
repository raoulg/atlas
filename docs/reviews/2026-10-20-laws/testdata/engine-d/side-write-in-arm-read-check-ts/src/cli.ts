// Fill a cache where it is missing, then read it only to check it is not empty.

import { existsSync, readFileSync, writeFileSync } from "node:fs";

if (!existsSync("cache/data.txt")) {
  writeFileSync("cache/data.txt", "fresh");
}
if (readFileSync("cache/data.txt", "utf8") === "") {
  throw new Error("empty cache");
}
