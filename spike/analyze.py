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
import re
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


# --- entry points: decorators that make a function a route into the program ---
DECO_RE = re.compile(r"^([\w.]+)\s*(?:\(\s*(?:['\"]([^'\"]*)['\"])?)?")
HTTP_METHODS = {"get", "post", "put", "delete", "patch", "websocket"}


def entry_of(decorators: list[str], name: str) -> dict | None:
    for d in decorators:
        m = DECO_RE.match(d)
        if not m:
            continue
        dn, arg = m.group(1), m.group(2)
        parts = dn.split(".")
        last = parts[-1]
        if last == "command":
            return {"kind": "cli", "label": arg or name.removeprefix("cmd_").replace("_", "-")}
        if last in HTTP_METHODS and len(parts) == 2:
            return {"kind": "route", "label": f"{last.upper()} {arg or ''}".strip()}
        if last in ("tool", "resource", "prompt") and "mcp" in dn.lower():
            return {"kind": "mcp", "label": arg or name}
    return None


# --- boundaries: calls that cross into the world (network, disk, databases, models) ---
NET_PKGS = {"requests", "httpx", "urllib", "http", "socket", "aiohttp"}
DB_PKGS = {"duckdb": "db_duckdb", "neo4j": "db_memgraph", "mgclient": "db_memgraph", "sqlite3": "db_sqlite",
           "psycopg2": "db_sql", "sqlalchemy": "db_sql"}
LLM_PKGS = {"openai", "anthropic"}
MODEL_PKGS = {"sentence_transformers", "torch", "transformers", "huggingface_hub", "datasets", "torchvision"}
FILE_PKGS = {"json", "yaml", "pickle", "csv", "tarfile", "zipfile", "shutil", "os", "pathlib", "pandas", "geopandas",
             "numpy", "rasterio", "lxml", "PIL", "matplotlib", "plt", "ruamel", "np", "pd"}
READ_BOUND = {"read_text", "read_bytes", "read_csv", "read_parquet", "read_file", "read_json", "read_excel", "iterdir",
              "glob", "rglob", "load", "load_workbook", "parse", "fromfile", "imread", "open_dataset", "read", "readlines",
              "safe_load", "listdir", "walk", "open"}
WRITE_BOUND = {"write_text", "write_bytes", "to_csv", "to_parquet", "to_json", "dump", "safe_dump", "savefig", "save",
               "mkdir", "copy", "copyfile", "copytree", "move", "unlink", "rmtree", "touch", "imwrite", "extractall",
               "makedirs", "rename", "replace", "writelines"}
READ_UNBOUND = {"read_text", "read_bytes", "iterdir", "glob", "rglob", "read", "readlines", "read_csv", "read_parquet",
                "read_file"}
WRITE_UNBOUND = {"write_text", "write_bytes", "mkdir", "unlink", "touch", "to_csv", "to_parquet", "to_json", "savefig",
                 "writelines", "rmtree"}


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
        self.ext_pkgs: set[str] | None = None
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
        # scripts next to the package (repo root and scripts/) are routes into it
        root = self.project_root()
        if root is not None:
            extras = sorted(root.glob("*.py")) + (sorted((root / "scripts").glob("*.py")) if (root / "scripts").is_dir() else [])
            for p in extras:
                rel = p.relative_to(root)
                mid = ".".join(["scripts", rel.stem]) if rel.parts[0] == "scripts" else f"_root.{rel.stem}"
                src = p.read_bytes()
                m = Module(mid, p, False, src)
                self.modules[mid] = m
                self.files[str(p)] = src.decode(errors="replace")
                self.add_node(mid, "module", rel.stem, str(p), m.tree.root_node, doc=first_docline(m.tree.root_node),
                              module=mid, is_pkg=False, entry={"kind": "script", "label": str(rel)})
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
                                      entry=entry_of(mdecs, mname), _sig=sig)
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
                              params=sig["params"], returns=sig["returns"],
                              entry=entry_of(decs, name), _sig=sig)
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
                tag = self.boundary(m, parts)
                if tag:
                    b = n.setdefault("boundaries", {})
                    b[tag] = b.get(tag, 0) + 1
                continue
            kind = "instantiates" if self.nodes[target]["kind"] == "class" else "calls"
            self.add_edge(n["id"], target, kind)

    def boundary(self, m: Module, parts: list[str]) -> str | None:
        """Classify a call that leaves the package: which part of the world does it touch?"""
        if m.ext_pkgs is None:
            m.ext_pkgs = {v.split(".")[0] for v in m.bindings.values() if not v.startswith(self.root)}
        head, last = parts[0], parts[-1]
        b = m.bindings.get(head)
        pkg = b.split(".")[0] if b and not b.startswith(self.root) else None
        if pkg in NET_PKGS:
            return "net"
        if pkg in DB_PKGS:
            return DB_PKGS[pkg]
        if pkg in LLM_PKGS:
            return "llm"
        if pkg in MODEL_PKGS:
            return "model"
        if pkg == "subprocess":
            return "process"
        if pkg in FILE_PKGS:
            if last in WRITE_BOUND:
                return "disk_write"
            if last in READ_BOUND:
                return "disk_read"
            return None
        if pkg is not None:
            return None
        if len(parts) == 1 and head == "open":
            return "disk"
        ext = m.ext_pkgs
        if last in READ_UNBOUND:
            return "disk_read"
        if last in WRITE_UNBOUND:
            return "disk_write"
        if last in ("run", "execute_query") and ext & {"neo4j", "mgclient"}:
            return "db_memgraph"
        if last in ("execute", "executemany", "sql", "register") and "duckdb" in ext:
            return "db_duckdb"
        if last in ("execute", "executemany") and "sqlite3" in ext:
            return "db_sqlite"
        if last == "create" and ext & LLM_PKGS:
            return "llm"
        if last in ("encode", "embed") and "sentence_transformers" in ext:
            return "model"
        return None

    def project_root(self) -> Path | None:
        for parent in [self.pkg_dir, *self.pkg_dir.parents][:4]:
            if (parent / "pyproject.toml").exists():
                return parent
        return None

    def mark_scripts(self):
        """`[project.scripts]` in the nearest pyproject.toml are entry points too."""
        import tomllib
        self.scripts: dict[str, str] = {}
        root = self.project_root()
        if root is None:
            return
        try:
            scripts = tomllib.loads((root / "pyproject.toml").read_text()).get("project", {}).get("scripts", {})
        except tomllib.TOMLDecodeError:
            return
        for name, target in scripts.items():
            mod, _, fn = target.partition(":")
            nid = f"{mod}.{fn}" if fn else mod
            if nid not in self.nodes and fn and mod in self.modules:
                nid = self.modules[mod].bindings.get(fn, nid)  # re-exported main
            n = self.nodes.get(nid)
            if n is not None and n["kind"] in ("function", "method"):
                if not n.get("entry"):
                    n["entry"] = {"kind": "cli", "label": name}
                self.scripts[name] = nid

    def read_makefile(self) -> list[dict]:
        """Makefile targets are the routes a developer actually uses: each one becomes a `make:` entry
        whose steps are the CLI commands and other targets its recipe invokes."""
        root = self.project_root()
        mk = (root / "Makefile") if root else None
        if mk is None or not mk.exists():
            return []
        targets: list[dict] = []
        cur = None
        for line in mk.read_text().splitlines():
            m = re.match(r"^([A-Za-z0-9_-]+):(?!=)\s*([^#]*)(?:##\s*(.*))?$", line)
            if m and not line.startswith("\t"):
                cur = {"id": f"make:{m.group(1)}", "label": m.group(1), "doc": (m.group(3) or "").strip(),
                       "deps": m.group(2).split(), "recipe": [], "calls": []}
                targets.append(cur)
            elif line.startswith("\t") and cur is not None:
                cur["recipe"].append(line[1:])
            elif line.strip() == "" or not line.startswith("\t"):
                cur = None if not m else cur
        by_label = {t["label"]: t["id"] for t in targets}
        cli_entries = {n["entry"]["label"]: n["id"] for n in self.nodes.values() if n.get("entry") and n["entry"]["kind"] == "cli"}
        script_names = "|".join(re.escape(s) for s in self.scripts) or None
        for t in targets:
            seen: list[str] = []
            for dep in t["deps"]:
                if dep in by_label and by_label[dep] not in seen:
                    seen.append(by_label[dep])
            for line in t["recipe"]:
                if re.match(r"\s*@?echo\b", line):
                    continue
                if script_names:
                    for mm in re.finditer(rf"\b(?:{script_names})\s+([a-z0-9][a-z0-9-]*)", line):
                        eid = cli_entries.get(mm.group(1))
                        if eid and eid not in seen:
                            seen.append(eid)
                for mm in re.finditer(r"(?:\$\(MAKE\)|\bmake)\s+([A-Za-z0-9_-]+)", line):
                    mid = by_label.get(mm.group(1))
                    if mid and mid not in seen:
                        seen.append(mid)
            t["calls"] = seen
        return [t for t in targets if t["recipe"] or t["calls"]]

    def infer_roles(self):
        """No roles file: one role per immediate subpackage, root modules together, columns ordered
        so that providers stand left of their consumers, a pure provider goes to the foundation band."""
        pkg_mods = [m for m in self.modules.values() if m.id == self.root or m.id.startswith(self.root + ".")]
        subs = sorted({m.id.split(".")[1] for m in pkg_mods if len(m.id.split(".")) > 1 and m.id.split(".")[1] in
                       {x.id.split(".")[1] for x in pkg_mods if x.is_pkg and len(x.id.split(".")) == 2}})
        if not subs:  # a flat package: every module is its own stage
            subs = sorted({m.id.split(".")[1] for m in pkg_mods if len(m.id.split(".")) == 2})
        rules = [[f"{self.root}.{s}", s.capitalize()] for s in subs]
        rules += [[self.root, "Root"], ["scripts", "Scripts"], ["_root", "Scripts"]]
        self.roles = {"order": [], "bands": {"top": ["Root", "Scripts"], "bottom": []}, "rules": rules, "base_roles": {},
                      "dependencies": "against_flow", "allowed": [], "inferred": True}
        self.assign_roles()
        deps: Counter = Counter()
        for e in self.edges:
            if e["kind"] == "contains":
                continue
            a, b = self.nodes.get(e["src"]), self.nodes.get(e["dst"])
            if not a or not b or a["role"] == b["role"] or "External" in (a["role"], b["role"]):
                continue
            deps[(a["role"], b["role"])] += 1
        roles = [r for _, r in rules if r not in ("Root", "Scripts")]
        incoming = {r: sum(v for (x, y), v in deps.items() if y == r) for r in roles}
        outgoing = {r: sum(v for (x, y), v in deps.items() if x == r) for r in roles}
        foundation = [r for r in roles if outgoing[r] == 0 and incoming[r] > 0 and len(roles) > 2]
        order = sorted([r for r in roles if r not in foundation], key=lambda r: (outgoing[r] - incoming[r], r))
        self.roles["order"], self.roles["bands"]["bottom"] = order, foundation

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
            "make": self.read_makefile(),
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
    roles = json.loads(a.roles.read_text()) if a.roles else None
    an = Analyzer(a.pkg_dir.resolve(), roles or {"order": [], "rules": [], "base_roles": {}})
    an.load()
    an.link()
    an.mark_scripts()
    if roles is None:
        an.infer_roles()
    an.assign_roles()
    out = an.export()
    a.out.write_text(json.dumps(out, ensure_ascii=False))
    s = out["stats"]
    print(json.dumps({k: v for k, v in s.items() if k != "unresolved_calls_top"}, indent=1, default=dict))
    print("unresolved (top):", s["unresolved_calls_top"][:15])
    print("roles:", Counter(n["role"] for n in out["nodes"] if n["kind"] in ("class", "function")))


if __name__ == "__main__":
    main()
