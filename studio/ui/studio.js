const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const byId = (id) => document.getElementById(id);
let workspace = null;
let activeView = 'atlas';

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
  byId('atlas-frame').hidden = view !== 'atlas';
  byId('history-frame').hidden = view !== 'history';
}

function displayWorkspace(next) {
  workspace = next;
  if (!next) { showState('welcome'); return; }
  byId('project-name').textContent = next.name;
  byId('project-path').textContent = next.path;
  byId('side-path').textContent = next.path;
  byId('atlas-frame').src = `${next.url}/`;
  byId('history-frame').src = `${next.url}/history`;
  selectView(activeView);
  showState('views');
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
byId('welcome-open').addEventListener('click', openWorkspace);
for (const tab of document.querySelectorAll('.side-tab')) tab.addEventListener('click', () => selectView(tab.dataset.view));
listen('studio-backend-stopped', () => { workspace = null; showState('error', 'Atlas stopped. Open the workspace again to restart it.'); });
invoke('current_workspace').then(displayWorkspace).catch((error) => showState('error', String(error)));
