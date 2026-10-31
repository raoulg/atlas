//! A Python file as the reader holds it, and the vocabulary every pass reads it with: its
//! definitions and their docs, the types and signatures they write, the dotted paths calls are
//! written with, the project's own packages, and where a name resolves to.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use indexmap::IndexMap;
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{self as ast};
use ruff_python_parser::{Mode, ParseOptions};
use ruff_text_size::{Ranged, TextRange};
use tree_sitter::{Node as TsNode, Parser, Tree};

use crate::builder::{Declared, SigInfo};
use crate::knowledge;
use crate::model::{ext, Node, NodeKind, Param, Resolved, Returns};
use crate::tree::{field_children, named_children, text, unquote, walk, Wrappers};

/// What hands on the value inside it: parentheses and `await`.
pub(super) const WRAPPERS: Wrappers = &["parenthesized_expression", "await"];

const BUILTIN_TYPES: &[&str] = &[
    "str",
    "int",
    "float",
    "bool",
    "bytes",
    "list",
    "dict",
    "set",
    "tuple",
    "None",
    "Any",
    "Optional",
    "Union",
    "Iterable",
    "Iterator",
    "Sequence",
    "Mapping",
    "Callable",
    "Literal",
    "TypeVar",
    "Self",
    "ClassVar",
    "type",
    "object",
    "frozenset",
    "Generator",
    "AsyncIterator",
    "Awaitable",
    "Annotated",
];

pub(super) struct Module {
    pub(super) id: String,
    pub(super) path: PathBuf,
    pub(super) src: Vec<u8>,
    pub(super) tree: Tree,
    pub(super) package: String,
    pub(super) bindings: HashMap<String, String>,
    pub(super) symbols: HashMap<String, String>,
    /// The modules it imports every public name of: `from .core import *`.
    pub(super) stars: Vec<String>,
    /// Where each definition is, by its first line and its name: its byte range.
    pub(super) defs: std::cell::OnceCell<HashMap<(u32, String), (usize, usize)>>,
}

/// identifier / attribute / dotted_name -> parts.
pub(super) fn dotted(n: Option<TsNode>, src: &[u8]) -> Vec<String> {
    let Some(n) = n else { return vec![] };
    match n.kind() {
        "identifier" => vec![text(n, src).to_string()],
        "dotted_name" => text(n, src).split('.').map(String::from).collect(),
        "attribute" => {
            let mut base = dotted(n.child_by_field_name("object"), src);
            match (base.is_empty(), n.child_by_field_name("attribute")) {
                (false, Some(a)) => {
                    base.push(text(a, src).to_string());
                    base
                }
                _ => vec![],
            }
        }
        "subscript" => dotted(n.child_by_field_name("value"), src),
        _ => vec![],
    }
}

/// An array annotated with its dtype and shape, jaxtyping style: `Float[Tensor, "batch dim"]`
/// is one type, `Float[Tensor, batch dim]`, its shape the contract a step keeps.
fn shaped<'t>(n: TsNode<'t>, src: &[u8]) -> Option<String> {
    // an expression subscripts (`Float[..]`); an annotation is a generic type of type parameters
    let unwrap = |x: TsNode<'t>| if x.kind() == "type" { named_children(x).into_iter().next().unwrap_or(x) } else { x };
    let (head, args) = match n.kind() {
        "subscript" => (dotted(n.child_by_field_name("value"), src), field_children(n, "subscript")),
        "generic_type" => {
            let kids = named_children(n);
            let params = kids
                .iter()
                .find(|k| k.kind() == "type_parameter")
                .map(|tp| named_children(*tp).into_iter().map(unwrap).collect())
                .unwrap_or_default();
            (dotted(kids.first().copied(), src), params)
        }
        _ => return None,
    };
    let ([array, shape], Some(dtype)) = (args.as_slice(), head.last()) else { return None };
    if shape.kind() != "string"
        || named_children(*shape).iter().any(|c| c.kind() == "interpolation")
        || matches!(dtype.as_str(), "Literal" | "Annotated")
    {
        return None;
    }
    let array = text(*array, src).rsplit('.').next().unwrap_or("");
    let dims = unquote(text(*shape, src)).split_whitespace().collect::<Vec<_>>().join(" ");
    Some(format!("{}[{array}, {dims}]", head.join(".")))
}

/// The unions an annotation writes in place, outermost first: `A | B`, `Union[A, B]`,
/// `Optional[A | B]`; a union inside one is part of it.
pub(super) fn unions<'t>(t: Option<TsNode<'t>>, src: &[u8]) -> Vec<TsNode<'t>> {
    let Some(t) = t else { return vec![] };
    let union = |n: TsNode| match n.kind() {
        "binary_operator" => n.child_by_field_name("operator").map_or(false, |o| text(o, src) == "|"),
        "union_type" => true,
        "subscript" => dotted(n.child_by_field_name("value"), src).last().map_or(false, |h| matches!(h.as_str(), "Union" | "Optional")),
        "generic_type" => {
            dotted(named_children(n).first().copied(), src).last().map_or(false, |h| matches!(h.as_str(), "Union" | "Optional"))
        }
        _ => false,
    };
    let all: Vec<TsNode> =
        walk(t, &["binary_operator", "union_type", "subscript", "generic_type"]).into_iter().filter(|n| union(*n)).collect();
    all.iter()
        .copied()
        .filter(|u| !all.iter().any(|o| o.id() != u.id() && o.start_byte() <= u.start_byte() && u.end_byte() <= o.end_byte()))
        .collect()
}

/// The types a union may be, one per member (`list[A] | B`: A, B), a nested union's among them;
/// none when a member names more than one type (`Callable[[A], B] | None`), which is no value
/// of either.
pub(super) fn members(u: TsNode, src: &[u8]) -> Vec<Vec<String>> {
    let parts: Vec<TsNode> = match u.kind() {
        "binary_operator" => [u.child_by_field_name("left"), u.child_by_field_name("right")].into_iter().flatten().collect(),
        "union_type" => named_children(u),
        "subscript" => field_children(u, "subscript"),
        _ => named_children(u).into_iter().filter(|k| k.kind() == "type_parameter").flat_map(named_children).collect(),
    };
    let mut out = vec![];
    for p in parts {
        let p = if p.kind() == "type" { named_children(p).into_iter().next().unwrap_or(p) } else { p };
        let nested = unions(Some(p), src).first().map_or(false, |n| n.id() == p.id());
        let names = if nested { members(p, src) } else { type_names(Some(p), src) };
        match names.as_slice() {
            [] => {}
            [one] => out.push(one.clone()),
            _ if nested => out.extend(names),
            _ => return vec![],
        }
    }
    out
}

/// An annotation's names apart from the unions it writes, and those unions: `def f(r: Fit |
/// Failed, n: int)` takes one union, which may be a sum, not a `Fit` and a `Failed`. A union whose
/// members are not each one type is read as its names.
fn split_unions(t: Option<TsNode>, src: &[u8]) -> (Vec<Vec<String>>, Vec<crate::builder::Union>) {
    let written: Vec<TsNode> =
        unions(t, src).into_iter().filter(|u| !members(*u, src).is_empty() || type_names(Some(*u), src).is_empty()).collect();
    let names = type_names_outside(t, src, &written);
    let unions = written.iter().map(|u| crate::builder::Union { names: members(*u, src), written: text(*u, src).to_string() }).collect();
    (names, unions)
}

/// All dotted names mentioned in an annotation, e.g. `list[Graph]` -> `[[Graph]]`. A shaped
/// array is one name, `[["Float[Tensor, batch dim]"]]`.
pub(super) fn type_names(t: Option<TsNode>, src: &[u8]) -> Vec<Vec<String>> {
    type_names_outside(t, src, &[])
}

/// The names of an annotation that stand outside the nodes `skip`.
fn type_names_outside(t: Option<TsNode>, src: &[u8], skip: &[TsNode]) -> Vec<Vec<String>> {
    let Some(t) = t else { return vec![] };
    let mut out = vec![];
    let mut keep = |parts: Vec<String>| {
        if parts.last().map_or(false, |l| !BUILTIN_TYPES.contains(&l.as_str())) {
            out.push(parts);
        }
    };
    let arrays: Vec<TsNode> = walk(t, &["subscript", "generic_type"]).into_iter().filter(|n| shaped(*n, src).is_some()).collect();
    // the parameters of a generic class that is not a container: `VectorCache[TVectorizer]`
    let mut params: Vec<TsNode> = vec![];
    for g in walk(t, &["subscript", "generic_type"]) {
        let (head, args) = match g.kind() {
            "subscript" => (dotted(g.child_by_field_name("value"), src), field_children(g, "subscript")),
            _ => (
                dotted(named_children(g).first().copied(), src),
                named_children(g).into_iter().filter(|k| k.kind() == "type_parameter").collect(),
            ),
        };
        if head.last().map_or(false, |h| !knowledge::python_container(h)) {
            params.extend(args);
        } else if head.last().is_some_and(|h| knowledge::PYTHON_MAPS.contains(&h.as_str())) {
            // a map holds its values by its keys, its first parameter: a key is no value it holds
            let keys = if g.kind() == "subscript" { args } else { args.into_iter().flat_map(named_children).collect() };
            params.extend(keys.into_iter().take(1));
        }
    }
    // a pair a collection holds is of a type the code does not name: `list[tuple[date, float]]`
    let head_of = |g: TsNode| match g.kind() {
        "subscript" => dotted(g.child_by_field_name("value"), src),
        _ => dotted(named_children(g).first().copied(), src),
    };
    let generic = |g: &TsNode| matches!(g.kind(), "subscript" | "generic_type");
    for g in walk(t, &["subscript", "generic_type"]) {
        let pair = head_of(g).last().is_some_and(|h| knowledge::PYTHON_TUPLES.contains(&h.as_str()))
            && !text(g, src).trim_end_matches(']').trim_end().ends_with("...");
        let mut up = g.parent();
        while let Some(p) = up.filter(|p| !generic(p)) {
            up = p.parent();
        }
        if pair && up.is_some_and(|p| head_of(p).last().is_some_and(|h| knowledge::python_collection(h))) {
            params.push(g);
        }
    }
    let within = |n: TsNode, outer: &TsNode| outer.start_byte() <= n.start_byte() && n.end_byte() <= outer.end_byte();
    let inside = |n: TsNode| arrays.iter().chain(params.iter()).chain(skip.iter()).any(|a| within(n, a));
    for a in arrays.iter().filter(|a| !arrays.iter().any(|b| b.id() != a.id() && within(**a, b)) && !skip.iter().any(|s| within(**a, s))) {
        keep(vec![shaped(*a, src).unwrap_or_default()]);
    }
    for n in walk(t, &["identifier", "attribute", "string"]) {
        if inside(n) {
            continue;
        }
        if n.kind() == "identifier" && n.parent().map_or(false, |p| p.kind() == "attribute") {
            continue;
        }
        if n.kind() == "string" {
            // a quoted annotation is a forward reference: `"pd.DataFrame"`, `"List[Node]"`
            let lit = text(n, src).trim_matches(|c| c == '"' || c == '\'');
            for name in quoted_values(lit) {
                keep(name.split('.').map(str::to_string).collect());
            }
            continue;
        }
        if n.parent().map_or(false, |p| p.kind() == "string" || p.kind() == "interpolation") {
            continue;
        }
        keep(dotted(Some(n), src));
    }
    out
}

/// The names in a quoted annotation that stand for values: `"List[Node]"` -> Node,
/// `"VectorCache[TVectorizer]"` -> VectorCache, its parameter being a type, `"dict[Mode, Plan]"`
/// -> Plan, its first parameter being a key.
fn quoted_values(lit: &str) -> Vec<&str> {
    let mut out = vec![];
    // per open bracket: whether what it holds are parameters of a generic class, and whether what
    // stands there now is the key of a map
    let mut opaque: Vec<(bool, bool)> = vec![];
    let mut last = "";
    let mut start = None;
    let is_name = |c: char| c.is_alphanumeric() || c == '_' || c == '.';
    for (i, c) in lit.char_indices().chain(std::iter::once((lit.len(), ' '))) {
        if is_name(c) {
            start.get_or_insert(i);
            continue;
        }
        if let Some(s) = start.take() {
            let w = &lit[s..i];
            if w.chars().next().map_or(false, |c| c.is_alphabetic() || c == '_') && !opaque.iter().any(|(o, key)| *o || *key) {
                out.push(w);
            }
            last = w;
        }
        match c {
            '[' => {
                let head = last.rsplit('.').next().unwrap_or(last);
                opaque.push((!knowledge::python_container(head), knowledge::PYTHON_MAPS.contains(&head)));
            }
            ']' => {
                opaque.pop();
            }
            ',' => {
                if let Some((_, key)) = opaque.last_mut() {
                    *key = false;
                }
            }
            _ => {}
        }
        if !c.is_whitespace() {
            last = if c == '[' { "" } else { last };
        }
    }
    out
}

pub(super) fn first_docline(body: Option<TsNode>, src: &[u8]) -> String {
    let Some(body) = body else { return String::new() };
    let Some(first) = named_children(body).into_iter().next() else { return String::new() };
    if first.kind() != "expression_statement" {
        return String::new();
    }
    match named_children(first).into_iter().next() {
        Some(s) if s.kind() == "string" => {
            let raw = text(s, src).trim_matches(|c| c == '"' || c == '\'' || c == ' ' || c == '\n');
            raw.lines().next().unwrap_or("").trim().to_string()
        }
        _ => String::new(),
    }
}

/// Direct child definitions of a block, with their decorators.
pub(super) fn iter_defs<'t>(block: TsNode<'t>, src: &[u8]) -> Vec<(Vec<String>, TsNode<'t>)> {
    let mut out = vec![];
    for ch in named_children(block) {
        match ch.kind() {
            "function_definition" | "class_definition" => out.push((vec![], ch)),
            "decorated_definition" => {
                let decs = named_children(ch)
                    .into_iter()
                    .filter(|d| d.kind() == "decorator")
                    .map(|d| text(d, src).trim_start_matches('@').to_string())
                    .collect();
                if let Some(d) = ch.child_by_field_name("definition") {
                    out.push((decs, d));
                }
            }
            _ => {}
        }
    }
    out
}

/// What a definition is among the definitions of its name in one block: an `@overload`, a
/// property's `@name.setter` or `@name.deleter`, or what runs.
pub(super) fn declared(name: &str, decorators: &[String]) -> Declared {
    let writes = |d: &String| d.strip_prefix(name).and_then(|r| r.strip_prefix('.')).map_or(false, |a| matches!(a, "setter" | "deleter"));
    if decorators.iter().any(|d| d.rsplit('.').next() == Some("overload")) {
        Declared::Overload
    } else if decorators.iter().any(writes) {
        Declared::Writer
    } else {
        Declared::Body
    }
}

/// Whether only the code beside a name may use it: it starts with an underscore. A name the
/// language calls on every user's behalf (`__eq__`, `__len__`, `__getattr__`) is the protocol
/// of the value or module that defines it, which an operator, a builtin or an import runs.
pub(super) fn private(name: &str) -> bool {
    let dunder = name.len() > 4 && name.starts_with("__") && name.ends_with("__");
    name.starts_with('_') && !dunder
}

/// The type that lets every value through unchecked, as typing names it.
const UNCHECKED: &str = "Any";

/// Whether an annotation is the unchecked type: `Any`, `typing.Any`, `"Any"`.
fn unchecked(t: TsNode, src: &[u8]) -> bool {
    text(t, src).trim_matches(['"', '\'']).rsplit('.').next() == Some(UNCHECKED)
}

/// A signature as data: each parameter by name, type and kind, and what it gives back. A method's
/// first parameter is the value or the class it is called on, unless it is a `staticmethod`.
pub(super) fn signature(f: TsNode, src: &[u8], receiver: bool) -> (Vec<Param>, Option<Returns>, SigInfo) {
    let mut params = vec![];
    let mut info = SigInfo::default();
    if let Some(pn) = f.child_by_field_name("parameters") {
        for p in named_children(pn) {
            if matches!(p.kind(), "comment" | "keyword_separator" | "positional_separator") {
                continue;
            }
            let name_node = match p.kind() {
                "identifier" | "list_splat_pattern" | "dictionary_splat_pattern" => Some(p),
                _ => p.child_by_field_name("name").or_else(|| named_children(p).into_iter().next()),
            };
            let pname = name_node.map(|n| text(n, src)).unwrap_or_else(|| text(p, src));
            let t = p.child_by_field_name("type");
            params.push(Param {
                name: pname.trim_start_matches('*').to_string(),
                ty: t.map(|t| text(t, src).to_string()).unwrap_or_default(),
                receiver: receiver && params.is_empty(),
                rest: pname.starts_with('*'),
                changes: false,
                unchecked: t.map_or(false, |t| unchecked(t, src)),
                holds: Param::held(type_names(t, src)),
            });
            let (names, unions) = split_unions(t, src);
            info.ptypes.extend(names);
            info.punions.extend(unions);
        }
    }
    let rt = f.child_by_field_name("return_type");
    (info.rtypes, info.runions) = split_unions(rt, src);
    (params, rt.map(|r| returns(r, src)), info)
}

/// What a return annotation gives: `None` no value, `Self` a value of the class it runs on, `Any`
/// a value it does not say.
pub(super) fn returns(rt: TsNode, src: &[u8]) -> Returns {
    let written = text(rt, src).to_string();
    let inner = if rt.kind() == "type" { named_children(rt).into_iter().next().unwrap_or(rt) } else { rt };
    let (value, own) = match inner.kind() {
        "none" => (String::new(), false),
        _ if written.rsplit('.').next() == Some("Self") => (String::new(), true),
        _ => (written.clone(), false),
    };
    Returns { written, value, own, fails: false, unchecked: unchecked(rt, src), holds: Param::held(type_names(Some(rt), src)) }
}

pub(super) fn new_parser() -> Result<Parser> {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter::Language::new(tree_sitter_python::LANGUAGE)).context("loading the Python grammar")?;
    Ok(parser)
}

/// A module from its text: a notebook's is the script its code cells make.
pub(super) fn load_module(parser: &mut Parser, id: &str, path: &Path, is_pkg: bool, text: &[u8]) -> Result<Module> {
    let src = match path.extension().and_then(|x| x.to_str()) {
        Some("ipynb") => {
            notebook_source(&String::from_utf8_lossy(text)).with_context(|| format!("parsing {}", path.display()))?.into_bytes()
        }
        _ => text.to_vec(),
    };
    let tree = parser.parse(&src, None).with_context(|| format!("parsing {}", path.display()))?;
    let package = if is_pkg { id.to_string() } else { id.rsplit_once('.').map(|x| x.0).unwrap_or(id).to_string() };
    Ok(Module {
        id: id.into(),
        path: path.into(),
        src,
        tree,
        package,
        bindings: HashMap::new(),
        symbols: HashMap::new(),
        stars: vec![],
        defs: std::cell::OnceCell::new(),
    })
}

/// A notebook as the script it runs: its code cells in order, joined as ruff and ty join them,
/// so that a position in it is the same for both trees (a cell that starts with a cell magic
/// is left out, as ty leaves it out). What ruff parses as an IPython escape command
/// (`%matplotlib inline`, `!pip install ..`, `files = !ls`) is blanked, so the rest parses as
/// Python: a statement becomes a comment of the same length, a value `...` of the same length. A
/// line that only starts with `%` or `!` inside an expression (`    % len(rows)`, where Black wraps
/// before an operator) is Python and stays.
fn notebook_source(text: &str) -> Result<String> {
    let nb = ruff_notebook::Notebook::from_source_code(text).map_err(|e| anyhow::anyhow!("{e}"))?;
    let source = nb.source_code();
    let parsed = ruff_python_parser::parse_cells_unchecked(source, nb.cell_offsets().content_ranges(), &ParseOptions::from(Mode::Ipython));
    let mut escapes = Escapes::default();
    escapes.visit_body(parsed.suite());
    let mut bytes = source.as_bytes().to_vec();
    for (range, statement) in escapes.found {
        blank(&mut bytes[range.start().to_usize()..range.end().to_usize()], statement);
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Blank an IPython escape command in place, byte for byte, each of its lines on its own: a
/// statement as a comment that keeps its text, a value as `...` padded with spaces (`0` where it
/// is shorter than that).
fn blank(command: &mut [u8], statement: bool) {
    for (i, line) in command.split_inclusive_mut(|b| *b == b'\n').enumerate() {
        let end = line.len() - usize::from(line.last() == Some(&b'\n'));
        if statement && end > 0 && line[0].is_ascii() {
            line[0] = b'#';
            continue;
        }
        line[..end].fill(b' ');
        let value: &[u8] = match (statement, i, end) {
            (false, 0, 3..) => b"...",
            (false, 0, 1..) => b"0",
            _ => b"",
        };
        line[..value.len()].copy_from_slice(value);
    }
}

/// The IPython escape commands of a notebook's cells, each by its range and whether it is a
/// statement of its own.
#[derive(Default)]
struct Escapes {
    found: Vec<(TextRange, bool)>,
}

impl<'a> Visitor<'a> for Escapes {
    fn visit_stmt(&mut self, stmt: &'a ast::Stmt) {
        match stmt {
            ast::Stmt::IpyEscapeCommand(c) => self.found.push((c.range(), true)),
            _ => visitor::walk_stmt(self, stmt),
        }
    }

    fn visit_expr(&mut self, expr: &'a ast::Expr) {
        match expr {
            ast::Expr::IpyEscapeCommand(c) => self.found.push((c.range(), false)),
            _ => visitor::walk_expr(self, expr),
        }
    }
}

/// `if __name__ == "__main__":` at a module's top level: where the module runs as a program, its
/// entry, not a choice its program makes.
pub(super) fn is_main_guard(s: TsNode, src: &[u8]) -> bool {
    if s.kind() != "if_statement" || s.parent().map_or(true, |p| p.kind() != "module") {
        return false;
    }
    let condition: String = s
        .child_by_field_name("condition")
        .map(|c| text(c, src))
        .unwrap_or("")
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .replace('\'', "\"");
    condition == "__name__==\"__main__\"" || condition == "\"__main__\"==__name__"
}

/// The project's own packages by the names they are imported by, the one the graph is named
/// after first: a dotted name in one of them is the project's own, any other a library's.
pub(super) struct Own(pub(super) Vec<String>);

impl Own {
    /// The place among the project's packages of the one the dotted name `id` is, or is in.
    pub(super) fn rank(&self, id: &str) -> Option<usize> {
        self.0.iter().position(|p| id.strip_prefix(p.as_str()).map_or(false, |rest| rest.is_empty() || rest.starts_with('.')))
    }

    /// The package of the project's own the dotted name `id` is, or is in.
    pub(super) fn package(&self, id: &str) -> Option<&str> {
        self.rank(id).map(|i| self.0[i].as_str())
    }

    /// Whether the dotted name `id` is the project's own.
    pub(super) fn holds(&self, id: &str) -> bool {
        self.rank(id).is_some()
    }
}

/// Dotted name in module scope -> one of the code's node ids, a name past the code, or None.
pub(super) fn resolve(
    m: &Module,
    parts: &[String],
    modules: &IndexMap<String, Module>,
    nodes: &IndexMap<String, Node>,
    own: &Own,
) -> Option<Resolved> {
    // a shaped array is its dtype's type with the shape kept: `ext:jaxtyping.Float[Tensor, batch dim]`
    // (knowledge::SHAPED_ARRAYS)
    if let [one] = parts {
        if let Some((head, shape)) = one.split_once('[') {
            let base = resolve(m, &head.split('.').map(str::to_string).collect::<Vec<_>>(), modules, nodes, own)?.id();
            return Some(Resolved::of_id(if knowledge::shaped(&base).is_some() { format!("{base}[{shape}") } else { base }));
        }
    }
    let head = parts.first()?;
    let mut target = m.symbols.get(head).or_else(|| m.bindings.get(head))?.clone();
    for p in &parts[1..] {
        target = format!("{target}.{p}");
    }
    if own.holds(&target) {
        return defined(&target, modules, nodes).map(Resolved::Code);
    }
    if nodes.contains_key(&target) {
        return Some(Resolved::Code(target));
    }
    Some(Resolved::Beyond(target))
}

/// The definition node of a symbol, found again by its line and name.
pub(super) fn def_node<'t>(m: &'t Module, n: &Node) -> Option<TsNode<'t>> {
    if n.kind == NodeKind::Module {
        return Some(m.tree.root_node());
    }
    let (start, _) = n.span?;
    let defs = m.defs.get_or_init(|| {
        walk(m.tree.root_node(), &["function_definition", "class_definition"])
            .into_iter()
            .filter_map(|d| {
                Some((
                    (d.start_position().row as u32 + 1, text(d.child_by_field_name("name")?, &m.src).to_string()),
                    (d.start_byte(), d.end_byte()),
                ))
            })
            .collect()
    });
    let &(a, b) = defs.get(&(start, n.name.clone()))?;
    let mut d = m.tree.root_node().descendant_for_byte_range(a, b)?;
    while !(matches!(d.kind(), "function_definition" | "class_definition") && d.start_byte() == a && d.end_byte() == b) {
        d = d.parent()?;
    }
    Some(d)
}

/// The names an annotation's outermost classes are written with, through the forms that say which
/// values it may be or describe one (`Optional[X]`, `X | None`, `Annotated[X, ..]`): `list[int]`
/// is a `list`.
pub(super) fn outer_type_names<'t>(t: TsNode<'t>, src: &[u8]) -> Vec<TsNode<'t>> {
    let (head, args) = match t.kind() {
        "identifier" | "attribute" | "member_type" => return vec![t],
        "type" | "parenthesized_expression" | "union_type" => {
            return named_children(t).into_iter().flat_map(|c| outer_type_names(c, src)).collect()
        }
        "binary_operator" => {
            return [t.child_by_field_name("left"), t.child_by_field_name("right")]
                .into_iter()
                .flatten()
                .flat_map(|c| outer_type_names(c, src))
                .collect()
        }
        "subscript" => (t.child_by_field_name("value"), field_children(t, "subscript")),
        "generic_type" => (
            named_children(t).first().copied(),
            named_children(t).into_iter().filter(|k| k.kind() == "type_parameter").flat_map(named_children).collect(),
        ),
        _ => return vec![],
    };
    let Some(head) = head else { return vec![] };
    let last = dotted(Some(head), src).last().cloned().unwrap_or_default();
    if knowledge::PYTHON_ALTERNATIVES.contains(&last.as_str()) {
        args.into_iter().flat_map(|a| outer_type_names(a, src)).collect()
    } else if knowledge::PYTHON_DESCRIBES.contains(&last.as_str()) {
        args.into_iter().take(1).flat_map(|a| outer_type_names(a, src)).collect()
    } else {
        vec![head]
    }
}

/// The library an external name belongs to: `ext:requests.Session` -> `requests`.
pub(super) fn lib_of(resolved: &str) -> Option<String> {
    ext::path(resolved).map(|r| r.split(['.', '[']).next().unwrap_or(r).to_string())
}

/// The package a library's module belongs to: `neo4j._sync.work.session` -> `neo4j`. A module
/// named with one underscore is the implementation behind the module without it: `_csv` is
/// `csv`, and `_duckdb` (from its stubs `_duckdb-stubs`) is `duckdb`.
pub(super) fn package_of(module: &str) -> String {
    let head = module.split('.').next().unwrap_or(module);
    head.strip_prefix('_').filter(|h| !h.is_empty() && !h.starts_with('_')).unwrap_or(head).to_string()
}

/// The dotted path a call is written with; through a call it is made on, `X(..).method` is
/// `X.method`.
pub(super) fn call_path(f: TsNode, src: &[u8]) -> Vec<String> {
    let parts = dotted(Some(f), src);
    if !parts.is_empty() || f.kind() != "attribute" {
        return parts;
    }
    let (Some(obj), Some(attr)) = (f.child_by_field_name("object"), f.child_by_field_name("attribute")) else { return vec![] };
    let mut inner = match obj.kind() {
        "call" => obj.child_by_field_name("function").map(|g| call_path(g, src)).unwrap_or_default(),
        _ => vec![],
    };
    if !inner.is_empty() {
        inner.push(text(attr, src).to_string());
    }
    inner
}

/// The names a call is written with: its dotted path, or for a method on a value written out
/// (`(root / name).write_text`) its name alone; none for a call of what an expression gives
/// (`getattr(self, kind)(..)`, `handlers[kind](..)`).
pub(super) fn named_path(f: TsNode, src: &[u8]) -> Vec<String> {
    match call_path(f, src) {
        p if p.is_empty() => f.child_by_field_name("attribute").map(|a| vec![text(a, src).to_string()]).unwrap_or_default(),
        p => p,
    }
}

/// What a call calls, as the code writes it, on one line.
pub(super) fn written(f: TsNode, src: &[u8]) -> String {
    text(f, src).split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A name followed through the packages that re-export it to where it is defined:
/// `pkg.Cache` -> `pkg.data.Cache` -> `pkg.data.cache.Cache`.
pub(super) fn defined(target: &str, modules: &IndexMap<String, Module>, nodes: &IndexMap<String, Node>) -> Option<String> {
    // a name re-exported through more packages than this goes round in a cycle
    const REEXPORTS: usize = 8;
    let mut t = target.to_string();
    for _ in 0..REEXPORTS {
        if nodes.contains_key(&t) {
            return Some(t);
        }
        let (modid, name) = t.rsplit_once('.')?;
        t = modules.get(modid)?.bindings.get(name)?.clone();
    }
    None
}

/// The assignments at the top level of a module.
pub(super) fn top_assignments<'t>(m: &'t Module) -> Vec<TsNode<'t>> {
    named_children(m.tree.root_node())
        .into_iter()
        .filter(|s| s.kind() == "expression_statement")
        .flat_map(named_children)
        .filter(|a| a.kind() == "assignment")
        .collect()
}
