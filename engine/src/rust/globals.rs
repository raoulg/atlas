//! The values a crate keeps between calls, which its functions change: a `static mut`, or a
//! `static` of a type that changes in place (a lock, a cell, an atomic), declared as an item or
//! among a macro's tokens (`thread_local!`, `lazy_static!`). Each is a place, `Global(module.NAME)`,
//! or `Global(module.function.NAME)` for one a function's body declares: a function that changes
//! it writes it, one that uses it otherwise reads it, wherever a path reaches it (`LATEST`,
//! `rates::LATEST`, `crate::rates::LATEST`). A static no function changes, and a `const`, is a
//! constant of the code, and no place.

use std::collections::{HashMap, HashSet};

use indexmap::IndexMap;
use tree_sitter::Node as TsNode;

use super::module::{bound_in, def_node, items, path_parts, path_to_id, resolve, type_names, Module};
use crate::builder::GraphBuilder;
use crate::knowledge;
use crate::model::{Dir, NodeKind, Touch, World};
use crate::tree::{named_children, parent, text, walk};

/// A static that can change, as declared: its name, and whether it is `mut` itself.
struct Static {
    name: String,
    mutable: bool,
}

/// One function's uses of the statics that can change: where each is kept, its name, which way,
/// and the line of the first such use.
type Uses = Vec<(String, String, Dir, u32)>;

/// Every function's reads and writes of the statics its crate changes, as touches of the places
/// they are.
pub(super) fn read(modules: &IndexMap<String, Module>, store: &mut GraphBuilder, crate_ids: &HashMap<String, String>) {
    let mut uses: Vec<(String, Uses)> = vec![];
    {
        let nodes = &store.nodes;
        // a library's type that changes in place, not one of the program's of the same name
        let in_place = |m: &Module, ty: &[String]| {
            ty.last().map_or(false, |t| knowledge::rust_interior(t))
                && !resolve(m, ty, modules, nodes, crate_ids).and_then(|r| nodes.get(&r)).map_or(false, |n| n.kind == NodeKind::Class)
        };
        let mut kept: HashMap<String, bool> = HashMap::new();
        for m in modules.values() {
            for s in items(m.tree.root_node()).into_iter().flat_map(|i| declared(i, m, &|t| in_place(m, t))) {
                kept.insert(format!("{}.{}", m.id, s.name), s.mutable);
            }
        }
        for n in nodes.values().filter(|n| matches!(n.kind, NodeKind::Function | NodeKind::Method)) {
            let m = &modules[&n.module];
            let Some(def) = def_node(m, n) else { continue };
            let Some(body) = def.child_by_field_name("body") else { continue };
            let src = &m.src;
            let own: HashMap<String, bool> = walk(body, &["static_item", "macro_invocation"])
                .into_iter()
                .flat_map(|i| declared(i, m, &|t| in_place(m, t)))
                .map(|s| (s.name, s.mutable))
                .collect();
            let bound = bound_in(def, src);
            let mut found: Uses = vec![];
            for x in walk(body, &["identifier", "scoped_identifier"]) {
                let Some(parts) = used(x, src) else { continue };
                let at = match parts.as_slice() {
                    [name] if bound.contains(name) => None,
                    [name] if own.contains_key(name) => Some((format!("{}.{name}", n.id), own[name])),
                    _ => named(m, &parts, &kept, modules, crate_ids).map(|id| (id.clone(), kept[&id])),
                };
                let Some((id, mutable)) = at else { continue };
                let effect = effect(x, mutable, false, src, 0);
                let name = id.rsplit('.').next().unwrap_or(&id).to_string();
                let line = x.start_position().row as u32 + 1;
                for (dir, yes) in [(Dir::Write, effect.writes), (Dir::Read, effect.reads)] {
                    if yes && !found.iter().any(|(k, _, d, _)| *k == id && *d == dir) {
                        found.push((id.clone(), name.clone(), dir, line));
                    }
                }
            }
            uses.push((n.id.clone(), found));
        }
    }
    let changed: HashSet<String> =
        uses.iter().flat_map(|(_, u)| u.iter().filter(|(_, _, d, _)| *d == Dir::Write).map(|(k, ..)| k.clone())).collect();
    for (f, found) in uses {
        for (artifact, name, dir, line) in found.into_iter().filter(|(k, ..)| changed.contains(k)) {
            store.touch(&f, Touch { what: name.clone(), artifact, op: name, line, ..Touch::new(World::Global, Some(dir)) });
        }
    }
}

/// The statics that can change an item or a statement declares: a `static` item, or those a macro
/// declares among its tokens as Rust writes one (`static ref CACHE: Mutex<..> = ..;`).
fn declared(item: TsNode, m: &Module, in_place: &dyn Fn(&[String]) -> bool) -> Vec<Static> {
    let src = &m.src;
    match item.kind() {
        "static_item" => {
            let Some(name) = item.child_by_field_name("name") else { return vec![] };
            let mutable = named_children(item).iter().any(|c| c.kind() == "mutable_specifier");
            if mutable || type_names(item.child_by_field_name("type"), src).iter().any(|t| in_place(t)) {
                vec![Static { name: text(name, src).to_string(), mutable }]
            } else {
                vec![]
            }
        }
        "macro_invocation" => {
            named_children(item).into_iter().filter(|t| t.kind() == "token_tree").flat_map(|t| in_tokens(t, src, in_place)).collect()
        }
        _ => vec![],
    }
}

/// The statics that can change among a macro's tokens: `static [ref] [mut] NAME: TYPE = ..`.
fn in_tokens(tt: TsNode, src: &[u8], in_place: &dyn Fn(&[String]) -> bool) -> Vec<Static> {
    let kids: Vec<TsNode> = (0..tt.child_count()).filter_map(|i| tt.child(i)).collect();
    let mut out = vec![];
    for i in (0..kids.len()).filter(|i| kids[*i].kind() == "static") {
        let (mut j, mut mutable) = (i + 1, false);
        while let Some(k) = kids.get(j).map(|k| text(*k, src)).filter(|k| matches!(*k, "ref" | "mut")) {
            mutable |= k == "mut";
            j += 1;
        }
        let (Some(name), Some(":")) = (kids.get(j).filter(|k| k.kind() == "identifier"), kids.get(j + 1).map(|k| k.kind())) else {
            continue;
        };
        let ty: Vec<String> = kids[j + 2..]
            .iter()
            .take_while(|k| !matches!(k.kind(), "=" | ";"))
            .filter(|k| k.kind() == "identifier")
            .map(|k| text(*k, src).to_string())
            .collect();
        if mutable || ty.iter().any(|t| in_place(std::slice::from_ref(t))) {
            out.push(Static { name: text(*name, src).to_string(), mutable });
        }
    }
    out
}

/// The path a name in a body stands for, where it names a value: not a part of a longer path, a
/// declaration's name or a `use`; among a macro's tokens, a path not after a `.` and not before a
/// `::` or a `!`.
fn used(x: TsNode, src: &[u8]) -> Option<Vec<String>> {
    let p = parent(x)?;
    if p.kind() != "token_tree" {
        if matches!(
            p.kind(),
            "scoped_identifier" | "static_item" | "use_declaration" | "use_list" | "scoped_use_list" | "use_as_clause" | "use_wildcard"
        ) {
            return None;
        }
        return Some(path_parts(Some(x), src)).filter(|p| !p.is_empty());
    }
    if x.kind() != "identifier" || matches!(x.next_sibling().map(|s| s.kind()), Some("::" | "!")) {
        return None;
    }
    let mut parts = vec![text(x, src).to_string()];
    let mut at = x.prev_sibling();
    while let Some(sep) = at.filter(|s| s.kind() == "::") {
        let before = sep.prev_sibling().filter(|b| matches!(b.kind(), "identifier" | "crate" | "self" | "super"))?;
        parts.insert(0, text(before, src).to_string());
        at = before.prev_sibling();
    }
    // a field, a method, or the name a declaration gives
    if at.map_or(false, |a| a.kind() == "." || matches!(text(a, src), "static" | "ref" | "mut")) {
        return None;
    }
    Some(parts)
}

/// The static a path names, by where it is kept: by its name in its own module, through a `use`
/// (one of its name, of its module, or of everything in its module), as a child module's
/// (`rates::LATEST`), by a path from the crate, or where a `pub use` re-exports it.
fn named(
    m: &Module,
    parts: &[String],
    kept: &HashMap<String, bool>,
    modules: &IndexMap<String, Module>,
    crate_ids: &HashMap<String, String>,
) -> Option<String> {
    let head = parts.first()?;
    let rest: String = parts[1..].iter().map(|p| format!(".{p}")).collect();
    let mut ids: Vec<String> = vec![];
    if matches!(head.as_str(), "crate" | "self" | "super") || crate_ids.contains_key(head) {
        ids.push(path_to_id(m, parts, crate_ids, &|id| modules.contains_key(id)));
    } else {
        ids.extend(m.symbols.get(head).map(|s| format!("{s}{rest}")));
        ids.extend(m.bindings.get(head).map(|b| format!("{b}{rest}")));
        ids.extend(m.wildcards.iter().map(|w| format!("{w}.{}", parts.join("."))));
        ids.push(format!("{}.{}", m.id, parts.join(".")));
    }
    ids.into_iter().find_map(|id| {
        if kept.contains_key(&id) {
            return Some(id);
        }
        let (home, name) = id.rsplit_once('.')?;
        modules.get(home)?.bindings.get(name).filter(|r| kept.contains_key(*r)).cloned()
    })
}

/// Whether a use changes a static, reads it, or both, through what keeps it.
#[derive(Default, Clone, Copy)]
struct Effect {
    writes: bool,
    reads: bool,
}

impl Effect {
    const READS: Effect = Effect { writes: false, reads: true };
    const WRITES: Effect = Effect { writes: true, reads: false };

    fn and(self, o: Effect) -> Effect {
        Effect { writes: self.writes || o.writes, reads: self.reads || o.reads }
    }

    fn any(self) -> bool {
        self.writes || self.reads
    }
}

/// What a use does with a static, written as an expression or among a macro's tokens.
fn effect(x: TsNode, open: bool, refers: bool, src: &[u8], depth: u32) -> Effect {
    if parent(x).map_or(false, |p| p.kind() == "token_tree") {
        among_tokens(x, open, src)
    } else {
        follow(x, open, refers, src, depth)
    }
}

/// What the expression around a use does with the static, followed through the methods that hand
/// its value on (`unwrap`) or give a guard on it (`lock`, `borrow_mut`), its fields and items, `*`
/// and `&`, into a local that keeps a guard or a reference (`let mut g = X.lock().unwrap()`) and
/// into a closure a method hands it to (`CACHE.with(|c| ..)`). A method that changes it, an
/// assignment to what can change and a `&mut` of it write it; anything else reads it. `open` is
/// whether the value can change where the use stands (a `static mut`'s, what a guard gives),
/// `refers` whether the expression still refers to the static rather than holding a copy.
fn follow(mut n: TsNode, mut open: bool, mut refers: bool, src: &[u8], depth: u32) -> Effect {
    let mut handed = Effect::default();
    loop {
        let Some(p) = parent(n) else { return handed.and(Effect::READS) };
        let is = |f: &str| p.child_by_field_name(f).map(|x| x.id()) == Some(n.id());
        match p.kind() {
            "field_expression" if is("value") => {
                let method = p.child_by_field_name("field").map_or("", |f| text(f, src));
                let Some(call) = p
                    .parent()
                    .filter(|c| c.kind() == "call_expression" && c.child_by_field_name("function").map(|f| f.id()) == Some(p.id()))
                else {
                    // a field of the value
                    refers = false;
                    n = p;
                    continue;
                };
                if knowledge::rust_changes(method, open) {
                    return handed.and(Effect::WRITES);
                }
                let guard = knowledge::rust_guards(method);
                let here = closures(call, src)
                    .into_iter()
                    .fold(Effect::default(), |e, (body, name)| e.and(kept_in(body, &name, 0, open || guard, src, depth)));
                // what a method gives after handing the value to a closure is the closure's
                if here.any() && !knowledge::hands_on(method) {
                    return handed.and(here);
                }
                if !guard && !knowledge::hands_on(method) {
                    return handed.and(Effect::READS);
                }
                handed = handed.and(here);
                open |= guard;
                refers |= guard;
                n = call;
            }
            "unary_expression" if p.child(0).map_or(false, |o| o.kind() == "*") => {
                refers = false;
                n = p;
            }
            "reference_expression" => {
                if open && named_children(p).iter().any(|c| c.kind() == "mutable_specifier") {
                    return handed.and(Effect::WRITES);
                }
                refers = true;
                n = p;
            }
            k if super::module::WRAPPERS.contains(&k) => n = p,
            "index_expression" if p.named_child(0).map(|v| v.id()) == Some(n.id()) => {
                refers = false;
                n = p;
            }
            "assignment_expression" | "compound_assignment_expr" if is("left") => {
                return handed.and(if open { Effect::WRITES } else { Effect::READS })
            }
            "let_declaration" if is("value") && refers => return handed.and(local(p, open, src, depth)),
            _ => return handed.and(Effect::READS),
        }
    }
}

/// What a local that keeps a guard or a reference does with the static (`let mut g =
/// X.lock().unwrap()`): what its uses after the `let` do. A `let` that binds a pattern reads it.
fn local(decl: TsNode, open: bool, src: &[u8], depth: u32) -> Effect {
    let name = decl
        .child_by_field_name("pattern")
        .filter(|x| matches!(x.kind(), "identifier" | "mut_pattern"))
        .map(|x| text(x, src).trim_start_matches("mut ").to_string());
    match (name, parent(decl)) {
        (Some(name), Some(scope)) => kept_in(scope, &name, decl.end_byte(), open, src, depth),
        _ => Effect::READS,
    }
}

/// The closures a call is handed, by their body and the name of their first parameter: what a
/// method hands its closure (`CACHE.with(|c| ..)`).
fn closures<'t>(call: TsNode<'t>, src: &[u8]) -> Vec<(TsNode<'t>, String)> {
    let args = call.child_by_field_name("arguments").map(named_children).unwrap_or_default();
    args.into_iter()
        .filter(|a| a.kind() == "closure_expression")
        .filter_map(|c| {
            let first = c.child_by_field_name("parameters").and_then(|ps| named_children(ps).into_iter().next())?;
            let pattern = if first.kind() == "parameter" { first.child_by_field_name("pattern")? } else { first };
            let name = text(pattern, src).trim_start_matches("mut ");
            if !name.chars().all(|ch| ch.is_alphanumeric() || ch == '_') {
                return None;
            }
            Some((c.child_by_field_name("body")?, name.to_string()))
        })
        .collect()
}

/// What the uses of a name that keeps a static (a local holding a guard or a reference, a
/// closure's parameter) do with it, in `scope` from `from` on. Kept and not used, it is read.
fn kept_in(scope: TsNode, name: &str, from: usize, open: bool, src: &[u8], depth: u32) -> Effect {
    if depth > 4 {
        return Effect::READS;
    }
    let uses: Vec<TsNode> = walk(scope, &["identifier"])
        .into_iter()
        .filter(|x| x.start_byte() >= from && text(*x, src) == name && used(*x, src).map_or(false, |p| p.len() == 1))
        .collect();
    uses.iter().fold(Effect { writes: false, reads: uses.is_empty() }, |e, x| e.and(effect(*x, open, true, src, depth + 1)))
}

/// What a use among a macro's tokens does with a static, followed through its fields and methods:
/// `N.fetch_add(1, ..)` changes `N`.
fn among_tokens(x: TsNode, mut open: bool, src: &[u8]) -> Effect {
    let mut at = x.next_sibling();
    while let Some(dot) = at.filter(|d| d.kind() == ".") {
        let Some(method) = dot.next_sibling().filter(|m| m.kind() == "identifier") else { break };
        let Some(args) = method.next_sibling().filter(|a| a.kind() == "token_tree" && a.child(0).map_or(false, |c| c.kind() == "(")) else {
            // a field
            at = method.next_sibling();
            continue;
        };
        let name = text(method, src);
        if knowledge::rust_changes(name, open) {
            return Effect::WRITES;
        }
        let guard = knowledge::rust_guards(name);
        if !guard && !knowledge::hands_on(name) {
            break;
        }
        open |= guard;
        at = args.next_sibling();
    }
    Effect::READS
}
