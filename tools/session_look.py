# /// script
# requires-python = ">=3.11"
# dependencies = ["playwright", "pillow"]
# ///
"""Draw the moment an agent points at a box, for the tour's chapter on pointing, with an Atlas build.

    uv run tools/session_look.py ATLAS

The sunspots pipeline of `engine/tests/pairs/pipeline/python-clean` is copied into a scratch
repository and served with `atlas serve`; `atlas mcp` is called as an agent calls it, to point at
`smooth` in the route `run`. The page lights the box in a pulse; the pulse is held still and set
frame by frame, and the frames are written as one looping animation,
`docs/img/session/look.webp`: the pulse, then a rest.
"""

import json
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile

from PIL import Image
from playwright.sync_api import sync_playwright

ROOT = pathlib.Path(__file__).resolve().parents[1]
SUBJECT = ROOT / 'engine/tests/pairs/pipeline/python-clean'
TYPINGS = ROOT / 'engine/tests/pairs/pipeline/typings'
TARGET = ROOT / 'docs/img/session/look.webp'
AT = 'sunspots.series.smooth'
# the part of the page around the box, in CSS pixels: as wide and high as the chat beside it reads
WIDTH, HEIGHT = 760, 190
FRAMES, REST = 22, 1200


def scratch(tmp):
    """The pipeline as a repository of its own, beside the stubs of the libraries it imports."""
    repo = tmp / 'sunspots'
    shutil.copytree(SUBJECT, repo, ignore=shutil.ignore_patterns('__pycache__', '.venv'))
    shutil.copytree(TYPINGS, repo / 'typings')
    pyproject = repo / 'pyproject.toml'
    pyproject.write_text(pyproject.read_text().replace('"../typings"', '"typings"'))
    env = {'GIT_AUTHOR_NAME': 'Raoul', 'GIT_AUTHOR_EMAIL': 'raoul@example.com', 'GIT_COMMITTER_NAME': 'Raoul',
           'GIT_COMMITTER_EMAIL': 'raoul@example.com', 'PATH': '/usr/bin:/bin:/usr/local/bin:/opt/homebrew/bin'}
    for args in (['init', '-q', '-b', 'main'], ['add', '-A'], ['commit', '-q', '-m', "Download, smooth and draw SILSO's sunspot numbers"]):
        subprocess.run(['git', *args], cwd=repo, env=env, check=True)
    return repo


def look(atlas, repo, **args):
    """`atlas mcp`'s look, called as a coding agent calls it."""
    call = {'jsonrpc': '2.0', 'id': 1, 'method': 'tools/call', 'params': {'name': 'look', 'arguments': {**args, 'path': str(repo)}}}
    out = subprocess.run([atlas, 'mcp'], input=json.dumps(call) + '\n', capture_output=True, text=True, check=True)
    assert not json.loads(out.stdout)['result'].get('isError'), out.stdout


def main(atlas):
    atlas = str(pathlib.Path(atlas).resolve())
    with tempfile.TemporaryDirectory() as tmp, sync_playwright() as p:
        tmp = pathlib.Path(tmp)
        repo = scratch(tmp)
        server = subprocess.Popen([atlas, 'serve', '--no-open', '--port', '0', str(repo)], stderr=subprocess.PIPE, text=True)
        for line in server.stderr:
            m = re.search(r'http://(\S+?)/?\s*$', line)
            if m:
                url = f'http://{m.group(1)}/'
                break
        else:
            raise RuntimeError('atlas serve did not say where it serves')
        try:
            browser = p.chromium.launch(channel='chrome')
            page = browser.new_context(viewport={'width': 1400, 'height': 700}, device_scale_factor=2).new_page()
            page.goto(url, wait_until='networkidle')
            page.wait_for_timeout(2000)
            look(atlas, repo, view={'route': 'sunspots.cli.run'}, at=AT)
            page.wait_for_selector('#scene .pointed', timeout=8000)
            page.wait_for_timeout(2600)
            # in on the box, so that its pulse reads
            for _ in range(2):
                page.get_by_role('button', name='+', exact=True).click()
                page.wait_for_timeout(350)
            page.wait_for_timeout(800)
            # the second pointing finds the page in its view, and only the pulse moves
            look(atlas, repo, at=AT)
            page.wait_for_selector('#scene .pointed', timeout=8000)
            # the page takes the pulse away after its two beats; each frame puts it back and holds it at its moment
            duration = page.evaluate("""() => { window.held = document.querySelector('#scene .pointed');
              const a = window.held.getAnimations()[0]; a.pause(); return a.effect.getTiming().duration; }""")
            box = page.locator(f'#scene g[data-id="{AT}"]').first.bounding_box()
            x, y = box['x'] + box['width'] / 2, box['y'] + box['height'] / 2
            clip = {'x': x - WIDTH * 0.45, 'y': y - HEIGHT * 0.6, 'width': WIDTH, 'height': HEIGHT}
            frames = []
            for i in range(FRAMES):
                page.evaluate("""t => { const g = window.held; g.classList.add('pointed');
                  const a = g.getAnimations()[0]; a.pause(); a.currentTime = t; }""", duration * i / FRAMES)
                shot = tmp / f'{i:02}.png'
                page.screenshot(path=str(shot), clip=clip)
                frames.append(Image.open(shot).convert('RGB'))
            browser.close()
        finally:
            server.terminate()
        step = round(duration / FRAMES)
        frames[0].save(TARGET, save_all=True, append_images=frames[1:], duration=[REST] + [step] * (FRAMES - 1), loop=0,
                       quality=90, method=6)
    print(TARGET.relative_to(ROOT), frames[0].size, len(frames), 'frames')


if __name__ == '__main__':
    sys.exit(main(sys.argv[1]))
