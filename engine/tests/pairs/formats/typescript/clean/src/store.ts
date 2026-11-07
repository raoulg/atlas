import { readFile, writeFile } from "node:fs/promises";

/** Where the table is kept. */
export const TABLE = "data/table.csv";

export async function save(text: string, to: string = TABLE): Promise<void> {
  await writeFile(to, text);
}

export async function load(path: string = TABLE): Promise<string> {
  return await readFile(path, "utf8");
}
