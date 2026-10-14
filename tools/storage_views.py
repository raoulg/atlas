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
import shutil
import tempfile

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


def verify_served(ctx, atlas, subject):
    """A reached line and an opened storage frame can be pointed at through MCP."""
    with tempfile.TemporaryDirectory(prefix='atlas-storage-served-') as scratch:
        repo = pathlib.Path(scratch) / 'storage'
        shutil.copytree(subject, repo)
        (repo / '.git').mkdir()
        server = subprocess.Popen([atlas, 'serve', str(repo), '--port', '0', '--no-open'], stderr=subprocess.PIPE, text=True)
        page = None
        try:
            address = server.stderr.readline().split('http://', 1)[1].split('/', 1)[0]
            page = ctx.new_page()
            page.set_default_timeout(8000)
            page.goto('http://' + address + '/#p=storage&v=%7B%22route%22%3A%22storage.run%22%7D', wait_until='domcontentloaded')
            settled(page)
            line = page.locator('path.sedge.reach[data-b="n:storage.run/0"]').first
            point = line.evaluate("""p => {
              for (let f = .1; f < .95; f += .05) {
                const q = p.getPointAtLength(p.getTotalLength()*f), pt = new DOMPoint(q.x,q.y).matrixTransform(p.getScreenCTM());
                if (document.elementFromPoint(pt.x,pt.y)?.closest('path[data-a]') === p) return {x:pt.x,y:pt.y};
              }
            }""")
            assert point, 'Interior connection must remain clickable'
            page.keyboard.down('Alt')
            page.mouse.click(point['x'], point['y'])
            page.keyboard.up('Alt')
            field = page.locator('#notefield textarea')
            field.fill('Why does join read these sources?')
            with page.expect_response(lambda r: r.url.endswith('/api/notes') and r.request.method == 'POST') as response:
                field.press('Enter')
            assert response.value.status == 200
            notes_file = repo / '.atlas/notes.json'
            assert json.loads(notes_file.read_text())[0]['on'] == 'w:Disk:in>n:storage.run/0'
            click_render(page, page.locator('.tnode[data-id="w:Disk:in"]'))
            page.locator('.frame.storage .ftitle').click(modifiers=['Alt'])
            field.fill('Do these sources belong together?')
            with page.expect_response(lambda r: r.url.endswith('/api/notes') and r.request.method == 'POST') as response:
                field.press('Enter')
            assert response.value.status == 200
            assert json.loads(notes_file.read_text())[-1]['on'] == 'group:w:Disk:in'
            shown_file = repo / '.atlas/shown.json'
            for _ in range(40):
                try:
                    shown = json.loads(shown_file.read_text())
                    if any(b['id'] == 'group:w:Disk:in' for b in shown['boxes']):
                        break
                except (FileNotFoundError, json.JSONDecodeError):
                    pass
                page.wait_for_timeout(100)
            else:
                raise AssertionError('Shared view must include the opened storage frame')
            call = {'jsonrpc': '2.0', 'id': 1, 'method': 'tools/call', 'params':
                    {'name': 'notes', 'arguments': {'path': str(repo)}}}
            result = subprocess.run([atlas, 'mcp'], input=json.dumps(call)+'\n', text=True, capture_output=True, check=True)
            assert 'Why does join read these sources?' in result.stdout
            assert 'Do these sources belong together?' in result.stdout
            assert page.locator('#fault').is_hidden()
        finally:
            if page:
                page.close()
            server.terminate()
            server.wait(timeout=10)


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
        overwrite = out / 'overwrite.html'
        subprocess.run([atlas, 'view', str(root / 'engine/tests/pairs/storage/clean'), '--no-open',
                        '--view', json.dumps({'route': 'storage.overwrite'}), '--out', str(overwrite)], check=True)
        page = ctx.new_page()
        page.goto(overwrite.as_uri(), wait_until='domcontentloaded')
        settled(page)
        shared = snapshot(page)
        assert page.locator('.tnode[data-id="w:Disk:path"]').count() == 1
        assert any(a == 'w:Disk:path' for a, b in shared['edges'])
        assert any(b == 'w:Disk:path' for a, b in shared['edges'])
        assert not page.locator('.tnode[data-id="w:Disk:in"], .tnode[data-id="w:Disk:out"]').count()
        assert not page.locator('.frame.storage').count()
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
        verify_served(ctx, atlas, root / 'engine/tests/pairs/storage/shortcut')
        browser.close()
    (out / 'checks.json').write_text(json.dumps(report, indent=2) + '\n')
    print('Storage pair and SQLite: singleton, frame membership, projected connections, interior ports and open/close cycles passed')
    if diff_page:
        print('Diff: storage frame round trip and reading switches without relayout passed')
    print('Serve/MCP: notes on interior connections and storage frames, and shared frame identity passed')


if __name__ == '__main__':
    main(*sys.argv[1:])
