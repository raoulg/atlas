const { invoke } = window.__TAURI__.core;
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
  workspace = next;
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
for (const tab of document.querySelectorAll('.side-tab')) tab.addEventListener('click', () => selectView(tab.dataset.view));
listen('studio-backend-stopped', () => { workspace = null; showState('error', 'Atlas stopped. Open the workspace again to restart it.'); });
invoke('current_workspace').then(displayWorkspace).catch((error) => showState('error', String(error)));
