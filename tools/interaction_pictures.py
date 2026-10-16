# /// script
# requires-python = ">=3.11"
# dependencies = ["playwright"]
# ///
"""Draw the interaction between a person and an agent with an Atlas build: the pictures and texts of
the tour on `docs/index.html`, from real views.

    uv run tools/interaction_pictures.py ATLAS [OUTDIR]

One subject, the sunspots pipeline of `engine/tests/pairs/pipeline/python-clean`, is copied into a
scratch repository and gets a commit of its own. `atlas serve` serves it, `atlas mcp` is called as an
agent calls it (JSON-RPC over stdio), and the page is driven in the installed Chrome as a person
drives it: clicks, a note written on a box. Nothing is drawn by hand: each picture is a screenshot
of the page after what the person or the agent did, and each text is what a tool answered.
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
TYPINGS = ROOT / 'engine/tests/pairs/pipeline/typings'


def git(repo, *args, when='2026-10-12T10:00:00'):
    env = {'GIT_AUTHOR_NAME': 'Raoul', 'GIT_AUTHOR_EMAIL': 'raoul@example.com',
           'GIT_COMMITTER_NAME': 'Raoul', 'GIT_COMMITTER_EMAIL': 'raoul@example.com',
           'GIT_AUTHOR_DATE': when, 'GIT_COMMITTER_DATE': when,
           'PATH': '/usr/bin:/bin:/usr/local/bin:/opt/homebrew/bin'}
    return subprocess.run(['git', *args], cwd=repo, env=env, check=True, capture_output=True, text=True).stdout


MARKER = '''

def prophecy(**_: str) -> Callable[[T], T]:
    """Marks sketch code that is not built yet; Atlas reads the mark by its name."""

    def mark(item: T) -> T:
        return item

    return mark
'''

DOC = '"""Sunspots: SILSO\'s monthly mean sunspot number, downloaded, smoothed and drawn."""\n'


def init(*imports):
    """`__init__.py` as it stands once its modules hold these names: [(module, [names])]."""
    lines = [DOC, 'from typing import Callable, TypeVar\n']
    names = ['prophecy']
    for module, ns in imports:
        lines.append(f'from sunspots.{module} import {", ".join(ns)}')
        names += ns
    body = '\n'.join(lines[:2]) + '\n' + '\n'.join(lines[2:]) + '\n\nT = TypeVar("T")\n\n'
    return body + '__all__ = [' + ', '.join(f'"{n}"' for n in sorted(names)) + ']\n' + MARKER


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


class Agent:
    """`atlas mcp`, called as a coding agent calls it."""

    def __init__(self, atlas, repo):
        self.atlas, self.repo = atlas, repo

    def tool(self, name, **args):
        args['path'] = str(self.repo)
        call = {'jsonrpc': '2.0', 'id': 1, 'method': 'tools/call', 'params': {'name': name, 'arguments': args}}
        out = subprocess.run([self.atlas, 'mcp'], input=json.dumps(call) + '\n', capture_output=True, text=True, check=True)
        reply = json.loads(out.stdout)
        text = reply['result']['content'][0]['text']
        assert not reply['result'].get('isError'), text
        return text


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


SERIES_VISION = """

class Archive:
    \"\"\"The smoothed months kept as a file: what goes in comes out.\"\"\"

    def __init__(self, path: Path) -> None: ...

    def write(self, frame: pd.DataFrame) -> None: ...

    def read(self) -> pd.DataFrame: ...
"""

CLI_VISION = """

@app.command()
def run_archived(raw: Path = RAW, figure: Path = FIGURE, archive: Path = Path("data/smoothed.csv"), months: int = 13, minimum: int = 1) -> None:
    \"\"\"Like `run`, and keep the smoothed months in a file.\"\"\"
    settings = Settings(months=months, minimum=minimum)
    frame = smooth(trusted(load(raw), settings.minimum), settings.months)
    Archive(archive).write(frame)
    Trend(settings).save(frame, figure)
"""

SKETCH = SERIES_VISION.strip('\n') + '\n\n\n' + CLI_VISION.strip('\n') + '\n'


def vision_files(repo):
    series = (repo / 'src/sunspots/series.py').read_text() + SERIES_VISION
    cli = (repo / 'src/sunspots/cli.py').read_text().replace('import load, peaks, smooth, trusted', 'import Archive, load, peaks, smooth, trusted')
    return {'src/sunspots/series.py': series, 'src/sunspots/cli.py': cli + CLI_VISION}


SEALED = """

@prophecy(sealed="raoulg/atlas#19")
class Format:
    \"\"\"The file the archive keeps its months in: what it is waits on a question.\"\"\"


@prophecy(sealed="raoulg/atlas#33")
class Compression:
    \"\"\"Whether the archive is compressed.\"\"\"
"""


BUILT_ARCHIVE = """class Archive:
    \"\"\"The smoothed months kept as a file: what goes in comes out.\"\"\"

    def __init__(self, path: Path) -> None:
        self.path = path

    def write(self, frame: pd.DataFrame) -> None:
        self.path.parent.mkdir(parents=True, exist_ok=True)
        frame.to_csv(self.path)

    def read(self) -> pd.DataFrame:
        return pd.read_csv(self.path, index_col=0, parse_dates=True)

"""


def build(repo):
    """The sketch built: Archive gets its bodies, run_archived loses its marker."""
    series = repo / 'src/sunspots/series.py'
    text = series.read_text()
    text, n = re.subn(r'@prophecy\nclass Archive:.*?(?=\n\n@prophecy|\Z)', BUILT_ARCHIVE.rstrip('\n') + '\n', text, flags=re.S)
    assert n == 1, 'the Archive sketch is not where it was'
    series.write_text(text)
    cli = repo / 'src/sunspots/cli.py'
    text, n = re.subn(r'@prophecy\(supersedes="sunspots.cli.run"\)\n', '', cli.read_text())
    assert n == 1, 'the run_archived marker is not where it was'
    cli.write_text(text)


def settle(page, ms=1500):
    page.wait_for_timeout(ms)
    assert page.locator('#fault').is_hidden(), page.locator('#fault').inner_text()


def fit(page):
    page.get_by_role('button', name='fit', exact=True).click()
    page.wait_for_timeout(700)


def block_of(text, name):
    """The lines of the item `name` in a file, its decorators first."""
    lines = text.split('\n')
    at = next(i for i, l in enumerate(lines) if re.match(rf'\s*(def|class) {name}\b', l))
    start = at
    while lines[start - 1].lstrip().startswith('@'):
        start -= 1
    end = at + 1
    while end < len(lines) and (not lines[end] or lines[end][0] in ' \t'):
        end += 1
    return '\n'.join(lines[start:end]).rstrip('\n')


def revelation_cli(atlas, repo):
    """What `atlas revelation` prints in a terminal."""
    out = subprocess.run([atlas, 'revelation', str(repo)], capture_output=True, text=True, check=True).stdout
    return '$ atlas revelation\n' + out.rstrip('\n')


def main(atlas, out=None):
    """With an OUTDIR the pictures and texts are written there; without, the page and pictures of `docs/`."""
    atlas = str(pathlib.Path(atlas).resolve())
    texts, shown, tour = {}, {}, {}
    to_docs = out is None
    with tempfile.TemporaryDirectory() as tmp, sync_playwright() as p:
        tmp = pathlib.Path(tmp)
        out = pathlib.Path(out) if out else pathlib.Path(tempfile.mkdtemp(prefix='atlas-interaction-'))
        out.mkdir(parents=True, exist_ok=True)
        repo, shas = scratch(tmp)
        agent = Agent(atlas, repo)
        server = Served(atlas, repo)
        browser = p.chromium.launch(channel='chrome')
        ctx = browser.new_context(viewport={'width': 1400, 'height': 560}, device_scale_factor=2, color_scheme='light',
                                  reduced_motion='reduce')
        ctx.add_init_script('(' + CAPTURE + ')()')
        page = ctx.new_page()
        page.set_default_timeout(15000)
        errors = []
        page.on('pageerror', lambda e: errors.append(str(e)))

        def shot(name, around=None, pad=(330, 330, 80, 80), record=True):
            """A screenshot of the page: whole, or around a box, for what is small in the whole."""
            clip = None
            if around:
                box = page.locator(f'#scene g[data-id="{around}"]').first.bounding_box()
                x, y = box['x'] + box['width'] / 2, box['y'] + box['height'] / 2
                left, right, up, down = pad
                clip = {'x': max(0, x - left), 'y': max(0, y - up), 'width': left + right, 'height': up + down}
            page.screenshot(path=str(out / f'{name}.png'), clip=clip)
            if record:
                shown[name] = canonical(snapshot(page))

        def route(name):
            """The page in the route `name`, drawn fresh."""
            page.goto(server.url(), wait_until='networkidle')
            settle(page)
            agent.tool('look', view={'route': name})
            settle(page, 4500)

        # history: six commits; the fifth against the sixth, as one picture
        page.goto(server.url('/history'), wait_until='networkidle')
        settle(page, 2000)
        page.set_viewport_size({'width': 2000, 'height': 520})
        page.get_by_text('Leave out the months with too few').first.click()
        page.get_by_text('Mark the maxima of the cycles').first.click(modifiers=['Shift'])
        settle(page, 4000)
        view = page.frame_locator('iframe')
        routes = view.locator('#entry').evaluate("s => [...s.options].map(o => [o.value, o.textContent])")
        texts['history-routes'] = json.dumps(routes)
        pick = next((v for v, label in routes if re.search(r'\brun\b', label) and not v.startswith(('kind:', 'part:'))), None)
        assert pick, routes
        view.locator('#entry').select_option(pick)
        page.wait_for_timeout(2500)
        # the commits as the page lists them, then the picture alone, as wide as the page is
        page.screenshot(path=str(out / 'history-list.png'), clip={'x': 0, 'y': 50, 'width': 420, 'height': 330})
        view.get_by_role('button', name='fit', exact=True).click()
        page.wait_for_timeout(900)
        # in on the new path, so that its text reads
        for _ in range(2):
            view.get_by_role('button', name='+', exact=True).click()
            page.wait_for_timeout(400)
        page.mouse.move(1500, 230)
        page.mouse.down()
        page.mouse.move(1235, 150, steps=12)
        page.mouse.up()
        page.wait_for_timeout(900)
        print('shot history', file=sys.stderr, flush=True)
        page.screenshot(path=str(out / 'history.png'), clip={'x': 420, 'y': 0, 'width': 1580, 'height': 520})
        tour['commits'] = list(shas)

        # today: the route as it stands, before anyone points at anything
        page.set_viewport_size({'width': 1400, 'height': 560})
        route('sunspots.cli.run')
        shot('today')

        # look: the agent takes the page to a view and points at a box; the page lights the box in a pulse,
        # and the picture is taken at its second peak
        route('sunspots.cli.run')
        texts['look'] = agent.tool('look', view={'route': 'sunspots.cli.run'}, at='sunspots.series.trusted')
        settle(page, 3500)
        # in on the box, and the agent points again: the page keeps its view and the box pulses once more
        for _ in range(5):
            page.get_by_role('button', name='+', exact=True).click()
            page.wait_for_timeout(350)
        page.wait_for_timeout(800)
        agent.tool('look', at='sunspots.series.trusted')
        page.wait_for_selector('#scene .pointed', timeout=5000)
        page.wait_for_timeout(1500)
        shot('look', around='sunspots.series.trusted', pad=(460, 460, 190, 140))

        tour['calls'] = {
            'look': 'atlas · look\n  view: route sunspots.cli.run\n  at: sunspots.series.trusted',
            'shown': 'atlas · shown',
            'notes': 'atlas · notes',
            'annotate': 'atlas · annotate\n  on: sunspots.series.trusted\n  text: "A minimum of 1 keeps every month with at least one observation. …"',
            'resolve': 'atlas · resolve\n  id: 1',
            'vision': 'atlas · vision\n  title: Keep the smoothed months in a file\n  files: src/sunspots/series.py, src/sunspots/cli.py',
            'prophetize': 'atlas · prophetize\n  supersedes: run_archived → run',
        }

        # shown: the person clicks a box; the agent reads which
        page.locator('#scene g[data-id="sunspots.series.trusted"]').first.click()
        settle(page)
        texts['shown'] = agent.tool('shown')

        # a note: the person writes it on a box; the agent reads it, answers on the box and resolves it
        page.locator('#scene g[data-id="sunspots.series.trusted"]').first.click(modifiers=['Alt'])
        page.wait_for_timeout(400)
        page.keyboard.type('Why a minimum of 1? What happens to the months with no observations?')
        shot('note-field', around='sunspots.series.trusted', pad=(300, 620, 100, 230), record=False)
        page.keyboard.press('Enter')
        page.wait_for_timeout(3500)
        texts['notes'] = agent.tool('notes')
        texts['annotate'] = agent.tool('annotate', on='sunspots.series.trusted',
                                      text="A minimum of 1 keeps every month with at least one observation.\nA month with none has no mean in SILSO's file (-1), and load has dropped it already: nothing is lost here.")
        texts['resolve'] = agent.tool('resolve', id=1, commit=git(repo, 'rev-parse', '--short', 'HEAD').strip())
        page.wait_for_timeout(3500)
        page.locator('#scene g[data-id="sunspots.series.trusted"] g.annot').first.hover()
        page.wait_for_timeout(600)
        shot('answered', around='sunspots.series.trusted', pad=(300, 620, 100, 230), record=False)

        # a vision: a change shown beside the code as it is, the new route pointed at
        texts['vision'] = agent.tool('vision', title='Keep the smoothed months in a file', files=vision_files(repo))
        page.wait_for_timeout(6000)
        agent.tool('look', view={'route': 'sunspots.cli.run_archived'})
        settle(page, 4500)
        shot('vision')
        tour['sketch'] = SKETCH
        tour['files-vision'] = '$ git status --short\n' + (git(repo, 'status', '--short').strip() or '(nothing: your files are as they were)')

        # a prophecy: the decided vision written into the tree, marked, with what the person adds by hand
        texts['prophetize'] = agent.tool('prophetize', supersedes={'sunspots.cli.run_archived': 'sunspots.cli.run'})
        tour['files-prophecy'] = '$ git status --short\n' + git(repo, 'status', '--short').rstrip()
        texts['diff'] = git(repo, 'diff')
        with (repo / 'src/sunspots/series.py').open('a') as f:
            f.write(SEALED)
        texts['check'] = agent.tool('check')
        texts['revelation'] = agent.tool('revelation')
        texts['seals'] = agent.tool('seals')
        assert 'closed: the seal can be broken' in texts['seals'], 'the seals picture needs gh and the network'
        page.wait_for_timeout(7000)
        route('sunspots.cli.run_archived')
        shot('prophecy')
        tour['revelation-decided'] = revelation_cli(atlas, repo)
        series, cli = (repo / 'src/sunspots/series.py').read_text(), (repo / 'src/sunspots/cli.py').read_text()
        tour['prophecy'] = '\n\n'.join([block_of(cli, 'run_archived'), block_of(series, 'Archive'), block_of(series, 'Compression')])
        texts['unseal'] = agent.tool('unseal', closed=True)
        texts['unseal-diff'] = git(repo, 'diff', 'src/sunspots/series.py')

        # built: the bodies are written and the markers go; what was a ghost is drawn as code
        build(repo)
        git(repo, 'commit', '-q', '-am', 'Keep the smoothed months in a file', when='2026-10-12T17:00:00')
        page.wait_for_timeout(7000)
        route('sunspots.cli.run_archived')
        shot('built')
        tour['files-built'] = '$ git log --oneline -1\n' + git(repo, 'log', '--oneline', '-1').strip()
        tour['revelation-built'] = revelation_cli(atlas, repo)
        assert not errors, errors
        server.stop()
        browser.close()
    for name, text in texts.items():
        (out / f'{name}.txt').write_text(text)
    tour['texts'] = {k: texts[k] for k in ('look', 'shown', 'notes', 'resolve', 'annotate', 'vision', 'prophetize')}
    status = publish(out, texts, shown, to_docs, tour)
    if to_docs:
        shutil.rmtree(out, ignore_errors=True)
    return status


def publish(out, texts, shown, to_docs, tour):
    """The page and its pictures into `docs/`, and what each picture draws kept: a picture that draws
    otherwise than it did is named, and the text beside it is to be reread."""
    (out / 'data.js').write_text('window.TOUR = ' + json.dumps(tour, indent=1) + ';\n')
    if not to_docs:
        (out / 'drawn.json').write_text(json.dumps(shown, indent=1) + '\n')
        print(out)
        return 0
    target = DOCS / 'img/interaction'
    target.mkdir(parents=True, exist_ok=True)
    kept = json.loads(DRAWN.read_text()) if DRAWN.exists() else {}
    for png in [*out.glob('*.png'), out / 'data.js']:
        shutil.copy(png, target / png.name)
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
