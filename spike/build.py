#!/usr/bin/env python3
"""Inline one or more analysis JSONs into the viewer template.

    python3 spike/build.py spike/atlas.html spike/rkg.graph.json spike/vectormesh.graph.json ...

Each graph file becomes a project in the page's project switcher; a `<name>.roles.json` next to
a `<name>.graph.json` overrides the roles stored in the graph.
"""
import json
import sys
from pathlib import Path

out, *graphs = (Path(p) for p in sys.argv[1:])
projects = {}
for graph in graphs:
    data = json.loads(graph.read_text())
    roles = graph.with_name(graph.name.replace(".graph.json", ".roles.json"))
    if roles.exists():
        data["roles"] = json.loads(roles.read_text())
    projects[data["root"]] = data
template = Path(__file__).with_name("viewer.html").read_text()
payload = json.dumps({"projects": projects}, ensure_ascii=False).replace("</", "<\\/")
out.write_text(template.replace("/*__DATA__*/null", payload, 1))
print(out, f"{out.stat().st_size / 1e6:.1f} MB", ", ".join(projects))
