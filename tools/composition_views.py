# /// script
# requires-python = ">=3.11"
# dependencies = ["playwright"]
# ///
"""Read two actual revisions, verify ordered diff pills, and capture both pictures.

    uv run tools/composition_views.py ATLAS OUTDIR
"""

import json
import pathlib
import subprocess
import sys
import tempfile

sys.dont_write_bytecode = True
from pipeline_views import CAPTURE, settled, shoot
from playwright.sync_api import sync_playwright

PROGRAM = (pathlib.Path(__file__).resolve().parents[1] / 'engine/tests/composition/number.py').read_text()


def handoff(page, first, second):
    drawn = page.evaluate("""() => ({
      steps: [...document.querySelectorAll('#scene g.tstep[data-layout-id]')]
        .filter(n => !n.closest('.d-' + (document.getElementById('app').classList.contains('only-a') ? 'added' : 'removed')))
        .map(n => [n.dataset.layoutId, n.dataset.id]),
      edges: [...document.querySelectorAll('#scene path.sedge[data-a][data-b]')]
        .filter(n => getComputedStyle(n).display !== 'none').map(n => [n.dataset.a,n.dataset.b])
    })""")
    source = {node for node, author in drawn['steps'] if author == 'composition.' + first}
    target = {node for node, author in drawn['steps'] if author == 'composition.' + second}
    return any(a in source and any(c == b and d in target for c, d in drawn['edges']) for a, b in drawn['edges'])


def main(atlas, output):
    atlas = str(pathlib.Path(atlas).resolve())
    out = pathlib.Path(output).resolve()
    out.mkdir(parents=True, exist_ok=True)
    results = {}
    with tempfile.TemporaryDirectory(prefix='atlas-composition-') as scratch:
        repo = pathlib.Path(scratch) / 'composition'
        repo.mkdir()
        (repo / 'pyproject.toml').write_text('[project]\nname="composition"\nversion="0.1.0"\n[project.scripts]\ncomposition="composition:run"\n')

        def git(*args):
            return subprocess.run(['git', '-C', str(repo), '-c', 'user.name=atlas',
                                   '-c', 'user.email=atlas@example.org', '-c', 'commit.gpgsign=false',
                                   '-c', 'tag.gpgsign=false', *args], check=True, capture_output=True, text=True).stdout

        git('init', '-q', '-b', 'main')
        for label, expression, expected in [('first', 'double(add_one(Number(3)))', '8'), ('second', 'add_one(double(Number(3)))', '7')]:
            source = PROGRAM.replace('EXPRESSION', expression)
            (repo / 'composition.py').write_text(source)
            (out / (label + '.py')).write_text(source)
            scope = {}
            exec(source, scope)
            assert str(eval(expression, scope).value) == expected
            git('add', 'composition.py', 'pyproject.toml')
            git('commit', '-q', '-m', label)
            git('tag', label)
            results[label] = {'expression': expression, 'result': int(expected), 'commit': git('rev-parse', 'HEAD').strip()}
            subprocess.run([atlas, 'view', str(repo), '--view', json.dumps({'route': 'composition.run'}),
                            '--no-open', '--out', str(out / (label + '.html'))], check=True)
        page_path = out / 'diff.html'
        subprocess.run([atlas, 'view', str(repo), '--since', 'first', '--until', 'second',
                        '--view', json.dumps({'route': 'composition.run'}), '--no-open', '--out', str(page_path)], check=True)
        changed = subprocess.run([atlas, 'changes', 'first..second', '--repo', str(repo), '--json', '--no-cache'],
                                 check=True, capture_output=True, text=True)
        results['changes'] = json.loads(changed.stdout)
        report = subprocess.run([atlas, 'changes', 'first..second', '--repo', str(repo), '--no-cache'],
                                check=True, capture_output=True, text=True)
        (out / 'changes.txt').write_text(report.stdout)
        results['changes_detects_order'] = any(r['id'] == 'composition.run' for r in results['changes']['routes'])
        assert results['changes_detects_order'], 'atlas-code changes must report the reordered composition'
        route = next(r for r in results['changes']['routes'] if r['id'] == 'composition.run')
        assert len(route['compositions_changed']) == 1
        assert not route['steps_added'] and not route['steps_removed']
        with sync_playwright() as p:
            browser = p.chromium.launch(channel='chrome')
            ctx = browser.new_context(viewport={'width': 1600, 'height': 1000}, device_scale_factor=2, reduced_motion='reduce')
            ctx.add_init_script('(' + CAPTURE + ')()')
            page = ctx.new_page()
            errors = []
            page.on('pageerror', lambda e: errors.append(str(e)))
            for label, first, second in [('first', 'add_one', 'double'), ('second', 'double', 'add_one')]:
                page.goto((out / (label + '.html')).as_uri(), wait_until='domcontentloaded')
                settled(page)
                assert handoff(page, first, second), f'{label}: the standalone composition must connect'
                shoot(page, out / (label + '.png'))
            page.goto(page_path.as_uri(), wait_until='domcontentloaded')
            settled(page)
            nodes = page.evaluate("Object.values(window.DATA.projects)[0].diff.nodes")
            assert not any('composition.' + name in nodes for name in ['run', 'add_one', 'double']), nodes
            page.wait_for_function("[...document.querySelectorAll('#scene g.tstep')].every(n => getComputedStyle(n).opacity === '1')")
            layouts = page.evaluate('window.layouts')
            for side, expected in [('a', ['composition.add_one', 'composition.double']), ('b', ['composition.double', 'composition.add_one'])]:
                page.locator(f'#diffbar button[data-side="{side}"]').click()
                visible = page.locator('#scene g.tstep[data-id]').evaluate_all("ns => ns.filter(n => ['composition.add_one','composition.double'].includes(n.dataset.id) && !n.closest('.d-' + (document.getElementById('app').classList.contains('only-a') ? 'added' : 'removed'))).map(n => ({id:n.dataset.id,x:n.getBoundingClientRect().x,y:n.getBoundingClientRect().y}))")
                # Site order is execution order; geometry follows the drawn value flow.
                calls = [n['id'] for n in visible]
                shoot(page, out / (side + '.png'))
                assert calls == expected, (side, calls)
                results[side] = visible
                results[side + '_handoff_preserved'] = handoff(page, *(['add_one', 'double'] if side == 'a' else ['double', 'add_one']))
                assert results[side + '_handoff_preserved'], f'{side}: union loses the nested result'
            assert page.evaluate('window.layouts') == layouts, 'Switching revisions must preserve layout'
            assert not errors, errors
            browser.close()
    (out / 'checks.json').write_text(json.dumps(results, indent=2) + '\n')
    print('Standalone value chains and ordered diff occurrences verified.')
    print('atlas-code changes detects order:', results['changes_detects_order'])
    print('Union preserves value handoff:', {s: results[s + '_handoff_preserved'] for s in ['a', 'b']})


if __name__ == '__main__':
    main(sys.argv[1], sys.argv[2])
