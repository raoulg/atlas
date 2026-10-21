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
