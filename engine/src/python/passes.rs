//! The passes that read a package's modules into the graph: the symbols each declares, its
//! imports, what it exports, the routes and commands it registers, its type aliases and sums, the
//! values it keeps, and the types, signatures and bodies that link them.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use indexmap::IndexMap;
use tree_sitter::Node as TsNode;

use super::body::{
    by_name, changed, changed_params, field_default, handed, library_change, link_calls, positional, sites, Arg, Changing, Evaluated, Seen,
    OWN,
};
use super::module::{
    call_path, declared, def_node, defined, dotted, first_docline, iter_defs, lib_of, package_of, private, resolve, signature,
    top_assignments, type_names, Module, Own,
};
use super::paths::{self, At, Built, Eval};
use crate::builder::{foreign_type, ClassInfo, GraphBuilder};
use crate::knowledge;
use crate::model::{Dir, EdgeKind, Entry, EntryKind, Methods, Node, NodeKind, Route, Touch, World};
use crate::pytypes::{Target, Types};
use crate::tree::{field_children, literals, named_children, span, text, unquote, walk};

/// A web app's routes at the paths a client asks for: under the prefix an app is mounted at
/// (`app.mount("/api", sub)`), a router is included with (`app.include_router(r, prefix="/v1")`,
/// `app.register_blueprint(bp, url_prefix="/new")`, which stands in for the blueprint's own), a
/// router declares (`APIRouter(prefix="/items")`, `Blueprint(.., url_prefix=..)`,
/// `Router(path=.., route_handlers=[..])`) and a controller names (`Controller.path`), each as
/// the framework the call goes to does it.
pub(super) fn route_prefixes(modules: &IndexMap<String, Module>, store: &mut GraphBuilder, types: &Types, own: &Own) {
    let mut declared: HashMap<String, String> = HashMap::new();
    let mut mounted: HashMap<String, (String, String, bool)> = HashMap::new();
    let sites = sites(modules, store);
    for m in modules.values() {
        let seen = Seen::new(m, types, &sites, modules, store, own);
        let src = &m.src;
        let key = |name: &str| m.bindings.get(name).cloned().unwrap_or_else(|| format!("{}.{name}", m.id));
        for call in walk(m.tree.root_node(), &["call"]) {
            let Some(f) = call.child_by_field_name("function") else { continue };
            // what a call does to routes is what the framework it goes to says
            let lib = seen.library(f);
            let positional: Vec<TsNode> = call
                .child_by_field_name("arguments")
                .map(named_children)
                .unwrap_or_default()
                .into_iter()
                .filter(|a| !matches!(a.kind(), "keyword_argument" | "comment"))
                .collect();
            let keyword = |k: &str| {
                named_children(call.child_by_field_name("arguments")?)
                    .into_iter()
                    .find(|a| a.kind() == "keyword_argument" && a.child_by_field_name("name").map(|n| text(n, src)) == Some(k))
                    .and_then(|a| a.child_by_field_name("value"))
            };
            // an argument as handed: a literal string or a name
            let handed = |arg: &knowledge::Arg, kind: &str| match arg {
                knowledge::Arg::At(i) => positional.get(*i).filter(|a| a.kind() == kind).map(|a| text(*a, src)),
                knowledge::Arg::Named(k) => keyword(k).filter(|v| v.kind() == kind).map(|v| text(v, src)),
            };
            let verb = (f.kind() == "attribute").then(|| f.child_by_field_name("attribute").map(|a| text(a, src))).flatten();
            if let Some(mount) = knowledge::MOUNTS.iter().find(|mt| Some(mt.verb) == verb && lib.as_deref() == Some(mt.package)) {
                let parent = f.child_by_field_name("object").map(|o| text(o, src).to_string());
                let prefix = match handed(&mount.prefix, "string") {
                    Some(p) => Some(unquote(p)),
                    None => matches!(mount.prefix, knowledge::Arg::Named(_)).then(String::new),
                };
                if let (Some(parent), Some(prefix), Some(child)) = (parent, prefix, handed(&mount.child, "identifier")) {
                    let replaces = mount.replaces && !prefix.is_empty();
                    mounted.insert(key(child), (key(&parent), prefix, replaces));
                }
            }
            // a router that carries its own prefix, and the handlers it is handed
            let name = call_path(f, src).last().cloned().unwrap_or_default();
            if let Some(router) = knowledge::PREFIXED_ROUTERS.iter().find(|r| r.class == name && lib.as_deref() == Some(r.package)) {
                let target = call
                    .parent()
                    .filter(|p| p.kind() == "assignment")
                    .and_then(|a| a.child_by_field_name("left"))
                    .filter(|l| l.kind() == "identifier");
                let Some(t) = target.map(|t| key(text(t, src))) else { continue };
                if let Some(prefix) = lit_kw(call, src, router.prefix) {
                    declared.insert(t.clone(), prefix);
                }
                let handlers =
                    router.handlers.and_then(|k| keyword(k)).filter(|l| l.kind() == "list").map(named_children).unwrap_or_default();
                for h in handlers.into_iter().filter(|h| h.kind() == "identifier") {
                    mounted.insert(key(text(h, src)), (t.clone(), String::new(), false));
                }
            }
        }
    }
    // a controller's routes are under the path the class names
    for (id, _) in store.classes.iter() {
        let bases: Vec<&str> =
            store.edges.iter().filter(|e| e.src == *id && e.kind == EdgeKind::Inherits).map(|e| e.dst.as_str()).collect();
        let Some((_, _, attr)) = knowledge::CONTROLLERS.iter().find(|(pkg, base, _)| {
            bases.iter().any(|b| lib_of(b).map(|l| package_of(&l)).as_deref() == Some(*pkg) && b.rsplit('.').next() == Some(*base))
        }) else {
            continue;
        };
        let Some(n) = store.nodes.get(id) else { continue };
        let Some(m) = modules.get(&n.module) else { continue };
        let body = def_node(m, n).and_then(|d| d.child_by_field_name("body"));
        let path = body
            .map(named_children)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|st| named_children(st).into_iter().next().filter(|a| a.kind() == "assignment"))
            .find_map(|a| {
                (a.child_by_field_name("left").map(|l| text(l, &m.src)) == Some(*attr))
                    .then(|| a.child_by_field_name("right"))
                    .flatten()
                    .filter(|v| v.kind() == "string")
                    .map(|v| unquote(text(v, &m.src)))
            });
        if let Some(path) = path {
            declared.insert(id.clone(), path);
        }
    }
    // each route by the app or router it is declared on (`@router.get(..)`), else by the class it
    // is a method of, else by itself (a handler a router is handed)
    let routes: Vec<(String, String)> = store
        .nodes
        .values()
        .filter(|n| n.entry.as_ref().map_or(false, |e| e.kind == EntryKind::Route))
        .filter_map(|n| {
            let m = modules.get(&n.module)?;
            let registers = |rest: &str| rest.split_once('(').map_or(false, |(verb, _)| knowledge::registers_route_by_decorator(verb));
            let recv = n.decorators.iter().find_map(|d| d.split_once('.').filter(|(_, rest)| registers(rest)).map(|(r, _)| r.to_string()));
            let router = match recv {
                Some(r) => m.bindings.get(&r).cloned().unwrap_or_else(|| format!("{}.{r}", m.id)),
                None => n.cls.clone().unwrap_or_else(|| n.id.clone()),
            };
            Some((n.id.clone(), router))
        })
        .collect();
    crate::entries::mount_routes(store, &routes, &declared, &mounted, str::to_string);
}

/// A keyword argument that is a string literal.
fn lit_kw(call: TsNode, src: &[u8], kw: &str) -> Option<String> {
    let args = named_children(call.child_by_field_name("arguments")?);
    let k = args.iter().find(|a| a.kind() == "keyword_argument" && a.child_by_field_name("name").map(|n| text(n, src)) == Some(kw))?;
    let v = k.child_by_field_name("value").filter(|v| v.kind() == "string")?;
    Some(unquote(text(v, src)))
}

pub(super) fn collect_symbols(m: &mut Module, store: &mut GraphBuilder) {
    let root = m.tree.root_node();
    let src = &m.src;
    let file = crate::tree::path_str(&m.path);
    for (decs, d) in iter_defs(root, src) {
        let name = d.child_by_field_name("name").map(|n| text(n, src)).unwrap_or("").to_string();
        let nid = format!("{}.{}", m.id, name);
        m.symbols.insert(name.clone(), nid.clone());
        let body = d.child_by_field_name("body");
        let span = Some((d.start_position().row as u32 + 1, d.end_position().row as u32 + 1));
        if d.kind() == "class_definition" {
            let bases: Vec<Vec<String>> = d
                .child_by_field_name("superclasses")
                .map(|s| named_children(s).into_iter().map(|b| dotted(Some(b), src)).filter(|b| !b.is_empty()).collect())
                .unwrap_or_default();
            let mut methods = vec![];
            let mut fields = vec![];
            if let Some(body) = body {
                for (mdecs, md) in iter_defs(body, src) {
                    let mname = md.child_by_field_name("name").map(|n| text(n, src)).unwrap_or("").to_string();
                    let mnid = format!("{nid}.{mname}");
                    let on_class = mdecs.iter().any(|d| matches!(d.as_str(), "classmethod" | "staticmethod"));
                    let (params, returns, sig) = signature(md, src, !mdecs.iter().any(|d| d == "staticmethod"));
                    let gives_class = returns.as_ref().map_or(false, |r| r.own || r.value.trim_matches(['"', '\'']) == name);
                    let declared_as = declared(&mname, &mdecs);
                    store.declare(
                        Node {
                            id: mnid.clone(),
                            kind: NodeKind::Method,
                            name: mname.clone(),
                            file: file.clone(),
                            span: Some((md.start_position().row as u32 + 1, md.end_position().row as u32 + 1)),
                            module: m.id.clone(),
                            doc: first_docline(md.child_by_field_name("body"), src),
                            cls: Some(nid.clone()),
                            on_class,
                            ctor: mname == "__init__" || on_class && gives_class,
                            private: private(&mname),
                            decorators: mdecs.clone(),
                            params,
                            returns,
                            ..Default::default()
                        },
                        sig,
                        declared_as,
                    );
                    if !methods.contains(&mname) {
                        methods.push(mname);
                    }
                    store.add_edge(&nid, &mnid, EdgeKind::Contains, None);
                }
                for st in named_children(body) {
                    if st.kind() != "expression_statement" {
                        continue;
                    }
                    let Some(a) = named_children(st).into_iter().next() else { continue };
                    if a.kind() != "assignment" {
                        continue;
                    }
                    if let (Some(left), Some(t)) = (a.child_by_field_name("left"), a.child_by_field_name("type")) {
                        if left.kind() == "identifier" {
                            fields.push((text(left, src).to_string(), type_names(Some(t), src)));
                        }
                    }
                }
            }
            store.classes.insert(nid.clone(), ClassInfo { bases: bases.clone(), fields, ..Default::default() });
            let private = private(&name);
            store.add_node(Node {
                id: nid.clone(),
                kind: NodeKind::Class,
                name,
                file: file.clone(),
                span,
                module: m.id.clone(),
                doc: first_docline(body, src),
                decorators: decs,
                private,
                bases: bases.iter().map(|b| b.join(".")).collect(),
                methods,
                ..Default::default()
            });
            store.add_edge(&m.id, &nid, EdgeKind::Contains, None);
        } else {
            let (params, returns, sig) = signature(d, src, false);
            let declared_as = declared(&name, &decs);
            store.declare(
                Node {
                    id: nid.clone(),
                    kind: NodeKind::Function,
                    name: name.clone(),
                    file: file.clone(),
                    span,
                    module: m.id.clone(),
                    doc: first_docline(body, src),
                    decorators: decs.clone(),
                    private: private(&name),
                    params,
                    returns,
                    ..Default::default()
                },
                sig,
                declared_as,
            );
            store.add_edge(&m.id, &nid, EdgeKind::Contains, None);
        }
    }
}

fn resolve_relative(m: &Module, dots: usize, name: Option<&str>) -> String {
    let mut base: Vec<&str> = m.package.split('.').collect();
    if dots > 1 {
        let keep = base.len().saturating_sub(dots - 1);
        base.truncate(keep);
    }
    if let Some(n) = name {
        base.push(n);
    }
    base.join(".")
}

fn import_edge(m: &Module, target: &str, module_ids: &HashSet<String>, store: &mut GraphBuilder, own: &Own) {
    if !own.holds(target) {
        return;
    }
    if module_ids.contains(target) || store.nodes.contains_key(target) {
        store.add_edge(&m.id, target, EdgeKind::Imports, None);
    } else if let Some((modid, _)) = target.rsplit_once('.') {
        if module_ids.contains(modid) {
            store.add_edge(&m.id, modid, EdgeKind::Imports, None);
        }
    }
}

pub(super) fn collect_imports(m: &mut Module, module_ids: &HashSet<String>, store: &mut GraphBuilder, own: &Own) {
    let tree_root = m.tree.root_node();
    let mut bindings: Vec<(String, String)> = vec![];
    let mut edges: Vec<String> = vec![];
    for st in walk(tree_root, &["import_statement", "import_from_statement"]) {
        let src = &m.src;
        if st.kind() == "import_statement" {
            for n in field_children(st, "name") {
                if n.kind() == "aliased_import" {
                    let target = n.child_by_field_name("name").map(|x| text(x, src)).unwrap_or("").to_string();
                    let alias = n.child_by_field_name("alias").map(|x| text(x, src)).unwrap_or("").to_string();
                    bindings.push((alias, target.clone()));
                    edges.push(target);
                } else {
                    let target = text(n, src).to_string();
                    let head = target.split('.').next().unwrap_or("").to_string();
                    bindings.push((head.clone(), head));
                    bindings.push((target.clone(), target.clone()));
                    edges.push(target);
                }
            }
        } else {
            let Some(mn) = st.child_by_field_name("module_name") else { continue };
            let modname = if mn.kind() == "relative_import" {
                let raw = text(mn, src);
                let dots = raw.len() - raw.trim_start_matches('.').len();
                let rest = raw.trim_start_matches('.');
                resolve_relative(m, dots, if rest.is_empty() { None } else { Some(rest) })
            } else {
                text(mn, src).to_string()
            };
            let names = field_children(st, "name");
            if names.is_empty() {
                if named_children(st).iter().any(|n| n.kind() == "wildcard_import") {
                    m.stars.push(modname.clone());
                }
                edges.push(modname.clone());
                continue;
            }
            for n in names {
                let (nm, alias) = if n.kind() == "aliased_import" {
                    (
                        n.child_by_field_name("name").map(|x| text(x, src)).unwrap_or("").to_string(),
                        n.child_by_field_name("alias").map(|x| text(x, src)).unwrap_or("").to_string(),
                    )
                } else {
                    let t = text(n, src).to_string();
                    (t.clone(), t)
                };
                let target = format!("{modname}.{nm}");
                bindings.push((alias, target.clone()));
                edges.push(target);
            }
        }
    }
    for (k, v) in bindings {
        m.bindings.insert(k, v);
    }
    // a function or class under another name is that function or class: `graaf = triple`
    let renamed: Vec<(String, String)> = top_assignments(m)
        .into_iter()
        .filter_map(|a| {
            let (left, right) = (a.child_by_field_name("left")?, a.child_by_field_name("right")?);
            if left.kind() != "identifier" || !matches!(right.kind(), "identifier" | "attribute") || a.child_by_field_name("type").is_some()
            {
                return None;
            }
            let parts = dotted(Some(right), &m.src);
            let head = m.symbols.get(parts.first()?).or_else(|| m.bindings.get(&parts[0]))?;
            let name = text(left, &m.src).to_string();
            (!m.symbols.contains_key(&name))
                .then(|| (name, std::iter::once(head.clone()).chain(parts[1..].iter().cloned()).collect::<Vec<_>>().join(".")))
        })
        .collect();
    m.bindings.extend(renamed);
    for target in edges {
        import_edge(m, &target, module_ids, store, own);
    }
}

/// What `from module import *` binds where it is written: each name the module declares in
/// `__all__`, else each public name it defines or binds. A star of a module that stars another
/// binds what that one brings as well. A module the reading does not hold (a library's) binds
/// nothing Atlas can name.
pub(super) fn star_imports(modules: &mut IndexMap<String, Module>) {
    // a chain of stars longer than this goes round in a cycle
    const STARS: usize = 8;
    for _ in 0..STARS {
        let mut bound: Vec<(String, String, String)> = vec![];
        for m in modules.values() {
            for from in m.stars.iter().filter_map(|s| modules.get(s)) {
                let names = declared_all(from).unwrap_or_else(|| {
                    from.symbols.keys().chain(from.bindings.keys()).filter(|n| !n.starts_with('_') && !n.contains('.')).cloned().collect()
                });
                for name in names.into_iter().filter(|n| !m.symbols.contains_key(n) && !m.bindings.contains_key(n)) {
                    let target = format!("{}.{name}", from.id);
                    bound.push((m.id.clone(), name, target));
                }
            }
        }
        if bound.is_empty() {
            break;
        }
        for (id, name, target) in bound {
            if let Some(m) = modules.get_mut(&id) {
                m.bindings.insert(name, target);
            }
        }
    }
}

/// The names a module declares public in `__all__`, when it does.
fn declared_all(m: &Module) -> Option<Vec<String>> {
    top_assignments(m).into_iter().find_map(|a| {
        let left = a.child_by_field_name("left").filter(|l| text(*l, &m.src) == "__all__")?;
        Some(literals(a.child_by_field_name("right").unwrap_or(left), &m.src, &["string"]))
    })
}

pub(super) fn link(modules: &IndexMap<String, Module>, store: &mut GraphBuilder, own: &Own, types: &Types, root_dir: &Path) {
    let aliases = type_aliases(modules);
    add_sums(modules, store, &aliases, own);
    let ids: Vec<String> = store.nodes.keys().cloned().collect();
    // bases and annotated fields
    for id in &ids {
        let (kind, module) = (store.nodes[id].kind, store.nodes[id].module.clone());
        if kind != NodeKind::Class {
            continue;
        }
        let m = &modules[&module];
        let info = store.classes.get(id).cloned().unwrap_or_default();
        let mut resolved = vec![];
        for b in &info.bases {
            let r = resolve(m, b, modules, &store.nodes, own).unwrap_or_else(|| format!("ext:{}", b.join(".")));
            if r.starts_with("ext:") {
                store.ext_node(&r);
            }
            store.add_edge(id, &r, EdgeKind::Inherits, None);
            resolved.push(r);
        }
        store.classes.entry(id.clone()).or_default().resolved = resolved;
        // a field's type is read as a signature's: a name the module aliases is the sum it names
        for (fname, types) in &info.fields {
            for t in types {
                for r in type_targets(m, t, modules, &aliases, &store.nodes, own).into_iter().filter(|r| !r.starts_with("ext:")) {
                    store.classes.entry(id.clone()).or_default().attr_types.insert(fname.clone(), r.clone());
                    store.add_edge(id, &r, EdgeKind::Has, Some(fname.clone()));
                }
            }
        }
    }
    // a method only declared: abstract, or of a protocol, which what has its methods implements
    let protocols: HashSet<String> = knowledge::PYTHON_PROTOCOLS.iter().map(|(p, n)| format!("ext:{p}.{n}")).collect();
    for id in &ids {
        let n = &store.nodes[id];
        let Some(cls) = n.cls.as_deref().filter(|_| n.kind == NodeKind::Method) else { continue };
        let of_protocol = store.classes.get(cls).map_or(false, |c| c.resolved.iter().any(|b| protocols.contains(b)));
        let abstract_ = n.decorators.iter().any(|d| knowledge::PYTHON_ABSTRACT.contains(&d.rsplit('.').next().unwrap_or(d)));
        if of_protocol || abstract_ {
            store.nodes.get_mut(id).expect("a node of the graph").declared = true;
        }
    }
    // signatures -> takes / returns, every one before any body is read
    for id in &ids {
        let (kind, module) = (store.nodes[id].kind, store.nodes[id].module.clone());
        if !matches!(kind, NodeKind::Function | NodeKind::Method) {
            continue;
        }
        let m = &modules[&module];
        let sig = store.sigs.get(id).cloned().unwrap_or_default();
        for (types, unions, kind) in [(&sig.ptypes, &sig.punions, EdgeKind::Takes), (&sig.rtypes, &sig.runions, EdgeKind::Returns)] {
            let mut targets: Vec<String> = types.iter().flat_map(|t| type_targets(m, t, modules, &aliases, &store.nodes, own)).collect();
            // a union of two or more known types is a sum, written in place as by a name
            for u in unions {
                let mut variants: Vec<String> = vec![];
                for r in u.names.iter().flat_map(|t| type_targets(m, t, modules, &aliases, &store.nodes, own)) {
                    if (!r.starts_with("ext:") || foreign_type(&r, &OWN)) && !variants.contains(&r) {
                        variants.push(r);
                    }
                }
                if variants.len() > 1 {
                    let at = store.nodes[id].clone();
                    targets.push(store.written_sum(&variants, &u.written, &at));
                } else {
                    targets.extend(variants);
                }
            }
            for r in targets {
                if r.starts_with("ext:") {
                    if !foreign_type(&r, &OWN) {
                        continue;
                    }
                    store.ext_node(&r);
                }
                store.add_edge(id, &r, kind, None);
            }
        }
        // a type written through an alias is held by the name of the type it stands for
        let held = crate::model::holds_resolved(&store.nodes[id], |h| resolve(m, &[h.to_string()], modules, &store.nodes, own));
        crate::model::set_holds(store.nodes.get_mut(id).expect("a node of the graph"), held);
    }
    let sites = sites(modules, store);
    // whether an override keeps its base's promise, and which base's it breaks, as ty decides it
    for id in &ids {
        let n = &store.nodes[id];
        let at = (n.kind == NodeKind::Method)
            .then(|| def_node(&modules[&n.module], n))
            .flatten()
            .and_then(|d| d.child_by_field_name("name"))
            .map(|x| x.start_byte());
        let facts = types.of(&n.file);
        let keeps = at.and_then(|at| facts?.keeps(at));
        let breaks = at.and_then(|at| match facts?.breaks(at)? {
            Target::Here { file, at } => sites.get(&(file.clone(), *at)).cloned(),
            Target::Lib(_) => None,
        });
        if let Some(n) = store.nodes.get_mut(id) {
            n.keeps = keeps;
            n.breaks = breaks;
        }
    }
    // what a framework registers, and what the attributes hold, as ty types them
    mark_registered(modules, store, types, &sites, own);
    attributes(modules, store, types, &sites, own);
    // the methods that change their receiver and the parameters each function changes, directly or
    // through a call of the package's that changes what it runs on or is handed (`self.add(..)`,
    // `reset(board)`): until nothing changes, which ends, as the sets only grow and are finite
    let mut changing = Changing::new();
    loop {
        let before = (store.mutating.len(), changing.values().map(Vec::len).sum::<usize>());
        let mut found = vec![];
        for id in &ids {
            let n = &store.nodes[id];
            let method = n.kind == NodeKind::Method;
            if !method && n.kind != NodeKind::Function {
                continue;
            }
            let m = &modules[&n.module];
            let Some(d) = def_node(m, n) else { continue };
            let Some(body) = d.child_by_field_name("body") else { continue };
            let seen = Seen::new(m, types, &sites, modules, store, own);
            let cls = n.cls.as_deref().filter(|_| method);
            let (receiver, params) = changed_params(&seen, &changing, d, body, method, cls);
            found.push((id.clone(), receiver && method && n.name != "__init__" && cls.is_some(), params));
        }
        for (id, receiver, params) in found {
            if receiver {
                store.mutating.insert(id.clone());
            }
            if !params.is_empty() {
                changing.insert(id, params);
            }
        }
        if (store.mutating.len(), changing.values().map(Vec::len).sum::<usize>()) == before {
            break;
        }
    }
    // bodies -> calls / instantiates / boundaries
    let eval = Eval::new(modules, types, root_dir);
    // a library's class by its package and its name, as the signatures name it
    let ext: HashMap<(String, String), String> = store
        .nodes
        .keys()
        .filter(|k| k.starts_with("ext:") && !k.contains('['))
        .filter_map(|k| Some(((package_of(&k[4..]), k.rsplit('.').next()?.to_string()), k.clone())))
        .collect();
    let mut handed = vec![];
    let mut waiting: Vec<(String, Touch, Vec<Built>)> = vec![];
    let mut args: Vec<Arg> = vec![];
    for id in &ids {
        if matches!(store.nodes[id].kind, NodeKind::Function | NodeKind::Method | NodeKind::Module) {
            let linked = link_calls(modules, store, types, &sites, &ext, &eval, own, &changing, id);
            handed.extend(linked.handed);
            waiting.extend(linked.waiting.into_iter().map(|(t, b)| (id.clone(), t, b)));
            args.extend(linked.args);
        }
    }
    // a parameter a call leaves out holds its declared default, and an entry's what it is run with
    let completed = {
        let store = &*store;
        let default = |f: &str, p: &str| -> Vec<Built> {
            // a value built from its fields: the field's declared default
            if let Some((m, d)) = field_default(modules, store, f, p) {
                return eval.of(At { m, scope: None, params: &[] }, d, 0);
            }
            let Some(n) = store.nodes.get(f) else { return vec![] };
            let Some(m) = modules.get(&n.module) else { return vec![] };
            let Some(def) = def_node(m, n) else { return vec![] };
            let declared =
                paths::parameters(def, &m.src, n.kind == NodeKind::Method).into_iter().find(|(q, _)| q == p).and_then(|(_, d)| d);
            let entry =
                n.entry.is_some().then(|| paths::entry_defaults(def, &m.src).into_iter().find(|(q, _)| q == p).map(|(_, d)| d)).flatten();
            [entry, declared].into_iter().flatten().flat_map(|d| eval.of(At { m, scope: None, params: &[] }, d, 0)).collect()
        };
        let handing: Vec<Evaluated> = args.iter().map(|arg| Evaluated { arg, modules, eval: &eval }).collect();
        let entry = |f: &str| store.nodes.get(f).map_or(false, |n| n.entry.is_some());
        let family = store.family();
        crate::paths::complete(waiting, &handing, &family, &default, &entry)
    };
    crate::paths::place_calls(store, completed, false);
    globals(modules, store, types);
    // the work handed to a managed transaction (`session.execute_read(work)`) goes its way
    for (work, kind, dir) in handed {
        if let Some(n) = store.nodes.get_mut(&work) {
            for t in n.touches.iter_mut().filter(|t| t.kind == kind && t.dir.is_none()) {
                t.dir = Some(dir);
            }
        }
    }
}

/// The values a module keeps between calls, which its functions change: a name a module binds at
/// its top level that a function rebinds (`global df`) or changes in place (`df.drop(..,
/// inplace=True)`, `LATEST[k] = v`), in its own module or where another imports it. Every
/// function that changes one writes it and every one that uses it reads it: a place,
/// `Global(module.name)`. A name no function changes is a constant of the code, and no place.
fn globals(modules: &IndexMap<String, Module>, store: &mut GraphBuilder, types: &Types) {
    // per function: what it writes, reads, and tests for a key, each kept value by name and line
    type Named = Vec<(String, String, u32)>;
    type Uses = (String, Named, Named, Named);
    let mut uses: Vec<Uses> = vec![];
    for m in modules.values() {
        let src = &m.src;
        let top: HashSet<String> = named_children(m.tree.root_node())
            .into_iter()
            .filter(|s| s.kind() == "expression_statement")
            .filter_map(|s| named_children(s).into_iter().next())
            .filter(|a| a.kind() == "assignment")
            .filter_map(|a| a.child_by_field_name("left"))
            .filter(|l| l.kind() == "identifier")
            .map(|l| text(l, src).to_string())
            .collect();
        // where a name of the module's scope is kept: its own top level, or the module it is imported from
        let kept = |n: &str| if top.contains(n) { Some(format!("{}.{n}", m.id)) } else { m.bindings.get(n).cloned() };
        let facts = types.of(&crate::tree::path_str(&m.path));
        // a module is code, not a value, and so are a class and a function: what an `import` binds,
        // or what ty says is code
        let imported: HashSet<&str> = walk(m.tree.root_node(), &["import_statement"])
            .into_iter()
            .flat_map(|st| field_children(st, "name"))
            .filter_map(|n| match n.kind() {
                "aliased_import" => n.child_by_field_name("alias").map(|a| text(a, src)),
                _ => text(n, src).split('.').next(),
            })
            .collect();
        let value = |x: TsNode| facts.and_then(|f| f.value((x.start_byte(), x.end_byte())));
        let code = |x: TsNode| imported.contains(text(x, src)) || value(x).map_or(false, |v| v.is_code || v.is_class);
        let fns: Vec<(String, TsNode)> = store
            .nodes
            .values()
            .filter(|n| n.module == m.id && matches!(n.kind, NodeKind::Function | NodeKind::Method))
            .filter_map(|n| Some((n.id.clone(), def_node(m, n)?)))
            .collect();
        for (id, def) in &fns {
            let Some(body) = def.child_by_field_name("body") else { continue };
            let mine = |n: &TsNode| paths::enclosing_def(*n).map(|d| d.id()) == Some(def.id());
            let declared: HashSet<String> = walk(body, &["global_statement"])
                .into_iter()
                .filter(mine)
                .flat_map(|g| named_children(g).into_iter().map(|x| text(x, src).to_string()))
                .collect();
            // what the function binds itself, unless it says the name is the module's
            let mut own: HashSet<String> = paths::parameters(*def, src, false).into_iter().map(|(n, _)| n).collect();
            for a in walk(body, &["assignment", "augmented_assignment", "for_statement"]).into_iter().filter(mine) {
                if let Some(l) = a.child_by_field_name("left").filter(|l| l.kind() == "identifier") {
                    own.insert(text(l, src).to_string());
                }
            }
            own.retain(|n| !declared.contains(n));
            // the value of the module's scope a chain reaches, and the chain's name for it: the name it
            // starts from (`LATEST[s]`), or, from a module, the value the module holds, when ty says it
            // is data (`os.environ[k]`, not `mpl.colors` or `pd.DataFrame`)
            let held = |x: TsNode| -> Option<(String, String)> {
                let (mut root, mut first) = (x, x);
                while let Some(inner) = match root.kind() {
                    "attribute" => root.child_by_field_name("object"),
                    "subscript" => root.child_by_field_name("value"),
                    _ => None,
                } {
                    (first, root) = (root, inner);
                }
                let name = text(root, src);
                if root.kind() != "identifier" || own.contains(name) {
                    return None;
                }
                if !code(root) {
                    return kept(name).map(|k| (k, name.to_string()));
                }
                let attr = first.child_by_field_name("attribute").filter(|_| first.kind() == "attribute")?;
                if !value(first).map_or(false, |v| !v.is_code && !v.is_class) {
                    return None;
                }
                let module = match facts.and_then(|f| f.def((first.start_byte(), first.end_byte()))) {
                    Some(Target::Lib(lib)) => lib.clone(),
                    _ => kept(name)?,
                };
                Some((format!("{module}.{}", text(attr, src)), text(first, src).to_string()))
            };
            let line = |n: TsNode| n.start_position().row as u32 + 1;
            let mut writes: Vec<(String, String, u32)> = vec![];
            for a in walk(body, &["assignment", "augmented_assignment"]).into_iter().filter(mine) {
                if let Some(l) = a.child_by_field_name("left").filter(|l| l.kind() == "identifier" && declared.contains(text(*l, src))) {
                    writes.extend(kept(text(l, src)).map(|k| (k, text(l, src).to_string(), line(a))));
                }
            }
            for x in changed(body, &|c| library_change(facts, c, src).into_iter().collect()) {
                if let Some((k, n)) = held(x).filter(|(k, _)| !writes.iter().any(|(w, _, _)| w == k)) {
                    writes.push((k, n, line(x)));
                }
            }
            let (mut reads, mut tests): (Vec<(String, String, u32)>, Vec<(String, String, u32)>) = (vec![], vec![]);
            for x in walk(body, &["identifier"]) {
                let named_here = x.parent().map_or(false, |p| {
                    // a name the code binds or labels, not the value it holds
                    matches!(
                        p.kind(),
                        "global_statement"
                            | "keyword_argument"
                            | "function_definition"
                            | "parameters"
                            | "default_parameter"
                            | "typed_parameter"
                            | "typed_default_parameter"
                    ) || (p.kind() == "attribute" && p.child_by_field_name("attribute").map(|a| a.id()) == Some(x.id()))
                        || (p.kind() == "assignment" && p.child_by_field_name("left").map(|l| l.id()) == Some(x.id()))
                        // the value an item or an attribute is written into: `d[k] = v` writes `d`, it reads nothing of it
                        || (matches!(p.kind(), "subscript" | "attribute")
                            && p.child_by_field_name(if p.kind() == "subscript" { "value" } else { "object" }).map(|v| v.id()) == Some(x.id())
                            && p.parent().map_or(false, |a| a.kind() == "assignment" && a.child_by_field_name("left").map(|l| l.id()) == Some(p.id())))
                });
                if named_here {
                    continue;
                }
                let chain = x
                    .parent()
                    .filter(|p| p.kind() == "attribute" && p.child_by_field_name("object").map(|o| o.id()) == Some(x.id()))
                    .unwrap_or(x);
                // `k in d` tests what the value holds, as `path.exists()` tests the disk
                let into = if tests_membership(chain, src) { &mut tests } else { &mut reads };
                if let Some((k, n)) = held(chain).filter(|(k, _)| !into.iter().any(|(r, _, _)| r == k)) {
                    into.push((k, n, line(x)));
                }
            }
            uses.push((id.clone(), writes, reads, tests));
        }
    }
    let state: HashSet<String> = uses.iter().flat_map(|(_, w, _, _)| w.iter().map(|(k, _, _)| k.clone())).collect();
    for (id, writes, reads, tests) in uses {
        for (dir, names, test) in [(Dir::Write, writes, false), (Dir::Read, reads, false), (Dir::Read, tests, true)] {
            for (kept, name, at) in names.into_iter().filter(|(k, _, _)| state.contains(k)) {
                let op = if test { crate::effects::MEMBERSHIP.to_string() } else { name.clone() };
                let t = Touch { what: name.clone(), artifact: kept, op, line: at, ..Touch::new(World::Global, Some(dir)) };
                store.touch(&id, t);
            }
        }
    }
}

/// Whether `x` is what a membership test looks in: the right of `k in d`, `k not in d`.
fn tests_membership(x: TsNode, src: &[u8]) -> bool {
    let Some(p) = x.parent().filter(|p| p.kind() == "comparison_operator") else { return false };
    let right = named_children(p).last().map(|r| r.id()) == Some(x.id());
    let ops: Vec<&str> = (0..p.child_count()).filter_map(|i| p.child(i)).filter(|c| !c.is_named()).map(|c| text(c, src)).collect();
    right && ops.iter().any(|o| matches!(*o, "in" | "not in" | "not"))
}

/// What a class's attributes hold, as its methods assign them (`self.store = Store(..)`). An
/// attribute whose type names a class of the package is an edge `has`.
fn attributes(
    modules: &IndexMap<String, Module>,
    store: &mut GraphBuilder,
    types: &Types,
    sites: &HashMap<(String, usize), String>,
    own: &Own,
) {
    let mut found: Vec<(String, String, String)> = vec![];
    for n in store.nodes.values().filter(|n| n.kind == NodeKind::Method) {
        let (Some(cls), Some(m)) = (n.cls.clone(), modules.get(&n.module)) else { continue };
        let Some(body) = def_node(m, n).and_then(|d| d.child_by_field_name("body")) else { continue };
        let seen = Seen::new(m, types, sites, modules, store, own);
        for a in walk(body, &["assignment"]) {
            let Some(left) = a.child_by_field_name("left").filter(|l| l.kind() == "attribute") else { continue };
            let lp = dotted(Some(left), &m.src);
            if lp.len() != 2 || lp[0] != "self" {
                continue;
            }
            let Some(v) = a.child_by_field_name("right").and_then(|r| seen.value(r)) else { continue };
            for c in seen.classes(v) {
                found.push((cls.clone(), lp[1].clone(), c));
            }
        }
    }
    for (cls, attr, c) in found {
        store.classes.entry(cls.clone()).or_default().attr_types.entry(attr.clone()).or_insert(c.clone());
        store.add_edge(&cls, &c, EdgeKind::Has, Some(attr));
    }
}

/// The functions a framework registers by a decorator, each a route into the program by the name
/// it registers: a command line's commands (`@cli.command()`), a web app's routes
/// (`@app.get("/x")`, `@app.route("/x", methods=[..])`, Litestar's `@get("/x")`), and a tool
/// server's tools for an agent, by a decorator on a value of the server's class (`@geo.tool`,
/// `@server.prompt("name")`). What registers is the library the decorator comes from, or the
/// class of the value it is on, as ty says, whatever either is called.
fn mark_registered(
    modules: &IndexMap<String, Module>,
    store: &mut GraphBuilder,
    types: &Types,
    sites: &HashMap<(String, usize), String>,
    own: &Own,
) {
    let mut found: Vec<(String, Entry)> = vec![];
    for m in modules.values() {
        let seen = Seen::new(m, types, sites, modules, store, own);
        let server = |o: TsNode| {
            seen.value(o).map_or(false, |v| {
                v.classes
                    .iter()
                    .zip(&v.names)
                    .any(|(t, name)| seen.package(t).map_or(false, |p| knowledge::TOOL_SERVERS.contains(&(p.as_str(), name.as_str()))))
            })
        };
        for n in store
            .nodes
            .values()
            .filter(|n| n.module == m.id && matches!(n.kind, NodeKind::Function | NodeKind::Method) && n.entry.is_none())
        {
            let Some(decorated) = def_node(m, n).and_then(|d| d.parent()).filter(|p| p.kind() == "decorated_definition") else { continue };
            let registered = named_children(decorated)
                .into_iter()
                .filter(|d| d.kind() == "decorator")
                .filter_map(|d| named_children(d).into_iter().next())
                .find_map(|d| {
                    let (f, call) = match d.kind() {
                        "call" => (d.child_by_field_name("function")?, Some(d)),
                        _ => (d, None),
                    };
                    let name = match f.kind() {
                        "attribute" => text(f.child_by_field_name("attribute")?, &m.src),
                        "identifier" => text(f, &m.src),
                        _ => return None,
                    };
                    let from = seen.library(f);
                    if let Some(cli) = from.as_deref().and_then(knowledge::cli_framework).filter(|c| c.commands.contains(&name)) {
                        let named = call
                            .and_then(|c| handed(c, 0, knowledge::COMMAND_NAME, &m.src))
                            .filter(|a| a.kind() == "string")
                            .map(|a| unquote(text(a, &m.src)));
                        let mut entry = Entry::new(EntryKind::Cli, named.unwrap_or_else(|| cli.command_name(&n.name)));
                        entry.options = command_options(&seen, cli, decorated, &m.src);
                        return Some(entry);
                    }
                    if let Some(web) = from.as_deref().and_then(knowledge::web_framework) {
                        return route(web, name, call, &m.src).map(Entry::route);
                    }
                    let on_server = f.kind() == "attribute" && f.child_by_field_name("object").map_or(false, server);
                    (on_server && knowledge::TOOL_DECORATORS.contains(&name)).then(|| {
                        let named = call.and_then(positional).filter(|a| a.kind() == "string").map(|a| unquote(text(a, &m.src)));
                        Entry::new(EntryKind::Mcp, named.unwrap_or_else(|| n.name.clone()))
                    })
                });
            found.extend(registered.map(|e| (n.id.clone(), e)));
        }
    }
    for (id, entry) in found {
        if let Some(n) = store.nodes.get_mut(&id) {
            n.entry = Some(entry);
        }
    }
}

/// What a command of a command-line framework takes from the command line, each as the human
/// types it, in the order the code declares them: by the command's parameters (typer's
/// `raw: Path = RAW` is `--raw`, `name: str` is `<name>`) or by its decorators
/// (`@click.option("--out", "-o")` is `--out`, `@click.argument("src")` is `<src>`). A call
/// declares one only where it is the framework's.
fn command_options(seen: &Seen, cli: &knowledge::CliFramework, decorated: TsNode, src: &[u8]) -> Vec<String> {
    let strings = |call: TsNode| {
        call.child_by_field_name("arguments")
            .map(named_children)
            .unwrap_or_default()
            .into_iter()
            .filter(|a| a.kind() == "string")
            .map(|a| unquote(text(a, src)))
            .collect::<Vec<String>>()
    };
    // the framework's declaring function a call is, by its last name
    let declares = |call: TsNode| -> Option<String> {
        let f = call.child_by_field_name("function")?;
        let name = call_path(f, src).last().cloned()?;
        (seen.library(f).as_deref() == Some(cli.package)).then_some(name)
    };
    let mut out = vec![];
    match cli.options {
        knowledge::OptionsBy::Parameters { option, argument } => {
            let Some(params) = named_children(decorated)
                .into_iter()
                .find(|d| d.kind() == "function_definition")
                .and_then(|d| d.child_by_field_name("parameters"))
            else {
                return out;
            };
            for p in named_children(params) {
                let (name, ty, default) = match p.kind() {
                    "identifier" => (Some(p), None, None),
                    "typed_parameter" => {
                        (named_children(p).into_iter().find(|c| c.kind() == "identifier"), p.child_by_field_name("type"), None)
                    }
                    "default_parameter" | "typed_default_parameter" => {
                        (p.child_by_field_name("name"), p.child_by_field_name("type"), p.child_by_field_name("value"))
                    }
                    _ => continue,
                };
                let Some(name) = name.map(|x| text(x, src).to_string()) else { continue };
                // what the default or the annotation declares (`= typer.Option(..)`, `Annotated[.., typer.Argument()]`)
                let declared = [default, ty]
                    .into_iter()
                    .flatten()
                    .flat_map(|x| walk(x, &["call"]))
                    .find_map(|c| declares(c).filter(|d| d == option || d == argument).map(|d| (d, c)));
                out.push(match declared {
                    Some((d, _)) if d == argument => crate::entries::argument(&name),
                    Some((_, c)) => crate::entries::flag(&strings(c), &name),
                    None if default.is_some() => crate::entries::flag(&[], &name),
                    None => crate::entries::argument(&name),
                });
            }
        }
        knowledge::OptionsBy::Decorators { option, argument } => {
            for d in named_children(decorated)
                .into_iter()
                .filter(|d| d.kind() == "decorator")
                .filter_map(|d| named_children(d).into_iter().find(|c| c.kind() == "call"))
            {
                let given = strings(d);
                match declares(d) {
                    Some(k) if k == option => {
                        out.push(crate::entries::flag(&given, given.iter().find(|s| !s.starts_with('-')).map_or("", String::as_str)))
                    }
                    Some(k) if k == argument => out.extend(given.first().map(|a| crate::entries::argument(a))),
                    _ => {}
                }
            }
        }
    }
    out
}

/// The route a web framework's decorator registers: the method it is named for, else the methods
/// it is handed (a string or an enum's member each, in a list, a tuple or a set, whose order says
/// nothing), and the path it is handed. A path that is not a literal leaves the route without one.
fn route(web: &knowledge::WebFramework, name: &str, call: Option<TsNode>, src: &[u8]) -> Option<Route> {
    let methods = if knowledge::registers_route(name) {
        knowledge::methods_served(name)
    } else if web.routes.contains(&name) {
        let listed = match call.and_then(|c| by_name(c, web.methods, src)) {
            Some(h) if matches!(h.kind(), "list" | "tuple" | "set") => named_children(h),
            Some(h) => vec![h],
            None => vec![],
        };
        let handed = Methods::of(listed.into_iter().filter_map(|x| match x.kind() {
            "string" => Some(unquote(text(x, src))),
            "attribute" => x.child_by_field_name("attribute").map(|a| text(a, src).to_string()),
            _ => None,
        }));
        if handed.0.is_empty() {
            Methods::of([knowledge::DEFAULT_METHOD])
        } else {
            handed
        }
    } else {
        return None;
    };
    Some(match call.and_then(|c| handed(c, 0, web.path, src)) {
        None => Route { methods, path: Some("/".into()) },
        Some(p) if p.kind() == "string" && walk(p, &["interpolation"]).is_empty() => Route { methods, path: Some(unquote(text(p, src))) },
        Some(_) => Route { methods, path: None },
    })
}

/// What a package exports is a route into it: the names in `__all__` of its `__init__.py` (or
/// of its one module), else (at its root) the public names that module imports from the
/// project's packages or defines. A public subpackage that declares `__all__` is a door too
/// (`from pkg.components import Serial`), labelled by its path from its package's root. Every
/// package of the project's own offers its doors; a name a root exports keeps the root's label,
/// and the package the project is named after labels first.
pub(super) fn mark_exports(modules: &IndexMap<String, Module>, store: &mut GraphBuilder, own: &Own) {
    let public = |id: &str| id.split('.').all(|p| !p.starts_with('_'));
    let mut packages: Vec<(&Module, usize, &str)> = modules
        .values()
        .filter(|m| m.package == m.id)
        .filter_map(|m| Some((m, own.rank(&m.id)?, own.package(&m.id)?)))
        .filter(|(m, _, root)| m.id == *root || public(&m.id))
        .collect();
    packages.sort_by_key(|(m, rank, root)| (m.id != *root, *rank, m.id.clone()));
    for (m, _, root) in packages {
        let names: Vec<String> = match declared_all(m) {
            Some(d) => d,
            None if m.id == root => m.symbols.keys().chain(m.bindings.keys()).filter(|n| !n.starts_with('_')).cloned().collect(),
            None => continue,
        };
        let path = m.id.strip_prefix(root).unwrap_or("").trim_start_matches('.');
        for name in names {
            let Some(t) = m.symbols.get(&name).or_else(|| m.bindings.get(&name)).filter(|t| own.holds(t)) else { continue };
            let Some(t) = defined(t, modules, &store.nodes) else { continue };
            store.export(&t, if path.is_empty() { name.clone() } else { format!("{path}.{name}") });
        }
    }
}

/// A module-level type alias: `Result = Union[FitResult, FailedFit]`, `Id = int | str`,
/// `T = TypeVar("T", bound="Base")`.
struct Alias {
    module: String,
    names: Vec<Vec<String>>,
    /// A union, `Optional` among them: the value is one of its names, a sum when they are two
    /// or more. A `TypeVar` stands for them.
    sum: bool,
    span: Option<(u32, u32)>,
    text: String,
}

/// Module-level type aliases by full name.
fn type_aliases(modules: &IndexMap<String, Module>) -> HashMap<String, Alias> {
    let mut out = HashMap::new();
    for (mid, m) in modules {
        // `type X = A | B`: a named union like any other
        for st in named_children(m.tree.root_node()).into_iter().filter(|st| st.kind() == "type_alias_statement") {
            let (Some(left), Some(right)) = (st.child_by_field_name("left"), st.child_by_field_name("right")) else { continue };
            let name = text(left, &m.src).split('[').next().unwrap_or("").trim().to_string();
            let value = named_children(right).into_iter().next().unwrap_or(right);
            let head = dotted(value.child_by_field_name("value"), &m.src).last().cloned().unwrap_or_default();
            let sum = text(value, &m.src).contains('|') || matches!(head.as_str(), "Union" | "Optional");
            if name.is_empty() || !sum {
                continue;
            }
            let alias = Alias {
                module: mid.clone(),
                names: type_names(Some(value), &m.src),
                sum,
                span: span(st),
                text: text(value, &m.src).to_string(),
            };
            out.insert(format!("{mid}.{name}"), alias);
        }
        for a in top_assignments(m) {
            let (Some(left), Some(right)) = (a.child_by_field_name("left"), a.child_by_field_name("right")) else { continue };
            if left.kind() != "identifier" {
                continue;
            }
            let head = dotted(right.child_by_field_name("value").or_else(|| right.child_by_field_name("function")), &m.src)
                .last()
                .cloned()
                .unwrap_or_default();
            let (names, sum) = match right.kind() {
                "subscript" if matches!(head.as_str(), "Union" | "Optional") => (type_names(Some(right), &m.src), true),
                "binary_operator" if text(right, &m.src).contains('|') => (type_names(Some(right), &m.src), true),
                // `T = TypeVar("T", bound="TransformBase")` stands for its bound, `TypeVar("T", A, B)` for its constraints
                "call" if head == "TypeVar" => (typevar_names(right, &m.src), false),
                _ => continue,
            };
            let alias = Alias { module: mid.clone(), names, sum, span: span(a), text: text(right, &m.src).to_string() };
            out.insert(format!("{mid}.{}", text(left, &m.src)), alias);
        }
    }
    out
}

/// A union of two or more known types is a type of its own, a sum: a node with a `variant`
/// edge to each type it may be. Signatures that name the alias take or return the sum.
fn add_sums(modules: &IndexMap<String, Module>, store: &mut GraphBuilder, aliases: &HashMap<String, Alias>, own: &Own) {
    let mut sums: Vec<(&String, &Alias)> = aliases.iter().filter(|(_, a)| a.sum).collect();
    sums.sort_by_key(|(id, _)| id.as_str());
    for (id, a) in sums {
        let m = &modules[&a.module];
        let mut variants: Vec<String> = vec![];
        for r in a.names.iter().filter_map(|n| resolve(m, n, modules, &store.nodes, own)) {
            if (!r.starts_with("ext:") || foreign_type(&r, &OWN)) && !variants.contains(&r) {
                variants.push(r);
            }
        }
        if variants.len() < 2 {
            continue;
        }
        store.add_node(Node {
            id: id.clone(),
            kind: NodeKind::Class,
            name: id.rsplit('.').next().unwrap_or(id).to_string(),
            file: crate::tree::path_str(&m.path),
            span: a.span,
            module: a.module.clone(),
            doc: a.text.clone(),
            ..Default::default()
        });
        store.add_edge(&a.module, id, EdgeKind::Contains, None);
        for v in variants {
            if v.starts_with("ext:") {
                store.ext_node(&v);
            }
            store.add_edge(id, &v, EdgeKind::Variant, None);
        }
    }
}

/// What a `TypeVar` ranges over: its bound, else its constraints; a bound may be a string.
fn typevar_names(call: TsNode, src: &[u8]) -> Vec<Vec<String>> {
    let Some(args) = call.child_by_field_name("arguments") else { return vec![] };
    let mut out = vec![];
    for (i, a) in named_children(args).into_iter().enumerate() {
        let value = match a.kind() {
            "keyword_argument" if a.child_by_field_name("name").map(|n| text(n, src)) == Some("bound") => a.child_by_field_name("value"),
            "keyword_argument" => None,
            _ if i > 0 => Some(a),
            _ => None,
        };
        let Some(v) = value else { continue };
        if v.kind() == "string" {
            let lit = text(v, src).trim_matches(|c| c == '"' || c == '\'');
            out.push(lit.split('.').map(str::to_string).collect());
        } else {
            out.extend(type_names(Some(v), src));
        }
    }
    out
}

/// What a type written in a signature or a field names: the definition it resolves to, else what
/// the alias of that name stands for.
fn type_targets(
    m: &Module,
    t: &[String],
    modules: &IndexMap<String, Module>,
    aliases: &HashMap<String, Alias>,
    nodes: &IndexMap<String, Node>,
    own: &Own,
) -> Vec<String> {
    match resolve(m, t, modules, nodes, own) {
        Some(r) => vec![r],
        None => alias_targets(m, t, modules, aliases, nodes, own),
    }
}

/// What an alias stands for, resolved in the module that defines it: the sum it names, else
/// the classes it ranges over.
fn alias_targets(
    m: &Module,
    t: &[String],
    modules: &IndexMap<String, Module>,
    aliases: &HashMap<String, Alias>,
    nodes: &IndexMap<String, Node>,
    own: &Own,
) -> Vec<String> {
    let [name] = t else { return vec![] };
    let full = m.bindings.get(name).cloned().unwrap_or_else(|| format!("{}.{name}", m.id));
    if nodes.contains_key(&full) {
        return vec![full];
    }
    let Some(a) = aliases.get(&full) else { return vec![] };
    a.names.iter().filter_map(|n| resolve(&modules[&a.module], n, modules, nodes, own)).collect()
}
