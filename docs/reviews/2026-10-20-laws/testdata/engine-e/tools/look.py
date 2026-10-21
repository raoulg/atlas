"""Load an Atlas page in a view and dump what the picture marks.

usage: look.py <page.html> '<view json>' [side u|a|b] [--clicks '<json list of data-id to click, in order>'] [--oracle]
Prints each box (id, label, diff class, mark, tooltip, inside tag) and each line (ends, diff class, bundle count).
"""
import json
import re
import sys
from urllib.parse import quote

from playwright.sync_api import sync_playwright

DUMP = """() => {
  const txt = e => (e?.textContent || '').trim();
  const boxes = [];
  for (const g of document.querySelectorAll('#scene g[data-id]')) {
    const id = g.dataset.layoutId || g.dataset.id;
    const titles = [...g.children].filter(c => c.tagName === 'title').map(txt);
    const label = txt(g.querySelector(':scope > text.title, :scope > text.ftitle'));
    const cls = [...g.classList].filter(c => c.startsWith('d-')).join(' ');
    const mark = txt(g.querySelector(':scope > text.dmark'));
    const tag = txt(g.querySelector(':scope > g.inside.diff text'));
    const tagTitle = txt(g.querySelector(':scope > g.inside.diff title'));
    boxes.push({id, kind: g.getAttribute('class'), label, cls, mark, titles, tag, tagTitle});
  }
  const lines = [];
  for (const p of document.querySelectorAll('#scene path.sedge:not(.interior-port)')) {
    const cls = [...p.classList].filter(c => c.startsWith('d-')).join(' ');
    lines.push({a: p.dataset.a, b: p.dataset.b, cls, title: txt(p.querySelector('title')), reach: p.classList.contains('reach')});
  }
  const bundles = [...document.querySelectorAll('#scene text.sedge-label.bundle')].map(t => ({text: txt(t), cls: [...t.classList].filter(c => c.startsWith('d-')).join(' ')}));
  const P = window.atlasPicture || {};
  return {n: P.n, boxes, lines, bundles, dropped: P.dropped || [], oracle: P.oracle || [], fault: document.getElementById('fault')?.hidden === false ? txt(document.getElementById('fault')) : null, hash: location.hash, onlyA: document.getElementById('app').classList.contains('only-a'), onlyB: document.getElementById('app').classList.contains('only-b'), pictureBoxes: P.boxes || [], pictureLines: P.lines || []};
}"""

CLICK = """async (id) => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const n0 = window.atlasPicture?.n || 0;
  const g = [...document.querySelectorAll('#scene g[data-id]')].find(g => (g.dataset.layoutId || g.dataset.id) === id);
  if (!g) return 'no box ' + id;
  const target = g.classList.contains('frame') ? g.querySelector('.ftitle') : g;
  target.dispatchEvent(new MouseEvent('click', {bubbles: true}));
  for (let i = 0; i < 200 && (window.atlasPicture?.n || 0) === n0; i++) await sleep(50);
  await sleep(100);
  return 'clicked ' + id + ' -> ' + location.hash;
}"""


def main():
    page_file, view = sys.argv[1], json.loads(sys.argv[2])
    side = sys.argv[3] if len(sys.argv) > 3 and not sys.argv[3].startswith("--") else "u"
    clicks = []
    if "--clicks" in sys.argv:
        clicks = json.loads(sys.argv[sys.argv.index("--clicks") + 1])
    oracle = "--oracle" in sys.argv
    html = open(page_file, encoding="utf-8").read()
    project = re.search(r'"projects":\{"([^"]+)":\{', html).group(1)
    import pathlib
    uri = pathlib.Path(page_file).resolve().as_uri()
    h = f"#p={project}" + (f"&v={quote(json.dumps(view))}" if view else "") + (f"&d={side}" if side in ("a", "b") else "")
    with sync_playwright() as p:
        browser = p.chromium.launch(channel="chrome")
        ctx = browser.new_context(viewport={"width": 1600, "height": 1000}, reduced_motion="reduce")
        if oracle:
            ctx.add_init_script("window.ATLAS_ORACLE = true")
        tab = ctx.new_page()
        errors = []
        tab.on("pageerror", lambda e: errors.append(str(e)))
        tab.goto(uri + h, wait_until="domcontentloaded")
        tab.wait_for_function("(window.atlasPicture && document.querySelectorAll('#scene g[data-id]').length > 0) || document.getElementById('fault')?.hidden === false", timeout=30000)
        tab.wait_for_timeout(300)
        for c in clicks:
            print("#", tab.evaluate(CLICK, c))
        out = tab.evaluate(DUMP)
        out["errors"] = errors
        print(json.dumps(out, indent=1, ensure_ascii=False))
        browser.close()


if __name__ == "__main__":
    main()
