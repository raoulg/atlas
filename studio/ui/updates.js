(function (root) {
  'use strict';

  // A newer build of Studio's channel: a button in the status bar says so, and a page shows what
  // changed (a release's page of notes, drawn in the window; for a nightly, the commits between the
  // two, in the browser) with the button that installs it. The page opens by itself once per newer
  // build, and again whenever the button is pressed. The same page shows what changed in the build that
  // runs, when asked (What's New).

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
    const outside = button('secondary-button', 'What changed ↗', () => shown && bridge.openOutside(shown.notes));
    // the build the page shows: a newer one, or the one that runs
    let shown = null;
    const later = button('secondary-button', 'Later', () => hide());
    const close = button('secondary-button', 'Close', () => hide());
    const actions = node('div', 'updates-actions');
    actions.append(install, outside, later, close);
    head.append(text, actions);
    const notes = node('iframe', 'updates-notes');
    notes.title = 'Release notes';
    notes.referrerPolicy = 'no-referrer';
    host.append(head, notes);

    let newer = null;

    function show(build = newer) {
      if (!build) return;
      shown = build;
      host.hidden = false;
      // without a page of notes for the window, a banner over the picture says it
      host.classList.toggle('compact', !build.page);
      if (build.current) {
        title.textContent = `What's new in Atlas Studio ${build.version}`;
        said.textContent = build.page ? '' : build.notes.includes('/commits/')
          ? 'This build is no release yet: what changed is in the commits it is made of.'
          : 'What changed is on its release page.';
      } else {
        title.textContent = build.nightly ? 'A newer nightly of Atlas Studio is out' : `Atlas Studio ${build.version} is out`;
        said.textContent = build.nightly
          ? `Nightly ${build.version}, built from next. What changed is in the commits since yours.`
          : (build.installs ? 'Studio installs it where it is now, and starts again on it.' : 'Download it from the release.') + (build.page ? '' : ' What changed is on its release page.');
      }
      install.hidden = !build.installs;
      later.hidden = build.current;
      close.hidden = !build.current;
      outside.hidden = build.page;
      notes.hidden = !build.page;
      if (build.page && notes.dataset.src !== build.notes) { notes.dataset.src = build.notes; notes.src = build.notes; }
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

    statusButton.addEventListener('click', () => show());
    host.addEventListener('keydown', (event) => { if (event.key === 'Escape') hide(); });
    return {found, show, isShown: () => !host.hidden};
  }

  root.AtlasUpdates = {mount};
})(typeof window !== 'undefined' ? window : globalThis);
