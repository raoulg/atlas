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
    if let Some(lit) = literals(call, src, string_kinds).into_iter().find(|l| !crate::effects::is_mode(l)) {
        let lit = lit.trim();
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

/// Every string literal under a call, unquoted, in source order: `f"{name}.png"` -> `{name}.png`.
pub fn literals(call: TsNode, src: &[u8], string_kinds: &[&str]) -> Vec<String> {
    walk(call, string_kinds)
        .into_iter()
        .map(|s| {
            let raw = text(s, src).trim_start_matches(|c: char| c.is_ascii_alphabetic()); // f"..", b"..", r#".."#
            let raw = raw.trim_matches('#');
            let raw = raw.strip_prefix("\"\"\"").and_then(|r| r.strip_suffix("\"\"\"")).unwrap_or(raw);
            raw.trim_matches(|c| c == '"' || c == '\'' || c == '`').to_string()
        })
        .collect()
}

/// What flows through a boundary call: the first plain identifier among the arguments that
/// is not the `what` (the path or command), as the author named it, with its type when a
/// local or parameter of that name is known.
pub fn payload_of(call: TsNode, src: &[u8], what: &str, arg_field: &str, locals: &std::collections::HashMap<String, String>) -> String {
    let Some(args) = call.child_by_field_name(arg_field) else { return String::new() };
    for id in walk(args, &["identifier"]) {
        let bad_parent = id.parent().map_or(false, |p| matches!(p.kind(), "scoped_identifier" | "attribute" | "field_expression" | "generic_function" | "keyword_argument" | "call_expression" | "call"));
        if bad_parent {
            continue;
        }
        let name = text(id, src);
        if name == what || name == "self" || name == "cls" {
            continue;
        }
        return match locals.get(name) {
            Some(t) => format!("{name}: {}", t.rsplit('.').next().unwrap_or(t)),
            None => name.to_string(),
        };
    }
    String::new()
}
