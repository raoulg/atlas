# /// script
# requires-python = ">=3.11"
# dependencies = ["playwright"]
# ///
"""Verify the rendered pipeline pairs and write their showcase pages and pictures.

    uv run tools/pipeline_views.py ATLAS OUTDIR

OUTDIR contains showcase/*.html and img/showcase/*.png. Copy the four Python and
Rust examples into docs to refresh the showcase. Chrome must be installed. Checks inspect the layout graph that
the viewer actually draws, and exercise opening and closing the save step.
"""

import json
import pathlib
import subprocess
import sys

from playwright.sync_api import sync_playwright

sys.dont_write_bytecode = True
from pictures import FAMILIES


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
    return sorted(s['nodes']), sorted(s['edges'])


def verify_clean(s):
    prefix = 'n:sunspots.cli.run/'
    first = 't:ext:pandas.DataFrame@' + prefix + '2'
    second = 't:ext:pandas.DataFrame@' + prefix + '3'
    chain = [prefix+'2', first, prefix+'3', second, prefix+'5', 'w:File:figures/sunspots.png']
    for a, b in zip(chain, chain[1:]):
        assert [a, b] in s['edges'], f'Missing rendered value flow: {a} -> {b}'
    assert not any(n.startswith('w:Global:') for n, _ in s['nodes'])


def main(atlas, out):
    root = pathlib.Path(__file__).resolve().parents[1]
    out = pathlib.Path(out).resolve()
    pages, images = out / 'showcase', out / 'img/showcase'
    pages.mkdir(parents=True, exist_ok=True)
    images.mkdir(parents=True, exist_ok=True)
    cases = [
        ('python-clean', 'python-clean', 'sunspots.cli.run', None),
        ('python-clean-save', 'python-clean', 'sunspots.cli.run', 'sunspots.plot.Plot.save'),
        ('python-sloppy', 'python-sloppy', 'sunspots', None),
        ('rust-clean', 'rust-clean', 'sunspots.main.main', 'sunspots.plot.Plot.save'),
        ('typescript-clean', 'typescript', 'typescript.cli.main', None),
    ]
    results = {}
    with sync_playwright() as p:
        browser = p.chromium.launch(channel='chrome')
        ctx = browser.new_context(viewport={'width': 1600, 'height': 1000}, device_scale_factor=2,
                                  color_scheme='light', reduced_motion='reduce')
        ctx.add_init_script('(' + CAPTURE + ')()')
        for name, subject, route, opened in cases:
            view = {'route': route}
            if opened:
                view['open'] = {'functions': [opened]}
            html = pages / (name + '.html')
            subprocess.run([atlas, 'view', str(root / 'engine/tests/pairs/pipeline' / subject),
                            '--view', json.dumps(view), '--no-open', '--out', str(html)], check=True)
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
            for family, closed, opened in families:
                assert closed > 0 and closed == opened, (name, family, closed, opened)
            assert canonical(snapshot(page)) == canonical(s), f'{name}: closing families changes the graph'
            assert page.locator('#fault').is_hidden(), page.locator('#fault').inner_text()
            assert not errors, errors
            shoot(page, images / (name + '.png'))
            print(name, ':', len(s['nodes']), 'drawn nodes;', len(families), 'family round trips; checks passed')
            page.close()
        assert world(results['python-clean']) == world(results['python-clean-save'])
        browser.close()
    (out / 'pipeline-checks.json').write_text(json.dumps(results, indent=2) + '\n')


if __name__ == '__main__':
    main(sys.argv[1], sys.argv[2])
