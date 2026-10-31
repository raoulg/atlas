//! Where a file call's file is: its path evaluated from the code, through the definitions ty
//! finds, and through the values a function's parameters are handed by its callers. An entry's
//! parameters hold their declared defaults (`@click.option(.., default=..)`).

use std::collections::HashMap;
use std::path::Path;

use indexmap::IndexMap;
use tree_sitter::Node as TsNode;

use super::module::{call_path, dotted, Module, WRAPPERS};
use crate::pytypes::{FileArg, Param, Target, Types};
use crate::tree::{beneath, named_children, path_str, text, unquote, walk};

use crate::knowledge::{self, PathAttribute, PathFn, PathMethod};
use crate::paths::{bound as named_by, cross, environment, hole, lit, or_else, parent, substitute, union};
pub(super) use crate::paths::{Built, Seg};

const DEPTH: u32 = 40;

/// Evaluates paths in the modules of one project.
pub(super) struct Eval<'a> {
    by_file: HashMap<String, &'a Module>,
    types: &'a Types,
    root: &'a Path,
}

/// Where an expression is evaluated: its module, and the function it is in with that function's
/// parameters.
#[derive(Clone, Copy)]
pub(super) struct At<'a, 't> {
    pub m: &'a Module,
    pub scope: Option<TsNode<'t>>,
    pub params: &'a [String],
}

impl<'a, 't> At<'a, 't> {
    /// Where a definition written at `offset` in `m` is evaluated: in this scope when it is written
    /// inside it, else at the top of `m`.
    fn of_definition(self, m: &'a Module, offset: usize) -> At<'a, 't> {
        let inside = std::ptr::eq(m, self.m) && self.scope.map_or(false, |s| s.start_byte() <= offset && offset < s.end_byte());
        if inside {
            At { m, ..self }
        } else {
            At { m, scope: None, params: &[] }
        }
    }
}

impl<'a> Eval<'a> {
    pub fn new(modules: &'a IndexMap<String, Module>, types: &'a Types, root: &'a Path) -> Self {
        Eval { by_file: modules.values().map(|m| (path_str(&m.path), m)).collect(), types, root }
    }

    /// The module's file, by its path in the project.
    fn file(&self, m: &Module) -> String {
        m.path.strip_prefix(self.root).map(path_str).unwrap_or_else(|_| path_str(&m.path))
    }

    fn def(&self, m: &Module, e: TsNode) -> Option<Target> {
        self.types.of(&path_str(&m.path))?.def((e.start_byte(), e.end_byte())).cloned()
    }

    fn signature<'s>(&'s self, m: &Module, f: TsNode) -> Option<&'s [Param]> {
        self.types.of(&path_str(&m.path))?.signature((f.start_byte(), f.end_byte()))
    }

    /// Whether calling `f` reaches a method of `pathlib`.
    fn of_pathlib(&self, m: &Module, f: TsNode) -> bool {
        self.types.of(&path_str(&m.path)).map_or(false, |x| {
            x.callees((f.start_byte(), f.end_byte()))
                .iter()
                .any(|c| matches!(&c.target, Target::Lib(m) if knowledge::PATH_LIBRARIES.contains(&m.as_str())))
        })
    }

    /// Whether ty types the value `e` as the class `class` of the library `lib`.
    fn of_class(&self, m: &Module, e: TsNode, lib: &str, class: &str) -> bool {
        self.types.of(&path_str(&m.path)).and_then(|x| x.value((e.start_byte(), e.end_byte()))).map_or(false, |v| {
            v.names.iter().any(|n| n == class)
                && v.classes.iter().any(|c| matches!(c, Target::Lib(x) if x == lib || x.starts_with(&format!("{lib}."))))
        })
    }

    /// Whether calling `f` reaches a function of the library `lib`, in its module or below it.
    fn of_library(&self, m: &Module, f: TsNode, lib: &str) -> bool {
        self.types.of(&path_str(&m.path)).map_or(false, |x| {
            x.callees((f.start_byte(), f.end_byte()))
                .iter()
                .any(|c| matches!(&c.target, Target::Lib(m) if m == lib || m.starts_with(&format!("{lib}."))))
        })
    }

    /// What the library's path function `f`, named `name`, makes of what it is handed, where ty
    /// finds the call goes into that library (knowledge::PYTHON_PATH_FUNCTIONS).
    fn path_function(&self, m: &Module, f: TsNode, name: &str) -> Option<PathFn> {
        let class = knowledge::PYTHON_PATH_CLASSES.iter().any(|(lib, c)| *c == name && self.of_library(m, f, lib));
        let function = || knowledge::PYTHON_PATH_FUNCTIONS.iter().find(|(lib, n, _)| *n == name && self.of_library(m, f, lib));
        if class {
            return Some(PathFn::Join);
        }
        function().map(|(_, _, k)| *k)
    }

    /// Whether calling `f` makes a path that starts where its first argument does: where ty finds
    /// the call goes into the library of the path class or the path function `f` names
    /// (knowledge::PYTHON_PATH_CLASSES, knowledge::PYTHON_PATH_FUNCTIONS), and what that makes
    /// starts at its first argument (`PathFn::starts_at_first`), as `Path("data")` and
    /// `os.path.join("data", name)` do. A class or a function of the project's own of the same
    /// name is the project's.
    pub fn makes_path(&self, m: &Module, f: TsNode) -> bool {
        self.path_function(m, f, &called(f, &m.src)).is_some_and(PathFn::starts_at_first)
    }

    /// What the method `f` of a library's path, named `name`, makes of the path it is called on,
    /// where ty finds the call goes into that library (knowledge::PYTHON_PATH_METHODS).
    fn path_method(&self, m: &Module, f: TsNode, name: &str) -> Option<PathMethod> {
        if f.kind() != "attribute" {
            return None;
        }
        knowledge::PYTHON_PATH_METHODS.iter().find(|(lib, n, _)| *n == name && self.of_library(m, f, lib)).map(|(_, _, k)| *k)
    }

    /// What the attribute `e`, named `name`, is of the path it is on, where ty finds the attribute is
    /// defined in the path's library (knowledge::PYTHON_PATH_ATTRIBUTES).
    fn path_attribute(&self, m: &Module, e: TsNode, name: &str) -> Option<PathAttribute> {
        let Some(Target::Lib(module)) = self.def(m, e) else { return None };
        knowledge::PYTHON_PATH_ATTRIBUTES
            .iter()
            .find(|(lib, n, _)| *n == name && (module == *lib || module.starts_with(&format!("{lib}."))))
            .map(|(_, _, k)| *k)
    }

    /// Whether calling `f`, named `name`, reaches a library's function that gives the directory a
    /// package ships its files in (knowledge::PACKAGE_FILES): in the library's module or below it.
    fn gives_package_files(&self, m: &Module, f: TsNode, name: &str) -> bool {
        self.types.of(&path_str(&m.path)).map_or(false, |x| {
            x.callees((f.start_byte(), f.end_byte())).iter().any(|c| match &c.target {
                Target::Lib(module) => knowledge::PACKAGE_FILES
                    .iter()
                    .any(|(lib, fun)| *fun == name && (module == lib || module.starts_with(&format!("{lib}.")))),
                Target::Here { .. } => false,
            })
        })
    }

    /// The directory the package named `name` lies in, by its path in the project: the package's
    /// own directory, or the one a single module is in.
    fn package_dir(&self, name: &str) -> Option<String> {
        let m = self.by_file.values().find(|m| m.id == name)?;
        let file = self.file(m);
        Some(file.rsplit_once('/').map_or(".".to_string(), |(dir, _)| dir.to_string()))
    }

    fn callee_def(&self, m: &Module, f: TsNode) -> Option<(String, usize)> {
        let facts = self.types.of(&path_str(&m.path))?;
        facts.callees((f.start_byte(), f.end_byte())).iter().find_map(|c| match &c.target {
            Target::Here { file, at } => Some((file.clone(), *at)),
            Target::Lib(_) => None,
        })
    }

    /// What a function of the package defined at `offset` in `file` gives back at call `c`: each
    /// `return` evaluated with the function's parameters bound to what `c` hands them, by place
    /// or by keyword, and to their defaults where it hands nothing.
    fn given(&self, at: At, file: &str, offset: usize, c: TsNode, depth: u32) -> Option<Vec<Built>> {
        let m = *self.by_file.get(file)?;
        let def = m.tree.root_node().descendant_for_byte_range(offset, offset)?.parent().filter(|d| d.kind() == "function_definition")?;
        let declared = parameters(def, &m.src, in_class(def));
        let names: Vec<String> = declared.iter().map(|(n, _)| n.clone()).collect();
        let inner = At { m, scope: Some(def), params: &names };
        let returns = walk(def.child_by_field_name("body")?, &["return_statement"])
            .into_iter()
            .filter(|r| enclosing_def(*r).map(|d| d.id()) == Some(def.id()));
        let gives =
            returns.filter_map(|r| named_children(r).into_iter().next()).fold(vec![], |acc, x| union(acc, self.of(inner, x, depth + 1)));
        if gives.is_empty() {
            return None;
        }
        let mut handed: HashMap<String, Vec<Built>> = HashMap::new();
        let mut i = 0;
        for a in c.child_by_field_name("arguments").map(named_children).unwrap_or_default().into_iter().filter(|a| a.kind() != "comment") {
            match a.kind() {
                "keyword_argument" => {
                    if let (Some(k), Some(v)) = (a.child_by_field_name("name"), a.child_by_field_name("value")) {
                        handed.insert(text(k, &at.m.src).to_string(), self.of(at, v, depth + 1));
                    }
                }
                "list_splat" | "dictionary_splat" => break,
                _ => {
                    if let Some(p) = names.get(i) {
                        handed.insert(p.clone(), self.of(at, a, depth + 1));
                    }
                    i += 1;
                }
            }
        }
        let value = |p: &str| match handed.get(p) {
            Some(v) => v.clone(),
            None => declared
                .iter()
                .find(|(n, _)| n == p)
                .and_then(|(_, d)| *d)
                .map(|d| self.of(At { m, scope: None, params: &[] }, d, depth + 1))
                .unwrap_or_else(|| vec![hole(p)]),
        };
        Some(gives.iter().flat_map(|b| substitute(b, value)).collect())
    }

    /// The values `e` can have.
    pub fn of(&self, at: At, e: TsNode, depth: u32) -> Vec<Built> {
        if depth > DEPTH {
            return vec![hole("…")];
        }
        let e = beneath(e, WRAPPERS);
        let src = &at.m.src;
        let named = || hole(dotted(Some(e), src).last().map(String::as_str).unwrap_or("value"));
        match e.kind() {
            "string" => self.string(at, e, depth),
            "concatenated_string" => named_children(e).into_iter().fold(vec![vec![]], |acc, s| cross(&acc, &self.string(at, s, depth), "")),
            "conditional_expression" => {
                let k = named_children(e);
                union(
                    k.first().map(|x| self.of(at, *x, depth + 1)).unwrap_or_default(),
                    k.get(2).map(|x| self.of(at, *x, depth + 1)).unwrap_or_default(),
                )
            }
            // `x or d`: `x`, or `d` where `x` says nothing
            "boolean_operator" if e.child_by_field_name("operator").map(|o| text(o, src)) == Some("or") => {
                let side = |f: &str| e.child_by_field_name(f).map(|x| self.of(at, x, depth + 1)).unwrap_or_default();
                or_else(side("left"), side("right"))
            }
            "boolean_operator" => named_children(e).into_iter().fold(vec![], |acc, x| union(acc, self.of(at, x, depth + 1))),
            "binary_operator" => {
                let (Some(l), Some(r)) = (e.child_by_field_name("left"), e.child_by_field_name("right")) else { return vec![named()] };
                let sep = match e.child_by_field_name("operator").map(|o| text(o, src)) {
                    Some("/") => "/",
                    Some("+") => "",
                    _ => return vec![named()],
                };
                cross(&self.of(at, l, depth + 1), &self.of(at, r, depth + 1), sep)
            }
            "identifier" => self.name(at, e, depth),
            "attribute" => {
                let (Some(obj), Some(attr)) = (e.child_by_field_name("object"), e.child_by_field_name("attribute")) else {
                    return vec![named()];
                };
                let name = text(attr, src);
                if self.path_attribute(at.m, e, name) == Some(PathAttribute::Parent) {
                    return self.of(at, obj, depth + 1).iter().map(parent).collect();
                }
                match name {
                    // the path a listing's element is at: `entry.path` of `os.scandir(d)`'s `entry`
                    a if knowledge::ELEMENT_PATHS.iter().any(|(lib, class, name)| *name == a && self.of_class(at.m, obj, lib, class)) => {
                        self.of(at, obj, depth + 1)
                    }
                    a => self.parsed(at, obj, a, depth).or_else(|| self.defined(at, e, depth)).unwrap_or_else(|| vec![named()]),
                }
            }
            "subscript" => {
                // `os.environ["DATA_DIR"]`: what the environment holds for the variable, where ty types
                // the mapping as the environment's
                let environ = e
                    .child_by_field_name("value")
                    .map_or(false, |v| knowledge::PYTHON_ENVIRONMENT.iter().any(|(lib, class, _)| self.of_class(at.m, v, lib, class)));
                if let Some(var) = e.child_by_field_name("subscript").filter(|s| environ && s.kind() == "string") {
                    return vec![environment(&unquote(text(var, src)))];
                }
                // `x.parents[n]`: the directory n + 1 levels up, where the attribute is the path's
                let v = e.child_by_field_name("value").filter(|v| v.kind() == "attribute");
                let parents = v.filter(|v| {
                    v.child_by_field_name("attribute").and_then(|a| self.path_attribute(at.m, *v, text(a, src)))
                        == Some(PathAttribute::Ancestors)
                });
                let n = e.child_by_field_name("subscript").and_then(|s| text(s, src).parse::<usize>().ok());
                match (parents.and_then(|v| v.child_by_field_name("object")), n) {
                    (Some(obj), Some(n)) => {
                        self.of(at, obj, depth + 1).iter().map(|b| (0..=n).fold(b.clone(), |b, _| parent(&b))).collect()
                    }
                    // `d[k]` for each `k` of `d` is one of the values `d` holds; a key looked up at
                    // run time is not decided by the code
                    _ => match e.child_by_field_name("value").filter(|v| self.ranges_over(e, *v, src)) {
                        Some(v) => {
                            let held = self.of(at, v, depth + 1);
                            if held.iter().all(|b| matches!(b.as_slice(), [Seg::Hole(_)])) {
                                vec![hole(dotted(Some(v), src).last().map(String::as_str).unwrap_or("value"))]
                            } else {
                                held
                            }
                        }
                        None => self.looked_up(at, e, depth).unwrap_or_else(|| vec![named()]),
                    },
                }
            }
            "call" => self.call(at, e, depth),
            // a socket's address, a host and a port: `("127.0.0.1", 7878)` is `127.0.0.1:7878`
            "tuple" if self.port(at, e).is_some() => {
                let host = named_children(e).into_iter().next().map(|h| self.of(at, h, depth + 1)).unwrap_or_default();
                cross(&host, &[lit(&self.port(at, e).unwrap_or_default())], ":")
            }
            // a collection of paths is the paths it can hold; iterating it yields one of them
            "list" | "tuple" | "set" => named_children(e).into_iter().fold(vec![], |acc, x| union(acc, self.of(at, x, depth + 1))),
            // a mapping holds its values: `{p.name: p for p in dirs}` holds `dirs`
            "dictionary" => named_children(e).into_iter().fold(vec![], |acc, x| union(acc, self.of(at, x, depth + 1))),
            "dictionary_comprehension" => e.child_by_field_name("body").map(|b| self.of(at, b, depth + 1)).unwrap_or_default(),
            "pair" => e.child_by_field_name("value").map(|v| self.of(at, v, depth + 1)).unwrap_or_default(),
            "list_splat" => named_children(e).first().map(|x| self.of(at, *x, depth + 1)).unwrap_or_default(),
            "list_comprehension" | "set_comprehension" | "generator_expression" => {
                e.child_by_field_name("body").map(|b| self.of(at, b, depth + 1)).unwrap_or_default()
            }
            "none" => vec![],
            _ => vec![hole("value")],
        }
    }

    /// `d[k]`, `d` a collection the code writes out where it stands or where `d` is defined (a
    /// mapping, a list, a tuple) or one that holds another's elements (`sorted(ps)`): the value `k`
    /// picks where the code decides `k` and `d` is written out, else one of the values `d` holds, as
    /// `for k in d: d[k]` is.
    fn looked_up(&self, at: At, e: TsNode, depth: u32) -> Option<Vec<Built>> {
        let (v, k) = (e.child_by_field_name("value")?, e.child_by_field_name("subscript")?);
        let key = match self.of(at, k, depth + 1).as_slice() {
            [b] if crate::paths::known(b) => Some(crate::paths::render(b)),
            _ => None,
        };
        let index = text(k, &at.m.src).parse::<i64>().ok();
        let values = self.collection(at, v, key.as_deref(), index, depth)?;
        (!values.iter().all(|b| matches!(b.as_slice(), [Seg::Hole(_)]))).then_some(values)
    }

    /// The values of the collection `v` that the key or index picks, where the code writes `v` out
    /// or makes it of another's elements, here or in the assignment that defines `v`; all of them
    /// where neither picks one.
    fn collection(&self, at: At, v: TsNode, key: Option<&str>, index: Option<i64>, depth: u32) -> Option<Vec<Built>> {
        let held = |x: TsNode, src: &[u8]| match x.kind() {
            "dictionary" | "list" | "tuple" | "list_comprehension" | "dictionary_comprehension" => true,
            "call" => x
                .child_by_field_name("function")
                .map_or(false, |f| call_path(f, src).last().is_some_and(|l| knowledge::SAME_ELEMENTS.contains(&l.as_str()))),
            _ => false,
        };
        if held(v, &at.m.src) {
            return Some(self.picked(at, v, key, index, depth));
        }
        if !matches!(v.kind(), "identifier" | "attribute") {
            return None;
        }
        let Target::Here { file, at: offset } = self.def(at.m, v)? else { return None };
        let m = *self.by_file.get(&file)?;
        let mut n = m.tree.root_node().descendant_for_byte_range(offset, offset)?;
        if n.kind() != "identifier" {
            n = n.parent().filter(|p| p.kind() == "identifier").unwrap_or(n);
        }
        let a = n.parent().filter(|a| a.kind() == "assignment" && a.child_by_field_name("left").map(|l| l.id()) == Some(n.id()))?;
        let right = a.child_by_field_name("right").filter(|r| held(*r, &m.src))?;
        Some(self.picked(at.of_definition(m, offset), right, key, index, depth))
    }

    /// What a key or an index picks of a collection the code writes out (a mapping by a key its
    /// pairs write, a list or a tuple by its place), else every value it holds.
    fn picked(&self, at: At, held: TsNode, key: Option<&str>, index: Option<i64>, depth: u32) -> Vec<Built> {
        let picked = match held.kind() {
            "dictionary" => key.and_then(|key| {
                named_children(held).into_iter().filter(|p| p.kind() == "pair").find(|p| {
                    p.child_by_field_name("key").map_or(false, |x| match self.of(at, x, depth + 1).as_slice() {
                        [b] => crate::paths::known(b) && crate::paths::render(b) == key,
                        _ => false,
                    })
                })
            }),
            "list" | "tuple" => index.and_then(|i| {
                let items: Vec<TsNode> = named_children(held).into_iter().filter(|x| x.kind() != "comment").collect();
                let i = if i < 0 { items.len() as i64 + i } else { i };
                usize::try_from(i).ok().and_then(|i| items.get(i).copied())
            }),
            _ => None,
        };
        self.of(at, picked.unwrap_or(held), depth + 1)
    }

    /// The elements a call of the language's own runs the function `f` it is handed on, where its
    /// parameter `slot` is the one an element fills (knowledge::PYTHON_EACH): what the call goes over
    /// holds.
    fn on_each(&self, at: At, f: TsNode, slot: usize, depth: u32) -> Option<Vec<Built>> {
        let src = &at.m.src;
        // where the call is handed `f`: by a keyword, or by its place
        let (args, keyword, place) = match f.parent()? {
            k if k.kind() == "keyword_argument" => (k.parent()?, k.child_by_field_name("name").map(|n| text(n, src)), None),
            a if a.kind() == "argument_list" => (a, None, positional(a).iter().position(|x| x.id() == f.id())),
            _ => return None,
        };
        let call = args.parent().filter(|c| c.kind() == "call")?;
        let callee = call.child_by_field_name("function")?;
        let name =
            call_path(callee, src).last().cloned().or_else(|| callee.child_by_field_name("attribute").map(|a| text(a, src).to_string()))?;
        let each = knowledge::PYTHON_EACH.iter().find(|e| {
            e.name == name
                && e.element as usize == slot
                && match e.function {
                    knowledge::Handed::Key(w) => keyword == Some(w),
                    knowledge::Handed::At(i) => place == Some(i),
                }
        })?;
        let over = match each.over {
            Some(i) => *positional(args).get(i)?,
            None => callee.child_by_field_name("object")?,
        };
        Some(self.of(at, over, depth + 1))
    }

    /// What the parameter `param`, at `slot`, of a function defined in a body is handed where the
    /// body calls it by the name `kept` it keeps the function by: a function written as a value
    /// (`read = lambda name: open(root / name)`) or defined in the body (`def read(name: str)`),
    /// called with `read("pyproject.toml")`. Each call's argument, as a function's parameter is what
    /// its callers hand it; a call is the name's where ty finds the name defined at `kept`.
    fn called_with(&self, at: At, kept: TsNode, slot: usize, param: &str, depth: u32) -> Option<Vec<Built>> {
        let src = &at.m.src;
        // the body the function is defined in: around the definition `kept` names, or the assignment
        let owner = kept.parent().filter(|d| d.kind() == "function_definition").unwrap_or(kept);
        let scope = enclosing_def(owner).unwrap_or_else(|| at.m.tree.root_node());
        let file = path_str(&at.m.path);
        let mut out: Vec<Built> = vec![];
        for call in walk(scope, &["call"]) {
            let Some(callee) = call.child_by_field_name("function").filter(|c| c.kind() == "identifier") else { continue };
            if !matches!(self.def(at.m, callee), Some(Target::Here { file: ref d, at }) if *d == file && at == kept.start_byte()) {
                continue;
            }
            let Some(args) = call.child_by_field_name("arguments") else { continue };
            let handed = positional(args).get(slot).copied().or_else(|| keyword(args, src, param));
            if let Some(a) = handed {
                out = union(out, self.of(at, a, depth + 1));
            }
        }
        (!out.is_empty()).then_some(out)
    }

    /// Whether the key of the subscript `e` of `value` is bound by a loop over `value`'s keys:
    /// `for k in d`, `for k in sorted(d)`, `for k in d.keys()`.
    fn ranges_over(&self, e: TsNode, value: TsNode, src: &[u8]) -> bool {
        let Some(key) = e.child_by_field_name("subscript").filter(|k| k.kind() == "identifier").map(|k| text(k, src)) else {
            return false;
        };
        let mut over = |mut it: TsNode| loop {
            match it.kind() {
                "call" => {
                    let f = it.child_by_field_name("function");
                    let named = f.map(|f| text(f, src).rsplit('.').next().unwrap_or("").to_string()).unwrap_or_default();
                    let inner = if f.map_or(false, |f| f.kind() == "attribute") && named == "keys" {
                        f.and_then(|f| f.child_by_field_name("object"))
                    } else if knowledge::SAME_ELEMENTS.contains(&named.as_str()) {
                        it.child_by_field_name("arguments").and_then(|a| named_children(a).first().copied())
                    } else {
                        None
                    };
                    match inner {
                        Some(i) => it = i,
                        None => return text(it, src) == text(value, src),
                    }
                }
                _ => return text(it, src) == text(value, src),
            }
        };
        let mut n = e.parent();
        while let Some(a) = n {
            if matches!(a.kind(), "for_in_clause" | "for_statement")
                && a.child_by_field_name("left").map_or(false, |l| text(l, src) == key)
                && a.child_by_field_name("right").map_or(false, &mut over)
            {
                return true;
            }
            if matches!(a.kind(), "list_comprehension" | "set_comprehension" | "dictionary_comprehension" | "generator_expression") {
                if named_children(a).into_iter().any(|c| {
                    c.kind() == "for_in_clause"
                        && c.child_by_field_name("left").map_or(false, |l| text(l, src) == key)
                        && c.child_by_field_name("right").map_or(false, &mut over)
                }) {
                    return true;
                }
            }
            n = a.parent();
        }
        false
    }

    /// A string literal, an f-string's holes evaluated: one value for each the holes can hold.
    fn string(&self, at: At, s: TsNode, depth: u32) -> Vec<Built> {
        let parts: Vec<TsNode> = named_children(s);
        if !parts.iter().any(|p| p.kind() == "interpolation") {
            return vec![lit(&unquote(text(s, &at.m.src)))];
        }
        let mut out: Vec<Built> = vec![vec![]];
        for p in parts {
            let piece = match p.kind() {
                "string_content" | "escape_sequence" => vec![lit(text(p, &at.m.src))],
                "interpolation" => named_children(p).first().map(|x| self.of(at, *x, depth + 1)).unwrap_or_else(|| vec![hole("value")]),
                _ => continue,
            };
            out = cross(&out, &piece, "");
        }
        out
    }

    fn name(&self, at: At, e: TsNode, depth: u32) -> Vec<Built> {
        let name = text(e, &at.m.src);
        if name == "__file__" {
            return vec![vec![Seg::Package(self.file(at.m))]];
        }
        let named = self.local(at.m, e).unwrap_or_else(|| name.to_string());
        self.defined(at, e, depth).map(|v| named_by(&named, v)).unwrap_or_else(|| vec![hole(name)])
    }

    /// The name a value bound in a function goes by beyond it: the binding's with the function's
    /// (`main.folder`, `Store.open.root`), as a parameter's is (`analyze.dir`), so that two functions'
    /// `file`s are two values. None for a module's name.
    fn local(&self, m: &Module, e: TsNode) -> Option<String> {
        let Target::Here { file, at: offset } = self.def(m, e)? else { return None };
        let home = *self.by_file.get(&file)?;
        let n = home.tree.root_node().descendant_for_byte_range(offset, offset)?;
        let f = enclosing_def(n)?;
        let src = &home.src;
        let mut owner = text(f.child_by_field_name("name")?, src).to_string();
        let mut p = f.parent();
        while let Some(x) = p.filter(|x| x.kind() != "class_definition" && x.kind() != "function_definition") {
            p = x.parent();
        }
        if let Some(class) = p.filter(|x| x.kind() == "class_definition").and_then(|c| c.child_by_field_name("name")) {
            owner = format!("{}.{owner}", text(class, src));
        }
        Some(format!("{owner}.{}", text(n, src)))
    }

    /// An attribute of the object a command-line parser gives (`args.out`): the option, holding the
    /// default of the argument that declares it, what the program runs with when its command line
    /// leaves it out. The attribute is named by the argument's `dest`, else its longest
    /// `--option` (`-` as `_`), else its positional name.
    fn parsed(&self, at: At, obj: TsNode, name: &str, depth: u32) -> Option<Vec<Built>> {
        let Target::Here { file, at: offset } = self.def(at.m, obj)? else { return None };
        let m = *self.by_file.get(&file)?;
        let facts = self.types.of(&file)?;
        let assigned = m.tree.root_node().descendant_for_byte_range(offset, offset)?.parent().filter(|p| p.kind() == "assignment")?;
        let right = assigned.child_by_field_name("right")?;
        let value = facts.value((right.start_byte(), right.end_byte()))?;
        let of_lib = |t: &Target, lib: &str| matches!(t, Target::Lib(l) if l == lib || l.starts_with(&format!("{lib}.")));
        let parser = knowledge::ARG_PARSERS
            .iter()
            .find(|p| value.names.iter().any(|n| n == p.namespace) && value.classes.iter().any(|c| of_lib(c, p.package)))?;
        for call in walk(m.tree.root_node(), &["call"]) {
            let Some(f) = call.child_by_field_name("function").filter(|f| f.kind() == "attribute") else { continue };
            if f.child_by_field_name("attribute").map(|a| text(a, &m.src)) != Some(parser.declares)
                || !facts.callees((f.start_byte(), f.end_byte())).iter().any(|c| of_lib(&c.target, parser.package))
            {
                continue;
            }
            let (mut flags, mut dest, mut default) = (vec![], None, None);
            for a in call.child_by_field_name("arguments").map(named_children).unwrap_or_default() {
                match a.kind() {
                    "string" => flags.push(unquote(text(a, &m.src))),
                    "keyword_argument" => match a.child_by_field_name("name").map(|k| text(k, &m.src)) {
                        Some(k) if k == parser.dest => dest = a.child_by_field_name("value").map(|v| unquote(text(v, &m.src))),
                        Some(k) if k == parser.default => default = a.child_by_field_name("value"),
                        _ => {}
                    },
                    _ => {}
                }
            }
            let option = |f: &String| f.trim_start_matches('-').replace('-', "_");
            let declared = dest
                .or_else(|| flags.iter().filter(|f| f.starts_with("--")).max_by_key(|f| f.len()).map(option))
                .or_else(|| flags.iter().find(|f| !f.starts_with('-')).cloned())
                .or_else(|| flags.first().map(option));
            if declared.as_deref() == Some(name) {
                return default
                    .map(|d| crate::paths::option(name, self.of(At { m, scope: enclosing_def(call), params: &[] }, d, depth + 1)));
            }
        }
        None
    }

    /// The values of what a name or attribute is defined as.
    fn defined(&self, at: At, e: TsNode, depth: u32) -> Option<Vec<Built>> {
        let Target::Here { file, at: offset } = self.def(at.m, e)? else { return None };
        self.follow(at, &file, offset, depth)
    }

    /// What the definition whose name is written at `offset` in `file` holds: an assignment's
    /// value, a property's or function's `return`, a parameter of the function being evaluated.
    fn follow(&self, at: At, file: &str, offset: usize, depth: u32) -> Option<Vec<Built>> {
        let m = *self.by_file.get(file)?;
        let mut n = m.tree.root_node().descendant_for_byte_range(offset, offset)?;
        if n.kind() != "identifier" {
            n = n.parent().filter(|p| p.kind() == "identifier").unwrap_or(n);
        }
        let name = text(n, &m.src).to_string();
        let within = |s: Option<TsNode>| s.map_or(false, |s| std::ptr::eq(m, at.m) && s.start_byte() <= offset && offset < s.end_byte());
        let mut p = n.parent()?;
        if p.kind() == "attribute" {
            p = p.parent()?;
        }
        // `with open(p) as f`: `f` is what the context gives, the file at `p`
        if p.kind() == "as_pattern_target" {
            let value = p.parent().filter(|a| a.kind() == "as_pattern").and_then(|a| named_children(a).into_iter().next())?;
            let here = at.of_definition(m, offset);
            return self.placed_later(here, value, n, depth).or_else(|| Some(self.of(here, value, depth + 1)));
        }
        match p.kind() {
            // `conn, _ = listener.accept()`: the element of what the pattern takes apart, where the
            // code says it
            "pattern_list" | "tuple_pattern" if p.parent().is_some_and(|a| a.kind() == "assignment") => {
                let i = named_children(p).iter().position(|x| x.id() == n.id())?;
                let right = p.parent()?.child_by_field_name("right")?;
                self.element(at.of_definition(m, offset), right, i, depth)
            }
            // `x |= ..` defines `x` as the assignment before it does, with what it adds
            "augmented_assignment" => {
                let first = enclosing_def(p).or_else(|| Some(m.tree.root_node())).and_then(|d| {
                    walk(d, &["assignment"])
                        .into_iter()
                        .find(|a| a.child_by_field_name("left").map_or(false, |l| l.kind() == "identifier" && text(l, &m.src) == name))
                })?;
                self.follow(at, file, first.child_by_field_name("left")?.start_byte(), depth + 1)
            }
            // `for m in mappen`: one of the paths `mappen` holds
            "for_in_clause" | "for_statement" if p.child_by_field_name("left").map(|l| l.id()) == Some(n.id()) => {
                let values = self.of(at.of_definition(m, offset), p.child_by_field_name("right")?, depth + 1);
                // an iterable of nothing known leaves the name the code gives the element
                (!values.iter().all(|b| matches!(b.as_slice(), [Seg::Hole(_)]))).then_some(values)
            }
            // a field a value is built from (a dataclass's, a model's): what its builders hand it
            "assignment" if p.child_by_field_name("type").is_some() && field_of(p, &m.src).is_some() => {
                let class = field_of(p, &m.src)?;
                let id = function_id(m, class)?;
                Some(vec![vec![Seg::Held(format!("{id}.__init__"), name)]])
            }
            "assignment" => {
                let right = p.child_by_field_name("right")?;
                let here = at.of_definition(m, offset);
                // set in another function from what it was handed (`self.cache_dir = Path(cache_dir)`
                // in `__init__`): what that function's callers hand it
                match enclosing_def(p).filter(|_| here.scope.is_none()).and_then(|d| function_id(m, d).map(|id| (d, id))) {
                    Some((d, id)) => {
                        let names: Vec<String> = parameters(d, &m.src, in_class(d)).into_iter().map(|(n, _)| n).collect();
                        let vals = self.of(At { m, scope: Some(d), params: &names }, right, depth + 1);
                        Some(
                            vals.into_iter()
                                .map(|b| b.into_iter().map(|s| if let Seg::Param(x) = s { Seg::Held(id.clone(), x) } else { s }).collect())
                                .collect(),
                        )
                    }
                    None => {
                        // a door made without a place takes the one a later call on it names
                        if let Some(placed) = self.placed_later(here, right, n, depth) {
                            return Some(placed);
                        }
                        let mut values = self.of(here, right, depth + 1);
                        // what the same function adds to it afterwards (`x |= {..}`, `x += [..]`)
                        if let Some(d) = enclosing_def(p) {
                            for aug in walk(d, &["augmented_assignment"]) {
                                let same = aug.child_by_field_name("left").map(|l| l.kind() == "identifier" && text(l, &m.src) == name);
                                if let (Some(true), Some(r)) = (same, aug.child_by_field_name("right")) {
                                    values = union(values, self.of(here, r, depth + 1));
                                }
                            }
                        }
                        Some(values)
                    }
                }
            }
            "function_definition" => {
                let body = p.child_by_field_name("body")?;
                let returns: Vec<TsNode> =
                    walk(body, &["return_statement"]).into_iter().filter(|r| enclosing_def(*r).map(|d| d.id()) == Some(p.id())).collect();
                let here = At { m, scope: Some(p), params: &[] };
                Some(
                    returns
                        .into_iter()
                        .filter_map(|r| named_children(r).into_iter().next())
                        .fold(vec![], |acc, x| union(acc, self.of(here, x, depth + 1))),
                )
            }
            // the parameter of a function handed to be called on each element of a collection
            // (`sorted(ps, key=lambda p: ..)`): one of the elements, as in `for p in ps`
            "lambda_parameters" => {
                let slot = named_children(p).iter().position(|x| x.id() == n.id())?;
                let (here, lambda) = (at.of_definition(m, offset), p.parent().filter(|l| l.kind() == "lambda")?);
                // or the function written as a value is kept by a name the body calls it by
                let kept = || {
                    lambda.parent().filter(|a| a.kind() == "assignment")?.child_by_field_name("left").filter(|l| l.kind() == "identifier")
                };
                let values = self.on_each(here, lambda, slot, depth).or_else(|| self.called_with(here, kept()?, slot, &name, depth))?;
                (!values.iter().all(|b| matches!(b.as_slice(), [Seg::Hole(_)]))).then_some(values)
            }
            "parameters"
            | "default_parameter"
            | "typed_parameter"
            | "typed_default_parameter"
            | "list_splat_pattern"
            | "dictionary_splat_pattern" => {
                // a parameter of a function defined in another's body: what that body hands it
                let (list, item) = if p.kind() == "parameters" { (p, n) } else { (p.parent()?, p) };
                let def = list.parent().filter(|d| d.kind() == "function_definition");
                let outer = def.filter(|d| !in_class(*d)).and_then(enclosing_def);
                if let (Some(d), Some(outer)) = (def, outer) {
                    let names: Vec<String> = parameters(outer, &m.src, in_class(outer)).into_iter().map(|(n, _)| n).collect();
                    let there = At { m, scope: Some(outer), params: &names };
                    let slot = named_children(list).iter().filter(|x| x.kind() != "comment").position(|x| x.id() == item.id());
                    if let Some(v) = slot.and_then(|s| self.called_with(there, d.child_by_field_name("name")?, s, &name, depth)) {
                        return Some(v);
                    }
                }
                Some(vec![if within(at.scope) && at.params.contains(&name) { vec![Seg::Param(name)] } else { hole(&name) }])
            }
            _ => None,
        }
    }

    fn call(&self, at: At, c: TsNode, depth: u32) -> Vec<Built> {
        let src = &at.m.src;
        let Some(f) = c.child_by_field_name("function") else { return vec![hole("call")] };
        // `sorted(p for p in ps)`: the generator is the argument list
        let args: Vec<TsNode> = match c.child_by_field_name("arguments") {
            Some(a) if a.kind() == "generator_expression" => vec![a],
            a => a.map(named_children).unwrap_or_default().into_iter().filter(|a| a.kind() != "comment").collect(),
        };
        let positional: Vec<TsNode> =
            args.iter().copied().filter(|a| !matches!(a.kind(), "keyword_argument" | "list_splat" | "dictionary_splat")).collect();
        let last = called(f, src);
        let last = last.as_str();
        let receiver = || f.child_by_field_name("object").map(|o| self.of(at, o, depth + 1)).unwrap_or_else(|| vec![hole("value")]);
        let joined = |xs: &[TsNode]| {
            xs.iter().fold(vec![vec![]], |acc: Vec<Built>, a| {
                if acc == vec![Vec::<Seg>::new()] {
                    self.of(at, *a, depth + 1)
                } else {
                    cross(&acc, &self.of(at, *a, depth + 1), "/")
                }
            })
        };
        // the directory a package ships its files in, by the package's name: the module's own for
        // `__name__` and its package's for `__package__`
        if self.gives_package_files(at.m, f, last) {
            let named = positional.first().map(|a| match text(*a, src) {
                "__name__" => at.m.id.clone(),
                "__package__" => at.m.package.clone(),
                _ => unquote(text(*a, src)),
            });
            return match named.and_then(|p| self.package_dir(&p)) {
                Some(dir) => vec![vec![Seg::Package(dir)]],
                None => vec![hole("call")],
            };
        }
        // what the environment says, or the default the code takes where it says nothing
        let environ = |at: At| {
            let var = positional.first().map(|a| unquote(text(*a, src))).unwrap_or_default();
            let default = positional.get(1).map(|a| self.of(at, *a, depth + 1)).unwrap_or_default();
            or_else(vec![environment(&var)], default)
        };
        // a door opened at the address it is handed (`socket.create_connection(address)`), one made
        // without a place, or a door to the place of the socket it is called on (`conn.dup()`)
        if self.of_doors(at.m, f) {
            if let Some((_, _, i)) = knowledge::PYTHON_DOORS.iter().find(|(_, name, _)| *name == last) {
                return i.and_then(|i| positional.get(i)).map(|a| self.of(at, *a, depth + 1)).unwrap_or_else(|| vec![hole(last)]);
            }
            if f.kind() == "attribute" && knowledge::PYTHON_DOOR_KEEPERS.contains(&(last, None)) {
                return receiver();
            }
        }
        // a library's path function, where ty finds the call goes into that library
        if let Some(kind) = self.path_function(at.m, f, last) {
            match kind {
                // a join of no parts is the directory the path is relative to: `Path()` is `.`
                PathFn::Join if positional.is_empty() => return vec![lit(".")],
                PathFn::Join => return joined(&positional),
                PathFn::Here => return vec![lit(".")],
                PathFn::Home => return vec![lit("~")],
                PathFn::Same if positional.len() == 1 => return self.of(at, positional[0], depth + 1),
                PathFn::Environment => return environ(at),
                _ => {}
            }
        }
        // a method of a library's path, where ty finds the call goes into that library
        if let Some(kind) = self.path_method(at.m, f, last) {
            return match kind {
                PathMethod::Join => cross(&receiver(), &joined(&positional), "/"),
                PathMethod::Parent => receiver().iter().map(parent).collect(),
                PathMethod::Same => receiver(),
                PathMethod::Extension => {
                    let suffix = positional.first().map(|a| unquote(text(*a, src))).unwrap_or_default();
                    receiver().iter().map(|b| crate::paths::with_suffix(b, &lit(&suffix))).collect()
                }
                PathMethod::Sibling => cross(&receiver().iter().map(parent).collect::<Vec<_>>(), &joined(&positional), "/"),
            };
        }
        // a variable read from the environment's mapping, as ty types it: `os.environ.get(..)`
        let mapping = f.child_by_field_name("object").filter(|_| f.kind() == "attribute");
        if mapping.is_some_and(|o| {
            knowledge::PYTHON_ENVIRONMENT.iter().any(|(lib, class, read)| *read == last && self.of_class(at.m, o, lib, class))
        }) {
            return environ(at);
        }
        match last {
            // the paths below a directory that match a pattern; `iterdir()` is `glob("*")`
            m if f.kind() == "attribute" && self.of_pathlib(at.m, f) => {
                if let Some((_, start, _)) = knowledge::PATH_PATTERNS.iter().find(|(name, _, n)| *name == m && *n == positional.len()) {
                    return cross(&receiver(), &cross(&[lit(start)], &joined(&positional), ""), "/");
                }
            }
            // the elements of what it is handed: `sorted(paths)`, `dict.fromkeys(paths)`
            m if knowledge::SAME_ELEMENTS.contains(&m) && positional.len() == 1 => return self.of(at, positional[0], depth + 1),
            // one of what it is handed: `max(paths, key=..)`, `next(paths, default)`
            m if knowledge::ONE_OF_ELEMENTS.contains(&m) && !positional.is_empty() => {
                return positional.iter().fold(vec![], |acc, a| union(acc, self.of(at, *a, depth + 1)));
            }
            _ => {}
        }
        // a listing of a directory: each element is below it, `*`, as the path or as its name alone
        // (a listing handed no directory lists the one the program runs in)
        if let Some(l) = knowledge::LISTINGS_BELOW.iter().find(|l| l.function == last && self.of_library(at.m, f, l.library)) {
            return match positional.first().filter(|_| !l.names) {
                Some(d) => cross(&self.of(at, *d, depth + 1), &[lit("*")], "/"),
                None => vec![lit("*")],
            };
        }
        // a method of a path gives back something at that path: `p.open("w")`
        if f.kind() == "attribute" && self.of_pathlib(at.m, f) {
            return receiver();
        }
        // a library call handed a path gives back something at that path: `open(p)`, `ZipFile(p)`
        if let Some(sig) = self.signature(at.m, f) {
            if let Some((_, a)) = file_args(sig, c, src).into_iter().find(|(p, _)| p.file == Some(FileArg::Path)) {
                return self.of(at, a, depth + 1);
            }
        }
        // a file a call opens where ty picks none of its signatures (it is handed a value ty does
        // not type) is at what the call is handed first, as the opening's touch is
        let opened =
            || self.types.of(&path_str(&at.m.path)).and_then(|x| x.value((c.start_byte(), c.end_byte()))).is_some_and(|v| v.open_file());
        if let Some(a) = positional.first().filter(|_| self.signature(at.m, f).is_none() && opened()) {
            return self.of(at, *a, depth + 1);
        }
        // a function of the package: what it gives back, its parameters bound to what the call
        // hands them
        if let Some((file, offset)) = self.callee_def(at.m, f) {
            if let Some(v) = self.given(at, &file, offset, c, depth).filter(|v| !v.is_empty()) {
                return v;
            }
            if let Some(v) = self.follow(At { scope: None, params: &[], ..at }, &file, offset, depth).filter(|v| !v.is_empty()) {
                return v;
            }
        }
        vec![hole("call")]
    }

    /// The port of a socket's address written as a host and a port (`("127.0.0.1", 7878)`): the
    /// pair's second, a number the code writes or a name it defines as one (`PORT = 7878`).
    fn port(&self, at: At, pair: TsNode) -> Option<String> {
        let [_, port] = named_children(pair).into_iter().filter(|x| x.kind() != "comment").collect::<Vec<_>>()[..] else { return None };
        let port = beneath(port, WRAPPERS);
        let number = |n: TsNode, src: &[u8]| (n.kind() == "integer").then(|| text(n, src).to_string());
        match port.kind() {
            "integer" => number(port, &at.m.src),
            "identifier" => {
                let Target::Here { file, at: offset } = self.def(at.m, port)? else { return None };
                let m = *self.by_file.get(&file)?;
                let n = m.tree.root_node().descendant_for_byte_range(offset, offset)?;
                let a = n.parent().filter(|a| a.kind() == "assignment" && a.child_by_field_name("left").map(|l| l.id()) == Some(n.id()))?;
                number(beneath(a.child_by_field_name("right")?, WRAPPERS), &m.src)
            }
            _ => None,
        }
    }

    /// Whether calling `f` reaches a function of a library whose values are doors (`knowledge::PYTHON_DOORS`).
    fn of_doors(&self, m: &Module, f: TsNode) -> bool {
        self.types.of(&path_str(&m.path)).map_or(false, |x| {
            x.callees((f.start_byte(), f.end_byte())).iter().any(|c| {
                matches!(&c.target, Target::Lib(l) if knowledge::PYTHON_DOORS.iter().any(|(lib, _, _)| *lib == super::module::package_of(l)))
            })
        })
    }

    /// The element `i` of what `value` gives, where the code says it: of a tuple it writes, or of the
    /// pair a socket's `accept` gives, whose first is a door to the listener's place.
    fn element(&self, at: At, value: TsNode, i: usize, depth: u32) -> Option<Vec<Built>> {
        let value = beneath(value, WRAPPERS);
        match value.kind() {
            "tuple" => named_children(value).into_iter().filter(|x| x.kind() != "comment").nth(i).map(|x| self.of(at, x, depth + 1)),
            "call" => {
                let f = value.child_by_field_name("function").filter(|f| f.kind() == "attribute")?;
                let method = text(f.child_by_field_name("attribute")?, &at.m.src);
                let kept = knowledge::PYTHON_DOOR_KEEPERS.contains(&(method, Some(i))) && self.of_doors(at.m, f);
                kept.then(|| f.child_by_field_name("object").map(|o| self.of(at, o, depth + 1))).flatten()
            }
            _ => None,
        }
    }

    /// The place of a door made without one, as a later call on the name it is kept by names it
    /// (`s = socket.socket()` and then `s.bind(address)`): the address the first such call after
    /// the binding at `name` is handed, in the function or the module the binding is in.
    fn placed_later(&self, at: At, value: TsNode, name: TsNode, depth: u32) -> Option<Vec<Built>> {
        let src = &at.m.src;
        let made = beneath(value, WRAPPERS);
        let f = made.child_by_field_name("function").filter(|_| made.kind() == "call")?;
        let unplaced = knowledge::PYTHON_DOORS.iter().any(|(_, n, i)| i.is_none() && *n == called(f, src)) && self.of_doors(at.m, f);
        if !unplaced {
            return None;
        }
        let scope = enclosing_def(name).unwrap_or(at.m.tree.root_node());
        let named = text(name, src);
        walk(scope, &["call"]).into_iter().filter(|c| c.start_byte() > name.end_byte()).find_map(|c| {
            let g = c.child_by_field_name("function").filter(|g| g.kind() == "attribute")?;
            g.child_by_field_name("object").filter(|o| o.kind() == "identifier" && text(*o, src) == named)?;
            let (_, i) =
                knowledge::PYTHON_DOOR_PLACES.iter().find(|(m, _)| Some(*m) == g.child_by_field_name("attribute").map(|a| text(a, src)))?;
            let args: Vec<TsNode> = c.child_by_field_name("arguments").map(named_children).unwrap_or_default();
            args.into_iter().filter(|a| !matches!(a.kind(), "keyword_argument" | "comment")).nth(*i).map(|a| self.of(at, a, depth + 1))
        })
    }
}

/// The arguments of a call matched to the parameters of its signature: positional ones in
/// order, keyword ones by name.
pub(super) fn file_args<'s, 't>(sig: &'s [Param], call: TsNode<'t>, src: &[u8]) -> Vec<(&'s Param, TsNode<'t>)> {
    let Some(args) = call.child_by_field_name("arguments") else { return vec![] };
    let mut out = vec![];
    let mut i = 0;
    for a in named_children(args).into_iter().filter(|a| a.kind() != "comment") {
        match a.kind() {
            "keyword_argument" => {
                let name = a.child_by_field_name("name").map(|n| text(n, src)).unwrap_or("");
                if let (Some(p), Some(v)) = (sig.iter().find(|p| p.name == name), a.child_by_field_name("value")) {
                    out.push((p, v));
                }
            }
            "list_splat" | "dictionary_splat" => break,
            _ => {
                if let Some(p) = sig.get(i) {
                    out.push((p, a));
                }
                i += 1;
            }
        }
    }
    out
}

/// The id the graph knows a function by: its module, the classes around it, its name. A
/// function inside a function has none of its own.
fn function_id(m: &Module, d: TsNode) -> Option<String> {
    let mut names = vec![text(d.child_by_field_name("name")?, &m.src).to_string()];
    let mut p = d.parent();
    while let Some(x) = p {
        match x.kind() {
            "class_definition" => names.push(text(x.child_by_field_name("name")?, &m.src).to_string()),
            "function_definition" => return None,
            _ => {}
        }
        p = x.parent();
    }
    names.reverse();
    Some(format!("{}.{}", m.id, names.join(".")))
}

/// The class an assignment declares a field of: one written in the body of a class that has no
/// `__init__` of its own, so that it is built from its fields.
fn field_of<'t>(a: TsNode<'t>, src: &[u8]) -> Option<TsNode<'t>> {
    let st = a.parent().filter(|x| x.kind() == "expression_statement")?;
    let class = st.parent().filter(|x| x.kind() == "block")?.parent().filter(|x| x.kind() == "class_definition")?;
    let body = class.child_by_field_name("body")?;
    let own_init = named_children(body).into_iter().any(|d| {
        let d = if d.kind() == "decorated_definition" { d.child_by_field_name("definition").unwrap_or(d) } else { d };
        d.kind() == "function_definition" && d.child_by_field_name("name").map(|n| text(n, src)) == Some("__init__")
    });
    (!own_init).then_some(class)
}

/// Whether a function is a method: defined in a class's body.
fn in_class(d: TsNode) -> bool {
    let mut p = d.parent();
    while let Some(x) = p {
        match x.kind() {
            "class_definition" => return true,
            "function_definition" | "module" => return false,
            _ => p = x.parent(),
        }
    }
    false
}

/// The function a node sits in.
pub(super) fn enclosing_def(n: TsNode) -> Option<TsNode> {
    let mut p = n.parent();
    while let Some(x) = p {
        if x.kind() == "function_definition" {
            return Some(x);
        }
        p = x.parent();
    }
    None
}

/// A function's parameters in order, with their defaults: `self` and `cls` are left out of a
/// method's, so the first is the first a caller passes.
pub(super) fn parameters<'t>(def: TsNode<'t>, src: &[u8], method: bool) -> Vec<(String, Option<TsNode<'t>>)> {
    let Some(ps) = def.child_by_field_name("parameters") else { return vec![] };
    let mut out = vec![];
    for p in named_children(ps) {
        let (name, default) = match p.kind() {
            "identifier" => (Some(p), None),
            "typed_parameter" => (named_children(p).into_iter().find(|c| c.kind() == "identifier"), None),
            "default_parameter" | "typed_default_parameter" => (p.child_by_field_name("name"), p.child_by_field_name("value")),
            _ => continue,
        };
        if let Some(n) = name {
            out.push((text(n, src).to_string(), default));
        }
    }
    if method && out.first().map_or(false, |(n, _)| n == "self" || n == "cls") {
        out.remove(0);
    }
    out
}

/// The defaults an entry's parameters take when it is run as written: a plain default, a click
/// option's or argument's `default=`, typer's `Option(default)`.
pub(super) fn entry_defaults<'t>(def: TsNode<'t>, src: &[u8]) -> Vec<(String, TsNode<'t>)> {
    let mut out = vec![];
    for (name, default) in parameters(def, src, false) {
        if let Some(d) = default {
            let given = match d.kind() {
                "call"
                    if call_path(d.child_by_field_name("function").unwrap_or(d), src)
                        .last()
                        .map_or(false, |l| l == "Option" || l == "Argument") =>
                {
                    d.child_by_field_name("arguments").and_then(|a| {
                        named_children(a)
                            .into_iter()
                            .find(|x| x.kind() != "keyword_argument" && x.kind() != "comment")
                            .or_else(|| keyword(a, src, "default"))
                    })
                }
                _ => Some(d),
            };
            out.extend(given.map(|g| (name.clone(), g)));
        }
    }
    let decorated = def.parent().filter(|p| p.kind() == "decorated_definition");
    for dec in decorated.map(|p| named_children(p).into_iter().filter(|c| c.kind() == "decorator").collect::<Vec<_>>()).unwrap_or_default()
    {
        let Some(call) = named_children(dec).into_iter().find(|c| c.kind() == "call") else { continue };
        let head = call_path(call.child_by_field_name("function").unwrap_or(call), src);
        if !matches!(head.last().map(String::as_str), Some("option" | "argument")) {
            continue;
        }
        let Some(args) = call.child_by_field_name("arguments") else { continue };
        let Some(default) = keyword(args, src, "default") else { continue };
        let names: Vec<String> = named_children(args).into_iter().filter(|a| a.kind() == "string").map(|a| unquote(text(a, src))).collect();
        let name = names
            .iter()
            .find(|n| !n.starts_with('-'))
            .cloned()
            .or_else(|| names.iter().filter(|n| n.starts_with("--")).max_by_key(|n| n.len()).cloned())
            .or_else(|| names.first().cloned());
        if let Some(n) = name {
            out.push((n.trim_start_matches('-').replace('-', "_"), default));
        }
    }
    out
}

/// The name a call's function `f` goes by: `Path` of `Path(..)` and of `pathlib.Path(..)`,
/// `joinpath` of `data_dir().joinpath(..)`.
fn called(f: TsNode, src: &[u8]) -> String {
    let parts = call_path(f, src);
    parts.last().cloned().or_else(|| f.child_by_field_name("attribute").map(|a| text(a, src).to_string())).unwrap_or_default()
}

/// The positional arguments of an argument list, in order.
fn positional(args: TsNode) -> Vec<TsNode> {
    named_children(args)
        .into_iter()
        .filter(|a| !matches!(a.kind(), "keyword_argument" | "list_splat" | "dictionary_splat" | "comment"))
        .collect()
}

fn keyword<'t>(args: TsNode<'t>, src: &[u8], name: &str) -> Option<TsNode<'t>> {
    named_children(args).into_iter().find_map(|a| {
        (a.kind() == "keyword_argument" && a.child_by_field_name("name").map(|n| text(n, src)) == Some(name))
            .then(|| a.child_by_field_name("value"))
            .flatten()
    })
}
