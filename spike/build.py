#!/usr/bin/env python3
"""Inline the analysis JSON into the viewer template.

    python3 spike/build.py spike/rkg.graph.json spike/rkg.roles.json spike/rkg.html
"""
import json
import sys
from pathlib import Path

graph, roles, out = (Path(p) for p in sys.argv[1:4])
data = json.loads(graph.read_text())
data["roles"] = json.loads(roles.read_text())
template = Path(__file__).with_name("viewer.html").read_text()
payload = json.dumps(data, ensure_ascii=False).replace("</", "<\\/")
out.write_text(template.replace("/*__DATA__*/null", payload, 1))
print(out, f"{out.stat().st_size / 1e6:.1f} MB")
