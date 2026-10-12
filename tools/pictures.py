# /// script
# requires-python = ">=3.11"
# dependencies = ["playwright", "pillow"]
# ///
"""Draw the routes of a repository with an Atlas build, and compare two sets of drawings: a change
that should leave the pictures as they were can be shown to, pixel for pixel. Walk the views a
reader can click to, so that none of them draws nothing.

    uv run tools/pictures.py draw ATLAS REPO OUTDIR [ENTRY ...]
    uv run tools/pictures.py compare BEFORE AFTER
    uv run tools/pictures.py walk ATLAS REPO OUTDIR

`draw` writes the overview and each route (the entries named, or up to 40 of the graph's) as a
PNG, in the light theme, with the code panel closed, once the page has laid itself out. It uses
the Chrome installed on the machine. `compare` reports the drawings that differ below the
header, which names the build. `walk` opens, one view at a time, every package of every part
and every route, and each family in the first two routes that hold it. It reports each view that throws (the page
says it could not draw it) or draws nothing, and each family that opens into another number of moments than it stood in
closed; it fails when there is one.
"""

import json
import os
import pathlib
import re
import subprocess
import sys


def page_of(atlas: str, repo: str, out: pathlib.Path) -> tuple[pathlib.Path, str, re.Match, dict]:
    """The page an Atlas build draws of a repository: its file, its text, where its project is named, its graph."""
    out.mkdir(parents=True, exist_ok=True)
    page_file = out / "page.html"
    # Atlas runs outside this script's own environment, so that ty finds the project's
    env = {k: v for k, v in os.environ.items() if k not in ("VIRTUAL_ENV", "CONDA_PREFIX")}
    subprocess.run([atlas, "view", repo, "--no-open", "--out", str(page_file)], check=True, capture_output=True, env=env)
    html = page_file.read_text()
    project = re.search(r'"projects":\{"([^"]+)":\{', html)
    start = html.index("window.DATA = ") + len("window.DATA = ")
    data, _ = json.JSONDecoder().raw_decode(html[start:].replace("<\\/", "</"))
    return page_file, html, project, data["projects"][project.group(1)]


def draw(atlas: str, repo: str, out: pathlib.Path, entries: list[str]) -> None:
    from playwright.sync_api import sync_playwright

    _, html, project, graph = page_of(atlas, repo, out)
    entries = entries or [n["id"] for n in graph["nodes"] if n.get("entry") and n["entry"]["kind"] != "test"][:40]
    with sync_playwright() as p:
        browser = p.chromium.launch(channel="chrome")
        ctx = browser.new_context(viewport={"width": 1600, "height": 1000}, color_scheme="light", reduced_motion="reduce")
        for i, entry in enumerate([""] + entries):
            h = html.replace(project.group(0), project.group(0) + (f'"open":{json.dumps(entry)},' if entry else ""), 1)
            h = h.replace("<script>\nwindow.DATA", '<script>\ntry { localStorage.setItem("atlas.code", "0"); } catch {}\nwindow.DATA', 1)
            f = out / f"{i:02d}.html"
            f.write_text(h)
            page = ctx.new_page()
            page.goto(f.as_uri(), wait_until="networkidle")
            page.wait_for_function("document.querySelectorAll('#scene *').length > 0", timeout=20000)
            page.wait_for_timeout(1500)
            page.screenshot(path=str(out / f"{i:02d}.png"))
            page.close()
            print(f"{i:02d} {entry or '(overview)'}")
        browser.close()


# Opens each closed family in the picture but those in `done`, one at a time, and closes it again:
# [family, boxes closed, frames open].
FAMILIES = """async (done) => {
  const sleep = ms => new Promise(r => setTimeout(r, ms)), fam = g => g.dataset.id.slice(2).split("@")[0];
  const closed = () => [...document.querySelectorAll('#scene g.tnode[data-id^="t:"]')].filter(g => /\\(\\d+\\)/.test(g.querySelector(".title")?.textContent || ""));
  const frames = f => [...document.querySelectorAll("#scene g.frame.fam")].filter(g => fam(g) === f);
  const out = [], before = closed().length;
  for (const f of [...new Set(closed().map(fam))].filter(f => !done.includes(f))) {
    const boxes = closed().filter(g => fam(g) === f);
    if (!boxes.length) { out.push([f, -1, -1]); continue; }
    boxes[0].dispatchEvent(new MouseEvent("click", {bubbles: true}));
    let open = [];
    for (let i = 0; i < 50 && !open.length; i++) { await sleep(100); open = frames(f); }
    out.push([f, new Set(boxes.map(g => g.dataset.id)).size, open.length]);
    open[0]?.querySelector(".ftitle")?.dispatchEvent(new MouseEvent("click", {bubbles: true}));
    for (let i = 0; i < 50 && (frames(f).length || closed().length < before); i++) await sleep(100);
  }
  return out;
}"""


def fault(tab) -> list[str]:
    """What the page says it could not draw, if it says so."""
    text = tab.evaluate("(f => f && !f.hidden ? f.querySelector('pre')?.textContent || 'a fault' : '')(document.getElementById('fault'))")
    return [text.splitlines()[0]] if text else []


def walk(atlas: str, repo: str, out: pathlib.Path) -> int:
    from urllib.parse import quote

    from playwright.sync_api import sync_playwright

    page_file, _, project, graph = page_of(atlas, repo, out)
    units = graph.get("flow", {}).get("units", {})
    views = []
    for part, tops in units.get("tops", []):
        views.append({"part": part})
        for u in tops:
            views.append({"part": part, "open": {"units": [u]}})
            views.extend({"part": part, "open": {"units": [u, k]}} for k in units.get("kids", {}).get(u, []))
    views.extend({"route": n["id"]} for n in graph["nodes"] if n.get("entry") and n["entry"]["kind"] != "test")
    bad, checked = 0, {}
    with sync_playwright() as p:
        browser = p.chromium.launch(channel="chrome")
        ctx = browser.new_context(viewport={"width": 1400, "height": 900}, reduced_motion="reduce")
        for v in views:
            tab, errors = ctx.new_page(), []
            tab.on("pageerror", lambda e: errors.append(str(e)))
            tab.goto(f"{page_file.as_uri()}#p={project.group(1)}&v={quote(json.dumps(v))}")
            try:
                tab.wait_for_function("document.querySelectorAll('#scene g[data-id]').length > 0 || !document.getElementById('fault')?.hidden", timeout=8000)
            except Exception:
                errors.append("draws nothing")
            errors.extend(fault(tab))
            if not errors and "route" in v:
                for f, closed, opened in tab.evaluate(FAMILIES, [f for f, n in checked.items() if n >= 2]):
                    checked[f] = checked.get(f, 0) + 1
                    if closed < 0:
                        errors.append(f"{f} is gone once another family has been opened and closed")
                    elif closed != opened:
                        errors.append(f"{f} stands in {closed} moments closed and opens into {opened}")
                errors.extend(fault(tab))
            if errors:
                bad += 1
                print(json.dumps(v), "·", errors[0])
            tab.close()
        browser.close()
    print(f"{len(views)} views, {bad} that throw or draw nothing")
    return bad


def compare(before: pathlib.Path, after: pathlib.Path) -> int:
    from PIL import Image, ImageChops

    same = differ = 0
    for a in sorted(before.glob("*.png")):
        b = after / a.name
        if not b.exists():
            print(a.name, "missing")
            differ += 1
            continue
        x, y = Image.open(a).convert("RGB"), Image.open(b).convert("RGB")
        box = (0, 60, x.size[0], x.size[1])
        where = x.size == y.size and ImageChops.difference(x.crop(box), y.crop(box)).getbbox()
        if x.size != y.size or where:
            print(a.name, "differs", where or "in size")
            differ += 1
        else:
            same += 1
    print(f"{same} the same, {differ} different")
    return differ


if __name__ == "__main__":
    match sys.argv[1:]:
        case ["draw", atlas, repo, out, *entries]:
            draw(atlas, repo, pathlib.Path(out), entries)
        case ["walk", atlas, repo, out]:
            sys.exit(1 if walk(atlas, repo, pathlib.Path(out)) else 0)
        case ["compare", before, after]:
            sys.exit(1 if compare(pathlib.Path(before), pathlib.Path(after)) else 0)
        case _:
            print(__doc__)
            sys.exit(2)
