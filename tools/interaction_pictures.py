# /// script
# requires-python = ">=3.11"
# dependencies = ["playwright"]
# ///
"""Draw the interaction between a person and an agent with an Atlas build: the pictures and texts of
`docs/interaction.html`, from real views.

    uv run tools/interaction_pictures.py ATLAS [OUTDIR]

One subject, the sunspots pipeline of `engine/tests/pairs/pipeline/python-clean`, is copied into a
scratch repository and gets a commit of its own. `atlas serve` serves it, `atlas mcp` is called as an
agent calls it (JSON-RPC over stdio), and the page is driven in the installed Chrome as a person
drives it: clicks, a note written on a box. Nothing is drawn by hand: each picture is a screenshot
of the page after what the person or the agent did, and each text is what a tool answered.
"""

import html
import json
import pathlib
import re
import shutil
import struct
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


def git(repo, *args):
    env = {'GIT_AUTHOR_NAME': 'Raoul', 'GIT_AUTHOR_EMAIL': 'raoul@example.com',
           'GIT_COMMITTER_NAME': 'Raoul', 'GIT_COMMITTER_EMAIL': 'raoul@example.com',
           'GIT_AUTHOR_DATE': '2026-10-12T10:00:00', 'GIT_COMMITTER_DATE': '2026-10-12T10:00:00',
           'PATH': '/usr/bin:/bin:/usr/local/bin:/opt/homebrew/bin'}
    return subprocess.run(['git', *args], cwd=repo, env=env, check=True, capture_output=True, text=True).stdout


MARKER = '''

def prophecy(**_: str) -> Callable[[T], T]:
    """Marks sketch code that is not built yet; Atlas reads the mark by its name."""

    def mark(item: T) -> T:
        return item

    return mark
'''


def edit(repo, name, *changes):
    path = repo / 'src/sunspots' / name
    text = path.read_text()
    for old, new in changes:
        assert old in text, (name, old)
        text = text.replace(old, new, 1)
    path.write_text(text)


def scratch(tmp):
    """The sunspots pipeline as a repository of its own, beside the stubs of the libraries it imports,
    with two commits: the pipeline, and a filter on the months that have too few observations."""
    base = tmp / 'scratch'
    repo = base / 'sunspots'
    shutil.copytree(SUBJECT, repo, ignore=shutil.ignore_patterns('__pycache__', '.venv'))
    shutil.copytree(TYPINGS, base / 'typings')
    # the project's no-op marker for sketch code
    edit(repo, '__init__.py',
         ('from sunspots.download import download', 'from typing import Callable, TypeVar\n\nfrom sunspots.download import download'),
         ('__all__ = [', 'T = TypeVar("T")\n\n__all__ = ["prophecy", '))
    with (repo / 'src/sunspots/__init__.py').open('a') as f:
        f.write(MARKER)
    git(repo, 'init', '-q', '-b', 'main')
    git(repo, 'add', '-A')
    git(repo, 'commit', '-q', '-m', 'The sunspots pipeline')
    edit(repo, 'series.py', ('def smooth(', '''def trusted(frame: pd.DataFrame, minimum: int) -> pd.DataFrame:
    """The months with at least `minimum` observations."""
    return frame[frame["observations"] >= minimum]


def smooth('''))
    edit(repo, 'settings.py', ('    months: int = 13\n', '    months: int = 13\n    minimum: int = 1\n'))
    edit(repo, 'cli.py',
         ('import load, smooth', 'import load, smooth, trusted'),
         ('months: int = 13) -> None:', 'months: int = 13, minimum: int = 1) -> None:'),
         ('Settings(months=months)', 'Settings(months=months, minimum=minimum)'),
         ('smooth(load(raw), settings.months)', 'smooth(trusted(load(raw), settings.minimum), settings.months)'))
    git(repo, 'commit', '-q', '-am', 'Leave out the months with too few observations')
    return repo


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
def run_archived(raw: Path = RAW, figure: Path = FIGURE, archive: Path = Path("data/smoothed.csv"), months: int = 13) -> None:
    \"\"\"Like `run`, and keep the smoothed months in a file.\"\"\"
    settings = Settings(months=months)
    frame = smooth(trusted(load(raw), settings.minimum), settings.months)
    Archive(archive).write(frame)
    Trend(settings).save(frame, figure)
"""


def vision_files(repo):
    series = (repo / 'src/sunspots/series.py').read_text() + SERIES_VISION
    cli = (repo / 'src/sunspots/cli.py').read_text().replace('import load, smooth, trusted', 'import Archive, load, smooth, trusted')
    return {'src/sunspots/series.py': series, 'src/sunspots/cli.py': cli + CLI_VISION}


SEALED = """

@prophecy(sealed="raoulg/atlas#19")
class Format:
    \"\"\"The file the archive keeps its months in: what it is waits on a question.\"\"\"


@prophecy(sealed="raoulg/atlas#33")
class Compression:
    \"\"\"Whether the archive is compressed.\"\"\"
"""


def settle(page, ms=1500):
    page.wait_for_timeout(ms)
    assert page.locator('#fault').is_hidden(), page.locator('#fault').inner_text()


def fit(page):
    page.get_by_role('button', name='fit', exact=True).click()
    page.wait_for_timeout(700)


def main(atlas, out=None):
    """With an OUTDIR the pictures and texts are written there; without, the page and pictures of `docs/`."""
    atlas = str(pathlib.Path(atlas).resolve())
    texts, shown = {}, {}
    to_docs = out is None
    with tempfile.TemporaryDirectory() as tmp, sync_playwright() as p:
        tmp = pathlib.Path(tmp)
        out = pathlib.Path(out) if out else pathlib.Path(tempfile.mkdtemp(prefix='atlas-interaction-'))
        out.mkdir(parents=True, exist_ok=True)
        repo = scratch(tmp)
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

        # history: the commits, and the two of them as one picture
        page.goto(server.url('/history'), wait_until='networkidle')
        settle(page, 2000)
        page.set_viewport_size({'width': 1400, 'height': 620})
        page.get_by_text('The sunspots pipeline').first.click()
        page.get_by_text('Leave out the months').first.click(modifiers=['Shift'])
        settle(page, 4000)
        page.screenshot(path=str(out / 'history.png'))

        # look: the agent takes the page to a view and points at a box
        page.set_viewport_size({'width': 1400, 'height': 560})
        page.goto(server.url(), wait_until='networkidle')
        settle(page)
        texts['look'] = agent.tool('look', view={'route': 'sunspots.cli.run'}, at='sunspots.series.trusted')
        settle(page, 3500)
        shot('look')

        # shown: the person clicks a box; the agent reads which
        page.locator('#scene g[data-id="sunspots.series.trusted"]').first.click()
        settle(page)
        texts['shown'] = agent.tool('shown')

        # notes: the person writes one on a box; the agent reads it and resolves it
        page.locator('#scene g[data-id="sunspots.series.trusted"]').first.click(modifiers=['Alt'])
        page.wait_for_timeout(400)
        page.keyboard.type('Why a minimum of 1? What happens to the months with no observations?')
        shot('note-field', around='sunspots.series.trusted', pad=(560, 460, 170, 250), record=False)
        page.keyboard.press('Enter')
        page.wait_for_timeout(3500)
        shot('note-kept', around='sunspots.series.trusted', record=False)
        texts['notes'] = agent.tool('notes')
        head = git(repo, 'rev-parse', '--short', 'HEAD').strip()
        texts['resolve'] = agent.tool('resolve', id=1, commit=head)
        page.wait_for_timeout(3500)
        shot('note-resolved', around='sunspots.series.trusted', record=False)

        # annotations: the agent's own words on a box
        texts['annotate'] = agent.tool('annotate', on='sunspots.series.trusted',
                                      text="The minimum is 1 by default: a month with no observation has no mean in SILSO's file, and load has dropped it already.")
        page.wait_for_timeout(3500)
        page.locator('#scene g[data-id="sunspots.series.trusted"]').first.hover()
        page.wait_for_timeout(600)
        shot('annotated', around='sunspots.series.trusted', record=False)

        # a vision: a change shown beside the code as it is, the new route pointed at
        texts['vision'] = agent.tool('vision', title='Keep the smoothed months in a file', files=vision_files(repo))
        page.wait_for_timeout(6000)
        agent.tool('look', view={'route': 'sunspots.cli.run_archived'})
        settle(page, 4500)
        shot('vision')

        # a prophecy: the decided vision written into the tree, marked, with what the person adds by hand
        texts['prophetize'] = agent.tool('prophetize', supersedes={'sunspots.cli.run_archived': 'sunspots.cli.run'})
        texts['diff'] = git(repo, 'diff')
        with (repo / 'src/sunspots/series.py').open('a') as f:
            f.write(SEALED)
        texts['check'] = agent.tool('check')
        texts['revelation'] = agent.tool('revelation')
        texts['seals'] = agent.tool('seals')
        assert 'closed: the seal can be broken' in texts['seals'], 'the seals picture needs gh and the network'
        page.wait_for_timeout(7000)
        page.goto(server.url(), wait_until='networkidle')
        settle(page)
        agent.tool('look', view={'route': 'sunspots.cli.run_archived'})
        settle(page, 4500)
        shot('prophecy')
        texts['unseal'] = agent.tool('unseal', closed=True)
        texts['unseal-diff'] = git(repo, 'diff', 'src/sunspots/series.py')
        assert not errors, errors
        server.stop()
        browser.close()
    for name, text in texts.items():
        (out / f'{name}.txt').write_text(text)
    status = publish(out, texts, shown, to_docs)
    if to_docs:
        shutil.rmtree(out, ignore_errors=True)
    return status


def head(text, lines):
    """The first lines of a long answer, and that there are more."""
    ls = text.rstrip('\n').split('\n')
    return '\n'.join(ls[:lines] + (['…'] if len(ls) > lines else []))


def width(png):
    return struct.unpack('>I', png.read_bytes()[16:20])[0]


def figure(png, title, tag, alt):
    return (f'  <figure>\n    <figcaption><b>{html.escape(title)}</b><span class="tag">{html.escape(tag)}</span></figcaption>\n'
            f'    <a href="img/interaction/{png.name}"><img src="img/interaction/{png.name}" style="--w:{width(png) // 2}" alt="{html.escape(alt)}"></a>\n  </figure>\n')


def block(title, text):
    return (f'  <figure class="output">\n    <figcaption><b>{html.escape(title)}</b><span class="tag">what the tool answered</span></figcaption>\n'
            f'    <pre>{html.escape(text.rstrip())}</pre>\n  </figure>\n')


def section(ident, title, lede, *parts):
    return f'<section id="{ident}">\n  <h2>{html.escape(title)}</h2>\n  <p class="lede">{lede}</p>\n  <div class="pair">\n{"".join(parts)}  </div>\n</section>\n'


EXTRA = """
.pair figure.output pre{border:0;max-width:100%;white-space:pre-wrap;overflow-wrap:anywhere}
.pair figure.output{width:min(960px,100%)}
nav.story{display:flex;flex-wrap:wrap;gap:8px 20px;font-family:var(--mono);font-size:13px;margin:0 0 56px}
nav.story a{color:var(--ink-2);text-decoration-color:var(--accent);text-underline-offset:3px}
"""


def page(out, texts):
    png = lambda n: out / f'{n}.png'
    style = re.search(r'<style>(.*?)</style>', (DOCS / 'index.html').read_text(), re.S).group(1)
    body = [
        section('history', 'Two commits, one picture',
                'Beside the picture <code>atlas serve</code> has a history: the commit graph, and any two commits drawn as one picture, '
                'with what the second added, removed and changed marked. Here the two commits of the pipeline: the second leaves out the '
                'months with too few observations. <code>series</code> gains one step, <code>cli</code> changes.',
                figure(png('history'), 'The history page', 'A ∪ B', 'The history of the sunspots repository with its two commits drawn as one picture: what the second added and changed is marked')),
        section('look', 'Pointing, both ways',
                'The person and the agent share one view. <code>look</code> takes the person\'s page to a view and points at a box: '
                '&ldquo;here, this one&rdquo;. <code>shown</code> goes the other way: what the person has open and the box they clicked, '
                'in the names the agent can use.',
                figure(png('look'), 'The agent took the page to the route and pointed at trusted', 'look', 'The route run of the sunspots pipeline with the step trusted pointed at'),
                block('look', texts['look']), block('shown, after the person clicked trusted', head(texts['shown'], 9))),
        section('notes', 'A note on a box',
                'Alt-click a box or a line and write what you see or do not understand. The agent reads the note where it works, answers it '
                'by a fix or by explaining, and resolves it with the commit that fixed it. Atlas is no chat: the note stays on the box, '
                'a ring while it is open, a &#10003; once it is resolved.',
                figure(png('note-field'), 'The person writes a note on trusted', 'alt-click', 'A note field open on the step trusted, with a question typed in'),
                figure(png('note-kept'), 'The note is kept', 'ring', 'The step trusted with a ring that says one note is on it'),
                block('notes', texts['notes']),
                figure(png('note-resolved'), 'The agent resolved it', '✓', 'The step trusted with a check mark: its note is resolved'),
                block('resolve', texts['resolve'])),
        section('annotations', 'What the agent says that the code does not',
                'A condition, a step\'s reason, why a place stands where it does: <code>annotate</code> puts the agent\'s own words on a box or a '
                'line. A &#10022; marks it; hover it for the words. They are the agent\'s statements, apart from what Atlas traces.',
                figure(png('annotated'), 'trusted, with a note resolved and an annotation', '✦', 'The step trusted with a check mark and a star: a resolved note and an annotation'),
                block('annotate', texts['annotate'])),
        section('vision', 'Show the change, do not describe it',
                'To ask &ldquo;shall I do this?&rdquo; the agent shows the files the change would write, as sketch code. The page draws the '
                'working tree and the vision as one picture: what the vision adds is marked, and <strong>A</strong>, <strong>A ∪ B</strong> and '
                '<strong>B</strong> switch between them. The vision tells what it does to the routes and to the findings.',
                figure(png('vision'), 'A vision: keep the smoothed months in a file', 'vision', 'The vision draws the new route run-archived beside the working tree'),
                block('vision', texts['vision'])),
        section('prophecy', 'Decide it: the vision becomes a prophecy',
                'When the person says yes, <code>prophetize</code> writes the vision into the tree as <em>prophecy</em>: sketch code, marked as '
                'not built yet, with what it <code>supersedes</code>. It leaves the files for the person to commit. Atlas draws the prophecy '
                'beside what exists, in its own colour with a dotted border, and the distance between the two is the list of what is left.',
                block('prophetize', texts['prophetize']), block('git diff', head(texts['diff'], 60)),
                figure(png('prophecy'), 'The prophecy as a route of its own', 'prophecy', 'The route run-archived drawn with its prophesied boxes dotted'),
                block('check: the finding on the empty body is gone, a prophecy is a todo', texts['check']),
                block('revelation: what is left to build', texts['revelation']),
                block('seals: what waits on an open question (asked of GitHub with gh; here two issues of Atlas stand in for the questions)', texts['seals']),
                block('unseal: the closed one is broken', texts['unseal'] + '\n\n' + texts['unseal-diff'])),
    ]
    nav = ' '.join(f'<a href="#{i}">{t}</a>' for i, t in [('history', 'history'), ('look', 'look and shown'), ('notes', 'notes'),
                                                            ('annotations', 'annotations'), ('vision', 'vision'), ('prophecy', 'prophecy')])
    return f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Atlas: working with your agent</title>
<meta name="description" content="How a person and an agent use Atlas together: history, pointing, notes, annotations, a vision and a prophecy, drawn from a real run.">
<link rel="icon" href="logo.svg">
<link rel="preconnect" href="https://fonts.googleapis.com">
<link href="https://fonts.googleapis.com/css2?family=Outfit:wght@400;500;600&family=JetBrains+Mono:wght@400;500&display=swap" rel="stylesheet">
<style>{style}{EXTRA}</style>
</head>
<body>
<main>
<header>
  <img src="logo.svg" alt="Atlas logo">
  <div>
    <h1>Working with your agent</h1>
    <p>Every picture is a screenshot of <code>atlas serve</code> after what the person or the agent did; every text is what a tool answered.</p>
  </div>
  <nav>
    <a href="index.html">atlas</a>
    <a href="https://github.com/raoulg/atlas">github</a>
    <a href="llms.txt">for agents</a>
  </nav>
</header>
<nav class="story">{nav}</nav>
{''.join(body)}<footer>
  <span>Drawn by <code>tools/interaction_pictures.py</code> from the sunspots pipeline, with the build that made the release.</span>
  <a href="reference/guide.md">the guide</a>
</footer>
</main>
</body>
</html>
"""


def publish(out, texts, shown, to_docs):
    """The page and its pictures into `docs/`, and what each picture draws kept: a picture that draws
    otherwise than it did is named, and the text beside it is to be reread."""
    if not to_docs:
        (out / 'drawn.json').write_text(json.dumps(shown, indent=1) + '\n')
        print(out)
        return 0
    target = DOCS / 'img/interaction'
    target.mkdir(parents=True, exist_ok=True)
    kept = json.loads(DRAWN.read_text()) if DRAWN.exists() else {}
    for png in out.glob('*.png'):
        shutil.copy(png, target / png.name)
    (DOCS / 'interaction.html').write_text(page(out, texts))
    DRAWN.write_text(json.dumps(shown, indent=1) + '\n')
    changed = [n for n, v in shown.items() if kept.get(n) != v]
    if not changed:
        print('every picture draws what it drew')
        return 0
    print('\nThese pictures draw otherwise; reread the text beside them in docs/interaction.html:')
    for n in changed:
        print(f'  {n}')
    return 0 if not kept else 1


if __name__ == '__main__':
    sys.exit(main(*sys.argv[1:]))
