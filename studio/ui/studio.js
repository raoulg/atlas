const { invoke, Channel } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const byId = (id) => document.getElementById(id);
let workspace = null;
let activeView = 'atlas';
let currentLesson = null;

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
    byId('atlas-frame').src = `${workspace.url}/#v=${encodeURIComponent(JSON.stringify(view))}${at ? `&at=${encodeURIComponent(at)}` : ''}`;
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
  byId('atlas-frame').src = `${workspace.url}/?at=${Date.now()}#${view ? `v=${encodeURIComponent(JSON.stringify(view))}&` : ''}at=${encodeURIComponent(id)}`;
  selectView('atlas');
}

const files = window.AtlasFiles.mount(byId('file-tree'), byId('preview'), {
  readFile: async (path) => new Uint8Array(await invoke('read_workspace_file', {path})),
  previewOpen: (open) => { byId('preview').hidden = !open; },
  pointAt,
  workspaceUrl: () => workspace?.url || '',
  openFullView: (view, at) => {
    if (!workspace) return;
    byId('atlas-frame').src = `${workspace.url}/#v=${encodeURIComponent(JSON.stringify(view))}${at ? `&at=${encodeURIComponent(at)}` : ''}`;
    selectView('atlas');
  },
});

const terminal = window.AtlasTerminal.mount(byId('terminal-panel'), {
  Channel,
  open: (cols, rows, onEvent) => invoke('terminal_open', {cols, rows, onEvent}),
  write: (id, data) => invoke('terminal_write', {id, data}),
  resize: (id, cols, rows) => invoke('terminal_resize', {id, cols, rows}),
  close: (id) => invoke('terminal_close', {id}),
  hidePanel: () => showTerminal(false),
});

function showTerminal(show = byId('terminal-panel').hidden) {
  if (show && !workspace) return;
  byId('terminal-panel').hidden = !show;
  byId('terminal-toggle').classList.toggle('on', show);
  if (show) terminal.shown();
}

// the terminal panel's height follows its top edge, dragged
byId('terminal-grip').addEventListener('pointerdown', (event) => {
  const panel = byId('terminal-panel');
  const start = event.clientY;
  const height = panel.offsetHeight;
  const most = byId('views').closest('.content').offsetHeight - 120;
  document.body.classList.add('dragging');
  const move = (moved) => panel.style.setProperty('--terminal-height', `${Math.max(120, Math.min(most, height + start - moved.clientY))}px`);
  const stop = () => {
    document.body.classList.remove('dragging');
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
  for (const section of ['welcome', 'loading', 'error', 'views']) byId(section).hidden = section !== name;
  byId('error').textContent = error;
  byId('status-text').textContent = name === 'views' ? 'Atlas · workspace open' : name === 'loading' ? 'Opening workspace' : name === 'error' ? 'Atlas stopped' : 'Ready';
}

function selectView(view) {
  activeView = view;
  for (const tab of document.querySelectorAll('.side-tab')) {
    const selected = tab.dataset.view === view;
    tab.classList.toggle('active', selected);
    if (selected) tab.setAttribute('aria-current', 'page');
    else tab.removeAttribute('aria-current');
  }
  for (const item of document.querySelectorAll('.side-lesson')) item.classList.toggle('active', view === 'lesson' && item.dataset.path === currentLesson);
  byId('atlas-frame').hidden = view !== 'atlas';
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
  byId('project-path').textContent = next.path;
  byId('side-path').textContent = next.path;
  byId('atlas-frame').src = `${next.url}/`;
  byId('history-frame').src = `${next.url}/history`;
  currentLesson = null;
  byId('insert-atlas-view').hidden = true;
  selectView('atlas');
  showState('views');
  loadLessons();
  loadFiles();
}

async function openWorkspace() {
  byId('open-workspace').disabled = true;
  const hadWorkspace = Boolean(workspace);
  if (!hadWorkspace) showState('loading');
  try {
    const chosen = await invoke('choose_workspace');
    if (chosen) displayWorkspace(chosen);
    else if (!hadWorkspace) showState('welcome');
  } catch (error) {
    if (hadWorkspace) byId('status-text').textContent = String(error);
    else showState('error', String(error));
  } finally {
    byId('open-workspace').disabled = false;
  }
}

byId('open-workspace').addEventListener('click', openWorkspace);
byId('insert-atlas-view').addEventListener('click', async () => {
  if (!currentLesson) return;
  await new Promise((resolve) => setTimeout(resolve, 350));
  try { await lessonPanel.insertCurrentView(); selectView('lesson'); }
  catch (error) { byId('status-text').textContent = String(error); }
});
byId('welcome-open').addEventListener('click', openWorkspace);
byId('terminal-toggle').addEventListener('click', () => showTerminal());
window.addEventListener('keydown', (event) => {
  if (event.ctrlKey && event.key === '`') { event.preventDefault(); showTerminal(); }
});
for (const tab of document.querySelectorAll('.side-tab')) tab.addEventListener('click', () => selectView(tab.dataset.view));
listen('studio-backend-stopped', () => { displayWorkspace(null); showState('error', 'Atlas stopped. Open the workspace again to restart it.'); });
showState('loading');
invoke('first_workspace').then(displayWorkspace).catch((error) => showState('error', String(error)));
