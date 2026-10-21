"""Write the Atlas pill for a README, as one SVG file.

    python3 tools/atlas_badge.py OUT.svg                 the pill: atlas | drawn on push
    python3 tools/atlas_badge.py OUT.svg FINDINGS.json   a pill of its own: atlas | N questions

The second form counts what `atlas check --json` found, teal at none and amber otherwise. It is
experimental: the questions Atlas asks are not yet reliable enough to wear as a badge.
"""

import json
import pathlib
import sys
from xml.sax.saxutils import escape

INK, VERMILION, TEAL, AMBER = '#141210', '#C43A14', '#3f7f8f', '#a8761a'
ICON = """<svg x="6" y="3" width="14" height="14" viewBox="0 0 128 128">
    <path d="M25 104 L64 25 L103 104 M40 84 L88 84" stroke="#f4f1ec" stroke-opacity="0.75" stroke-width="7" fill="none" stroke-linecap="round" stroke-linejoin="round"/>
    <g fill="#86b9c9"><rect x="16" y="95" width="18" height="18"/><rect x="55" y="16" width="18" height="18"/><rect x="26" y="75" width="18" height="18"/><rect x="85" y="75" width="18" height="18"/></g>
    <rect x="95" y="95" width="18" height="18" fill="#f2643c"/>
  </svg>"""


def width(text):
    return round(5.6 * len(text) + 10)


def render(segments):
    """segments: (text, fill) pairs; the first sits beside the icon."""
    label = [width(segments[0][0]) + 16] + [width(t) for t, _ in segments[1:]]
    total = sum(label)
    name = ': '.join(t for t, _ in segments)
    out = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{total}" height="20" viewBox="0 0 {total} 20" role="img" aria-label="{escape(name)}">',
           f'  <title>{escape(name)}</title>',
           f'  <clipPath id="r"><rect width="{total}" height="20" rx="3"/></clipPath>',
           '  <g clip-path="url(#r)">']
    x = 0
    for (_, fill), w in zip(segments, label):
        out.append(f'    <rect x="{x}" width="{w}" height="20" fill="{fill}"/>')
        x += w
    out += ['  </g>',
            '  <!-- The logo\'s route, simplified to read at 14px: A with its crossbar, ending in vermilion. -->',
            f'  {ICON}',
            '  <g fill="#fff" font-family="Verdana,Geneva,DejaVu Sans,sans-serif" font-size="10" text-rendering="geometricPrecision">']
    x = 0
    for k, ((text, _), w) in enumerate(zip(segments, label)):
        start = x + (24 if k == 0 else 5)
        length = w - (24 + 4 if k == 0 else 10)
        colour = ' fill="#f4f1ec"' if k == 0 else ''
        out.append(f'    <text x="{start}" y="14" textLength="{length}"{colour}>{escape(text)}</text>')
        x += w
    out += ['  </g>', '</svg>', '']
    return '\n'.join(out)


def main(out, findings=None):
    segments = [('atlas', INK), ('drawn on push', VERMILION)]
    if findings:
        n = len(json.loads(pathlib.Path(findings).read_text())['findings'])
        segments[1] = (f'{n} question' + ('' if n == 1 else 's'), AMBER if n else TEAL)
    pathlib.Path(out).write_text(render(segments))


if __name__ == '__main__':
    main(*sys.argv[1:3])
