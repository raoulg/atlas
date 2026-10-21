# /// script
# requires-python = ">=3.11"
# dependencies = ["playwright"]
# ///
"""Draw the history of the tour on `docs/index.html` with an Atlas build: the commits of a
repository as `atlas serve` lists them, with two of them drawn as one picture beside the list.

    uv run tools/interaction_pictures.py ATLAS [OUTDIR]

One subject, the sunspots pipeline of `engine/tests/pairs/pipeline/python-clean`, is grown in a
scratch repository as six commits that each add a step. `atlas serve` serves it, and its history page
is driven in the installed Chrome as a person drives it: the fifth commit and the sixth picked, the
route `run` chosen. Nothing is drawn by hand: each picture is a screenshot of the page.
"""

import json
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile

from playwright.sync_api import sync_playwright

sys.dont_write_bytecode = True
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from docs_pictures import CAPTURE, canonical, snapshot

ROOT = pathlib.Path(__file__).resolve().parents[1]
DOCS = ROOT / 'docs'
DRAWN = DOCS / 'img/interaction/drawn.json'
SUBJECT = ROOT / 'engine/tests/pairs/pipeline/python-clean'
# the history picture: how far it is zoomed in, and where Settings and the top of the route land on the page
ZOOM, LEFT, TOP = 2, 470, 150
TYPINGS = ROOT / 'engine/tests/pairs/pipeline/typings'


def git(repo, *args, when='2026-10-12T10:00:00'):
    env = {'GIT_AUTHOR_NAME': 'Raoul', 'GIT_AUTHOR_EMAIL': 'raoul@example.com',
           'GIT_COMMITTER_NAME': 'Raoul', 'GIT_COMMITTER_EMAIL': 'raoul@example.com',
           'GIT_AUTHOR_DATE': when, 'GIT_COMMITTER_DATE': when,
           'PATH': '/usr/bin:/bin:/usr/local/bin:/opt/homebrew/bin'}
    return subprocess.run(['git', *args], cwd=repo, env=env, check=True, capture_output=True, text=True).stdout


DOC = '"""Sunspots: SILSO\'s monthly mean sunspot number, downloaded, smoothed and drawn."""\n'


def init(*imports):
    """`__init__.py` as it stands once its modules hold these names: [(module, [names])]."""
    lines = [DOC]
    names = []
    for module, ns in imports:
        lines.append(f'from sunspots.{module} import {", ".join(ns)}')
        names += ns
    body = lines[0] + '\n' + '\n'.join(lines[1:]) + '\n\n'
    return body + '__all__ = [' + ', '.join(f'"{n}"' for n in sorted(names)) + ']\n'


SERIES_HEAD = '''"""The monthly mean sunspot number as a frame indexed by month.

SILSO's file has no header; its columns are separated by semicolons, and a month without an
observation has `-1` for its mean.
"""

from pathlib import Path

import pandas as pd

COLUMNS = ["year", "month", "decimal_date", "mean", "std", "observations", "definitive"]
MISSING = -1
'''

LOAD = '''

def load(path: Path) -> pd.DataFrame:
    """The months in SILSO's file at `path` that have a mean, indexed by their first day."""
    table = pd.read_csv(path, sep=";", header=None, names=COLUMNS, skipinitialspace=True, na_values=[MISSING])
    first_days = pd.to_datetime(table[["year", "month"]].assign(day=1))
    return table.set_index(first_days)[["mean", "std", "observations"]].dropna(subset=["mean"])
'''

TRUSTED = '''

def trusted(frame: pd.DataFrame, minimum: int) -> pd.DataFrame:
    """The months with at least `minimum` observations."""
    return frame[frame["observations"] >= minimum]
'''

SMOOTH = '''

def smooth(frame: pd.DataFrame, months: int) -> pd.DataFrame:
    """`frame` with a `smoothed` column: the mean over `months` months, centred on each."""
    return frame.assign(smoothed=frame["mean"].rolling(months, center=True).mean())
'''

PEAKS = '''

def peaks(frame: pd.DataFrame, years: int) -> pd.DataFrame:
    """`frame` with a `peak` column: whether a month holds the highest smoothed mean of the `years` years round it."""
    highest = frame["smoothed"].rolling(years * 12 + 1, center=True).max()
    return frame.assign(peak=frame["smoothed"] == highest)
'''

SETTINGS_HEAD = '''"""The choices of a run that are not places: how far to smooth and how the figure looks."""

from dataclasses import dataclass


@dataclass(frozen=True)
class Settings:
'''
SETTINGS_LOOKS = '''    title: str = "Monthly mean total sunspot number"
    size: tuple[float, float] = (10.0, 4.0)
    dpi: int = 150
'''

PLOT_HEAD = '''"""Figures of a sunspot series: each kind of plot draws a figure, and every plot saves one."""

from abc import ABC, abstractmethod
from pathlib import Path

import pandas as pd
from matplotlib.figure import Figure

from sunspots.settings import Settings


class Plot(ABC):
    """A figure drawn from a frame and saved as an image."""

    def __init__(self, settings: Settings) -> None:
        self.settings = settings

    @abstractmethod
    def draw(self, frame: pd.DataFrame) -> Figure: ...

    def save(self, frame: pd.DataFrame, to: Path) -> None:
        """Draw `frame` and write the figure to `to`."""
        figure = self.draw(frame)
        to.parent.mkdir(parents=True, exist_ok=True)
        figure.savefig(to, dpi=self.settings.dpi)
'''

TREND_MEANS = '''

class Trend(Plot):
    """The monthly means as a thin line%s."""

    def draw(self, frame: pd.DataFrame) -> Figure:
        figure = Figure(figsize=self.settings.size, layout="constrained")
        ax = figure.add_subplot()
        ax.plot(frame.index, frame["mean"], color="0.7", linewidth=0.6, label="monthly mean")
%s        ax.set_title(self.settings.title)
        ax.set_ylabel("sunspot number")
        ax.legend()
        return figure
'''
TREND_SMOOTHED = '        ax.plot(frame.index, frame["smoothed"], color="C0", label=f"{self.settings.months}-month mean")\n'

CYCLES = '''

class Cycles(Plot):
    """The smoothed mean as a line, with the maximum of each cycle marked."""

    def draw(self, frame: pd.DataFrame) -> Figure:
        figure = Figure(figsize=self.settings.size, layout="constrained")
        ax = figure.add_subplot()
        ax.plot(frame.index, frame["smoothed"], color="C0")
        tops = frame[frame["peak"]]
        ax.plot(tops.index, tops["smoothed"], "o", color="C3", label="maximum of a cycle")
        ax.set_title("The maxima of the solar cycles")
        ax.set_ylabel("sunspot number")
        ax.legend()
        return figure
'''

CLI_HEAD = '''"""The command line: a run names the source and the files, and hands them down."""

from pathlib import Path

import typer

%s

SILSO = "https://www.sidc.be/SILSO/DATA/SN_m_tot_V2.0.csv"
RAW = Path("data/raw/sunspots.csv")
%s
app = typer.Typer()


@app.command()
'''


def stage(repo, files):
    """The files of one commit, whole: what the commit does not name is what it has not got yet."""
    src = repo / 'src/sunspots'
    for name in src.glob('*.py'):
        name.unlink()
    for name, text in files.items():
        (src / name).write_text(text)


def history():
    """The sunspots pipeline as six commits that each add a step, as a pipeline grows: the files of each."""
    download = (SUBJECT / 'src/sunspots/download.py').read_text()
    commits = []
    # 1 the download
    commits.append(('Download SILSO\'s monthly sunspot numbers', {
        'download.py': download,
        '__init__.py': init(('download', ['download'])),
        'cli.py': CLI_HEAD % ('from sunspots.download import download', '') + '''def run(raw: Path = RAW) -> None:
    """Download SILSO's monthly sunspot numbers where none are kept."""
    if not raw.exists():
        download(SILSO, raw)
'''}))
    # 2 the preprocessing
    s2 = {**commits[0][1], 'series.py': SERIES_HEAD + LOAD,
          '__init__.py': init(('download', ['download']), ('series', ['load']))}
    s2['cli.py'] = CLI_HEAD % ('from sunspots.download import download\nfrom sunspots.series import load', '') + '''def run(raw: Path = RAW) -> None:
    """Download SILSO's monthly sunspot numbers where none are kept, and describe the months that have a mean."""
    if not raw.exists():
        download(SILSO, raw)
    typer.echo(load(raw).describe().to_string())
'''
    commits.append(('Load the months that have a mean', s2))
    # 3 the first figure
    s3 = {**s2, 'settings.py': SETTINGS_HEAD + SETTINGS_LOOKS, 'plot.py': PLOT_HEAD + TREND_MEANS % ('', ''),
          '__init__.py': init(('download', ['download']), ('plot', ['Plot', 'Trend']), ('series', ['load']), ('settings', ['Settings']))}
    s3['cli.py'] = CLI_HEAD % ('from sunspots.download import download\nfrom sunspots.plot import Trend\nfrom sunspots.series import load\nfrom sunspots.settings import Settings',
                               'FIGURE = Path("figures/sunspots.png")\n') + '''def run(raw: Path = RAW, figure: Path = FIGURE) -> None:
    """Download SILSO's monthly sunspot numbers where none are kept and save a figure of them."""
    if not raw.exists():
        download(SILSO, raw)
    Trend(Settings()).save(load(raw), figure)
'''
    commits.append(('Draw the monthly means', s3))
    # 4 the smoothing
    s4 = {**s3, 'series.py': SERIES_HEAD + LOAD + SMOOTH,
          'settings.py': SETTINGS_HEAD + '    months: int = 13\n' + SETTINGS_LOOKS,
          'plot.py': PLOT_HEAD + TREND_MEANS % (', with their smoothed mean over them', TREND_SMOOTHED),
          '__init__.py': init(('download', ['download']), ('plot', ['Plot', 'Trend']), ('series', ['load', 'smooth']), ('settings', ['Settings']))}
    s4['cli.py'] = CLI_HEAD % ('from sunspots.download import download\nfrom sunspots.plot import Trend\nfrom sunspots.series import load, smooth\nfrom sunspots.settings import Settings',
                               'FIGURE = Path("figures/sunspots.png")\n') + '''def run(raw: Path = RAW, figure: Path = FIGURE, months: int = 13) -> None:
    """Download SILSO's monthly sunspot numbers where none are kept, smooth them and save their trend."""
    settings = Settings(months=months)
    if not raw.exists():
        download(SILSO, raw)
    frame = smooth(load(raw), settings.months)
    Trend(settings).save(frame, figure)
'''
    commits.append(('Smooth them with a 13-month mean', s4))
    # 5 the filter
    s5 = {**s4, 'series.py': SERIES_HEAD + LOAD + TRUSTED + SMOOTH,
          'settings.py': SETTINGS_HEAD + '    months: int = 13\n    minimum: int = 1\n' + SETTINGS_LOOKS,
          '__init__.py': init(('download', ['download']), ('plot', ['Plot', 'Trend']), ('series', ['load', 'smooth', 'trusted']), ('settings', ['Settings']))}
    s5['cli.py'] = CLI_HEAD % ('from sunspots.download import download\nfrom sunspots.plot import Trend\nfrom sunspots.series import load, smooth, trusted\nfrom sunspots.settings import Settings',
                               'FIGURE = Path("figures/sunspots.png")\n') + '''def run(raw: Path = RAW, figure: Path = FIGURE, months: int = 13, minimum: int = 1) -> None:
    """Download SILSO's monthly sunspot numbers where none are kept, smooth them and save their trend."""
    settings = Settings(months=months, minimum=minimum)
    if not raw.exists():
        download(SILSO, raw)
    frame = smooth(trusted(load(raw), settings.minimum), settings.months)
    Trend(settings).save(frame, figure)
'''
    commits.append(('Leave out the months with too few observations', s5))
    # 6 the cycles
    s6 = {**s5, 'series.py': SERIES_HEAD + LOAD + TRUSTED + SMOOTH + PEAKS,
          'settings.py': SETTINGS_HEAD + '    months: int = 13\n    minimum: int = 1\n    cycle: int = 11\n' + SETTINGS_LOOKS,
          'plot.py': PLOT_HEAD + TREND_MEANS % (', with their smoothed mean over them', TREND_SMOOTHED) + CYCLES,
          '__init__.py': init(('download', ['download']), ('plot', ['Cycles', 'Plot', 'Trend']),
                              ('series', ['load', 'peaks', 'smooth', 'trusted']), ('settings', ['Settings']))}
    s6['cli.py'] = CLI_HEAD % ('from sunspots.download import download\nfrom sunspots.plot import Cycles, Trend\nfrom sunspots.series import load, peaks, smooth, trusted\nfrom sunspots.settings import Settings',
                               'FIGURE = Path("figures/sunspots.png")\nCYCLES = Path("figures/cycles.png")\n') + '''def run(raw: Path = RAW, figure: Path = FIGURE, cycles: Path = CYCLES, months: int = 13, minimum: int = 1) -> None:
    """Download SILSO's monthly sunspot numbers where none are kept, smooth them, save their trend and the maxima of their cycles."""
    settings = Settings(months=months, minimum=minimum)
    if not raw.exists():
        download(SILSO, raw)
    frame = smooth(trusted(load(raw), settings.minimum), settings.months)
    Trend(settings).save(frame, figure)
    Cycles(settings).save(peaks(frame, settings.cycle), cycles)
'''
    commits.append(('Mark the maxima of the cycles', s6))
    return commits


def scratch(tmp):
    """The sunspots pipeline as a repository of its own, beside the stubs of the libraries it imports, grown in
    six commits; and the commits' names by their message."""
    base = tmp / 'scratch'
    repo = base / 'sunspots'
    shutil.copytree(SUBJECT, repo, ignore=shutil.ignore_patterns('__pycache__', '.venv', 'src'))
    (repo / 'src/sunspots').mkdir(parents=True)
    # the stubs of the libraries are the repository's own, so that every commit reads with them
    shutil.copytree(TYPINGS, repo / 'typings')
    pyproject = repo / 'pyproject.toml'
    pyproject.write_text(pyproject.read_text().replace('"../typings"', '"typings"'))
    stub = repo / 'typings/pandas/__init__.pyi'
    stub.write_text(stub.read_text().replace('    def mean(self) -> Series: ...', '    def mean(self) -> Series: ...\n    def max(self) -> Series: ...'))
    git(repo, 'init', '-q', '-b', 'main')
    shas = {}
    for i, (message, files) in enumerate(history()):
        stage(repo, files)
        git(repo, 'add', '-A')
        git(repo, 'commit', '-q', '-m', message, when=f'2026-10-12T1{i}:00:00')
        shas[message] = git(repo, 'rev-parse', '--short', 'HEAD').strip()
    return repo, shas


class Served:
    """`atlas serve`, and the address it serves at."""

    def __init__(self, atlas, repo):
        self.proc = subprocess.Popen([atlas, 'serve', '--no-open', '--port', '0', str(repo)],
                                     stderr=subprocess.PIPE, text=True)
        for line in self.proc.stderr:
            m = re.search(r'http://(\S+?)/?\s*$', line)
            if m:
                self.at = m.group(1)
                break
        else:
            raise RuntimeError('atlas serve did not say where it serves')

    def url(self, path='/'):
        return f'http://{self.at}{path}'

    def stop(self):
        self.proc.terminate()
        self.proc.wait()


def settle(page, ms=1500):
    page.wait_for_timeout(ms)
    assert page.locator('#fault').is_hidden(), page.locator('#fault').inner_text()


def main(atlas, out=None):
    """With an OUTDIR the pictures are written there; without, the pictures of `docs/`."""
    atlas = str(pathlib.Path(atlas).resolve())
    shown = {}
    to_docs = out is None
    with tempfile.TemporaryDirectory() as tmp, sync_playwright() as p:
        tmp = pathlib.Path(tmp)
        out = pathlib.Path(out) if out else pathlib.Path(tempfile.mkdtemp(prefix='atlas-interaction-'))
        out.mkdir(parents=True, exist_ok=True)
        repo, _ = scratch(tmp)
        server = Served(atlas, repo)
        browser = p.chromium.launch(channel='chrome')
        ctx = browser.new_context(viewport={'width': 2000, 'height': 520}, device_scale_factor=2, color_scheme='light',
                                  reduced_motion='reduce')
        ctx.add_init_script('(' + CAPTURE + ')()')
        page = ctx.new_page()
        page.set_default_timeout(15000)
        errors = []
        page.on('pageerror', lambda e: errors.append(str(e)))

        # the fifth commit against the sixth, as one picture
        page.goto(server.url('/history'), wait_until='networkidle')
        settle(page, 2000)
        page.get_by_text('Leave out the months with too few').first.click()
        page.get_by_text('Mark the maxima of the cycles').first.click(modifiers=['Shift'])
        settle(page, 4000)
        view = page.frame_locator('iframe')
        routes = view.locator('#entry').evaluate("s => [...s.options].map(o => [o.value, o.textContent])")
        pick = next((v for v, label in routes if re.search(r'\brun\b', label) and not v.startswith(('kind:', 'part:'))), None)
        assert pick, routes
        view.locator('#entry').select_option(pick)
        page.wait_for_timeout(2500)
        shown['history'] = canonical(snapshot(next(f for f in page.frames if f != page.main_frame)))
        view.get_by_role('button', name='fit', exact=True).click()
        page.wait_for_timeout(900)
        # in on what the sixth commit adds: from Settings to the figures, just under the bar with A ∪ B
        for _ in range(ZOOM):
            view.get_by_role('button', name='+', exact=True).click()
            page.wait_for_timeout(400)
        boxes = view.locator('#scene g.tstep, #scene g.tnode, #scene g.wnode')
        settings = view.locator('#scene g.tnode', has_text='Settings · settings').first.bounding_box()
        top = min(b['y'] for b in boxes.evaluate_all('gs => gs.map(g => { const r = g.getBoundingClientRect(); return {y: r.y}; })'))
        page.mouse.move(1200, 300)
        page.mouse.down()
        page.mouse.move(1200 - (settings['x'] - LEFT), 300 - (top - TOP), steps=12)
        page.mouse.up()
        page.wait_for_timeout(900)
        frame = next(f for f in page.frames if f != page.main_frame)
        drawn = frame.evaluate("""() => [...document.querySelectorAll('#scene g.tstep, #scene g.tnode, #scene g.wnode')]
          .map(g => { const r = g.getBoundingClientRect(); return {right: r.right, bottom: r.bottom}; })""")
        origin = page.locator('iframe').bounding_box()
        right, bottom = origin['x'] + max(r['right'] for r in drawn), origin['y'] + max(r['bottom'] for r in drawn)
        # the page as it is: the commits on the left, the two drawn as one on the right, up to the figures
        page.screenshot(path=str(out / 'history.png'), clip={'x': 0, 'y': 0, 'width': right + 30, 'height': bottom + 36})
        assert not errors, errors
        server.stop()
        browser.close()
    status = publish(out, shown, to_docs)
    if to_docs:
        shutil.rmtree(out, ignore_errors=True)
    return status


def publish(out, shown, to_docs):
    """The pictures into `docs/`, and what each draws kept: a picture that draws otherwise than it did
    is named, and the text beside it is to be reread."""
    if not to_docs:
        (out / 'drawn.json').write_text(json.dumps(shown, indent=1) + '\n')
        print(out)
        return 0
    kept = json.loads(DRAWN.read_text()) if DRAWN.exists() else {}
    for png in out.glob('*.png'):
        shutil.copy(png, DRAWN.parent / png.name)
    DRAWN.write_text(json.dumps(shown, indent=1) + '\n')
    changed = [n for n, v in shown.items() if kept.get(n) != v]
    if not changed:
        print('every picture draws what it drew')
        return 0
    print('\nThese pictures draw otherwise; reread the text beside them in docs/index.html:')
    for n in changed:
        print(f'  {n}')
    return 0 if not kept else 1


if __name__ == '__main__':
    sys.exit(main(*sys.argv[1:]))
