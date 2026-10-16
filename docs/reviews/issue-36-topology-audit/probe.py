# /// script
# requires-python = ">=3.11"
# dependencies = ["playwright"]
# ///
"""Capture Atlas's semantic plan and rejected endpoints from a temporary HTML copy."""

import argparse
import json
from pathlib import Path

from playwright.sync_api import sync_playwright


CAPTURE = """window.auditSet = async v => { setView(v); await render(); };
window.AUDIT = {
  view: viewData(), root: structuredClone(root),
  places: [...places.values()], placeEntries: [...places],
  pills: [...pills.values()].map(p => ({...p, D: [...p.D], C: [...p.C],
    goes: [...p.goes].map(([k, v]) => [k, [...v]])})),
  frames: [...frames.values()],
  families: [...famFrames.values()],
  worldFrames: [...worldFrames.values()].map(f => ({...f, kids: [...f.kids]})),
  edges: [...edgeInfo.values()].map(e => ({...e, sides: e.sides ? [...e.sides] : null})),
  roles: [...roles], roleAt: [...roleAt], twins: [...twinOf],
  tree: [...treeInfo].map(([k, v]) => [k, {step: v.step, on: v.on, site: v.site}]),
  dropped: window.AUDIT_DROPPED || []
};
window.AUDIT_DROPPED = [];
if (window.AUDIT_FAST) return;
"""


def instrument(html: str) -> str:
    layout = "const layout = await new ELK().layout("
    guard = "if (!nodeOf.has(a) || !nodeOf.has(b) || a === b) return;"
    if html.count(layout) != 1 or html.count(guard) != 1:
        raise ValueError("Expected one layout and one endpoint guard; inspect this viewer's injection points")
    html = html.replace(layout, CAPTURE + layout)
    return html.replace(guard, """if (!nodeOf.has(a) || !nodeOf.has(b) || a === b) {
      (window.AUDIT_DROPPED ||= []).push({a, b, info,
        missingA: !nodeOf.has(a), missingB: !nodeOf.has(b)});
      return;
    }""")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("html", type=Path)
    parser.add_argument("states", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--graph", type=Path)
    args = parser.parse_args()
    if args.output.exists() and any(args.output.iterdir()):
        parser.error("Use an empty output directory")
    args.output.mkdir(parents=True, exist_ok=True)
    html = args.html.read_text()
    if args.graph:
        start = html.index("window.DATA = ") + len("window.DATA = ")
        _, length = json.JSONDecoder().raw_decode(html[start:])
        graph = json.loads(args.graph.read_text())
        data = {"atlas": "synthetic graph / exported viewer", "projects": {graph["root"]: graph}}
        html = html[:start] + json.dumps(data).replace("</", "<\\/") + html[start + length:]
    copy = args.output / "instrumented.html"
    copy.write_text(instrument(html))
    states = json.loads(args.states.read_text())
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(channel="chrome")
        try:
            page = browser.new_page(viewport={"width": 1600, "height": 1000}, reduced_motion="reduce")
            errors = []
            page.on("pageerror", lambda error: errors.append(str(error)))
            page.goto(copy.resolve().as_uri(), wait_until="networkidle")
            page.wait_for_function("window.AUDIT !== undefined", timeout=30000)
            page.evaluate("window.AUDIT_FAST = true")
            for state in states:
                name = state["name"]
                if not name or Path(name).name != name:
                    raise ValueError("State names must be plain filenames")
                result = page.evaluate("async v => { await window.auditSet(v); return window.AUDIT; }", state["view"])
                if result["view"] != state["view"]:
                    raise ValueError(f"Viewer normalized requested state {name}: {result['view']}")
                (args.output / f"{name}.json").write_text(json.dumps(result, indent=2))
                missing = sum(bool(e["missingA"] or e["missingB"]) for e in result["dropped"])
                print(f"{name}: {len(result['edges'])} edges; {missing} missing endpoints", flush=True)
            if errors:
                raise RuntimeError("; ".join(errors))
        finally:
            browser.close()


if __name__ == "__main__":
    main()
