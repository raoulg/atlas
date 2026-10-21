import { readFileSync, writeFileSync } from "node:fs";

function bill(text: string): string {
  return text.toUpperCase();
}

export function main(): void {
  writeFileSync("out/invoice.txt", bill(readFileSync("data/ledger.csv", "utf8")));
}
