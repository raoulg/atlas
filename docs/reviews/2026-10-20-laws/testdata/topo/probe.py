# /// script
# requires-python = ">=3.11"
# dependencies = ["playwright"]
# ///
"""Render a mock with Atlas, open a sequence of views, and dump what the page drew.

    ATLAS=target/release/atlas uv run probe.py MOCK OUTDIR VIEWS_JSON [--shots]

VIEWS_JSON is a list of view specs as the page takes them in its address ({"route": .., "open": {..}}),
or of {"view": spec, "click": [box ids]} to click boxes after loading the view. For every view the
script writes the picture (boxes with their parent, lines, sides, the oracle's faults, dropped lines)
and a summary of the plan, and with --shots a PNG.
"""

import json
import os
import pathlib
import re
import subprocess
import sys
from urllib.parse import quote

ATLAS = os.environ.get("ATLAS", "atlas")

DUMP = """() => {
  const P = window.atlasPicture || {};
  const plan = window.atlasPlan;
  const keys = m => m ? [...m.keys()] : [];
  const fine = plan ? [...plan.fine.pills.values()].map(p => ({id: p.id, parent: p.parent, own: !!p.own, D: keys(p.D), C: keys(p.C), goes: [...(p.goes || new Map())].map(([k, s]) => [k, [...s]])})) : null;
  const ttl = [...document.querySelectorAll('#scene g[data-id]')].map(g => [g.dataset.layoutId || g.dataset.id, (g.querySelector('.title,.ftitle')?.textContent || '').trim()]);
  const lines = [...document.querySelectorAll('#scene path.sedge:not(.interior-port)')].map(p => ({a: p.dataset.a, b: p.dataset.b, cls: p.getAttribute('class'), w: p.getAttribute('stroke-width')}));
  const bundles = [...document.querySelectorAll('#scene text.bundle')].map(t => t.textContent);
  return {n: P.n, boxes: P.boxes, lines: P.lines, sides: P.sides, oracle: P.oracle, dropped: P.dropped, titles: ttl, drawn: lines, bundles, fine,
          fineCount: plan ? plan.fine.pills.size : null, fault: (document.getElementById('fault') && !document.getElementById('fault').hidden) ? document.getElementById('fault').textContent : null};
}"""


def main() -> None:
    from playwright.sync_api import sync_playwright

    mock, out = pathlib.Path(sys.argv[1]).resolve(), pathlib.Path(sys.argv[2]).resolve()
    views = json.loads(sys.argv[3])
    shots = "--shots" in sys.argv
    out.mkdir(parents=True, exist_ok=True)
    page = out / "page.html"
    subprocess.run([ATLAS, "view", str(mock), "--no-open", "--no-cache", "--out", str(page)], check=True, capture_output=True)
    html = page.read_text()
    project = re.search(r'"projects":\{"([^"]+)":\{', html).group(1)
    html = html.replace("<script>\nwindow.DATA", '<script>\ntry { localStorage.setItem("atlas.code", "0"); } catch {}\nwindow.DATA', 1)
    page.write_text(html)
    results = []
    with sync_playwright() as p:
        browser = p.chromium.launch(channel="chrome")
        ctx = browser.new_context(viewport={"width": 1500, "height": 900}, color_scheme="light", reduced_motion="reduce")
        ctx.add_init_script("window.ATLAS_ORACLE = true")
        tab = ctx.new_page()
        errors = []
        tab.on("pageerror", lambda e: errors.append(str(e).splitlines()[0]))
        for i, v in enumerate(views):
            spec, clicks = (v["view"], v.get("click", [])) if "view" in v else (v, [])
            tab.goto("about:blank")
            tab.goto(f"{page.as_uri()}#p={project}&v={quote(json.dumps(spec))}", wait_until="domcontentloaded")
            tab.wait_for_function("window.atlasPicture && (document.querySelectorAll('#scene g[data-id]').length > 0 || !document.getElementById('fault')?.hidden)", timeout=30000)
            tab.wait_for_timeout(300)
            for c in clicks:
                n0 = tab.evaluate("window.atlasPicture.n")
                ok = tab.evaluate("id => { const g = window.atlasBoxes.get(id); if (!g) return false; g.dispatchEvent(new MouseEvent('click', {bubbles: true})); return true; }", c)
                if not ok:
                    errors.append(f"no box {c}")
                    continue
                tab.wait_for_function(f"window.atlasPicture.n !== {n0}", timeout=10000)
                tab.wait_for_timeout(250)
            d = tab.evaluate(DUMP)
            d["view"] = spec
            d["clicks"] = clicks
            d["errors"] = list(errors)
            errors.clear()
            results.append(d)
            if shots:
                tab.screenshot(path=str(out / f"{i:02d}.png"))
        browser.close()
    (out / "dump.json").write_text(json.dumps(results, indent=1))
    for i, d in enumerate(results):
        print(f"== view {i}: {json.dumps(d['view'])} clicks={d['clicks']}")
        if d["fault"]:
            print("   FAULT", d["fault"][:300])
        print("   boxes:", [f"{b} in {p}" for b, p in (d["boxes"] or [])])
        print("   lines:", [f"{a}>{b}" for a, b in (d["lines"] or [])])
        print("   sides:", d["sides"])
        if d["bundles"]:
            print("   bundles:", d["bundles"])
        print("   oracle:", d["oracle"])
        if d["dropped"]:
            print("   dropped:", d["dropped"])
        if d["errors"]:
            print("   errors:", d["errors"])


if __name__ == "__main__":
    main()
