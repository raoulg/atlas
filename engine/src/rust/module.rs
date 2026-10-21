//! A Rust file as the reader holds it, and the vocabulary every pass reads it with: its items,
//! their docs and attributes, the paths and types they name, the signature of a function, and
//! where a path resolves to.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use anyhow::{Context, Result};
use indexmap::IndexMap;
use tree_sitter::{Node as TsNode, Parser, Tree};

use crate::builder::SigInfo;
use crate::knowledge;
use crate::model::{Node, NodeKind, Param, Returns};
use crate::tree::{named_children, text, walk, Wrappers};

/// What hands on the value inside it: parentheses, a reference `&x`, `x?` and `x.await`.
pub(super) const WRAPPERS: Wrappers = &["parenthesized_expression", "reference_expression", "try_expression", "await_expression"];

pub(crate) struct Crate {
    pub(super) name: String,
    pub(crate) dir: PathBuf,
}

pub(super) struct Module {
    pub(super) id: String,
    pub(super) krate: String,
    /// The directory its crate's manifest is in, by its path in the repository (`.` at its top):
    /// what `CARGO_MANIFEST_DIR` holds where the crate is built.
    pub(super) manifest_dir: String,
    pub(super) parent: String,
    pub(super) path: PathBuf,
    pub(super) src: Vec<u8>,
    pub(super) tree: Tree,
    pub(super) bindings: HashMap<String, String>,
    pub(super) wildcards: Vec<String>,
    /// The libraries' modules a glob import brings every name of: `ext:plotters.prelude` for
    /// `use plotters::prelude::*`.
    pub(super) globs: Vec<String>,
    /// The names the module's items declare as type parameters (`DB` in `fn draw<DB: Backend>`).
    pub(super) generics: HashSet<String>,
    pub(super) symbols: HashMap<String, String>,
    pub(super) is_root: bool,
    /// For the root of a binary target (`src/main.rs`, `src/bin/x.rs`): the binary's name.
    pub(super) bin: Option<String>,
    /// The items a node of the graph is defined by, by their first line and name: where they are
    /// in the file.
    pub(super) defs: HashMap<(u32, String), (usize, usize)>,
    /// Whether the module is a library's, read as far as the program's calls go into it: its
    /// paths are the library's, `ext:` where Atlas does not read them.
    pub(super) library: bool,
}

pub(super) fn new_parser() -> Result<Parser> {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter::Language::new(tree_sitter_rust::LANGUAGE)).context("loading the Rust grammar")?;
    Ok(parser)
}

/// `a::b::C`, `crate::x`, `Self`, identifiers -> parts.
pub(super) fn path_parts(n: Option<TsNode>, src: &[u8]) -> Vec<String> {
    let Some(n) = n else { return vec![] };
    match n.kind() {
        "identifier" | "type_identifier" | "crate" | "super" | "self" | "metavariable" => vec![text(n, src).to_string()],
        "scoped_identifier" | "scoped_type_identifier" => {
            let mut p = path_parts(n.child_by_field_name("path"), src);
            if let Some(name) = n.child_by_field_name("name") {
                p.push(text(name, src).to_string());
            }
            p
        }
        "generic_type" | "reference_type" | "dynamic_type" | "abstract_type" => {
            path_parts(n.child_by_field_name("type").or_else(|| named_children(n).into_iter().next()), src)
        }
        "generic_function" => path_parts(n.child_by_field_name("function"), src),
        "bracketed_type" => path_parts(named_children(n).into_iter().next(), src),
        _ => vec![],
    }
}

/// The type names a type mentions as values, as written: each generic's own name, and its
/// arguments only where it holds or stands for them (`Option<Vec<Node>>` -> `[[Option], [Vec],
/// [Node]]`, `impl Iterator<Item = Node>` -> `[[Iterator], [Node]]`), never a generic class's
/// own parameters (`DrawingArea<DB, Shift>` -> `[[DrawingArea]]`). References, slices, arrays,
/// tuples, `impl` and `dyn` go through to what they hold; a `fn(..)` or `Fn(..)` type is a
/// callback and names none.
pub(super) fn type_names(t: Option<TsNode>, src: &[u8]) -> Vec<Vec<String>> {
    let Some(t) = t else { return vec![] };
    let mut out = vec![];
    collect_types(t, src, &mut out);
    out
}

/// Whether a type changes through a shared reference, or holds one that does: `&RefCell<Log>`,
/// `Rc<RefCell<Log>>`, `&[Cell<u8>]`, `&AtomicUsize`. Its value may change though no `&mut` is
/// written; the type says so as `&mut` does.
pub(super) fn changes_inside(t: TsNode, src: &[u8]) -> bool {
    match t.kind() {
        "type_identifier" | "scoped_type_identifier" => path_parts(Some(t), src).last().map_or(false, |n| knowledge::rust_interior(n)),
        "generic_type" => {
            let head = path_parts(t.child_by_field_name("type"), src).last().cloned().unwrap_or_default();
            knowledge::rust_interior(&head)
                || (knowledge::rust_container(&head)
                    && t.child_by_field_name("type_arguments")
                        .map_or(false, |a| named_children(a).into_iter().any(|x| changes_inside(x, src))))
        }
        "reference_type" | "array_type" | "tuple_type" => named_children(t).into_iter().any(|x| changes_inside(x, src)),
        _ => false,
    }
}

fn collect_types(n: TsNode, src: &[u8], out: &mut Vec<Vec<String>>) {
    match n.kind() {
        "type_identifier" | "scoped_type_identifier" => {
            let parts = path_parts(Some(n), src);
            if !parts.is_empty() {
                out.push(parts);
            }
        }
        "generic_type" => {
            let parts = path_parts(n.child_by_field_name("type"), src);
            let holds = parts.last().map_or(false, |g| knowledge::rust_container(g));
            if !parts.is_empty() {
                out.push(parts);
            }
            if let Some(args) = n.child_by_field_name("type_arguments").filter(|_| holds) {
                collect_types(args, src, out);
            }
        }
        // `Item = Node` names the value an iterator gives, not a type called `Item`
        "type_binding" => {
            if let Some(t) = n.child_by_field_name("type") {
                collect_types(t, src, out);
            }
        }
        "function_type" => {}
        _ => {
            for c in named_children(n) {
                collect_types(c, src, out);
            }
        }
    }
}

/// Attributes (`#[test]`, `#[derive(..)]`) sit before an item as siblings.
pub(super) fn attrs_of(item: TsNode, src: &[u8]) -> Vec<String> {
    let mut out = vec![];
    let mut p = item.prev_named_sibling();
    while let Some(a) = p {
        if a.kind() != "attribute_item" {
            break;
        }
        out.push(text(a, src).trim_start_matches("#[").trim_end_matches(']').to_string());
        p = a.prev_named_sibling();
    }
    out
}

/// The first line of the `///` doc comment before an item.
pub(super) fn doc_of(item: TsNode, src: &[u8]) -> String {
    let mut lines = vec![];
    let mut p = item.prev_named_sibling();
    while let Some(c) = p {
        match c.kind() {
            "line_comment" if text(c, src).starts_with("///") => lines.push(text(c, src).trim_start_matches('/').trim().to_string()),
            "attribute_item" | "line_comment" | "block_comment" => {}
            _ => break,
        }
        p = c.prev_named_sibling();
    }
    lines.reverse();
    lines.into_iter().find(|l| !l.is_empty()).unwrap_or_default()
}

/// Leading `//!` comments of a file.
pub(super) fn module_doc(root: TsNode, src: &[u8]) -> String {
    for c in named_children(root) {
        match c.kind() {
            "line_comment" => {
                let t = text(c, src);
                if let Some(rest) = t.strip_prefix("//!") {
                    let rest = rest.trim();
                    if !rest.is_empty() {
                        return rest.to_string();
                    }
                } else {
                    return String::new();
                }
            }
            _ => return String::new(),
        }
    }
    String::new()
}

/// Top-level items, with inline `mod x { }` bodies flattened in.
pub(super) fn items<'t>(root: TsNode<'t>) -> Vec<TsNode<'t>> {
    let mut out = vec![];
    let mut stack: Vec<TsNode<'t>> = named_children(root).into_iter().rev().collect();
    while let Some(n) = stack.pop() {
        if n.kind() == "mod_item" {
            if let Some(body) = n.child_by_field_name("body") {
                for k in named_children(body).into_iter().rev() {
                    stack.push(k);
                }
            }
        } else {
            out.push(n);
        }
    }
    out
}

/// A signature as data: each parameter by name, type and kind, and what it gives back: the value,
/// which for a `Result<T, E>` is `T`, whatever that is; `E` is what it fails with. `self`,
/// `&self`, `&mut self` and `self: Box<Self>` are the value the method is called on; a `&mut`
/// says the body may change what it is handed, and so does a type that changes through a shared
/// reference (`&RefCell<T>`).
pub(super) fn signature(f: TsNode, src: &[u8]) -> (Vec<Param>, Option<Returns>, SigInfo) {
    let mut params = vec![];
    let mut info = SigInfo::default();
    if let Some(pn) = f.child_by_field_name("parameters") {
        for p in named_children(pn) {
            let mutable = |t: TsNode| t.kind() == "reference_type" && named_children(t).iter().any(|c| c.kind() == "mutable_specifier");
            match p.kind() {
                "self_parameter" => {
                    params.push(Param { name: "self".into(), receiver: true, changes: text(p, src).contains("mut"), ..Default::default() })
                }
                "parameter" => {
                    let pattern = p.child_by_field_name("pattern");
                    let ty = p.child_by_field_name("type");
                    params.push(Param {
                        name: pattern.map(|x| text(x, src).trim_start_matches("mut ").to_string()).unwrap_or_default(),
                        ty: ty.map(|t| text(t, src).to_string()).unwrap_or_default(),
                        receiver: pattern.map_or(false, |x| x.kind() == "self"),
                        changes: ty.map_or(false, |t| mutable(t) || changes_inside(t, src)),
                        ..Default::default()
                    });
                    info.ptypes.extend(type_names(ty, src));
                }
                _ => {}
            }
        }
    }
    let rt = f.child_by_field_name("return_type");
    info.rtypes = type_names(rt.map(|r| value_of(r, src).0), src);
    (params, rt.map(|r| returns(r, src)), info)
}

/// The value a return type gives, and whether it may fail instead: a `Result<T, E>` gives `T`
/// when it does not fail, a reference what it refers to.
fn value_of<'t>(rt: TsNode<'t>, src: &[u8]) -> (TsNode<'t>, bool) {
    let mut t = rt;
    let mut fails = false;
    loop {
        let inner = match t.kind() {
            "reference_type" => t.child_by_field_name("type"),
            "generic_type" if t.child_by_field_name("type").map_or(false, |n| text(n, src).rsplit("::").next() == Some("Result")) => {
                fails = true;
                t.child_by_field_name("type_arguments").and_then(|a| named_children(a).into_iter().next())
            }
            _ => None,
        };
        match inner {
            Some(i) => t = i,
            None => return (t, fails),
        }
    }
}

/// What a return type gives: its value (`T` of a `Result<T, E>`), `()` no value, `Self` a value
/// of the type the `impl` is for.
pub(super) fn returns(rt: TsNode, src: &[u8]) -> Returns {
    let (t, fails) = value_of(rt, src);
    let (value, own) = match (t.kind(), text(t, src)) {
        ("unit_type", _) => (String::new(), false),
        (_, "Self") => (String::new(), true),
        (_, v) => (v.to_string(), false),
    };
    Returns { written: text(rt, src).to_string(), value, own, fails, ..Default::default() }
}

/// A method whose first parameter is the value it is called on.
pub(super) fn takes_self(params: &[Param]) -> bool {
    params.first().map_or(false, |p| p.receiver)
}

/// A function on a type that gives a value of it: `new`, `default`, `from_yaml`.
pub(super) fn constructs(params: &[Param], returns: &Option<Returns>, ty: &str) -> bool {
    !takes_self(params) && returns.as_ref().map_or(false, |r| r.own || r.value == ty)
}

/// Visible outside its module: `pub`, `pub(crate)`, `pub(super)`.
pub(super) fn visible(item: TsNode) -> bool {
    named_children(item).iter().any(|c| c.kind() == "visibility_modifier")
}

pub(super) fn fn_name(f: TsNode, src: &[u8]) -> String {
    f.child_by_field_name("name").map(|n| text(n, src)).unwrap_or("").to_string()
}

/// A path as written -> an internal id or `ext:...`.
pub(super) fn path_to_id(m: &Module, parts: &[String], crate_ids: &HashMap<String, String>, is_module: &dyn Fn(&str) -> bool) -> String {
    let Some(head) = parts.first() else { return String::new() };
    let (base, rest): (String, &[String]) = match head.as_str() {
        "crate" => (m.krate.clone(), &parts[1..]),
        "self" => (m.id.clone(), &parts[1..]),
        "super" => {
            let mut base = m.parent.clone();
            let mut i = 1;
            while parts.get(i).map(String::as_str) == Some("super") {
                base = base.rsplit_once('.').map(|x| x.0.to_string()).unwrap_or(base);
                i += 1;
            }
            (base, &parts[i..])
        }
        h if crate_ids.contains_key(h) => (crate_ids[h].clone(), &parts[1..]),
        h if is_module(&format!("{}.{h}", m.id)) => (format!("{}.{h}", m.id), &parts[1..]),
        _ => return format!("ext:{}", parts.join(".")),
    };
    let mut id = base;
    for p in rest {
        id.push('.');
        id.push_str(p);
    }
    // a library's path is its own, found where Atlas reads it
    if m.library {
        id.insert_str(0, "ext:");
    }
    id
}

/// The names a file's items declare as type parameters: a function's, an impl's, a trait's, a
/// type's.
pub(super) fn type_parameters(tree: &Tree, src: &[u8]) -> HashSet<String> {
    walk(tree.root_node(), &["type_parameters"])
        .into_iter()
        .flat_map(named_children)
        .filter_map(|t| match t.kind() {
            "type_identifier" => Some(t),
            "type_parameter" | "constrained_type_parameter" | "optional_type_parameter" => {
                t.child_by_field_name("name").or_else(|| t.child_by_field_name("left"))
            }
            _ => None,
        })
        .map(|n| text(n, src).to_string())
        .collect()
}

/// A path as written -> what it names: one of the program's items, or `ext:..` a library's. Rust
/// keeps types and modules apart from values, and the head of a longer path (`download::f`) is a
/// module or a type, never a function bound by the same name (`pub use download::download`).
/// A name the module neither declares nor imports by name is the prelude's, or one a library's
/// glob import brings in (`DrawingArea` after `use plotters::prelude::*`), when the module has
/// one such glob.
pub(super) fn resolve(
    m: &Module,
    parts: &[String],
    modules: &IndexMap<String, Module>,
    nodes: &IndexMap<String, Node>,
    crate_ids: &HashMap<String, String>,
) -> Option<String> {
    let head = parts.first()?;
    let is_module = |id: &str| nodes.get(id).map_or(false, |n| n.kind == NodeKind::Module);
    let value = |id: &&String| parts.len() > 1 && nodes.get(*id).map_or(false, |n| matches!(n.kind, NodeKind::Function | NodeKind::Method));
    let (mut target, rest): (String, &[String]) = if matches!(head.as_str(), "crate" | "self" | "super") || crate_ids.contains_key(head) {
        (path_to_id(m, parts, crate_ids, &is_module), &[])
    } else if let Some(s) = m.symbols.get(head).filter(|s| !value(s)) {
        (s.clone(), &parts[1..])
    } else if let Some(b) = m.bindings.get(head).filter(|b| !value(b)) {
        (b.clone(), &parts[1..])
    } else if let Some(w) = m.wildcards.iter().find(|w| nodes.contains_key(&format!("{w}.{head}"))) {
        (format!("{w}.{head}"), &parts[1..])
    } else if nodes.get(&format!("{}.{head}", m.id)).map_or(false, |n| n.kind == NodeKind::Module) {
        // a child module declared with `mod x;`, addressed as `x::f`
        (format!("{}.{head}", m.id), &parts[1..])
    } else if let Some(glob) = glob_of(m, parts) {
        (format!("{glob}.{head}"), &[])
    } else {
        (format!("ext:{}", parts.join(".")), &[])
    };
    for p in rest {
        target.push('.');
        target.push_str(p);
    }
    // a library's item where Atlas read its code
    if let Some(lib) = target.strip_prefix("ext:") {
        return Some(library_item(lib, modules, nodes, 0).unwrap_or(target));
    }
    if nodes.contains_key(&target) {
        return Some(target);
    }
    // a re-export through another module's `pub use`
    let (modid, nm) = target.rsplit_once('.')?;
    let re = modules.get(modid)?.bindings.get(nm)?;
    nodes.contains_key(re).then(|| re.clone())
}

/// The item of a library's code Atlas read that a library's path names (`serde_json.from_str`):
/// followed from the crate's root through the modules the path names and the names they re-export
/// or bring in by a glob. None where Atlas did not read it.
fn library_item(path: &str, modules: &IndexMap<String, Module>, nodes: &IndexMap<String, Node>, depth: u32) -> Option<String> {
    let parts: Vec<&str> = path.split('.').collect();
    let mut cur = modules.get(parts[0]).filter(|m| m.library)?;
    for (i, seg) in parts.iter().enumerate().skip(1) {
        let below = format!("{}.{seg}", cur.id);
        if let Some(child) = modules.get(&below) {
            cur = child;
            continue;
        }
        let rest: String = parts[i + 1..].iter().map(|p| format!(".{p}")).collect();
        if nodes.contains_key(&below) {
            let full = format!("{below}{rest}");
            return nodes.contains_key(&full).then_some(full);
        }
        if depth >= 8 {
            return None;
        }
        if let Some(b) = cur.bindings.get(*seg) {
            return library_item(&format!("{}{rest}", b.strip_prefix("ext:").unwrap_or(b)), modules, nodes, depth + 1);
        }
        // a glob brings in the names of a module read
        return cur
            .globs
            .iter()
            .filter_map(|g| library_module(g.strip_prefix("ext:").unwrap_or(g), modules))
            .find_map(|g| library_item(&format!("{}.{seg}{rest}", g.id), modules, nodes, depth + 1));
    }
    Some(cur.id.clone())
}

/// The module of a library's code Atlas read that a path names through the modules alone.
fn library_module<'m>(path: &str, modules: &'m IndexMap<String, Module>) -> Option<&'m Module> {
    let mut parts = path.split('.');
    let mut cur = modules.get(parts.next()?).filter(|m| m.library)?;
    for seg in parts {
        cur = modules.get(&format!("{}.{seg}", cur.id))?;
    }
    Some(cur)
}

/// The library glob a name is brought in by: a name of one segment the module neither declares
/// nor imports by name, which is not the prelude's nor a type parameter, when the module has one
/// library glob; with two the code does not say which.
fn glob_of<'m>(m: &'m Module, parts: &[String]) -> Option<&'m String> {
    let ([name], [glob]) = (parts, m.globs.as_slice()) else { return None };
    (name != "Self" && !knowledge::RUST_PRELUDE.contains(&name.as_str()) && !m.generics.contains(name)).then_some(glob)
}

/// What a path names where a value is wanted, as where it is called: where it names a module,
/// the function of that name the module's parent declares or re-exports
/// (`mod download; pub use download::download;`), since Rust keeps modules apart from values.
pub(super) fn resolve_value(
    m: &Module,
    parts: &[String],
    modules: &IndexMap<String, Module>,
    nodes: &IndexMap<String, Node>,
    crate_ids: &HashMap<String, String>,
) -> Option<String> {
    let r = resolve(m, parts, modules, nodes, crate_ids)?;
    if nodes.get(&r).map_or(true, |n| n.kind != NodeKind::Module) {
        return Some(r);
    }
    let value = r.rsplit_once('.').and_then(|(parent, name)| {
        let home = modules.get(parent)?;
        resolve(home, &[name.to_string()], modules, nodes, crate_ids).filter(|v| nodes.get(v).map_or(false, |n| n.kind != NodeKind::Module))
    });
    Some(value.unwrap_or(r))
}

/// The kinds of item a node of the graph is defined by.
const DEFINES: &[&str] = &["function_item", "function_signature_item", "struct_item", "enum_item", "trait_item"];

/// A file's items by their first line and name, at their place in the file; the first of two
/// alike.
pub(super) fn definitions(tree: &Tree, src: &[u8]) -> HashMap<(u32, String), (usize, usize)> {
    let mut out = HashMap::new();
    for d in walk(tree.root_node(), DEFINES) {
        if let Some(name) = d.child_by_field_name("name") {
            out.entry((d.start_position().row as u32 + 1, text(name, src).to_string())).or_insert((d.start_byte(), d.end_byte()));
        }
    }
    out
}

/// The item a node is defined by: the module's file, or the function, struct, enum or trait.
pub(super) fn def_node<'t>(m: &'t Module, n: &Node) -> Option<TsNode<'t>> {
    if n.kind == NodeKind::Module {
        return Some(m.tree.root_node());
    }
    let (start, _) = n.span?;
    let &(from, to) = m.defs.get(&(start, n.name.clone()))?;
    let mut d = m.tree.root_node().descendant_for_byte_range(from, to)?;
    while !DEFINES.contains(&d.kind()) || d.byte_range() != (from..to) {
        d = d.parent()?;
    }
    Some(d)
}

/// Expand the head of a call path through its binding, so `fs::read` becomes `std.fs.read`.
pub(super) fn expand(m: &Module, parts: &[String]) -> Vec<String> {
    match parts.first().and_then(|h| m.bindings.get(h)) {
        Some(b) if b.starts_with("ext:") => {
            let mut out: Vec<String> = b[4..].split('.').map(String::from).collect();
            out.extend(parts[1..].iter().cloned());
            out
        }
        _ => parts.to_vec(),
    }
}

/// The names a pattern binds: `x` of `Some(x)`, `a` and `b` of `(a, mut b)`; the type a pattern
/// takes apart (`Some`) and a guard bind nothing.
fn pattern(p: TsNode, src: &[u8], out: &mut HashSet<String>) {
    match p.kind() {
        "identifier" | "shorthand_field_identifier" => {
            out.insert(text(p, src).to_string());
        }
        _ => {
            let skipped = [p.child_by_field_name("type"), p.child_by_field_name("condition")];
            for c in named_children(p).into_iter().filter(|c| !skipped.contains(&Some(*c))) {
                pattern(c, src, out);
            }
        }
    }
}

/// The names in scope where `at` stands in the body of `def`, as Rust scopes a binding: the
/// function's parameters; each `let` written before it in a block around it, and each function
/// declared in one; and the patterns of the closures, loops, `match` arms, `if let`s and
/// `while let`s whose bodies hold it. A name bound there shadows the item of that name: `wrapped`
/// after `let wrapped = vec![]` is the local, not the function `wrapped` of the module.
pub(super) fn bound_at<'t>(def: TsNode<'t>, at: TsNode<'t>, src: &[u8]) -> HashSet<String> {
    fn lets(c: Option<TsNode>) -> Vec<TsNode> {
        c.map(|c| walk(c, &["let_condition"])).unwrap_or_default().into_iter().filter_map(|l| l.child_by_field_name("pattern")).collect()
    }
    let mut patterns: Vec<TsNode<'t>> = vec![];
    let mut child = at;
    while let Some(p) = child.parent() {
        let is = |f: &str| p.child_by_field_name(f).map(|x| x.id()) == Some(child.id());
        match p.kind() {
            "block" => patterns.extend(named_children(p).into_iter().filter_map(|s| match s.kind() {
                "let_declaration" if s.end_byte() <= child.start_byte() => s.child_by_field_name("pattern"),
                "function_item" => s.child_by_field_name("name"),
                _ => None,
            })),
            "closure_expression" if is("body") => {
                patterns.extend(p.child_by_field_name("parameters").map(named_children).unwrap_or_default())
            }
            "for_expression" if is("body") => patterns.extend(p.child_by_field_name("pattern")),
            "match_arm" if !is("pattern") => patterns.extend(p.child_by_field_name("pattern")),
            "if_expression" if is("consequence") => patterns.extend(lets(p.child_by_field_name("condition"))),
            "while_expression" if is("body") => patterns.extend(lets(p.child_by_field_name("condition"))),
            _ => {}
        }
        if p.id() == def.id() {
            let params = p.child_by_field_name("parameters").map(named_children).unwrap_or_default();
            patterns.extend(params.into_iter().filter_map(|x| x.child_by_field_name("pattern")));
            break;
        }
        child = p;
    }
    let mut out = HashSet::new();
    for p in patterns {
        pattern(p, src, &mut out);
    }
    out
}

/// Whether a path written at `at` in the body of `def` names a local bound there (`bound_at`)
/// rather than an item: a path of one segment can.
pub(super) fn shadowed(def: Option<TsNode>, at: TsNode, parts: &[String], src: &[u8]) -> bool {
    match (def, parts) {
        (Some(d), [name]) => bound_at(d, at, src).contains(name),
        _ => false,
    }
}

/// The names a function binds: its parameters, its type parameters and those of the impl or
/// trait it is in, and in its body each `let`, closure parameter, loop or match binding, and
/// function declared there.
pub(super) fn bound_in(def: TsNode, src: &[u8]) -> HashSet<String> {
    let mut out = HashSet::new();
    let mut scope = Some(def);
    while let Some(s) = scope {
        if s.id() == def.id() || matches!(s.kind(), "impl_item" | "trait_item") {
            for t in s.child_by_field_name("type_parameters").map(named_children).unwrap_or_default() {
                let name = if t.kind() == "type_identifier" {
                    Some(t)
                } else {
                    t.child_by_field_name("left").or_else(|| t.child_by_field_name("name"))
                };
                out.extend(name.map(|n| text(n, src).to_string()));
            }
        }
        scope = s.parent();
    }
    for n in
        walk(def, &["parameter", "closure_parameters", "let_declaration", "for_expression", "match_arm", "let_condition", "function_item"])
    {
        match n.kind() {
            "closure_parameters" => named_children(n).into_iter().for_each(|p| pattern(p, src, &mut out)),

            "function_item" if n.id() != def.id() => out.extend(n.child_by_field_name("name").map(|x| text(x, src).to_string())),
            "function_item" => {}
            _ => n.child_by_field_name("pattern").into_iter().for_each(|p| pattern(p, src, &mut out)),
        }
    }
    out
}

/// The variants of `Option` and `Result` that hold the value they give: where a value is followed,
/// `Some(x)` and `Ok(x)` are `x`.
pub(super) const HOLDING: &[&str] = &["Some", "Ok"];

/// `Option` and `Result` methods whose argument is the other arm: `find(g).unwrap_or_else(|| infer(g))`.
pub(super) const OR_ELSE: &[&str] = &["unwrap_or_else", "unwrap_or", "or_else", "or", "map_or_else", "map_or"];
