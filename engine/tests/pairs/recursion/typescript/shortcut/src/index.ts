// A tree's items, split into a node's own and those below it. `split` takes the node's own from `itemsOf` and the rest
// from `below`, which splits each child in turn. `climb` enters a recursion of three (`deeper`, `onward`, `kids`) at two of
// its functions. The floor every item is raised to, and every node is held to, is the module's.

/** An item of a node. */
export interface Item {
  readonly n: number;
}

/** The least an item counts. */
export interface Floor {
  readonly n: number;
}

/** A node of the tree. */
export interface Node {
  readonly n: number;
  readonly kids: Node[];
}

const FLOOR: Floor = { n: 1 };

function itemsOf(node: Node, floor: Floor): Item[] {
  return [{ n: Math.max(node.n, floor.n) }];
}

function below(node: Node): Item[] {
  const out: Item[] = new Array<Item>();
  for (const k of node.kids) {
    const [a, b] = split(k);
    out.push(...a);
    out.push(...b);
  }
  return out;
}

/** The node's own items and the items below it. */
export function split(node: Node): [Item[], Item[]] {
  const mine = itemsOf(node, FLOOR);
  const rest = below(node);
  return [mine, rest];
}

function deeper(node: Node): Item[] {
  const out: Item[] = new Array<Item>();
  if (node.n > FLOOR.n) {
    out.push(...onward(node));
  }
  return out;
}

function kids(node: Node): Item[] {
  return deeper(node);
}

function onward(node: Node): Item[] {
  return kids(node);
}

/** The items over the floor, from the node and from the rest of the tree. */
export function climb(node: Node): Item[] {
  const first = deeper(node);
  const second = onward(node);
  return [...first, ...second];
}
