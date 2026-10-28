const { invoke, Channel } = window.__TAURI__.core;
// events for this window: each window is one workspace, and a menu item acts on the window in front
const thisWindow = window.__TAURI__.webviewWindow.getCurrentWebviewWindow();
const listen = (event, handler) => thisWindow.listen(event, handler);
const byId = (id) => document.getElementById(id);
let workspace = null;
let activeView = 'atlas';
let currentLesson = null;

// the served page, told that Studio hosts it (its controls then stand in Studio's bar); a new `t`
// loads it anew where only the fragment changes
const atlasPage = (hash = '') => `${workspace.url}/?host=${encodeURIComponent(location.origin)}&palette=${palette()}&t=${Date.now()}${hash ? '#' + hash : ''}`;

// how Studio looks: the mode, and a theme for light and one for dark (Settings › Appearance). The theme
// of the scheme in force colours Studio, its terminal, and the pages it frames, which take its roles in
// their address and as a message when it changes.
const Themes = window.AtlasThemes;
let appearance = {theme: 'system', light: 'atlas-light', dark: 'atlas-dark'};
let shownTheme = Themes.byId('atlas-light', 'light');
const palette = () => encodeURIComponent(JSON.stringify(Themes.paletteOf(shownTheme)));
function showAppearance(next) {
  appearance = next;
  shownTheme = Themes.show(appearance);
  terminal?.setTheme(Themes.terminalOf(shownTheme));
  if (workspace) {
    for (const frame of [byId('atlas-frame'), byId('history-frame')]) {
      frame.contentWindow?.postMessage({atlas: 'palette', palette: Themes.paletteOf(shownTheme)}, new URL(workspace.url).origin);
    }
  }
  const other = shownTheme.scheme === 'dark' ? 'light' : 'dark';
  byId('scheme-toggle').textContent = `◐ ${shownTheme.name}`;
  byId('scheme-toggle').title = `Switch to ${Themes.byId(appearance[other], other).name} (⇧⌘L)`;
}
async function setAppearance(next) {
  showAppearance(next);
  try { await invoke('set_appearance', {appearance: next}); }
  catch (error) { byId('status-text').textContent = String(error); }
}
// the other scheme, in its theme: from the system's, the one it does not show
const toggleScheme = () => setAppearance({...appearance, theme: Themes.schemeOf(appearance) === 'dark' ? 'light' : 'dark'});
const viewHash = (view, at) => [view ? `v=${encodeURIComponent(JSON.stringify(view))}` : '', at ? `at=${encodeURIComponent(at)}` : ''].filter(Boolean).join('&');

const lessonPanel = window.AtlasLesson.mount(byId('lesson-panel'), {
  workspaceUrl: () => workspace?.url || '',
  openLesson: (path) => invoke('open_lesson', { path }),
  readReference: (path, reference) => invoke('read_lesson_reference', { path, reference }),
  readArtifact: async (path, reference) => {
    const bytes = await invoke('read_lesson_asset', { path, reference });
    const extension = reference.split('.').at(-1)?.toLowerCase();
    const type = {svg: 'image/svg+xml', png: 'image/png', jpg: 'image/jpeg', jpeg: 'image/jpeg', webp: 'image/webp', gif: 'image/gif'}[extension] || 'application/octet-stream';
    return URL.createObjectURL(new Blob([new Uint8Array(bytes)], {type}));
  },
  saveLesson: (path, text) => invoke('save_lesson', {path, text}),
  currentView: () => invoke('current_atlas_view'),
  saveView: (path, name, view) => invoke('save_lesson_view', {path, name, view}),
  runScript: (path) => invoke('run_lesson', {path}),
  openFullView: (view, at) => {
    if (!workspace) return;
    byId('atlas-frame').src = atlasPage(viewHash(view, at));
    selectView('atlas');
  },
  openLink: (from, target) => {
    if (!target?.match(/\.(md|html)(?:#.*)?$/i)) return;
    const base = from.split('/').slice(0, -1).join('/');
    const path = [base, target.split('#')[0]].filter(Boolean).join('/');
    openLesson(path);
  },
});

// the Atlas view, opened where it is and pointing at a box: a box not drawn is opened the way to
async function pointAt(id) {
  if (!workspace) return;
  let view = null;
  try { view = JSON.parse(await invoke('current_atlas_view')).view || null; } catch {}
  byId('atlas-frame').src = atlasPage(viewHash(view, id));
  selectView('atlas');
}

const files = window.AtlasFiles.mount(byId('file-tree'), byId('preview'), {
  readFile: async (path) => new Uint8Array(await invoke('read_workspace_file', {path})),
  previewOpen: (open) => { byId('preview').hidden = !open; },
  pointAt,
  workspaceUrl: () => workspace?.url || '',
  openFullView: (view, at) => {
    if (!workspace) return;
    byId('atlas-frame').src = atlasPage(viewHash(view, at));
    selectView('atlas');
  },
});

// the Atlas view's controls, in Studio's top bar: the page describes them, the bar answers
const atlasFrame = byId('atlas-frame');
const tellPage = (key, value) => atlasFrame.contentWindow?.postMessage({atlas: 'do', key, value}, new URL(workspace.url).origin);
const bar = window.AtlasBar.mount(byId('atlas-bar'), tellPage);
window.addEventListener('message', async (event) => {
  if (!workspace || event.source !== atlasFrame.contentWindow || event.origin !== new URL(workspace.url).origin) return;
  if (event.data?.atlas === 'bar') bar.draw(event.data.controls);
  else if (event.data?.atlas === 'export') {
    try {
      const saved = await invoke('export_view', {view: JSON.stringify(event.data.view)});
      if (saved) byId('status-text').textContent = `Exported ${saved}`;
    } catch (error) { byId('status-text').textContent = String(error); }
  }
});
atlasFrame.addEventListener('load', () => bar.clear());

const workspaces = window.AtlasWorkspaces.mount(byId('workspaces'), {
  list: () => invoke('recent_workspaces'),
  current: () => workspace?.path,
  open: (path) => openAt(path),
  openFolder: () => openFolder(),
  forget: (path) => invoke('forget_workspace', {path}),
  elsewhere: () => invoke('open_elsewhere'),
  closed: () => selectView(activeView),
});

const terminal = window.AtlasTerminal.mount(byId('terminal-panel'), {
  Channel,
  open: (cols, rows, onEvent) => invoke('terminal_open', {cols, rows, onEvent}),
  write: (id, data) => invoke('terminal_write', {id, data}),
  resize: (id, cols, rows) => invoke('terminal_resize', {id, cols, rows}),
  close: (id) => invoke('terminal_close', {id}),
  hidePanel: () => showTerminal(false),
  dock: (dock) => setDock(dock),
});

// where the terminal docks (the buttons in its bar), kept in Studio's settings for every window
let dock = 'bottom';
function showDock(next) {
  dock = window.AtlasTerminal.dockOf(next);
  byId('terminal-panel').closest('.content').dataset.dock = dock;
  terminal.setDock(dock);
  terminal.fit();
}
async function setDock(next) {
  showDock(next);
  try { await invoke('set_terminal_dock', {dock}); }
  catch (error) { byId('status-text').textContent = String(error); }
}

function showTerminal(show = byId('terminal-panel').hidden) {
  if (show && !workspace) return;
  byId('terminal-panel').hidden = !show;
  byId('terminal-toggle').classList.toggle('on', show);
  if (show) terminal.shown();
}

// the terminal panel's size follows its inner edge, dragged: the top edge at the bottom, the edge that
// faces the view at a side; the view keeps as much as the panel's least size
byId('terminal-grip').addEventListener('pointerdown', (event) => {
  // a drag moves the edge and selects no text on its way
  event.preventDefault();
  const panel = byId('terminal-panel');
  const content = panel.closest('.content');
  const {size, axis, least} = window.AtlasTerminal.DOCKS[dock];
  const from = {x: event.clientX, y: event.clientY};
  const start = axis === 'y' ? panel.offsetHeight : panel.offsetWidth;
  const most = (axis === 'y' ? content.offsetHeight : content.offsetWidth) - least;
  document.body.classList.add('dragging', `dragging-${axis}`);
  const move = (moved) => panel.style.setProperty(size, `${window.AtlasTerminal.dragged(dock, start, from, {x: moved.clientX, y: moved.clientY}, most)}px`);
  const stop = () => {
    document.body.classList.remove('dragging', 'dragging-x', 'dragging-y');
    window.removeEventListener('pointermove', move);
    terminal.fit();
  };
  window.addEventListener('pointermove', move);
  window.addEventListener('pointerup', stop, {once: true});
});

// the files Atlas reads, followed while the workspace is open; the tree is drawn anew only when
// the listing changed
let listed = '';
async function loadFiles() {
  if (!workspace) return;
  try {
    const text = await invoke('workspace_files');
    if (text === listed) return;
    listed = text;
    files.load(JSON.parse(text));
  } catch (error) { byId('status-text').textContent = String(error); }
}
// the box the Atlas view has selected, marked in the tree
async function followView() {
  if (!workspace) return;
  try { files.shown(JSON.parse(await invoke('current_atlas_view'))); } catch {}
}
setInterval(loadFiles, 3000);
setInterval(followView, 1000);
window.addEventListener('focus', loadFiles);

function showState(name, error = '') {
  for (const section of ['loading', 'error', 'views']) byId(section).hidden = section !== name;
  if (name === 'welcome') workspaces.show(false);
  else if (name !== 'views' || workspace) byId('workspaces').hidden = true;
  byId('error').textContent = error;
  byId('status-text').textContent = name === 'views' ? 'Atlas · workspace open' : name === 'loading' ? 'Opening workspace' : name === 'error' ? 'Atlas stopped' : 'Ready';
}

function selectView(view) {
  activeView = view;
  // going to a view leaves the workspaces page and the update's
  if (workspace) { byId('workspaces').hidden = true; byId('updates').hidden = true; byId('settings').hidden = true; }
  for (const tab of document.querySelectorAll('.side-tab')) {
    const selected = tab.dataset.view === view;
    tab.classList.toggle('active', selected);
    if (selected) tab.setAttribute('aria-current', 'page');
    else tab.removeAttribute('aria-current');
  }
  for (const item of document.querySelectorAll('.side-lesson')) item.classList.toggle('active', view === 'lesson' && item.dataset.path === currentLesson);
  byId('atlas-frame').hidden = view !== 'atlas';
  byId('atlas-bar').hidden = view !== 'atlas' || !workspace;
  byId('history-frame').hidden = view !== 'history';
  byId('lesson-panel').hidden = view !== 'lesson';
}

async function openLesson(path) {
  try {
    await lessonPanel.open(path);
    currentLesson = path;
    byId('insert-atlas-view').hidden = false;
    selectView('lesson');
  } catch (error) { byId('status-text').textContent = String(error); }
}

async function loadLessons() {
  const list = byId('lesson-list');
  list.replaceChildren();
  try {
    const paths = await invoke('list_lessons');
    if (!paths.length) { const empty = document.createElement('p'); empty.className = 'lesson-empty'; empty.textContent = 'No lessons in this workspace'; list.append(empty); return; }
    for (const path of paths) {
      const button = document.createElement('button');
      button.type = 'button';
      button.className = 'side-lesson';
      button.dataset.path = path;
      button.textContent = path.split(/[\\/]/).slice(-2).join(" / ");
      button.title = path;
      button.addEventListener('click', () => openLesson(path));
      list.append(button);
    }
  } catch (error) { byId('status-text').textContent = String(error); }
}

function displayWorkspace(next) {
  if (workspace?.path !== next?.path) {
    terminal.closeAll();
    showTerminal(false);
    files.clear();
    listed = '';
  }
  workspace = next;
  byId('terminal-toggle').disabled = !next;
  if (!next) { showState('welcome'); return; }
  byId('project-name').textContent = next.name;
  byId('workspace-button').title = `${next.path}\nWorkspaces (⌘O)`;
  byId('side-path').textContent = next.path;
  bar.clear();
  byId('atlas-frame').src = atlasPage();
  byId('history-frame').src = `${next.url}/history?palette=${palette()}`;
  currentLesson = null;
  byId('insert-atlas-view').hidden = true;
  selectView('atlas');
  showState('views');
  loadLessons();
  loadFiles();
}

// a workspace opened: the one asked for by its path, or a folder chosen in the system's dialog
async function opening(open) {
  const hadWorkspace = Boolean(workspace);
  if (!hadWorkspace) showState('loading');
  else byId('status-text').textContent = 'Opening workspace…';
  try {
    // nothing opened: the dialog was cancelled, or the workspace is open in another window, which came
    // to the front
    const opened = await open();
    if (opened) displayWorkspace(opened);
    else if (!hadWorkspace) showState('welcome');
    else byId('status-text').textContent = 'Atlas · workspace open';
  } catch (error) {
    if (hadWorkspace) byId('status-text').textContent = String(error);
    else showState('error', String(error));
  }
}
// a workspace open in another window brings that window to the front, and this one goes back to its own
const openAt = (path) => opening(() => invoke('open_workspace_at', {path})).then(() => { if (workspace && workspaces.isShown()) selectView(activeView); });
const openFolder = () => opening(() => invoke('choose_workspace'));
// the workspaces page covers the view, and the view's controls step aside
const showWorkspaces = () => { byId('atlas-bar').hidden = true; workspaces.show(Boolean(workspace)); };
byId('open-workspace').addEventListener('click', showWorkspaces);

byId('insert-atlas-view').addEventListener('click', async () => {
  if (!currentLesson) return;
  await new Promise((resolve) => setTimeout(resolve, 350));
  try { await lessonPanel.insertCurrentView(); selectView('lesson'); }
  catch (error) { byId('status-text').textContent = String(error); }
});
byId('workspace-button').addEventListener('click', showWorkspaces);
byId('terminal-toggle').addEventListener('click', () => showTerminal());
for (const tab of document.querySelectorAll('.side-tab')) tab.addEventListener('click', () => selectView(tab.dataset.view));
// which build this is, at the right of the status bar: its version, and for a nightly its commit;
// the settings name the commit of every build. A click on it shows what changed in it (What's New).
let buildSaid = 'Atlas Studio';
invoke('studio_build').then((build) => {
  buildSaid = `Atlas Studio ${build.version}` + (build.nightly ? ' · nightly' : '') + (build.commit ? ` · ${build.commit}` : '');
  byId('studio-version').textContent = `Atlas Studio ${build.version}` + (build.nightly ? ` · nightly ${build.commit}` : '');
  byId('studio-version').title = `What's new in atlas ${build.version}${build.nightly ? '-nightly' : ''}${build.commit ? ` (${build.commit})` : ''}`;
}).catch(() => {});

// a newer build of Studio's channel, looked for once an hour against an answer a day old at most
const updates = window.AtlasUpdates.mount(byId('updates'), byId('update-button'), {
  install: () => invoke('install_update'),
  openOutside: (url) => invoke('plugin:opener|open_url', {url}),
  closed: () => { if (workspace) selectView(activeView); },
});
listen('studio-update', ({payload}) => updates.found(payload));
// what changed in the build that runs: its release's notes in the window, else on GitHub
async function whatsNew() {
  if (settingsPage.isShown()) settingsPage.hide();
  byId('status-text').textContent = 'Looking up what changed…';
  try { updates.show(await invoke('whats_new')); byId('status-text').textContent = 'Ready'; }
  catch (error) { byId('status-text').textContent = String(error); }
}
async function checkNow() {
  byId('status-text').textContent = 'Looking for a newer Studio…';
  try {
    const newer = await invoke('check_update', {now: true});
    updates.found(newer);
    if (newer) updates.show();
    byId('status-text').textContent = newer ? 'A newer Studio is out' : 'Atlas Studio is up to date';
  } catch (error) { byId('status-text').textContent = String(error); }
}

// Studio's settings (⌘,): the theme, the coding agents with Atlas, the version
const settingsPage = window.AtlasSettings.mount(byId('settings'), {
  get: () => invoke('get_settings'),
  appearance: () => appearance,
  themes: Themes.THEMES,
  swatchOf: Themes.swatchOf,
  setAppearance,
  register: (id) => invoke('register_agent', {id}),
  build: () => buildSaid,
  checkUpdates: () => checkNow(),
  whatsNew: () => whatsNew(),
  say: (text) => { byId('status-text').textContent = text; },
  closed: () => { if (workspace) selectView(activeView); else showState('welcome'); },
});

// the menu bar's items, said by name
listen('studio-menu', ({payload}) => {
  const action = payload?.action;
  if (action === 'workspace.page') showWorkspaces();
  else if (action === 'workspace.folder') openFolder();
  else if (action === 'open') openAt(payload.path);
  else if (action === 'recents') { if (workspaces.isShown()) workspaces.refresh(); }
  else if (action === 'view.atlas' && workspace) selectView('atlas');
  else if (action === 'view.history' && workspace) selectView('history');
  else if (action === 'view.terminal') showTerminal();
  else if (action === 'view.scheme') toggleScheme();
  else if (action === 'update.check') checkNow();
  else if (action === 'update.notes') whatsNew();
  else if (action === 'settings') { byId('atlas-bar').hidden = true; settingsPage.show(); }
});
listen('appearance', ({payload}) => showAppearance(payload));
listen('terminal-dock', ({payload}) => showDock(payload));
Themes.darkSystem.addEventListener('change', () => { if (appearance.theme === 'system') showAppearance(appearance); });
byId('scheme-toggle').addEventListener('click', toggleScheme);
byId('studio-version').addEventListener('click', () => whatsNew());
invoke('get_settings').then((settings) => { showAppearance(settings.appearance); showDock(settings.terminal); })
  .catch(() => { showAppearance(appearance); showDock(dock); });
listen('studio-backend-stopped', () => { displayWorkspace(null); showState('error', 'Atlas stopped. Open the workspace again to restart it.'); });
showState('loading');
// the first look for a newer Studio waits for the first workspace, which opening would close it over
invoke('first_workspace').then(displayWorkspace).catch((error) => showState('error', String(error)))
  .finally(() => invoke('check_update', {now: false}).then((newer) => updates.found(newer)).catch(() => {}));
