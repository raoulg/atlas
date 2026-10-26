// What a graph draws: every node's touches, pushed one by one onto a list the body changes.

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
  const out: Where[] = [];
  for (const n of g.nodes) {
    for (const t of n.touches) {
      out.push(t.drawn());
    }
  }
  return out;
}
