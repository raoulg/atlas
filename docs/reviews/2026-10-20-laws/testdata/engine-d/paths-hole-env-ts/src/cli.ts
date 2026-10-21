// The data directory comes from the environment: the code does not decide it, so it is a hole.

import { readFileSync } from "node:fs";
import { join } from "node:path";

const root = process.env.DATA_DIR ?? "";
console.log(readFileSync(join(root, "sub", "report.txt"), "utf8"));
