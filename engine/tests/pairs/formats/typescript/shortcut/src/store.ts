import { readFile, writeFile } from "node:fs/promises";

export async function save(text: string, to: string): Promise<void> {
  await writeFile(to, text);
}

export async function load(path: string): Promise<string> {
  return await readFile(path, "utf8");
}
