// What a graph draws: every node's touches, each drawn by its own method where it is.

export class Where {
  constructor(public place: string) {}
}

export class Touch {
  constructor(public op: string) {}

  drawn(): Where {
    return new Where(this.op);
  }
}

export class Node {
  constructor(public id: string, public touches: Touch[]) {}
}

export class Graph {
  constructor(public nodes: Node[]) {}
}

export function drawn(g: Graph): Where[] {
  return g.nodes.flatMap((n) => n.touches.map((t) => t.drawn()));
}
