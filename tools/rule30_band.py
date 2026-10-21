# /// script
# requires-python = ">=3.11"
# ///
"""Draw the Rule 30 band with Atlas's logo in it as two SVGs, for the README: a light and a dark one.

    uv run tools/rule30_band.py

The band is the tour's (docs/index.html): Rule 30 from a single seed, generations 37-49 over every
column that lives in them (-49..49), the cells squares of 0.78125 of the pitch, generation 43 in
accent. Atlas's logo is a window onto the same automaton, generations 37-49 over columns 4..16, so it
stands in the band at its own columns and the band's cells run on into it. The background is left
open, so the band sits on whatever page GitHub draws it on.
"""

import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parents[1]
SKIP, ROWS, COL_MIN, COL_MAX, ACCENT_ROW, SQUARE, LOGO_COL = 37, 13, -49, 49, 6, 0.78125, 4
PITCH = 10
THEMES = {'light': ('#E2DCD3', '#C43A14'), 'dark': ('#2B2621', '#F2643C')}


def grid():
    """The generations of the window, each the cells of its columns."""
    reach = SKIP + ROWS
    row = [0] * (COL_MAX - COL_MIN + 1 + 2 * reach)
    row[-COL_MIN + reach] = 1
    rows = []
    for g in range(SKIP + ROWS):
        if g >= SKIP:
            rows.append(row[reach:reach + COL_MAX - COL_MIN + 1])
        row = [0] + [row[i - 1] ^ (row[i] | row[i + 1]) for i in range(1, len(row) - 1)] + [0]
    return rows


def logo():
    """The logo's drawing, without its svg element and title, and the size of its square."""
    text = (ROOT / 'docs/logo.svg').read_text()
    size = float(re.search(r'viewBox="0 0 ([\d.]+) ', text).group(1))
    inner = text[text.index('>', text.index('<svg')) + 1:text.rindex('</svg>')]
    return re.sub(r'\s*<title>.*?</title>|\s*<!--.*?-->', '', inner, flags=re.S).strip(), size


def band(cell, accent):
    width, height = (COL_MAX - COL_MIN + 1) * PITCH, ROWS * PITCH
    inset, size = (1 - SQUARE) / 2 * PITCH, SQUARE * PITCH
    paths = {False: [], True: []}
    for r, cols in enumerate(grid()):
        for i, alive in enumerate(cols):
            if alive:
                paths[r == ACCENT_ROW].append(f'M{i * PITCH + inset:g} {r * PITCH + inset:g}h{size:g}v{size:g}h-{size:g}z')
    drawing, square = logo()
    left = (LOGO_COL - COL_MIN) * PITCH
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}" width="{width}" height="{height}" '
            f'role="img" aria-label="Atlas">\n<title>Atlas</title>\n'
            f'<path fill="{cell}" d="{"".join(paths[False])}"/>\n<path fill="{accent}" d="{"".join(paths[True])}"/>\n'
            f'<g transform="translate({left} 0) scale({height / square:g})">\n{drawing}\n</g>\n</svg>\n')


def main():
    for theme, (cell, accent) in THEMES.items():
        out = ROOT / f'docs/img/band-{theme}.svg'
        out.write_text(band(cell, accent))
        print(out.relative_to(ROOT))


if __name__ == '__main__':
    main()
