(function (root) {
  'use strict';

  // The workspace sidebar folds to a rail that keeps the section icons. Folding only hides what the
  // sidebar holds, so the file tree keeps its open folders and the lesson list its place; the scroll
  // offsets are put back when it unfolds. The choice is remembered in this webview's storage.

  const KEY = 'atlas-studio-sidebar';

  // The remembered choice: folded only when it says so.
  const foldedIn = (kept) => kept === 'rail';

  // Whether a key event is the toggle: ⌘B on macOS, Ctrl+B elsewhere, with no other modifier.
  function isToggle(event, mac) {
    const modifier = mac ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey;
    return Boolean(modifier) && !event.shiftKey && !event.altKey && String(event.key).toLowerCase() === 'b';
  }

  const titleOf = (folded, mac) => `${folded ? 'Show' : 'Hide'} the sidebar (${mac ? '⌘B' : 'Ctrl+B'})`;

  // Wires the toggle `button` to the `workbench`; `held` are the scrolling elements the sidebar keeps.
  // `skip(event)` says that a key belongs to what has focus (the terminal), not to the toggle.
  function mount({workbench, button, held = [], storage, mac, skip = () => false}) {
    const offsets = new Map();
    let folded = false;
    const put = (next, remember) => {
      if (next) held.forEach((each) => offsets.set(each, each.scrollTop));
      folded = next;
      workbench.dataset.sidebar = next ? 'rail' : 'open';
      button.setAttribute('aria-expanded', String(!next));
      button.title = titleOf(next, mac);
      button.querySelector('.glyph').textContent = next ? '›' : '‹';
      if (!next) held.forEach((each) => { each.scrollTop = offsets.get(each) || 0; });
      if (remember) { try { storage.setItem(KEY, next ? 'rail' : 'open'); } catch {} }
    };
    let kept = null;
    try { kept = storage.getItem(KEY); } catch {}
    put(foldedIn(kept), false);
    const toggle = () => put(!folded, true);
    button.addEventListener('click', toggle);
    root.document.addEventListener('keydown', (event) => {
      if (!isToggle(event, mac) || skip(event)) return;
      event.preventDefault();
      toggle();
    });
    return {toggle, isFolded: () => folded};
  }

  root.AtlasSidebar = {KEY, foldedIn, isToggle, titleOf, mount};
})(typeof window !== 'undefined' ? window : globalThis);
