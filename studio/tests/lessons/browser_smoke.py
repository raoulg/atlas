# /// script
# requires-python = ">=3.11"
# dependencies = ["playwright"]
# ///
"""Exercise the lesson panel with a local Studio bridge and lesson fixtures."""

from __future__ import annotations

import http.server
import socketserver
import threading
from pathlib import Path

from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[3]


class Server(socketserver.ThreadingTCPServer):
    allow_reuse_address = True


handler = lambda *args, **kwargs: http.server.SimpleHTTPRequestHandler(*args, directory=str(ROOT), **kwargs)
with Server(("127.0.0.1", 0), handler) as server:
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    base = f"http://127.0.0.1:{server.server_address[1]}"
    bridge = r"""
      const stored = new Map();
      let output = false;
      window.__TAURI__ = {
        event: {listen: async () => () => {}},
        core: {invoke: async (command, args = {}) => {
          const lessonBase = 'studio/tests/lessons/vector-cache/';
          if (command === 'current_workspace') return {path: '/fixture', name: 'vector-cache', url: location.origin + '/mock-atlas'};
          if (command === 'list_lessons') return [lessonBase + 'lesson.md', lessonBase + 'lesson.html'];
          if (command === 'open_lesson') return {text: stored.get(args.path) || await (await fetch('/' + args.path)).text(), script: 'demo.py'};
          if (command === 'read_lesson_reference') {
            const path = lessonBase + args.reference;
            return stored.get(path) || await (await fetch('/' + path)).text();
          }
          if (command === 'read_lesson_asset') {
            if (!output) throw new Error('Run the script first');
            return Array.from(new TextEncoder().encode('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><circle cx="5" cy="5" r="3"/></svg>'));
          }
          if (command === 'run_lesson') {
            output = true;
            return {stdout: 'query (2,) → rose\n', stderr: '', code: 0, artifacts: ['output/flowers.svg']};
          }
          if (command === 'current_atlas_view') return JSON.stringify({view: {route: 'demo', open: {functions: ['demo.nearest']}}, selected: 'demo.nearest'});
          if (command === 'save_lesson_view') {
            const ref = 'views/' + args.name + '.json';
            stored.set(lessonBase + ref, args.view);
            return ref;
          }
          if (command === 'save_lesson') { stored.set(args.path, args.text); return null; }
          throw new Error(command);
        }}
      };
    """
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(channel="chrome")
        page = browser.new_page(viewport={"width": 1440, "height": 900})
        page.add_init_script(bridge)
        page.goto(base + "/studio/ui/index.html", wait_until="networkidle")
        page.locator('.side-lesson[data-path$="lesson.md"]').click()
        page.locator('.lesson-article h1').wait_for()
        assert page.locator('.lesson-view iframe').count() == 1
        assert 'embed=lesson' in page.locator('.lesson-view iframe').first.get_attribute('src')
        assert page.locator('.lesson-view iframe').first.bounding_box()['width'] > 950
        assert page.locator('.lesson-source').is_hidden()
        page.get_by_role('button', name='Full view ↗').first.click()
        assert 'at=_root.demo.nearest' in page.locator('#atlas-frame').get_attribute('src')
        page.locator('.side-lesson[data-path$="lesson.md"]').click()
        page.get_by_role('button', name='Source', exact=True).first.click()
        page.locator('.lesson-source').wait_for(state='visible')
        page.get_by_role('button', name='Close', exact=True).click()
        assert page.locator('.lesson-source').is_hidden()
        assert "Image unavailable" in page.locator('.lesson-article').inner_text()
        page.get_by_role("button", name="Run script").click()
        page.locator('.lesson-article img[src^="blob:"]').wait_for()
        assert 'query (2,) → rose' in page.locator('.lesson-terminal').inner_text()
        assert page.locator('.lesson-terminal').bounding_box()['y'] < 900
        page.get_by_role("button", name="Insert view in lesson").click()
        page.wait_for_function("document.querySelectorAll('.lesson-view').length === 2")
        page.locator('.side-lesson[data-path$="lesson.html"]').click()
        page.get_by_role("heading", name="Vector cache in HTML").wait_for()
        assert page.locator(".lesson-view iframe").count() == 1
        assert page.locator(".lesson-article img[src^=\"blob:\"]").count() == 1
        screenshot = Path("/private/tmp/atlas-studio-lesson-smoke.png")
        page.screenshot(path=str(screenshot), full_page=True)
        browser.close()
    server.shutdown()
print(f"Studio lesson UI smoke passed: {screenshot}")
