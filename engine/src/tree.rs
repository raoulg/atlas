//! Small helpers over tree-sitter nodes, shared by the language modules.

use std::path::Path;

use tree_sitter::Node as TsNode;

/// A path as the graph writes it: `/` between its parts on every system, so a graph reads the
/// same wherever it was made.
pub fn path_str(p: &Path) -> String {
    let s = p.to_string_lossy();
    if cfg!(windows) {
        s.replace('\\', "/")
    } else {
        s.into_owned()
    }
}

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

/// The expressions of a language that hand on the value inside them: parentheses, and the
/// language's casts, assertions and unwrappings (`(x as T)!`, `&x`, `x?`, `await x`). Atlas reads a
/// value through them as through an identity, as it reads an `Option`, a `Result` or a promise as
/// what it holds. Each reader names its language's.
pub type Wrappers = &'static [&'static str];

/// The value a wrapper hands on: its `value` (`&mut x`), else its first named child.
pub fn wrapped<'t>(n: TsNode<'t>) -> Option<TsNode<'t>> {
    n.child_by_field_name("value").or_else(|| named_children(n).into_iter().next())
}

/// The expression a value is, beneath its wrappers: `x` of `(x as T)!`.
pub fn beneath<'t>(n: TsNode<'t>, wrappers: Wrappers) -> TsNode<'t> {
    let mut n = n;
    while wrappers.contains(&n.kind()) {
        match wrapped(n) {
            Some(x) => n = x,
            None => break,
        }
    }
    n
}

/// The outermost wrapper an expression stands in, or the expression itself: `(x as T)!` of `x`.
pub fn outermost<'t>(n: TsNode<'t>, wrappers: Wrappers) -> TsNode<'t> {
    let mut n = n;
    while let Some(p) = n.parent().filter(|p| wrappers.contains(&p.kind()) && wrapped(*p).map(|x| x.id()) == Some(n.id())) {
        n = p;
    }
    n
}

/// Pre-order, source order.
/// What an expression hands a call, by the names in it: each name with the type the reader
/// knows for it, or by its name alone where an earlier step of the body made it. A name the
/// reader knows nothing of (a function, a module, a loop variable) hands nothing; `skip` are the
/// body's own receiver (`self`, `this`), which a call on it does not hand.
pub fn held_names(
    e: TsNode,
    src: &[u8],
    skip: &[&str],
    ty: &dyn Fn(&str) -> Option<String>,
    made: &std::collections::HashSet<String>,
) -> Vec<crate::model::Held> {
    held_at(e, src, skip, &|i| (text(i, src).to_string(), ty(text(i, src))), made)
}

/// What an expression hands a call, as `held_names` reads it, each name read where it stands:
/// `holder` is handed the name's node and gives the name that holds its value there and that
/// value's type, so that a name bound twice is typed by the binding it uses, and a name bound to
/// another (`let record: Record = result?`) hands the value that one holds.
pub fn held_at(
    e: TsNode,
    src: &[u8],
    skip: &[&str],
    holder: &dyn Fn(TsNode) -> (String, Option<String>),
    made: &std::collections::HashSet<String>,
) -> Vec<crate::model::Held> {
    let mut out: Vec<crate::model::Held> = vec![];
    for i in walk(e, &["identifier", "shorthand_property_identifier"]) {
        if skip.contains(&text(i, src)) {
            continue;
        }
        let (name, ty) = holder(i);
        let held = match ty {
            Some(t) => crate::model::Held { ty: t, name, at: i.start_byte() as u32, fills: vec![] },
            None if made.contains(&name) => crate::model::Held { ty: String::new(), name, at: i.start_byte() as u32, fills: vec![] },
            None => continue,
        };
        if !out.contains(&held) {
            out.push(held);
        }
    }
    out
}

/// Whether a body holds a node `is` picks out, outside the definitions of kind `nested` it holds.
pub fn holds(body: TsNode, src: &[u8], is: fn(TsNode, &[u8]) -> bool, nested: &[&str]) -> bool {
    let mut stack = vec![body];
    while let Some(n) = stack.pop() {
        if is(n, src) {
            return true;
        }
        stack.extend(named_children(n).into_iter().filter(|k| !nested.contains(&k.kind())));
    }
    false
}

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
    /// Is this choice a guard: none of its arms runs on past its end (each ends in a `return`, a `raise`, a `continue`
    /// or a `break`) and it has no arm for when none is chosen. What follows it in its block runs only when none is,
    /// so it is the guard's last arm (`if not xs: return Failed(..)` then `return Fit(..)`).
    pub guards: fn(TsNode, &[u8]) -> bool,
}

/// The arm that follows a guard in its block: the one after the guard's own arms.
fn after_guard(g: TsNode, src: &[u8], ids: &std::collections::HashMap<usize, u32>, syn: &ChoiceSyntax) -> u32 {
    let k = ids.get(&g.id());
    walk_all(g)
        .into_iter()
        .filter(|x| ids.get(&x.id()) == k)
        .flat_map(|x| {
            named_children(x).into_iter().filter_map(move |c| match (syn.part)(x, c, src) {
                Part::Arm(a) => Some(a),
                _ => None,
            })
        })
        .max()
        .map_or(0, |a| a + 1)
}

/// The choice a statement of a block is: the statement itself, or the one choice it is made of (an
/// expression written as a statement, `if .. { .. }` in a block).
fn choice_of<'t>(s: TsNode<'t>, ids: &std::collections::HashMap<usize, u32>) -> Option<TsNode<'t>> {
    if ids.contains_key(&s.id()) {
        return Some(s);
    }
    let c = s.named_child(0).filter(|c| s.named_child_count() == 1 && c.start_byte() == s.start_byte())?;
    ids.contains_key(&c.id()).then_some(c)
}

/// The statement of its block a choice stands as: itself, or the statement made of it alone.
fn statement_of(g: TsNode) -> TsNode {
    match g.parent() {
        Some(p) if p.named_child_count() == 1 && p.start_byte() == g.start_byte() => p,
        _ => g,
    }
}

/// The guards before `n` in its block, the nearest first, each with its choice and the arm `n` is in.
pub fn guarded_by(n: TsNode, src: &[u8], ids: &std::collections::HashMap<usize, u32>, syn: &ChoiceSyntax) -> Vec<(u32, u32)> {
    let mut out = vec![];
    let mut s = n.prev_named_sibling();
    while let Some(st) = s {
        if let Some(g) = choice_of(st, ids).filter(|g| (syn.guards)(*g, src)) {
            out.push((ids[&g.id()], after_guard(g, src, ids, syn)));
        }
        s = st.prev_named_sibling();
    }
    out
}

/// The guards of a body, each with its choice, the arm what follows it is in, and the node that arm starts and ends at.
pub fn guards<'t>(
    body: TsNode<'t>,
    src: &[u8],
    ids: &std::collections::HashMap<usize, u32>,
    syn: &ChoiceSyntax,
) -> Vec<(u32, u32, TsNode<'t>, TsNode<'t>)> {
    let mut out = vec![];
    for g in walk_all(body).into_iter().filter(|g| ids.contains_key(&g.id()) && (syn.guards)(*g, src)) {
        let s = statement_of(g);
        let (Some(first), Some(last)) =
            (s.next_named_sibling(), s.parent().and_then(|p| p.named_child(p.named_child_count().saturating_sub(1))))
        else {
            continue;
        };
        out.push((ids[&g.id()], after_guard(g, src, ids, syn), first, last));
    }
    out
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

/// The arms of the choices `n` sits in, from the outermost in: of each choice that holds it, and of each guard before it
/// in a block that holds it.
pub fn arms_of(n: TsNode, body: TsNode, src: &[u8], ids: &std::collections::HashMap<usize, u32>, syn: &ChoiceSyntax) -> Vec<(u32, u32)> {
    let mut out = vec![];
    let mut child = n;
    while child.id() != body.id() {
        let Some(p) = child.parent() else { break };
        out.extend(guarded_by(child, src, ids, syn));
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

/// How a language writes a comparison of one value with its cases: `mode == "train"`,
/// `mode in {"a", "b"}`, `mode is Mode.FAST`, a `match` or a `switch` over them.
pub struct CompareSyntax {
    /// The kinds of node that may compare.
    pub kinds: &'static [&'static str],
    /// A test of equality, identity or membership: its two operands.
    pub operands: for<'t> fn(TsNode<'t>, &[u8]) -> Option<(TsNode<'t>, TsNode<'t>)>,
    /// The cases an operand writes: itself, or the elements of a set of them (`{"a", "b"}`).
    pub each: for<'t> fn(TsNode<'t>) -> Vec<TsNode<'t>>,
    /// The literal a case is: a string with nothing spliced into it.
    pub literal: fn(TsNode, &[u8]) -> Option<String>,
    /// Where a case is written as a member of something (`Mode.FAST`, `Mode::Fast`): the bytes of
    /// what it is a member of.
    pub member: fn(TsNode) -> Option<(usize, usize)>,
    /// Whether an operand is a value a body can test: a name, a member, an item.
    pub subject: fn(TsNode) -> bool,
    /// A choice over cases (`match`, `switch`): the value as written, the nodes of its cases, and
    /// the node that holds the cases when the choice is numbered there rather than where it is
    /// decided.
    pub over: for<'t> fn(TsNode<'t>, &[u8]) -> Option<(String, Vec<TsNode<'t>>, Option<TsNode<'t>>)>,
    /// A binding that makes a name stand for a value (`m = args.mode`, `let m = &self.mode`): the
    /// name and the value.
    pub copies: for<'t> fn(TsNode<'t>) -> Option<(TsNode<'t>, TsNode<'t>)>,
    /// The value an expression reads in place, with no call between: a name, or a field or an item
    /// of one, also through a reference.
    pub place: for<'t> fn(TsNode<'t>) -> Option<TsNode<'t>>,
    /// What binds names in a body: the language's `UseSyntax::binds`.
    pub binds: fn(TsNode) -> Option<(TsNode, TsNode)>,
}

/// What a reader knows of a body beyond its syntax: the name the body holds the value its method
/// runs on by, and whether the bytes of a qualifier write a type (a class, an enumeration).
pub struct Scope<'a> {
    pub receiver: Option<&'a str>,
    pub is_type: &'a dyn Fn((usize, usize)) -> bool,
}

/// One case a comparison names.
enum Case {
    /// `"fast"`, unquoted.
    Literal(String),
    /// `Mode.FAST`, as written.
    Member(String),
}

/// The case a node writes: a literal, or a member of a type.
fn case(n: TsNode, src: &[u8], cmp: &CompareSyntax, scope: &Scope) -> Option<Case> {
    if let Some(l) = (cmp.literal)(n, src) {
        return Some(Case::Literal(l));
    }
    (cmp.member)(n).filter(|q| (scope.is_type)(*q)).map(|_| Case::Member(text(n, src).to_string()))
}

/// The literals and members an operand names, when it names nothing else.
fn cases_of(n: TsNode, src: &[u8], cmp: &CompareSyntax, scope: &Scope) -> Option<(Vec<String>, Vec<String>)> {
    let each = (cmp.each)(n);
    if each.is_empty() {
        return None;
    }
    let (mut literals, mut members) = (vec![], vec![]);
    for c in each {
        match case(c, src, cmp, scope)? {
            Case::Literal(l) => literals.push(l),
            Case::Member(m) => members.push(m),
        }
    }
    Some((literals, members))
}

/// The name a value is written from: `args` in `args.mode`, `this` in `this.mode`.
pub fn root_of(subject: &str) -> &str {
    subject.split(|c: char| !(c.is_alphanumeric() || c == '_')).next().unwrap_or("")
}

/// The names a body binds once, to a value it reads in place: each stands for that value
/// (`m = args.mode` makes `m` stand for `args.mode`). A name bound again stands for none.
fn stand_ins(body: TsNode, src: &[u8], cmp: &CompareSyntax) -> std::collections::HashMap<String, String> {
    let mut bound: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    let mut copies = std::collections::HashMap::new();
    for n in walk_all(body) {
        if let Some((target, _)) = (cmp.binds)(n) {
            // the names a target binds, not those of a field it keeps the value in (`self.m = v`)
            for i in walk(target, &["identifier"]) {
                let (mut p, mut kept) = (i, false);
                while p.id() != target.id() {
                    let Some(up) = p.parent() else { break };
                    kept = (cmp.place)(up).is_some();
                    if kept {
                        break;
                    }
                    p = up;
                }
                if !kept {
                    *bound.entry(text(i, src)).or_default() += 1;
                }
            }
        }
        if let Some((name, value)) = (cmp.copies)(n).filter(|(name, _)| name.kind() == "identifier") {
            if let Some(v) = (cmp.place)(value) {
                copies.insert(text(name, src).to_string(), text(v, src).to_string());
            }
        }
    }
    copies.retain(|name, _| bound.get(name.as_str()) == Some(&1));
    copies
}

/// A subject as the body was handed it: each name that stands for a value read as that value.
fn handed(subject: &str, stand_ins: &std::collections::HashMap<String, String>) -> String {
    let mut s = subject.to_string();
    for _ in 0..8 {
        let root = root_of(&s);
        match stand_ins.get(root).filter(|v| root_of(v) != root) {
            Some(v) => s = format!("{v}{}", &s[root.len()..]),
            None => break,
        }
    }
    s
}

/// Where a body compares one value with its cases (literals, members of a type), and the choice
/// each comparison decides.
pub fn compares(
    body: TsNode,
    src: &[u8],
    choices: &std::collections::HashMap<usize, u32>,
    cmp: &CompareSyntax,
    syn: &ChoiceSyntax,
    scope: &Scope,
) -> Vec<crate::model::Compare> {
    let stand_ins = stand_ins(body, src, cmp);
    let compare = |written: &str, (cases, members): (Vec<String>, Vec<String>), line: u32, choice: Option<u32>| {
        let subject = handed(written, &stand_ins);
        crate::model::Compare {
            receiver: scope.receiver.is_some_and(|r| root_of(&subject) == r),
            written: if subject == written { String::new() } else { written.to_string() },
            subject,
            cases,
            members,
            line,
            choice,
        }
    };
    let mut out = vec![];
    for n in walk(body, cmp.kinds) {
        let line = n.start_position().row as u32 + 1;
        if let Some((subject, nodes, held)) = (cmp.over)(n, src) {
            let (mut cases, mut members) = (vec![], vec![]);
            for c in nodes.into_iter().filter_map(|c| case(c, src, cmp, scope)) {
                match c {
                    Case::Literal(l) => cases.push(l),
                    Case::Member(m) => members.push(m),
                }
            }
            if !cases.is_empty() || !members.is_empty() {
                let choice = match held {
                    Some(b) => choices.get(&b.id()).copied(),
                    None => decides(n, body, src, choices, syn),
                };
                out.push(compare(&subject, (cases, members), line, choice));
            }
            continue;
        }
        let Some((l, r)) = (cmp.operands)(n, src) else { continue };
        let (subject, cases) = match (cases_of(l, src, cmp, scope), cases_of(r, src, cmp, scope)) {
            (None, Some(c)) => (l, c),
            (Some(c), None) => (r, c),
            _ => continue,
        };
        if (cmp.subject)(subject) {
            out.push(compare(text(subject, src), cases, line, decides(n, body, src, choices, syn)));
        }
    }
    out
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
/// table, a command), else the first argument's own text, else nothing. All of it: a label is
/// cut where it is drawn.
pub fn what_of(call: TsNode, src: &[u8], string_kinds: &[&str], arg_field: &str) -> String {
    if let Some(lit) = literals(call, src, string_kinds).into_iter().find(|l| !crate::effects::is_mode(l)) {
        return lit.trim().to_string();
    }
    let Some(args) = call.child_by_field_name(arg_field) else { return String::new() };
    named_children(args).into_iter().next().map_or(String::new(), |a| what_in(a, src, string_kinds))
}

/// What one argument names a place by: the first string literal in it, else the name it is
/// written as (`&self.path` is `self.path`).
pub fn what_in(a: TsNode, src: &[u8], string_kinds: &[&str]) -> String {
    if let Some(lit) = literals(a, src, string_kinds).into_iter().find(|l| !crate::effects::is_mode(l)) {
        return lit.trim().to_string();
    }
    match a.kind() {
        "identifier" | "attribute" | "field_expression" | "reference_expression" | "scoped_identifier" => {
            text(a, src).trim_start_matches('&').trim_start_matches("mut ").to_string()
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
        "attribute" | "member_expression" => f.child_by_field_name("object"),
        "field_expression" => f.child_by_field_name("value"),
        _ => None,
    }
}

fn first_arg(args: TsNode) -> Option<TsNode> {
    named_children(args).into_iter().find(|a| a.kind() != "comment")
}

/// Whether an expression's value starts at a literal: the leftmost operand of a `/` or a
/// `+`, the receiver of a method, the first argument of a constructor like `Path(..)`.
pub fn head_literal(e: TsNode, src: &[u8], string_kinds: &[&str]) -> bool {
    let child = |f: &str| e.child_by_field_name(f);
    match e.kind() {
        k if string_kinds.contains(&k) => {
            let lit = unquote(text(e, src));
            // `{root}/x` (Python) and `${root}/x` (a template string) start at a value
            !lit.is_empty() && !lit.starts_with('{') && !lit.starts_with("${")
        }
        "binary_operator" | "binary_expression" => child("left").map_or(false, |l| head_literal(l, src, string_kinds)),
        "keyword_argument" => child("value").map_or(false, |v| head_literal(v, src, string_kinds)),
        "parenthesized_expression" | "reference_expression" => {
            child("value").or_else(|| first_arg(e)).map_or(false, |v| head_literal(v, src, string_kinds))
        }
        "call" | "call_expression" | "new_expression" => {
            let Some(f) = child("function").or_else(|| child("constructor")) else { return false };
            match receiver_of(f) {
                Some(r) => head_literal(r, src, string_kinds),
                // only a path or URL builder starts the place at its argument; what any other
                // function makes of a string (a key, a name) is not known here
                None if crate::knowledge::PATH_MAKERS.contains(&text(f, src)) => {
                    child("arguments").and_then(first_arg).map_or(false, |a| head_literal(a, src, string_kinds))
                }
                None => false,
            }
        }
        "attribute" | "field_expression" | "member_expression" => receiver_of(e).map_or(false, |r| head_literal(r, src, string_kinds)),
        // `format!("out/{}.png", x)`: the template decides
        "macro_invocation" => walk(e, string_kinds).first().map_or(false, |l| head_literal(*l, src, string_kinds)),
        _ => false,
    }
}

/// What flows through a boundary call: the first plain identifier among the arguments that
/// is not the `what` (the path or command), as the author named it, with its type when a
/// local or parameter of that name is known.
pub fn payload_of(call: TsNode, src: &[u8], what: &str, arg_field: &str, locals: &std::collections::HashMap<String, String>) -> String {
    let Some(args) = call.child_by_field_name(arg_field) else { return String::new() };
    for id in walk(args, &["identifier"]) {
        let bad_parent = id.parent().map_or(false, |p| {
            matches!(
                p.kind(),
                "scoped_identifier"
                    | "attribute"
                    | "field_expression"
                    | "generic_function"
                    | "keyword_argument"
                    | "call_expression"
                    | "call"
            )
        });
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
        let op = c
            .child_by_field_name("function")
            .map(|f| text(f, code.as_bytes()).rsplit(['.', ':']).next().unwrap_or("").to_string())
            .unwrap_or_default();
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

    /// What a call touches is read whole: two long commands that start alike stay two.
    #[test]
    fn what_a_call_touches_is_not_cut() {
        let mut p = tree_sitter::Parser::new();
        p.set_language(&tree_sitter_python::LANGUAGE.into()).unwrap();
        let mut what = |code: &str| {
            let tree = p.parse(code, None).unwrap();
            what_of(walk(tree.root_node(), &["call"])[0], code.as_bytes(), &["string"], "arguments")
        };
        let since = what("subprocess.run('git log --format=%H --since=yesterday --reverse')");
        assert_eq!(since, "git log --format=%H --since=yesterday --reverse");
        assert_ne!(since, what("subprocess.run('git log --format=%H --until=today --reverse')"));
        assert_eq!(
            what("open(config.paths.templates_directory_of_the_creation_wizard)"),
            "config.paths.templates_directory_of_the_creation_wizard"
        );
    }
}
