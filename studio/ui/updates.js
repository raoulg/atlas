(function (root) {
  'use strict';

  // A newer build of Studio's channel: a button in the status bar says so, and a page shows what
  // changed (a release's page of notes, drawn in the window; for a nightly, the commits between the
  // two, in the browser) with the button that installs it. The page opens by itself once per newer
  // build, and again whenever the button is pressed.

  const node = (tag, className, label) => {
    const element = document.createElement(tag);
    if (className) element.className = className;
    if (label !== undefined) element.textContent = label;
    return element;
  };
  const button = (className, label, action) => {
    const element = node('button', className, label);
    element.type = 'button';
    element.addEventListener('click', action);
    return element;
  };

  function mount(host, statusButton, bridge) {
    const head = node('header', 'updates-head');
    const text = node('div', 'updates-text');
    const title = node('h1');
    const said = node('p');
    text.append(title, said);
    const install = button('primary-button', 'Update and restart', () => run());
    const outside = button('secondary-button', 'What changed ↗', () => newer && bridge.openOutside(newer.notes));
    const later = button('secondary-button', 'Later', () => hide());
    const actions = node('div', 'updates-actions');
    actions.append(install, outside, later);
    head.append(text, actions);
    const notes = node('iframe', 'updates-notes');
    notes.title = 'Release notes';
    notes.referrerPolicy = 'no-referrer';
    host.append(head, notes);

    let newer = null;

    function show() {
      if (!newer) return;
      host.hidden = false;
      // without a page of notes for the window, a banner over the picture says it
      host.classList.toggle('compact', !newer.page);
      title.textContent = newer.nightly ? 'A newer nightly of Atlas Studio is out' : `Atlas Studio ${newer.version} is out`;
      said.textContent = newer.nightly
        ? `Nightly ${newer.version}, built from next. What changed is in the commits since yours.`
        : (newer.installs ? 'Studio installs it where it is now, and starts again on it.' : 'Download it from the release.') + (newer.page ? '' : ' What changed is on its release page.');
      install.hidden = !newer.installs;
      outside.hidden = newer.page;
      notes.hidden = !newer.page;
      if (newer.page && notes.dataset.src !== newer.notes) { notes.dataset.src = newer.notes; notes.src = newer.notes; }
    }

    function hide() { host.hidden = true; bridge.closed(); }

    async function run() {
      install.disabled = true;
      install.textContent = 'Updating…';
      try { await bridge.install(); }
      catch (error) { said.textContent = String(error); install.disabled = false; install.textContent = 'Try again'; }
    }

    // a newer build seen, by the hourly look or by asking; the page opens by itself once for it
    function found(next) {
      newer = next;
      statusButton.hidden = !next;
      if (!next) return;
      statusButton.textContent = next.nightly ? `↑ nightly ${next.version}` : `↑ Studio ${next.version}`;
      statusButton.title = 'A newer build is out: what changed, and update';
      let seen = '';
      try { seen = localStorage.getItem('atlas-studio-update-shown') || ''; } catch {}
      if (seen !== next.version) {
        try { localStorage.setItem('atlas-studio-update-shown', next.version); } catch {}
        show();
      }
    }

    statusButton.addEventListener('click', show);
    host.addEventListener('keydown', (event) => { if (event.key === 'Escape') hide(); });
    return {found, show, isShown: () => !host.hidden};
  }

  root.AtlasUpdates = {mount};
})(typeof window !== 'undefined' ? window : globalThis);
