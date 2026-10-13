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

export class Archief {
  constructor(private map: string) {}

  bewaar(naam: string, tekst: string): void {
    fs.writeFileSync(path.join(this.map, naam), tekst);
  }
}

export class Logboek {
  map: string;

  constructor(map: string) {
    this.map = map;
  }

  noteer(regel: string): void {
    fs.appendFileSync(path.join(this.map, "log.txt"), regel);
  }
}

export function archiveer(): void {
  new Archief("archief").bewaar("a.txt", "b");
  new Logboek("logs").noteer("x");
}

export function echo(): void {
  process.stdin.on("data", (d) => process.stdout.write(d));
}
