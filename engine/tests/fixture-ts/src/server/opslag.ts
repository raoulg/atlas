import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const WORTEL = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");
export const DATA = process.env.DATA_DIR ?? path.join(WORTEL, "data");

function lees(bestand: string): string {
  return fs.readFileSync(path.join(DATA, bestand), "utf8");
}

export function klanten(): string {
  return lees("klanten.csv");
}

export function bewaar(tekst: string): void {
  fs.writeFileSync(fileURLToPath(new URL("../../uit/regels.pdf", import.meta.url)), tekst);
}
