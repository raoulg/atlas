// A tree's items, gathered into one list that `collectInto` fills. `byLiteral` and `byCall` make the empty list each in
// its own way, `[]` and `new Array()`: the same list, made by the body.

/** An item of a node. */
export interface Item {
  readonly n: number;
}

/** A node of the tree. */
export interface Node {
  readonly n: number;
  readonly kids: Node[];
}

function collectInto(node: Node, out: Item[]): void {
  out.push({ n: node.n });
  for (const k of node.kids) {
    collectInto(k, out);
  }
}

/** The items of the tree, gathered into a list written as a literal. */
export function byLiteral(node: Node): Item[] {
  const items: Item[] = [];
  collectInto(node, items);
  return items;
}

/** The items of the tree, gathered into a list a call makes. */
export function byCall(node: Node): Item[] {
  const items: Item[] = new Array<Item>();
  collectInto(node, items);
  return items;
}
