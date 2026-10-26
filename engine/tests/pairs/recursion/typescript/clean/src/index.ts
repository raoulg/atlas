// A tree's items, split into a node's own and those below it. `split` takes the node's own from `itemsOf` and the rest
// from `below`, which splits each child in turn. `climb` enters a recursion of three (`deeper`, `onward`, `kids`) at two of
// its functions. The floor every item is raised to, and every node is held to, is handed down the recursion.

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

function itemsOf(node: Node, floor: Floor): Item[] {
  return [{ n: Math.max(node.n, floor.n) }];
}

function below(node: Node, floor: Floor): Item[] {
  const out: Item[] = new Array<Item>();
  for (const k of node.kids) {
    const [a, b] = split(k, floor);
    out.push(...a);
    out.push(...b);
  }
  return out;
}

/** The node's own items and the items below it. */
export function split(node: Node, floor: Floor): [Item[], Item[]] {
  const mine = itemsOf(node, floor);
  const rest = below(node, floor);
  return [mine, rest];
}

function deeper(node: Node, floor: Floor): Item[] {
  const out: Item[] = new Array<Item>();
  if (node.n > floor.n) {
    out.push(...onward(node, floor));
  }
  return out;
}

function kids(node: Node, floor: Floor): Item[] {
  return deeper(node, floor);
}

function onward(node: Node, floor: Floor): Item[] {
  return kids(node, floor);
}

/** The items over the floor, from the node and from the rest of the tree. */
export function climb(node: Node, floor: Floor): Item[] {
  const first = deeper(node, floor);
  const second = onward(node, floor);
  return [...first, ...second];
}
