//! A function's body as Rust reads it: its steps in the order they run, the calls among a macro's
//! tokens, what its calls touch, and the syntax of its choices and comparisons.

use std::collections::{HashMap, HashSet};

use indexmap::IndexMap;
use tree_sitter::Node as TsNode;

use super::module::{
    bound_in, changes_inside, def_node, expand, path_parts, qualified, resolve, resolve_value, shadowed, type_names, Module, OR_ELSE, THEN,
    WRAPPERS,
};
use super::paths::{self, Arg, At, Eval};
use crate::body::{Bindings, Crosses, Range, Step, Steps, UseSyntax};
use crate::builder::{foreign_type, GraphBuilder};
use crate::effects::{self, Crossing};
use crate::knowledge::{self, OwnTypes};
use crate::model::{Bucket, EdgeKind, Held, Node, NodeKind, Slot, Touch, World};
use crate::tree::{
    beneath, evaluation_order, literals, named_children, nth_of_kind, path_maker_named, text, unquote, walk, wrapped, ChoiceSyntax,
    CompareSyntax, Part,
};

/// The macros that end a body by panicking.
const PANICS: &[&str] = &["panic", "assert", "assert_eq", "assert_ne", "unreachable", "todo", "unimplemented"];

/// A `panic!` and the macros that panic when what they check does not hold.
fn panics(n: TsNode, src: &[u8]) -> bool {
    n.kind() == "macro_invocation" && n.child_by_field_name("macro").map_or(false, |m| PANICS.contains(&text(m, src)))
}

/// A function on a type, by the casing Rust's names follow (C-CASE): `File::open`, not
/// `fs::read_to_string`. What it returns is a value of that type, or one wrapping it.
fn on_type(parts: &[String]) -> bool {
    parts.len() > 1 && parts[parts.len() - 2].starts_with(|c: char| c.is_ascii_uppercase())
}

/// A library value a method is called on, traced to the call that made it.
struct Made<'t> {
    /// the type's path: `reqwest.Client`
    ty: Vec<String>,
    /// the making call's path: `reqwest.Client.new`
    ctor: Vec<String>,
    /// the method calls on it before a local held it: `append`, `open` for `f` in
    /// `let f = OpenOptions::new().append(true).open(p)?`
    before: Vec<(String, TsNode<'t>)>,
    /// the method calls on it in this expression, before this one
    here: Vec<(String, TsNode<'t>)>,
    /// where a local was made, when one holds the value
    origin: Option<TsNode<'t>>,
}

/// What an operation on a value of a library's type `ty` moves: a read of the path the value
/// holds (`read_to_string`), or what the effect tables say of `ty::op`; nothing for opening what
/// a call gave (`unwrap`).
fn moved_by(ty: &[String], op: &str) -> Option<Crossing> {
    match op {
        _ if knowledge::hands_on(op) => None,
        _ if knowledge::RUST_PATH_READS.contains(&op) => Some(Crossing::read(World::Disk)),
        _ => effects::boundary_rust(&[ty, &[op.to_string()]].concat()),
    }
}

/// The string literals under a node that can name a place: a closure's are its own code's, and a
/// message said when a value is missing (`expect`) names none.
fn place_literals(n: TsNode, src: &[u8]) -> Vec<String> {
    // a message said when the value is missing names no place: only what the method is called on
    let method = function(n).filter(|f| f.kind() == "field_expression");
    if method.and_then(|f| f.child_by_field_name("field")).is_some_and(|m| knowledge::RUST_MESSAGES.contains(&text(m, src))) {
        return method.and_then(|f| f.child_by_field_name("value")).map(|v| place_literals(v, src)).unwrap_or_default();
    }
    match n.kind() {
        "closure_expression" => vec![],
        "string_literal" | "raw_string_literal" => vec![unquote(text(n, src))],
        _ => named_children(n).into_iter().flat_map(|k| place_literals(k, src)).collect(),
    }
}

/// What a call calls: its function, and through a turbofish the method it names (`collect` of
/// `rows.collect::<Vec<_>>()`), as the call without it writes it.
fn function(c: TsNode) -> Option<TsNode> {
    let f = c.child_by_field_name("function")?;
    match f.child_by_field_name("function").filter(|m| f.kind() == "generic_function" && m.kind() == "field_expression") {
        Some(method) => Some(method),
        None => Some(f),
    }
}

/// Whether a string literal is the format string of a macro that formats: a token of its own
/// among the macro's tokens (`format!("{key}")`, also inside another macro's tokens).
fn formats(lit: TsNode, src: &[u8]) -> bool {
    let Some(t) = lit.parent().filter(|t| t.kind() == "token_tree") else { return false };
    let name = match t.parent() {
        Some(m) if m.kind() == "macro_invocation" => m.child_by_field_name("macro").and_then(|x| path_parts(Some(x), src).pop()),
        _ => t.prev_sibling().filter(|b| b.kind() == "!").and_then(|b| b.prev_sibling()).map(|x| text(x, src).to_string()),
    };
    name.map_or(false, |n| knowledge::RUST_FORMAT_MACROS.contains(&n.as_str()))
}

/// The names a format string writes in its braces, each with its byte offset in the literal:
/// `key` and `low` of `"{key} {low:.1}"`; `{{` is a brace, `{}` and `{0}` name nothing.
fn inline_names(s: &str) -> Vec<(&str, usize)> {
    let b = s.as_bytes();
    let (mut out, mut i) = (vec![], 0);
    while i < b.len() {
        if b[i] == b'{' && b.get(i + 1) == Some(&b'{') {
            i += 2;
            continue;
        }
        if b[i] == b'{' {
            let start = i + 1;
            let mut j = start;
            while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
                j += 1;
            }
            let ident = j > start && !b[start].is_ascii_digit();
            if ident && matches!(b.get(j), Some(b'}' | b':')) {
                out.push((&s[start..j], start));
            }
            i = j.max(i + 1);
            continue;
        }
        i += 1;
    }
    out
}

/// The kinds of node a pattern is built of, between a name it binds and what binds it.
const PATTERNS: &[&str] = &[
    "identifier",
    "shorthand_field_identifier",
    "mut_pattern",
    "ref_pattern",
    "reference_pattern",
    "captured_pattern",
    "or_pattern",
    "tuple_pattern",
    "tuple_struct_pattern",
    "struct_pattern",
    "field_pattern",
    "slice_pattern",
    "match_pattern",
    "parameter",
];

/// Where a name a pattern binds is visible, as Rust scopes it: a `let`'s from its end to the end of
/// its block, a parameter's over its function, a closure's over its body, a loop's over its body, an
/// arm's over its guard and its value, an `if let`'s or a `while let`'s over the block it opens.
fn scope(pat: TsNode) -> Option<(usize, usize)> {
    let mut n = pat;
    loop {
        let p = n.parent()?;
        match p.kind() {
            "let_declaration" => return Some((p.end_byte(), p.parent()?.end_byte())),
            "parameters" => return Some((p.parent()?.start_byte(), p.parent()?.end_byte())),
            "closure_parameters" => return Some((p.end_byte(), p.parent()?.end_byte())),
            "for_expression" => {
                let body = p.child_by_field_name("body")?;
                return Some((body.start_byte(), body.end_byte()));
            }
            "match_arm" => return Some((n.end_byte(), p.end_byte())),
            "let_condition" => {
                let mut c = p.parent()?;
                while c.kind() == "let_chain" {
                    c = c.parent()?;
                }
                let block = c.child_by_field_name(if c.kind() == "while_expression" { "body" } else { "consequence" })?;
                return Some((p.end_byte(), block.end_byte()));
            }
            k if PATTERNS.contains(&k) => n = p,
            _ => return None,
        }
    }
}

/// A name a pattern binds, as written: `x` of `mut x`, `&x` or `ref x`.
fn bound_name<'s>(pat: TsNode, src: &'s [u8]) -> &'s str {
    text(pat, src).trim_start_matches('&').trim_start_matches("ref ").trim_start_matches("mut ").trim()
}

/// The names a call's value is kept by, each with the byte where it holds the value. A name a
/// binding gives it (`let x = f(..)?`, `x = f(..)`, a pattern: `let (a, b) = g(..)`, `if let
/// Some(line) = f(..)`, the patterns of a `match` on it), through what an arm of a choice gives
/// (`let x = if c { Some(f(..)) } else { None }`: the last expression of a block, a `match` arm,
/// `Some`, `Ok` and `Err`), and element by element from a tuple bound to a tuple pattern (`let
/// (a, b) = (f(), g())`). Of a chain of calls only the last keeps what it gives under the name: what
/// a call gives that another is made on goes into that call (`read_to_string(p).expect(..)`).
fn kept_as(c: TsNode, src: &[u8]) -> Vec<(String, usize)> {
    let mut n = c;
    loop {
        let Some(p) = n.parent() else { return vec![] };
        let is = |field: &str| p.child_by_field_name(field).map(|x| x.id()) == Some(n.id());
        match p.kind() {
            k if WRAPPERS.contains(&k) || k == "else_clause" => n = p,
            "match_expression" if is("body") => n = p,
            "block"
                if p.named_child(p.named_child_count().saturating_sub(1)).map(|x| x.id()) == Some(n.id())
                    && n.kind() != "expression_statement" =>
            {
                n = p
            }
            "arguments"
                if p.named_child_count() == 1
                    && p.parent().and_then(|c| function(c)).map_or(false, |f| matches!(text(f, src), "Some" | "Ok" | "Err")) =>
            {
                match p.parent() {
                    Some(call) => n = call,
                    None => return vec![],
                }
            }
            // a `let .. else` decides by what it binds, and keeps what it binds (`let Some(n) = f(..) else { .. }`)
            "let_declaration" | "let_condition" if is("value") => {
                return p.child_by_field_name("pattern").map_or(vec![], |x| binds(x, src))
            }
            _ if (CHOICES.chooses)(p, src) && !matches!((CHOICES.part)(p, n, src), Part::Decides) => n = p,
            // what a `match` takes apart is kept by the names its arms' patterns bind
            "match_expression" if is("value") => {
                return p
                    .child_by_field_name("body")
                    .map(named_children)
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|arm| arm.child_by_field_name("pattern"))
                    .flat_map(|x| binds(x, src))
                    .collect()
            }
            "tuple_expression" => {
                let at = named_children(p).iter().position(|x| x.id() == n.id());
                let Some(b) = p.parent().filter(|b| matches!(b.kind(), "let_declaration" | "let_condition")) else { return vec![] };
                let Some(pat) = b.child_by_field_name("pattern").filter(|x| x.kind() == "tuple_pattern") else { return vec![] };
                let parts = named_children(pat);
                return match at {
                    Some(i) if parts.len() == named_children(p).len() && b.child_by_field_name("value").map(|v| v.id()) == Some(p.id()) => {
                        binds(parts[i], src)
                    }
                    _ => vec![],
                };
            }
            "assignment_expression" if is("right") => {
                return p
                    .child_by_field_name("left")
                    .filter(|l| l.kind() == "identifier")
                    .map(|l| (text(l, src).to_string(), p.start_byte()))
                    .into_iter()
                    .collect();
            }
            _ => return vec![],
        }
    }
}

/// The names a pattern binds, each with the byte from which it is visible: not the variant a
/// pattern names (`Some`, `Mode::Fast`, `None`: a name Rust writes as a type's), and not what a
/// guard tests.
fn binds(pat: TsNode, src: &[u8]) -> Vec<(String, usize)> {
    bound_names(pat, src).into_iter().filter_map(|i| Some((text(i, src).to_string(), scope(i)?.0))).collect()
}

/// The names a pattern binds, as nodes: not the variant a pattern names, not what a guard tests.
fn bound_names<'t>(pat: TsNode<'t>, src: &[u8]) -> Vec<TsNode<'t>> {
    let guard = pat.child_by_field_name("condition").map(|g| g.byte_range());
    let variant = |i: TsNode| {
        i.parent().map_or(false, |p| {
            matches!(p.kind(), "scoped_identifier" | "scoped_type_identifier")
                || (matches!(p.kind(), "tuple_struct_pattern" | "struct_pattern")
                    && p.child_by_field_name("type").map(|t| t.id()) == Some(i.id()))
        })
    };
    walk(pat, &["identifier", "shorthand_field_identifier"])
        .into_iter()
        .filter(|i| guard.as_ref().map_or(true, |g| !g.contains(&i.start_byte())))
        .filter(|i| !variant(*i) && !names_type(text(*i, src)))
        .collect()
}

/// What a part of a value Rust's own code computes is made of (`body::computed_of`): a literal, a type and a path of none (a
/// constant by its path, `f32::EPSILON`, names no value of the body's, as it is handed none written in place), any other
/// part the name it is a part of (`root`).
fn part(p: TsNode) -> Option<Vec<TsNode>> {
    match p.kind() {
        "integer_literal" | "float_literal" | "boolean_literal" | "string_literal" | "raw_string_literal" | "char_literal"
        | "line_comment" | "block_comment" | "scoped_identifier" => Some(vec![]),
        k if k.ends_with("type") || k.ends_with("type_identifier") => Some(vec![]),
        _ => root(p).map(|r| vec![r]),
    }
}

/// The name a value is a part of, where the code writes it so: through what hands a value on
/// unchanged, a field and an index (`settings.months`, `&config["base"]`, `row?`), the name or the
/// `self` it starts at; none for a value a call or a literal makes.
fn root(v: TsNode) -> Option<TsNode> {
    let mut v = beneath(v, WRAPPERS);
    loop {
        v = match v.kind() {
            "identifier" | "self" => return Some(v),
            "field_expression" => beneath(v.child_by_field_name("value")?, WRAPPERS),
            "index_expression" => beneath(v.named_child(0)?, WRAPPERS),
            _ => return None,
        };
    }
}

/// The value an expression reaches through what hands a value on (`x.unwrap()`, `x?`) and the
/// guards a cell gives on what it holds (`log.borrow_mut()`, `m.lock()`): `log` of
/// `log.borrow_mut()`, through which a change of a field changes the log.
fn opened<'t>(v: TsNode<'t>, src: &[u8]) -> TsNode<'t> {
    let mut v = beneath(v, WRAPPERS);
    while let Some(f) = function(v).filter(|f| f.kind() == "field_expression") {
        let method = f.child_by_field_name("field").map_or("", |m| text(m, src));
        match f.child_by_field_name("value") {
            Some(on) if knowledge::rust_guards(method) || knowledge::hands_on(method) => v = beneath(on, WRAPPERS),
            _ => break,
        }
    }
    v
}

/// The names what a method hands the function it is given is a part of, where the method hands it
/// each element of the value it runs on or what an `Option` holds (`knowledge::RUST_EACH`), through
/// the methods that go over its parts: `series` of `series.windows(m).map(..)`, whose windows `map`
/// hands its closure, `text` of `text.lines().filter_map(..)`. Where an adaptor goes on with the
/// elements of what it is handed (`a.iter().chain(&b)`), an element is a part of either: `a`, then
/// `b`. None for a method that hands no element, or a value no name holds.
fn each_of<'t>(call: TsNode<'t>, src: &[u8]) -> Vec<TsNode<'t>> {
    let on = method_on(call, src).filter(|(m, _)| knowledge::rust_element(m).is_some()).map(|(_, v)| v);
    on.map_or(vec![], |on| elements_of(on, src))
}

/// The parameters of a closure that a method hands each element of what it runs on (`knowledge::RUST_EACH`), each
/// with the value the method runs on: `t` and `n.touches.iter()` of `n.touches.iter().map(|t| t.drawn())`, `x` and
/// not `n` of `xs.iter().fold(0, |n, x| ..)`, whose first is what it gathers. A parameter that writes its type
/// (`|t: &Touch|`) is given as the `parameter` it is.
fn element_parameters<'t>(closure: TsNode<'t>, src: &[u8]) -> Vec<(TsNode<'t>, TsNode<'t>)> {
    let Some(call) = closure.parent().filter(|a| a.kind() == "arguments").and_then(|a| a.parent()) else { return vec![] };
    let Some((at, on)) = method_on(call, src).and_then(|(m, v)| Some((knowledge::rust_element(m)?, v))) else { return vec![] };
    let params = closure.child_by_field_name("parameters").map(named_children).unwrap_or_default();
    params.into_iter().filter(|p| !p.kind().ends_with("comment")).skip(at as usize).map(|p| (p, on)).collect()
}

/// The method a call makes on a value, by its name, and the value: `map` and `xs.iter()` of
/// `xs.iter().map(..)`.
fn method_on<'t, 's>(call: TsNode<'t>, src: &'s [u8]) -> Option<(&'s str, TsNode<'t>)> {
    function(call)
        .filter(|f| f.kind() == "field_expression")
        .and_then(|f| Some((text(f.child_by_field_name("field")?, src), f.child_by_field_name("value")?)))
}

/// The names the elements of `on` are parts of, through the methods that go over its parts and keep
/// its elements, and through what an adaptor joins after them (`each_of`).
fn elements_of<'t>(mut on: TsNode<'t>, src: &[u8]) -> Vec<TsNode<'t>> {
    let mut joined = vec![];
    while let Some((m, v)) = method_on(beneath(on, WRAPPERS), src) {
        if knowledge::RUST_JOINS_ELEMENTS.contains(&m) {
            joined.extend(positional(beneath(on, WRAPPERS)).into_iter().flatten().flat_map(|a| elements_of(a, src)));
        } else if !(knowledge::RUST_PARTS.contains(&m)
            || knowledge::RUST_ELEMENTS.contains(&m)
            || knowledge::RUST_KEEPS_ELEMENTS.contains(&m)
            || knowledge::hands_on(m))
        {
            break;
        }
        on = v;
    }
    // one name written twice is one value: `n` of `n.touches.iter().chain(&n.instead)`
    let mut out: Vec<TsNode<'t>> = vec![];
    for r in root(on).into_iter().chain(joined) {
        if !out.iter().any(|o| text(*o, src) == text(r, src)) {
            out.push(r);
        }
    }
    out
}

/// The library value a chain of method calls starts from: a call of a function on a library's
/// type (`OpenOptions::new()`), reached through the locals that hold what it made
/// (`c` from `let c = reqwest::Client::new()`), or a value `declared` of a library's type (a
/// parameter `conn: &Connection`, a field `self.conn`), made wherever it was.
fn library_chain<'t>(
    mut v: TsNode<'t>,
    m: &Module,
    src: &[u8],
    made_by: &Bindings<TsNode<'t>>,
    declared: &dyn Fn(TsNode<'t>) -> Option<Vec<String>>,
) -> Option<Made<'t>> {
    let (mut before, mut here, mut origin) = (vec![], vec![], None);
    let mut locals = 0;
    loop {
        match v.kind() {
            k if WRAPPERS.contains(&k) => v = wrapped(v)?,
            // a dereference is the value it refers to: `conn` of `(&*conn).read(..)`
            "unary_expression" if v.child(0).map_or(false, |o| o.kind() == "*") => v = named_children(v).into_iter().next()?,
            "identifier" | "field_expression" if !made_by.holds(text(v, src), v.start_byte()) => {
                let ty = declared(v)?;
                before.reverse();
                here.reverse();
                return Some(Made { ty, ctor: vec![], before, here, origin });
            }
            "identifier" if locals < 8 => {
                v = *made_by.get(text(v, src), v.start_byte())?;
                origin.get_or_insert(v);
                locals += 1;
            }
            "call_expression" => {
                let f = function(v)?;
                if f.kind() == "field_expression" {
                    let call = (text(f.child_by_field_name("field")?, src).to_string(), v);
                    if locals == 0 {
                        here.push(call)
                    } else {
                        before.push(call)
                    }
                    v = f.child_by_field_name("value")?;
                    continue;
                }
                let parts = path_parts(Some(f), src);
                if !on_type(&parts) {
                    return None;
                }
                let ctor = expand(m, &parts);
                before.reverse();
                here.reverse();
                return Some(Made { ty: ctor[..ctor.len() - 1].to_vec(), ctor, before, here, origin });
            }
            _ => return None,
        }
    }
}

/// What a parenthesised group among a macro's tokens calls.
enum Called<'t> {
    /// A path: `round2`, `Self::label`, `crate::report::round2`, `Vec::<u8>::new`.
    Path(Vec<String>),
    /// A method, with the names its receiver is reached by (`store` in `store.rows()`, `self`
    /// and `conn` in `self.conn.f()`); none when the receiver is no name (`a.f().g()`).
    Method(Vec<TsNode<'t>>, String),
}

/// The keywords after which a name and a group declare (`fn f(..)`, `struct S(..)`), name a
/// type (`dyn Fn(..)`) or start a pattern (`let Some(x) = ..`): no call. Among a macro's tokens
/// some are names to tree-sitter (`dyn`), so they are known by how they are written.
const NOT_CALLED_AFTER: &[&str] = &["fn", "struct", "enum", "union", "trait", "type", "dyn", "impl", "let"];

/// What a group among a macro's tokens calls: the path or the method written right before it. A
/// macro's tokens are no expression to tree-sitter, so they are read as Rust writes a call: a path
/// followed by a parenthesised group. A group in a pattern calls nothing: one an arm, a binding
/// or a guard follows (`Some(x) =>`, `Some(x) =`), or one where a macro takes a pattern
/// (`matches!(v, Some(_))`).
fn token_callee<'t>(g: TsNode<'t>, src: &[u8]) -> Option<Called<'t>> {
    if g.child(0).map(|x| x.kind()) != Some("(") || !in_macro(g) || in_pattern(g, src) {
        return None;
    }
    if g.next_sibling().map_or(false, |n| matches!(n.kind(), "=>" | "=" | "if" | "@")) {
        return None;
    }
    let mut name = g.prev_sibling()?;
    if name.kind() == ">" {
        name = turbofish(name)?;
    }
    if name.kind() != "identifier" {
        return None;
    }
    let mut parts = vec![text(name, src).to_string()];
    let mut at = name.prev_sibling();
    while let Some(sep) = at.filter(|t| t.kind() == "::") {
        let mut before = sep.prev_sibling()?;
        if before.kind() == ">" {
            before = turbofish(before)?;
        }
        if !matches!(before.kind(), "identifier" | "self" | "super" | "crate") {
            return None;
        }
        parts.insert(0, text(before, src).to_string());
        at = before.prev_sibling();
    }
    match at {
        Some(dot) if dot.kind() == "." => {
            let [method] = <[String; 1]>::try_from(parts).ok()?;
            Some(Called::Method(receiver(dot), method))
        }
        Some(k) if NOT_CALLED_AFTER.contains(&text(k, src)) => None,
        _ => Some(Called::Path(parts)),
    }
}

/// The names a method's receiver is reached by, from the `.` before the method: `self.conn` ->
/// `self`, `conn`; none when the chain passes through something that is not a name (`a.f().g()`,
/// `Type::X.g()`).
fn receiver(dot: TsNode) -> Vec<TsNode> {
    let mut names = vec![];
    let mut t = dot.prev_sibling();
    while let Some(n) = t {
        if !matches!(n.kind(), "identifier" | "self") {
            return vec![];
        }
        names.insert(0, n);
        match n.prev_sibling() {
            Some(d) if d.kind() == "." => t = d.prev_sibling(),
            Some(d) if d.kind() == "::" => return vec![],
            _ => return names,
        }
    }
    vec![]
}

/// The token before the turbofish that `close` ends (`f` of `f::<T>`); none when `close` ends no
/// turbofish (`a > (b)`).
fn turbofish(close: TsNode) -> Option<TsNode> {
    let mut depth = 0;
    let mut t = Some(close);
    while let Some(x) = t {
        match x.kind() {
            ">" => depth += 1,
            "<" => {
                depth -= 1;
                if depth == 0 {
                    return x.prev_sibling().filter(|s| s.kind() == "::")?.prev_sibling();
                }
            }
            _ => {}
        }
        t = x.prev_sibling();
    }
    None
}

/// Whether a group sits among the tokens of a macro's invocation, below its own arguments.
fn in_macro(g: TsNode) -> bool {
    let mut t = g.parent().filter(|p| p.kind() == "token_tree");
    while let Some(x) = t {
        match x.parent() {
            Some(p) if p.kind() == "token_tree" => t = Some(p),
            Some(p) => return p.kind() == "macro_invocation",
            None => return false,
        }
    }
    false
}

/// Whether a group stands in the pattern of a macro that takes one: after its first argument,
/// before a guard's `if`.
fn in_pattern(g: TsNode, src: &[u8]) -> bool {
    let mut child = g;
    while let Some(t) = child.parent().filter(|t| t.kind() == "token_tree") {
        let name = match t.parent() {
            Some(m) if m.kind() == "macro_invocation" => m.child_by_field_name("macro").and_then(|x| path_parts(Some(x), src).pop()),
            _ => t.prev_sibling().filter(|b| b.kind() == "!").and_then(|b| b.prev_sibling()).map(|x| text(x, src).to_string()),
        };
        if name.map_or(false, |n| knowledge::RUST_PATTERN_MACROS.contains(&n.as_str())) {
            let kids: Vec<TsNode> = (0..t.child_count()).filter_map(|i| t.child(i)).collect();
            let at = kids.iter().position(|k| k.id() == child.id());
            let comma = kids.iter().position(|k| k.kind() == ",");
            let guard = comma.and_then(|c| kids.iter().skip(c).position(|k| text(*k, src) == "if").map(|i| c + i));
            if let (Some(at), Some(comma)) = (at, comma) {
                if at > comma && guard.map_or(true, |g| at < g) {
                    return true;
                }
            }
        }
        child = t;
    }
    false
}

/// The names among a call's tokens that stand for values: not a field after a `.`, not a part of
/// a path, not a macro.
fn token_values(g: TsNode) -> Vec<TsNode> {
    walk(g, &["identifier"])
        .into_iter()
        .filter(|i| {
            !matches!(i.prev_sibling().map(|p| p.kind()), Some("." | "::"))
                && !matches!(i.next_sibling().map(|n| n.kind()), Some("::" | "!"))
        })
        .collect()
}

/// Whether an identifier stands for a value: not a segment of a path (`fs` of `fs::read`, `Amount`
/// of `Error::Amount`), not a macro's name, and among a macro's tokens not a field or a method after
/// a `.` nor a path's segment.
fn names_value(i: TsNode, src: &[u8]) -> bool {
    match i.parent() {
        Some(p) if p.kind() == "scoped_identifier" => false,
        Some(p) if p.kind() == "macro_invocation" => p.child_by_field_name("macro").map(|m| m.id()) != Some(i.id()),
        Some(p) if p.kind() == "token_tree" => {
            !matches!(i.prev_sibling().map(|p| p.kind()), Some("." | "::"))
                && !matches!(i.next_sibling().map(|n| n.kind()), Some("::" | "!"))
                && !names_type(text(i, src))
        }
        _ => true,
    }
}

/// Whether a name among a macro's tokens stands where a value goes, as it would written outside the macro: an argument of
/// its own, by reference or not (`format!("{:?}", Plain)`, `vec![&Plain]`), or the value a method is called on or a field
/// read of (`Plain.name()`); not a segment of a path or a field after a `.`, and not in a pattern the macro takes
/// (`matches!(v, Plain)`).
fn token_value(n: TsNode, src: &[u8]) -> bool {
    let Some(g) = n.parent().filter(|g| g.kind() == "token_tree") else { return false };
    if in_pattern(n, src) || matches!(n.prev_sibling().map(|p| p.kind()), Some("." | "::")) {
        return false;
    }
    let alone = paths::token_groups(g).into_iter().any(|group| {
        let rest: Vec<TsNode> = group.into_iter().skip_while(|t| matches!(t.kind(), "&" | "mut" | "mutable_specifier")).collect();
        matches!(rest.as_slice(), [one] if one.id() == n.id())
    });
    alone || n.next_sibling().is_some_and(|s| s.kind() == ".")
}

/// What a call is handed, by place: its arguments; among a macro's tokens, each argument that is
/// one name or literal (`&x` is `x`), and none for one written another way.
fn positional(c: TsNode) -> Vec<Option<TsNode>> {
    if c.kind() != "token_tree" {
        return c
            .child_by_field_name("arguments")
            .map(named_children)
            .unwrap_or_default()
            .into_iter()
            .filter(|a| !a.kind().ends_with("comment"))
            .map(Some)
            .collect();
    }
    paths::token_groups(c)
        .into_iter()
        .map(|g| {
            let rest: Vec<TsNode> = g.into_iter().skip_while(|t| matches!(t.kind(), "&" | "mut" | "mutable_specifier")).collect();
            match rest.as_slice() {
                [one] if one.is_named() && one.kind() != "token_tree" => Some(*one),
                _ => None,
            }
        })
        .collect()
}

/// What reading a body gives besides its edges and touches: its file and database calls with
/// their paths and statements as built (placed once the parameters' values are known), and the
/// arguments of its calls of the program's functions.
#[derive(Default)]
pub(super) struct Linked<'t> {
    pub(super) waiting: Vec<(Touch, Vec<crate::paths::Built>)>,
    pub(super) args: Vec<Arg<'t>>,
}

pub(super) fn link_calls<'t>(
    modules: &'t IndexMap<String, Module>,
    store: &mut GraphBuilder,
    crate_ids: &HashMap<String, String>,
    eval: &Eval<'t>,
    returned: &HashMap<String, String>,
    id: &str,
) -> Linked<'t> {
    let n = store.nodes[id].clone();
    let m = &modules[&n.module];
    let Some(d) = def_node(m, &n) else { return Linked::default() };
    let Some(body) = d.child_by_field_name("body") else { return Linked::default() };
    let mut steps = Steps::new(body, &m.src, &CHOICES, &USES);
    let read = {
        let mut b = Body::new(modules, store, crate_ids, eval, returned, m, &n, d, body);
        // the error type `?` converts to: the program's type the signature's `Result` names, else none of the code's
        let error = n.returns.as_ref().filter(|r| r.fails).map(|r| {
            let named = crate::body::error_written(&r.written).map(|e| e.split('<').next().unwrap_or(e).trim_start_matches(['&', ' ']));
            let parts = named.map(|e| e.trim_start_matches("dyn ").split("::").map(str::to_string).collect::<Vec<_>>());
            parts.and_then(|p| b.resolve(&p)).and_then(|t| b.class_of(&t)).unwrap_or_default()
        });
        steps.converts(error);
        // a loop binds its element once what it goes over is made, and before its body runs
        let calls =
            evaluation_order(walk(body, &["call_expression", "struct_expression", "identifier", "scoped_identifier", "token_tree"]));
        let mut ran: Vec<((usize, u8), TsNode)> = calls.into_iter().map(|c| ((c.end_byte(), 0), c)).collect();
        let iterations = b.iterations(body);
        let at: HashMap<usize, (TsNode, TsNode)> = iterations.iter().map(|(p, v)| (p.id(), (*p, *v))).collect();
        ran.extend(iterations.iter().map(|(p, v)| ((v.end_byte(), 1), *p)));
        ran.sort_by_key(|(k, _)| *k);
        for ((_, rank), c) in ran {
            if rank == 1 {
                let (pat, value) = at[&c.id()];
                b.each(&mut steps, pat, value);
                continue;
            }
            let (t, w) = (b.read.touches.len(), b.read.waiting.len());
            b.call(&mut steps, c);
            steps.mark(c, b.read.touches[t..].iter_mut().chain(b.read.waiting[w..].iter_mut().map(|(t, _)| t)));
        }
        let kept = steps.kept.clone();
        // among a macro's tokens, the value a call that is a step is made on is that step's
        let stepped = |n: TsNode| {
            n.parent().filter(|p| p.kind() == "token_tree").is_some_and(|p| b.steps_within(p).iter().any(|r| r.contains(&n.start_byte())))
        };
        let held = |n: TsNode<'t>| match n.kind() {
            _ if stepped(n) => vec![],
            "self" => b.cls.iter().map(|k| Held::new(k.clone(), "")).collect(),
            _ => b.hands(&[Some(n)], &kept),
        };
        steps.uses(&held);
        steps.fills(&held);
        b.read
    };
    let receiver = n.params.iter().find(|p| p.receiver).map(|p| p.name.as_str());
    let is_type = |(a, b): (usize, usize)| names_type(&String::from_utf8_lossy(&m.src[a..b]));
    let compares =
        crate::tree::compares(body, &m.src, steps.choices(), &COMPARES, &CHOICES, &crate::tree::Scope { receiver, is_type: &is_type });
    steps.finish(store, id);
    if let Some(node) = store.nodes.get_mut(id) {
        node.compares = compares;
        node.raises = read.exits || crate::tree::holds(body, &m.src, panics, &["function_item", "impl_item", "trait_item", "mod_item"]);
    }
    for (t, input) in read.mutates {
        store.add_edge(id, &t, EdgeKind::Mutates, Some(input));
    }
    if let Some(node) = store.nodes.get_mut(id) {
        node.calls_handed = read.unresolved.iter().any(|(u, _)| n.params.iter().any(|p| &p.name == u));
    }
    for (name, bucket) in read.unresolved {
        store.not_followed(name, bucket);
    }
    for t in read.touches {
        store.touch(id, t);
    }
    Linked { waiting: read.waiting, args: read.args }
}

/// What reading a body finds besides its steps: what it touches, the file and database calls
/// whose paths and statements wait for what callers hand, the calls of the program's functions
/// with their arguments, the calls Atlas could not follow, the inputs it changes in place, and
/// whether it calls what ends the program.
#[derive(Default)]
struct Read<'t> {
    touches: Vec<Touch>,
    waiting: Vec<(Touch, Vec<crate::paths::Built>)>,
    args: Vec<Arg<'t>>,
    unresolved: Vec<(String, Bucket)>,
    mutates: Vec<(String, String)>,
    exits: bool,
}

/// A function's body as Rust reads it: the program around it, and the classes its names hold.
pub(super) struct Body<'a, 't> {
    modules: &'a IndexMap<String, Module>,
    store: &'a GraphBuilder,
    crate_ids: &'a HashMap<String, String>,
    eval: &'a Eval<'t>,
    m: &'t Module,
    src: &'t [u8],
    /// The function the body is of, its id, and its parameters by name.
    id: String,
    def: TsNode<'t>,
    params: Vec<String>,
    cls: Option<String>,
    /// The classes the body's names hold, binding by binding: its typed parameters, and locals
    /// assigned from a constructor, a struct literal or a function with a typed return, or handed
    /// to a parameter that takes one.
    locals: Bindings<String>,
    /// The locals a call made, by the call: `let c = reqwest::Client::new()`.
    made_by: Bindings<TsNode<'t>>,
    /// The names bound `mut`: what a method changes in place.
    mutable: Bindings<()>,
    /// Every name a pattern binds, binding by binding: `true` for the function's own parameters,
    /// each of which holds what its call hands it.
    parameter: Bindings<bool>,
    /// The locals bound to a closure, by the class of what the closure makes: `wrong` of `let wrong
    /// = || Error::Amount(..)` makes an `Error` wherever it is called.
    closures: Bindings<String>,
    /// The locals bound to the value another name holds, through what hands it on unchanged (`let
    /// record: Record = result?`): that name, where it is used.
    aliases: Bindings<(String, usize)>,
    /// The classes the fields of the body's own class hold.
    attr_types: std::collections::BTreeMap<String, String>,
    /// The library types its parameters are declared with: `conn` is `ext:rusqlite.Connection`.
    foreign: Bindings<String>,
    /// The library types its `let`s hold by the parameter they are handed to: `area` is a
    /// `DrawingArea` where `draw(&area, ..)` takes one.
    taken: Bindings<String>,
    /// The class each function gives back, by its signature.
    returned: &'a HashMap<String, String>,
    /// The names the function binds, found when a call not followed first asks.
    bound: std::cell::OnceCell<HashSet<String>>,
    /// The library calls that keep the command line the program is run with under a name: the
    /// end of a chain that starts at reading it (`collect` of `env::args().skip(1).collect()`).
    keeps_command_line: HashSet<usize>,
    /// It gives no value: what the last expression of its block gives goes nowhere.
    unit: bool,
    /// What its loops go over, as written: each `for` loop's value.
    loop_values: HashSet<usize>,
    /// The constants its module declares, by name, each with the class of the program its declared type holds: a `const`,
    /// or a `static` that nothing changes (`const FLOOR: Floor = ..`). A name of one holds what the module holds, a value the
    /// body did not make.
    constants: HashMap<String, String>,
    read: Read<'t>,
}

const STRINGS: &[&str] = &["string_literal", "raw_string_literal"];

impl<'a, 't> Body<'a, 't> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        modules: &'a IndexMap<String, Module>,
        store: &'a GraphBuilder,
        crate_ids: &'a HashMap<String, String>,
        eval: &'a Eval<'t>,
        returned: &'a HashMap<String, String>,
        m: &'t Module,
        n: &Node,
        d: TsNode<'t>,
        body: TsNode<'t>,
    ) -> Self {
        let cls = n.cls.clone();
        let attr_types = cls.as_ref().and_then(|c| store.classes.get(c)).map(|c| c.attr_types.clone()).unwrap_or_default();
        let params = paths::param_names(d, &m.src);
        let mut b = Body {
            modules,
            store,
            crate_ids,
            eval,
            m,
            src: &m.src,
            id: n.id.clone(),
            def: d,
            params,
            cls,
            locals: Bindings::default(),
            made_by: Bindings::default(),
            mutable: Bindings::default(),
            parameter: Bindings::default(),
            closures: Bindings::default(),
            aliases: Bindings::default(),
            attr_types,
            foreign: Bindings::default(),
            taken: Bindings::default(),
            returned,
            bound: Default::default(),
            keeps_command_line: HashSet::new(),
            unit: n.returns.as_ref().map_or(true, |r| r.value.is_empty() && !r.own),
            loop_values: HashSet::new(),
            constants: HashMap::new(),
            read: Read::default(),
        };
        b.constants = b.constants();
        b.parameters(d);
        b.binders(d, body);
        b.loop_values = b.iterations(body).into_iter().map(|(_, v)| v.id()).collect();
        let mut untyped = b.lets(body);
        b.closures(body);
        b.loops(d, body);
        // a loop variable over a sequence whose type the body does not write is typed by how it is used, as a `let` is
        untyped.extend(
            b.iterations(body)
                .into_iter()
                .map(|(p, _)| p)
                .filter(|p| matches!(p.kind(), "identifier" | "mut_pattern" | "reference_pattern"))
                .filter_map(|p| Some((bound_name(p, b.src).to_string(), scope(p)?))),
        );
        b.patterns(body);
        b.held(body);
        b.projections(body);
        b.handed_to(body, untyped);
        // a macro that prints shows the human something
        for mac in walk(body, &["macro_invocation"]) {
            let name = mac.child_by_field_name("macro").map(|x| text(x, b.src)).unwrap_or("");
            if effects::screen_macro(name) {
                b.read.touches.push(Touch {
                    op: name.into(),
                    line: mac.start_position().row as u32 + 1,
                    ..Touch::new(World::Screen, None)
                });
            }
        }
        b
    }

    fn resolve(&self, parts: &[String]) -> Option<String> {
        resolve(self.m, parts, self.modules, &self.store.nodes, self.crate_ids)
    }

    /// What a path written at `at` names where a value is wanted: none where it names a local the
    /// body binds there, which shadows the item of that name.
    fn resolve_value(&self, at: TsNode, parts: &[String]) -> Option<String> {
        if shadowed(Some(self.def), at, parts, self.src) {
            return None;
        }
        resolve_value(self.m, parts, self.modules, &self.store.nodes, self.crate_ids)
    }

    /// The library type of the types a declaration writes: the first that is no type of Rust's own,
    /// else the first library type it names (`Value` of `Result<Value, reqwest::Error>`, read as
    /// what the `Result` holds; `Vec` of `Vec<f64>`).
    fn library_type(&self, types: &[Vec<String>]) -> Option<String> {
        let libs: Vec<String> = types.iter().filter_map(|t| self.resolve(t).filter(|r| r.starts_with("ext:"))).collect();
        libs.iter().find(|r| foreign_type(r, &OWN)).or(libs.first()).cloned()
    }

    fn class_of(&self, id: &str) -> Option<String> {
        self.store.nodes.get(id).filter(|x| x.kind == NodeKind::Class).map(|_| id.to_string())
    }

    /// The class a function gives back, by its signature.
    fn returned_class(&self, id: &str) -> Option<String> {
        self.returned.get(id).cloned()
    }

    /// Typed parameters give receivers a type (`store.add_edge(..)` with `store: &mut
    /// GraphBuilder`), and `&mut T`, `&RefCell<T>` and `&mut self` hand the value back changed:
    /// `T -> T`.
    fn parameters(&mut self, d: TsNode<'t>) {
        let src = self.src;
        let Some(pn) = d.child_by_field_name("parameters") else { return };
        for p in named_children(pn) {
            if p.kind() == "self_parameter" && text(p, src).contains("mut") {
                self.read.mutates.extend(self.cls.clone().map(|c| (c, "self".to_string())));
                continue;
            }
            if p.kind() != "parameter" {
                continue;
            }
            let Some(pat) = p.child_by_field_name("pattern") else { continue };
            let pname = text(pat, src).trim_start_matches("mut ").to_string();
            let (from, to) = (d.start_byte(), d.end_byte());
            let by_mut_ref = p.child_by_field_name("type").map_or(false, |t| text(t, src).starts_with("&mut") || changes_inside(t, src));
            let types = type_names(p.child_by_field_name("type"), src);
            if let Some(lib) = self.library_type(&types) {
                self.foreign.bind(&pname, from, to, lib);
            }
            let class = types.iter().find_map(|t| self.resolve(t).filter(|r| !r.starts_with("ext:")).and_then(|r| self.class_of(&r)));
            if let Some(r) = class {
                if by_mut_ref {
                    self.read.mutates.push((r.clone(), pname.clone()));
                }
                self.locals.bind(&pname, from, to, r);
            }
        }
    }

    /// Where each name a pattern binds is visible: the function's parameters over the function, and
    /// a name a `let`, an `if let`, a `match` arm, a closure or a loop binds, which shadows a
    /// parameter of that name, where Rust scopes it.
    fn binders(&mut self, d: TsNode<'t>, body: TsNode<'t>) {
        let src = self.src;
        for p in d.child_by_field_name("parameters").map(named_children).unwrap_or_default() {
            if let Some(pat) = p.child_by_field_name("pattern").filter(|_| p.kind() == "parameter") {
                for name in bound_names(pat, src) {
                    self.parameter.bind(text(name, src), d.start_byte(), d.end_byte(), true);
                }
            }
        }
        for n in walk(body, &["let_declaration", "let_condition", "match_arm", "closure_expression", "for_expression"]) {
            let pattern = match n.kind() {
                "closure_expression" => n.child_by_field_name("parameters"),
                _ => n.child_by_field_name("pattern"),
            };
            for name in pattern.map(|p| bound_names(p, src)).unwrap_or_default() {
                if let Some((from, to)) = scope(name) {
                    self.parameter.bind(text(name, src), from, to, false);
                }
            }
        }
    }

    /// A local bound to a closure makes what the closure gives wherever it is called or handed to a
    /// call that calls it (`ok_or_else(wrong)`, `map_err(|_| wrong())`): the class of the value its
    /// body ends in, which a step inside the closure builds.
    fn closures(&mut self, body: TsNode<'t>) {
        let src = self.src;
        for l in walk(body, &["let_declaration"]) {
            let Some(pat) = l.child_by_field_name("pattern").filter(|p| matches!(p.kind(), "identifier" | "mut_pattern")) else { continue };
            let Some(f) = l.child_by_field_name("value").map(|v| beneath(v, WRAPPERS)).filter(|v| v.kind() == "closure_expression") else {
                continue;
            };
            let mut end = f.child_by_field_name("body");
            while let Some(b) = end.filter(|b| b.kind() == "block") {
                end = named_children(b).into_iter().filter(|s| !s.kind().ends_with("comment")).last();
            }
            let (Some(made), Some((from, to))) = (end.and_then(|e| self.value_class(e)), scope(pat)) else { continue };
            self.closures.bind(bound_name(pat, src), from, to, made);
        }
    }

    /// The names a pattern binds hold the types its struct or enum variant declares for them:
    /// `Command::Report { format, .. }` binds `format` to the `Format` the variant's field is.
    fn patterns(&mut self, body: TsNode<'t>) {
        let src = self.src;
        for p in walk(body, &["struct_pattern"]) {
            let Some(fields) = self.declared_fields(&path_parts(p.child_by_field_name("type"), src)) else { continue };
            for f in named_children(p).into_iter().filter(|f| f.kind() == "field_pattern") {
                // `format` binds the field, `format: f` binds `f`
                let Some(field) = f.child_by_field_name("name").map(|x| text(x, src)) else { continue };
                let (bound, at) = match f.child_by_field_name("pattern") {
                    Some(x) if x.kind() == "identifier" => (text(x, src), x),
                    Some(_) => continue,
                    None => (field, f),
                };
                let Some((from, to)) = scope(at) else { continue };
                if let Some((_, class)) = fields.iter().find(|(n, _)| n == field) {
                    self.locals.bind(bound, from, to, class.clone());
                }
            }
        }
    }

    /// The name a pattern binds to the value `Some` or `Ok` holds holds the class of what the
    /// pattern takes apart: `if let Some(line) = find(..)`, `Some(summary) => ..` of `match
    /// self.kept_summary(day)?`, as `let summary = .. .unwrap()` would.
    fn held(&mut self, body: TsNode<'t>) {
        let src = self.src;
        for n in walk(body, &["let_declaration", "let_condition", "match_expression"]) {
            let Some(value) = n.child_by_field_name("value") else { continue };
            let mut pairs: Vec<(TsNode, TsNode)> = match n.kind() {
                "match_expression" => n
                    .child_by_field_name("body")
                    .map(named_children)
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|arm| arm.child_by_field_name("pattern"))
                    .filter_map(|p| named_children(p).into_iter().next())
                    .map(|p| (p, value))
                    .collect(),
                _ => n.child_by_field_name("pattern").into_iter().map(|p| (p, value)).collect(),
            };
            // a tuple bound to a tuple pattern, element by element: `let (Some(first), Some(last)) = (xs.first(), xs.last())`
            if let [(p, v)] = pairs.as_slice() {
                let (p, v) = (*p, beneath(*v, WRAPPERS));
                if p.kind() == "tuple_pattern" && v.kind() == "tuple_expression" && named_children(p).len() == named_children(v).len() {
                    pairs = named_children(p).into_iter().zip(named_children(v)).collect();
                }
            }
            for (p, value) in pairs {
                let one = match p.kind() {
                    "tuple_struct_pattern" => {
                        let ty = p.child_by_field_name("type").map(|t| text(t, src)).unwrap_or("");
                        let inner: Vec<TsNode> = named_children(p)
                            .into_iter()
                            .filter(|x| Some(x.id()) != p.child_by_field_name("type").map(|t| t.id()))
                            .collect();
                        match inner.as_slice() {
                            [one] if super::module::HOLDING.contains(&ty) => *one,
                            _ => continue,
                        }
                    }
                    // a name of a tuple's element; a whole `let` is typed where its `let` is read
                    _ if n.kind() == "let_declaration" && n.child_by_field_name("pattern").map(|x| x.id()) == Some(p.id()) => continue,
                    _ => p,
                };
                if !matches!(one.kind(), "identifier" | "mut_pattern" | "ref_pattern") {
                    continue;
                }
                let (Some((from, to)), Some(class)) = (scope(one), self.value_class(value)) else { continue };
                self.locals.bind(bound_name(one, src), from, to, class);
            }
        }
    }

    /// A name a binding gives a part of another name's value holds that value (a projection bound
    /// to a name is still the projection): `let months = settings.months`, `let base =
    /// &config["base"]`, `let record: Record = result?`, `let (date, rate) = row?`, the fields a
    /// `match` on `cli.command` takes apart (`Command::Import { file }`), `Ok(body)` of a `match
    /// fetched`. A value a call makes is the call's, and is not followed.
    fn projections(&mut self, body: TsNode<'t>) {
        let src = self.src;
        for n in walk(body, &["let_declaration", "let_condition", "match_expression"]) {
            let Some(held) = n.child_by_field_name("value").and_then(root) else { continue };
            let patterns: Vec<TsNode> = match n.kind() {
                "match_expression" => n
                    .child_by_field_name("body")
                    .map(named_children)
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|arm| arm.child_by_field_name("pattern"))
                    .collect(),
                _ => n.child_by_field_name("pattern").into_iter().collect(),
            };
            for name in patterns.into_iter().flat_map(|p| bound_names(p, src)) {
                if let Some((from, to)) = scope(name) {
                    self.aliases.bind(text(name, src), from, to, (text(held, src).to_string(), held.start_byte()));
                }
            }
        }
        for c in walk(body, &["closure_expression"]) {
            let Some(call) = c.parent().filter(|a| a.kind() == "arguments").and_then(|a| a.parent()) else { continue };
            // a name holds one value: where the elements are parts of two (`a.iter().chain(&b)`), the closure's names hold
            // a value the body binds out of sight
            let &[held] = each_of(call, src).as_slice() else { continue };
            // the parameters the elements fill: not what a `fold` gathers
            for name in element_parameters(c, src).into_iter().flat_map(|(p, _)| bound_names(p, src)) {
                if let Some((from, to)) = scope(name) {
                    self.aliases.bind(text(name, src), from, to, (text(held, src).to_string(), held.start_byte()));
                }
            }
        }
    }

    /// The program's classes a struct pattern's fields hold, by field: a struct's, or an enum
    /// variant's (`Command::Report`), read where the enum declares it.
    fn declared_fields(&self, parts: &[String]) -> Option<Vec<(String, String)>> {
        if let Some(r) = self.resolve(parts).filter(|r| !r.starts_with("ext:")).and_then(|r| self.class_of(&r)) {
            return self.store.classes.get(&r).map(|c| c.attr_types.clone().into_iter().collect());
        }
        let (variant, owner) = parts.split_last()?;
        let e = self.resolve(owner).filter(|r| !r.starts_with("ext:")).and_then(|r| self.class_of(&r))?;
        let n = self.store.nodes.get(&e)?;
        let home = self.modules.get(&n.module)?;
        let item = def_node(home, n).filter(|d| d.kind() == "enum_item")?;
        let v = walk(item, &["enum_variant"])
            .into_iter()
            .find(|v| v.child_by_field_name("name").map(|x| text(x, &home.src)) == Some(variant.as_str()))?;
        let list = v.child_by_field_name("body").filter(|b| b.kind() == "field_declaration_list")?;
        let class = |t: Vec<String>| {
            resolve(home, &t, self.modules, &self.store.nodes, self.crate_ids)
                .filter(|r| !r.starts_with("ext:"))
                .and_then(|r| self.class_of(&r))
        };
        Some(
            named_children(list)
                .into_iter()
                .filter(|f| f.kind() == "field_declaration")
                .filter_map(|f| {
                    Some((
                        text(f.child_by_field_name("name")?, &home.src).to_string(),
                        type_names(f.child_by_field_name("type"), &home.src).into_iter().find_map(class)?,
                    ))
                })
                .collect(),
        )
    }

    /// Locals assigned from a constructor, a struct literal, or a function with a typed return;
    /// `.unwrap()`, `.expect(..)`, `.context(..)` hand on the type of the value they are called on.
    /// Gives the `let`s that write no type and whose value is of none the body knows, each with
    /// where it is visible.
    fn lets(&mut self, body: TsNode<'t>) -> Vec<(String, (usize, usize))> {
        let src = self.src;
        let mut untyped = vec![];
        for l in walk(body, &["let_declaration"]) {
            let Some(pat) = l.child_by_field_name("pattern").filter(|p| matches!(p.kind(), "identifier" | "mut_pattern")) else { continue };
            let name = text(pat, src).trim_start_matches("mut ").to_string();
            let Some((from, to)) = scope(pat) else { continue };
            if pat.kind() == "mut_pattern" || named_children(l).iter().any(|x| x.kind() == "mutable_specifier") {
                self.mutable.bind(&name, from, to, ());
            }
            let declared = type_names(l.child_by_field_name("type"), src);
            if let Some(t) = self.library_type(&declared) {
                self.foreign.bind(&name, from, to, t);
            }
            let annotated = declared.iter().find_map(|t| self.resolve(t).and_then(|t| self.class_of(&t)));
            let mut value = l.child_by_field_name("value");
            while let Some(v) = value {
                value = match v.kind() {
                    k if WRAPPERS.contains(&k) => wrapped(v),
                    "unary_expression" => named_children(v).into_iter().next(),
                    "call_expression" => match function(v) {
                        Some(f)
                            if f.kind() == "field_expression"
                                && f.child_by_field_name("field").map_or(false, |x| knowledge::hands_on(text(x, src))) =>
                        {
                            f.child_by_field_name("value")
                        }
                        _ => break,
                    },
                    _ => break,
                };
            }
            if let Some(made) = l.child_by_field_name("value") {
                self.made_by.bind(&name, from, to, made);
            }
            match annotated.or_else(|| value.and_then(|v| self.value_class(v))) {
                Some(t) => self.locals.bind(&name, from, to, t),
                None if l.child_by_field_name("type").is_none() => untyped.push((name, (from, to))),
                None => {}
            }
        }
        untyped
    }

    /// A loop variable over a sequence the body names (`for r in readings`, `&readings`,
    /// `readings.iter()`) is an element of it, of the class the sequence's written type holds
    /// (`readings: &[Reading]`, `let rows: Vec<Row>`, a field `g.nodes`, `self.rows`). So is a
    /// parameter of a closure a method hands each element of what it runs on, as a loop binds it
    /// (`n.touches.iter().map(|t| t.drawn())` as `for t in &n.touches`), or what an `Option` holds
    /// (`self.cache.map(|c| ..)`); one that writes its type is of that type (`|t: &Touch|`). They are
    /// read outer before inner, so that a closure inside another knows what the outer one is handed.
    fn loops(&mut self, d: TsNode<'t>, body: TsNode<'t>) {
        let src = self.src;
        let mut written: Bindings<String> = Bindings::default();
        for p in d.child_by_field_name("parameters").map(named_children).unwrap_or_default() {
            if let (Some(pat), Some(t)) = (p.child_by_field_name("pattern"), p.child_by_field_name("type")) {
                written.bind(text(pat, src).trim_start_matches("mut "), d.start_byte(), d.end_byte(), text(t, src).to_string());
            }
        }
        for l in walk(body, &["let_declaration"]) {
            let Some(pat) = l.child_by_field_name("pattern").filter(|p| matches!(p.kind(), "identifier" | "mut_pattern")) else { continue };
            if let (Some((from, to)), Some(t)) = (scope(pat), l.child_by_field_name("type")) {
                written.bind(text(pat, src).trim_start_matches("mut "), from, to, text(t, src).to_string());
            }
        }
        // a sequence's type, or for a closure an `Option`'s, which hands it what it holds
        let holds = |t: &str, closure: bool| {
            let head = t.trim_start_matches('&').trim_start_matches("mut ").trim();
            let name = head.split('<').next().unwrap_or("").rsplit("::").next().unwrap_or("");
            head.starts_with('[') || knowledge::RUST_SEQUENCES.contains(&name) || (closure && name == "Option")
        };
        let closures =
            walk(body, &["closure_expression"]).into_iter().flat_map(|c| element_parameters(c, src)).map(|(p, on)| (p, on, true));
        let mut binders: Vec<(TsNode, TsNode, bool)> =
            self.iterations(body).into_iter().map(|(p, v)| (p, v, false)).chain(closures).collect();
        binders.sort_by_key(|(p, _, _)| p.start_byte());
        for (pat, value, closure) in binders {
            let (pat, declared) = match pat.kind() {
                "parameter" => match pat.child_by_field_name("pattern") {
                    Some(p) => (p, pat.child_by_field_name("type")),
                    None => continue,
                },
                _ => (pat, None),
            };
            if !matches!(pat.kind(), "identifier" | "mut_pattern" | "reference_pattern") {
                continue;
            }
            let name = bound_name(pat, src).to_string();
            let Some((from, to)) = scope(pat) else { continue };
            let element = match self.sequence(value) {
                _ if declared.is_some() => type_names(declared, src).iter().find_map(|t| self.resolve(t).and_then(|r| self.class_of(&r))),
                // a sequence the body names, as its type is written (`readings: &[Reading]`)
                Some(s) if s.kind() == "identifier" => {
                    let (name, at) = (text(s, src), s.start_byte());
                    written.get(name, at).filter(|t| holds(t, closure)).and_then(|_| self.locals.get(name, at).cloned())
                }
                // a field of a value of the program's struct (`g.nodes`, `self.rows`): the class its type names
                Some(s) if s.kind() == "field_expression" => {
                    let owner = s.child_by_field_name("value").and_then(|o| match o.kind() {
                        "self" => self.cls.clone(),
                        _ => self.locals.get(text(o, src), o.start_byte()).cloned(),
                    });
                    let field = s.child_by_field_name("field").map(|f| text(f, src)).unwrap_or("");
                    let named = owner
                        .and_then(|c| self.store.classes.get(&c))
                        .and_then(|k| k.fields.iter().find(|(f, _)| f == field))
                        .map(|(_, t)| t.clone());
                    named.unwrap_or_default().iter().find_map(|t| self.resolve(t).and_then(|r| self.class_of(&r)))
                }
                _ => None,
            };
            if let Some(class) = element {
                self.locals.bind(&name, from, to, class);
            }
        }
    }

    /// What the body goes over element by element: each `for` loop's pattern and value.
    fn iterations(&self, body: TsNode<'t>) -> Vec<(TsNode<'t>, TsNode<'t>)> {
        walk(body, &["for_expression"])
            .into_iter()
            .filter_map(|l| Some((l.child_by_field_name("pattern")?, l.child_by_field_name("value")?)))
            .collect()
    }

    /// What a loop goes over, through `&xs`, `(xs)`, the methods that go over a sequence's
    /// elements and the adaptors that keep them (`xs.iter().filter(..)`): the sequence itself. Of
    /// what joins elements after its own (`xs.iter().chain(&ys)`), whose elements are all of one
    /// type, it is the first.
    fn sequence(&self, value: TsNode<'t>) -> Option<TsNode<'t>> {
        let src = self.src;
        let mut over = Some(value);
        while let Some(v) = over {
            over = match v.kind() {
                k if WRAPPERS.contains(&k) => wrapped(v),
                "call_expression" => function(v).filter(|f| {
                    f.kind() == "field_expression"
                        && f.child_by_field_name("field").map_or(false, |x| {
                            let m = text(x, src);
                            knowledge::RUST_ELEMENTS.contains(&m)
                                || knowledge::RUST_KEEPS_ELEMENTS.contains(&m)
                                || knowledge::RUST_JOINS_ELEMENTS.contains(&m)
                        })
                }),
                "field_expression" if v.parent().map_or(false, |p| p.kind() == "call_expression") => v.child_by_field_name("value"),
                _ => return Some(v),
            };
        }
        None
    }

    /// Rust fixes the type of a `let` that writes none by how the value is used: a `let` whose
    /// value is of no type the body knows, handed whole to a parameter of one of the program's
    /// functions (`draw(&area, ..)`), holds that parameter's type, read in the module that
    /// declares it: a class of the program's, or a library's (`area: &DrawingArea<DB, Shift>`
    /// through `use plotters::prelude::*`).
    fn handed_to(&mut self, body: TsNode<'t>, untyped: Vec<(String, (usize, usize))>) {
        let src = self.src;
        let mut open: Vec<(String, (usize, usize))> =
            untyped.into_iter().filter(|(n, (from, _))| self.locals.binding(n, *from).map_or(true, |(f, _)| f != *from)).collect();
        // bound whole to a `let` that writes its type: `result` of `let record: Record = result?`
        for l in walk(body, &["let_declaration"]) {
            let (Some(pat), Some(value)) = (l.child_by_field_name("pattern"), l.child_by_field_name("value")) else { continue };
            let held = beneath(value, WRAPPERS);
            if l.child_by_field_name("type").is_none() || held.kind() != "identifier" {
                continue;
            }
            let Some((_, class)) = scope(pat).and_then(|(from, _)| self.locals.binding(bound_name(pat, src), from)) else { continue };
            let class = class.clone();
            let (name, at) = (text(held, src), held.start_byte());
            let inner =
                open.iter().enumerate().filter(|(_, (n, (f, t)))| n == name && *f <= at && at < *t).max_by_key(|(_, (_, (f, _)))| *f);
            if let Some((i, (_, (from, to)))) = inner.map(|(i, b)| (i, b.clone())) {
                if self.locals.binding(name, at).map_or(true, |(f, _)| f < from) {
                    self.locals.bind(name, from, to, class);
                    open.remove(i);
                }
            }
        }
        for c in walk(body, &["call_expression"]) {
            if open.is_empty() {
                return;
            }
            let Some((t, on_value)) = self.callee(c) else { continue };
            let Some(n) = self.store.nodes.get(&t) else { continue };
            let Some(home) = self.modules.get(&n.module) else { continue };
            let Some(def) = def_node(home, n) else { continue };
            let declared: Vec<TsNode> = def
                .child_by_field_name("parameters")
                .map(named_children)
                .unwrap_or_default()
                .into_iter()
                .filter(|p| p.kind() == "parameter")
                .collect();
            let skip = usize::from(!on_value && paths::takes_self(def));
            for (p, a) in declared.into_iter().zip(positional(c).into_iter().skip(skip)) {
                let Some(a) = a.map(|a| beneath(a, WRAPPERS)).filter(|x| x.kind() == "identifier") else { continue };
                let (name, at) = (text(a, src).to_string(), a.start_byte());
                // the binding the argument uses: the innermost open one, unless a typed one inside it shadows it
                let Some(i) = open
                    .iter()
                    .enumerate()
                    .filter(|(_, (n, (from, to)))| *n == name && *from <= at && at < *to)
                    .max_by_key(|(_, (_, (from, _)))| *from)
                    .map(|(i, _)| i)
                else {
                    continue;
                };
                let (from, to) = open[i].1;
                if self.locals.binding(&name, at).map_or(false, |(f, _)| f > from) {
                    continue;
                }
                let ty = p.child_by_field_name("type");
                let named = |parts: &Vec<String>| match parts.as_slice() {
                    [s] if s == "Self" => n.cls.clone(),
                    _ => resolve(home, parts, self.modules, &self.store.nodes, self.crate_ids),
                };
                // a class of the program's as its parameter is typed, the records it holds
                // (`&[Month]`); a library's type only where it is the parameter's own, never one
                // it holds (`&[(NaiveDate, f64)]` names no what)
                let class =
                    type_names(ty, &home.src).iter().filter_map(named).find(|r| !r.starts_with("ext:") && self.class_of(r).is_some());
                let lib = Some(path_parts(ty, &home.src))
                    .filter(|parts| !parts.is_empty())
                    .as_ref()
                    .and_then(named)
                    .filter(|r| foreign_type(r, &OWN));
                match (class, lib) {
                    (Some(class), _) => self.locals.bind(&name, from, to, class),
                    (None, Some(lib)) => self.taken.bind(&name, from, to, lib),
                    (None, None) => continue,
                };
                open.remove(i);
            }
        }
    }

    /// The program's function a call runs, and whether it is a method called on a value: one on
    /// the class its receiver holds, or one its path names.
    fn callee(&self, c: TsNode) -> Option<(String, bool)> {
        let src = self.src;
        let f = function(c)?;
        let own = |t: String| self.store.nodes.get(&t).filter(|n| matches!(n.kind, NodeKind::Function | NodeKind::Method)).map(|_| t);
        if f.kind() == "field_expression" {
            let class = self.value_class(f.child_by_field_name("value")?)?;
            return own(self.store.method_of(&class, text(f.child_by_field_name("field")?, src))?).map(|t| (t, true));
        }
        let parts = path_parts(Some(f), src);
        let t = match (parts.first().map(String::as_str), &self.cls) {
            (Some("Self"), Some(cls)) => self.store.method_of(cls, parts.get(1)?)?,
            _ => self.resolve_value(f, &parts)?,
        };
        own(t).map(|t| (t, false))
    }

    /// The class a value is of, where the body says: a name it holds, a struct literal, a method
    /// by what its signature gives back on the class of its receiver (a name, `self`, or the value
    /// another call gives), a function by its return.
    fn value_class(&self, v: TsNode) -> Option<String> {
        let src = self.src;
        match v.kind() {
            k if WRAPPERS.contains(&k) => self.value_class(wrapped(v)?),
            "self" => self.cls.clone(),
            "identifier" => self.locals.get(text(v, src), v.start_byte()).cloned().or_else(|| self.unit_struct(v)),
            "scoped_identifier" => self.unit_struct(v),
            "struct_expression" => self.resolve(&path_parts(v.child_by_field_name("name"), src)).and_then(|r| self.class_of(&r)),
            "call_expression" if function(v).map_or(false, |f| f.kind() == "field_expression") => {
                let f = function(v)?;
                let on = self.value_class(f.child_by_field_name("value")?)?;
                let name = f.child_by_field_name("field").map(|x| text(x, src)).unwrap_or("");
                match self.store.method_of(&on, name) {
                    Some(t) => self.returned_class(&t),
                    // one of the records a sequence of them holds: `series.first()`
                    None if knowledge::RUST_ONE_ELEMENT.contains(&name) => Some(on),
                    None => None,
                }
            }
            "call_expression" => {
                let parts = path_parts(function(v), src);
                if parts.is_empty() {
                    return None;
                }
                self.resolve_value(v, &parts)
                    .as_deref()
                    .and_then(|f| self.returned_class(f))
                    .or_else(|| self.resolve(&parts[..parts.len() - 1]).and_then(|r| self.class_of(&r)))
            }
            _ => None,
        }
    }

    /// The type a name of the body holds at byte `at`: a class of the program's, else the library's
    /// type a `let` takes from the parameter it is handed to, else a library's type it is declared
    /// with (`area: &DrawingArea<DB, Shift>`), not one of Rust's own (`Vec<f64>`, `PathBuf`); a name
    /// the body binds nowhere, of a constant its module declares, the class that constant holds.
    fn held_type(&self, name: &str, at: usize) -> Option<String> {
        let library = || self.foreign.get(name, at).filter(|t| foreign_type(t, &OWN));
        let constant = || self.constants.get(name).filter(|_| self.parameter.get(name, at).is_none());
        self.locals.get(name, at).or_else(|| self.taken.get(name, at)).or_else(library).or_else(constant).cloned()
    }

    /// The constants the body's module declares at its top, each with the class of the program its declared type holds
    /// (`Body::constants`): a `static` that is `mut` or changes in place is a place, `Global(..)`, no constant.
    fn constants(&self) -> HashMap<String, String> {
        let src = self.src;
        let mut out = HashMap::new();
        for item in named_children(self.m.tree.root_node()).into_iter().filter(|i| matches!(i.kind(), "const_item" | "static_item")) {
            let types = type_names(item.child_by_field_name("type"), src);
            let changes = named_children(item).iter().any(|c| c.kind() == "mutable_specifier")
                || types.iter().any(|t| t.last().is_some_and(|l| knowledge::rust_interior(l)));
            let Some(name) = item.child_by_field_name("name").filter(|_| !changes) else { continue };
            let class = types.iter().find_map(|t| self.resolve(t).filter(|r| !r.starts_with("ext:")).and_then(|r| self.class_of(&r)));
            if let Some(c) = class {
                out.insert(text(name, src).to_string(), c);
            }
        }
        out
    }

    /// The name whose value `name` holds at byte `at`, and where: the name itself, or the one a
    /// `let` bound it to (`record` of `let record: Record = result?` holds what `result` holds).
    fn holder(&self, name: &str, at: usize) -> (String, usize) {
        let (mut name, mut at) = (name.to_string(), at);
        for _ in 0..8 {
            match self.aliases.get(&name, at) {
                Some((n, a)) => (name, at) = (n.clone(), *a),
                None => break,
            }
        }
        (name, at)
    }

    /// What a call is handed: the value it is called on, its arguments, a literal's fields.
    /// The instance the body runs on is handed where an argument names it or a part of it
    /// (`fs::read_to_string(&self.path)`, `let conn = &self.conn; conn.prepare(..)`), as a value of
    /// its class no name of the body holds; a call made on `self` itself runs on it instead.
    fn hands(&self, parts: &[Option<TsNode>], kept: &HashSet<String>) -> Vec<Held> {
        self.handed(self.hands_of(parts, kept))
    }

    /// The values a call is handed, each of the body's own parameters of no type of the code's among
    /// them marked (`Held::param`): it holds a value only where the function's calls fill it with one,
    /// which is known once every body is read (`builder::settle_parameters`).
    fn handed(&self, held: Vec<(Held, Option<usize>)>) -> Vec<Held> {
        held.into_iter().map(|(h, param)| Held { param: param.is_some(), ..h }).collect()
    }

    /// What `hands` gives, each value with the place of the body's own parameter of no type of the
    /// code's it is, where it is one.
    fn hands_of(&self, parts: &[Option<TsNode>], kept: &HashSet<String>) -> Vec<(Held, Option<usize>)> {
        let mut out: Vec<(Held, Option<usize>)> = vec![];
        // the receiver a method called by its path runs on is its first argument (`Self::raw(self)`)
        let runs_on: Vec<usize> =
            parts.iter().flatten().map(|p| beneath(*p, WRAPPERS)).filter(|p| p.kind() == "self").map(|p| p.id()).collect();
        for p in parts.iter().flatten() {
            // a call inside that is a step of its own takes its names itself; this call is handed what it gives
            let steps = self.steps_within(*p);
            let outside = |at: usize| !steps.iter().any(|r| r.contains(&at));
            let mut held: Vec<(Held, Option<usize>)> = vec![];
            let mut add = |h: (Held, Option<usize>)| {
                if outside(h.0.at as usize) && !held.iter().any(|(x, _)| *x == h.0) {
                    held.push(h);
                }
            };
            // what a closure written in the call binds in its own body is that closure's (`|row| { let amount = ..; .. }`)
            let closures: Vec<TsNode> =
                walk(*p, &["closure_expression"]).into_iter().filter_map(|c| c.child_by_field_name("body")).collect();
            let bound_inside = |i: TsNode| {
                let (name, at) = (text(i, self.src), i.start_byte());
                closures.iter().filter(|b| b.start_byte() <= at && at < b.end_byte()).any(|b| {
                    walk(*b, &["let_declaration", "let_condition", "match_arm"])
                        .into_iter()
                        .filter(|l| l.start_byte() < at)
                        .filter_map(|l| l.child_by_field_name("pattern"))
                        .any(|pat| walk(pat, &["identifier"]).into_iter().any(|x| text(x, self.src) == name))
                })
            };
            for i in walk(*p, &["identifier"]).into_iter().filter(|i| names_value(*i, self.src) && !bound_inside(*i)) {
                for h in self.held_through(text(i, self.src), i.start_byte(), kept, 0) {
                    add(h);
                }
            }
            if beneath(*p, WRAPPERS).kind() != "self" {
                for s in walk(*p, &["self"]).into_iter().filter(|s| !runs_on.contains(&s.id())) {
                    if let Some(ty) = self.cls.clone() {
                        add((Held::at(ty, String::new(), s.start_byte(), false), None));
                    }
                }
            }
            // a name a format string writes (`format!("{key} {low}")`) is an argument as one after it is
            for lit in walk(*p, &["string_literal", "raw_string_literal"]).into_iter().filter(|l| formats(*l, self.src)) {
                for (name, off) in inline_names(text(lit, self.src)) {
                    for h in self.held_through(name, lit.start_byte() + off, kept, 0) {
                        add(h);
                    }
                }
            }
            for (h, param) in held {
                if !out.iter().any(|(o, _)| o.ty == h.ty && o.name == h.name && (o.at == h.at || h.name.is_empty())) {
                    out.push((h, param));
                }
            }
        }
        out
    }

    /// The values the name `name` written at byte `at` holds, as a call is handed them: the value `held_by` says, or, where
    /// a `let` binds the name to what Rust's own code computes of names and nothing has made it anew since, no step and no
    /// assignment (`let gap = first.db - second.db`, `let both = [first, second]`), the values those names hold there, as the
    /// value written in the name's place hands them (`body::stands_for`). Each is handed where the name is written, from
    /// where the name it holds it by is (`Held::bound`).
    fn held_through(&self, name: &str, at: usize, kept: &HashSet<String>, depth: usize) -> Vec<(Held, Option<usize>)> {
        let (holder, from) = self.holder(name, at);
        let made = self.made_by.get(&holder, from).filter(|_| depth <= 8 && !kept.contains(&holder) && !self.reassigned(&holder, from));
        let names = made.map(|v| crate::body::stands_for(*v, &USES, self.src, &part)).unwrap_or_default();
        if names.is_empty() {
            return self.held_by(name, at, kept).into_iter().collect();
        }
        names
            .into_iter()
            .flat_map(|n| {
                let held = match n.kind() {
                    "self" => self.cls.clone().map(|ty| (Held::at(ty, String::new(), n.start_byte(), false), None)).into_iter().collect(),
                    _ => self.held_through(text(n, self.src), n.start_byte(), kept, depth + 1),
                };
                held.into_iter().map(move |(h, param)| {
                    (Held { at: at as u32, bound: if h.bound != 0 { h.bound } else { n.start_byte() as u32 }, ..h }, param)
                })
            })
            .collect()
    }

    /// Whether the body assigns the name again where the `let` visible at `at` binds it (`x = ..`, `x += ..`): what the
    /// `let` bound it to is then not all it holds.
    fn reassigned(&self, name: &str, at: usize) -> bool {
        let Some(binding) = self.made_by.which(name, at).filter(|_| self.mutable.holds(name, at)) else { return false };
        walk(self.def, &["assignment_expression", "compound_assignment_expr"]).into_iter().any(|a| {
            a.child_by_field_name("left").is_some_and(|l| {
                l.kind() == "identifier" && text(l, self.src) == name && self.made_by.which(name, l.start_byte()) == Some(binding)
            })
        })
    }

    /// The value the name `name` written at byte `at` holds, as a call is handed it: the instance
    /// the body runs on for `self`, a value of the type the name holds, and a value of no type of
    /// the code's where a step kept it under the name or the name is one of the function's
    /// parameters, which holds what its call hands it (given with the parameter's place). None for
    /// a name that holds no value Atlas draws (a local the body bound to a literal).
    fn held_by(&self, name: &str, at: usize, kept: &HashSet<String>) -> Option<(Held, Option<usize>)> {
        let (name, from) = self.holder(name, at);
        // a name a `let` bound to a projection holds what the projection starts from, there (`let t = &self.title`)
        let bound = |h: Held| Held { bound: if from != at { from as u32 } else { 0 }, ..h };
        if name == "self" {
            return Some((bound(Held::at(self.cls.clone()?, String::new(), at, false)), None));
        }
        // a closure is no value; what it makes is, and no name holds that
        if let Some(made) = self.closures.get(&name, from) {
            return Some((Held::at(made.clone(), String::new(), at, false), None));
        }
        let param = || self.params.iter().position(|p| *p == name).filter(|_| self.parameter.get(&name, from) == Some(&true));
        match self.held_type(&name, from) {
            Some(ty) => Some((bound(Held::at(ty, name, at, false)), None)),
            None if kept.contains(&name) => Some((bound(Held::at(String::new(), name, at, false)), None)),
            None => {
                let at_param = param()?;
                Some((bound(Held::at(String::new(), name, at, false)), Some(at_param)))
            }
        }
    }

    /// The stretches of `p` that a call of the program's own code covers, `p` itself among them: a
    /// function or method the body calls (`level.prefix()`, also among a macro's tokens), a type it
    /// builds by its literal. Each is a step of its own, and takes the names in it itself.
    fn steps_within(&self, p: TsNode) -> Vec<std::ops::Range<usize>> {
        let src = self.src;
        let own = |t: &str| self.store.nodes.get(t).map_or(false, |n| matches!(n.kind, NodeKind::Function | NodeKind::Method));
        let mut out = vec![];
        for n in walk(p, &["call_expression", "struct_expression", "token_tree"]) {
            match n.kind() {
                "call_expression" if self.callee(n).is_some() => out.push(n.byte_range()),
                "struct_expression"
                    if self
                        .resolve(&path_parts(n.child_by_field_name("name"), src))
                        .filter(|r| !r.starts_with("ext:"))
                        .and_then(|r| self.class_of(&r))
                        .is_some() =>
                {
                    out.push(n.byte_range())
                }
                "token_tree" => match token_callee(n, src) {
                    Some(Called::Path(parts)) if self.resolve_value(n, &parts).map_or(false, |t| own(&t)) => out.push(n.byte_range()),
                    Some(Called::Method(recv, method)) => {
                        let owner = |t: TsNode| {
                            if t.kind() == "self" {
                                self.cls.clone()
                            } else {
                                self.locals.get(text(t, src), t.start_byte()).cloned().or_else(|| self.unit_struct(t))
                            }
                        };
                        let class = match recv.as_slice() {
                            [r] => owner(*r),
                            [o, f] => {
                                owner(*o).and_then(|c| self.store.classes.get(&c)).and_then(|k| k.attr_types.get(text(*f, src)).cloned())
                            }
                            _ => None,
                        };
                        if class.and_then(|k| self.store.method_of(&k, &method)).map_or(false, |t| !t.starts_with("ext:")) {
                            out.push(recv.first().map_or(n.start_byte(), |r| r.start_byte())..n.end_byte());
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
        }
        out
    }

    /// A loop as the step it is: it takes what it goes over, and gives each element under the
    /// name it binds (`for n in &g.nodes` takes `g` and gives `n`), of the class `loops` found.
    fn each(&mut self, steps: &mut Steps<'t, '_>, pat: TsNode<'t>, value: TsNode<'t>) {
        let names: Vec<String> = walk(pat, &["identifier"]).into_iter().map(|i| text(i, self.src).to_string()).collect();
        // it goes over what it is handed, or over what a step of the program gives it (`store.rows()`);
        // over a parameter of no type of the code's (`notes: &[String]`) it makes no value, as over
        // elements of no type Atlas draws
        let held = self.hands_of(&[Some(value)], &steps.kept);
        let over = held.iter().any(|(_, param)| param.is_none()) || !self.steps_within(value).is_empty();
        let mut hands = self.handed(held);
        // a step it goes over as written (`for row in store.rows()`) stands where the loop does, and hands it what it gives
        hands.extend(self.looped(value));
        let inside = scope(pat).map_or(pat.end_byte(), |(from, _)| from);
        let gives: Vec<Held> = names
            .iter()
            .filter_map(|n| match self.held_type(n, inside) {
                Some(t) => Some(Held::new(t, n.clone())),
                None => over.then(|| Held::new(String::new(), n.clone())),
            })
            .collect();
        // the step stands where what it goes over is written: a call there hands it what it gives, the loop's body does not
        steps.each(value, &format!("for {} in {}", text(pat, self.src), text(value, self.src)), hands, gives);
    }

    /// What a call gives back, by the name the body keeps it by: the type the local is declared
    /// with, else the class the call builds or returns.
    fn gives(&self, c: TsNode<'t>, to: Option<&str>) -> Vec<Held> {
        let made = |t: &str| self.class_of(t).or_else(|| self.returned_class(t));
        let kept = kept_as(c, self.src);
        if kept.is_empty() {
            if let Some(h) = self.looped(c) {
                return vec![h];
            }
        }
        // the class the call builds or returns is the value of a name that keeps it whole
        let whole = kept.len() == 1;
        kept.into_iter()
            .map(|(name, at)| {
                Held::new(self.held_type(&name, at).or_else(|| to.filter(|_| whole).and_then(made)).unwrap_or_default(), name)
            })
            .collect()
    }

    /// What a call of the program's function that a loop goes over, written as the loop's value
    /// (`for row in store.rows()`), gives the loop: one value, of the class it builds or returns,
    /// else of no type of the code's, which no name holds and the step names (`#`). The call and the
    /// loop stand at the same expression, so the call is not found inside the loop's step as one
    /// written in it (`for row in store.rows().iter()`) is: the value is how the two are joined.
    fn looped(&self, c: TsNode<'t>) -> Option<Held> {
        let (t, _) = self.callee(c).filter(|_| self.loop_values.contains(&c.id()))?;
        match self.class_of(&t).or_else(|| self.returned_class(&t)) {
            Some(class) => Some(Held::new(class, "")),
            None => Some(Held::new("", format!("#{}@{}", self.id, c.start_byte()))),
        }
    }

    /// A boundary call's touches, refined by the literals that name its place.
    fn touch(&self, c: TsNode, kind: Crossing, op: &str, lits: &[String]) -> Vec<Touch> {
        Crosses::read(c, self.src, kind, None, op, lits, STRINGS, &self.locals.visible(c.start_byte()), &path_maker_named(self.src))
            .touches(None)
    }

    /// One call of the body, as the step it is: a struct literal, a method on a value, or a path,
    /// written as an expression or among a macro's tokens.
    fn call(&mut self, steps: &mut Steps<'t, '_>, c: TsNode<'t>) {
        if c.kind() == "struct_expression" {
            return self.literal(steps, c);
        }
        if matches!(c.kind(), "identifier" | "scoped_identifier") {
            return self.unit(steps, c);
        }
        if c.kind() == "token_tree" {
            return self.token_call(steps, c);
        }
        let Some(f) = function(c) else { return };
        self.handed_on(steps, c);
        if f.kind() == "field_expression" {
            self.method_call(steps, c, f);
        } else {
            let parts = qualified(f, self.src).map_or_else(|| path_parts(Some(f), self.src), |(_, through)| through);
            self.path_call(steps, c, parts);
        }
    }

    /// A call among a macro's tokens, `c` the group it is handed (`(e.amount)` of `round2` in
    /// `params![.., round2(e.amount)]`): it runs like any call, so it is followed like one. A
    /// method is followed on a receiver the body knows the class of (`store.rows()`, `self.f()`,
    /// `self.conn.f()`).
    fn token_call(&mut self, steps: &mut Steps<'t, '_>, c: TsNode<'t>) {
        let src = self.src;
        let Some(called) = token_callee(c, src) else { return };
        self.handed_on(steps, c);
        let (recv, method) = match called {
            Called::Path(parts) => return self.path_call(steps, c, parts),
            Called::Method(recv, method) => (recv, method),
        };
        let owner = |t: TsNode| match t.kind() {
            "self" => self.cls.clone(),
            _ => self.locals.get(text(t, src), t.start_byte()).cloned().or_else(|| self.unit_struct(t)),
        };
        let class = match recv.as_slice() {
            [r] => owner(*r),
            [o, f] => owner(*o).and_then(|c| self.store.classes.get(&c)).and_then(|k| k.attr_types.get(text(*f, src)).cloned()),
            _ => None,
        };
        match class {
            // a method not found on the type (`to_string`, `len`) is not known; it builds nothing
            Some(r) => {
                if let Some(t) = self.store.method_of(&r, &method).filter(|t| !t.starts_with("ext:")) {
                    self.own_step(steps, c, &t, recv.last().copied(), Some(&r), None);
                }
            }
            None => self.library_step(steps, c, &method, recv.last().copied()),
        }
    }

    /// `Type { .. }`: a value of one of the program's types, built from its fields.
    fn literal(&self, steps: &mut Steps<'t, '_>, c: TsNode<'t>) {
        let Some(r) = self
            .resolve(&path_parts(c.child_by_field_name("name"), self.src))
            .filter(|r| !r.starts_with("ext:"))
            .and_then(|r| self.class_of(&r))
        else {
            return;
        };
        let step =
            Step { hands: self.hands(&[c.child_by_field_name("body")], &steps.kept), gives: self.gives(c, Some(&r)), ..Default::default() };
        steps.call(c, &r, EdgeKind::Instantiates, None, step);
    }

    /// `Markdown`, named where a value goes: a unit struct named as a value is a value of it,
    /// built, also where a method is called on it (`Plain.show()`), and among a macro's tokens where
    /// they put a value (`println!("{}", Plain.name())`).
    fn unit(&self, steps: &mut Steps<'t, '_>, c: TsNode<'t>) {
        let is = |p: TsNode, field: &str| p.child_by_field_name(field).map(|v| v.id()) == Some(c.id());
        let value = c.parent().map_or(false, |p| match p.kind() {
            "arguments" | "return_expression" | "array_expression" | "tuple_expression" | "reference_expression" => true,
            "let_declaration" | "field_initializer" | "match_arm" | "field_expression" => is(p, "value"),
            "assignment_expression" => is(p, "right"),
            "block" => p.named_child(p.named_child_count().saturating_sub(1)).map(|x| x.id()) == Some(c.id()),
            "token_tree" => token_value(c, self.src),
            _ => false,
        });
        // a unit struct's name cannot be bound by a pattern: it is the struct wherever it is in scope
        if !value {
            return;
        }
        let Some(r) = self.unit_struct(c) else { return };
        let step = Step { gives: self.gives(c, Some(&r)), ..Default::default() };
        steps.call(c, &r, EdgeKind::Instantiates, None, step);
    }

    /// The unit struct a path written as a value names (`Words`, `formats::Words`): its name alone is a value of it,
    /// built there. A struct with fields, a tuple struct (whose name is the function that builds one), an enum or a
    /// trait is no value by its name alone, and neither is a name the body binds.
    fn unit_struct(&self, v: TsNode) -> Option<String> {
        if v.kind() == "identifier" && self.parameter.get(text(v, self.src), v.start_byte()).is_some() {
            return None;
        }
        let r = self.resolve(&path_parts(Some(v), self.src)).filter(|r| !r.starts_with("ext:")).and_then(|r| self.class_of(&r))?;
        let n = self.store.nodes.get(&r)?;
        let def = self.modules.get(&n.module).and_then(|home| def_node(home, n))?;
        (def.kind() == "struct_item" && def.child_by_field_name("body").is_none()).then_some(r)
    }

    /// An own function handed on as a value (`xs.iter().map(Self::label)`, `thread::spawn(run)`):
    /// called by whoever receives it. A method that hands the function each element of what it runs
    /// on (`text.lines().map(Expense::parse)`) calls it there, as the closure `|line|
    /// Expense::parse(line)` would: a step written where the function is named, handed the element,
    /// a part of what the method runs on, and what it gives goes into the step around it.
    fn handed_on(&mut self, steps: &mut Steps<'t, '_>, c: TsNode<'t>) {
        let element = function(c)
            .filter(|f| f.kind() == "field_expression")
            .and_then(|f| f.child_by_field_name("field"))
            .and_then(|m| knowledge::rust_element(text(m, self.src)));
        for a in positional(c).into_iter().flatten() {
            if !matches!(a.kind(), "identifier" | "scoped_identifier") {
                continue;
            }
            let ps = path_parts(Some(a), self.src);
            let handed = match (ps.first().map(String::as_str), &self.cls) {
                (Some("Self"), Some(c)) => ps.get(1).and_then(|m| self.store.method_of(c, m)),
                _ => self.resolve_value(a, &ps),
            };
            let Some(t) =
                handed.filter(|t| self.store.nodes.get(t).map_or(false, |n| matches!(n.kind, NodeKind::Function | NodeKind::Method)))
            else {
                continue;
            };
            let Some(element) = element else {
                steps.handed(c, a, &t, EdgeKind::Calls, None);
                continue;
            };
            let elements: Vec<Option<TsNode>> = each_of(c, self.src).into_iter().map(Some).collect();
            let hands = self.hands(&elements, &steps.kept);
            // the element fills the function's parameter at its place, the first of a method being the value it runs on
            let takes_self = self.store.nodes[&t].params.first().map_or(false, |p| p.receiver);
            let slot = match (takes_self, element) {
                (true, 0) => Slot::Receiver,
                (true, at) => Slot::At(at - 1),
                (false, at) => Slot::At(at),
            };
            steps.handed(c, a, &t, EdgeKind::Calls, Some((hands, slot)));
        }
    }

    /// The library path a call goes into, where it goes into a library's code Atlas has not read:
    /// a function or a type's by its path (`serde_json.from_str`, `regex.Regex.new`), or a method
    /// of the library type of the value it is called on (`regex.Regex.is_match`).
    pub(super) fn library_target(&self, c: TsNode<'t>) -> Option<Vec<String>> {
        let f = function(c)?;
        if f.kind() == "field_expression" {
            if self.callee(c).is_some() {
                return None;
            }
            let method = text(f.child_by_field_name("field")?, self.src).to_string();
            let made = library_chain(f.child_by_field_name("value")?, self.m, self.src, &self.made_by, &|v| self.declared(v))?;
            return Some(made.ty.into_iter().chain([method]).collect());
        }
        let parts = path_parts(Some(f), self.src);
        let t = self.resolve_value(f, &parts)?;
        Some(t.strip_prefix("ext:")?.split('.').map(String::from).collect())
    }

    /// A call Atlas reads as computing what it gives, `written` as the code writes it: where what
    /// it gives goes nowhere, or is the last value of a block that gives none, the call is made for
    /// an effect Atlas does not see.
    fn dropped(&self, steps: &mut Steps<'t, '_>, c: TsNode<'t>, written: &str) {
        let body = self.def.child_by_field_name("body");
        let last = body.and_then(|b| b.named_child(b.named_child_count().saturating_sub(1)));
        if self.unit && last.map(|l| l.id()) == Some(c.id()) {
            steps.unseen(c, &format!("{written}(..)"));
        } else {
            steps.dropped(c, written);
        }
    }

    /// A library's operation not read: a step where the body keeps what it gives.
    fn library_step(&self, steps: &mut Steps<'t, '_>, c: TsNode<'t>, op: &str, recv: Option<TsNode<'t>>) {
        let hands = self.call_hands(c, recv, &steps.kept);
        if self.keeps_command_line.contains(&c.id()) {
            return steps.parses(c, op, hands, self.gives(c, None), vec![]);
        }
        steps.op(c, op, hands, self.gives(c, None));
    }

    /// Where the command line a call reads is kept (`env::args()`): the end of the chain of
    /// methods called on what it gives, where a name keeps that (`let args: Vec<String> =
    /// env::args().skip(1).collect()`), else the call itself. The step there takes the entry's
    /// words, and declares no options: the code names none.
    fn reads_command_line(&mut self, c: TsNode<'t>) {
        let mut end = c;
        loop {
            let value = crate::tree::outermost(end, WRAPPERS);
            let on = value
                .parent()
                .filter(|f| f.kind() == "field_expression" && f.child_by_field_name("value").map(|v| v.id()) == Some(value.id()));
            match on.and_then(|f| f.parent()).filter(|p| p.kind() == "call_expression") {
                Some(call) => end = call,
                None => break,
            }
        }
        let at = if kept_as(end, self.src).is_empty() { c } else { end };
        self.keeps_command_line.insert(at.id());
    }

    /// What a call is handed: the value it is called on, and the names that stand for values in
    /// its arguments.
    fn call_hands(&self, c: TsNode<'t>, recv: Option<TsNode<'t>>, kept: &HashSet<String>) -> Vec<Held> {
        let parts: Vec<Option<TsNode>> = match c.kind() {
            "token_tree" => std::iter::once(recv).chain(token_values(c).into_iter().map(Some)).collect(),
            _ => vec![recv, c.child_by_field_name("arguments")],
        };
        let mut hands = self.hands(&parts, kept);
        // among a macro's tokens, a method called on a part of `self` (`format!(.., self.kind.name())`) runs on a projection
        // of the instance, as written outside a macro: it is handed the instance
        let instance =
            std::iter::successors(recv.filter(|_| c.kind() == "token_tree").and_then(|r| r.prev_sibling()), |t| t.prev_sibling())
                .take_while(|t| matches!(t.kind(), "." | "identifier" | "self"))
                .find(|t| t.kind() == "self");
        if let (Some(s), Some(ty)) = (instance, self.cls.clone()) {
            if !hands.iter().any(|h| h.ty == ty && h.name.is_empty()) {
                hands.push(Held::at(ty, String::new(), s.start_byte(), false));
            }
        }
        hands
    }

    /// A step to one of the program's own functions or types; a method called on a value of a
    /// kind of its own type (a trait's method on a type that implements it) runs on that kind.
    fn own_step(
        &mut self,
        steps: &mut Steps<'t, '_>,
        c: TsNode<'t>,
        t: &str,
        recv: Option<TsNode<'t>>,
        class: Option<&str>,
        via: Option<String>,
    ) {
        let kind = if self.store.nodes[t].kind == NodeKind::Class { EdgeKind::Instantiates } else { EdgeKind::Calls };
        let on_receiver = recv.map_or(false, |r| r.kind() == "self");
        let on = class.filter(|k| self.store.nodes[t].cls.as_deref().map_or(false, |b| b != *k)).unwrap_or_default().to_string();
        // a type built by parsing the command line the program is run with (`Cli::parse()`)
        let command_line =
            (kind == EdgeKind::Instantiates && c.kind() == "call_expression").then(|| self.eval.command_line(self.m, c)).flatten();
        let step = Step {
            on,
            on_receiver,
            hands: self.call_hands(c, recv, &steps.kept),
            gives: self.gives(c, Some(t)),
            command_line,
            ..Default::default()
        };
        // a method called by its path is handed the value it runs on first
        let by_path = match c.kind() {
            "token_tree" => matches!(token_callee(c, self.src), Some(Called::Path(_))),
            _ => function(c).map_or(false, |f| f.kind() != "field_expression"),
        };
        // among a macro's tokens a method's call is written from its receiver on, as `Body::steps_within` reads it
        let from = match c.kind() {
            "token_tree" => match token_callee(c, self.src) {
                Some(Called::Method(names, _)) => names.first().map(|r| r.start_byte()),
                _ => None,
            },
            _ => None,
        };
        let step = Step { from, ..step };
        let on_value = recv.is_some() && !by_path;
        let passed = self.passed(c, t, on_value);
        let receiver_first = !on_value && self.takes_self(t);
        steps.call(c, t, kind, via.filter(|_| kind == EdgeKind::Instantiates), Step { receiver_first, ..step });
        if let Some(arg) = self.argument(t, passed) {
            self.read.args.push(arg);
        }
    }

    /// Whether the program's function `t` takes the value it runs on, `self`: a method called by its
    /// path is handed it first.
    fn takes_self(&self, t: &str) -> bool {
        let n = self.store.nodes.get(t).filter(|n| matches!(n.kind, NodeKind::Function | NodeKind::Method));
        n.and_then(|n| def_node(self.modules.get(&n.module)?, n)).is_some_and(paths::takes_self)
    }

    /// What a call of the program's function `t` hands each of its parameters, by the parameter's
    /// name and place: a method called on a value is handed the rest, one called by its path its
    /// receiver first.
    fn passed(&self, c: TsNode<'t>, t: &str, on_value: bool) -> Vec<(String, usize, TsNode<'t>)> {
        let Some(n) = self.store.nodes.get(t).filter(|n| matches!(n.kind, NodeKind::Function | NodeKind::Method)) else { return vec![] };
        let Some((home, def)) = self.modules.get(&n.module).and_then(|home| Some((home, def_node(home, n)?))) else { return vec![] };
        let skip = usize::from(!on_value && paths::takes_self(def));
        paths::param_names(def, &home.src)
            .into_iter()
            .zip(positional(c).into_iter().skip(skip))
            .enumerate()
            .filter_map(|(at, (p, a))| Some((p, at, a?)))
            .collect()
    }

    /// The call of the program's function `t` with what it hands each of its parameters, by name.
    fn argument(&self, t: &str, passed: Vec<(String, usize, TsNode<'t>)>) -> Option<Arg<'t>> {
        let n = self.store.nodes.get(t).filter(|n| matches!(n.kind, NodeKind::Function | NodeKind::Method))?;
        def_node(self.modules.get(&n.module)?, n)?;
        let passed = passed.into_iter().filter(|(p, _, _)| !p.is_empty()).map(|(p, _, a)| (p, a)).collect();
        Some(Arg {
            caller: self.id.clone(),
            callee: t.to_string(),
            module: self.m.id.clone(),
            scope: Some(self.def),
            params: self.params.clone(),
            passed,
        })
    }

    /// Where an expression of the body is evaluated.
    fn at(&self) -> At<'t, '_> {
        At { m: self.m, scope: Some(self.def), params: &self.params }
    }

    /// A call's touches, each with the path or statement it waits for when it has one.
    fn keep(&mut self, touches: Vec<(Touch, Vec<crate::paths::Built>)>) {
        for (t, built) in touches {
            if built.is_empty() {
                self.read.touches.push(t);
            } else {
                self.read.waiting.push((t, built));
            }
        }
    }

    /// `receiver.method(..)`: the program's method on the class the receiver holds, or a
    /// library's operation, which may touch the world through the value its chain made.
    fn method_call(&mut self, steps: &mut Steps<'t, '_>, c: TsNode<'t>, f: TsNode<'t>) {
        let src = self.src;
        let (Some(value), Some(field)) = (f.child_by_field_name("value"), f.child_by_field_name("field")) else { return };
        let method = text(field, src).to_string();
        let receiver = match value.kind() {
            "self" | "identifier" => self.value_class(value),
            "field_expression" => {
                let v = value.child_by_field_name("value").map(|v| opened(v, src));
                let fld = value.child_by_field_name("field").map(|x| text(x, src)).unwrap_or("");
                match v.map(|v| v.kind()) {
                    Some("self") => self.attr_types.get(fld).cloned(),
                    Some("identifier") => v
                        .and_then(|v| self.locals.get(text(v, src), v.start_byte()))
                        .and_then(|t| self.store.classes.get(t))
                        .and_then(|c| c.attr_types.get(fld).cloned()),
                    _ => None,
                }
            }
            // the value a call gives (`Trend::new(settings).save(..)`), or one written another way:
            // the class the body says it is
            _ => self.value_class(value),
        };
        let own = receiver.as_deref().and_then(|r| self.store.method_of(r, &method)).filter(|t| !t.starts_with("ext:"));
        if own.is_none() && self.changes_library_value(value, &method) {
            let hands = self.call_hands(c, Some(value), &steps.kept);
            let mut changed = self.hands(&[Some(value)], &steps.kept);
            // a name no step made, a collection a macro built (`vec![]`), holds what is put into it
            if changed.is_empty() && value.kind() == "identifier" {
                let put = c
                    .child_by_field_name("arguments")
                    .map(named_children)
                    .unwrap_or_default()
                    .into_iter()
                    .find_map(|a| self.value_class(a));
                changed.push(Held::new(put.unwrap_or_default(), text(value, src)));
            }
            steps.changes(c, &method, hands, self.gives(c, None), changed);
            return;
        }
        match (own, receiver) {
            (Some(t), r) => self.own_step(steps, c, &t, Some(value), r.as_deref(), None),
            // a method not found on a name's type (`to_string`, `clone`, `first` of the records a
            // slice holds) is the language's: a step where the body names what it gives, and what
            // it does where its value goes nowhere Atlas does not see
            (None, Some(_)) if matches!(value.kind(), "self" | "identifier" | "field_expression") => {
                self.library_step(steps, c, &method, Some(value));
                self.dropped(steps, c, &format!("{}.{method}", text(value, src)));
            }
            // a library's operation; on what a call gives, also one its class does not have, which
            // is the wrapper's the class is given in (`map_or` of the `Option<&Node>` of `ix.node(..)`)
            _ => {
                self.library_step(steps, c, &method, Some(value));
                let touches = self.chain_touches(c, value, &method);
                if touches.is_empty() {
                    self.dropped(steps, c, text(f, src));
                }
                self.keep(touches);
            }
        }
    }

    /// Whether `method` changes the library value it is called on: the value's type, as the body
    /// declares it or else as it makes it, is of a crate whose row of `knowledge::MUTATORS` has
    /// the method. A prelude type (`Vec`, `String`) is std's.
    fn changes_library_value(&self, value: TsNode<'t>, method: &str) -> bool {
        let ty = match self.declared(value) {
            Some(ty) => ty,
            None => match library_chain(value, self.m, self.src, &self.made_by, &|v| self.declared(v)) {
                Some(made) => made.ty,
                None => return self.changed_in_place(value, method),
            },
        };
        let Some(t) = self.resolve(&ty) else { return false };
        let Some(path) = t.strip_prefix("ext:") else { return false };
        let krate = match path.split_once('.') {
            Some((k, _)) => k,
            None if knowledge::RUST_PRELUDE.contains(&path) => knowledge::RUST_PRELUDE_CRATE,
            None => return false,
        };
        knowledge::mutates(knowledge::Registry::Crates, krate, method)
    }

    /// Whether `method` changes a name bound `mut` it is called on, whose type the body does not
    /// write (`text.push_str(..)` on `let mut text = fs::read_to_string(p)?`, `out.push(..)` on `let
    /// mut out = vec![]`): Rust lets a method change only a value its name is bound `mut` to, and
    /// a method of the prelude's collections of the name that changes what it runs on is that one.
    fn changed_in_place(&self, value: TsNode<'t>, method: &str) -> bool {
        value.kind() == "identifier"
            && self.mutable.holds(text(value, self.src), value.start_byte())
            && knowledge::mutates(knowledge::Registry::Crates, knowledge::RUST_PRELUDE_CRATE, method)
    }

    /// The library type a value is declared to be: a parameter's (`conn: &Connection`), or a
    /// field's of the body's own class or of a local's class (`self.conn`, `store.conn`).
    fn declared(&self, v: TsNode) -> Option<Vec<String>> {
        let src = self.src;
        let ext = match v.kind() {
            "identifier" => self.foreign.get(text(v, src), v.start_byte()).cloned(),
            "field_expression" => {
                let field = text(v.child_by_field_name("field")?, src);
                let owner = opened(v.child_by_field_name("value")?, src);
                let cls = match owner.kind() {
                    "self" => self.cls.clone(),
                    "identifier" => self.locals.get(text(owner, src), owner.start_byte()).cloned(),
                    _ => None,
                }?;
                self.store.classes.get(&cls)?.foreign.get(field).cloned()
            }
            _ => None,
        }?;
        Some(ext.strip_prefix("ext:")?.split('.').map(String::from).collect())
    }

    /// What a library's method call touches, through the value its chain made: a value made by a
    /// touch is at that touch's place (`File::open(p)`, `Command::new("git")`), and a call on it
    /// adds a place only when it names its own (`conn.execute("INSERT INTO log ..")`); a value
    /// built without touching anything touches where it is used, at the place its building
    /// named (`OpenOptions::new().append(true).open("log")`).
    /// The path a file is at is the one the call that names it is handed, as built, and a statement
    /// is what the call that runs it is handed.
    fn chain_touches(&self, c: TsNode<'t>, value: TsNode<'t>, method: &str) -> Vec<(Touch, Vec<crate::paths::Built>)> {
        let src = self.src;
        let made = library_chain(value, self.m, src, &self.made_by, &|v| self.declared(v));
        // the outer call of a chain stands for the chain: `client.get(url).send()`
        if made.is_some() && c.parent().map_or(false, |p| p.kind() == "field_expression") {
            return vec![];
        }
        let subtree = || {
            let mut l = place_literals(c, src);
            l.extend(made.as_ref().and_then(|x| x.origin).map(|o| place_literals(o, src)).unwrap_or_default());
            l
        };
        let moved = made.as_ref().and_then(|x| {
            let here: Vec<(String, TsNode)> = x.here.iter().cloned().chain([(method.to_string(), c)]).collect();
            // a builder's calls up to its `build` build, and move nothing (C-BUILDER)
            let (start, end) = knowledge::BUILDER;
            let built = if x.ctor.last().map(String::as_str) == Some(start) {
                x.before.iter().chain(&here).position(|(op, _)| op == end).map_or(usize::MAX, |i| i + 1)
            } else {
                0
            };
            // a value its constructor makes without touching anything (`OpenOptions::new()`) is set up
            // by a file opener's options until a call opens it: `append(true)` and `create(true)` say
            // how the call that opens it goes, and move nothing themselves
            let option = |op: &str| knowledge::OPENS_TO_WRITE.contains(&op);
            let set_up =
                x.ctor.last().is_some_and(|c| knowledge::BUILDS.contains(&c.as_str())) && effects::boundary_rust(&x.ctor).is_none();
            let opened = match set_up {
                true => x.before.iter().chain(&here).position(|(op, _)| !option(op) && moved_by(&x.ty, op).is_some()).unwrap_or(usize::MAX),
                false => 0,
            };
            let moves = |i: usize, op: &str| if i < built || (i < opened && option(op)) { None } else { moved_by(&x.ty, op) };
            let n = x.before.len();
            // the move is named by the first call a library knows by name, else the first that moves
            let (kind, op, call) = here
                .iter()
                .enumerate()
                .filter_map(|(i, (op, node))| moves(n + i, op).map(|k| (k, op.clone(), *node)))
                .min_by_key(|(k, _, _)| *k == Crossing::to(World::Disk))?;
            let touched =
                effects::boundary_rust(&x.ctor).is_some() || x.before.iter().enumerate().any(|(i, (op, _))| moves(i, op).is_some());
            let writes = x.before.iter().chain(&here).any(|(o, _)| knowledge::OPENS_TO_WRITE.contains(&o.as_str()));
            let own: Vec<String> = here
                .iter()
                .flat_map(|(_, n)| n.child_by_field_name("arguments").map(|a| place_literals(a, src)).unwrap_or_default())
                .collect();
            Some((if writes && kind.world == World::Disk { Crossing::write(World::Disk) } else { kind }, op, call, touched, own))
        });
        let first = |call: TsNode<'t>| {
            call.child_by_field_name("arguments").and_then(|a| named_children(a).into_iter().find(|x| !x.kind().ends_with("comment")))
        };
        let now = |ts: Vec<Touch>| ts.into_iter().map(|t| (t, vec![])).collect::<Vec<_>>();
        match moved {
            // a statement run on a connection
            Some((kind, op, call, _, _)) if matches!(kind.world, World::Db(_)) && self.statement(kind, first(call)).is_some() => {
                let built = self.statement(kind, first(call)).unwrap_or_default();
                self.touch(c, kind, &op, &[])
                    .into_iter()
                    .take(1)
                    .map(|t| (Touch { artifact: String::new(), dir: None, ..t }, built.clone()))
                    .collect()
            }
            Some((kind, op, _, true, own)) => now(self.touch(c, kind, &op, &own).into_iter().filter(|t| !t.artifact.is_empty()).collect()),
            // a file opened where a builder's value is used, at the path its `open` is handed
            Some((kind, op, call, false, _)) if kind.world == World::Disk && first(call).is_some() => {
                let built = first(call).map(|a| self.eval.of(self.at(), a, 0)).unwrap_or_default();
                self.touch(c, kind, &op, &subtree()).into_iter().map(|t| (t, built.clone())).collect()
            }
            // a socket's traffic, at the address its operation is handed (`send_to(buf, addr)`), else
            // at the place of the connection it runs on, which the callers hand as they hand a path
            Some((kind, op, call, false, _))
                if kind.world == World::Net
                    && made.as_ref().map_or(false, |x| knowledge::socket_path(&x.ty))
                    && knowledge::rust_socket_op(&op) =>
            {
                let address = knowledge::RUST_SOCKET_ADDRESSES
                    .iter()
                    .find(|(o, _)| *o == op)
                    .and_then(|(_, i)| positional(call).get(*i).copied().flatten());
                let on = function(call).filter(|f| f.kind() == "field_expression").and_then(|f| f.child_by_field_name("value"));
                let built = crate::paths::request_of(&op, address.or(on).map(|a| self.eval.of(self.at(), a, 0)).unwrap_or_default());
                self.touch(c, kind, &op, &subtree()).into_iter().map(|t| (t, built.clone())).collect()
            }
            // a request on a client, at the URL its method is handed, first unless the crate says
            // otherwise (knowledge::REQUEST_URL_AT)
            Some((kind, op, call, false, _)) if kind.world == World::Net && first(call).is_some() => {
                let krate = made.as_ref().and_then(|x| x.ty.first()).map(String::as_str);
                let at = knowledge::REQUEST_URL_AT.iter().find(|(k, m, _)| Some(*k) == krate && *m == op).map_or(0, |(_, _, i)| *i);
                let url = call
                    .child_by_field_name("arguments")
                    .and_then(|a| named_children(a).into_iter().filter(|x| !x.kind().ends_with("comment")).nth(at));
                let built = crate::paths::request_of(&op, url.map(|a| self.eval.of(self.at(), a, 0)).unwrap_or_default());
                self.touch(c, kind, &op, &subtree()).into_iter().map(|t| (t, built.clone())).collect()
            }
            // a method of a path, at the path it is called on: `cli.raw.exists()`
            Some((kind, op, _, false, _)) if kind.world == World::Disk && knowledge::RUST_PATH_READS.contains(&op.as_str()) => {
                let built = self.eval.of(self.at(), value, 0);
                self.touch(c, kind, &op, &subtree()).into_iter().map(|t| (t, built.clone())).collect()
            }
            Some((kind, op, _, false, _)) => now(self.touch(c, kind, &op, &subtree())),
            // a method of a path reads the file the path is
            None if knowledge::RUST_PATH_READS.contains(&method) => {
                let built = self.eval.of(self.at(), value, 0);
                self.touch(c, Crossing::read(World::Disk), method, &subtree()).into_iter().map(|t| (t, built.clone())).collect()
            }
            None => vec![],
        }
    }

    /// The touches of a call that names what it reaches by what it is handed first (the program
    /// `Command::new` runs): the value the function holds there, a constant's as much as a
    /// literal's, one touch for each value it can be; as written when the function does not hold
    /// it whole.
    fn touch_handed(&self, c: TsNode<'t>, tag: Crossing, op: &str, first: Option<TsNode<'t>>) -> Vec<Touch> {
        let lits = literals(c, self.src, STRINGS);
        let values: Vec<String> = match first.filter(|a| literals(*a, self.src, STRINGS).is_empty()) {
            Some(a) => {
                let built: Vec<crate::paths::Built> = self.eval.of(self.at(), a, 0).iter().flat_map(crate::paths::chosen).collect();
                let whole = built.iter().all(crate::paths::known);
                if whole {
                    built.iter().map(crate::paths::render).collect()
                } else {
                    vec![]
                }
            }
            None => vec![],
        };
        if values.is_empty() {
            return self.touch(c, tag, op, &lits);
        }
        let mut out = vec![];
        for v in values {
            let held = self.locals.visible(c.start_byte());
            let mut cr = Crosses::read(
                c,
                self.src,
                tag,
                None,
                op,
                &[vec![v.clone()], lits.clone()].concat(),
                STRINGS,
                &held,
                &path_maker_named(self.src),
            );
            cr.what = v;
            out.extend(cr.touches(None));
        }
        out
    }

    /// The statement a call to a database whose statements have a grammar is handed, as built.
    fn statement(&self, kind: Crossing, arg: Option<TsNode<'t>>) -> Option<Vec<crate::paths::Built>> {
        let World::Db(db) = kind.world else { return None };
        arg.map(|a| self.eval.of(self.at(), a, 0)).filter(|b| crate::paths::is_statement(db, b))
    }

    /// A method called by its path runs on the value it is handed first, as the same call in method
    /// syntax does (`Source::read(p)` as `p.read()`): the method of that name the value's class runs,
    /// that class being the one the path is qualified by (`<Plain as Source>::read(p)`) or else the
    /// one the body knows the value to be of. Gives the method, the value and its class; none for a
    /// function that takes no `self`.
    fn on_first(&self, c: TsNode<'t>, t: &str) -> Option<(String, TsNode<'t>, Option<String>)> {
        let n = self.store.nodes.get(t).filter(|n| n.kind == NodeKind::Method && n.params.first().map_or(false, |p| p.receiver))?;
        let first = positional(c).into_iter().next().flatten()?;
        let named = c
            .child_by_field_name("function")
            .and_then(|f| qualified(f, self.src))
            .and_then(|(ty, _)| self.resolve(&ty))
            .and_then(|r| self.class_of(&r));
        let class = named.or_else(|| self.value_class(first));
        let ran = class.as_deref().and_then(|k| self.store.method_of(k, &n.name)).filter(|m| !m.starts_with("ext:"));
        Some((ran.unwrap_or_else(|| t.to_string()), first, class))
    }

    /// `path::to::f(..)`, `Self::f(..)`, `Type::new(..)`: the program's function or type, or a
    /// library's, which may cross into the world.
    fn path_call(&mut self, steps: &mut Steps<'t, '_>, c: TsNode<'t>, mut parts: Vec<String>) {
        if parts.is_empty() {
            // a call through a value no path names (`(self.on_done)()`) is one Atlas cannot follow
            if let Some(f) = function(c).filter(|_| c.kind() == "call_expression") {
                self.dropped(steps, c, text(f, self.src));
            }
            return;
        }
        let target = match (parts[0].as_str(), &self.cls) {
            ("Self", Some(cls)) => {
                let t = self.store.method_of(cls, parts.get(1).map(String::as_str).unwrap_or("")).unwrap_or_else(|| cls.clone());
                parts[0] = cls.clone();
                Some(t)
            }
            ("Self", None) => None,
            // `Type::new(..)`: a function on a type
            _ => self.resolve_value(c, &parts).or_else(|| {
                let r = self
                    .resolve(&parts[..parts.len().saturating_sub(1)])
                    .filter(|r| !r.starts_with("ext:") && parts.len() > 1)
                    .and_then(|r| self.class_of(&r))?;
                Some(self.store.method_of(&r, &parts[parts.len() - 1]).unwrap_or(r))
            }),
        };
        match target {
            // `Type::method(..)` with no such method known: keep the name, it is what was called
            Some(t) if !t.starts_with("ext:") => {
                let via = (parts.len() > 1 && parts[0] != "Self").then(|| parts.last().cloned()).flatten();
                match self.on_first(c, &t) {
                    Some((ran, first, class)) => self.own_step(steps, c, &ran, Some(first), class.as_deref(), via),
                    None => self.own_step(steps, c, &t, None, None, via),
                }
            }
            _ => {
                let op = parts.last().cloned().unwrap_or_default();
                let full = expand(self.m, &parts);
                if knowledge::RUST_COMMAND_LINE.contains(&full.iter().map(String::as_str).collect::<Vec<_>>().as_slice()) {
                    self.reads_command_line(c);
                }
                self.library_step(steps, c, &op, None);
                self.read.unresolved.push((full.iter().take(2).cloned().collect::<Vec<_>>().join("."), self.bucket(&parts)));
                let exits = matches!(full.as_slice(), [.., module, f] if knowledge::ends_the_run(module, f));
                self.read.exits |= exits;
                match effects::boundary_rust(&full) {
                    Some(tag) => {
                        let touches = self.crossed(c, tag, &op);
                        self.keep(touches);
                    }
                    // what the tables say nothing of beside what it gives, Atlas does not see
                    None if !exits => self.dropped(steps, c, &parts.join("::")),
                    None => {}
                }
            }
        }
    }

    /// What the path a call not followed starts from is bound to: a name the function binds, a
    /// `use` of a library, one of the program's own items, or nothing the program binds: the
    /// prelude, or a crate the program depends on, where a longer path starts (`std::fs::read`,
    /// `serde_json::to_string`).
    fn bucket(&self, parts: &[String]) -> Bucket {
        let head = parts[0].as_str();
        if self.bound.get_or_init(|| bound_in(self.def, self.src)).contains(head) {
            return Bucket::Local;
        }
        if self.m.bindings.get(head).map_or(false, |b| b.starts_with("ext:")) {
            return Bucket::Library;
        }
        let free = head != "Self" && self.resolve(parts).map_or(false, |r| r.starts_with("ext:"));
        if !free {
            Bucket::Unknown
        } else if knowledge::RUST_PRELUDE.contains(&head) {
            Bucket::Builtin
        } else if parts.len() > 1 {
            Bucket::Library
        } else {
            Bucket::Unknown
        }
    }

    /// What a library's call by path touches: a file call the file at the path it is handed, as
    /// built (one that moves or copies reads the path it is handed first and writes the one it
    /// is handed second: `fs::copy(a, b)`); a database call the tables its statement names; any
    /// other what its literals name.
    fn crossed(&self, c: TsNode<'t>, tag: Crossing, op: &str) -> Vec<(Touch, Vec<crate::paths::Built>)> {
        let args: Vec<TsNode<'t>> = positional(c).into_iter().map_while(|a| a).collect();
        let now = |ts: Vec<Touch>| ts.into_iter().map(|t| (t, vec![])).collect::<Vec<_>>();
        if let Some(built) = self.statement(tag, args.first().copied()) {
            return self
                .touch(c, tag, op, &[])
                .into_iter()
                .take(1)
                .map(|t| (Touch { artifact: String::new(), dir: None, ..t }, built.clone()))
                .collect();
        }
        // a file database is opened on the file it is handed
        if let (World::Db(db), Some(a)) = (tag.world, args.first()) {
            if knowledge::opens_a_file(db, op) {
                let built = self.eval.of(self.at(), *a, 0);
                return self
                    .touch(c, tag, op, &[])
                    .into_iter()
                    .take(1)
                    .map(|t| (Touch { artifact: String::new(), dir: None, ..t }, built.clone()))
                    .collect();
            }
        }
        let files: Vec<(Crossing, TsNode<'t>)> = match crate::body::sides(op, args.len()) {
            Some(sides) if tag.world == World::Disk => sides.into_iter().zip(args).collect(),
            _ if tag.world == World::Disk && !args.is_empty() => vec![(tag, args[0])],
            // a request is its method and its URL, whole where the callers make it, as a path is
            _ if tag.world == World::Net && !args.is_empty() => {
                let built = crate::paths::request_of(op, self.eval.of(self.at(), args[0], 0));
                return self.touch(c, tag, op, &literals(c, self.src, STRINGS)).into_iter().map(|t| (t, built.clone())).collect();
            }
            _ => return now(self.touch_handed(c, tag, op, args.first().copied())),
        };
        let (mut out, two) = (vec![], files.len() > 1);
        for (kind, a) in files {
            let lits = literals(a, self.src, STRINGS);
            let mut cr = Crosses::read(
                c,
                self.src,
                kind,
                None,
                op,
                &lits,
                STRINGS,
                &self.locals.visible(c.start_byte()),
                &path_maker_named(self.src),
            );
            // a file is named by the argument that is its path, not by what is written into it; a
            // call handed two paths names each by its own
            cr.what = if two {
                lits.first().map(|l| l.trim().to_string()).unwrap_or_else(|| text(a, self.src).to_string())
            } else {
                crate::tree::what_in(a, self.src, STRINGS)
            };
            cr.fixed = crate::tree::head_literal(a, self.src, STRINGS, &path_maker_named(self.src));
            let built = self.eval.of(self.at(), a, 0);
            out.extend(cr.touches(None).into_iter().map(|t| (t, built.clone())));
        }
        out
    }
}

/// How many `else if` steps an `if` sits down its chain.
fn else_if_depth(n: TsNode) -> u32 {
    let mut d = 0;
    let mut cur = n;
    while let Some(up) = cur.parent().filter(|p| p.kind() == "else_clause").and_then(|e| e.parent()).filter(|p| p.kind() == "if_expression")
    {
        d += 1;
        cur = up;
    }
    d
}

/// The method of a call that chooses (`x.unwrap_or_else(..)`, `ok.then(..)`), if it is one.
fn chooser<'s>(n: TsNode, src: &'s [u8]) -> Option<&'s str> {
    let m = function(n).filter(|f| f.kind() == "field_expression")?.child_by_field_name("field")?;
    Some(text(m, src)).filter(|m| OR_ELSE.contains(m) || THEN.contains(m))
}

/// The macros that end a body without giving a value: it panics.
const DIVERGES: &[&str] = &["panic", "unreachable", "todo", "unimplemented"];

/// Whether a block never runs on past its end: its last statement returns, breaks, continues or
/// panics.
fn leaves(block: Option<TsNode>, src: &[u8]) -> bool {
    let Some(last) = block.and_then(|b| named_children(b).into_iter().filter(|s| !s.kind().ends_with("comment")).last()) else {
        return false;
    };
    let e = if last.kind() == "expression_statement" { last.named_child(0).unwrap_or(last) } else { last };
    match e.kind() {
        "return_expression" | "break_expression" | "continue_expression" => true,
        "macro_invocation" => e.child_by_field_name("macro").map_or(false, |m| DIVERGES.contains(&text(m, src))),
        _ => false,
    }
}

/// Rust's choices: `if`/`else if`/`else` (with `if let`), `match`, `let .. else`, the
/// `Option`/`Result` methods that run their argument only when the receiver has nothing, and
/// `bool::then`, which runs its argument only when the receiver holds. What the call is made on
/// always runs: it flows through an or-else into what the call gives, and decides a `then`.
const CHOICES: ChoiceSyntax = ChoiceSyntax {
    chooses: |n, src| match n.kind() {
        "if_expression" | "match_block" | "match_arm" => true,
        "let_declaration" => n.child_by_field_name("alternative").is_some(),
        "call_expression" => chooser(n, src).is_some(),
        _ => false,
    },
    continues: |n| {
        if n.kind() == "match_arm" {
            return n.parent();
        }
        let up = n.parent().filter(|p| p.kind() == "else_clause")?.parent()?;
        (n.kind() == "if_expression" && up.kind() == "if_expression").then_some(up)
    },
    part: |c, child, src| {
        let is = |f: &str| c.child_by_field_name(f).map(|x| x.id()) == Some(child.id());
        match c.kind() {
            "if_expression" if is("condition") => Part::Decides,
            "if_expression" if is("consequence") => Part::Arm(else_if_depth(c)),
            "if_expression" if is("alternative") => {
                if named_children(child).iter().any(|x| x.kind() == "if_expression") {
                    Part::Through
                } else {
                    Part::Arm(else_if_depth(c) + 1)
                }
            }
            "match_block" if child.kind() == "match_arm" => Part::Arm(nth_of_kind(c, child)),
            // a pattern and its guard decide which arm runs
            "match_arm" if is("pattern") => Part::Decides,
            "call_expression" if is("function") && chooser(c, src).map_or(false, |m| THEN.contains(&m)) => Part::Decides,
            "call_expression" if is("function") => Part::Through,
            "call_expression" if is("arguments") => Part::Arm(1),
            // the `else` of a `let .. else` runs when the pattern does not match
            "let_declaration" if is("alternative") => Part::Arm(0),
            _ => Part::Through,
        }
    },
    // a `let .. else`, whose `else` must leave, and an `if` without an `else` whose block and each
    // `else if`'s leaves: what follows runs otherwise
    guards: |n, src| match n.kind() {
        "let_declaration" => n.child_by_field_name("alternative").is_some(),
        "if_expression" => {
            let mut at = n;
            loop {
                if !leaves(at.child_by_field_name("consequence"), src) {
                    return false;
                }
                match at.child_by_field_name("alternative").and_then(|e| named_children(e).into_iter().next()) {
                    None => return true,
                    Some(next) if next.kind() == "if_expression" => at = next,
                    Some(_) => return false,
                }
            }
        }
        _ => false,
    },
};

/// Where Rust's values go: `let x = v`, `if let x = v`, `x = v`, `x += v`, `for x in v`; `return` and the last
/// expression of the function's block leave; a panic fails.
const USES: UseSyntax = UseSyntax {
    binds: |n| match n.kind() {
        "let_declaration" | "let_condition" => n.child_by_field_name("pattern").zip(n.child_by_field_name("value")),
        "assignment_expression" | "compound_assignment_expr" => n.child_by_field_name("left").zip(n.child_by_field_name("right")),
        "for_expression" => n.child_by_field_name("pattern").zip(n.child_by_field_name("value")),
        _ => None,
    },
    leaves: |n, body| {
        // the last named child of the block, which stands in it
        n.kind() == "return_expression"
            || (!matches!(n.kind(), "expression_statement" | "let_declaration" | "empty_statement")
                && body.named_child(body.named_child_count().saturating_sub(1)).map(|l| l.id()) == Some(n.id()))
    },
    fails: panics,
    hands: |n, child| match n.kind() {
        "call_expression" => n.child_by_field_name("arguments").map(|a| a.id()) == Some(child.id()),
        "macro_invocation" => child.kind() == "token_tree",
        _ => false,
    },
    // `&mut s` handed to a call: the call writes into `s`
    changes: |n| {
        let mut out = vec![];
        let Some(args) = n.child_by_field_name("arguments").filter(|_| n.kind() == "call_expression") else { return out };
        for i in 0..args.named_child_count() {
            let Some(a) = args.named_child(i).filter(|a| a.kind() == "reference_expression") else { continue };
            let mutable = (0..a.child_count()).any(|j| a.child(j).is_some_and(|c| c.kind() == "mutable_specifier"));
            if let Some(v) = a.child_by_field_name("value").filter(|v| mutable && v.kind() == "identifier") {
                out.push(v);
            }
        }
        out
    },
    splits: |n, child| n.kind() == "match_expression" && n.child_by_field_name("value").map(|v| v.id()) == Some(child.id()),
    names: &["identifier"],
    keeps: &["field_expression", "index_expression"],
    field: |n, src| {
        (n.kind() == "field_expression")
            .then(|| Some((n.child_by_field_name("value")?, text(n.child_by_field_name("field")?, src).to_string())))
            .flatten()
    },
    pairs: |targets, value| targets.kind() == "tuple_expression" && value.kind() == "tuple_expression",
    // `&&` and `||` are choices; `*x` is the value `x` refers to
    computes: |n, src| match n.kind() {
        "binary_expression" => n.child_by_field_name("operator").is_some_and(|o| !matches!(text(o, src), "&&" | "||")),
        "unary_expression" => n.child(0).is_some_and(|o| o.kind() != "*"),
        "tuple_expression" | "array_expression" | "type_cast_expression" => true,
        _ => false,
    },
    wrappers: WRAPPERS,
    // a statement ends in `;`: a block's last expression without one is the block's value
    drops: |n, child, src| match n.kind() {
        "expression_statement" => n.named_child(0).map(|c| c.id()) == Some(child.id()) && text(n, src).trim_end().ends_with(';'),
        "let_declaration" => {
            n.child_by_field_name("value").map(|v| v.id()) == Some(child.id())
                && n.child_by_field_name("pattern").is_some_and(|p| text(p, src) == "_")
        }
        _ => false,
    },
    nested: &[
        "function_item",
        "impl_item",
        "trait_item",
        "mod_item",
        "struct_item",
        "enum_item",
        "use_declaration",
        "attribute_item",
        "macro_definition",
        "const_item",
        "static_item",
    ],
    reads: &["self", "string_literal", "raw_string_literal"],
    // a part of a path (`Error::Empty`), a macro's name, and among a macro's tokens a field after a `.`
    label: |n, _| {
        let Some(p) = n.parent() else { return false };
        match p.kind() {
            "scoped_identifier" | "scoped_type_identifier" => true,
            "macro_invocation" => p.child_by_field_name("macro").is_some_and(|m| m.id() == n.id()),
            "token_tree" => {
                matches!(n.prev_sibling().map(|s| s.kind()), Some("." | "::"))
                    || matches!(n.next_sibling().map(|s| s.kind()), Some("::" | "!"))
            }
            _ => false,
        }
    },
    binders: |n, child| {
        let is = |f: &str| n.child_by_field_name(f).is_some_and(|x| x.id() == child.id());
        match n.kind() {
            "closure_expression" if is("parameters") => Some(true),
            "match_pattern" if !is("condition") => Some(false),
            _ => None,
        }
    },
    updates: &["compound_assignment_expr"],
    parts: |_, _| false,
    raises: |_, _, _| false,
    // `Err(e)`, `Result::Err(e)`
    errs: |n, child, src| {
        n.kind() == "arguments"
            && n.named_child_count() == 1
            && n.named_child(0).map(|c| c.id()) == Some(child.id())
            && n.parent()
                .and_then(|c| c.child_by_field_name("function"))
                .is_some_and(|f| matches!(text(f, src).rsplit("::").next(), Some("Err")))
    },
    // a statement without `;` is the value of the block it ends (`match v { .. }` last in a function)
    tails: &["block", "else_clause", "match_arm", "match_expression", "expression_statement"],
    propagates: Some(&["try_expression"]),
    catches: |_, _, _| None,
    args,
};

/// The arguments a Rust call writes, each with the parameter it fills (`body::placed`): the value a
/// method is called on (`x` of `x.f(..)`, also a step's result, `Trend::new(..)` of
/// `Trend::new(..).save(..)`), each argument by its place, a struct literal's fields by their names and
/// what its `..base` spreads; among a macro's tokens, each group by its place and the names a method
/// written there is called on.
fn args(n: TsNode, src: &[u8]) -> Vec<(Range, Slot)> {
    let range = |n: TsNode| (n.start_byte(), n.end_byte());
    let between = |g: &[TsNode]| Some((g.first()?.start_byte(), g.last()?.end_byte()));
    match n.kind() {
        "call_expression" => {
            let recv = function(n).filter(|f| f.kind() == "field_expression").and_then(|f| f.child_by_field_name("value"));
            let mut out: Vec<(Range, Slot)> = recv.map(|r| (range(r), Slot::Receiver)).into_iter().collect();
            let written = n.child_by_field_name("arguments").map(named_children).unwrap_or_default();
            out.extend(crate::body::placed(written.into_iter().filter(|a| !a.kind().ends_with("comment")), |_| None, |_| None));
            out
        }
        "struct_expression" => {
            let fields = n.child_by_field_name("body").map(named_children).unwrap_or_default();
            let named = |f: TsNode| -> Option<(Range, Slot)> {
                match f.kind() {
                    "field_initializer" => {
                        Some((range(f.child_by_field_name("value")?), Slot::Name(text(f.child_by_field_name("field")?, src).to_string())))
                    }
                    "shorthand_field_initializer" => Some((range(f), Slot::Name(text(f, src).to_string()))),
                    "base_field_initializer" => Some((range(f), Slot::Keywords)),
                    _ => None,
                }
            };
            fields.into_iter().filter_map(named).collect()
        }
        "token_tree" => {
            let recv = match token_callee(n, src) {
                Some(Called::Method(names, _)) => between(&names).map(|r| (r, Slot::Receiver)),
                _ => None,
            };
            let groups = paths::token_groups(n).into_iter().enumerate().filter_map(|(i, g)| Some((between(&g)?, Slot::At(i as u32))));
            recv.into_iter().chain(groups).collect()
        }
        _ => vec![],
    }
}

/// Rust's comparisons: `==` and `!=` against a string or a member of a type (`Mode::Fast`), a
/// `match` on either, and `matches!` on strings.
const COMPARES: CompareSyntax = CompareSyntax {
    kinds: &["binary_expression", "match_expression", "macro_invocation"],
    operands: tested,
    each: |n| vec![n],
    literal: |n, src| matches!(n.kind(), "string_literal" | "raw_string_literal").then(|| unquote(text(n, src))),
    member: |n| (n.kind() == "scoped_identifier").then(|| n.child_by_field_name("path")).flatten().map(|p| (p.start_byte(), p.end_byte())),
    subject: |_| true,
    over: matched,
    copies: |n| (n.kind() == "let_declaration").then(|| n.child_by_field_name("pattern").zip(n.child_by_field_name("value"))).flatten(),
    place,
    binds: USES.binds,
};

/// The value an expression reads in place: a name, a field or an item, through `&`.
fn place(n: TsNode) -> Option<TsNode> {
    match n.kind() {
        "identifier" | "self" | "field_expression" | "index_expression" => Some(n),
        "reference_expression" => n.child_by_field_name("value").and_then(place),
        _ => None,
    }
}

/// Whether the bytes of a qualifier write a type: its last segment is written as Rust writes a
/// type's name, in upper camel case (`Mode`, `Self`, `crate::Mode`), which the compiler's naming
/// lints hold every type to.
fn names_type(q: &str) -> bool {
    q.rsplit("::").next().and_then(|s| s.chars().next()).is_some_and(|c| c.is_uppercase())
}

/// The two operands of a test of equality.
fn tested<'t>(n: TsNode<'t>, src: &[u8]) -> Option<(TsNode<'t>, TsNode<'t>)> {
    let op = n.child_by_field_name("operator").map(|o| text(o, src)).unwrap_or("");
    matches!(op, "==" | "!=").then_some((n.child_by_field_name("left")?, n.child_by_field_name("right")?))
}

/// A `match`, with the block that holds its arms, or a `matches!(kind, "a" | "b")`, decided where
/// it stands: the value and the cases of the patterns (each string, and each path pattern alone or
/// among alternatives: `Mode::Fast | Mode::Slow =>`; a variant taken apart, `Shape::Circle(r)`, is
/// a sum unwrapped, no case).
fn matched<'t>(n: TsNode<'t>, src: &[u8]) -> Option<(String, Vec<TsNode<'t>>, Option<TsNode<'t>>)> {
    fn strings(p: TsNode) -> Vec<TsNode> {
        walk(p, &["string_literal", "raw_string_literal"])
    }
    fn paths(p: TsNode) -> Vec<TsNode> {
        named_children(p)
            .into_iter()
            .flat_map(|c| match c.kind() {
                "scoped_identifier" => vec![c],
                "or_pattern" => paths(c),
                _ => vec![],
            })
            .collect()
    }
    if n.kind() == "match_expression" {
        let (Some(value), Some(block)) = (n.child_by_field_name("value"), n.child_by_field_name("body")) else { return None };
        // a pattern's guard (`x if x.kind() == "a"`) tests another value
        let unguarded = |p: TsNode<'t>| {
            let guard = p.child_by_field_name("condition").map(|g| g.id());
            named_children(p).into_iter().filter(move |c| Some(c.id()) != guard)
        };
        let cases = named_children(block)
            .into_iter()
            .filter_map(|a| a.child_by_field_name("pattern"))
            .flat_map(|p| unguarded(p).flat_map(strings).chain(paths(p)))
            .collect();
        return Some((text(value, src).to_string(), cases, Some(block)));
    }
    if n.kind() != "macro_invocation" || n.child_by_field_name("macro").map(|x| text(x, src)) != Some("matches") {
        return None;
    }
    let tt = named_children(n).into_iter().find(|c| c.kind() == "token_tree")?;
    // a nested `( .. )` is a token tree of its own, so the first comma among the tokens splits
    let kids: Vec<TsNode> = (0..tt.child_count()).filter_map(|i| tt.child(i)).collect();
    let comma = kids.iter().position(|k| k.kind() == ",").filter(|c| *c > 1)?;
    let guard = kids.iter().skip(comma).position(|k| text(*k, src) == "if").map_or(kids.len() - 1, |i| comma + i);
    let (from, to) = (kids[comma].end_byte(), kids[guard].start_byte());
    let cases = strings(tt).into_iter().filter(|s| s.start_byte() >= from && s.end_byte() <= to).collect();
    let subject = String::from_utf8_lossy(&src[kids[1].start_byte()..kids[comma - 1].end_byte()]).to_string();
    Some((subject, cases, None))
}

/// Rust's own types: std, core and alloc, the containers and wrappers, and the associated types
/// a trait names (`Self::Target`, `S::Ok`).
pub(super) const OWN: OwnTypes = knowledge::RUST_OWN_TYPES;
