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

/// What a boundary call touches: the first string literal under the call (a path, a
/// table, a command), else the first argument's own text, else nothing. Long literals
/// are cut; a SQL or Cypher statement keeps its first three words.
pub fn what_of(call: TsNode, src: &[u8], string_kinds: &[&str], arg_field: &str) -> String {
    if let Some(s) = walk(call, string_kinds).into_iter().next() {
        let raw = text(s, src);
        let raw = raw.trim_start_matches(|c: char| c.is_ascii_alphabetic()); // f"..", b"..", r".."
        let lit = raw.trim_matches(|c| c == '"' || c == '\'' || c == '`').trim();
        let words: Vec<&str> = lit.split_whitespace().collect();
        let out = if words.len() > 3 { words[..3].join(" ") + " …" } else { lit.to_string() };
        return out.chars().take(48).collect();
    }
    let Some(args) = call.child_by_field_name(arg_field) else { return String::new() };
    match named_children(args).into_iter().next() {
        Some(a) if matches!(a.kind(), "identifier" | "attribute" | "field_expression" | "reference_expression" | "scoped_identifier") => {
            text(a, src).trim_start_matches('&').trim_start_matches("mut ").chars().take(40).collect()
        }
        _ => String::new(),
    }
}
