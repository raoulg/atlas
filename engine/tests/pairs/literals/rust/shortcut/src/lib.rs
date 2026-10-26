//! A tree's items, gathered into one list that `collect_into` fills. `by_constant` starts from the
//! module's list, `by_call` makes its own with `Vec::new()`.

/// An item of a node.
pub struct Item {
    pub n: u32,
}

/// A node of the tree.
pub struct Node {
    pub kids: Vec<Node>,
    pub n: u32,
}

/// The list every gathering starts from.
const SEED: Vec<Item> = Vec::new();

fn collect_into(node: &Node, out: &mut Vec<Item>) {
    out.push(Item { n: node.n });
    for k in &node.kids {
        collect_into(k, out);
    }
}

/// The items of the tree, gathered into the module's list.
pub fn by_constant(node: &Node) -> Vec<Item> {
    let mut items = SEED;
    collect_into(node, &mut items);
    items
}

/// The items of the tree, gathered into a list a call makes.
pub fn by_call(node: &Node) -> Vec<Item> {
    let mut items = Vec::new();
    collect_into(node, &mut items);
    items
}
