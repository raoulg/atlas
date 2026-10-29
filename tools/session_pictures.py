# /// script
# requires-python = ">=3.11"
# dependencies = ["playwright", "pillow"]
# ///
"""Draw the page pictures of the tour's two sessions with an Atlas build, in the light and the dark theme.

    uv run tools/session_pictures.py ATLAS

The sessions are the ones the tour shows beside its chats: pointing (where does the smoothing
happen; a note on `smooth`, read and answered) and designing ahead (a vision of `peaks` and
`Cycles`, decided into a prophecy, then built). Each is played again on the sunspots pipeline of
`engine/tests/pairs/pipeline/python-clean`, copied into a scratch repository: `atlas-code serve` serves
it, `atlas-code mcp` is called as the agent called it, and the page is driven in the installed Chrome as
the person drove it. Every picture is a screenshot of the page, at twice the density, written to
`docs/img/session/<name>-<theme>.png`; the moment the agent points is an animation, `look-<theme>.webp`:
the viewer's pulse held still and set frame by frame, then a rest.
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
OUT = ROOT / 'docs/img/session'
THEMES = ['light', 'dark']
FRAMES, REST = 22, 1200

NOTE = 'why does smooth use "center"?'
ANSWER = ('center=True dates each 13-month mean by the month in its middle, so the smoothed line has no lag: a peak in '
          'the smoothed curve falls on the month of the real peak. Without it, pandas dates each mean by the last month '
          'of the window and the curve trails the data by six months. This is the standard 13-month smoothed sunspot '
          'number. The price: the first and last six months have no full window and get no smoothed value.')

MARKERS = '''"""Marks for code that is not built yet: Atlas reads them by their name."""

from typing import Callable, TypeVar

T = TypeVar("T")


def prophecy(**_: str) -> Callable[[T], T]:
    """Marks sketch code that is not built yet; Atlas reads the mark by its name."""

    def mark(item: T) -> T:
        return item

    return mark
'''

PEAKS = '''

def peaks(frame: pd.DataFrame, years: int) -> pd.DataFrame:
    """`frame` with a `peak` column: true where the smoothed mean is the highest within `years` years
    around it, the maximum of a solar cycle."""
    %s
'''
PEAKS_BODY = ('highest = frame["smoothed"].rolling(12 * years, center=True, min_periods=1).max()\n'
              '    return frame.assign(peak=frame["smoothed"] == highest)')

CYCLES = '''

class Cycles(Trend):
    """The trend, with the maximum of each solar cycle marked."""

    def draw(self, frame: pd.DataFrame) -> Figure:
        figure = super().draw(frame)
        %s
        return figure
'''
CYCLES_BODY = ('ax = figure.axes[0]\n'
               '        maxima = frame[frame["peak"]]\n'
               '        ax.scatter(maxima.index, maxima["smoothed"], color="C3", zorder=3, label="cycle maximum")\n'
               '        ax.legend()')

RUN_CYCLES = '''

@app.command()
def run_cycles(raw: Path = RAW, figure: Path = FIGURE, months: int = 13, years: int = 11) -> None:
    """Like `run`, and mark the maximum of each solar cycle on the trend."""
    settings = Settings(months=months)
    if not raw.exists():
        download(SILSO, raw)
    frame = peaks(smooth(load(raw), settings.months), years)
    Cycles(settings).save(frame, figure)
'''


def git(repo, *args):
    env = {'GIT_AUTHOR_NAME': 'Raoul', 'GIT_AUTHOR_EMAIL': 'raoul@example.com', 'GIT_COMMITTER_NAME': 'Raoul',
           'GIT_COMMITTER_EMAIL': 'raoul@example.com', 'PATH': '/usr/bin:/bin:/usr/local/bin:/opt/homebrew/bin'}
    subprocess.run(['git', *args], cwd=repo, env=env, check=True, capture_output=True)


def scratch(at, markers):
    """The pipeline as a repository of its own, beside the stubs of the libraries it imports; with
    `markers`, the decorator that marks a prophecy in a module of its own, committed after it."""
    repo = at / 'sunspots'
    shutil.copytree(SUBJECT, repo, ignore=shutil.ignore_patterns('__pycache__', '.venv'))
    shutil.copytree(TYPINGS, repo / 'typings')
    pyproject = repo / 'pyproject.toml'
    pyproject.write_text(pyproject.read_text().replace('"../typings"', '"typings"'))
    # what the built design calls of the libraries
    for stub, old, new in [('pandas/__init__.pyi', '    def mean(self) -> Series: ...', '    def mean(self) -> Series: ...\n    def max(self) -> Series: ...'),
                           ('matplotlib/axes.pyi', '    def plot(self', '    def scatter(self, x: Any, y: Any, **kwargs: Any) -> Any: ...\n    def plot(self'),
                           ('matplotlib/figure.pyi', 'class Figure:\n', 'class Figure:\n    axes: list[Axes]\n')]:
        f = repo / 'typings' / stub
        f.write_text(f.read_text().replace(old, new, 1))
    git(repo, 'init', '-q', '-b', 'main')
    git(repo, 'add', '-A')
    git(repo, 'commit', '-q', '-m', "Download, smooth and draw SILSO's sunspot numbers")
    if markers:
        (repo / 'src/sunspots/markers.py').write_text(MARKERS)
        git(repo, 'add', '-A')
        git(repo, 'commit', '-q', '-m', 'A marker for sketch code that is not built yet')
    return repo


class Session:
    """`atlas-code serve` on a repository, its page in the browser, and `atlas-code mcp` as the agent calls it."""

    def __init__(self, atlas, repo, browser, theme):
        self.atlas, self.repo = atlas, repo
        self.server = subprocess.Popen([atlas, 'serve', '--no-open', '--port', '0', str(repo)], stderr=subprocess.PIPE, text=True)
        for line in self.server.stderr:
            m = re.search(r'http://(\S+?)/?\s*$', line)
            if m:
                self.url = f'http://{m.group(1)}/'
                break
        else:
            raise RuntimeError('atlas-code serve did not say where it serves')
        self.context = browser.new_context(viewport={'width': 1400, 'height': 760}, device_scale_factor=2, color_scheme=theme)
        self.page = self.context.new_page()
        self.errors = []
        self.page.on('pageerror', lambda e: self.errors.append(str(e)))
        self.page.goto(self.url, wait_until='networkidle')
        self.page.wait_for_timeout(2000)

    def tool(self, name, **args):
        call = {'jsonrpc': '2.0', 'id': 1, 'method': 'tools/call', 'params': {'name': name, 'arguments': {**args, 'path': str(self.repo)}}}
        out = subprocess.run([self.atlas, 'mcp'], input=json.dumps(call) + '\n', capture_output=True, text=True, check=True)
        result = json.loads(out.stdout)['result']
        assert not result.get('isError'), result
        return result['content'][0]['text']

    def route(self, name, wait=4500):
        self.tool('look', view={'route': name})
        self.page.wait_for_timeout(wait)

    def zoom_in(self, times):
        for _ in range(times):
            self.page.locator('#zin').click()
            self.page.wait_for_timeout(350)
        self.page.wait_for_timeout(800)

    def around(self, at, left, up, width, height):
        """The part of the page around box `at`: its centre `left` and `up` from the clip's corner."""
        box = self.page.locator(f'#scene g[data-id="{at}"]').first.bounding_box()
        x, y = box['x'] + box['width'] / 2, box['y'] + box['height'] / 2
        return {'x': x - left, 'y': y - up, 'width': width, 'height': height}

    def shot(self, name, theme, clip):
        self.page.screenshot(path=str(OUT / f'{name}-{theme}.png'), clip=clip)

    def close(self):
        assert not self.errors, self.errors
        self.context.close()
        self.server.terminate()


def pointing(atlas, browser, theme, tmp):
    """Where does the smoothing happen: the agent points at `smooth`; a note on it; the agent's answer."""
    s = Session(atlas, scratch(tmp / f'point-{theme}', markers=False), browser, theme)
    at = 'sunspots.series.smooth'
    s.tool('look', view={'route': 'sunspots.cli.run'}, at=at)
    s.page.wait_for_selector('#scene .pointed', timeout=8000)
    s.page.wait_for_timeout(2600)
    s.zoom_in(2)
    # the second pointing finds the page in its view, and only the pulse moves; the page takes the
    # pulse away after its two beats, so each frame puts it back and holds it at its moment
    s.tool('look', at=at)
    s.page.wait_for_selector('#scene .pointed', timeout=8000)
    duration = s.page.evaluate("""() => { window.held = document.querySelector('#scene .pointed');
      const a = window.held.getAnimations()[0]; a.pause(); return a.effect.getTiming().duration; }""")
    clip = s.around(at, 342, 114, 760, 190)
    frames = []
    for i in range(FRAMES):
        s.page.evaluate("""t => { const g = window.held; g.classList.add('pointed');
          const a = g.getAnimations()[0]; a.pause(); a.currentTime = t; }""", duration * i / FRAMES)
        shot = tmp / f'look-{theme}-{i:02}.png'
        s.page.screenshot(path=str(shot), clip=clip)
        frames.append(Image.open(shot).convert('RGB'))
    frames[0].save(OUT / f'look-{theme}.webp', save_all=True, append_images=frames[1:],
                   duration=[REST] + [round(duration / FRAMES)] * (FRAMES - 1), loop=0, quality=90, method=6)
    s.page.evaluate("() => window.held.classList.remove('pointed')")
    # the person's note on the box
    s.page.locator(f'#scene g[data-id="{at}"]').first.click(modifiers=['Alt'])
    s.page.wait_for_timeout(400)
    s.page.keyboard.type(NOTE)
    s.page.wait_for_timeout(300)
    s.shot('note', theme, s.around(at, 250, 100, 760, 260))
    s.page.keyboard.press('Enter')
    s.page.wait_for_timeout(3500)
    # the agent reads it, answers on the box, and resolves it
    assert NOTE in s.tool('notes')
    s.tool('annotate', on=at, text=ANSWER)
    s.tool('resolve', id=1)
    s.page.wait_for_timeout(3500)
    s.page.locator('body').click(position={'x': 5, 'y': 700})
    s.page.locator(f'#scene g[data-id="{at}"] g.annot').first.hover()
    s.page.wait_for_timeout(700)
    s.shot('answer', theme, s.around(at, 250, 100, 760, 260))
    s.close()


def designing(atlas, browser, theme, tmp):
    """Cycles' maxima: the vision, the prophecy it is decided into, and the design built."""
    repo = scratch(tmp / f'design-{theme}', markers=True)
    s = Session(atlas, repo, browser, theme)
    src = repo / 'src/sunspots'
    series, plot, cli = (src / 'series.py').read_text(), (src / 'plot.py').read_text(), (src / 'cli.py').read_text()
    imports = ('from sunspots.plot import Trend\nfrom sunspots.series import load, smooth\n',
               'from sunspots.plot import Cycles, Trend\nfrom sunspots.series import load, peaks, smooth\n')
    vision = {'src/sunspots/series.py': series + PEAKS % '...',
              'src/sunspots/plot.py': plot + CYCLES % '...',
              'src/sunspots/cli.py': cli.replace(*imports) + RUN_CYCLES}
    at = 'sunspots.series.peaks'
    clip = lambda: s.around(at, 440, 135, 760, 270)
    s.tool('vision', title='Find the maximum of each solar cycle and mark it on the figure', files=vision)
    s.page.wait_for_timeout(5000)
    s.route('sunspots.cli.run_cycles')
    s.zoom_in(1)
    s.shot('vision', theme, clip())
    s.tool('prophetize', supersedes={'sunspots.cli.run_cycles': 'sunspots.cli.run'})
    s.page.wait_for_timeout(7000)
    s.route('sunspots.cli.run_cycles')
    s.zoom_in(1)
    s.shot('prophecy', theme, clip())
    assert '3 prophecies' in s.tool('revelation')
    # built: the bodies written, the markers and their import gone
    for f, old, new in [('series.py', 'raise NotImplementedError', PEAKS_BODY),
                        ('plot.py', '        ...\n        return figure', f'        {CYCLES_BODY}\n        return figure')]:
        (src / f).write_text((src / f).read_text().replace(old, new, 1))
    for f in ['series.py', 'plot.py', 'cli.py']:
        text = (src / f).read_text()
        text = re.sub(r'\n?from sunspots\.markers import prophecy\n', '\n', text)
        text = re.sub(r'@prophecy\([^)]*\)\n', '', text)
        (src / f).write_text(text)
    git(repo, 'commit', '-qam', 'Mark the maxima of the cycles')
    s.page.wait_for_timeout(7000)
    s.route('sunspots.cli.run_cycles')
    s.zoom_in(1)
    s.shot('built', theme, clip())
    assert '0 prophecies' in s.tool('revelation')
    s.close()


def main(atlas):
    atlas = str(pathlib.Path(atlas).resolve())
    OUT.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as tmp, sync_playwright() as p:
        browser = p.chromium.launch(channel='chrome')
        for theme in THEMES:
            pointing(atlas, browser, theme, pathlib.Path(tmp))
            designing(atlas, browser, theme, pathlib.Path(tmp))
            print(theme, 'drawn')
        browser.close()


if __name__ == '__main__':
    sys.exit(main(sys.argv[1]))
