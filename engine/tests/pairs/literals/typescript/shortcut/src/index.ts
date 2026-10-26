// A tree's items, gathered into one list that `collectInto` fills. `byConstant` starts from the module's list, `byCall`
// makes its own with `new Array()`.

/** An item of a node. */
export interface Item {
  readonly n: number;
}

/** A node of the tree. */
export interface Node {
  readonly n: number;
  readonly kids: Node[];
}

const SEED: Item[] = [];

function collectInto(node: Node, out: Item[]): void {
  out.push({ n: node.n });
  for (const k of node.kids) {
    collectInto(k, out);
  }
}

/** The items of the tree, gathered into the module's list. */
export function byConstant(node: Node): Item[] {
  const items = SEED;
  collectInto(node, items);
  return items;
}

/** The items of the tree, gathered into a list a call makes. */
export function byCall(node: Node): Item[] {
  const items: Item[] = new Array<Item>();
  collectInto(node, items);
  return items;
}
