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
says it could not draw it), draws nothing or plans a line to a box it does not draw, and each family that opens into another number of moments than it stood in
closed; it fails when there is one.

`topology` also asks the page for its oracle, which holds the plan of every route to the same laws before anything is laid
out (`window.ATLAS_ORACLE`, `atlasPicture.oracle`): for each box that can be opened, the picture with that box open is
taken from the route's finest plan and compared with the one drawn, and every difference is reported.
It opens one box at a time in each route (up to ROUTES routes, default all; up to OPENS boxes in each,
default 8: its steps, then its closed families) and holds the picture that opens to the one it was: opening a
box refines the quotient, so it takes no line away (every line of the closed picture is a line of the open one,
seen through the quotient map), draws no line the closed picture did not (but one a step reads back of what it
wrote), moves no box from outside a frame into it, and closing the box draws the closed picture again. It fails
when a box does otherwise.
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
    subprocess.run([atlas, "view", repo, "--no-open", "--out", str(page_file), *os.environ.get("ATLAS_VIEW_ARGS", "").split()], check=True, capture_output=True)
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

# Opens one box of the route in the page and says what the picture did: `null` lists the boxes that open, `{id, kind}` opens one.
# The quotient map sends each box of the open picture to the box of the closed one it lies in, or to itself.
TOPOLOGY = """async (spec) => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const settle = async n0 => { for (let i = 0; i < 100 && window.atlasPicture.n === n0; i++) await sleep(50); await sleep(30); };
  const snap = () => { const P = window.atlasPicture; return {n: P.n, folded: new Map(window.atlasPlan?.coarse?.folded || []), boxes: new Map(P.boxes), sides: new Map(P.sides || []), lines: P.lines.map(([a, b]) => a + ">" + b)}; };
  const keyOf = id => id.replace(/^[no]:/, "");
  const closedFamily = g => g.matches('g.tnode[data-id^="t:"]') && /\\(\\d+\\)/.test(g.querySelector(".title")?.textContent || "");
  const targets = () => [...window.atlasBoxes].flatMap(([id, g]) => g.classList.contains("tstep") && /^▸/.test(g.querySelector(".title")?.textContent || "") ? [{id, kind: "step"}] : closedFamily(g) ? [{id, kind: "family"}] : []);
  if (spec === null) return targets();
  const phiFor = (coarse, fine) => {
    const keys = new Map();
    for (const id of coarse.boxes.keys()) if (/^n:/.test(id)) for (const k of keyOf(id).split("+")) keys.set(k, id);
    const under = k => { let best = null; for (const [kc, id] of keys) if ((k === kc || k.startsWith(kc + "/") || k.startsWith(kc + "~") || k.startsWith(kc + "|")) && (!best || kc.length > best[0].length)) best = [kc, id]; return best && best[1]; };
    return id => {
      if (coarse.boxes.has(id)) return id;
      if (coarse.folded.has(id)) return coarse.folded.get(id);
      for (let p = fine.boxes.get(id); p && p !== "root"; p = fine.boxes.get(p)) if (coarse.boxes.has(p)) return p;
      const m = id.match(/^(.*?)@(.*)$/);
      if (m) {
        const k = keyOf(m[2]), u = under(k);
        if (u) {
          if (k === keyOf(u)) { const par = coarse.boxes.get(u), at = /~c\\d+$/.test(par) ? par : u; if (coarse.boxes.has(m[1] + "@" + at)) return m[1] + "@" + at; }
          return u;
        }
      }
      // siblings with the same lines are one box, `n:a+b`: it lies where its first step lies
      if (/^[no]:/.test(id)) { const u = under(keyOf(id).split("+")[0]); if (u) return u; }
      return id;
    };
  };
  const coarse = snap(), g = window.atlasBoxes.get(spec.id);
  if (!g) return {gone: true};
  let n0 = window.atlasPicture.n;
  g.dispatchEvent(new MouseEvent("click", {bubbles: true}));
  await settle(n0);
  const fine = snap(), phi = phiFor(coarse, fine), image = new Set();
  for (const l of fine.lines) { const [a, b] = l.split(">"); const A = phi(a), B = phi(b); if (A !== B) image.add(A + ">" + B); }
  const base = new Set(coarse.lines), lost = [...base].filter(l => !image.has(l));
  // a line from a place into the box that wrote it is the box reading back what it wrote: the composite takes nothing from there
  // (a place used both ways stands twice, `from X ⇄` and `into X ⇄`: the line goes to the twin)
  const twin = x => x.replace(/ ⇄$/, "");
  const wrote = l => { const [a, b] = l.split(">"); return [...base, ...image].some(m => { const [c, d] = m.split(">"); return c === b && twin(d) === twin(a); }); };
  const gained = [...image].filter(l => !base.has(l) && !wrote(l));
  const moved = [...fine.boxes].filter(([b, par]) => coarse.boxes.has(b) && coarse.boxes.get(b) !== par).map(([b, par]) => `${b}: ${coarse.boxes.get(b)} -> ${par}`);
  // a place of the world stands on the side it stood: opening a box does not make what it does there another's
  const resided = [...fine.sides].filter(([id, side]) => coarse.sides.has(id) && coarse.sides.get(id) !== side).map(([id, side]) => `${id}: ${coarse.sides.get(id)} -> ${side}`);
  // close it again by its frame
  const frame = [...fine.boxes].find(([b, par]) => !coarse.boxes.has(b) && coarse.boxes.has(par) && /^(n|t):/.test(par))?.[1];
  let back = null;
  const fr = frame && [...document.querySelectorAll("#scene g.frame[data-id]")].find(x => x.dataset.id === frame);
  if (fr) {
    n0 = window.atlasPicture.n;
    fr.querySelector(".ftitle")?.dispatchEvent(new MouseEvent("click", {bubbles: true}));
    await settle(n0);
    const again = snap(), same = (a, b) => a.length === b.length && a.every(x => b.includes(x));
    back = same([...again.boxes.keys()], [...coarse.boxes.keys()]) && same(again.lines, coarse.lines);
  }
  return {opened: spec.id, lost, gained, moved, resided, reversible: back};
}"""

# Opens the boxes named, one after the other, and gives the picture: its boxes with the box that holds each, and its lines.
ORDER = """async (specs) => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const settle = async n0 => { for (let i = 0; i < 100 && window.atlasPicture.n === n0; i++) await sleep(50); await sleep(30); };
  const closedFamily = g => g.matches('g.tnode[data-id^="t:"]') && /\\(\\d+\\)/.test(g.querySelector(".title")?.textContent || "");
  for (const spec of specs) {
    const g = window.atlasBoxes.get(spec.id);
    if (!g) return {gone: true};
    const n0 = window.atlasPicture.n;
    g.dispatchEvent(new MouseEvent("click", {bubbles: true}));
    await settle(n0);
  }
  const P = window.atlasPicture;
  return {boxes: P.boxes.map(b => b[0] + " in " + b[1]).sort(), lines: P.lines.map(l => l.join(">")).sort()};
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
        for i, v in enumerate(views):
            tab, errors = ctx.new_page(), []
            tab.on("pageerror", lambda e: errors.append(str(e)))
            try:
                tab.goto(f"{page_file.as_uri()}#p={project.group(1)}&v={quote(json.dumps(v))}", wait_until="domcontentloaded")
                tab.wait_for_function("document.querySelectorAll('#scene g[data-id]').length > 0 || !document.getElementById('fault')?.hidden", timeout=8000)
            except Exception as e:
                errors.append(str(e).splitlines()[0])
            if not errors:
                errors.extend(fault(tab))
            if not errors:
                lost = tab.evaluate("window.atlasPicture?.dropped || []")
                if lost:
                    errors.append(f"{len(lost)} lines are planned to a box that is not drawn, as {lost[0]}")
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
            if (i + 1) % 40 == 0:
                print(f"{i + 1}/{len(views)} views checked", flush=True)
        browser.close()
    print(f"{len(views)} views, {bad} that throw or draw nothing")
    return bad


def topology(atlas: str, repo: str, out: pathlib.Path, routes: int, opens: int) -> int:
    from urllib.parse import quote

    from playwright.sync_api import sync_playwright

    page_file, _, project, graph = page_of(atlas, repo, out)
    entries = [n["id"] for n in graph["nodes"] if n.get("entry") and n["entry"]["kind"] != "test"][:routes]
    bad = checked = drawn_more = 0
    with sync_playwright() as p:
        browser = p.chromium.launch(channel="chrome")
        ctx = browser.new_context(viewport={"width": 1400, "height": 900}, reduced_motion="reduce")
        ctx.add_init_script("window.ATLAS_ORACLE = true")
        tab = ctx.new_page()
        tab.on("pageerror", lambda e: print("page error:", str(e).splitlines()[0]))

        def load(route: str) -> bool:
            tab.goto("about:blank")
            tab.goto(f"{page_file.as_uri()}#p={project.group(1)}&v={quote(json.dumps({'route': route}))}", wait_until="domcontentloaded")
            try:
                tab.wait_for_function("window.atlasPicture && document.querySelectorAll('#scene g[data-id]').length > 0", timeout=30000)
            except Exception:
                return False
            tab.wait_for_timeout(200)
            return True

        for route in entries:
            if not load(route):
                continue
            plan_faults = tab.evaluate("window.atlasPicture.oracle || []")
            if plan_faults:
                bad += 1
                print(route, "· the plan opens to a picture that does not refine it")
                for f in plan_faults[:5]:
                    print("   ", f)
            for spec in tab.evaluate(TOPOLOGY, None)[:opens]:
                if not load(route):
                    break
                r = tab.evaluate(TOPOLOGY, spec)
                checked += 1
                if r.get("gone"):
                    continue
                faults = [f"{what} {x}" for what in ("lost", "moved", "resided") for x in r[what][:3]] + (["does not close to the picture it was"] if r["reversible"] is False else [])
                more = [f"more {x}" for x in r["gained"][:3]]
                if faults or more:
                    bad += bool(faults)
                    print(route, "· opens", spec["kind"], spec["id"])
                    for f in faults + more:
                        print("   ", f)
                    drawn_more += bool(more)
            # two boxes opened in either order draw the same picture
            specs = tab.evaluate(TOPOLOGY, None)[:3]
            for i, A in enumerate(specs):
                for B in specs[i + 1:]:
                    shots = []
                    for order in ([A, B], [B, A]):
                        if not load(route):
                            break
                        shots.append(tab.evaluate(ORDER, order))
                    checked += 1
                    if len(shots) == 2 and not any(x.get("gone") for x in shots) and shots[0] != shots[1]:
                        bad += 1
                        print(route, "· opens", A["id"], "and", B["id"], "and the order changes the picture")
                        for key in ("boxes", "lines"):
                            for x in sorted(set(shots[0][key]) ^ set(shots[1][key]))[:3]:
                                print("   ", key, x)
        # a focus keeps the rest of the code on either side of it when its package is opened
        tops = [u for _, us in graph.get("flow", {}).get("units", {}).get("tops", []) for u in us][:opens]
        for u in tops:
            ends = []
            for view in ({"focus": [u]}, {"focus": [u], "open": {"units": [u]}}):
                tab.goto("about:blank")
                tab.goto(f"{page_file.as_uri()}#p={project.group(1)}&v={quote(json.dumps(view))}", wait_until="domcontentloaded")
                try:
                    tab.wait_for_function("window.atlasPicture && document.querySelectorAll('#scene g[data-id]').length > 0", timeout=30000)
                except Exception:
                    break
                lines = tab.evaluate("window.atlasPicture.lines.map(l => l.join('>'))")
                ends.append(({l for l in lines if l.startswith("n:unit:~in>")}, {l for l in lines if l.endswith(">n:unit:~out")}))
            checked += 1
            if len(ends) == 2 and ((ends[0][0] and not ends[1][0]) or (ends[0][1] and not ends[1][1])):
                bad += 1
                print("focus on", u, "· opens it and loses the rest of the code on", "the way in" if ends[0][0] and not ends[1][0] else "the way out")
        browser.close()
    print(f"{checked} boxes opened in {len(entries)} routes, {bad} that do not refine the picture they open from, {drawn_more} that draw a line more than the closed box did")
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
        case ["topology", atlas, repo, out, *limits]:
            sys.exit(1 if topology(atlas, repo, pathlib.Path(out), *(int(x) for x in (limits + ["9999", "8"][len(limits):]))) else 0)
        case ["compare", before, after]:
            sys.exit(1 if compare(pathlib.Path(before), pathlib.Path(after)) else 0)
        case _:
            print(__doc__)
            sys.exit(2)
