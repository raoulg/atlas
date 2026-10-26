(function (root) {
  'use strict';

  // The workspace's files as Atlas reads them, a tree beside the picture: a file the graph defines
  // something in is traced (a filled dot), any other file is only physical (an open dot). Selecting
  // a file previews it; a fact in the preview points the Atlas view at its box.

  const node = (tag, className, label) => {
    const element = document.createElement(tag);
    if (className) element.className = className;
    if (label !== undefined) element.textContent = label;
    return element;
  };
  const control = (label, handler, title) => {
    const element = node('button', '', label);
    element.type = 'button';
    if (title) element.title = title;
    element.addEventListener('click', handler);
    return element;
  };
  const extension = (path) => (path.match(/\.([^./]+)$/)?.[1] || '').toLowerCase();
  const IMAGES = {png: 'image/png', jpg: 'image/jpeg', jpeg: 'image/jpeg', gif: 'image/gif', webp: 'image/webp', svg: 'image/svg+xml'};
  const KIND = {module: 'module', class: 'class', function: 'function', method: 'method'};

  // the directories and files of a listing, as nested folders
  function tree(files) {
    const top = {name: '', path: '', dirs: new Map(), files: []};
    for (const file of files) {
      const parts = file.path.split('/');
      let at = top;
      for (const part of parts.slice(0, -1)) {
        const path = at.path ? at.path + '/' + part : part;
        if (!at.dirs.has(part)) at.dirs.set(part, {name: part, path, dirs: new Map(), files: []});
        at = at.dirs.get(part);
      }
      at.files.push({name: parts.at(-1), path: file.path, facts: file.facts});
    }
    return top;
  }

  // a box id of the page, as the id of the fact it shows: a type box (`t:…`) and a step of a route
  // (`n:…/0`) name the fact they come from before any `~`, `@` or `/`
  const factOf = (box) => box.replace(/^[tn]:/, '').split(/[~@/]/)[0];

  // a path relative to the file `from`, from the workspace's root
  function relative(from, target) {
    const parts = from.split('/').slice(0, -1);
    for (const part of target.split('#')[0].split('/')) {
      if (part === '..') parts.pop();
      else if (part && part !== '.') parts.push(part);
    }
    return parts.join('/');
  }

  function mount(treeHost, previewHost, bridge) {
    const list = node('div', 'file-tree');
    list.setAttribute('role', 'tree');
    list.setAttribute('aria-label', 'Files');
    const empty = node('p', 'file-empty', 'Open a workspace to see its files.');
    treeHost.append(list, empty);

    const header = node('header', 'preview-head');
    const heading = node('div', 'preview-title');
    const name = node('strong', '', 'Preview');
    const where = node('span', '', '');
    heading.append(name, where);
    const modes = node('div', 'preview-modes');
    const close = control('✕', () => closePreview(), 'Close the preview');
    close.className = 'preview-close';
    header.append(heading, close);
    const tabs = node('div', 'preview-tabs');
    tabs.setAttribute('role', 'tablist');
    const body = node('div', 'preview-body');
    const footer = node('footer', 'preview-facts');
    previewHost.append(header, tabs, modes, body, footer);

    let listing = {files: []};
    let byPath = new Map();
    let factFile = new Map();
    let open = new Set();
    let selected = null;
    let inView = null;
    let shownKey = '';
    const previews = [];
    let active = null;
    const urls = [];

    function rows(folder, depth, out) {
      for (const dir of [...folder.dirs.values()].sort((a, b) => a.name.localeCompare(b.name))) {
        out.push({dir: true, path: dir.path, name: dir.name, depth});
        if (open.has(dir.path)) rows(dir, depth + 1, out);
      }
      for (const file of [...folder.files].sort((a, b) => a.name.localeCompare(b.name))) out.push({dir: false, ...file, depth});
      return out;
    }

    function draw() {
      const focused = document.activeElement?.closest?.('.file-row')?.dataset.path;
      const shown = rows(tree(listing.files), 0, []);
      list.replaceChildren();
      for (const row of shown) {
        const item = node('div', 'file-row' + (row.dir ? ' dir' : ''));
        item.setAttribute('role', 'treeitem');
        item.dataset.path = row.path;
        item.style.setProperty('--depth', row.depth);
        item.tabIndex = row.path === (focused || selected || shown[0]?.path) ? 0 : -1;
        if (row.dir) item.setAttribute('aria-expanded', String(open.has(row.path)));
        else item.setAttribute('aria-selected', String(row.path === selected));
        item.classList.toggle('selected', row.path === selected);
        item.classList.toggle('in-view', row.path === inView);
        const caret = node('span', 'caret', row.dir ? (open.has(row.path) ? '▾' : '▸') : '');
        const badge = node('span', 'badge', row.dir ? '' : (extension(row.name)[0] || '·').toUpperCase());
        const label = node('span', 'name', row.name);
        item.append(caret, badge, label);
        if (row.path === inView) {
          const here = node('span', 'here', '◇');
          here.title = 'Selected in the Atlas view';
          item.append(here);
        }
        if (!row.dir) {
          const traced = row.facts.length > 0;
          const dot = node('span', 'dot' + (traced ? ' traced' : ''), traced ? '●' : '○');
          dot.title = traced ? `${row.facts.length} in the graph` : 'No graph fact: a physical file only';
          item.append(dot);
        }
        item.title = row.path;
        item.addEventListener('click', () => choose(row));
        list.append(item);
      }
      empty.hidden = shown.length > 0;
      if (focused) list.querySelector(`[data-path="${CSS.escape(focused)}"]`)?.focus();
    }

    function choose(row) {
      if (row.dir) {
        if (open.has(row.path)) open.delete(row.path); else open.add(row.path);
        draw();
        list.querySelector(`[data-path="${CSS.escape(row.path)}"]`)?.focus();
        return;
      }
      openFile(row.path);
    }

    // arrows move through the tree, right and left open and close a folder, Enter opens
    list.addEventListener('keydown', (event) => {
      const items = [...list.querySelectorAll('.file-row')];
      const at = items.indexOf(document.activeElement);
      if (at < 0) return;
      const item = items[at];
      const row = {dir: item.classList.contains('dir'), path: item.dataset.path};
      const go = (index) => {
        const next = items[Math.max(0, Math.min(items.length - 1, index))];
        items.forEach((each) => { each.tabIndex = -1; });
        next.tabIndex = 0;
        next.focus();
      };
      if (event.key === 'ArrowDown') go(at + 1);
      else if (event.key === 'ArrowUp') go(at - 1);
      else if (event.key === 'Home') go(0);
      else if (event.key === 'End') go(items.length - 1);
      else if (event.key === 'ArrowRight' && row.dir && !open.has(row.path)) choose(row);
      else if (event.key === 'ArrowLeft' && row.dir && open.has(row.path)) choose(row);
      else if (event.key === 'ArrowLeft') {
        const parent = row.path.split('/').slice(0, -1).join('/');
        const index = items.findIndex((each) => each.dataset.path === parent);
        if (index >= 0) go(index);
      } else if (event.key === 'Enter' || event.key === ' ') choose(row);
      else return;
      event.preventDefault();
    });

    function reveal(path) {
      const parts = path.split('/');
      for (let i = 1; i < parts.length; i++) open.add(parts.slice(0, i).join('/'));
    }

    // what the listing says now; the tree keeps its open folders and its selection
    function load(next) {
      listing = next;
      byPath = new Map(next.files.map((file) => [file.path, file]));
      factFile = new Map(next.files.flatMap((file) => file.facts.map((fact) => [fact.id, file.path])));
      if (!open.size) for (const file of next.files) if (file.facts.length) { reveal(file.path); break; }
      draw();
      footerFor(active);
    }

    function clear() {
      listing = {files: []};
      byPath = new Map();
      factFile = new Map();
      open = new Set();
      selected = null;
      inView = null;
      shownKey = '';
      previews.length = 0;
      closePreview();
      draw();
    }

    // the box the Atlas view has selected: its file is marked in the tree, opened up to it
    function shown(view) {
      const box = view?.selected || '';
      if (box === shownKey) return;
      shownKey = box;
      const path = factFile.get(box) || factFile.get(factOf(box)) || null;
      inView = path;
      if (path) reveal(path);
      draw();
      if (path) list.querySelector(`[data-path="${CSS.escape(path)}"]`)?.scrollIntoView({block: 'nearest'});
    }

    function closePreview() {
      active = null;
      selected = null;
      previews.length = 0;
      urls.splice(0).forEach((url) => URL.revokeObjectURL(url));
      bridge.previewOpen(false);
      draw();
    }

    function drawTabs() {
      tabs.replaceChildren();
      for (const path of previews) {
        const tab = control(path.split('/').at(-1), () => openFile(path), path);
        tab.setAttribute('role', 'tab');
        tab.setAttribute('aria-selected', String(path === active));
        tab.classList.toggle('active', path === active);
        const shut = node('span', 'tab-close', '×');
        shut.title = 'Close';
        shut.addEventListener('click', (event) => {
          event.stopPropagation();
          previews.splice(previews.indexOf(path), 1);
          if (path === active) previews.length ? openFile(previews.at(-1)) : closePreview();
          else drawTabs();
        });
        tab.append(shut);
        tabs.append(tab);
      }
    }

    function footerFor(path) {
      footer.replaceChildren();
      const facts = byPath.get(path)?.facts || [];
      if (!path) return;
      if (!facts.length) { footer.append(node('span', 'muted', 'Local file · no traced graph fact')); return; }
      footer.append(node('span', 'muted', 'In the graph'));
      for (const fact of facts.filter((fact) => fact.kind !== 'module')) {
        const chip = control(fact.name, () => bridge.pointAt(fact.id), `${KIND[fact.kind] || fact.kind} · line ${fact.line ?? '?'} · show in the Atlas view`);
        chip.className = 'fact ' + fact.kind;
        footer.append(chip);
      }
      if (facts.every((fact) => fact.kind === 'module')) {
        const module = facts[0];
        const chip = control(module.name, () => bridge.pointAt(module.id), 'module · show in the Atlas view');
        chip.className = 'fact module';
        footer.append(chip);
      }
    }

    function blobUrl(bytes, type) {
      const url = URL.createObjectURL(new Blob([bytes], {type}));
      urls.push(url);
      return url;
    }

    function sourceView(text, facts) {
      const lines = text.replaceAll('\r\n', '\n').split('\n');
      if (lines.at(-1) === '') lines.pop();
      const starts = new Map();
      for (const fact of facts) if (fact.line && fact.kind !== 'module') starts.set(fact.line, fact);
      const pre = node('pre', 'source');
      lines.forEach((line, index) => {
        const row = node('div', 'line');
        const number = node('span', 'number', String(index + 1));
        const fact = starts.get(index + 1);
        if (fact) {
          row.classList.add('fact-line');
          number.title = `${fact.name}: show in the Atlas view`;
          number.addEventListener('click', () => bridge.pointAt(fact.id));
        }
        row.append(number, node('span', 'code', line || ' '));
        pre.append(row);
      });
      return pre;
    }

    function modeButtons(current, render) {
      modes.replaceChildren();
      for (const mode of ['Preview', 'Source']) {
        const button = control(mode, () => render(mode));
        button.classList.toggle('active', mode === current);
        modes.append(button);
      }
    }

    async function openFile(path) {
      selected = path;
      active = path;
      if (!previews.includes(path)) previews.push(path);
      reveal(path);
      draw();
      drawTabs();
      bridge.previewOpen(true);
      name.textContent = 'Preview';
      where.textContent = path;
      modes.replaceChildren();
      body.replaceChildren(node('p', 'muted', 'Reading…'));
      footerFor(path);
      let bytes;
      try { bytes = await bridge.readFile(path); }
      catch (error) { if (active === path) body.replaceChildren(node('p', 'preview-error', String(error))); return; }
      if (active !== path) return;
      const kind = extension(path);
      const facts = byPath.get(path)?.facts || [];
      if (IMAGES[kind]) {
        const image = node('img', 'preview-image');
        image.alt = path;
        image.src = blobUrl(bytes, IMAGES[kind]);
        body.replaceChildren(image);
        return;
      }
      if (kind === 'pdf') {
        const frame = node('iframe', 'preview-frame');
        frame.title = path;
        frame.src = blobUrl(bytes, 'application/pdf');
        body.replaceChildren(frame);
        return;
      }
      let text;
      try { text = new TextDecoder('utf-8', {fatal: true}).decode(bytes); }
      catch { body.replaceChildren(node('p', 'muted', `${path} is not text; Studio has no preview for it.`)); return; }
      if (kind === 'md' || kind === 'markdown') {
        const render = async (mode) => {
          modeButtons(mode, render);
          if (mode === 'Source') { body.replaceChildren(sourceView(text, facts)); return; }
          const article = await root.AtlasLesson.render(root.AtlasLesson.parse(text), {
            workspaceUrl: bridge.workspaceUrl(),
            openLink: (target) => {
              if (/^[a-z]+:/i.test(target)) return;
              const linked = relative(path, target);
              if (byPath.has(linked)) openFile(linked);
            },
            readArtifact: async (reference) => {
              const target = relative(path, reference);
              return blobUrl(await bridge.readFile(target), IMAGES[extension(target)] || 'application/octet-stream');
            },
            readView: async (reference) => JSON.parse(new TextDecoder().decode(await bridge.readFile(relative(path, reference)))),
            openFullView: bridge.openFullView,
          });
          if (active === path) body.replaceChildren(article);
        };
        render('Preview');
        return;
      }
      if (kind === 'html' || kind === 'htm') {
        const render = (mode) => {
          modeButtons(mode, render);
          if (mode === 'Source') { body.replaceChildren(sourceView(text, facts)); return; }
          // the page alone: no script runs and it reaches neither Studio nor the files
          const frame = node('iframe', 'preview-frame');
          frame.title = path;
          frame.setAttribute('sandbox', '');
          frame.srcdoc = text;
          body.replaceChildren(frame);
        };
        render('Preview');
        return;
      }
      body.replaceChildren(sourceView(text, facts));
    }

    draw();
    return {load, clear, shown, openFile, factFile: (id) => factFile.get(id) || factFile.get(factOf(id))};
  }

  root.AtlasFiles = {mount, tree, factOf, relative};
  if (typeof module !== 'undefined' && module.exports) module.exports = root.AtlasFiles;
})(typeof window !== 'undefined' ? window : globalThis);
