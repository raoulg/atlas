//! The passes that read a crate's modules into the graph: which code is test code and which is
//! exported, the symbols each module declares, its `use` declarations, its `impl` blocks, and the
//! fields, signatures and calls that link them.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use tree_sitter::Node as TsNode;

use super::body::{link_calls, OWN};
use super::globals;
use super::module::{
    attrs_of, constructs, def_node, doc_of, fn_name, items, path_parts, path_to_id, resolve, signature, takes_self, type_names, visible,
    Module,
};
use super::paths::{Eval, Evaluated};
use crate::builder::{foreign_type, ClassInfo, GraphBuilder};
use crate::knowledge;
use crate::model::{EdgeKind, Entry, EntryKind, Node, NodeKind};
use crate::tree::{named_children, span, text, walk};

/// A trait impl found in a module, resolved once bindings exist.
struct Impl<'t> {
    module: String,
    type_parts: Vec<String>,
    trait_parts: Vec<String>,
    body: Option<TsNode<'t>>,
}

/// Whether an attribute compiles what it stands on only to test the crate: `cfg(test)`, or a `cfg`
/// that asks for `test` among all its conditions and negates none (`cfg(all(test, unix))`).
fn cfg_test(attr: &str) -> bool {
    let Some(cond) = attr.strip_prefix("cfg(") else { return false };
    !cond.contains("not(") && !cond.contains("any(") && cond.split(|c: char| !(c.is_alphanumeric() || c == '_')).any(|w| w == "test")
}

/// Whether a node stands under `#[cfg(test)]`: it, or an item around it, is compiled only to test the crate.
fn under_cfg_test(n: TsNode, src: &[u8]) -> bool {
    std::iter::successors(Some(n), |n| n.parent()).any(|a| attrs_of(a, src).iter().any(|x| cfg_test(x)))
}

/// Test code by where it stands: an item under `#[cfg(test)]`, which an inline `mod tests { .. }`
/// gives all it holds (the graph keeps them in the module around it), and a file module declared
/// under one (`#[cfg(test)] mod tests;`), with every module below it.
pub(super) fn mark_tests(modules: &IndexMap<String, Module>, store: &mut GraphBuilder) {
    // module -> the lines of its items that are compiled only for tests
    let mut lines: HashMap<&str, Vec<(u32, u32)>> = HashMap::new();
    // the files and directories of the test modules declared in a file of their own
    let mut declared: Vec<PathBuf> = vec![];
    for m in modules.values().filter(|m| !m.library) {
        let mut todo = named_children(m.tree.root_node());
        while let Some(item) = todo.pop() {
            let tested = attrs_of(item, &m.src).iter().any(|a| cfg_test(a));
            match (item.kind(), item.child_by_field_name("body")) {
                ("mod_item", Some(body)) if !tested => todo.extend(named_children(body)),
                ("mod_item", None) if tested => {
                    let Some(name) = item.child_by_field_name("name").map(|n| text(n, &m.src)) else { continue };
                    // a crate's root and a `mod.rs` keep their modules beside them, any other file in a directory of its name
                    let stem = m.path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                    let base = if m.is_root || matches!(stem, "lib" | "main" | "mod") {
                        m.path.parent().map(Path::to_path_buf).unwrap_or_default()
                    } else {
                        m.path.with_extension("")
                    };
                    declared.push(base.join(format!("{name}.rs")));
                    declared.push(base.join(name));
                }
                _ if tested => lines.entry(m.id.as_str()).or_default().extend(span(item)),
                _ => {}
            }
        }
    }
    for m in modules.values().filter(|m| declared.iter().any(|d| m.path == *d || m.path.starts_with(d))) {
        if let Some(n) = store.nodes.get_mut(&m.id) {
            n.test = true;
        }
    }
    for n in store.nodes.values_mut().filter(|n| n.kind != NodeKind::Module) {
        let within = |(first, last): &(u32, u32)| n.span.is_some_and(|(at, _)| *first <= at && at <= *last);
        if lines.get(n.module.as_str()).is_some_and(|ls| ls.iter().any(within)) {
            n.test = true;
        }
    }
}

/// A `pub` item at the root of a library target is a route into it; a binary exports nothing.
fn exported(m: &Module, item: tree_sitter::Node, src: &[u8]) -> bool {
    m.is_root && m.bin.is_none() && is_pub(item, src)
}

/// Visible outside the crate: `pub`, not `pub(crate)`.
fn is_pub(item: TsNode, src: &[u8]) -> bool {
    named_children(item).iter().any(|c| c.kind() == "visibility_modifier" && text(*c, src) == "pub")
}

/// What a library exports besides its root's own `pub` items: the names its public modules
/// re-export (`pub use`) and the `pub` items of the modules it makes public (`pub mod`), each
/// labelled by its path from the root. A name the root exports keeps the root's label.
pub(super) fn mark_exports(modules: &IndexMap<String, Module>, store: &mut GraphBuilder, crate_ids: &HashMap<String, String>) {
    let mut public: Vec<&Module> = modules.values().filter(|m| m.is_root && m.bin.is_none()).collect();
    let mut i = 0;
    while i < public.len() {
        let m = public[i];
        for item in named_children(m.tree.root_node()).into_iter().filter(|x| x.kind() == "mod_item" && is_pub(*x, &m.src)) {
            let Some(name) = item.child_by_field_name("name") else { continue };
            if let Some(c) = modules.get(&format!("{}.{}", m.id, text(name, &m.src))) {
                if !public.iter().any(|p| p.id == c.id) {
                    public.push(c);
                }
            }
        }
        i += 1;
    }
    let pub_items = |m: &Module| -> Vec<(String, String)> {
        items(m.tree.root_node())
            .into_iter()
            .filter(|x| is_pub(*x, &m.src))
            .filter_map(|x| x.child_by_field_name("name").map(|n| text(n, &m.src).to_string()))
            .filter_map(|n| m.symbols.get(&n).map(|id| (n, id.clone())))
            .collect()
    };
    for m in &public {
        let path = m.id.strip_prefix(&m.krate).unwrap_or(&m.id).trim_start_matches('.').replace('.', "::");
        let label = |name: &str| if path.is_empty() { name.to_string() } else { format!("{path}::{name}") };
        let mut names: Vec<(String, String)> = vec![];
        for u in named_children(m.tree.root_node()).into_iter().filter(|x| x.kind() == "use_declaration" && is_pub(*x, &m.src)) {
            let (mut out, mut wild) = (vec![], vec![]);
            collect_use(u, &[], &m.src, &mut out, &mut wild);
            for (alias, parts) in out {
                if let Some(r) = resolve(m, &parts, modules, &store.nodes, crate_ids) {
                    names.push((alias, r));
                }
            }
            for w in wild {
                if let Some(t) = resolve(m, &w, modules, &store.nodes, crate_ids).and_then(|r| modules.get(&r)) {
                    names.extend(pub_items(t));
                }
            }
        }
        names.extend(pub_items(m));
        for (name, id) in names {
            store.export(&id, label(&name));
        }
    }
}

/// The route a function is: a test, the `main` of a binary, or what the crate exports.
fn entry_of(m: &Module, item: TsNode, src: &[u8], name: &str, attrs: &[String]) -> Option<Entry> {
    if attrs.iter().any(|a| a == "test") {
        Some(Entry::new(EntryKind::Test, name.to_string()))
    } else if let Some(bin) = m.bin.as_ref().filter(|_| name == "main") {
        Some(Entry::new(EntryKind::Cli, bin.clone()))
    } else if exported(m, item, src) {
        Some(Entry::new(EntryKind::Export, name.to_string()))
    } else {
        None
    }
}

pub(super) fn collect_symbols(m: &mut Module, store: &mut GraphBuilder) {
    let src = &m.src;
    let file = crate::tree::path_str(&m.path);
    for item in items(m.tree.root_node()) {
        match item.kind() {
            "function_item" => {
                let name = fn_name(item, src);
                let nid = format!("{}.{}", m.id, name);
                m.symbols.insert(name.clone(), nid.clone());
                let (params, returns, sig) = signature(item, src);
                store.sigs.insert(nid.clone(), sig);
                let attrs = attrs_of(item, src);
                let entry = entry_of(m, item, src, &name, &attrs);
                store.add_node(Node {
                    id: nid.clone(),
                    kind: NodeKind::Function,
                    private: !visible(item),
                    name,
                    file: file.clone(),
                    span: span(item),
                    module: m.id.clone(),
                    doc: doc_of(item, src),
                    decorators: attrs,
                    params,
                    returns,
                    entry,
                    ..Default::default()
                });
                store.add_edge(&m.id, &nid, EdgeKind::Contains, None);
            }
            "struct_item" | "enum_item" | "union_item" | "trait_item" => {
                let name = item.child_by_field_name("name").map(|n| text(n, src)).unwrap_or("").to_string();
                let nid = format!("{}.{}", m.id, name);
                m.symbols.insert(name.clone(), nid.clone());
                let mut fields: Vec<(String, Vec<Vec<String>>)> = vec![];
                let mut variants: Vec<Vec<String>> = vec![];
                let mut arms = vec![];
                let mut methods = vec![];
                if let Some(body) = item.child_by_field_name("body") {
                    if item.kind() == "trait_item" {
                        for f in
                            named_children(body).into_iter().filter(|f| matches!(f.kind(), "function_signature_item" | "function_item"))
                        {
                            let mname = fn_name(f, src);
                            let mnid = format!("{nid}.{mname}");
                            let (params, returns, sig) = signature(f, src);
                            store.sigs.insert(mnid.clone(), sig);
                            store.add_node(Node {
                                id: mnid.clone(),
                                kind: NodeKind::Method,
                                name: mname.clone(),
                                file: file.clone(),
                                span: span(f),
                                module: m.id.clone(),
                                doc: doc_of(f, src),
                                cls: Some(nid.clone()),
                                decorators: attrs_of(f, src),
                                on_class: !takes_self(&params),
                                ctor: constructs(&params, &returns, &name),
                                // a trait's method without a body of its own is what its implementations fill in
                                declared: f.kind() == "function_signature_item",
                                params,
                                returns,
                                ..Default::default()
                            });
                            methods.push(mname);
                            store.add_edge(&nid, &mnid, EdgeKind::Contains, None);
                        }
                    } else {
                        for f in walk(body, &["field_declaration"]) {
                            let fname = f.child_by_field_name("name").map(|n| text(n, src)).unwrap_or("").to_string();
                            fields.push((fname, type_names(f.child_by_field_name("type"), src)));
                        }
                        // an enum is one of its variants, a sum of the types they carry
                        for v in walk(body, &["enum_variant"]) {
                            let Some(carried) = v.child_by_field_name("body") else { continue };
                            let vname = v.child_by_field_name("name").map(|n| text(n, src)).unwrap_or("").to_string();
                            variants.extend(type_names(Some(carried), src));
                            arms.push((vname, type_names(Some(carried), src), span(v)));
                        }
                    }
                }
                store.classes.insert(nid.clone(), ClassInfo { fields, variants, arms, ..Default::default() });
                let entry = exported(m, item, src).then(|| Entry::new(EntryKind::Export, name.clone()));
                store.add_node(Node {
                    id: nid.clone(),
                    kind: NodeKind::Class,
                    private: !visible(item),
                    name,
                    file: file.clone(),
                    span: span(item),
                    module: m.id.clone(),
                    doc: doc_of(item, src),
                    decorators: attrs_of(item, src),
                    methods,
                    entry,
                    ..Default::default()
                });
                store.add_edge(&m.id, &nid, EdgeKind::Contains, None);
            }
            "const_item" | "static_item" | "type_item" => {
                if let Some(n) = item.child_by_field_name("name") {
                    m.symbols.insert(text(n, src).to_string(), format!("{}.{}", m.id, text(n, src)));
                }
            }
            _ => {}
        }
    }
}

fn collect_use(n: TsNode, prefix: &[String], src: &[u8], out: &mut Vec<(String, Vec<String>)>, wild: &mut Vec<Vec<String>>) {
    match n.kind() {
        "use_declaration" => {
            if let Some(a) = n.child_by_field_name("argument") {
                collect_use(a, prefix, src, out, wild);
            }
        }
        "scoped_identifier" => {
            let mut parts = prefix.to_vec();
            parts.extend(path_parts(Some(n), src));
            let alias = parts.last().cloned().unwrap_or_default();
            out.push((alias, parts));
        }
        "identifier" | "crate" | "super" => {
            let mut parts = prefix.to_vec();
            let t = text(n, src).to_string();
            parts.push(t.clone());
            out.push((t, parts));
        }
        "self" => {
            if let Some(last) = prefix.last() {
                out.push((last.clone(), prefix.to_vec()));
            }
        }
        "scoped_use_list" => {
            let mut path = prefix.to_vec();
            path.extend(path_parts(n.child_by_field_name("path"), src));
            if let Some(list) = n.child_by_field_name("list") {
                for c in named_children(list) {
                    collect_use(c, &path, src, out, wild);
                }
            }
        }
        "use_list" => {
            for c in named_children(n) {
                collect_use(c, prefix, src, out, wild);
            }
        }
        "use_as_clause" => {
            let mut parts = prefix.to_vec();
            parts.extend(path_parts(n.child_by_field_name("path"), src));
            let alias = n.child_by_field_name("alias").map(|a| text(a, src)).unwrap_or("").to_string();
            out.push((alias, parts));
        }
        "use_wildcard" => {
            let mut parts = prefix.to_vec();
            parts.extend(path_parts(named_children(n).into_iter().next(), src));
            wild.push(parts);
        }
        _ => {}
    }
}

pub(super) fn collect_uses(m: &mut Module, crate_ids: &HashMap<String, String>, module_ids: &HashSet<String>, store: &mut GraphBuilder) {
    let mut out = vec![];
    let mut wild = vec![];
    // for each name and glob: whether test code imports it, which the module around it does not
    let (mut tested, mut wild_tested) = (vec![], vec![]);
    for u in walk(m.tree.root_node(), &["use_declaration"]) {
        collect_use(u, &[], &m.src, &mut out, &mut wild);
        let test = under_cfg_test(u, &m.src);
        tested.resize(out.len(), test);
        wild_tested.resize(wild.len(), test);
    }
    let mut bindings = vec![];
    let mut edges = vec![];
    let is_module = |id: &str| module_ids.contains(id);
    for ((alias, parts), test) in out.into_iter().zip(tested) {
        let id = path_to_id(m, &parts, crate_ids, &is_module);
        if !id.starts_with("ext:") && !test {
            edges.push(id.clone());
        }
        bindings.push((alias, id));
    }
    for (w, test) in wild.into_iter().zip(wild_tested) {
        let id = path_to_id(m, &w, crate_ids, &is_module);
        // a glob over a name the module binds (`use Kind::*` of its own enum) is no library's
        let bound = w.first().map_or(false, |h| m.symbols.contains_key(h) || bindings.iter().any(|(a, _)| a == h));
        if id.starts_with("ext:") {
            if !bound && !m.globs.contains(&id) {
                m.globs.push(id);
            }
        } else {
            if !test {
                edges.push(id.clone());
            }
            m.wildcards.push(id);
        }
    }
    for (k, v) in bindings {
        m.bindings.insert(k, v);
    }
    for target in edges {
        if module_ids.contains(&target) || store.nodes.contains_key(&target) {
            store.add_edge(&m.id, &target, EdgeKind::Imports, None);
        } else if let Some((modid, _)) = target.rsplit_once('.') {
            if module_ids.contains(modid) {
                store.add_edge(&m.id, modid, EdgeKind::Imports, None);
            }
        }
    }
}

/// The `impl` blocks of the modules `ids`: methods on their types, and the traits they implement.
pub(super) fn collect_impls(
    modules: &IndexMap<String, Module>,
    store: &mut GraphBuilder,
    crate_ids: &HashMap<String, String>,
    ids: &[String],
) {
    let mut impls: Vec<Impl> = vec![];
    for m in ids.iter().filter_map(|id| modules.get(id)) {
        for item in items(m.tree.root_node()).into_iter().filter(|i| i.kind() == "impl_item") {
            impls.push(Impl {
                module: m.id.clone(),
                type_parts: path_parts(item.child_by_field_name("type"), &m.src),
                trait_parts: path_parts(item.child_by_field_name("trait"), &m.src),
                body: item.child_by_field_name("body"),
            });
        }
    }
    for im in impls {
        let m = &modules[&im.module];
        let src = &m.src;
        let Some(cls) = resolve(m, &im.type_parts, modules, &store.nodes, crate_ids).filter(|c| !c.starts_with("ext:")) else { continue };
        if !im.trait_parts.is_empty() {
            let tr = resolve(m, &im.trait_parts, modules, &store.nodes, crate_ids)
                .unwrap_or_else(|| format!("ext:{}", im.trait_parts.join(".")));
            if tr.starts_with("ext:") {
                store.ext_node(&tr);
            }
            store.add_edge(&cls, &tr, EdgeKind::Inherits, None);
            store.classes.entry(cls.clone()).or_default().resolved.push(tr);
        }
        let Some(body) = im.body else { continue };
        let file = crate::tree::path_str(&m.path);
        for f in named_children(body).into_iter().filter(|f| f.kind() == "function_item") {
            let mname = fn_name(f, src);
            let mnid = format!("{cls}.{mname}");
            if store.nodes.contains_key(&mnid) {
                continue;
            }
            let (params, returns, sig) = signature(f, src);
            store.sigs.insert(mnid.clone(), sig);
            let attrs = attrs_of(f, src);
            store.add_node(Node {
                id: mnid.clone(),
                kind: NodeKind::Method,
                name: mname.clone(),
                file: file.clone(),
                span: span(f),
                module: m.id.clone(),
                doc: doc_of(f, src),
                cls: Some(cls.clone()),
                entry: attrs.iter().any(|a| a == "test").then(|| Entry::new(EntryKind::Test, mname.clone())),
                decorators: attrs,
                on_class: !takes_self(&params),
                ctor: constructs(&params, &returns, cls.rsplit('.').next().unwrap_or(&cls)),
                private: im.trait_parts.is_empty() && !visible(f),
                params,
                returns,
                ..Default::default()
            });
            if let Some(c) = store.nodes.get_mut(&cls) {
                c.methods.push(mname);
            }
            store.add_edge(&cls, &mnid, EdgeKind::Contains, None);
        }
    }
}

/// Fields, signatures and bodies into edges: the program's, and those of the library's code read.
pub(super) fn link(modules: &IndexMap<String, Module>, store: &mut GraphBuilder, crate_ids: &HashMap<String, String>) {
    let ids: Vec<String> = store.nodes.keys().cloned().collect();
    for id in &ids {
        let (kind, module) = (store.nodes[id].kind, store.nodes[id].module.clone());
        if kind != NodeKind::Class {
            continue;
        }
        let m = &modules[&module];
        let (fields, arms) = store.classes.get(id).map(|c| (c.fields.clone(), c.arms.clone())).unwrap_or_default();
        for (fname, types) in fields {
            for t in types {
                let Some(r) = resolve(m, &t, modules, &store.nodes, crate_ids) else { continue };
                if r.starts_with("ext:") {
                    store.classes.entry(id.clone()).or_default().foreign.entry(fname.clone()).or_insert(r);
                } else if store.nodes.get(&r).map_or(false, |x| x.kind == NodeKind::Class) {
                    store.classes.entry(id.clone()).or_default().attr_types.insert(fname.clone(), r.clone());
                    store.add_edge(id, &r, EdgeKind::Has, Some(fname.clone()));
                }
            }
        }
        // a variant is the types it carries that the graph holds; one that carries none of them
        // (`Fit { score: f64 }`, `Fit(f64)`) is a type of its own, named after its enum, which a
        // path does not name: `Outcome::Fit { .. }` makes an `Outcome`
        let mut seen: Vec<String> = vec![];
        for (vname, carried, at) in arms {
            let mut known: Vec<String> = vec![];
            for r in carried.iter().filter_map(|t| resolve(m, t, modules, &store.nodes, crate_ids)) {
                let holds = if r.starts_with("ext:") {
                    foreign_type(&r, &OWN)
                } else {
                    store.nodes.get(&r).map_or(false, |x| x.kind == NodeKind::Class)
                };
                if holds && r != *id && !known.contains(&r) {
                    known.push(r);
                }
            }
            if known.is_empty() {
                let enum_node = &store.nodes[id];
                let own = Node {
                    id: format!("{id}::{vname}"),
                    kind: NodeKind::Class,
                    name: format!("{}::{vname}", enum_node.name),
                    file: enum_node.file.clone(),
                    span: at,
                    module: module.clone(),
                    ..Default::default()
                };
                known.push(own.id.clone());
                store.add_node(own);
                store.add_edge(&module, &known[0], EdgeKind::Contains, None);
            }
            for r in known {
                if seen.contains(&r) {
                    continue;
                }
                if r.starts_with("ext:") {
                    store.ext_node(&r);
                }
                store.add_edge(id, &r, EdgeKind::Variant, None);
                seen.push(r);
            }
        }
    }
    let fixed = fixed_by_implementations(modules, store, crate_ids);
    for id in &ids {
        let (kind, module) = (store.nodes[id].kind, store.nodes[id].module.clone());
        if !matches!(kind, NodeKind::Function | NodeKind::Method) {
            continue;
        }
        let m = &modules[&module];
        let sig = store.sigs.get(id).cloned().unwrap_or_default();
        let own = store.nodes[id].cls.clone();
        let def = def_node(m, &store.nodes[id]);
        for (types, kind) in [(&sig.ptypes, EdgeKind::Takes), (&sig.rtypes, EdgeKind::Returns)] {
            for t in types {
                let resolved: Vec<String> = match (t.as_slice(), &own) {
                    // `Self` is the type the impl is for
                    ([s], Some(c)) if s == "Self" => vec![c.clone()],
                    // `Self::Item` is the type the impl fixes it to; in the trait, each its implementations fix
                    ([s, name], Some(c)) if s == "Self" => match def.and_then(impl_around) {
                        Some(item) => {
                            fixed_in(item, name, &m.src).iter().filter_map(|t| resolve(m, t, modules, &store.nodes, crate_ids)).collect()
                        }
                        None => fixed.get(&(c.clone(), name.clone())).cloned().unwrap_or_default(),
                    },
                    _ => resolve(m, t, modules, &store.nodes, crate_ids).into_iter().collect(),
                };
                for r in resolved {
                    signed(store, id, &r, kind);
                }
            }
        }
        take_sums(modules, store, crate_ids, id);
    }
    // every signature before any body: a body's locals take their types from what the functions
    // it calls give back, wherever those are defined
    let eval = Eval::new(modules, &store.nodes, crate_ids);
    let returned = returned_classes(store);
    let (mut waiting, mut args) = (vec![], vec![]);
    // the program's bodies, then those of the library's code read
    let (own, library): (Vec<&String>, Vec<&String>) = ids
        .iter()
        .filter(|id| matches!(store.nodes[*id].kind, NodeKind::Function | NodeKind::Method))
        .partition(|id| !store.nodes[*id].library);
    for id in own.into_iter().chain(library) {
        let l = link_calls(modules, store, crate_ids, &eval, &returned, id);
        waiting.extend(l.waiting.into_iter().map(|(t, b)| (id.clone(), t, b)));
        args.extend(l.args);
    }
    // a path or a statement built from a function's parameters is whole where its callers hand them
    let handing: Vec<Evaluated> = args.iter().map(|arg| Evaluated { arg, eval: &eval }).collect();
    let entry = |f: &str| store.nodes.get(f).map_or(false, |n| n.entry.is_some());
    let family = store.family();
    let completed = crate::paths::complete(waiting, &handing, &family, &|_, _| vec![], &entry);
    crate::paths::place_calls(store, completed, false);
    globals::read(modules, store, crate_ids);
}

/// The `impl` block a function is defined in.
fn impl_around(def: TsNode) -> Option<TsNode> {
    def.parent().filter(|b| b.kind() == "declaration_list").and_then(|b| b.parent()).filter(|i| i.kind() == "impl_item")
}

/// The type an `impl` fixes one of its trait's associated types to, as written: `Book` for `Item` in
/// `impl Store for Shelf { type Item = Book; .. }`.
fn fixed_in(item: TsNode, name: &str, src: &[u8]) -> Vec<Vec<String>> {
    let body = item.child_by_field_name("body").map(named_children).unwrap_or_default();
    body.into_iter()
        .filter(|t| t.kind() == "type_item" && t.child_by_field_name("name").map(|n| text(n, src)) == Some(name))
        .flat_map(|t| type_names(t.child_by_field_name("type"), src))
        .collect()
}

/// What the implementations of each of the program's traits fix its associated types to, by the
/// trait and the type's name: a trait's `Self::Item` is the one each implementation names, so what
/// its method gives is any of them.
fn fixed_by_implementations(
    modules: &IndexMap<String, Module>,
    store: &GraphBuilder,
    crate_ids: &HashMap<String, String>,
) -> HashMap<(String, String), Vec<String>> {
    let mut out: HashMap<(String, String), Vec<String>> = HashMap::new();
    for m in modules.values() {
        for item in items(m.tree.root_node()).into_iter().filter(|i| i.kind() == "impl_item") {
            let parts = path_parts(item.child_by_field_name("trait"), &m.src);
            let Some(tr) = Some(parts).filter(|p| !p.is_empty()).and_then(|p| resolve(m, &p, modules, &store.nodes, crate_ids)) else {
                continue;
            };
            let body = item.child_by_field_name("body").map(named_children).unwrap_or_default();
            for t in body.into_iter().filter(|t| t.kind() == "type_item") {
                let Some(name) = t.child_by_field_name("name").map(|n| text(n, &m.src).to_string()) else { continue };
                let types = out.entry((tr.clone(), name)).or_default();
                for r in
                    type_names(t.child_by_field_name("type"), &m.src).iter().filter_map(|p| resolve(m, p, modules, &store.nodes, crate_ids))
                {
                    if !types.contains(&r) {
                        types.push(r);
                    }
                }
            }
        }
    }
    out
}

/// A type a signature names, as the edge of its kind (`Takes`, `Returns`): one of the program's,
/// or a library's that holds data.
fn signed(store: &mut GraphBuilder, id: &str, r: &str, kind: EdgeKind) {
    if r.starts_with("ext:") {
        if !foreign_type(r, &OWN) {
            return;
        }
        store.ext_node(r);
    }
    store.add_edge(id, r, kind, None);
}

/// The class each function gives back, by the first of its signature's types that is one.
pub(super) fn returned_classes(store: &GraphBuilder) -> HashMap<String, String> {
    let mut out: HashMap<String, String> = HashMap::new();
    for e in
        store.edges.iter().filter(|e| e.kind == EdgeKind::Returns && store.nodes.get(&e.dst).map_or(false, |n| n.kind == NodeKind::Class))
    {
        out.entry(e.src.clone()).or_insert_with(|| e.dst.clone());
    }
    out
}

/// The library's sums a function takes, each a type of its own: `fetched: Result<Value,
/// reqwest::Error>` takes a `Result<Value, reqwest::Error>`, a sum whose variants are the types
/// it is written with, as an enum of the program is a sum of the types its variants hold (a type
/// of the program's or a library's that holds data). Its id is the type with each name resolved,
/// so one sum written two ways is one; a sum of none of those is none the graph holds.
fn take_sums(modules: &IndexMap<String, Module>, store: &mut GraphBuilder, crate_ids: &HashMap<String, String>, id: &str) {
    let n = store.nodes[id].clone();
    let m = &modules[&n.module];
    let src = &m.src;
    let Some(params) = def_node(m, &n).and_then(|d| d.child_by_field_name("parameters")) else { return };
    let resolved = |parts: &[String], nodes: &IndexMap<String, Node>| match (parts, &n.cls) {
        ([s], Some(c)) if s == "Self" => Some(c.clone()),
        _ => resolve(m, parts, modules, nodes, crate_ids),
    };
    for p in named_children(params).into_iter().filter(|p| p.kind() == "parameter") {
        let Some(mut t) = p.child_by_field_name("type") else { continue };
        while let Some(inner) = (t.kind() == "reference_type").then(|| t.child_by_field_name("type")).flatten() {
            t = inner;
        }
        let generic = path_parts(t.child_by_field_name("type").filter(|_| t.kind() == "generic_type"), src);
        if !generic.last().map_or(false, |g| knowledge::RUST_SUMS.contains(&g.as_str())) {
            continue;
        }
        // the program's own enum of that name is read as the program's
        let of = resolved(&generic, &store.nodes);
        if of.as_ref().map_or(false, |r| store.nodes.get(r).map_or(false, |x| x.kind == NodeKind::Class)) {
            continue;
        }
        let mut variants: Vec<String> = vec![];
        for a in t.child_by_field_name("type_arguments").map(named_children).unwrap_or_default() {
            for r in type_names(Some(a), src).iter().filter_map(|x| resolved(x, &store.nodes)) {
                let known = if r.starts_with("ext:") {
                    foreign_type(&r, &OWN)
                } else {
                    store.nodes.get(&r).map_or(false, |x| x.kind == NodeKind::Class)
                };
                if known && !variants.contains(&r) {
                    variants.push(r);
                }
            }
        }
        if variants.is_empty() {
            continue;
        }
        let mut meant = String::new();
        let mut at = t.start_byte();
        for x in walk(t, &["type_identifier", "scoped_type_identifier"]) {
            if x.start_byte() < at || (x.kind() == "type_identifier" && x.parent().map_or(false, |p| p.kind() == "scoped_type_identifier"))
            {
                continue;
            }
            let parts = path_parts(Some(x), src);
            meant.push_str(&String::from_utf8_lossy(&src[at..x.start_byte()]));
            let r = resolved(&parts, &store.nodes).unwrap_or_else(|| parts.join("."));
            meant.push_str(r.strip_prefix("ext:").unwrap_or(&r));
            at = x.end_byte();
        }
        meant.push_str(&String::from_utf8_lossy(&src[at..t.end_byte()]));
        let sum = format!("ext:{}", meant.split_whitespace().collect::<String>());
        if !store.nodes.contains_key(&sum) {
            let home = of.as_deref().map(|r| r.trim_start_matches("ext:")).and_then(|r| r.rsplit_once('.')).map_or("", |(h, _)| h);
            store.add_node(Node {
                id: sum.clone(),
                kind: NodeKind::External,
                name: text(t, src).split_whitespace().collect::<Vec<_>>().join(" "),
                module: home.to_string(),
                ..Default::default()
            });
            for v in variants {
                if v.starts_with("ext:") {
                    store.ext_node(&v);
                }
                store.add_edge(&sum, &v, EdgeKind::Variant, None);
            }
        }
        store.add_edge(id, &sum, EdgeKind::Takes, None);
    }
}
