#!/usr/bin/env python3
"""Spike: tree-sitter over a Python package -> typed symbol graph (JSON).

    uv run --python 3.13 --with "tree-sitter==0.25.2" --with "tree-sitter-python==0.25.0" \
        spike/analyze.py <package_dir> --roles roles.json -o out.json

Nodes: module | class | function | method | external
Edges: contains | imports | inherits | calls | instantiates | takes | returns | has
Roles come from `roles.json`: {"order": [...], "rules": [[module_prefix, role], ...],
"base_roles": {"pkg.mod.Class": role}} — a class whose base chain reaches a
`base_roles` root takes that role, otherwise its module's role.
"""
from __future__ import annotations

import argparse
import json
from collections import Counter
from pathlib import Path

import tree_sitter_python as tsp
from tree_sitter import Language, Node as TSNode, Parser

LANG = Language(tsp.language())
PARSER = Parser(LANG)

BUILTIN_TYPES = {
    "str", "int", "float", "bool", "bytes", "list", "dict", "set", "tuple", "None",
    "Any", "Optional", "Union", "Iterable", "Iterator", "Sequence", "Mapping",
    "Callable", "Path", "Literal", "TypeVar", "Self", "ClassVar", "type", "object",
    "frozenset", "Generator", "AsyncIterator", "Awaitable", "Annotated",
}


def txt(n: TSNode | None) -> str:
    return n.text.decode() if n is not None and n.text else ""


def first_docline(body: TSNode | None) -> str:
    if body is None or not body.named_children:
        return ""
    first = body.named_children[0]
    if first.type == "expression_statement" and first.named_children and first.named_children[0].type == "string":
        raw = txt(first.named_children[0]).strip("\"' \n")
        return raw.splitlines()[0].strip() if raw else ""
    return ""


def iter_defs(block: TSNode):
    """Direct child definitions of a block, with their decorators."""
    for ch in block.named_children:
        if ch.type in ("function_definition", "class_definition"):
            yield [], ch
        elif ch.type == "decorated_definition":
            decs = [txt(d).lstrip("@") for d in ch.named_children if d.type == "decorator"]
            d = ch.child_by_field_name("definition")
            if d is not None:
                yield decs, d


def walk(n: TSNode, types: set[str]):
    stack = [n]
    while stack:
        cur = stack.pop()
        if cur.type in types:
            yield cur
        stack.extend(reversed(cur.named_children))


def dotted(n: TSNode | None) -> list[str]:
    """identifier / attribute / dotted_name -> parts."""
    if n is None:
        return []
    if n.type == "identifier":
        return [txt(n)]
    if n.type == "dotted_name":
        return txt(n).split(".")
    if n.type == "attribute":
        base = dotted(n.child_by_field_name("object"))
        attr = n.child_by_field_name("attribute")
        return base + [txt(attr)] if base and attr is not None else []
    if n.type == "subscript":
        return dotted(n.child_by_field_name("value"))
    return []


def type_names(t: TSNode | None) -> list[list[str]]:
    """All dotted names mentioned in an annotation, e.g. list[Graph] -> [[Graph]]."""
    if t is None:
        return []
    out = []
    for n in walk(t, {"identifier", "attribute"}):
        p = n.parent
        if n.type == "identifier" and p is not None and p.type == "attribute":
            continue
        parts = dotted(n)
        if parts and parts[-1] not in BUILTIN_TYPES:
            out.append(parts)
    return out


class Module:
    def __init__(self, mid: str, path: Path, is_pkg: bool, src: bytes):
        self.id = mid
        self.path = path
        self.is_pkg = is_pkg
        self.src = src
        self.package = mid if is_pkg else mid.rsplit(".", 1)[0]
        self.bindings: dict[str, str] = {}   # local name -> fully qualified target
        self.symbols: dict[str, str] = {}    # top-level name -> node id
        self.tree = PARSER.parse(src)


class Analyzer:
    def __init__(self, pkg_dir: Path, roles: dict):
        self.pkg_dir = pkg_dir
        self.root = pkg_dir.name
        self.roles = roles
        self.modules: dict[str, Module] = {}
        self.nodes: dict[str, dict] = {}
        self.edges: list[dict] = []
        self.edge_set: set[tuple] = set()
        self.unresolved: Counter = Counter()
        self.files: dict[str, str] = {}

    # ---------- pass 1: modules & symbols ----------
    def load(self):
        for p in sorted(self.pkg_dir.rglob("*.py")):
            if any(part.startswith(".") or part == "__pycache__" for part in p.parts):
                continue
            rel = p.relative_to(self.pkg_dir.parent).with_suffix("")
            parts = list(rel.parts)
            is_pkg = parts[-1] == "__init__"
            if is_pkg:
                parts = parts[:-1]
            mid = ".".join(parts)
            src = p.read_bytes()
            m = Module(mid, p, is_pkg, src)
            self.modules[mid] = m
            self.files[str(p)] = src.decode(errors="replace")
            self.add_node(mid, "module", parts[-1], str(p), m.tree.root_node,
                          doc=first_docline(m.tree.root_node), module=mid, is_pkg=is_pkg)
        for m in self.modules.values():
            self.collect_symbols(m)
        for m in self.modules.values():
            self.collect_imports(m)

    def add_node(self, nid, kind, name, file, span: TSNode | None, **extra):
        d = {"id": nid, "kind": kind, "name": name, "file": file}
        if span is not None:
            d["span"] = [span.start_point.row + 1, span.end_point.row + 1]
        d.update(extra)
        self.nodes[nid] = d
        return d

    def add_edge(self, src, dst, kind, **extra):
        key = (src, dst, kind)
        if key in self.edge_set or src == dst:
            return
        self.edge_set.add(key)
        self.edges.append({"src": src, "dst": dst, "kind": kind, **extra})

    def signature(self, fn: TSNode) -> dict:
        params = []
        ptypes: list[list[str]] = []
        pn = fn.child_by_field_name("parameters")
        if pn is not None:
            for p in pn.named_children:
                name_node = p if p.type == "identifier" else p.child_by_field_name("name")
                if name_node is None and p.named_children:
                    name_node = p.named_children[0]
                pname = txt(name_node) if name_node is not None else txt(p)
                t = p.child_by_field_name("type")
                params.append(pname + (": " + txt(t) if t is not None else ""))
                ptypes.extend(type_names(t))
        rt = fn.child_by_field_name("return_type")
        return {"params": params, "returns": txt(rt), "_ptypes": ptypes, "_rtypes": type_names(rt)}

    def collect_symbols(self, m: Module):
        for decs, d in iter_defs(m.tree.root_node):
            name = txt(d.child_by_field_name("name"))
            nid = f"{m.id}.{name}"
            m.symbols[name] = nid
            body = d.child_by_field_name("body")
            if d.type == "class_definition":
                sup = d.child_by_field_name("superclasses")
                bases = [dotted(b) for b in sup.named_children] if sup is not None else []
                bases = [b for b in bases if b]
                node = self.add_node(nid, "class", name, str(m.path), d, module=m.id,
                                     doc=first_docline(body), decorators=decs,
                                     bases=[".".join(b) for b in bases], methods=[])
                node["_bases"] = bases
                node["_attr_types"] = {}
                node["_fields"] = []
                if body is not None:
                    for mdecs, md in iter_defs(body):
                        mname = txt(md.child_by_field_name("name"))
                        mnid = f"{nid}.{mname}"
                        sig = self.signature(md)
                        self.add_node(mnid, "method", mname, str(m.path), md, module=m.id,
                                      cls=nid, doc=first_docline(md.child_by_field_name("body")),
                                      decorators=mdecs, params=sig["params"], returns=sig["returns"],
                                      _sig=sig)
                        node["methods"].append(mname)
                        self.add_edge(nid, mnid, "contains")
                    for st in body.named_children:
                        if st.type == "expression_statement" and st.named_children:
                            a = st.named_children[0]
                            if a.type == "assignment":
                                left, t = a.child_by_field_name("left"), a.child_by_field_name("type")
                                if left is not None and left.type == "identifier" and t is not None:
                                    node["_fields"].append((txt(left), type_names(t)))
                self.add_edge(m.id, nid, "contains")
            else:
                sig = self.signature(d)
                self.add_node(nid, "function", name, str(m.path), d, module=m.id,
                              doc=first_docline(body), decorators=decs,
                              params=sig["params"], returns=sig["returns"], _sig=sig)
                self.add_edge(m.id, nid, "contains")

    def resolve_relative(self, m: Module, dots: int, name: str | None) -> str:
        base = m.package.split(".")
        base = base[: len(base) - (dots - 1)] if dots > 1 else base
        return ".".join(base + ([name] if name else []))

    def collect_imports(self, m: Module):
        for st in walk(m.tree.root_node, {"import_statement", "import_from_statement"}):
            if st.type == "import_statement":
                for n in st.children_by_field_name("name"):
                    if n.type == "aliased_import":
                        target = txt(n.child_by_field_name("name"))
                        m.bindings[txt(n.child_by_field_name("alias"))] = target
                    else:
                        target = txt(n)
                        m.bindings[target.split(".")[0]] = target.split(".")[0]
                        m.bindings[target] = target
                    self.import_edge(m, target)
            else:
                mn = st.child_by_field_name("module_name")
                if mn is None:
                    continue
                if mn.type == "relative_import":
                    raw = txt(mn)
                    dots = len(raw) - len(raw.lstrip("."))
                    rest = raw.lstrip(".") or None
                    modname = self.resolve_relative(m, dots, rest)
                else:
                    modname = txt(mn)
                names = st.children_by_field_name("name")
                if not names:  # wildcard
                    self.import_edge(m, modname)
                    continue
                for n in names:
                    if n.type == "aliased_import":
                        nm = txt(n.child_by_field_name("name"))
                        alias = txt(n.child_by_field_name("alias"))
                    else:
                        nm, alias = txt(n), txt(n)
                    target = f"{modname}.{nm}"
                    m.bindings[alias] = target
                    self.import_edge(m, target)

    def import_edge(self, m: Module, target: str):
        if not target.startswith(self.root):
            return
        if target in self.modules or target in self.nodes:
            self.add_edge(m.id, target, "imports")
        else:
            mod = target.rsplit(".", 1)[0]
            if mod in self.modules:
                self.add_edge(m.id, mod, "imports")

    # ---------- resolution ----------
    def resolve(self, m: Module, parts: list[str]) -> str | None:
        """Dotted name in module scope -> node id, 'ext:<name>' or None."""
        if not parts:
            return None
        head = parts[0]
        if head in m.symbols:
            target = m.symbols[head]
        elif head in m.bindings:
            target = m.bindings[head]
        else:
            return None
        for p in parts[1:]:
            target = f"{target}.{p}"
        if target in self.nodes:
            return target
        if target.startswith(self.root + "."):
            mod, _, nm = target.rpartition(".")
            mm = self.modules.get(mod)
            if mm is not None and nm in mm.bindings:
                t2 = mm.bindings[nm]
                return t2 if t2 in self.nodes else None
            return None
        return "ext:" + target

    def ext_node(self, eid: str):
        if eid not in self.nodes:
            full = eid[4:]
            self.nodes[eid] = {"id": eid, "kind": "external", "name": full.split(".")[-1], "file": "",
                               "module": full.rsplit(".", 1)[0] if "." in full else ""}

    def method_of(self, cls_id: str, name: str, depth=0) -> str | None:
        if depth > 8:
            return None
        cand = f"{cls_id}.{name}"
        if cand in self.nodes:
            return cand
        for b in self.nodes.get(cls_id, {}).get("_resolved_bases", []):
            if not b.startswith("ext:"):
                r = self.method_of(b, name, depth + 1)
                if r:
                    return r
        return None

    # ---------- pass 2: edges ----------
    def link(self):
        for n in list(self.nodes.values()):
            if n["kind"] != "class":
                continue
            m = self.modules[n["module"]]
            rb = []
            for b in n["_bases"]:
                r = self.resolve(m, b) or ("ext:" + ".".join(b))
                if r.startswith("ext:"):
                    self.ext_node(r)
                rb.append(r)
                self.add_edge(n["id"], r, "inherits")
            n["_resolved_bases"] = rb
            for fname, types in n["_fields"]:
                for t in types:
                    r = self.resolve(m, t)
                    if r and not r.startswith("ext:"):
                        n["_attr_types"][fname] = r
                        self.add_edge(n["id"], r, "has", via=fname)
        for n in list(self.nodes.values()):
            if n["kind"] == "method" and n["name"] == "__init__":
                self.infer_self_attrs(n)
        for n in list(self.nodes.values()):
            if n["kind"] in ("function", "method"):
                m = self.modules[n["module"]]
                sig = n.pop("_sig")
                for t in sig["_ptypes"]:
                    r = self.resolve(m, t)
                    if r and not r.startswith("ext:"):
                        self.add_edge(n["id"], r, "takes")
                for t in sig["_rtypes"]:
                    r = self.resolve(m, t)
                    if r and not r.startswith("ext:"):
                        self.add_edge(n["id"], r, "returns")
                self.link_calls(n)
            elif n["kind"] == "module":
                self.link_calls(n)

    def def_node(self, n: dict) -> TSNode | None:
        m = self.modules[n["module"]]
        if n["kind"] == "module":
            return m.tree.root_node
        s = n["span"]
        for d in walk(m.tree.root_node, {"function_definition", "class_definition"}):
            if d.start_point.row + 1 == s[0] and txt(d.child_by_field_name("name")) == n["name"]:
                return d
        return None

    def infer_self_attrs(self, init: dict):
        cls = self.nodes[init["cls"]]
        m = self.modules[init["module"]]
        d = self.def_node(init)
        if d is None:
            return
        ptypes: dict[str, str] = {}
        pn = d.child_by_field_name("parameters")
        if pn is not None:
            for p in pn.named_children:
                name_node = p.child_by_field_name("name")
                t = p.child_by_field_name("type")
                if name_node is not None and t is not None:
                    for tn in type_names(t):
                        r = self.resolve(m, tn)
                        if r and not r.startswith("ext:"):
                            ptypes[txt(name_node)] = r
        body = d.child_by_field_name("body")
        if body is None:
            return
        for a in walk(body, {"assignment"}):
            left = a.child_by_field_name("left")
            right = a.child_by_field_name("right")
            t = a.child_by_field_name("type")
            if left is None or left.type != "attribute":
                continue
            lp = dotted(left)
            if len(lp) != 2 or lp[0] != "self":
                continue
            target = None
            if t is not None:
                for tn in type_names(t):
                    r = self.resolve(m, tn)
                    if r and not r.startswith("ext:"):
                        target = r
            if target is None and right is not None:
                if right.type == "identifier" and txt(right) in ptypes:
                    target = ptypes[txt(right)]
                elif right.type == "call":
                    r = self.resolve(m, dotted(right.child_by_field_name("function")))
                    if r and self.nodes.get(r, {}).get("kind") == "class":
                        target = r
            if target:
                cls["_attr_types"][lp[1]] = target
                self.add_edge(cls["id"], target, "has", via=lp[1])

    def link_calls(self, n: dict):
        m = self.modules[n["module"]]
        d = self.def_node(n)
        if d is None:
            return
        body = d.child_by_field_name("body") if n["kind"] != "module" else d
        if body is None:
            return
        cls = self.nodes.get(n.get("cls", "")) if n["kind"] == "method" else None
        for c in walk(body, {"call"}):
            if n["kind"] == "module":
                p, inner = c.parent, False
                while p is not None and p is not body:
                    if p.type in ("function_definition", "class_definition"):
                        inner = True
                        break
                    p = p.parent
                if inner:
                    continue
            parts = dotted(c.child_by_field_name("function"))
            if not parts:
                continue
            target = None
            if parts[0] in ("self", "cls") and cls is not None:
                if len(parts) == 2:
                    target = self.method_of(cls["id"], parts[1])
                elif len(parts) >= 3:
                    t = cls["_attr_types"].get(parts[1])
                    if t and len(parts) == 3:
                        target = self.method_of(t, parts[2]) or t
            else:
                target = self.resolve(m, parts)
                if target is None and len(parts) > 1:
                    r = self.resolve(m, parts[:-1])
                    if r and self.nodes.get(r, {}).get("kind") == "class":
                        target = self.method_of(r, parts[-1]) or r
            if target is None or target.startswith("ext:"):
                self.unresolved[".".join(parts[:2])] += 1
                continue
            kind = "instantiates" if self.nodes[target]["kind"] == "class" else "calls"
            self.add_edge(n["id"], target, kind)

    # ---------- roles ----------
    def assign_roles(self):
        rules = self.roles.get("rules", [])
        base_roles = self.roles.get("base_roles", {})

        def module_role(mid: str) -> str:
            best = None
            for prefix, role in rules:
                if mid == prefix or mid.startswith(prefix + "."):
                    if best is None or len(prefix) > len(best[0]):
                        best = (prefix, role)
            return best[1] if best else "Other"

        def base_role(cid: str, depth=0) -> str | None:
            if cid in base_roles:
                return base_roles[cid]
            if depth > 8:
                return None
            for b in self.nodes.get(cid, {}).get("_resolved_bases", []):
                if b in base_roles:
                    return base_roles[b]
                if not b.startswith("ext:"):
                    r = base_role(b, depth + 1)
                    if r:
                        return r
            return None

        for n in self.nodes.values():
            if n["kind"] == "external":
                n["role"] = "External"
                continue
            n["role"], n["role_source"] = module_role(n["module"]), "package"
            if n["kind"] == "class":
                br = base_role(n["id"])
                if br:
                    n["role"], n["role_source"] = br, "base"
        for n in self.nodes.values():
            if n["kind"] == "method":
                c = self.nodes[n["cls"]]
                n["role"], n["role_source"] = c["role"], c["role_source"]

    def export(self) -> dict:
        nodes = [{k: v for k, v in n.items() if not k.startswith("_")} for n in self.nodes.values()]
        return {
            "root": self.root,
            "roles": self.roles,
            "nodes": nodes,
            "edges": self.edges,
            "files": self.files,
            "stats": {
                "modules": len(self.modules),
                "nodes": Counter(n["kind"] for n in self.nodes.values()),
                "edges": Counter(e["kind"] for e in self.edges),
                "unresolved_calls_top": self.unresolved.most_common(25),
                "unresolved_calls_total": sum(self.unresolved.values()),
            },
        }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("pkg_dir", type=Path)
    ap.add_argument("--roles", type=Path)
    ap.add_argument("-o", "--out", type=Path, default=Path("graph.json"))
    a = ap.parse_args()
    roles = json.loads(a.roles.read_text()) if a.roles else {"order": [], "rules": [], "base_roles": {}}
    an = Analyzer(a.pkg_dir.resolve(), roles)
    an.load()
    an.link()
    an.assign_roles()
    out = an.export()
    a.out.write_text(json.dumps(out, ensure_ascii=False))
    s = out["stats"]
    print(json.dumps({k: v for k, v in s.items() if k != "unresolved_calls_top"}, indent=1, default=dict))
    print("unresolved (top):", s["unresolved_calls_top"][:15])
    print("roles:", Counter(n["role"] for n in out["nodes"] if n["kind"] in ("class", "function")))


if __name__ == "__main__":
    main()
