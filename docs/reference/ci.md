# Atlas — the picture in your README, always current

A picture of the code in a README is out of date from the first commit after it was drawn. Atlas
draws it from the code, so it can be drawn again whenever the code changes: on every push, by a
GitHub workflow, or before every commit, by a hook. The README then shows the code as it is, and a
click on the picture opens the page to explore it.

The workflow runs on GitHub's machines: it needs nothing installed on yours. That makes it the way
in where Atlas cannot be installed locally, on a laptop someone else manages for instance: the
page it draws is opened in the browser, from GitHub Pages or as a download.

Both draw the page with `atlas view`, and take a picture of it with one small script.

## The picture script

`atlas view` writes the page as one HTML file. A README shows images, so this script opens the
page in Chrome and takes a picture of it, in the light and the dark theme. Save it in your
repository as `tools/atlas_picture.py`:

```python
# /// script
# requires-python = ">=3.11"
# dependencies = ["playwright"]
# ///
"""Take a picture of a page `atlas view` wrote, in the light and the dark theme.

    uv run tools/atlas_picture.py PAGE PICTURE

writes PICTURE (light) and PICTURE with `-dark` before its suffix. It draws in the installed Chrome.
"""

import pathlib
import sys

from playwright.sync_api import sync_playwright

page_file, picture = pathlib.Path(sys.argv[1]).resolve(), pathlib.Path(sys.argv[2])
with sync_playwright() as p:
    browser = p.chromium.launch(channel="chrome")
    for theme, out in [("light", picture), ("dark", picture.with_stem(picture.stem + "-dark"))]:
        context = browser.new_context(viewport={"width": 1400, "height": 800}, device_scale_factor=2,
                                      color_scheme=theme, reduced_motion="reduce")
        context.add_init_script("localStorage.setItem('atlas.code', '0')")  # no code panel
        page = context.new_page()
        page.goto(page_file.as_uri(), wait_until="networkidle")
        page.wait_for_function("document.querySelectorAll('#scene g[data-id]').length > 0", timeout=120_000)
        page.evaluate("document.fonts.ready")
        page.wait_for_timeout(500)
        page.screenshot(path=str(out))
        context.close()
    browser.close()
```

It runs with [uv](https://docs.astral.sh/uv/), which brings Playwright along, and draws in the
Chrome that is installed: GitHub's Ubuntu runners have it. Where there is none, `uvx playwright
install chromium` fetches Playwright's own, and `channel="chrome"` is left out.

## Which picture

Without more, the picture is the overview Atlas opens on: every part of the repository as one step,
from what it reads to what it writes. To show one route instead (your command, your API endpoint),
name it with `--view`:

```bash
atlas view --no-open --out site/index.html --view '{"route": "mypackage.cli.main"}'
```

The id is the one the page's address writes once the route is open (`…&v={"route": …}`); the page
the workflow publishes has it too, so it can be found without Atlas on your machine. `--view` also
takes the functions to open in it, as `atlas view --help` says.

## On every push: a GitHub workflow

This workflow draws the page on every push to `main`, takes its picture, and publishes both on
GitHub Pages. Turn Pages on first: *Settings → Pages → Build and deployment → Source: GitHub
Actions*. Then add `.github/workflows/atlas.yml`:

```yaml
name: atlas

on:
  push:
    branches: [main]
  workflow_dispatch:

permissions:
  contents: read
  pages: write
  id-token: write

concurrency:
  group: pages
  cancel-in-progress: true

jobs:
  picture:
    runs-on: ubuntu-latest
    environment:
      name: github-pages
      url: ${{ steps.deploy.outputs.page_url }}
    steps:
      - uses: actions/checkout@v4
      - uses: astral-sh/setup-uv@v6
      - name: Install the project's environment     # see "What Atlas reads on the runner"
        run: uv sync
      - name: Install Atlas
        run: |
          curl -fsSL https://raw.githubusercontent.com/raoulg/atlas/main/install.sh | sh
          echo "$HOME/.local/bin" >> "$GITHUB_PATH"
      - name: Draw the page
        run: atlas view --no-open --out site/index.html
      - name: Take its picture
        run: uv run tools/atlas_picture.py site/index.html site/atlas.png
      - uses: actions/upload-pages-artifact@v3
        with:
          path: site
      - id: deploy
        uses: actions/deploy-pages@v4
```

After the first run the page is at `https://<owner>.github.io/<repository>/`, and the pictures
beside it. Put them in the README, the dark one for readers in the dark theme, the whole a link to
the page:

```html
<a href="https://<owner>.github.io/<repository>/">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://<owner>.github.io/<repository>/atlas-dark.png">
    <img src="https://<owner>.github.io/<repository>/atlas.png" alt="The picture Atlas draws of this repository">
  </picture>
</a>
```

The README itself never changes: the pictures it names are drawn anew on every push. GitHub shows
images in a README through its own image cache, which may show the earlier picture for a while
after a push.

`ATLAS_CHANNEL=nightly` before `sh` installs the nightly build instead of the latest release. The
runner reads the update check as CI and stays silent.

### A private repository

The page holds the code it draws: a box opens on its source. A Pages site is public, also for a
private repository, unless the plan restricts who can see it (GitHub Enterprise Cloud). For code
that is not public, keep the page as a download of the run instead of a site. Replace the last two
steps with:

```yaml
      - uses: actions/upload-artifact@v4
        with:
          name: atlas
          path: site
```

Everyone with access to the repository then finds `atlas.zip` under the run (*Actions → atlas →
the run → Artifacts*): unpack it and open `index.html` in a browser. The pictures can then not be
named from the README by a URL; the hook below commits them into the repository instead.

### What Atlas reads on the runner

Atlas reads what the runner has, and draws the libraries your code calls as far as it finds them
there. Install them before Atlas runs, as you would to build:

| Language | Step | Without it |
| --- | --- | --- |
| Python | `uv sync` (or a `.venv` made otherwise; Atlas reads the project's `.venv` or the environment `[tool.ty.environment]` names) | libraries have no types: a call to `requests.get` is known, what it gives back is not |
| Rust | `cargo fetch` | the crates your code calls stay closed |
| TypeScript | `npm ci` (or your package manager's) | the packages in `node_modules` stay closed |

A step for a language the repository does not have is left out; `uv sync` in a repository
without a `pyproject.toml` fails.

### Checks on a pull request

The same install gives a job that runs `atlas check` on every pull request. It exits with 1 when
the code and a principle disagree, so the job fails and the findings stand in its log:

```yaml
on:
  pull_request:

jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
      - name: Install Atlas
        run: |
          curl -fsSL https://raw.githubusercontent.com/raoulg/atlas/main/install.sh | sh
          echo "$HOME/.local/bin" >> "$GITHUB_PATH"
      - run: atlas check
      - name: Draw what the pull request changed
        if: always()
        run: atlas view --no-open --out changes/index.html --since origin/${{ github.base_ref }}
      - uses: actions/upload-artifact@v4
        if: always()
        with:
          name: atlas-changes
          path: changes
```

The second half draws the base branch and the pull request as one picture, what was added,
removed and changed marked, and keeps it as a download of the run: a review that starts from
the picture instead of the diff. `fetch-depth: 0` gives the runner the base branch to compare with.

## Before every commit: a hook

Where Atlas is installed, a hook draws the picture before each commit and commits it with the
code, so the picture in the repository always belongs to the commit it is in. The README names
it by its path:

```html
<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/atlas-dark.png">
  <img src="docs/atlas.png" alt="The picture Atlas draws of this repository">
</picture>
```

As a plain git hook, `.git/hooks/pre-commit` (made executable with `chmod +x`):

```sh
#!/bin/sh
set -e
page="$(mktemp -d)/atlas.html"
atlas view --no-open --out "$page"
uv run tools/atlas_picture.py "$page" docs/atlas.png
git add docs/atlas.png docs/atlas-dark.png
```

or with [lefthook](https://github.com/evilmartians/lefthook), in `lefthook.yml`, which every clone
shares:

```yaml
pre-commit:
  commands:
    atlas:
      run: >
        page="$(mktemp -d)/atlas.html" &&
        atlas view --no-open --out "$page" &&
        uv run tools/atlas_picture.py "$page" docs/atlas.png &&
        git add docs/atlas.png docs/atlas-dark.png
```

The page goes to a temporary folder, outside the repository. Only the pictures are committed; the page holds the code it draws and grows with it, and has no place in the history.
The hook draws the working tree, also what is not staged, so a commit of part of your changes
carries a picture of all of them.
