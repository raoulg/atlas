//! A tree's items, gathered into one list that `collect_into` fills. `by_literal` and `by_call` make
//! the empty list each in its own way, `vec![]` and `Vec::new()`: the same list, made by the body.

/// An item of a node.
pub struct Item {
    pub n: u32,
}

/// A node of the tree.
pub struct Node {
    pub kids: Vec<Node>,
    pub n: u32,
}

fn collect_into(node: &Node, out: &mut Vec<Item>) {
    out.push(Item { n: node.n });
    for k in &node.kids {
        collect_into(k, out);
    }
}

/// The items of the tree, gathered into a list written as a literal.
pub fn by_literal(node: &Node) -> Vec<Item> {
    let mut items = vec![];
    collect_into(node, &mut items);
    items
}

/// The items of the tree, gathered into a list a call makes.
pub fn by_call(node: &Node) -> Vec<Item> {
    let mut items = Vec::new();
    collect_into(node, &mut items);
    items
}
