# /// script
# requires-python = ">=3.11"
# dependencies = ["playwright"]
# ///
"""Draw the pictures the README, the Pages and the guide show, with an Atlas build, and say which
of them now show something else.

    uv run tools/docs_pictures.py ATLAS

The pictures are views of two subjects, drawn in the installed Chrome:

- the sunspots pipeline, written in Python, Rust and TypeScript (the pages and pictures of
  `docs/showcase`);
- Atlas itself, its `analyze` route, in the light and the dark theme (the guide's flowchart).

Every page and picture is written into `docs/`, and the widths the Pages give the pictures follow
them. What each picture draws, its boxes with their words and its lines, is kept in
`docs/img/drawn.json`. A picture that draws otherwise than `drawn.json` says may no longer match
the text beside it: the tool names it with the files that show it, and exits with 1. The pipeline
pairs are checked on the way: the clean route hands its values from step to step, opening and
closing `Trend.save` and each family gives the same picture, and the shortcut's shared frame is
read and written.
"""

import json
import pathlib
import struct
import subprocess
import sys
import tempfile

from playwright.sync_api import sync_playwright

sys.dont_write_bytecode = True
from pictures import FAMILIES

ROOT = pathlib.Path(__file__).resolve().parents[1]
DOCS = ROOT / 'docs'
DRAWN = DOCS / 'img/drawn.json'
# the files whose text stands beside the pictures
TEXTS = ['README.md', 'docs/index.html', 'docs/reference/guide.md']

CAPTURE = """() => {
  localStorage.setItem('atlas.code', '0');
  let Impl;
  Object.defineProperty(window, 'ELK', {configurable: true,
    get() { return Impl; },
    set(C) { Impl = class extends C {
      async layout(g, ...args) {
        window.planned = structuredClone(g);
        const result = await super.layout(g, ...args);
        window.layouts = (window.layouts || 0) + 1;
        return result;
      }
    }; }
  });
}"""


def snapshot(page):
    return page.evaluate("""() => {
      const edges = g => [...(g.edges || []).flatMap(e =>
        e.sources.flatMap(a => e.targets.map(b => [a, b]))),
        ...(g.children || []).flatMap(edges)];
      return {nodes: [...document.querySelectorAll('#scene g[data-id]')]
        .map(n => [n.dataset.id, n.textContent]), edges: edges(window.planned)};
    }""")


def settled(page):
    page.wait_for_function("window.layouts > 0 && document.querySelectorAll('#scene g[data-id]').length > 0")
    page.evaluate("document.fonts.ready")
    assert page.locator('#fault').is_hidden(), page.locator('#fault').inner_text()


def shoot(page, destination):
    bounds = page.locator('#scene').evaluate("g => {const b=g.getBBox();return {x:b.x,y:b.y,width:b.width,height:b.height}}")
    page.set_viewport_size({'width': max(1400, int(bounds['width']) + 80), 'height': max(900, int(bounds['height']) + 180)})
    page.locator('#scene').evaluate("(g,b) => g.setAttribute('transform', `translate(${30-b.x},${70-b.y})`)", bounds)
    page.locator('#scene').screenshot(path=str(destination))


def world(s):
    return {n for n, _ in s['nodes'] if n.startswith('w:')}


def canonical(s):
    return {'nodes': sorted(s['nodes']), 'edges': sorted(s['edges'])}


def verify_clean(s):
    prefix = 'n:sunspots.cli.run/'
    first = 't:ext:pandas.DataFrame@' + prefix + '2'
    second = 't:ext:pandas.DataFrame~frame@' + prefix + '3'
    chain = [prefix+'2', first, prefix+'3', second, prefix+'5', 'w:File:figures/sunspots.png']
    for a, b in zip(chain, chain[1:]):
        assert [a, b] in s['edges'], f'Missing rendered value flow: {a} -> {b}'
    assert not any(n.startswith('w:Global:') for n, _ in s['nodes'])


def width(png):
    """A PNG's width in pixels, from its header."""
    return struct.unpack('>I', png.read_bytes()[16:20])[0]


def draw(atlas, subject, view, html):
    subprocess.run([atlas, 'view', str(subject), '--view', json.dumps(view), '--no-open', '--out', str(html)],
                   check=True, capture_output=True)


def pipeline(ctx, atlas, out, drawn):
    """The sunspots pipeline in each language: its pages and pictures, checked."""
    cases = [
        ('python-clean', 'python-clean', 'sunspots.cli.run', None),
        ('python-clean-save', 'python-clean', 'sunspots.cli.run', 'sunspots.plot.Plot.save'),
        ('python-sloppy', 'python-sloppy', 'sunspots', None),
        ('rust-clean', 'rust-clean', 'sunspots.main.main', 'sunspots.plot.Plot.save'),
        ('typescript-clean', 'typescript', 'typescript.cli.main', None),
    ]
    results = {}
    for name, subject, route, opened in cases:
        view = {'route': route}
        if opened:
            view['open'] = {'functions': [opened]}
        html = out / (name + '.html')
        draw(atlas, ROOT / 'engine/tests/pairs/pipeline' / subject, view, html)
        page = ctx.new_page()
        errors = []
        page.on('pageerror', lambda e: errors.append(str(e)))
        page.goto(html.as_uri(), wait_until='networkidle')
        settled(page)
        s = snapshot(page)
        results[name] = s
        expected = 'w:File:figures/sunspots.' + ('svg' if subject == 'typescript' else 'png')
        assert expected in world(s), (name, world(s))
        assert any(b == expected for a, b in s['edges']), f'{name}: file has no incoming arrow'
        if name == 'python-clean':
            verify_clean(s)
            before = page.evaluate('window.layouts')
            page.locator('#scene g.tstep[data-id="sunspots.plot.Plot.save"]').click()
            page.wait_for_function('(n) => window.layouts > n', arg=before)
            settled(page)
            expanded = snapshot(page)
            assert world(expanded) == world(s), 'Opening save changes world destinations'
            assert any('draw' in label for _, label in expanded['nodes']), 'Opening save hides draw'
            before = page.evaluate('window.layouts')
            page.locator('#scene g.frame[data-id*="/5"] .ftitle').first.click()
            page.wait_for_function('(n) => window.layouts > n', arg=before)
            settled(page)
            assert canonical(snapshot(page)) == canonical(s), 'Closing save changes the graph'
        if name == 'python-sloppy':
            shared = [n for n, _ in s['nodes'] if n.startswith('w:Global:') and 'sunspots.df' in n]
            assert shared, 'The shortcut must expose its shared frame'
            assert any(a in shared for a, b in s['edges']), 'Shared frame is not read'
            assert any(b in shared for a, b in s['edges']), 'Shared frame is not written'
        families = page.evaluate(FAMILIES, [])
        if name == 'rust-clean':
            assert any(f == 'sunspots.plot.Trend' for f, _, _ in families), 'Trend must exercise family zoom'
        for family, closed, opened_ in families:
            assert closed > 0 and closed == opened_, (name, family, closed, opened_)
        assert canonical(snapshot(page)) == canonical(s), f'{name}: closing families changes the graph'
        assert page.locator('#fault').is_hidden(), page.locator('#fault').inner_text()
        assert not errors, errors
        shoot(page, out / (name + '.png'))
        print(name, ':', len(s['nodes']), 'drawn nodes;', len(families), 'family round trips; checks passed')
        page.close()
        drawn[name] = (canonical(s), {out / (name + '.html'): DOCS / 'showcase' / (name + '.html'),
                                      out / (name + '.png'): DOCS / 'img/showcase' / (name + '.png')})
    assert world(results['python-clean']) == world(results['python-clean-save'])


def flowchart(browser, atlas, out, drawn):
    """Atlas's own `analyze` route, the whole page, in the light and the dark theme."""
    html = out / 'flowchart.html'
    draw(atlas, ROOT, {'route': 'atlas-engine.analyze'}, html)
    files, s = {}, None
    for theme in ['light', 'dark']:
        ctx = browser.new_context(viewport={'width': 1200, 'height': 560}, device_scale_factor=2,
                                  color_scheme=theme, reduced_motion='reduce')
        ctx.add_init_script('(' + CAPTURE + ')()')
        page = ctx.new_page()
        page.goto(html.as_uri(), wait_until='networkidle')
        settled(page)
        s = s or snapshot(page)
        png = out / f'flowchart-{theme}.png'
        page.screenshot(path=str(png))
        files[png] = DOCS / f'img/flowchart-{theme}.png'
        ctx.close()
    drawn['flowchart'] = (canonical(s), files)
    print('flowchart :', len(s['nodes']), 'drawn nodes')


def main(atlas):
    kept = json.loads(DRAWN.read_text()) if DRAWN.exists() else {}
    drawn = {}
    with tempfile.TemporaryDirectory() as tmp, sync_playwright() as p:
        out = pathlib.Path(tmp)
        browser = p.chromium.launch(channel='chrome')
        ctx = browser.new_context(viewport={'width': 1600, 'height': 1000}, device_scale_factor=2,
                                  color_scheme='light', reduced_motion='reduce')
        ctx.add_init_script('(' + CAPTURE + ')()')
        pipeline(ctx, atlas, out, drawn)
        flowchart(browser, atlas, out, drawn)
        browser.close()
        for _, files in drawn.values():
            for source, target in files.items():
                target.write_bytes(source.read_bytes())
    # the Pages show each picture at its own size: half its pixels, drawn at twice the density
    index = DOCS / 'index.html'
    text = index.read_text()
    for png in sorted((DOCS / 'img/showcase').glob('*.png')):
        marker = f'src="img/showcase/{png.name}" style="--w:'
        if marker in text:
            start = text.index(marker) + len(marker)
            end = text.index('"', start)
            text = text[:start] + str(width(png) // 2) + text[end:]
    index.write_text(text)
    DRAWN.write_text(json.dumps({name: shown for name, (shown, _) in drawn.items()}, indent=1) + '\n')
    changed = [name for name, (shown, _) in drawn.items() if kept.get(name) != shown]
    if not changed:
        print('every picture draws what it drew')
        return 0
    print('\nThese pictures draw otherwise; reread the text beside them and change what no longer holds:')
    for name in changed:
        images = [t.name for t in drawn[name][1].values() if t.suffix == '.png']
        where = [f for f in TEXTS if any(i in (ROOT / f).read_text() for i in images)]
        print(f'  {name}: {", ".join(images)}, shown in {", ".join(where) or "no text"}')
    return 1


if __name__ == '__main__':
    sys.exit(main(sys.argv[1]))
