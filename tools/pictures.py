# /// script
# requires-python = ">=3.11"
# dependencies = ["playwright", "pillow"]
# ///
"""Draw the routes of a repository with an Atlas build, and compare two sets of drawings: a change
that should leave the pictures as they were can be shown to, pixel for pixel.

    uv run tools/pictures.py draw ATLAS REPO OUTDIR [ENTRY ...]
    uv run tools/pictures.py compare BEFORE AFTER

`draw` writes the overview and each route (the entries named, or up to 40 of the graph's) as a
PNG, in the light theme, with the code panel closed, once the page has laid itself out. It uses
the Chrome installed on the machine. `compare` reports the drawings that differ below the
header, which names the build.
"""

import json
import pathlib
import re
import subprocess
import sys


def draw(atlas: str, repo: str, out: pathlib.Path, entries: list[str]) -> None:
    from playwright.sync_api import sync_playwright

    out.mkdir(parents=True, exist_ok=True)
    page_file = out / "page.html"
    subprocess.run([atlas, "view", repo, "--no-open", "--out", str(page_file)], check=True, capture_output=True)
    html = page_file.read_text()
    project = re.search(r'"projects":\{"([^"]+)":\{', html)
    start = html.index("window.DATA = ") + len("window.DATA = ")
    data, _ = json.JSONDecoder().raw_decode(html[start:].replace("<\\/", "</"))
    graph = data["projects"][project.group(1)]
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
        case ["compare", before, after]:
            sys.exit(1 if compare(pathlib.Path(before), pathlib.Path(after)) else 0)
        case _:
            print(__doc__)
            sys.exit(2)
