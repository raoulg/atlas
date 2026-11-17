import { writeFileSync } from "node:fs";

import { load, summarize } from "../src/lib/feed.js";

writeFileSync("summary.txt", summarize(load("feed.json")));
