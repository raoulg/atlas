# /// script
# requires-python = ">=3.11"
# dependencies = ["playwright"]
# ///
"""Draw the release report of Atlas with Atlas itself: per issue of a release, what changed in Atlas's code and what
a user sees differently, as pictures and the data they are drawn from.

    uv run tools/release_report.py BEFORE AFTER BASE HEAD OUTDIR [--issues 154,147,...] [--cards A,B,C]

BEFORE and AFTER are the Atlas builds of the two releases, BASE..HEAD the commits between them (`v0.67.2..next`). A
commit serves the issues named in the parenthesis that ends its subject. The picture of a change is never chosen by
taste: zoom is a quotient by the boxes that are open, so the right picture is the coarsest view in which every changed
fact is a box or a line of its own, the view that opens exactly the frames that hold one.

- **A, what a user sees.** The subjects are the programs whose `.plan` the issue's commits change (a plan lies beside
  the program it plans, under `engine/tests/pairs` and `engine/tests/plans`), each in the route its facts are written
  for. The changed facts are the plan lines the commits add, remove, mark or unmark. A fact is the issue's when its
  mark, its own comment or the comment that heads it names the issue (a line put in place of a marked one carries that
  mark's issue), or when nothing names any issue its commit serves; a plan that names the issue comes before one that
  is the issue's only that way. The view opens the frames on the path to the facts, by the names
  `engine/tests/plans.mjs` gives, resolved in each build's own page. Both builds draw the program as HEAD has it; the
  boxes and lines only one of them draws, and the places that stand on another side, are outlined in the page's own
  colours (`--del` in the before picture, `--add` in the after). The release changes more than one issue, so the
  difference also says which of its boxes the issue's facts name.
- **B, what changed in Atlas.** `atlas-code view` of the repository with `--since <first commit>^ --until <last commit>`,
  which marks what was added, removed and changed. The changed facts are the functions the engine marks (their own
  arrow differs); a function the diff edits whose arrow stays is outlined amber where it is drawn, and leads the route
  only when the engine marks none. The route reaches the most of them, a call of a trait's or a base's method reaching
  its overrides, opening the fewest frames on the way: first among the functions an entry calls directly and the
  exports at a part's root, then among the functions nothing calls (a rule in a table), then the entries. A view of
  more than BOXES boxes is too fine for a card and is cut at the frame that holds every change, the calls above it
  named beside the picture (`called_by`); where that frame is still too fine, one picture per call it makes toward the
  changes (at most PICTURES), and a single change whose caller is still too fine is drawn as itself.
- **C, where no picture shows it.** The card says why and shows what does: the help of the command the title names
  (`atlas-code --help` when it names none) with both builds, `atlas-code check` on the pairs the issue changes, a gesture in the
  viewer played in both builds (`GESTURES`), the documentation's changed paragraphs before and after. A change that
  touches no plan, pair, check, command line, viewer or documentation is internal: its card says so beside B.

Everything an issue draws goes into OUTDIR/<issue>/: the pictures (PNG, light theme, device scale 2, cropped to the
diagram, at most 1600 pixels wide), the page each is taken from, the view it opens, and for A the difference (the boxes
and lines only the before picture draws, and only the after). OUTDIR/report.json lists it all, issue by issue; the page
that shows the report is made from it elsewhere. Reading Atlas at two revisions for B is the slow step (about a minute
the first time; Atlas keeps its readings).
"""

import argparse
import collections
import difflib
import json
import os
import pathlib
import re
import subprocess
import sys
import tempfile
import time
import urllib.parse

from playwright.sync_api import sync_playwright

sys.dont_write_bytecode = True
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from docs_pictures import CAPTURE, settled, shoot

ROOT = pathlib.Path(__file__).resolve().parents[1]
# CSS pixels: at device scale 2 a picture is at most 1600 pixels wide
WIDEST = 800
EDITED = '#e8a33d'
MANIFESTS = ('pyproject.toml', 'Cargo.toml', 'package.json')
READ = ('.rs', '.py', '.ts', '.tsx')
# at most this many subjects for A, and pairs for a check, per issue
SUBJECTS, PAIRS = 3, 4
# a picture of Atlas with more boxes than this is cut at the frames that hold the changes, into at most PICTURES
BOXES, PICTURES = 80, 3

# A change in the viewer shows only in what a reader does: the gesture, played in both builds on one subject.
GESTURES = {
    142: {'subject': 'engine/tests/pairs/pipeline/python-clean', 'route': 'sunspots.cli.run',
          'box': 'sunspots.plot.Plot.save', 'does': ['pin', 'open'],
          'says': 'pin a step with a double click, then open it with a click'},
}


def git(*args):
    return subprocess.run(['git', '-C', str(ROOT), *args], check=True, capture_output=True, text=True).stdout


def text_at(rev, path):
    """A file as a revision has it; empty where it has none."""
    r = subprocess.run(['git', '-C', str(ROOT), 'show', f'{rev}:{path}'], capture_output=True, text=True, check=False)
    return r.stdout if r.returncode == 0 else ''


def kind(path):
    """What a file of the repository is, for the report."""
    if path.endswith('.plan'):
        return 'plan'
    if path.startswith(('engine/tests/pairs/', 'engine/tests/plans/', 'engine/tests/fixture')):
        return 'subject'
    if path.startswith(('engine/tests/', 'cli/tests/')):
        return 'test'
    if path == 'viewer/plan.js':
        return 'model'
    if path.startswith('viewer/'):
        return 'viewer'
    if path.startswith('engine/src/principles') or path == 'docs/reference/principles.md':
        return 'check'
    if path.startswith('engine/src/'):
        return 'engine'
    if path.startswith('cli/'):
        return 'cli'
    if path.startswith('docs/') or path.endswith('.md'):
        return 'docs'
    if path.startswith(('tools/', '.github/')) or path in ('install.sh', 'install.ps1', 'lefthook.yml'):
        return 'tooling'
    return 'other'


def served(subject):
    """The issues a commit serves: those in the parenthesis that ends its subject, else every one it names."""
    m = re.search(r'\((#\d+(?:,\s*#\d+)*)\)\s*$', subject)
    return sorted({int(n) for n in re.findall(r'#(\d+)', m.group(1) if m else subject)})


def inventory(base, head):
    """Per issue: its commits, oldest first, and the files they touch."""
    issues = {}
    for line in git('log', '--no-merges', '--reverse', '--format=%H%x09%s', f'{base}..{head}').splitlines():
        sha, subject = line.split('\t', 1)
        named = served(subject)
        files = [f for f in git('show', '--format=', '--name-only', sha).splitlines() if f] if named else []
        for n in named:
            it = issues.setdefault(n, {'issue': n, 'commits': [], 'files': []})
            it['commits'].append({'sha': sha, 'subject': subject, 'serves': named, 'files': files})
            it['files'] += [f for f in files if f not in it['files']]
    out = subprocess.run(['gh', 'issue', 'list', '--repo', 'raoulg/atlas', '--state', 'all', '--limit', '1000',
                          '--json', 'number,title,labels,state'], capture_output=True, text=True, cwd=ROOT, check=False)
    meta = {i['number']: i for i in json.loads(out.stdout)} if out.returncode == 0 else {}
    for n, it in issues.items():
        m = meta.get(n, {})
        it.update(title=m.get('title'), state=m.get('state'), labels=[label['name'] for label in m.get('labels', [])],
                  kinds=dict(collections.Counter(kind(f) for f in it['files'])))
    return issues


# ---------- the facts of a plan ----------

def plan_facts(text):
    """The facts of a `.plan` file, in order: its route, the fact, its mark, and the issues its comments name."""
    route, heading, in_comment, out = None, set(), False, []
    for raw in text.splitlines():
        line = raw.split(' # ')[0].strip()
        if raw.strip().startswith('#'):
            if not in_comment:
                heading = set()
            heading |= {int(n) for n in re.findall(r'#(\d+)', raw)}
            in_comment = True
            continue
        in_comment = False
        if not line:
            continue
        words = line.split()
        if words[0] == 'route':
            route = ' '.join(words[1:])
            continue
        mark = None
        if words[0] == 'known' and len(words) > 2 and words[1].startswith('#'):
            way = words[2] if words[2] in ('extra', 'fault') else 'known'
            mark = (int(words[1][1:]), way)
            words = words[3 if way != 'known' else 2:]
        note = raw[len(raw.split(' # ')[0]):]
        named = heading | {int(n) for n in re.findall(r'#(\d+)', note)} | ({mark[0]} if mark else set())
        out.append({'route': route, 'fact': ' '.join(words), 'state': mark[1] if mark else 'holds', 'named': named,
                    'text': line})
    return out


def plan_changes(commit, path):
    """The facts one commit changes in one plan: (route, fact) -> (state before, state after, issues named)."""
    old, new = plan_facts(text_at(commit + '^', path)), plan_facts(text_at(commit, path))
    # a line the commit puts in place of another (one hunk, the same place in it) carries the issue the old one's mark named
    carried = {}
    sm = difflib.SequenceMatcher(None, [f['text'] for f in old], [f['text'] for f in new], autojunk=False)
    for op, i1, i2, j1, j2 in sm.get_opcodes():
        if op == 'replace':
            for a, b in zip(old[i1:i2], new[j1:j2]):
                if a['state'] != 'holds':
                    carried.setdefault((b['route'], b['fact']), set()).update(a['named'])
    before = {(f['route'], f['fact']): f for f in old if f['state'] != 'fault'}
    after = {(f['route'], f['fact']): f for f in new if f['state'] != 'fault'}
    out = {}
    for key in before.keys() | after.keys():
        a, b = before.get(key), after.get(key)
        sa, sb = a['state'] if a else None, b['state'] if b else None
        if sa != sb:
            out[key] = (sa, sb, (a['named'] if a else set()) | (b['named'] if b else set()) | carried.get(key, set()))
    return out


def facts_of(it):
    """The plan facts an issue's commits change, by plan and route."""
    changed = {}
    for c in it['commits']:
        for path in [f for f in c['files'] if f.endswith('.plan')]:
            for (route, fact), (sa, sb, named) in plan_changes(c['sha'], path).items():
                mine = named & set(c['serves']) or set(c['serves'])
                if it['issue'] not in mine:
                    continue
                key = (path, route, fact)
                first = changed.get(key, {}).get('before', sa)
                changed[key] = {'plan': path, 'route': route, 'fact': fact, 'before': first, 'after': sb,
                                'commit': c['sha'][:7], 'named': it['issue'] in named or c['serves'] == [it['issue']]}
    by = collections.defaultdict(list)
    for f in changed.values():
        if f['before'] != f['after']:
            by[(f['plan'], f['route'])].append(f)
    return by


def subjects_of(it):
    """The plans and routes whose facts an issue changes, those that name the issue first; a plan whose facts are the
    issue's only because its commit serves it among others is kept only when no plan names the issue."""
    by = facts_of(it)
    named = lambda facts: sum(f['named'] for f in facts)
    ranked = sorted(by.items(), key=lambda kv: (-named(kv[1]), -len(kv[1]), kv[0]))
    if any(named(facts) for _, facts in ranked):
        ranked = [kv for kv in ranked if named(kv[1])]
    return ranked[:SUBJECTS]


CHANNELS = ('value reached', 'value', 'own', 'reached', 'call', 'back')


def fact_paths(fact):
    """The occurrences a fact names, and the frames it says a value stands in: what must be open to see it."""
    head, _, rest = fact.partition(' ')
    if head in ('step', 'choice'):
        return [rest.removesuffix(' recursive')], []
    if head == 'line':
        a, _, b = rest.partition(' > ')
        for c in CHANNELS:
            if b.endswith(' ' + c):
                b = b[:-len(c) - 1]
                break
        return [p for box in (a, b) for p in box_paths(box)], []
    if head == 'home':
        value, _, frame = rest.rpartition(' ')
        return box_paths(value), ([] if frame == 'outside' else [frame])
    return [], []


def fact_boxes(fact):
    """The boxes a fact names, as the plan names them."""
    head, _, rest = fact.partition(' ')
    if head in ('step', 'choice'):
        return [rest.removesuffix(' recursive')]
    if head == 'line':
        a, _, b = rest.partition(' > ')
        for c in CHANNELS:
            if b.endswith(' ' + c):
                return [a, b[:-len(c) - 1]]
        return [a, b]
    if head == 'home':
        return [rest.rpartition(' ')[0]]
    if head == 'side':
        return [rest.rpartition(' ')[0]]
    return []


def box_paths(box):
    """The occurrence a box of a plan names: a step by its path, a value by the step that made it."""
    if ' in ' in box:
        return [box.rsplit(' in ', 1)[1]]
    if '@' in box:
        return [box.rsplit('@', 1)[1]]
    return [box]


# ---------- the pages ----------

# the page keeps its model inside one function; this puts what the report reads of it on `window`
EXPOSE = 'window.__atlas = {' + ', '.join(
    f'get {n}() {{ return typeof {n} === "undefined" ? undefined : {n}; }}' for n in
    ['V', 'treeInfo', 'byId', 'typeLabel', 'nameOf', 'isCtor', 'PARTS', 'unitLabel', 'entryLabel', 'stepLabel', 'PICTURE',
     'pinned']) + '};\n'

# The names `engine/tests/plans.mjs` gives the occurrences of a route's finest plan, computed in the page of either
# build: each node of the plan's tree by its path of labels, a frame with the step it opens. The builds that keep the
# step's label inside the drawing get it from the same helpers.
NAMES = r"""() => {
  const A = window.__atlas, F = window.atlasPlan && window.atlasPlan.fine, V = A.V;
  if (!F || !V.route) return null;
  const {treeInfo, byId, typeLabel, nameOf, isCtor, PARTS, unitLabel, entryLabel} = A;
  const stepName = id => { if (id.startsWith("part:")) { const p = PARTS.find(x => x.id === id.slice(5)) || {}; return `${p.id} · ${p.language}`; } if (id.startsWith("unit:")) return unitLabel(id.slice(5)); const n = byId.get(id); if (!n) return id; if (!V.route && n.entry) return entryLabel(n); if (n.kind === "class") return nameOf(n) + "()"; if (n.make) return n.name; if (isCtor(n) && !n.on_class) return typeLabel(n.cls) + "()"; return n.cls ? `${typeLabel(n.cls)}.${nameOf(n)}` : nameOf(n); };
  const stepLabel = A.stepLabel || (key => { const info = treeInfo.get(key); if (!info) return stepName(key); if (info.step.startsWith("op:")) return info.step.slice(3); if (info.built) return typeLabel(info.built) + "()"; const n = byId.get(info.step); return info.on && !info.named && n && n.cls && !isCtor(n) ? `${typeLabel(info.on)}.${nameOf(n)}` : stepName(info.step); });
  const order = new Map([...treeInfo.keys()].map((k, i) => [k, i]));
  const nodes = new Map();
  for (const [id, f] of F.frames) nodes.set(id, {id, parent: f.parent, key: f.step, synthetic: !!f.synthetic, frame: true});
  for (const [id, P] of F.pills) if (!P.own) nodes.set(id, {id, parent: P.parent, key: P.steps[0], synthetic: false, frame: false});
  const rank = n => n.synthetic ? Math.min(...[...nodes.values()].filter(m => m.parent === n.id).map(rank)) : order.get(n.key) ?? 1e9;
  const kids = new Map();
  for (const n of nodes.values()) { if (!kids.has(n.parent)) kids.set(n.parent, []); kids.get(n.parent).push(n); }
  const name = new Map();
  const visit = (parent, path) => {
    const ns = (kids.get(parent) || []).sort((a, b) => rank(a) - rank(b)), seen = new Map();
    let choices = 0;
    for (const n of ns) {
      const seg = n.synthetic ? `?${++choices}` : stepLabel(n.key).replace(/\s+/g, " "), k = (seen.get(seg) || 0) + 1;
      seen.set(seg, k);
      const p = path === null ? seg : `${path}/${seg}${k > 1 ? "#" + k : ""}`;
      name.set(n.id, p);
      visit(n.id, p);
    }
  };
  visit(null, null);
  const occ = id => (id.endsWith("|") ? occ(id.slice(0, -1)) + "/?" : name.get(id.startsWith("o:") ? "n:" + id.slice(2) : id) ?? id);
  const placeName = k => (F.places.get(k)?.label ?? k.replace(/^w:/, "")).replace(/\s+/g, " ");
  const valueName = k => {
    const p = F.places.get(k);
    if (k.startsWith("v:back:") && p) return `${placeName(p.stands)} in ${occ(p.at.replace(/\/~back$/, ""))}`;
    let type = p?.type, nm = p?.name && !p.name.startsWith("#") ? p.name : p?.held || "", at = p?.at;
    if (!p) { const m = k.match(/^t:([^~@]+)(?:~([^@]*))?(?:@(.+))?$/); if (!m) return null; type = m[1]; nm = m[2] && !m[2].startsWith("#") ? m[2] : ""; at = m[3]; }
    return (type ? typeLabel(type) : "_") + (nm ? "·" + nm : "") + (at ? "@" + occ(at) : "");
  };
  const canon = id => {
    if (name.has(id)) return name.get(id);
    if (id.startsWith("o:")) return "own " + occ(id);
    if (id.startsWith("e:")) return "entry";
    if (id.startsWith("w:")) return placeName(id);
    if (/^[tv]:/.test(id)) return valueName(id);
    return null;
  };
  window.__canon = canon;
  return {route: V.route, open: [...V.open.steps], tree: [...nodes.values()].map(n => ({name: name.get(n.id), id: n.id, parent: n.parent, key: n.key, frame: n.frame, synthetic: n.synthetic}))};
}"""

# What the page draws: each box by its id in the layout, the name the plan gives it and its words; each line by its ends
# and its kind
DRAWN = r"""() => {
  const P = window.__atlas.PICTURE, canon = window.__canon || (() => null), words = s => s.replace(/\s+/g, " ").trim();
  const boxes = [...P.boxes].map(([id, el]) => ({id, name: canon(id), text: words(el.textContent), cls: el.getAttribute("class")}));
  for (const el of document.querySelectorAll("#scene g.frame[data-id]"))
    boxes.push({id: el.dataset.id, name: canon(el.dataset.id), text: words(el.querySelector(".ftitle")?.textContent || ""), cls: el.getAttribute("class")});
  const sides = (window.atlasPicture?.sides || []).map(([id, side]) => ({id, name: canon(id) || id, side}));
  return {boxes, lines: P.lines.map(e => ({a: e.a, b: e.b, kind: e.kind || ""})), sides};
}"""

# Outlines boxes and lines in a colour of the page's own (a CSS variable) or one given
OUTLINE = r"""([boxes, lines, colour]) => {
  const P = window.__atlas.PICTURE, frames = new Map([...document.querySelectorAll("#scene g.frame[data-id]")].map(e => [e.dataset.id, e]));
  const paint = x => { x.style.setProperty("stroke", colour, "important"); x.style.setProperty("stroke-width", "3px", "important"); x.style.setProperty("stroke-dasharray", "none", "important"); x.style.setProperty("opacity", "1", "important"); };
  let n = 0;
  for (const id of boxes) { const el = P.boxes.get(id) || frames.get(id), r = el && el.querySelector(":scope > rect.bg"); if (r) { paint(r); n++; } }
  for (const [a, b, kind] of lines) for (const e of P.lines) if (e.a === a && e.b === b && (e.kind || "") === kind) { e.els.filter(x => x.tagName === "path").forEach(paint); n++; }
  const probe = document.createElement("div"); probe.style.color = colour; document.body.append(probe);
  const rgb = getComputedStyle(probe).color; probe.remove();
  return {outlined: n, colour: rgb};
}"""

TREE = r"""() => [...window.__atlas.treeInfo].map(([k, i]) => [k, i.step])"""


def picture(page, destination):
    """The diagram alone, at most WIDEST wide. A page fits its picture to the window again when the window changes
    size, so the window takes the picture's size first and the page settles before `shoot` places it."""
    bounds = page.locator('#scene').evaluate("g => {const b=g.getBBox();return {width:b.width,height:b.height}}")
    scale = min(1, WIDEST / bounds['width']) if bounds['width'] else 1
    page.set_viewport_size({'width': max(1400, int(bounds['width'] * scale) + 80),
                            'height': max(900, int(bounds['height'] * scale) + 180)})
    page.wait_for_timeout(600)
    # the state bar names the open steps over the picture; the view beside the picture names them too
    page.evaluate("() => { const c = document.getElementById('crumbs'); if (c) c.style.display = 'none'; }")
    shoot(page, destination, WIDEST)


def page_data(html):
    """The graph a page is drawn from: its first project."""
    text = html.read_text()
    start = text.index('window.DATA = ') + len('window.DATA = ')
    data, _ = json.JSONDecoder().raw_decode(text[start:].replace('<\\/', '</'))
    return next(iter(data['projects'].values()))


def view_of(atlas, subject, html, view=None, extra=()):
    """`atlas-code view` of a subject into `html`, in a view; what it said, or why it failed."""
    args = [str(atlas), 'view', str(subject), *extra, '--no-open', '--out', str(html)]
    if view:
        args += ['--view', json.dumps(view)]
    r = subprocess.run(args, capture_output=True, text=True, check=False, cwd=ROOT)
    if r.returncode:
        raise RuntimeError(f'{" ".join(args[1:])}: {r.stderr.strip() or r.stdout.strip()}')
    return r.stdout.strip()


class Browser:
    """The installed Chrome, in the light theme at device scale 2, and the probes of the pages it opens."""

    def __init__(self, p, tmp):
        self.browser = p.chromium.launch(channel='chrome')
        self.ctx = self.browser.new_context(viewport={'width': 1600, 'height': 1000}, device_scale_factor=2,
                                            color_scheme='light', reduced_motion='reduce')
        self.ctx.add_init_script('(' + CAPTURE + ')()')
        self.tmp, self.n = tmp, 0

    def open(self, html, view=None):
        """A page Atlas wrote, with the report's probe, in `view` if given (as its address would)."""
        text = html.read_text()
        # the model lives in `mount` where the page has one (a served page mounts each new reading), else in the
        # function around the whole script
        i = text.rindex('dispose = () =>') if 'function mount(' in text else text.rindex('})();')
        self.n += 1
        probe = self.tmp / f'{self.n:03d}-{html.stem}.html'
        probe.write_text(text[:i] + EXPOSE + text[i:])
        page = self.ctx.new_page()
        page.set_default_timeout(180_000)
        page.errors = []
        page.on('pageerror', lambda e: page.errors.append(str(e)))
        url = probe.as_uri() + ('#v=' + urllib.parse.quote(json.dumps(view)) if view else '')
        page.goto(url, wait_until='networkidle')
        settled(page)
        return page


# ---------- A: what a user sees ----------

def route_in(data, route):
    """The route in another build's reading of the same program: itself, else the one function whose id ends as it does."""
    ids = [n['id'] for n in data['nodes'] if n['kind'] in ('function', 'method', 'class')]
    if route in ids:
        return route
    def tail(a):
        same = 0
        for x, y in zip(reversed(a.split('.')), reversed(route.split('.'))):
            if x != y:
                break
            same += 1
        return same
    best = max((tail(i) for i in ids), default=0)
    found = [i for i in ids if tail(i) == best]
    return found[0] if best >= 2 and len(found) == 1 else None


def opens_for(tree, facts):
    """The steps to open so that every fact is a box or a line of its own: the frames on the path to what it names."""
    by_name = {n['name']: n for n in tree if n['name']}
    by_id = {n['id']: n for n in tree}
    keys, missing = set(), []

    def resolve(path):
        p = path
        while p and p not in by_name:
            p = re.sub(r'#\d+$', '', p) if re.search(r'#\d+$', p) else (p.rsplit('/', 1)[0] if '/' in p else '')
        return by_name.get(p), p == path

    def frames_above(n, itself):
        """The steps of the frames that hold `n`, and its own when it is a frame to be seen open; never the route."""
        out, m = [], (n if itself else by_id.get(n['parent']))
        while m is not None and m['parent'] is not None:
            if m['frame'] and not m['synthetic']:
                out.append(m['key'])
            m = by_id.get(m['parent'])
        return out

    for f in facts:
        inside, frames = fact_paths(f['fact'])
        for path, opened in [(p, False) for p in inside] + [(p, True) for p in frames]:
            node, exact = resolve(path)
            if node is None:
                continue
            if not exact:
                missing.append(path)
            keys.update(frames_above(node, opened or not exact))
    return sorted(keys), sorted(set(missing))


def difference(before, after):
    """What one picture draws and the other does not: boxes by their name in the plan and their words, lines by the names
    of their ends and their kind, and the places that stand on another side. Beside it, in each picture, the ids of the
    boxes and lines to outline."""
    def box(b):
        return (b['name'] or '', b['text'])

    def ends(s):
        names = {b['id']: b['name'] or b['text'] for b in s['boxes']}
        return [(names.get(e['a'], e['a']), names.get(e['b'], e['b']), e['kind']) for e in s['lines']]
    bb, ab = collections.Counter(map(box, before['boxes'])), collections.Counter(map(box, after['boxes']))
    bl, al = collections.Counter(ends(before)), collections.Counter(ends(after))
    only_b, only_a = bb - ab, ab - bb
    lines_b, lines_a = bl - al, al - bl
    changed = sorted({(n, tb, ta) for n, tb in only_b for m, ta in only_a if n and n == m})
    sb, sa = ({s['name']: s['side'] for s in x.get('sides', [])} for x in (before, after))
    moved = sorted((n, sb[n], sa[n]) for n in sb.keys() & sa.keys() if sb[n] != sa[n])
    ids = lambda s, wanted: [b['id'] for b in s['boxes'] if box(b) in wanted or (b['name'] or b['text']) in {n for n, _, _ in moved}]
    lids = lambda s, wanted: [[e['a'], e['b'], e['kind']] for e, k in zip(s['lines'], ends(s)) if k in wanted]
    return {
        'equal': not (only_b or only_a or lines_b or lines_a or moved),
        'counts': {'boxes_before': sum(bb.values()), 'boxes_after': sum(ab.values()), 'lines_before': sum(bl.values()),
                   'lines_after': sum(al.values()), 'boxes_only_before': sum(only_b.values()),
                   'boxes_only_after': sum(only_a.values()), 'lines_only_before': sum(lines_b.values()),
                   'lines_only_after': sum(lines_a.values()), 'boxes_changed': len(changed), 'sides_changed': len(moved)},
        'boxes_only_before': sorted(only_b.elements()), 'boxes_only_after': sorted(only_a.elements()),
        'lines_only_before': sorted(lines_b.elements()), 'lines_only_after': sorted(lines_a.elements()),
        'boxes_changed': [{'name': n, 'before': tb, 'after': ta} for n, tb, ta in changed],
        'sides_changed': [{'name': n, 'before': b, 'after': a} for n, b, a in moved],
    }, (ids(before, only_b), lids(before, lines_b)), (ids(after, only_a), lids(after, lines_a))


def user_card(br, builds, subject, route, facts, out, tag, was=None):
    """Card A: the program drawn by both builds in the view that opens the frames holding the changed facts. `was`
    names the route the before build starts the program at, where the change moves its start."""
    card = {'kind': 'A', 'subject': subject, 'route': route, 'facts': facts}
    program = ROOT / subject
    probes, views = {}, {}
    for side, atlas in builds.items():
        html = out / f'{tag}-{side}.html'
        view_of(atlas, program, html)
        r = route if side == 'after' else was or route_in(page_data(html), route)
        if r is None:
            card.setdefault('notes', []).append(f'the {side} build reads no route {route}')
            continue
        page = br.open(html, {'route': r})
        names = page.evaluate(NAMES)
        page.close()
        if names is None:
            card.setdefault('notes', []).append(f'the {side} build draws no plan of {r}')
            continue
        keys, missing = opens_for(names['tree'], facts)
        views[side] = {'route': r, **({'open': {'steps': keys}} if keys else {})}
        if missing:
            card.setdefault('missing', {})[side] = missing
    card['view'] = views
    (out / f'{tag}-view.json').write_text(json.dumps(views, indent=1) + '\n')
    snaps, pages = {}, {}
    for side, view in views.items():
        html = out / f'{tag}-{side}.html'
        view_of(builds[side], program, html, view)
        page = br.open(html)
        page.evaluate(NAMES)
        applied = page.evaluate('() => [...window.__atlas.V.open.steps].sort()')
        if applied != view.get('open', {}).get('steps', []):
            card.setdefault('notes', []).append(f'the {side} page opened {applied}, not {view.get("open")}')
        snaps[side], pages[side] = page.evaluate(DRAWN), page
        probes[side] = html
    if len(snaps) == 2:
        diff, only_b, only_a = difference(snaps['before'], snaps['after'])
        # the release changes more than this issue: which of the differences are boxes its facts name
        named = {b for f in facts for b in fact_boxes(f['fact'])}
        diff['named_by_facts'] = {side: sorted({n for n, _ in diff[f'boxes_only_{side}'] if n in named}
                                               | {s['name'] for s in diff['sides_changed'] if s['name'] in named})
                                  for side in ('before', 'after')}
        card['difference'] = diff
        (out / f'{tag}-difference.json').write_text(json.dumps(diff, indent=1, ensure_ascii=False) + '\n')
        if diff['equal']:
            card['says'] = 'the user sees no difference in this view'
            pages['before'].close()
            del pages['before']
        else:
            card['colours'] = {'before': pages['before'].evaluate(OUTLINE, [*only_b, 'var(--del)'])['colour'],
                               'after': pages['after'].evaluate(OUTLINE, [*only_a, 'var(--add)'])['colour']}
    card['pictures'] = {}
    for side, page in pages.items():
        png = out / f'{tag}-{side}.png'
        picture(page, png)
        card['pictures'][side] = png.name
        card.setdefault('errors', []).extend(page.errors)
        page.close()
    card['pages'] = {side: html.name for side, html in probes.items()}
    return card


# ---------- B: what changed in Atlas ----------

def hunks(a, b, files):
    """The lines a diff changes, by file: the old side's and the new side's."""
    old, new = collections.defaultdict(list), collections.defaultdict(list)
    path = None
    for line in git('diff', '-U0', a, b, '--', *files).splitlines():
        if line.startswith('+++ '):
            path = line[6:] if line.startswith('+++ b/') else None
        elif line.startswith('--- '):
            path = line[6:] if line.startswith('--- a/') else path
        elif line.startswith('@@') and path:
            m = re.match(r'@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@', line)
            o, on, n, nn = int(m[1]), int(m[2] or 1), int(m[3]), int(m[4] or 1)
            old[path].append((o, o + max(on, 1) - 1))
            new[path].append((n, n + max(nn, 1) - 1))
    return old, new


def edited(data, old, new):
    """The functions whose span a hunk touches, each with how many of its lines the hunks cover: a removed one on the
    old side, any other on the new."""
    marks = (data.get('diff') or {}).get('nodes', {})
    out = {}
    for n in data['nodes']:
        if n['kind'] not in ('function', 'method') or not n.get('file') or not n.get('span'):
            continue
        side = old if marks.get(n['id']) == 'removed' else new
        lo, hi = n['span']
        for path, spans in side.items():
            if n['file'].endswith('/' + path):
                lines = sum(max(0, min(b, hi) - max(a, lo) + 1) for a, b in spans)
                if lines:
                    out[n['id']] = out.get(n['id'], 0) + lines
    return out


def is_test(n):
    return (n.get('entry') or {}).get('kind') == 'test' or '/tests/' in (n.get('file') or '')


def call_graph(data):
    """Who calls whom, and a call of a trait's or a base's method also each override of it: the language's own dispatch."""
    calls = collections.defaultdict(set)
    for e in data['edges']:
        if e['kind'] == 'calls':
            calls[e['src']].add(e['dst'])
    below = collections.defaultdict(set)
    for e in data['edges']:
        if e['kind'] == 'inherits':
            below[e['dst']].add(e['src'])
    byid = {n['id']: n for n in data['nodes']}
    for n in data['nodes']:
        if n['kind'] == 'method' and n.get('cls') in below:
            calls[n['id']].update(f'{s}.{n["name"]}' for s in below[n['cls']] if f'{s}.{n["name"]}' in byid)
    return calls


def choose_route(data, calls, targets, weight, said, entries_too=True):
    """The route that reaches the most of the targets with the fewest frames to open, and how it was found. First among
    the functions an entry calls directly (an entry is the sum of what it dispatches to, so each is a finer route) and
    the exports at a part's root; then among the functions nothing in the graph calls (a rule in a table, a function an
    attribute names); the entries themselves last."""
    roots = {p['id'] for p in data.get('parts', [])}
    code = [n for n in data['nodes'] if n['kind'] in ('function', 'method') and not is_test(n)]
    entries = [n['id'] for n in data['nodes'] if (n.get('entry') or {}).get('kind') not in (None, 'test', 'export')]
    commands = {c for e in entries for c in calls[e]} | {
        n['id'] for n in code if (n.get('entry') or {}).get('kind') == 'export' and n.get('module') in roots and n['kind'] == 'function'}
    called = {g for gs in calls.values() for g in gs}
    uncalled = {n['id'] for n in code if n['id'] not in called and n['id'] not in entries}
    tiers = [('a command or a root export', commands), ('a function nothing in the graph calls', uncalled)]
    for how, candidates in tiers + ([('an entry', set(entries))] if entries_too else []):
        route, paths = best_route(data, calls, candidates, targets, weight, said)
        if route:
            return route, paths, how
    return None, {}, None


def best_route(data, calls, candidates, targets, weight, said):
    """Of the candidates, the one that reaches the most targets, then opens the fewest frames on the calls to them (the
    functions on the paths, each once), then calls the fewest steps deep, then is named by the issue's title (`said`),
    then has the most lines of its own the diff edits."""
    byid = {n['id']: n for n in data['nodes']}
    best = None
    for c in sorted(candidates):
        if c not in byid or byid[c]['kind'] not in ('function', 'method'):
            continue
        depth, parent, frontier = {c: 0}, {}, [c]
        while frontier:
            nxt = []
            for f in frontier:
                for g in calls[f]:
                    if g not in depth:
                        depth[g], parent[g] = depth[f] + 1, f
                        nxt.append(g)
            frontier = nxt
        reached = [t for t in targets if t in depth]
        paths = {t: path_to(parent, c, t) for t in reached}
        frames = len({f for p in paths.values() for f in p[:-1]})
        score = (-len(reached), frames, sum(depth[t] for t in reached), byid[c]['name'] not in said, -weight.get(c, 0), c)
        if reached and (best is None or score < best[0]):
            best = (score, c, paths)
    return (best[1], best[2]) if best else (None, {})


def path_to(parent, start, target):
    out = [target]
    while out[-1] != start:
        out.append(parent[out[-1]])
    return out[::-1]


def holder(paths):
    """The functions every path runs through, from the route down: the last is the frame that holds all that changed."""
    common = []
    for fs in zip(*paths.values()):
        if len(set(fs)) > 1:
            break
        common.append(fs[0])
    # a target is a box of its own, never the frame its picture is cut at, unless other changes lie below it
    while len(common) > 1 and all(len(p) == len(common) for p in paths.values()):
        common.pop()
    return common


def change_card(br, atlas, it, out):
    """Card B: Atlas's own code at the issue's first commit's parent and its last, in the route that reaches what changed,
    opened on the calls to it."""
    first, last = it['commits'][0]['sha'], it['commits'][-1]['sha']
    code = [f for f in it['files'] if kind(f) in ('engine', 'check', 'cli') and f.endswith(READ)]
    card = {'kind': 'B', 'since': first[:7] + '^', 'until': last[:7], 'files': code}
    if not code:
        changed = [f for f in it['files'] if kind(f) in ('viewer', 'model', 'engine', 'check', 'cli')]
        card['says'] = ('no picture of Atlas: its commits change no code Atlas reads (Rust, Python, TypeScript)'
                        + (f', only {", ".join(changed)}' if changed else ''))
        return card
    extra = ['--since', first + '^', '--until', last]
    html = out / 'b-change.html'
    started = time.time()
    view_of(atlas, ROOT, html, None, extra)
    card['reading_seconds'] = round(time.time() - started, 1)
    data = page_data(html)
    marks = (data.get('diff') or {}).get('nodes', {})
    kinds = {n['id']: n['kind'] for n in data['nodes']}
    tests = {n['id'] for n in data['nodes'] if is_test(n)}
    marked = {f for f, m in marks.items() if kinds.get(f) in ('function', 'method') and f not in tests}
    old, new = hunks(first + '^', last, code)
    weight = edited(data, old, new)
    touched = set(weight) - tests
    card['marked'] = {f: marks[f] for f in sorted(marks) if f not in tests}
    card['edited'] = sorted(touched - marked)
    # the changed facts of the picture are the functions the engine marks (their own arrow differs); a body edited
    # with the same arrow is outlined where it is drawn, and leads the route only when the engine marks nothing
    targets = sorted(marked) or sorted(touched)
    card['targets'] = {'of': 'marked by the engine' if marked else 'edited (the engine marks no function)', 'functions': targets}
    if not marked:
        card['says'] = ('the engine marks no function: every arrow stays as it was, so Atlas draws its own code the same; '
                        'the functions the diff edits are outlined amber')
    calls = call_graph(data)
    said = set(re.findall(r'\w+', it.get('title') or ''))
    # a second route only for what the first does not reach, never a whole entry nor a function by itself alone
    routes, left = [], targets
    for i in range(2):
        route, paths, how = choose_route(data, calls, left, weight, said, entries_too=not i)
        if not route or (i and set(paths) <= {route}):
            break
        routes.append((route, paths, how))
        left = [t for t in left if t not in paths]
        if not left:
            break
    card['unreached'] = [t for t in targets if not any(t in paths for _, paths, _ in routes)]
    pictures, card['measured'] = [], []
    for route, paths, how in routes:
        cuts = cut_down(br, atlas, html, extra, route, paths, PICTURES - len(pictures), card['measured'])
        pictures += [(how, *c) for c in cuts]
    card['routes'] = []
    for i, (how, route, paths, called_by, view) in enumerate(pictures or [(None, None, {}, [], {})]):
        entry = draw_change(br, atlas, html, extra, route, paths, view, card['edited'], out, f'b{i + 1}', targets)
        entry.update(found_as=how, called_by=called_by)
        card['routes'].append(entry)
    shown = {f for entry in card['routes'] for f in entry['shows']}
    card['not_drawn'] = [t for t in targets if t not in shown and t not in card['unreached']]
    html.unlink()
    return card


def view_for(br, html, route, paths):
    """The view of a route that opens the frames on the calls from it to the changed functions, by the occurrences the
    page's own plan has; a function the plan does not reach opens by name along the call path."""
    view = {'route': route} if route else {}
    if not route:
        return view
    page = br.open(html, view)
    tree = page.evaluate(TREE)
    page.close()
    known = {key for key, _ in tree}
    steps = {}
    for key, fn in tree:
        if fn in paths and fn != route:
            steps.setdefault(fn, []).append(key)
    keys, functions = set(), set()
    for fn in paths:
        if fn == route:
            continue
        if fn in steps:
            parts = min(steps[fn], key=lambda k: (k.count('/'), k)).split('/')
            for i in range(2, len(parts)):
                keys.add('/'.join(parts[:i]))
                # an alternative of a dispatch (`step|override`) shows once its dispatch is open
                if '|' in parts[i - 1]:
                    keys.add('/'.join(parts[:i - 1] + [parts[i - 1].split('|')[0]]))
        else:
            functions.update(paths[fn][1:-1])
    opened = {}
    if keys & known:
        opened['steps'] = sorted(keys & known)
    if functions:
        opened['functions'] = sorted(functions)
    if opened:
        view['open'] = opened
    return view


def boxes_in(br, atlas, html, extra, view):
    view_of(atlas, ROOT, html, view, extra)
    page = br.open(html)
    n = page.evaluate('() => window.__atlas.PICTURE.boxes.size + document.querySelectorAll("#scene g.frame[data-id]").length')
    page.close()
    return n


def cut_down(br, atlas, html, extra, route, paths, budget, measured):
    """The pictures of what a route reaches, each at most BOXES boxes where a cut allows: the route's view; else the view
    of the lowest function every call path runs through; else one picture per call that function makes toward the
    changes, each cut the same way; a single change whose caller is still too fine, the change itself. Each picture is
    (route, what it reaches, the calls that lead to it, its view)."""
    if budget < 1:
        return []
    view = view_for(br, html, route, paths)
    n = boxes_in(br, atlas, html, extra, view)
    measured.append({'route': route, 'reaches': len(paths), 'boxes': n})
    if n <= BOXES:
        return [(route, paths, [], view)]
    common = holder(paths)
    if len(common) > 1:
        below = {t: p[len(common) - 1:] for t, p in paths.items()}
        return [(r, ps, common[:-1] + c, v) for r, ps, c, v in cut_down(br, atlas, html, extra, common[-1], below, budget, measured)]
    if len(paths) == 1:
        # the frame that holds the one change is still too fine: the change itself, its callers named
        (t, p), = paths.items()
        if t == route:
            return [(route, paths, [], view)]
        return [(r, ps, p[:-1] + c, v) for r, ps, c, v in cut_down(br, atlas, html, extra, t, {t: [t]}, budget, measured)]
    groups = collections.defaultdict(dict)
    for t, p in paths.items():
        groups[p[1] if len(p) > 1 else t][t] = p
    if len(groups) < 2:
        return [(route, paths, [], view)]
    out = []
    for g in sorted(groups, key=lambda g: (-len(groups[g]), g))[:budget]:
        out += cut_down(br, atlas, html, extra, route, groups[g], 1, measured)
    return out


def draw_change(br, atlas, html, extra, route, paths, view, edited_fns, out, tag, targets):
    """One picture of B: the route opened on the calls from it to what changed, with what the issue edits and the
    engine does not mark outlined."""
    entry = {'route': route, 'reaches': paths}
    entry['view'] = view
    (out / f'{tag}-view.json').write_text(json.dumps(view, indent=1) + '\n')
    view_of(atlas, ROOT, html, view, extra)
    page = br.open(html)
    if edited_fns:
        boxes = page.evaluate("""(fns) => [...window.__atlas.PICTURE.boxes.keys(), ...[...document.querySelectorAll('#scene g.frame[data-id]')].map(e => e.dataset.id)]
          .filter(id => { const k = id.replace(/^[no]:/, ''), i = window.__atlas.treeInfo.get(k); return fns.includes(i ? i.step : k); })""",
                              edited_fns)
        entry['outlined_edited'] = page.evaluate(OUTLINE, [boxes, [], EDITED])['outlined']
        entry['colours'] = {'edited': EDITED}
    drawn = page.evaluate(DRAWN)
    # the functions the picture draws a box or a frame of
    entry['shows'] = sorted(set(page.evaluate("""() => [...window.__atlas.PICTURE.boxes.keys(), ...[...document.querySelectorAll('#scene g.frame[data-id]')].map(e => e.dataset.id)]
      .map(id => { const k = id.replace(/^[no]:/, ''), i = window.__atlas.treeInfo.get(k); return i ? i.step : k; })""")) & set(targets))
    entry['drawn'] = {'boxes': len(drawn['boxes']), 'lines': len(drawn['lines']),
                      'marked': collections.Counter(c for b in drawn['boxes'] for c in (b['cls'] or '').split() if c.startswith('d-'))}
    picture(page, out / f'{tag}-change.png')
    entry['errors'] = page.errors
    page.close()
    entry['picture'] = f'{tag}-change.png'
    return entry


# ---------- C: where no picture shows it ----------

def text_diff(before, after):
    """The lines only one of two texts has."""
    a, b = before.splitlines(), after.splitlines()
    sm = difflib.SequenceMatcher(None, a, b, autojunk=False)
    gone, new = [], []
    for op, i1, i2, j1, j2 in sm.get_opcodes():
        if op != 'equal':
            gone += a[i1:i2]
            new += b[j1:j2]
    return {'only_before': gone, 'only_after': new, 'equal': not gone and not new}


def run(build, args, cwd=ROOT):
    env = {**os.environ, 'COLUMNS': '100', 'NO_COLOR': '1', 'TERM': 'dumb'}
    r = subprocess.run([str(build), *args], capture_output=True, text=True, cwd=cwd, env=env, check=False)
    # a build is named by its file (`atlas-b423b56`); its output names it `atlas`, as an installed one does
    text = re.sub(r'\x1b\[[0-9;]*m', '', r.stdout + r.stderr).replace(str(build), 'atlas-code')
    return re.sub(rf'\b{re.escape(pathlib.Path(build).name)}\b', 'atlas-code', text), r.returncode


def command_card(builds, it, out):
    """The interface of the command an issue changes, with both builds: the help of the one its title names, else
    `atlas-code --help`. Only its help is run, since a command itself may write a page, open a browser or serve."""
    said = ' '.join([it.get('title') or ''] + [c['subject'] for c in it['commits']])
    m = re.search(r'`?atlas((?:\s+[a-z][\w-]*)?(?:\s+--?[a-z][\w-]*)+|\s+[a-z][\w-]*)', said)
    args = [w for w in (m.group(1).split() if m else []) if not w.startswith('-')] + ['--help']
    card = {'kind': 'C', 'what': 'command', 'command': 'atlas ' + ' '.join(args),
            'why': 'a command-line text: no picture draws it, so its output is shown with both builds'}
    for side, build in builds.items():
        text, code = run(build, args)
        (out / f'c-command-{side}.txt').write_text(text)
        card[side] = {'text': f'c-command-{side}.txt', 'exit': code, 'lines': len(text.splitlines())}
    card['difference'] = text_diff((out / 'c-command-before.txt').read_text(), (out / 'c-command-after.txt').read_text())
    return card


def programs_in(files):
    """The pair programs a set of files lies in: each nearest directory above a file that has a manifest."""
    out = []
    for f in files:
        d = (ROOT / f).parent
        while d != ROOT and not any((d / m).is_file() for m in MANIFESTS):
            d = d.parent
        rel = str(d.relative_to(ROOT)) if d != ROOT else None
        if rel and rel.startswith(('engine/tests/pairs', 'engine/tests/plans')) and rel not in out:
            out.append(rel)
    return out


def check_card(builds, it, out):
    """`atlas-code check` on the pairs the issue changes, with both builds."""
    programs = programs_in([f for f in it['files'] if kind(f) == 'subject'])[:PAIRS]
    card = {'kind': 'C', 'what': 'check', 'why': 'a finding is text: `atlas-code check` on the pairs the issue changes, with both builds',
            'pairs': []}
    for i, program in enumerate(programs):
        pair = {'program': program}
        for side, build in builds.items():
            text, code = run(build, ['check', str(ROOT / program)])
            name = f'c-check-{i}-{side}.txt'
            (out / name).write_text(text)
            total = re.search(r'^(\d+) questions?$', text, re.MULTILINE)
            pair[side] = {'text': name, 'exit': code, 'questions': int(total.group(1)) if total else None}
        pair['difference'] = text_diff((out / pair['before']['text']).read_text(), (out / pair['after']['text']).read_text())
        card['pairs'].append(pair)
    return card if programs else None


GESTURE = r"""async ([box, does]) => {
  const A = window.__atlas, el = () => document.querySelector(`#scene g.tstep[data-id="${CSS.escape(box)}"]`);
  const wait = ms => new Promise(r => setTimeout(r, ms));
  const said = [];
  for (const d of does) {
    const g = el();
    if (!g) { said.push(`${d}: no box ${box}`); break; }
    const n = window.layouts;
    if (d === "pin") { g.dispatchEvent(new MouseEvent("click", {bubbles: true})); await wait(40); g.dispatchEvent(new MouseEvent("click", {bubbles: true})); await wait(300); }
    if (d === "open") { g.dispatchEvent(new MouseEvent("click", {bubbles: true})); await wait(400); for (let i = 0; i < 100 && window.layouts === n; i++) await wait(100); await wait(400); }
    said.push(`${d}: pinned ${[...A.pinned].join(", ") || "nothing"}`);
  }
  const lit = A.PICTURE.lines.filter(e => e.els.some(x => x.classList.contains("on"))).length;
  return {said, pinned: [...A.pinned], lit_lines: lit, lines: A.PICTURE.lines.length,
          lit_boxes: [...A.PICTURE.boxes.values()].filter(x => x.classList.contains("on")).length, boxes: A.PICTURE.boxes.size};
}"""


def gesture_card(br, builds, it, out):
    """A gesture in the viewer, played in both builds on one subject."""
    g = GESTURES.get(it['issue'])
    card = {'kind': 'C', 'what': 'gesture', 'why': 'a viewer change shows only in what a reader does: '
            + (g['says'] + ', in both builds' if g else 'no gesture is written down for it (GESTURES)')}
    if not g:
        return card
    card.update(subject=g['subject'], route=g['route'], box=g['box'], does=g['does'], pictures={})
    for side, build in builds.items():
        html = out / f'c-gesture-{side}.html'
        view_of(build, ROOT / g['subject'], html)
        r = g['route'] if side == 'after' else route_in(page_data(html), g['route'])
        page = br.open(html, {'route': r})
        card[side] = page.evaluate(GESTURE, [g['box'], g['does']])
        settled(page)
        picture(page, out / f'c-gesture-{side}.png')
        card['pictures'][side] = f'c-gesture-{side}.png'
        card.setdefault('errors', []).extend(page.errors)
        page.close()
    return card


def docs_card(it, out):
    """The sections of the documentation the issue's commits change, before and after."""
    docs = [f for f in it['files'] if kind(f) in ('docs', 'check') and f.endswith('.md')]
    if not docs:
        return None
    first, last = it['commits'][0]['sha'] + '^', it['commits'][-1]['sha']
    sections = []
    for path in docs:
        a, b = text_at(first, path).splitlines(), text_at(last, path).splitlines()
        sm = difflib.SequenceMatcher(None, a, b, autojunk=False)
        for op, i1, i2, j1, j2 in sm.get_opcodes():
            if op == 'equal':
                continue
            # the paragraph around the change, on each side
            grow = lambda lines, lo, hi: (next((i for i in range(lo - 1, -1, -1) if not lines[i].strip()), -1) + 1,
                                          next((i for i in range(hi, len(lines)) if not lines[i].strip()), len(lines)))
            (oa, ob), (na, nb) = grow(a, i1, i2), grow(b, j1, j2)
            heading = next((line for line in reversed(b[:max(j1, na) + 1]) if line.startswith('#')), '')
            sections.append({'file': path, 'heading': heading, 'before': '\n'.join(a[oa:ob]), 'after': '\n'.join(b[na:nb])})
    merged = []
    for s in sections:
        if merged and merged[-1]['file'] == s['file'] and merged[-1]['after'] == s['after']:
            continue
        merged.append(s)
    (out / 'c-docs.json').write_text(json.dumps(merged, indent=1, ensure_ascii=False) + '\n')
    return {'kind': 'C', 'what': 'docs', 'why': 'the documentation the change writes, before and after', 'sections': len(merged),
            'text': 'c-docs.json'}


INTERNAL = {'engine', 'test', 'tooling', 'other'}


def report(it, builds, br, out, cards):
    out.mkdir(parents=True, exist_ok=True)
    started = time.time()
    kinds = set(it['kinds'])
    entry = {k: it[k] for k in ('issue', 'title', 'state', 'labels', 'kinds')}
    entry['commits'] = [{'sha': c['sha'][:7], 'subject': c['subject']} for c in it['commits']]
    entry['cards'] = []
    say = entry['cards'].append

    def attempt(what, f, *args):
        try:
            card = f(*args)
            if card:
                say(card)
        except Exception as e:  # noqa: BLE001 - a card that cannot be drawn says why; the others are still drawn
            say({'kind': what, 'failed': f'{type(e).__name__}: {e}'})

    if 'A' in cards:
        for i, ((plan, route), facts) in enumerate(subjects_of(it)):
            attempt('A', user_card, br, builds, plan.removesuffix('.plan'), route, facts, out, f'a{i + 1}')
    if 'B' in cards:
        attempt('B', change_card, br, builds['after'], it, out)
    if 'C' in cards:
        if 'cli' in kinds:
            attempt('C', command_card, builds, it, out)
        if 'check' in kinds:
            attempt('C', check_card, builds, it, out)
        if kinds & {'viewer', 'model'}:
            attempt('C', gesture_card, br, builds, it, out)
        if kinds & {'docs', 'check'}:
            attempt('C', docs_card, it, out)
        drawn_for_user = any(c['kind'] == 'A' for c in entry['cards'])
        if not drawn_for_user and kinds <= INTERNAL and not any(kind(f) == 'test' and 'pairs' in f for f in it['files']):
            say({'kind': 'C', 'what': 'internal',
                 'why': 'internal: its commits change no plan, pair, check, command line, viewer or documentation, '
                        'so what a user is shown is the same; the change is in Atlas\'s own code (B)'})
    entry['seconds'] = round(time.time() - started, 1)
    return entry


def main():
    ap = argparse.ArgumentParser(description=__doc__.split('\n\n')[0])
    ap.add_argument('before')
    ap.add_argument('after')
    ap.add_argument('base')
    ap.add_argument('head')
    ap.add_argument('outdir')
    ap.add_argument('--issues', help='comma-separated issue numbers; default: every issue BASE..HEAD serves')
    ap.add_argument('--cards', default='A,B,C', help='which kinds of card to draw (A, B, C)')
    a = ap.parse_args()
    builds = {'before': pathlib.Path(a.before).resolve(), 'after': pathlib.Path(a.after).resolve()}
    out = pathlib.Path(a.outdir).resolve()
    out.mkdir(parents=True, exist_ok=True)
    head = git('rev-parse', a.head).strip()
    if git('rev-parse', 'HEAD').strip() != head or git('status', '--porcelain', '--', 'engine/tests'):
        sys.exit(f'the subjects are drawn as HEAD has them: check out {a.head} with a clean engine/tests first')
    issues = inventory(a.base, a.head)
    wanted = [int(n) for n in a.issues.split(',')] if a.issues else sorted(issues)
    versions = {side: run(b, ['--version'])[0].strip() for side, b in builds.items()}
    result = {'base': a.base, 'head': a.head, 'builds': versions, 'issues': []}
    with tempfile.TemporaryDirectory() as tmp, sync_playwright() as p:
        br = Browser(p, pathlib.Path(tmp))
        for n in wanted:
            if n not in issues:
                result['issues'].append({'issue': n, 'failed': f'no commit in {a.base}..{a.head} serves #{n}'})
                continue
            entry = report(issues[n], builds, br, out / str(n), set(a.cards.split(',')))
            result['issues'].append(entry)
            print(f'#{n}: {", ".join(c["kind"] + ("!" if "failed" in c else "") for c in entry["cards"]) or "no card"}'
                  f' in {entry["seconds"]}s', flush=True)
        br.browser.close()
    (out / 'report.json').write_text(json.dumps(result, indent=1, ensure_ascii=False, default=list) + '\n')


if __name__ == '__main__':
    main()
