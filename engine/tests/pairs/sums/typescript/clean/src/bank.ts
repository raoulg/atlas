// A knowledge base kept in a file, a node per line; read back as the graph of its nodes.
import { readFileSync, writeFileSync } from "node:fs";

export class Begrip {
  constructor(readonly uuid: string, readonly naam: string) {}
}

export class Bron {
  constructor(readonly uuid: string, readonly url: string) {}
}

export type Node = Begrip | Bron;

export class Graaf {
  constructor(readonly nodes: Node[]) {}
}

export class Kennisbank {
  constructor(private readonly path: string) {}

  voegToe(bron: Bron): void {
    this.write(`${bron.uuid} bron ${bron.url}`);
  }

  graaf(): Graaf {
    return new Graaf(readFileSync(this.path, "utf8").split("\n").map((line) => this.node(line)));
  }

  private node(line: string): Node {
    const [uuid = "", soort = "", tekst = ""] = line.split(" ");
    return soort === "bron" ? new Bron(uuid, tekst) : new Begrip(uuid, tekst);
  }

  private write(line: string): void {
    writeFileSync(this.path, readFileSync(this.path, "utf8") + line + "\n");
  }
}
