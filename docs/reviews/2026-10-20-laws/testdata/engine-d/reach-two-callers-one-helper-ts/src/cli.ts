// Two steps hand one helper different files: each step writes only its own.

import { writeFileSync } from "node:fs";

function writeTo(name: string): void {
  writeFileSync(name, "x");
}

function alpha(): void {
  writeTo("data/alpha.txt");
}

function beta(): void {
  writeTo("data/beta.txt");
}

alpha();
beta();
