(function (root) {
  'use strict';

  // The workspaces Studio has opened, most recent first: a page (⌘O) to filter them and open one,
  // or a new folder. With no workspace open it is the page Studio starts on.

  const node = (tag, className, label) => {
    const element = document.createElement(tag);
    if (className) element.className = className;
    if (label !== undefined) element.textContent = label;
    return element;
  };

  // how long ago, in words: "today", "yesterday", "3 days ago"
  function ago(seconds, now = Date.now() / 1000) {
    const days = Math.floor(now / 86400) - Math.floor(seconds / 86400);
    if (days <= 0) return 'today';
    const words = new Intl.RelativeTimeFormat('en', {numeric: 'auto'});
    if (days < 7) return words.format(-days, 'day');
    if (days < 60) return words.format(-Math.round(days / 7), 'week');
    return words.format(-Math.round(days / 30), 'month');
  }

  // the workspaces whose name or path holds every word of `filter`
  function matching(list, filter) {
    const words = filter.toLowerCase().split(/\s+/).filter(Boolean);
    return list.filter((recent) => words.every((word) => (recent.name + ' ' + recent.path).toLowerCase().includes(word)));
  }

  function mount(host, bridge) {
    const card = node('div', 'workspaces-card');
    const mark = node('div', 'welcome-mark', '◇');
    const eyebrow = node('p', 'eyebrow', 'ATLAS STUDIO');
    const title = node('h1', '', 'Workspaces');
    const lead = node('p', 'workspaces-lead', 'Open a repository to read its live Atlas view and history.');
    const filter = node('input', 'workspaces-filter');
    filter.type = 'search';
    filter.placeholder = 'Filter recent workspaces';
    filter.setAttribute('aria-label', 'Filter recent workspaces');
    const list = node('ol', 'workspaces-list');
    list.setAttribute('role', 'listbox');
    const empty = node('p', 'workspaces-empty', 'No workspaces opened yet.');
    const actions = node('div', 'workspaces-actions');
    const folder = node('button', 'primary-button', 'Open folder…');
    folder.type = 'button';
    folder.append(node('kbd', '', '⇧⌘O'));
    const close = node('button', 'secondary-button', 'Back to the workspace');
    close.type = 'button';
    close.append(node('kbd', '', 'esc'));
    actions.append(folder, close);
    card.append(mark, eyebrow, title, lead, filter, list, empty, actions);
    host.append(card);

    let recent = [];
    let shown = [];
    let at = 0;
    let closable = false;

    function draw() {
      shown = matching(recent, filter.value);
      at = Math.max(0, Math.min(at, shown.length - 1));
      list.replaceChildren();
      shown.forEach((workspace, index) => {
        const item = node('li', 'workspace-item');
        item.setAttribute('role', 'option');
        item.setAttribute('aria-selected', String(index === at));
        item.classList.toggle('active', index === at);
        item.classList.toggle('current', workspace.path === bridge.current());
        const text = node('div', 'workspace-text');
        text.append(node('strong', '', workspace.name), node('span', '', workspace.path));
        const when = node('span', 'workspace-when', workspace.path === bridge.current() ? 'open now' : ago(workspace.opened));
        const forget = node('button', 'workspace-forget', '×');
        forget.type = 'button';
        forget.title = 'Remove from this list';
        forget.addEventListener('click', async (event) => {
          event.stopPropagation();
          await bridge.forget(workspace.path);
          await refresh();
        });
        item.append(text, when, forget);
        item.addEventListener('click', () => open(workspace));
        item.addEventListener('mousemove', () => { if (at !== index) { at = index; draw(); } });
        list.append(item);
      });
      empty.hidden = shown.length > 0;
      empty.textContent = recent.length ? 'No workspace matches.' : 'No workspaces opened yet.';
      list.querySelector('.active')?.scrollIntoView({block: 'nearest'});
    }

    function open(workspace) {
      if (workspace.path === bridge.current()) { hide(); return; }
      bridge.open(workspace.path);
    }

    async function refresh() {
      try { recent = await bridge.list(); } catch { recent = []; }
      draw();
    }

    async function show(canClose) {
      closable = canClose;
      close.hidden = !closable;
      mark.hidden = eyebrow.hidden = lead.hidden = closable;
      title.textContent = closable ? 'Workspaces' : 'A place to read software together.';
      host.hidden = false;
      filter.value = '';
      at = 0;
      await refresh();
      filter.focus();
    }

    function hide() {
      if (!closable) return;
      host.hidden = true;
      bridge.closed();
    }

    filter.addEventListener('input', () => { at = 0; draw(); });
    filter.addEventListener('keydown', (event) => {
      if (event.key === 'ArrowDown') at = Math.min(at + 1, shown.length - 1);
      else if (event.key === 'ArrowUp') at = Math.max(at - 1, 0);
      else if (event.key === 'Enter') { if (shown[at]) open(shown[at]); return; }
      else return;
      event.preventDefault();
      draw();
    });
    host.addEventListener('keydown', (event) => { if (event.key === 'Escape') { event.preventDefault(); hide(); } });
    folder.addEventListener('click', () => bridge.openFolder());
    close.addEventListener('click', hide);

    return {show, hide, refresh, isShown: () => !host.hidden};
  }

  root.AtlasWorkspaces = {mount, ago, matching};
  if (typeof module !== 'undefined' && module.exports) module.exports = root.AtlasWorkspaces;
})(typeof window !== 'undefined' ? window : globalThis);
