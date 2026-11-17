import { readFileSync } from "node:fs";

export interface Item {
  title: string;
  body: string;
}

export function load(path: string): Item[] {
  return JSON.parse(readFileSync(path, "utf8")) as Item[];
}

export function summarize(items: Item[]): string {
  return items.map((i) => `${i.title}: ${i.body.slice(0, 80)}`).join("\n");
}
