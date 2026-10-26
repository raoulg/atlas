//! A tree's items, split into a node's own and those below it. `split` takes the node's own from
//! `items_of` and the rest from `below`, which splits each child in turn. `climb` enters a recursion
//! of three (`deeper`, `onward`, `kids`) at two of its functions. The floor every item is raised to,
//! and every node is held to, is the module's.

/// An item of a node.
pub struct Item {
    pub n: u32,
}

/// The least an item counts.
pub struct Floor {
    pub n: u32,
}

/// A node of the tree.
pub struct Node {
    pub kids: Vec<Node>,
    pub n: u32,
}

/// The floor of every split.
const FLOOR: Floor = Floor { n: 1 };

fn items_of(node: &Node, floor: &Floor) -> Vec<Item> {
    vec![Item { n: node.n.max(floor.n) }]
}

fn below(node: &Node) -> Vec<Item> {
    let mut out = Vec::new();
    for k in &node.kids {
        let (a, b) = split(k);
        out.extend(a);
        out.extend(b);
    }
    out
}

/// The node's own items and the items below it.
pub fn split(node: &Node) -> (Vec<Item>, Vec<Item>) {
    let mine = items_of(node, &FLOOR);
    let rest = below(node);
    (mine, rest)
}

fn deeper(node: &Node) -> Vec<Item> {
    let mut out = Vec::new();
    if node.n > FLOOR.n {
        out.extend(onward(node));
    }
    out
}

fn kids(node: &Node) -> Vec<Item> {
    deeper(node)
}

fn onward(node: &Node) -> Vec<Item> {
    kids(node)
}

/// The items over the floor, from the node and from the rest of the tree.
pub fn climb(node: &Node) -> Vec<Item> {
    let mut first = deeper(node);
    let second = onward(node);
    first.extend(second);
    first
}
