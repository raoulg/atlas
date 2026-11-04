// Count the lines of a data file and write the count to a report.
import { open, type FileHandle } from "node:fs/promises";

/** The data file. */
const DATA = "data.txt";
/** The report its count is written to. */
const REPORT = "report.txt";

async function main(): Promise<void> {
  const data = await open(DATA);
  const n = await count(data);
  await data.close();
  const report = await open(REPORT, "w");
  await emit(report, n);
  await report.close();
}

/** The lines of the file handed in. */
export async function count(f: FileHandle): Promise<number> {
  const text = await f.readFile("utf8");
  return text.split("\n").length - 1;
}

/** Write the count to the file handed in. */
export async function emit(out: FileHandle, n: number): Promise<void> {
  await out.writeFile(`${n}\n`);
}

await main();
