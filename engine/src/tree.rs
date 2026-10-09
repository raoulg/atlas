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

/// Every named node under `n`, pre-order, source order.
pub fn walk_all<'t>(n: TsNode<'t>) -> Vec<TsNode<'t>> {
    let mut out = vec![];
    let mut stack = vec![n];
    while let Some(cur) = stack.pop() {
        out.push(cur);
        for k in named_children(cur).into_iter().rev() {
            stack.push(k);
        }
    }
    out
}

/// What a direct child is to the choice that holds it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Part {
    /// One of the alternatives, by its place in the choice.
    Arm(u32),
    /// What decides which arm runs: a condition, a subject.
    Decides,
    /// Neither: an `elif` or an `else if` that carries its own arms, a `finally`.
    Through,
}

/// How a language writes choices: `if`/`elif`/`else`, `match`, `x or y`,
/// `unwrap_or_else(..)`.
pub struct ChoiceSyntax {
    /// Does this node choose between arms?
    pub chooses: fn(TsNode, &[u8]) -> bool,
    /// The node whose choice this one continues: an `elif` its `if`, an `else if` the `if`
    /// before it.
    pub continues: fn(TsNode) -> Option<TsNode>,
    /// What a direct child is to the choice.
    pub part: fn(TsNode, TsNode, &[u8]) -> Part,
}

/// The choices of a body, numbered in source order; a continuation shares the number of the
/// choice it continues.
pub fn number_choices(body: TsNode, src: &[u8], syn: &ChoiceSyntax) -> std::collections::HashMap<usize, u32> {
    let mut ids = std::collections::HashMap::new();
    let mut next = 0;
    for n in walk_all(body) {
        if !(syn.chooses)(n, src) {
            continue;
        }
        match (syn.continues)(n).and_then(|c| ids.get(&c.id()).copied()) {
            Some(c) => {
                ids.insert(n.id(), c);
            }
            None => {
                ids.insert(n.id(), next);
                next += 1;
            }
        }
    }
    ids
}

/// The arms of the choices `n` sits in, from the outermost in.
pub fn arms_of(n: TsNode, body: TsNode, src: &[u8], ids: &std::collections::HashMap<usize, u32>, syn: &ChoiceSyntax) -> Vec<(u32, u32)> {
    let mut out = vec![];
    let mut child = n;
    while child.id() != body.id() {
        let Some(p) = child.parent() else { break };
        if let (Some(&c), Part::Arm(a)) = (ids.get(&p.id()), (syn.part)(p, child, src)) {
            out.push((c, a));
        }
        child = p;
    }
    out.reverse();
    out
}

/// The choice `n` helps decide: the nearest choice whose deciding part holds it. A
/// comparison inside an arm, or outside every choice, decides none.
pub fn decides(n: TsNode, body: TsNode, src: &[u8], ids: &std::collections::HashMap<usize, u32>, syn: &ChoiceSyntax) -> Option<u32> {
    let mut child = n;
    while child.id() != body.id() {
        let p = child.parent()?;
        if let Some(&c) = ids.get(&p.id()) {
            match (syn.part)(p, child, src) {
                Part::Decides => return Some(c),
                Part::Arm(_) => return None,
                Part::Through => {}
            }
        }
        child = p;
    }
    None
}

/// The position of `child` among the named children of `parent` of the same kind.
pub fn nth_of_kind(parent: TsNode, child: TsNode) -> u32 {
    named_children(parent).into_iter().filter(|c| c.kind() == child.kind()).take_while(|c| c.id() != child.id()).count() as u32
}

/// Calls in the order they run: what a call is handed (its arguments, its receiver) before
/// the call itself, so `Serial([Padding(), Mean()])` runs `Padding`, `Mean`, then `Serial`.
pub fn evaluation_order(mut calls: Vec<TsNode>) -> Vec<TsNode> {
    calls.sort_by_key(|c| c.end_byte());
    calls
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
    walk(call, string_kinds).into_iter().map(|s| unquote(text(s, src))).collect()
}

/// A string literal's contents: `f"{name}.png"` -> `{name}.png`, `r#"x"#` -> `x`.
pub fn unquote(raw: &str) -> String {
    let raw = raw.trim_start_matches(|c: char| c.is_ascii_alphabetic()); // f"..", b"..", r#".."#
    let raw = raw.trim_matches('#');
    let raw = raw.strip_prefix("\"\"\"").and_then(|r| r.strip_suffix("\"\"\"")).unwrap_or(raw);
    raw.trim_matches(|c| c == '"' || c == '\'' || c == '`').to_string()
}

/// Does a boundary call decide its own place? True when the path or URL it is handed (its
/// first argument, or the receiver it is called on) is built up from a literal:
/// `open("out.csv")`, `Path("data") / name`, `f"out/{x}.png"`, `Path::new("a.toml")`. False
/// when it starts at a value from elsewhere: `root / "x.csv"`, `root.join("Cargo.toml")`,
/// `f"{root}/x"`.
///
/// When the operation works on the path it is called on (`dir.glob("*")`), only that path
/// counts: the argument is a pattern below it, or what is written.
pub fn starts_at_literal(call: TsNode, src: &[u8], string_kinds: &[&str], arg_field: &str, on_receiver: bool) -> bool {
    let receiver = call.child_by_field_name("function").and_then(receiver_of);
    let first = if on_receiver { None } else { call.child_by_field_name(arg_field).and_then(first_arg) };
    [receiver, first].into_iter().flatten().any(|e| head_literal(e, src, string_kinds))
}

/// `x` in `x.method`, for Python attributes and Rust field expressions.
fn receiver_of(f: TsNode) -> Option<TsNode> {
    match f.kind() {
        "attribute" => f.child_by_field_name("object"),
        "field_expression" => f.child_by_field_name("value"),
        _ => None,
    }
}

fn first_arg(args: TsNode) -> Option<TsNode> {
    named_children(args).into_iter().find(|a| a.kind() != "comment")
}

/// Whether an expression's value starts at a literal: the leftmost operand of a `/` or a
/// `+`, the receiver of a method, the first argument of a constructor like `Path(..)`.
fn head_literal(e: TsNode, src: &[u8], string_kinds: &[&str]) -> bool {
    let child = |f: &str| e.child_by_field_name(f);
    match e.kind() {
        k if string_kinds.contains(&k) => {
            let lit = unquote(text(e, src));
            !lit.is_empty() && !lit.starts_with('{')
        }
        "binary_operator" | "binary_expression" => child("left").map_or(false, |l| head_literal(l, src, string_kinds)),
        "keyword_argument" => child("value").map_or(false, |v| head_literal(v, src, string_kinds)),
        "parenthesized_expression" | "reference_expression" => {
            child("value").or_else(|| first_arg(e)).map_or(false, |v| head_literal(v, src, string_kinds))
        }
        "call" | "call_expression" => {
            let Some(f) = child("function") else { return false };
            match receiver_of(f) {
                Some(r) => head_literal(r, src, string_kinds),
                None => child("arguments").and_then(first_arg).map_or(false, |a| head_literal(a, src, string_kinds)),
            }
        }
        "attribute" | "field_expression" => receiver_of(e).map_or(false, |r| head_literal(r, src, string_kinds)),
        // `format!("out/{}.png", x)`: the template decides
        "macro_invocation" => walk(e, string_kinds).first().map_or(false, |l| head_literal(*l, src, string_kinds)),
        _ => false,
    }
}

/// A call's argument as written: the one passed by `keyword`, else the first passed by position.
pub fn argument(call: TsNode, src: &[u8], keyword: &str) -> Option<String> {
    let args = named_children(call.child_by_field_name("arguments")?);
    let by_name = args.iter().find(|a| a.kind() == "keyword_argument" && a.child_by_field_name("name").map(|n| text(n, src)) == Some(keyword));
    let arg = match by_name {
        Some(k) => k.child_by_field_name("value")?,
        None => *args.iter().find(|a| !matches!(a.kind(), "keyword_argument" | "comment"))?,
    };
    let t = text(arg, src);
    Some(if arg.kind() == "string" { unquote(t) } else { t.to_string() })
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Whether the first call in `code` is handed a place that starts at a literal.
    fn fixed(lang: tree_sitter::Language, code: &str, call: &str, strings: &[&str]) -> bool {
        let mut p = tree_sitter::Parser::new();
        p.set_language(&lang).unwrap();
        let tree = p.parse(code, None).unwrap();
        let c = walk(tree.root_node(), &[call])[0];
        let op = c.child_by_field_name("function").map(|f| text(f, code.as_bytes()).rsplit(['.', ':']).next().unwrap_or("").to_string()).unwrap_or_default();
        starts_at_literal(c, code.as_bytes(), strings, "arguments", crate::effects::on_receiver(&op))
    }

    #[test]
    fn a_place_is_fixed_when_it_starts_at_a_literal() {
        let py = |code: &str| fixed(tree_sitter_python::LANGUAGE.into(), code, "call", &["string"]);
        assert!(py("open('out.csv', 'w')"));
        assert!(py("pd.read_csv(Path('data') / name)"));
        assert!(py("Path('data/x.csv').read_text()"));
        assert!(py("df.to_csv(f'out/{name}.csv')"));
        assert!(py("requests.get(url='https://example.org')"));
        assert!(!py("open(path, 'w')"));
        assert!(!py("pd.read_csv(root / 'x.csv')"));
        assert!(!py("df.to_csv(f'{root}/x.csv')"));
        assert!(!py("self.path.read_text()"));
        assert!(!py("cache_dir.glob(f'*_{tag}')"), "a pattern below a directory handed in");
        assert!(!py("p.write_text('done')"), "what is written is not where");
        let rs = |code: &str| fixed(tree_sitter_rust::LANGUAGE.into(), code, "call_expression", &["string_literal", "raw_string_literal"]);
        assert!(rs("fn f() { std::fs::read_to_string(\"Cargo.toml\"); }"));
        assert!(rs("fn f() { Path::new(\"a.toml\").exists(); }"));
        assert!(!rs("fn f() { std::fs::read_to_string(root.join(\"Cargo.toml\")); }"));
        assert!(!rs("fn f() { std::fs::write(&out, html); }"));
    }
}
