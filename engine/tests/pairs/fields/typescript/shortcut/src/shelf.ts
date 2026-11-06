// A shelf the program keeps its notes on: the shelf is handed a name and puts its folder under one it names itself,
// each method names its own file in it, and one function writes them.

import { writeFileSync } from "node:fs";
import { join } from "node:path";

export class Shelf {
  private readonly root: string;

  constructor(name: string) {
    this.root = join("shelves", name);
  }

  index(): string {
    return join(this.root, "index.txt");
  }

  keep(text: string): void {
    write(join(this.root, "note.txt"), text);
  }
}

function write(path: string, text: string): void {
  writeFileSync(path, text, "utf-8");
}
