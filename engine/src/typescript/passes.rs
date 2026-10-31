//! The passes that read a package's modules into the graph: the symbols each declares, its
//! imports, which modules are programs and what they export, the routers, containers and wrapped
//! functions it makes, the routes it serves, and the types, signatures and bodies that link them.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use serde::de::IgnoredAny;
use serde::Deserialize;
use tree_sitter::Node as TsNode;

use super::body::{builds_container, chain_root, changes_in, container_type, link_calls, typed_package, Body, Changing, Linked, OWN};
use super::module::{
    declarations, defined, doc_of, function_in, function_of, is_property, member_parts, package_from_ext, package_of, prophecy_of, resolve,
    signature, top_variables, type_head, type_names, Bound, Imported, Module, EXTENSIONS, WRAPPERS,
};
use super::paths::{self, Eval};
use crate::builder::{foreign_type, ClassInfo, Declared, GraphBuilder};
use crate::knowledge;
use crate::model::{Dir, EdgeKind, Entry, EntryKind, Node, NodeKind, Route, Touch, World};
use crate::repo::Repo;
use crate::tree::{beneath, named_children, span, text, unquote, walk};
use path_clean::clean;

/// The file an import specifier names: relative to the importing file, or through a path alias.
fn resolve_spec(spec: &str, from: &Path, aliases: &[(String, PathBuf)], files: &HashMap<PathBuf, String>) -> Option<String> {
    let base = if spec.starts_with('.') {
        from.parent()?.join(spec)
    } else {
        let (alias, dir) = aliases.iter().find(|(a, _)| !a.is_empty() && spec.starts_with(a.as_str()))?;
        dir.join(&spec[alias.len()..])
    };
    let base = clean(&base);
    let stripped = base.with_extension("");
    let mut candidates = vec![base.clone()];
    for b in [&base, &stripped] {
        for ext in EXTENSIONS {
            candidates.push(PathBuf::from(format!("{}.{ext}", b.display())));
            candidates.push(b.join(format!("index.{ext}")));
        }
    }
    candidates.into_iter().find_map(|c| files.get(&c).cloned())
}

/// A name from a package as an id: `ol/Map.js` default -> `ext:ol.Map`, `react` useState ->
/// `ext:react.useState`.
fn ext_id(spec: &str, name: Option<&str>) -> String {
    let path = spec.strip_prefix("node:").unwrap_or(spec);
    let path = path.rsplit_once('.').filter(|(_, e)| EXTENSIONS.contains(e)).map(|(p, _)| p).unwrap_or(path);
    match name {
        Some(n) => format!("ext:{}.{n}", path.replace('/', ".")),
        None => {
            let (pkg, last) = path.rsplit_once('/').unwrap_or((path, path));
            format!("ext:{}.{last}", pkg.replace('/', "."))
        }
    }
}

/// The path of the file route a declaration makes: `createFileRoute("/map")({..})`.
fn route_path(value: TsNode, src: &[u8]) -> Option<String> {
    walk(value, &["call_expression"]).into_iter().find_map(|c| {
        let f = c.child_by_field_name("function")?;
        let maker = text(f, src);
        if knowledge::ROOT_ROUTES.contains(&maker) {
            return Some("__root".into());
        }
        knowledge::FILE_ROUTES.contains(&maker).then_some(())?;
        let lit = c.child_by_field_name("arguments").and_then(|a| named_children(a).into_iter().find(|x| x.kind() == "string"))?;
        Some(unquote(text(lit, src)))
    })
}

fn add_function(m: &mut Module, store: &mut GraphBuilder, name: &str, def: TsNode, f: TsNode, body: TsNode, cls: Option<&str>) -> String {
    add_declared(m, store, name, def, f, body, cls, Declared::Body)
}

/// A function, from one declaration of its name: what `GraphBuilder::declare` keeps of it, and,
/// when it is the declaration kept, the body it runs.
#[allow(clippy::too_many_arguments)]
fn add_declared(
    m: &mut Module,
    store: &mut GraphBuilder,
    name: &str,
    def: TsNode,
    f: TsNode,
    body: TsNode,
    cls: Option<&str>,
    declared: Declared,
) -> String {
    let src = &m.src;
    let id = match cls {
        Some(c) => format!("{c}.{name}"),
        None => format!("{}.{name}", m.id),
    };
    let params = f.child_by_field_name("parameters").or_else(|| f.child_by_field_name("parameter"));
    let (params, returns, sig) = signature(params, f.child_by_field_name("return_type"), src);
    let on_class = cls.is_some() && (0..def.child_count()).filter_map(|i| def.child(i)).any(|c| c.kind() == "static");
    // the constructor initializes what `new` makes; a static method that gives its class constructs
    let class_name = cls.map(|c| c.rsplit('.').next().unwrap_or(c));
    let gives_class = returns.as_ref().map_or(false, |r| r.own || Some(r.value.as_str()) == class_name);
    let kept = declared == Declared::Body || !store.nodes.contains_key(&id);
    store.declare(
        Node {
            id: id.clone(),
            kind: if cls.is_some() { NodeKind::Method } else { NodeKind::Function },
            on_class,
            ctor: cls.is_some() && (name == "constructor" || on_class && gives_class),
            // a member is private when it says so; a module's own name is, unless the module exports it
            private: cls.is_some()
                && (name.starts_with('#')
                    || named_children(def).iter().any(|c| c.kind() == "accessibility_modifier" && text(*c, src) != "public")),
            name: name.to_string(),
            file: crate::tree::path_str(&m.path),
            span: span(def),
            module: m.id.clone(),
            doc: doc_of(def, src),
            prophecy: prophecy_of(def, src),
            cls: cls.map(str::to_string),
            // a method an interface or an abstract class declares is its signature alone
            declared: matches!(f.kind(), "method_signature" | "abstract_method_signature"),
            params,
            returns,
            ..Default::default()
        },
        sig,
        declared,
    );
    if kept {
        m.bodies.insert(id.clone(), (body.start_byte(), body.end_byte()));
    }
    id
}

/// What a declaration of a function is among the declarations of its name in one scope: a
/// signature without a body, where the name is declared again, is an overload; a `set` accessor
/// writes the property its `get` reads.
fn declared_as(d: TsNode, overloaded: &HashSet<String>, src: &[u8]) -> Declared {
    let name = d.child_by_field_name("name").map_or("", |n| text(n, src));
    let signature = matches!(d.kind(), "function_signature" | "method_signature");
    if signature && overloaded.contains(name) {
        Declared::Overload
    } else if (0..d.child_count()).filter_map(|i| d.child(i)).any(|c| c.kind() == "set") {
        Declared::Writer
    } else {
        Declared::Body
    }
}

/// The names declared more than once among `decls`.
fn declared_again<'t>(decls: impl Iterator<Item = TsNode<'t>>, src: &[u8]) -> HashSet<String> {
    let mut seen = HashSet::new();
    decls.filter_map(|d| d.child_by_field_name("name")).map(|n| text(n, src).to_string()).filter(|n| !seen.insert(n.clone())).collect()
}

pub(super) fn collect_symbols(m: &mut Module, store: &mut GraphBuilder) {
    let tree = m.tree.clone();
    let src = m.src.clone();
    let mut route: Option<String> = None;
    let functions = |d: &TsNode| matches!(d.kind(), "function_declaration" | "function_signature");
    let overloaded = declared_again(declarations(tree.root_node()).into_iter().map(|(d, _, _)| d).filter(functions), &src);
    for (d, exported, default) in declarations(tree.root_node()) {
        let name = d.child_by_field_name("name").map(|n| text(n, &src).to_string()).unwrap_or_default();
        if exported && !name.is_empty() {
            m.exported.push(name.clone());
        }
        match d.kind() {
            "function_declaration" | "function_signature" | "generator_function_declaration" | "function_expression" | "function"
                if !name.is_empty() =>
            {
                let body = d.child_by_field_name("body").unwrap_or(d);
                let id = add_declared(m, store, &name, d, d, body, None, declared_as(d, &overloaded, &src));
                store.add_edge(&m.id.clone(), &id, EdgeKind::Contains, None);
                m.symbols.insert(name.clone(), id.clone());
                if default {
                    m.default = Some(Bound::Own(id));
                }
            }
            "class_declaration" | "abstract_class_declaration" | "class" if !name.is_empty() => {
                let id = format!("{}.{name}", m.id);
                let mut bases = vec![];
                let mut fields: Vec<(String, Vec<Vec<String>>)> = vec![];
                for h in walk(d, &["class_heritage"]).into_iter().take(1) {
                    for c in walk(h, &["extends_clause", "implements_clause"]) {
                        for v in named_children(c).into_iter().filter(|v| v.kind() != "type_arguments") {
                            let parts: Vec<String> =
                                text(v, &src).split('<').next().unwrap_or("").split('.').map(|s| s.trim().to_string()).collect();
                            if !parts.iter().any(String::is_empty) {
                                bases.push(parts);
                            }
                        }
                    }
                }
                let mut methods: Vec<String> = vec![];
                if let Some(body) = d.child_by_field_name("body") {
                    let members = |x: &TsNode| matches!(x.kind(), "method_definition" | "abstract_method_signature" | "method_signature");
                    let overloaded = declared_again(named_children(body).into_iter().filter(members), &src);
                    for member in named_children(body) {
                        match member.kind() {
                            "method_definition" | "abstract_method_signature" | "method_signature" => {
                                let mname = member.child_by_field_name("name").map(|n| text(n, &src).to_string()).unwrap_or_default();
                                let mbody = member.child_by_field_name("body").unwrap_or(member);
                                let declared = declared_as(member, &overloaded, &src);
                                let mid = add_declared(m, store, &mname, member, member, mbody, Some(&id), declared);
                                store.add_edge(&id, &mid, EdgeKind::Contains, None);
                                let own = mid.rsplit('.').next().unwrap_or("").to_string();
                                if !methods.contains(&own) {
                                    methods.push(own);
                                }
                                // `constructor(private bron: Bron)` declares a field
                                if mname == "constructor" {
                                    for p in member.child_by_field_name("parameters").map(named_children).unwrap_or_default() {
                                        if is_property(p) {
                                            if let Some(pat) = p.child_by_field_name("pattern") {
                                                fields.push((text(pat, &src).to_string(), type_names(p.child_by_field_name("type"), &src)));
                                            }
                                        }
                                    }
                                }
                            }
                            "public_field_definition" | "property_signature" => {
                                let fname = member.child_by_field_name("name").map(|n| text(n, &src).to_string()).unwrap_or_default();
                                // a field holding an arrow is a method in all but syntax
                                if let Some(f) = member.child_by_field_name("value").filter(|v| v.kind() == "arrow_function") {
                                    let mid = add_function(m, store, &fname, member, f, f, Some(&id));
                                    store.add_edge(&id, &mid, EdgeKind::Contains, None);
                                    methods.push(fname);
                                } else {
                                    fields.push((fname, type_names(member.child_by_field_name("type"), &src)));
                                }
                            }
                            _ => {}
                        }
                    }
                }
                add_type(m, store, &name, d, bases, fields, methods);
                if default {
                    m.default = Some(Bound::Own(id));
                }
            }
            "interface_declaration" => {
                let bases: Vec<Vec<String>> = walk(d, &["extends_type_clause"])
                    .into_iter()
                    .flat_map(|c| named_children(c))
                    .map(|v| text(v, &src).split('<').next().unwrap_or("").split('.').map(|s| s.trim().to_string()).collect())
                    .collect();
                let body = d.child_by_field_name("body");
                let fields = body.map(|b| fields_of(b, &src)).unwrap_or_default();
                let methods = body.map(|b| method_signatures(m, store, &name, b)).unwrap_or_default();
                add_type(m, store, &name, d, bases, fields, methods);
            }
            "type_alias_declaration" => {
                let Some(value) = d.child_by_field_name("value") else { continue };
                match value.kind() {
                    // a named shape is a type of its own
                    "object_type" => {
                        let fields = fields_of(value, &src);
                        let methods = method_signatures(m, store, &name, value);
                        add_type(m, store, &name, d, vec![], fields, methods);
                    }
                    kind => {
                        m.aliases.insert(name.clone(), (type_names(Some(value), &src), kind == "union_type"));
                    }
                }
            }
            "enum_declaration" => add_type(m, store, &name, d, vec![], vec![], vec![]),
            "lexical_declaration" | "variable_declaration" => {
                for v in named_children(d).into_iter().filter(|v| v.kind() == "variable_declarator") {
                    let (Some(n), Some(value)) = (v.child_by_field_name("name"), v.child_by_field_name("value")) else { continue };
                    if n.kind() != "identifier" {
                        continue;
                    }
                    if let Some(path) = route_path(value, &src) {
                        route = Some(path);
                        continue;
                    }
                    let vname = text(n, &src).to_string();
                    if exported {
                        m.exported.push(vname.clone());
                    }
                    // an object of functions is a namespace: `api.get(..)` calls its `get`; one with a
                    // declared type is a value of that type
                    if value.kind() == "object" && v.child_by_field_name("type").is_none() && namespace(m, store, &vname, value) {
                        continue;
                    }
                    // a router is a value its registrations hand routes to, not a function
                    if router_made(value, m, &src) {
                        m.routers.insert(vname);
                        continue;
                    }
                    let Some(f) = function_in(value) else {
                        // a container holds what its type holds, as a parameter's does: `Record<Mode, Plan>`, `readonly Plan[]`
                        // and `Map<string, Plan>` hold Plans, the key no value of them
                        let held = |t: TsNode| {
                            let written = text(t, &src).trim_start_matches("readonly ").trim();
                            let head = written.split(['<', '[', ' ', '|']).next().unwrap_or("");
                            match knowledge::TYPESCRIPT_CONTAINERS.contains(&head) || written.ends_with("[]") {
                                true => type_names(Some(t), &src).into_iter().next().unwrap_or_default(),
                                false => type_head(t, &src),
                            }
                        };
                        let declared = v.child_by_field_name("type").and_then(|t| named_children(t).into_iter().next()).map(held);
                        let built = (value.kind() == "new_expression")
                            .then(|| value.child_by_field_name("constructor").map(|c| member_parts(c, &src)))
                            .flatten();
                        if let Some(t) = declared.or(built).filter(|t| !t.is_empty()) {
                            m.values.insert(vname, t);
                        }
                        continue;
                    };
                    let id = add_function(m, store, &vname, d, f, value, None);
                    // an arrow handed to a call takes its parameters' types from the call
                    if f.id() != value.id() {
                        if let (Some(n), Some(call)) =
                            (store.nodes.get_mut(&id), f.parent().and_then(|a| a.parent()).filter(|c| c.kind() == "call_expression"))
                        {
                            n.typed_by = call.child_by_field_name("function").map(|x| text(x, &src).to_string()).unwrap_or_default();
                        }
                    }
                    store.add_edge(&m.id.clone(), &id, EdgeKind::Contains, None);
                    m.symbols.insert(vname, id);
                }
            }
            "identifier" if default => {
                m.default = Some(Bound::Own(format!("{}.{}", m.id, text(d, &src))));
            }
            _ => {}
        }
    }
    // a file route is a page the browser shows at its path: a route that serves no request a
    // program sends, so it is labelled by its path alone
    if let Some(path) = route {
        if let Some(n) = store.nodes.get_mut(&m.id) {
            n.entry = Some(Entry::new(EntryKind::Route, if path == "__root" { "/ (root)".into() } else { path }));
        }
    }
    for n in
        store.nodes.values_mut().filter(|n| n.module == m.id && n.cls.is_none() && matches!(n.kind, NodeKind::Function | NodeKind::Class))
    {
        let local = n.name.split('.').next().unwrap_or(&n.name);
        n.private = !m.exported.iter().any(|e| e == local);
    }
}

/// The functions of an object literal, each a function of the module named by its key
/// (`api.get`); false when the object holds none.
fn namespace(m: &mut Module, store: &mut GraphBuilder, name: &str, object: TsNode) -> bool {
    let src = m.src.clone();
    let mut any = false;
    for member in named_children(object) {
        let (key, f, body) = match member.kind() {
            "pair" => match (
                member.child_by_field_name("key"),
                member.child_by_field_name("value").filter(|v| matches!(v.kind(), "arrow_function" | "function_expression" | "function")),
            ) {
                (Some(k), Some(v)) => (k, v, v),
                _ => continue,
            },
            "method_definition" => match (member.child_by_field_name("name"), member.child_by_field_name("body")) {
                (Some(k), Some(b)) => (k, member, b),
                _ => continue,
            },
            _ => continue,
        };
        let key = if key.kind() == "string" { unquote(text(key, &src)) } else { text(key, &src).to_string() };
        let id = add_function(m, store, &format!("{name}.{key}"), member, f, body, None);
        store.add_edge(&m.id.clone(), &id, EdgeKind::Contains, None);
        any = true;
    }
    if any {
        m.symbols.insert(name.to_string(), format!("{}.{name}", m.id));
    }
    any
}

/// What an interface or a named object type promises its implementations do: each method
/// signature is a method of the type, with no body of its own.
fn method_signatures(m: &mut Module, store: &mut GraphBuilder, type_name: &str, body: TsNode) -> Vec<String> {
    let id = format!("{}.{type_name}", m.id);
    let mut out: Vec<String> = vec![];
    let sigs: Vec<TsNode> = named_children(body).into_iter().filter(|x| x.kind() == "method_signature").collect();
    let overloaded = declared_again(sigs.iter().copied(), &m.src);
    for sig in sigs {
        let name = sig.child_by_field_name("name").map(|n| text(n, &m.src).to_string()).unwrap_or_default();
        let declared = declared_as(sig, &overloaded, &m.src);
        let mid = add_declared(m, store, &name, sig, sig, sig, Some(&id), declared);
        store.add_edge(&id, &mid, EdgeKind::Contains, None);
        if !out.contains(&name) {
            out.push(name);
        }
    }
    out
}

fn fields_of(body: TsNode, src: &[u8]) -> Vec<(String, Vec<Vec<String>>)> {
    named_children(body)
        .into_iter()
        .filter(|p| p.kind() == "property_signature")
        .filter_map(|p| Some((text(p.child_by_field_name("name")?, src).to_string(), type_names(p.child_by_field_name("type"), src))))
        .collect()
}

fn add_type(
    m: &mut Module,
    store: &mut GraphBuilder,
    name: &str,
    d: TsNode,
    bases: Vec<Vec<String>>,
    fields: Vec<(String, Vec<Vec<String>>)>,
    methods: Vec<String>,
) {
    let id = format!("{}.{name}", m.id);
    store.classes.insert(id.clone(), ClassInfo { bases: bases.clone(), fields, ..Default::default() });
    store.add_node(Node {
        id: id.clone(),
        kind: NodeKind::Class,
        name: name.to_string(),
        file: crate::tree::path_str(&m.path),
        span: span(d),
        module: m.id.clone(),
        doc: doc_of(d, &m.src),
        prophecy: prophecy_of(d, &m.src),
        bases: bases.iter().map(|b| b.join(".")).collect(),
        methods,
        ..Default::default()
    });
    store.add_edge(&m.id.clone(), &id, EdgeKind::Contains, None);
    m.symbols.insert(name.to_string(), id);
}

pub(super) fn collect_imports(m: &mut Module, aliases: &[(String, PathBuf)], files: &HashMap<PathBuf, String>) {
    let src = m.src.clone();
    let tree = m.tree.clone();
    // `const { ApiProvider } = await import("./api-provider")`: names a body imports when it runs
    for v in walk(tree.root_node(), &["variable_declarator"]) {
        let (Some(pattern), Some(value)) = (v.child_by_field_name("name"), v.child_by_field_name("value")) else { continue };
        if pattern.kind() != "object_pattern" {
            continue;
        }
        let Some(spec) = walk(value, &["call_expression"]).into_iter().find_map(|c| {
            (c.child_by_field_name("function")?.kind() == "import").then_some(())?;
            let lit = named_children(c.child_by_field_name("arguments")?).into_iter().find(|a| a.kind() == "string")?;
            Some(unquote(text(lit, &src)))
        }) else {
            continue;
        };
        let own = resolve_spec(&spec, &m.path, aliases, files);
        for name in walk(pattern, &["shorthand_property_identifier_pattern"]) {
            let name = text(name, &src).to_string();
            let bound = match &own {
                Some(id) => Bound::Own(format!("{id}.{name}")),
                None => Bound::Ext(ext_id(&spec, Some(&name))),
            };
            if own.is_none() {
                m.imported.push(Imported { local: name.clone(), spec: spec.clone(), name: Some(name.clone()) });
            }
            m.dynamic.insert(name, bound);
        }
    }
    for st in named_children(tree.root_node()) {
        let Some(source) = st.child_by_field_name("source") else {
            // `export { a, b as c }` names what is already here
            if st.kind() == "export_statement" {
                for s in walk(st, &["export_specifier"]) {
                    let (Some(n), alias) = (s.child_by_field_name("name"), s.child_by_field_name("alias")) else { continue };
                    let target = format!("{}.{}", m.id, text(n, &src));
                    m.exported.push(text(alias.unwrap_or(n), &src).to_string());
                    if let Some(a) = alias {
                        m.bindings.insert(text(a, &src).to_string(), Bound::Own(target));
                    }
                }
            }
            continue;
        };
        let spec = unquote(text(source, &src));
        let own = resolve_spec(&spec, &m.path, aliases, files);
        if own.is_none() {
            m.ext_pkgs.insert(package_of(&spec));
        }
        let member = |name: Option<&str>| match &own {
            Some(id) => Bound::Own(match name {
                Some(n) => format!("{id}.{n}"),
                None => id.clone(),
            }),
            None => Bound::Ext(ext_id(&spec, name)),
        };
        let imported = |into: &mut Vec<Imported>, local: &str, name: Option<&str>| {
            if own.is_none() {
                into.push(Imported { local: local.to_string(), spec: spec.clone(), name: name.map(str::to_string) });
            }
        };
        match st.kind() {
            "import_statement" => {
                for clause in named_children(st).into_iter().filter(|c| c.kind() == "import_clause") {
                    for c in named_children(clause) {
                        match c.kind() {
                            "identifier" => {
                                let local = text(c, &src).to_string();
                                imported(&mut m.imported, &local, Some("default"));
                                m.bindings.insert(local, if own.is_some() { member(Some("default")) } else { member(None) });
                            }
                            "namespace_import" => {
                                if let Some(id) = named_children(c).into_iter().find(|x| x.kind() == "identifier") {
                                    imported(&mut m.imported, text(id, &src), None);
                                    m.bindings.insert(text(id, &src).to_string(), member(None));
                                }
                            }
                            "named_imports" => {
                                for s in named_children(c).into_iter().filter(|s| s.kind() == "import_specifier") {
                                    let Some(n) = s.child_by_field_name("name") else { continue };
                                    let local = s.child_by_field_name("alias").unwrap_or(n);
                                    imported(&mut m.imported, text(local, &src), Some(text(n, &src)));
                                    m.bindings.insert(text(local, &src).to_string(), member(Some(text(n, &src))));
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            "export_statement" => {
                let specs = walk(st, &["export_specifier"]);
                if specs.is_empty() {
                    // `export * from "./x"`
                    if let Some(id) = &own {
                        m.wildcards.push(id.clone());
                    }
                }
                for s in specs {
                    let Some(n) = s.child_by_field_name("name") else { continue };
                    let local = s.child_by_field_name("alias").unwrap_or(n);
                    imported(&mut m.imported, text(local, &src), Some(text(n, &src)));
                    m.exported.push(text(local, &src).to_string());
                    m.bindings.insert(text(local, &src).to_string(), member(Some(text(n, &src))));
                }
            }
            _ => {}
        }
    }
}

/// The module a path in `package.json` names, with or without its extension.
fn module_at(repo: &Path, path: &str, files: &HashMap<PathBuf, String>) -> Option<String> {
    let p = clean(&repo.join(path));
    files.get(&p).cloned().or_else(|| files.iter().find(|(f, _)| f.with_extension("") == p.with_extension("")).map(|(_, id)| id.clone()))
}

/// `package.json` `bin`, and each script that runs a file of the package (`tsx server/index.ts`):
/// a command, a CLI route into the module it runs. Scripts that run one module name it together.
pub(super) fn mark_bins(repo: &Repo, files: &HashMap<PathBuf, String>, store: &mut GraphBuilder) {
    let Some(text) = repo.text("package.json") else { return };
    let repo = repo.root();
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else { return };
    let bins: Vec<(String, String)> = match &json["bin"] {
        serde_json::Value::String(p) => vec![(json["name"].as_str().unwrap_or("bin").to_string(), p.clone())],
        serde_json::Value::Object(o) => o.iter().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string()))).collect(),
        _ => vec![],
    };
    for (name, path) in bins {
        if let Some(n) = module_at(repo, &path, files).and_then(|id| store.nodes.get_mut(&id)) {
            n.entry = Some(Entry::new(EntryKind::Cli, name));
        }
    }
    let mut scripts: IndexMap<String, Vec<String>> = IndexMap::new();
    for (name, cmd) in json["scripts"].as_object().into_iter().flatten() {
        let Some(cmd) = cmd.as_str() else { continue };
        if let Some(id) = cmd.split_whitespace().find_map(|w| module_at(repo, w.trim_matches(['"', '\'']), files)) {
            scripts.entry(id).or_default().push(name.clone());
        }
    }
    for (id, mut names) in scripts {
        names.sort_by_key(|n| (n.len(), n.clone()));
        if let Some(n) = store.nodes.get_mut(&id).filter(|n| n.entry.is_none()) {
            n.entry = Some(Entry::new(EntryKind::Cli, names.join(" · ")));
        }
    }
}

/// What a package exports is a route into it: the names its entry module exports, the module
/// `package.json` points to (`exports`, `module`, `main`) or else the index at its root. A
/// name re-exported is followed to where it is defined.
pub(super) fn mark_exports(repo: &Repo, files: &HashMap<PathBuf, String>, modules: &IndexMap<String, Module>, store: &mut GraphBuilder) {
    let text = repo.text("package.json").unwrap_or_default();
    let json = serde_json::from_str::<serde_json::Value>(&text).unwrap_or_default();
    let repo = repo.root();
    // what the package exports at its root, as an import of a library's is found
    let exported = Exports::of(&text).map(|e| e.at(".")).unwrap_or_default();
    let declared = exported.into_iter().chain(["module", "main"].iter().filter_map(|k| json[*k].as_str().map(str::to_string)));
    let root = files
        .values()
        .find(|id| modules.get(*id).map_or(false, |m| m.path.file_stem().and_then(|s| s.to_str()) == Some("index") && !id.contains('.')))
        .cloned();
    let Some(entry) = declared.into_iter().find_map(|p| module_at(repo, &p, files)).or(root) else { return };
    let mut names: Vec<(String, String)> = vec![];
    let mut stack = vec![(entry.clone(), 0)];
    while let Some((mid, depth)) = stack.pop() {
        let Some(m) = modules.get(&mid) else { continue };
        names.extend(m.exported.iter().map(|n| (n.clone(), format!("{mid}.{n}"))));
        if depth < 8 {
            stack.extend(m.wildcards.iter().map(|w| (w.clone(), depth + 1)));
        }
    }
    for (name, target) in names {
        if let Some(id) = defined(&target, modules, &store.nodes, 0) {
            store.export(&id, name);
        }
    }
}

/// Whether a value is made by a web framework's router: `new Hono()`, `express()`,
/// `express.Router()`, and the chain of registrations on it (`new Hono().get(..).post(..)`).
fn router_made(mut v: TsNode, m: &Module, src: &[u8]) -> bool {
    loop {
        v = beneath(v, WRAPPERS);
        match v.kind() {
            "call_expression" => match v.child_by_field_name("function") {
                Some(f)
                    if f.kind() == "member_expression"
                        && f.child_by_field_name("object").map_or(false, |o| matches!(o.kind(), "call_expression" | "new_expression")) =>
                {
                    v = f.child_by_field_name("object").unwrap();
                }
                Some(f) => return router_pkg(&member_parts(f, src), m),
                None => return false,
            },
            "new_expression" => return v.child_by_field_name("constructor").map_or(false, |c| router_pkg(&member_parts(c, src), m)),
            _ => return false,
        }
    }
}

/// Module-level names for an imported function wrapped to answer with a promise:
/// `const run = promisify(execFile)` is `execFile`.
pub(super) fn collect_wrapped(m: &mut Module) {
    let mut wrapped = vec![];
    for (d, _, _) in declarations(m.tree.root_node()) {
        for v in named_children(d).into_iter().filter(|v| v.kind() == "variable_declarator") {
            let (Some(n), Some(value)) = (v.child_by_field_name("name"), v.child_by_field_name("value")) else { continue };
            if let Some(b) = promisified(value, m, &m.src) {
                wrapped.push((text(n, &m.src).to_string(), b));
            }
        }
    }
    m.bindings.extend(wrapped);
}

/// What `util.promisify(f)` wraps, when `f` is bound.
fn promisified(value: TsNode, m: &Module, src: &[u8]) -> Option<Bound> {
    let f = value.child_by_field_name("function").filter(|_| value.kind() == "call_expression")?;
    let parts = member_parts(f, src);
    let pkg = match m.bindings.get(parts.first()?)? {
        Bound::Ext(e) => package_from_ext(e, &m.ext_pkgs),
        Bound::Own(_) => return None,
    };
    knowledge::PROMISIFIERS.contains(&(pkg.as_str(), parts.last()?.as_str())).then_some(())?;
    let arg = named_children(value.child_by_field_name("arguments")?).into_iter().next().filter(|a| a.kind() == "identifier")?;
    m.bindings.get(text(arg, src)).cloned()
}

/// The fields of a module's classes and shapes that hold one of the language's own containers, by
/// the class: declared as one (`rows: Row[]`, a constructor's `readonly seen: Set<string>`) or set
/// to a new one (`rows = []`).
pub(super) fn container_fields(m: &Module) -> HashMap<String, HashSet<String>> {
    let src = &m.src[..];
    let holds = |ty: Option<TsNode>, value: Option<TsNode>| match ty {
        Some(t) => container_type(text(t, src), m),
        None => value.map_or(false, |v| builds_container(v, m, src)),
    };
    let mut out: HashMap<String, HashSet<String>> = HashMap::new();
    for (d, _, _) in declarations(m.tree.root_node()) {
        let Some(name) = d.child_by_field_name("name").map(|n| text(n, src)) else { continue };
        let body = match d.kind() {
            "class_declaration" | "abstract_class_declaration" | "class" | "interface_declaration" => d.child_by_field_name("body"),
            "type_alias_declaration" => d.child_by_field_name("value").filter(|v| v.kind() == "object_type"),
            _ => None,
        };
        let Some(body) = body else { continue };
        let mut fields = HashSet::new();
        for member in named_children(body) {
            match member.kind() {
                "public_field_definition" | "property_signature" => {
                    if holds(member.child_by_field_name("type"), member.child_by_field_name("value")) {
                        fields.extend(member.child_by_field_name("name").map(|n| text(n, src).to_string()));
                    }
                }
                // `constructor(readonly rows: Row[])` declares a field, and `this.rows = []` in it sets one
                "method_definition" if member.child_by_field_name("name").map(|n| text(n, src)) == Some("constructor") => {
                    for p in member.child_by_field_name("parameters").map(named_children).unwrap_or_default() {
                        if is_property(p) && holds(p.child_by_field_name("type"), None) {
                            fields.extend(p.child_by_field_name("pattern").map(|n| text(n, src).to_string()));
                        }
                    }
                    for a in member.child_by_field_name("body").map(|b| walk(b, &["assignment_expression"])).unwrap_or_default() {
                        let (Some(l), Some(r)) = (a.child_by_field_name("left"), a.child_by_field_name("right")) else { continue };
                        if let ([this, field], true) = (member_parts(l, src).as_slice(), builds_container(r, m, src)) {
                            if this == "this" {
                                fields.insert(field.clone());
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        if !fields.is_empty() {
            out.insert(format!("{}.{name}", m.id), fields);
        }
    }
    out
}

fn router_pkg(parts: &[String], m: &Module) -> bool {
    match parts.first().and_then(|h| m.bindings.get(h)) {
        Some(Bound::Ext(e)) => knowledge::ROUTERS.contains(&package_from_ext(e, &m.ext_pkgs).as_str()),
        _ => false,
    }
}

/// The routes a module registers on its routers: `app.get("/x", (c) => ..)` is a handler at
/// `GET /x`, `app.all("/x", ..)` one at every method (`knowledge::methods_served`), and
/// `app.route("/api", sub)` (`app.use` in Express) mounts one router on another. An inline handler
/// is a function of its own, named by its registration (`ALL /x`).
pub(super) fn collect_routes(m: &mut Module, store: &mut GraphBuilder) {
    let tree = m.tree.clone();
    let src = m.src.clone();
    let lit = |n: TsNode| matches!(n.kind(), "string" | "template_string") && !walk(n, &["template_substitution"]).iter().any(|_| true);
    for c in walk(tree.root_node(), &["call_expression"]) {
        let Some(f) = c.child_by_field_name("function").filter(|f| f.kind() == "member_expression") else { continue };
        let (Some(obj), Some(prop)) = (f.child_by_field_name("object"), f.child_by_field_name("property")) else { continue };
        let verb = text(prop, &src).to_string();
        // the router: a module-level router by name, or the chain a declaration makes
        let router = match obj.kind() {
            "identifier" if m.routers.contains(text(obj, &src)) => text(obj, &src).to_string(),
            _ => {
                let mut up = c.parent();
                while let Some(p) =
                    up.filter(|p| matches!(p.kind(), "member_expression" | "call_expression") || WRAPPERS.contains(&p.kind()))
                {
                    up = p.parent();
                }
                let declared = up
                    .filter(|p| p.kind() == "variable_declarator")
                    .and_then(|d| d.child_by_field_name("name"))
                    .map(|n| text(n, &src).to_string());
                // or a chain of registrations on a named router: `app.get(..).post(..)`
                let chained = || {
                    let mut base = obj;
                    while base.kind() == "call_expression" {
                        base = base.child_by_field_name("function").and_then(|f| f.child_by_field_name("object"))?;
                    }
                    (base.kind() == "identifier").then(|| text(base, &src).to_string())
                };
                match declared.or_else(chained) {
                    Some(name) if m.routers.contains(&name) => name,
                    _ => continue,
                }
            }
        };
        let args = c.child_by_field_name("arguments").map(named_children).unwrap_or_default();
        let Some(path) = args.first().filter(|a| lit(**a)).map(|a| unquote(text(*a, &src))) else { continue };
        if let Some(mount) = knowledge::MOUNTS.iter().find(|mt| mt.verb == verb) {
            let sub = match mount.child {
                knowledge::Arg::At(i) => args.get(i).filter(|a| a.kind() == "identifier"),
                knowledge::Arg::Named(_) => None,
            };
            if let Some(sub) = sub.filter(|_| matches!(mount.prefix, knowledge::Arg::At(0))) {
                m.mounts.push((router.clone(), path, text(*sub, &src).to_string()));
            }
            continue;
        }
        if !knowledge::registers_route(&verb) {
            continue;
        }
        // the route as served, by the methods it serves; an inline handler is named by its registration as written
        let served = Route { methods: knowledge::methods_served(&verb), path: Some(path.clone()) };
        let written = format!("{} {path}", verb.to_uppercase());
        let Some(handler) = args.last().filter(|a| a.id() != args[0].id()) else { continue };
        let id = match handler.kind() {
            "arrow_function" | "function_expression" | "function" => {
                let body = handler.child_by_field_name("body").unwrap_or(*handler);
                let id = add_function(m, store, &format!("{router}.{}", written.replace('.', "_")), c, *handler, body, None);
                if let Some(n) = store.nodes.get_mut(&id) {
                    n.name = written;
                }
                store.add_edge(&m.id.clone(), &id, EdgeKind::Contains, None);
                id
            }
            "identifier" => match m.symbols.get(text(*handler, &src)) {
                Some(id) => id.clone(),
                None => continue,
            },
            _ => continue,
        };
        if let Some(n) = store.nodes.get_mut(&id).filter(|n| n.entry.is_none()) {
            n.entry = Some(Entry::route(served));
            m.routes.push((id, router.clone()));
        }
    }
}

/// A route's full path: the prefixes of the routers it is mounted under, out to the one nobody
/// mounts. `:id` is a segment, written `{id}` as in every other language.
pub(super) fn route_prefixes(modules: &IndexMap<String, Module>, store: &mut GraphBuilder) {
    // a router by where it is made: the module that makes it, followed through imports
    fn key_of(m: &Module, local: &str, modules: &IndexMap<String, Module>, depth: u32) -> Option<String> {
        if m.routers.contains(local) {
            return Some(format!("{}.{local}", m.id));
        }
        let bound = if local == "default" { m.default.clone() } else { m.bindings.get(local).cloned() };
        let Some(Bound::Own(target)) = bound else { return None };
        let (mid, name) = target.rsplit_once('.').filter(|_| depth < 8)?;
        key_of(modules.get(mid)?, name, modules, depth + 1)
    }
    let mut mounted: HashMap<String, (String, String, bool)> = HashMap::new();
    for m in modules.values() {
        for (parent, prefix, sub) in &m.mounts {
            if let (Some(p), Some(s)) = (key_of(m, parent, modules, 0), key_of(m, sub, modules, 0)) {
                mounted.insert(s, (p, prefix.clone(), false));
            }
        }
    }
    let routes: Vec<(String, String)> = modules
        .values()
        .flat_map(|m| m.routes.iter().map(move |(id, router)| (id.clone(), key_of(m, router, modules, 0).unwrap_or_default())))
        .collect();
    crate::entries::mount_routes(store, &routes, &HashMap::new(), &mounted, express_segment);
}

/// A part of a route's path as the graph writes it: Express's `:id` is `{id}`, its `*` the rest.
fn express_segment(s: &str) -> String {
    if let Some(p) = s.strip_prefix(':') {
        format!("{{{}}}", p.trim_end_matches('?'))
    } else if s == "*" {
        "{rest:path}".into()
    } else {
        s.to_string()
    }
}

/// Types, signatures and bodies into edges: the program's, and the functions of the library's code
/// read.
pub(super) fn link(modules: &IndexMap<String, Module>, store: &mut GraphBuilder, repo: &Path) {
    let signatures = crate::clock::stage("signatures");
    // a union of two or more known types is a sum; any other alias stands for what it names
    for m in modules.values() {
        let mut names: Vec<&String> = m.aliases.keys().collect();
        names.sort();
        for name in names {
            let (types, union) = &m.aliases[name];
            if !*union {
                continue;
            }
            let mut variants: Vec<String> = vec![];
            for r in types.iter().filter_map(|t| resolve(m, t, modules, &store.nodes)) {
                if (!r.starts_with("ext:") || foreign_type(&r, &OWN)) && !variants.contains(&r) {
                    variants.push(r);
                }
            }
            if variants.len() < 2 {
                continue;
            }
            let id = format!("{}.{name}", m.id);
            store.add_node(Node {
                id: id.clone(),
                kind: NodeKind::Class,
                name: name.clone(),
                file: crate::tree::path_str(&m.path),
                module: m.id.clone(),
                ..Default::default()
            });
            store.add_edge(&m.id, &id, EdgeKind::Contains, None);
            for v in variants {
                if v.starts_with("ext:") {
                    store.ext_node(&v);
                }
                store.add_edge(&id, &v, EdgeKind::Variant, None);
            }
        }
    }
    let type_of = |m: &Module, t: &[String], nodes: &IndexMap<String, Node>| -> Vec<String> {
        if let Some(r) = resolve(m, t, modules, nodes) {
            return vec![r];
        }
        // an alias declared here or imported: what it stands for
        let [name] = t else { return vec![] };
        let (home, alias) = match m.aliases.get(name) {
            Some(_) => (m, name.clone()),
            None => match m.bindings.get(name) {
                Some(Bound::Own(target)) => {
                    let Some((mid, n)) = target.rsplit_once('.') else { return vec![] };
                    match modules.get(mid) {
                        Some(h) => (h, n.to_string()),
                        None => return vec![],
                    }
                }
                _ => return vec![],
            },
        };
        let sum = format!("{}.{alias}", home.id);
        if nodes.contains_key(&sum) {
            return vec![sum];
        }
        home.aliases.get(&alias).map(|(ts, _)| ts.iter().filter_map(|x| resolve(home, x, modules, nodes)).collect()).unwrap_or_default()
    };
    let ids: Vec<String> = store.nodes.keys().cloned().collect();
    // bases and typed fields
    for id in &ids {
        let n = &store.nodes[id];
        if n.kind != NodeKind::Class {
            continue;
        }
        let Some(m) = modules.get(&n.module) else { continue };
        let info = store.classes.get(id).cloned().unwrap_or_default();
        let mut resolved = vec![];
        for b in &info.bases {
            if let Some(r) = resolve(m, b, modules, &store.nodes) {
                if r.starts_with("ext:") {
                    store.ext_node(&r);
                }
                store.add_edge(id, &r, EdgeKind::Inherits, None);
                resolved.push(r);
            }
        }
        store.classes.entry(id.clone()).or_default().resolved = resolved;
        for (fname, types) in &info.fields {
            for t in types {
                for r in type_of(m, t, &store.nodes).into_iter().filter(|r| !r.starts_with("ext:") || foreign_type(r, &OWN)) {
                    if r.starts_with("ext:") {
                        store.ext_node(&r);
                    } else {
                        store.classes.entry(id.clone()).or_default().attr_types.insert(fname.clone(), r.clone());
                    }
                    store.add_edge(id, &r, EdgeKind::Has, Some(fname.clone()));
                }
            }
        }
    }
    // signatures -> takes / returns
    for id in &ids {
        let n = store.nodes[id].clone();
        let Some(m) = modules.get(&n.module) else { continue };
        if matches!(n.kind, NodeKind::Function | NodeKind::Method) {
            let sig = store.sigs.get(id).cloned().unwrap_or_default();
            for (types, unions, kind) in [(&sig.ptypes, &sig.punions, EdgeKind::Takes), (&sig.rtypes, &sig.runions, EdgeKind::Returns)] {
                let mut targets: Vec<String> = types.iter().flat_map(|t| type_of(m, t, &store.nodes)).collect();
                // a union of two or more known types is a sum, written in place as by a name
                for u in unions {
                    let mut variants: Vec<String> = vec![];
                    for r in u.names.iter().flat_map(|t| type_of(m, t, &store.nodes)) {
                        if (!r.starts_with("ext:") || foreign_type(&r, &OWN)) && !variants.contains(&r) {
                            variants.push(r);
                        }
                    }
                    if variants.len() > 1 {
                        targets.push(store.written_sum(&variants, &u.written, &n));
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
            let held = crate::model::holds_resolved(&store.nodes[id], |h| resolve(m, &[h.to_string()], modules, &store.nodes));
            crate::model::set_holds(store.nodes.get_mut(id).expect("a node of the graph"), held);
        }
    }
    // the class a function hands on, for typing what a body holds
    let mut returned: HashMap<String, String> = HashMap::new();
    for e in store.edges.iter().filter(|e| e.kind == EdgeKind::Returns) {
        if store.nodes.get(&e.dst).map_or(false, |x| x.kind == NodeKind::Class) {
            returned.entry(e.src.clone()).or_insert_with(|| e.dst.clone());
        }
    }
    let bases = store
        .classes
        .iter()
        .map(|(id, k)| (id.clone(), k.resolved.iter().filter(|b| store.nodes.contains_key(*b)).cloned().collect()))
        .collect();
    let eval = Eval::new(modules, repo, bases);
    // of a library's code, the methods of the classes the program names, and of their bases
    let named = named_classes(modules, store);
    // what each function changes among what it is handed (the value a method runs on, its
    // parameters), directly or through a call of the program's that changes what it is handed: until
    // nothing changes, which ends, as the sets only grow and the functions are finite. A method that
    // changes its receiver is one that changes the value it is called on.
    let mut changing = Changing::new();
    loop {
        let mut found = vec![];
        for id in &ids {
            let n = &store.nodes[id];
            if n.library && !n.cls.as_ref().map_or(false, |c| named.contains(c)) {
                continue;
            }
            let Some(m) = modules.get(&n.module).filter(|_| matches!(n.kind, NodeKind::Function | NodeKind::Method)) else { continue };
            let Some(body) = m.bodies.get(id).and_then(|(s, e)| m.tree.root_node().descendant_for_byte_range(*s, *e)) else { continue };
            let now = Body::new(m, modules, store, n, body, &returned, &eval, &changing).changes_handed();
            if now.len() > changing.get(id).map_or(0, Vec::len) {
                found.push((id.clone(), now));
            }
        }
        if found.is_empty() {
            break;
        }
        for (id, now) in found {
            if now.iter().any(|(r, _)| r == "this") {
                store.mutating.insert(id.clone());
            }
            changing.insert(id, now);
        }
    }
    // bodies -> calls / instantiates / boundaries: the program's, then the functions of the
    // library's code read; a library's module runs its own code on import, by no call of the
    // program's
    let mut linked = Linked::default();
    let (own, library): (Vec<&String>, Vec<&String>) = ids
        .iter()
        .filter(|id| {
            let n = &store.nodes[*id];
            matches!(n.kind, NodeKind::Function | NodeKind::Method)
                || (n.kind == NodeKind::Module && !n.library && modules.contains_key(*id))
        })
        .partition(|id| !store.nodes[*id].library);
    drop(signatures);
    for id in own.into_iter().chain(library) {
        let Some(m) = store.nodes.get(id).and_then(|n| modules.get(&n.module)) else { continue };
        let _body = crate::clock::of("body", m.id.split('.').next().unwrap_or(&m.id), store.nodes[id].library);
        link_calls(m, modules, store, id, &returned, &eval, &changing, &mut linked);
        store.count_linked(id);
    }
    let callers = crate::clock::stage("callers");
    let handing: Vec<paths::Evaluated> = linked.args.iter().map(|arg| paths::Evaluated { arg, eval: &eval }).collect();
    let entry = |f: &str| store.nodes.get(f).map_or(false, |n| n.entry.is_some());
    let family = store.family();
    let completed = crate::paths::complete(linked.calls, &handing, &family, &|_, _| vec![], &entry);
    // a page's request without a host goes to the page's own origin
    crate::paths::place_calls(store, completed, true);
    drop(callers);
    let _globals = crate::clock::stage("globals");
    globals(modules, store, &returned, &eval, &changing);
}

/// The library's classes the program names (imports, builds or calls a method of), with the
/// classes of the library they inherit from.
fn named_classes(modules: &IndexMap<String, Module>, store: &GraphBuilder) -> HashSet<String> {
    let class = |id: &str| {
        store.nodes.get(id).filter(|n| n.library).and_then(|n| if n.kind == NodeKind::Class { Some(n.id.clone()) } else { n.cls.clone() })
    };
    let mut todo: Vec<String> = modules
        .values()
        .filter(|m| !store.nodes.get(&m.id).map_or(false, |n| n.library))
        .flat_map(|m| m.read.values())
        .filter_map(|d| class(d))
        .collect();
    let mut out = HashSet::new();
    while let Some(c) = todo.pop() {
        if out.insert(c.clone()) {
            todo.extend(
                store
                    .classes
                    .get(&c)
                    .map(|i| i.resolved.clone())
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|b| store.nodes.get(b).map_or(false, |n| n.library)),
            );
        }
    }
    out
}

/// One use of a module's value by a function: where the value is kept (`module.name`), the name
/// the function uses it by, and the line.
type Use = (String, String, u32);

/// The values a module keeps between calls, which its functions change: a name a module declares
/// at its top level that a function assigns anew (`current = config`, `count++`) or changes in
/// place (`LATEST[k] = v`, `seen.n = ..`, `delete seen[k]`, `queue.push(..)` and the language's
/// other methods that change their receiver), in its own module or where another imports it.
/// Every function that changes one writes it and every one that uses it reads it: a place,
/// `Global(module.name)`. A name no function changes is a constant of the code, and no place; the
/// methods of a value a package made are the package's, and change no value of the language.
fn globals(
    modules: &IndexMap<String, Module>,
    store: &mut GraphBuilder,
    returned: &HashMap<String, String>,
    eval: &Eval,
    changing: &Changing,
) {
    let tops: HashMap<&str, HashSet<String>> = modules.values().map(|m| (m.id.as_str(), top_names(m))).collect();
    let mutators = knowledge::mutators(knowledge::Registry::Npm);
    let mut uses: Vec<(String, Vec<Use>, Vec<Use>)> = vec![];
    for m in modules.values() {
        let (src, root) = (&m.src[..], m.tree.root_node());
        let kept = |n: &str| {
            if tops[m.id.as_str()].contains(n) {
                Some(format!("{}.{n}", m.id))
            } else {
                kept_in(modules, &tops, m.bindings.get(n)?, 0)
            }
        };
        for n in store.nodes.values().filter(|n| n.module == m.id && matches!(n.kind, NodeKind::Function | NodeKind::Method)) {
            let Some(&(s, e)) = m.bodies.get(&n.id) else { continue };
            let Some(body) = root.descendant_for_byte_range(s, e) else { continue };
            // a function defined in this one is a function of its own
            let inner: Vec<(usize, usize)> = m.bodies.values().copied().filter(|&(a, b)| s <= a && b <= e && (a, b) != (s, e)).collect();
            let mine = |x: &TsNode| !inner.iter().any(|&(a, b)| a <= x.start_byte() && x.end_byte() <= b);
            let b = Body::new(m, modules, store, n, body, returned, eval, changing);
            let bound = b.bound_in();
            let module_value = |x: &str| if bound.contains(x) { None } else { kept(x) };
            let line = |x: TsNode| x.start_position().row as u32 + 1;
            let mut writes: Vec<Use> = vec![];
            for a in walk(body, &["assignment_expression", "augmented_assignment_expression", "update_expression"]).into_iter().filter(mine)
            {
                if let Some(t) =
                    a.child_by_field_name("left").or_else(|| a.child_by_field_name("argument")).filter(|t| t.kind() == "identifier")
                {
                    note(&mut writes, module_value(text(t, src)), text(t, src), line(a));
                }
            }
            // a method of the language's own that changes its receiver, on a value no package made
            let language: &dyn Fn(TsNode) -> Vec<TsNode> = &|c| {
                let Some(f) = function_of(c).filter(|f| f.kind() == "member_expression") else { return vec![] };
                let changing = f.child_by_field_name("property").map_or(false, |p| mutators.contains(&text(p, src)));
                if !changing || b.callee(c).map_or(false, |t| b.own(&t)) {
                    return vec![];
                }
                let Some(o) = f.child_by_field_name("object") else { return vec![] };
                let made = chain_root(o, src).and_then(module_value).map_or(false, |k| package_value(modules, &k));
                if made {
                    vec![]
                } else {
                    vec![o]
                }
            };
            for (at, name) in changes_in(body, src, language).into_iter().filter(|(x, _)| mine(x)) {
                note(&mut writes, module_value(name), name, line(at));
            }
            let mut reads: Vec<Use> = vec![];
            for x in walk(body, &["identifier", "shorthand_property_identifier"]).into_iter().filter(mine) {
                // what an assignment puts in a name, not the value the name held
                let assigned = x.parent().map_or(false, |p| {
                    p.kind() == "assignment_expression" && p.child_by_field_name("left").map(|l| l.id()) == Some(x.id())
                });
                if !assigned {
                    note(&mut reads, module_value(text(x, src)), text(x, src), line(x));
                }
            }
            uses.push((n.id.clone(), writes, reads));
        }
    }
    let state: HashSet<String> = uses.iter().flat_map(|(_, w, _)| w.iter().map(|(k, _, _)| k.clone())).collect();
    for (id, writes, reads) in uses {
        for (dir, names) in [(Dir::Write, writes), (Dir::Read, reads)] {
            for (kept, name, at) in names.into_iter().filter(|(k, _, _)| state.contains(k)) {
                store.touch(&id, Touch { what: name.clone(), artifact: kept, op: name, line: at, ..Touch::new(World::Global, Some(dir)) });
            }
        }
    }
}

/// A use of a module's value, once per value.
fn note(uses: &mut Vec<Use>, kept: Option<String>, name: &str, line: u32) {
    if let Some(k) = kept.filter(|k| !uses.iter().any(|(u, _, _)| u == k)) {
        uses.push((k, name.to_string(), line));
    }
}

/// The values a module declares at its top level: `let current = ..`, `const LATEST = new Map()`.
fn top_names(m: &Module) -> HashSet<String> {
    top_variables(m.tree.root_node())
        .into_iter()
        .filter_map(|v| v.child_by_field_name("name").filter(|n| n.kind() == "identifier"))
        .map(|n| text(n, &m.src).to_string())
        .collect()
}

/// Where a name a module imports is kept: the top-level value of the module it comes from,
/// followed through re-exports.
fn kept_in(modules: &IndexMap<String, Module>, tops: &HashMap<&str, HashSet<String>>, bound: &Bound, depth: u32) -> Option<String> {
    let Bound::Own(target) = bound else { return None };
    let (mid, name) = target.rsplit_once('.')?;
    let home = modules.get(mid)?;
    if tops.get(mid).map_or(false, |t| t.contains(name)) {
        return Some(target.clone());
    }
    if depth > 8 {
        return None;
    }
    match if name == "default" { home.default.as_ref() } else { home.bindings.get(name) } {
        Some(b) => kept_in(modules, tops, b, depth + 1),
        None => home.wildcards.iter().find_map(|w| kept_in(modules, tops, &Bound::Own(format!("{w}.{name}")), depth + 1)),
    }
}

/// Whether the top-level value kept at `kept` is one a package made: built by a package's
/// constructor or function (`new Pool()`, `express()`, `new Hono().basePath(..)`), or declared
/// with a package's type (`let pool: Pool`).
fn package_value(modules: &IndexMap<String, Module>, kept: &str) -> bool {
    let Some((home, name)) = kept.rsplit_once('.').and_then(|(mid, name)| Some((modules.get(mid)?, name))) else { return false };
    let src = &home.src[..];
    let Some(v) =
        top_variables(home.tree.root_node()).into_iter().find(|v| v.child_by_field_name("name").map(|n| text(n, src)) == Some(name))
    else {
        return false;
    };
    if v.child_by_field_name("type").and_then(|t| typed_package(t, home, src)).is_some() {
        return true;
    }
    let package = |n: TsNode| matches!(member_parts(n, src).first().and_then(|h| home.bindings.get(h)), Some(Bound::Ext(_)));
    let mut value = v.child_by_field_name("value");
    while let Some(x) = value.map(|v| beneath(v, WRAPPERS)) {
        match x.kind() {
            "new_expression" => return x.child_by_field_name("constructor").map_or(false, package),
            // a chain of calls on what a package made: `new Hono().basePath("/api")`
            "call_expression" => match function_of(x) {
                Some(f)
                    if f.kind() == "member_expression"
                        && f.child_by_field_name("object").map_or(false, |o| matches!(o.kind(), "call_expression" | "new_expression")) =>
                {
                    value = f.child_by_field_name("object");
                }
                Some(f) => return package(f),
                None => return false,
            },
            _ => return false,
        }
    }
    false
}

/// A package's `exports`, each object's keys in the order the package writes them: Node matches
/// an import's conditions in that order.
#[derive(Deserialize, Debug, PartialEq)]
#[serde(untagged)]
pub(super) enum Exports {
    /// A path in the package.
    Path(String),
    /// Paths tried in turn.
    Fallbacks(Vec<Exports>),
    /// What each path below the package (`.`, `./sub`, `./*`) or each condition gives.
    Map(IndexMap<String, Exports>),
    /// Anything else: `null` exports nothing.
    Other(IgnoredAny),
}

impl Exports {
    /// The `exports` of a package's manifest, the text of its `package.json`.
    pub(super) fn of(manifest: &str) -> Option<Exports> {
        #[derive(Deserialize)]
        struct Manifest {
            exports: Option<Exports>,
        }
        serde_json::from_str::<Manifest>(manifest).ok()?.exports
    }

    /// The paths of code what the package exports at `key` (`.`, `./sub`) gives an import, in the
    /// order an import tries them; a pattern (`./*`) by what its `*` stands for.
    pub(super) fn at(&self, key: &str) -> Vec<String> {
        match self {
            Exports::Map(o) if o.keys().any(|k| k.starts_with('.')) => match o.get(key) {
                Some(v) => v.code(),
                None => o
                    .iter()
                    .find_map(|(k, v)| {
                        let (pre, post) = k.split_once('*')?;
                        let star = key.strip_prefix(pre)?.strip_suffix(post)?;
                        Some(v.code().into_iter().map(|t| t.replace('*', star)).collect::<Vec<_>>()).filter(|t| !t.is_empty())
                    })
                    .unwrap_or_default(),
            },
            _ if key == "." => self.code(),
            _ => vec![],
        }
    }

    /// The paths of code a value of `exports` gives an import, in the order Node tries them: an
    /// object's conditions in the package's order, those an import of code meets
    /// (knowledge::EXPORT_CONDITIONS), and a list's entries in turn. A file of types is none.
    fn code(&self) -> Vec<String> {
        match self {
            Exports::Path(p) if !types_only(p) => vec![p.clone()],
            Exports::Fallbacks(a) => a.iter().flat_map(Exports::code).collect(),
            Exports::Map(o) => {
                o.iter().filter(|(c, _)| knowledge::EXPORT_CONDITIONS.contains(&c.as_str())).flat_map(|(_, v)| v.code()).collect()
            }
            _ => vec![],
        }
    }
}

/// Whether a file holds only types: `.d.ts`, `.d.mts`, `.d.cts`.
fn types_only(p: &str) -> bool {
    [".d.ts", ".d.mts", ".d.cts"].iter().any(|x| p.ends_with(x))
}

#[cfg(test)]
mod tests {
    use super::Exports;

    fn root(exports: &str) -> Vec<String> {
        Exports::of(&format!("{{\"name\": \"kit\", \"exports\": {exports}}}")).expect("the package exports").at(".")
    }

    /// Node goes through an `exports` object's conditions in the order the package writes them and
    /// takes the first the import meets: the order is the package's, whichever conditions it names.
    #[test]
    fn an_import_takes_the_first_condition_the_package_writes() {
        let conditions = ["node", "import", "require", "module", "browser", "default"];
        for a in conditions {
            for b in conditions.iter().filter(|b| **b != a) {
                let written = format!("{{\"{a}\": \"./{a}.js\", \"{b}\": \"./{b}.js\"}}");
                assert_eq!(root(&written).first().map(String::as_str), Some(format!("./{a}.js").as_str()), "{written}");
            }
        }
    }

    /// `types` names declarations, never code: wherever the package writes it, and whatever file it
    /// names, an import of code takes what the other conditions give.
    #[test]
    fn types_is_no_condition_of_code() {
        for written in [
            r#"{"types": "./src/index.ts", "default": "./index.js"}"#,
            r#"{"default": "./index.js", "types": "./src/index.ts"}"#,
            r#"{".": {"types": "./index.d.ts", "default": "./index.js"}}"#,
        ] {
            assert_eq!(root(written), ["./index.js"], "{written}");
        }
    }

    /// The forms `exports` gives the package's root in are one: a path, the path at `.`, the path
    /// under the condition every import meets, and a list of one path.
    #[test]
    fn the_forms_of_a_root_export_are_one() {
        for written in [r#""./index.js""#, r#"{".": "./index.js"}"#, r#"{".": {"default": "./index.js"}}"#, r#"["./index.js"]"#] {
            assert_eq!(root(written), ["./index.js"], "{written}");
        }
        let patterns = Exports::of(r#"{"exports": {"./*": {"import": "./lib/*.js"}}}"#).expect("exports");
        assert_eq!(patterns.at("./read"), ["./lib/read.js"]);
    }
}
