# /// script
# requires-python = ">=3.11"
# dependencies = ["playwright"]
# ///
"""Verify storage refinement and interior ports on the clean/shortcut file-join pair.

    uv run tools/storage_views.py ATLAS OUTDIR [DIFF_PAGE]

Checks the graph passed to layout, the rendered frame membership and the actual
endpoints inside closed steps. OUTDIR keeps pages, measurements and screenshots.
"""

import json
import pathlib
import subprocess
import sys

from playwright.sync_api import sync_playwright

sys.dont_write_bytecode = True
from pipeline_views import CAPTURE, canonical, settled, shoot, snapshot


def click_render(page, locator):
    before = page.evaluate('window.layouts')
    locator.click()
    page.wait_for_function('(n) => window.layouts > n', arg=before)
    settled(page)


def projected_edges(s, mapping):
    return sorted({(mapping.get(a, a), mapping.get(b, b)) for a, b in s['edges']})


def assert_ports(page):
    ports = page.evaluate("""() => [...document.querySelectorAll('.interior-port')].map(p => {
      const box = [...document.querySelectorAll('#scene .tstep')]
        .find(g => g.dataset.layoutId === p.dataset.step)?.querySelector('rect.bg');
      const b = box?.getBoundingClientRect(), r = p.getBoundingClientRect();
      return {step: p.dataset.step, inside: !!b && r.x + r.width/2 > b.x &&
        r.x + r.width/2 < b.right && r.y + r.height/2 > b.y && r.y + r.height/2 < b.bottom};
    })""")
    assert ports and all(p['inside'] for p in ports), ports
    return ports


def main(atlas, destination, diff_page=None):
    root = pathlib.Path(__file__).resolve().parents[1]
    out = pathlib.Path(destination).resolve()
    out.mkdir(parents=True, exist_ok=True)
    report = {}
    with sync_playwright() as p:
        browser = p.chromium.launch(channel='chrome')
        ctx = browser.new_context(viewport={'width': 1600, 'height': 1000}, color_scheme='light', reduced_motion='reduce')
        ctx.add_init_script('(' + CAPTURE + ')()')
        for side in ['clean', 'shortcut']:
            html = out / (side + '.html')
            subprocess.run([atlas, 'view', str(root / 'engine/tests/pairs/storage' / side), '--no-open',
                            '--view', json.dumps({'route': 'storage.run'}), '--out', str(html)], check=True)
            page, errors = ctx.new_page(), []
            page.on('pageerror', lambda e: errors.append(str(e)))
            page.goto(html.as_uri(), wait_until='networkidle')
            settled(page)
            closed = snapshot(page)
            report[side] = {'closed': closed, 'ports': assert_ports(page)}
            if side == 'clean':
                assert {'w:File:data/first.txt', 'w:File:data/second.txt', 'w:File:out/combined.txt'} <= {n for n, _ in closed['nodes']}
                assert not page.locator('.frame.storage').count()
            else:
                assert page.locator('[data-id="w:Disk:destination"]').count() == 1, 'Singleton output must show its expression'
                assert not page.locator('[data-id="w:Disk:out"]').count(), 'Singleton output must not be hidden'
                click_render(page, page.locator('[data-id="w:Disk:in"]'))
                opened = snapshot(page)
                frame = page.locator('.frame.storage[data-id="group:w:Disk:in"]')
                assert frame.count() == 1 and 'Disk in' in frame.text_content()
                kids = page.evaluate("""() => {
                  const find = g => g.id === 'group:w:Disk:in' ? g : (g.children || []).map(find).find(Boolean);
                  const f = find(window.planned); return f?.children.map(n => n.id);
                }""")
                assert set(kids) == {'w:Disk:first', 'w:Disk:second'}, kids
                assert projected_edges(opened, {k: 'w:Disk:in' for k in kids}) == projected_edges(closed, {}), 'Refinement changes connections'
                report[side]['opened'] = opened
                shoot(page, out / 'shortcut-open.png')
                click_render(page, frame.locator('.ftitle'))
                assert canonical(snapshot(page)) == canonical(closed), 'Closing storage changes the graph'

            click_render(page, page.locator('.tstep[data-id="storage.join"]'))
            assert page.locator('.frame:not(.storage) .ftitle').count() >= 2
            assert page.locator('.tstep[data-id="storage.read_pair"]').count() == 1
            own = page.locator('.tstep.own[data-id="storage.join"]').get_attribute('data-layout-id')
            assert page.locator(f'.interior-port[data-step="{own}"]').count() == 0, 'Own effects must stop at the boundary'
            click_render(page, page.locator('.frame[data-id="n:storage.run/0"] .ftitle'))
            assert canonical(snapshot(page)) == canonical(closed), 'Closing a step changes the graph'
            if side == 'shortcut':
                click_render(page, page.locator('[data-id="w:Disk:in"]'))
                click_render(page, page.locator('.tstep[data-id="storage.join"]'))
                click_render(page, page.locator('#crumbs button.all'))
                assert canonical(snapshot(page)) == canonical(closed), 'Close all must preserve the route and restore the quotient'
                assert 'storage.run' in page.url
            assert not errors, errors
            shoot(page, out / (side + '.png'))
            page.close()
        database = out / 'database.html'
        subprocess.run([atlas, 'view', str(root / 'engine/tests/pairs/python/clean'), '--no-open',
                        '--view', json.dumps({'route': 'harvest.store.ReadingStore'}), '--out', str(database)], check=True)
        page = ctx.new_page()
        page.goto(database.as_uri(), wait_until='networkidle')
        settled(page)
        closed = snapshot(page)
        click_render(page, page.locator('.tnode[data-id="w:SQLite"]'))
        opened = snapshot(page)
        frame = page.locator('.frame.storage[data-id="group:w:SQLite"]')
        assert frame.count() == 1
        assert {'w:SQLite', 'w:SQLite:readings'} <= {n for n, _ in opened['nodes']}
        assert projected_edges(opened, {'w:SQLite:readings': 'w:SQLite'}) == projected_edges(closed, {})
        shoot(page, out / 'database-open.png')
        click_render(page, frame.locator('.ftitle'))
        assert canonical(snapshot(page)) == canonical(closed), 'Closing a DB group changes the graph'
        page.close()
        if diff_page:
            page = ctx.new_page()
            page.goto(pathlib.Path(diff_page).resolve().as_uri(), wait_until='networkidle')
            settled(page)
            closed = snapshot(page)
            click_render(page, page.locator('.tnode[data-id="w:Disk:in"]'))
            assert page.locator('.frame.storage[data-id="group:w:Disk:in"]').count() == 1
            layouts = page.evaluate('window.layouts')
            for side in ['a', 'b', 'u']:
                page.locator(f'#diffbar button[data-side="{side}"]').click()
                assert page.locator('#fault').is_hidden()
                assert page.evaluate('window.layouts') == layouts, 'Reading switch must not lay out again'
            click_render(page, page.locator('.frame.storage[data-id="group:w:Disk:in"] .ftitle'))
            assert canonical(snapshot(page)) == canonical(closed), 'Closing a diff group changes the graph'
            page.close()
        browser.close()
    (out / 'checks.json').write_text(json.dumps(report, indent=2) + '\n')
    print('Storage pair and SQLite: singleton, frame membership, projected connections, interior ports and open/close cycles passed')
    if diff_page:
        print('Diff: storage frame round trip and reading switches without relayout passed')


if __name__ == '__main__':
    main(*sys.argv[1:])
