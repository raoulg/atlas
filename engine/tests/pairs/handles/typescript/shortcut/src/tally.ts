// Count the lines of a data file and write the count to a report.
import { open } from "node:fs/promises";

async function main(): Promise<void> {
  await emit(await count());
}

/** The lines of the data file. */
export async function count(): Promise<number> {
  const f = await open("data.txt");
  const text = await f.readFile("utf8");
  await f.close();
  return text.split("\n").length - 1;
}

/** Write the count to the report. */
export async function emit(n: number): Promise<void> {
  const out = await open("report.txt", "w");
  await out.writeFile(`${n}\n`);
  await out.close();
}

await main();
