//! Small helpers over tree-sitter nodes, shared by the language modules.

use tree_sitter::Node as TsNode;

pub fn text<'a>(n: TsNode, src: &'a [u8]) -> &'a str {
    n.utf8_text(src).unwrap_or("")
}

pub fn named_children<'t>(n: TsNode<'t>) -> Vec<TsNode<'t>> {
    let mut c = n.walk();
    n.named_children(&mut c).collect()
}

pub fn field_children<'t>(n: TsNode<'t>, field: &str) -> Vec<TsNode<'t>> {
    let mut c = n.walk();
    n.children_by_field_name(field, &mut c).collect()
}

/// Pre-order, source order.
pub fn walk<'t>(n: TsNode<'t>, kinds: &[&str]) -> Vec<TsNode<'t>> {
    let mut out = vec![];
    let mut stack = vec![n];
    while let Some(cur) = stack.pop() {
        if kinds.contains(&cur.kind()) {
            out.push(cur);
        }
        for k in named_children(cur).into_iter().rev() {
            stack.push(k);
        }
    }
    out
}

/// Row-based span, 1-based, as the viewer shows it.
pub fn span(n: TsNode) -> Option<(u32, u32)> {
    Some((n.start_position().row as u32 + 1, n.end_position().row as u32 + 1))
}
