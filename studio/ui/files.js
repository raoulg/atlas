(function (root) {
  'use strict';

  // The workspace's files as Atlas reads them, a tree beside the picture: a file the graph defines
  // something in is traced (a filled dot), any other file is only physical (an open dot). A traced
  // file opens where the picture has it, its module's box; the picture is that file's reading. Any
  // other file opens in the preview, as a traced one does on request (alt-click, shift-Enter, or its
  // row's preview button).

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
  // the language highlight.js reads a file in, by its extension or its name
  const LANGUAGES = {
    rs: 'rust', py: 'python', pyi: 'python', ts: 'typescript', tsx: 'typescript', mts: 'typescript', js: 'javascript', mjs: 'javascript',
    cjs: 'javascript', jsx: 'javascript', json: 'json', toml: 'ini', ini: 'ini', cfg: 'ini', yml: 'yaml', yaml: 'yaml', sh: 'bash',
    bash: 'bash', zsh: 'bash', md: 'markdown', html: 'xml', htm: 'xml', svg: 'xml', xml: 'xml', css: 'css', scss: 'scss', sql: 'sql',
    go: 'go', java: 'java', kt: 'kotlin', c: 'c', h: 'c', cpp: 'cpp', hpp: 'cpp', cc: 'cpp', rb: 'ruby', lua: 'lua', r: 'r',
    diff: 'diff', patch: 'diff', makefile: 'makefile', mk: 'makefile', swift: 'swift', cs: 'csharp', php: 'php',
  };
  const language = (path) => LANGUAGES[extension(path)] || LANGUAGES[path.split('/').at(-1).toLowerCase()] || '';

  // Where the preview docks: beside the view, on the left or the right, its inner edge following the
  // pointer as the terminal's does (`AtlasPanel`). It folds to a bar at that side.
  const SIDES = {
    left: {label: 'on the left', folded: 'foldedLeft', least: 260, axis: 'x', grows: 1},
    right: {label: 'on the right', folded: 'foldedRight', least: 260, axis: 'x', grows: -1},
  };
  const sideOf = (value) => Object.hasOwn(SIDES, value) ? value : 'right';

  // `text` highlighted as `language`, one piece of HTML per line, each with the spans it is inside
  // closed at its end and opened again on the next; nothing when highlight.js does not read it.
  // highlight.js writes the text escaped, and only `<span class>` around it.
  function highlightedLines(text, language, hljs = root.hljs) {
    if (!hljs || !language || !hljs.getLanguage(language) || text.length > 500000) return null;
    const html = hljs.highlight(text, {language, ignoreIllegals: true}).value;
    const lines = [];
    const open = [];
    let line = '';
    for (const part of html.split(/(<span class="[^"]*">|<\/span>|\n)/)) {
      if (part === '\n') { lines.push(line + '</span>'.repeat(open.length)); line = open.join(''); }
      else if (part.startsWith('<span')) { open.push(part); line += part; }
      else if (part === '</span>') { open.pop(); line += part; }
      else line += part;
    }
    lines.push(line + '</span>'.repeat(open.length));
    return lines;
  }

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

  // where an HTML file of the workspace is previewed, its folders kept, so that what it refers to by a
  // relative path loads from beside it
  const previewUrl = (path, base = root.__TAURI__?.core.convertFileSrc('', 'preview') || 'preview://localhost/') =>
    base.replace(/\/?$/, '/') + path.split('/').map(encodeURIComponent).join('/');

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
    // the preview's own place: its side, folded to a bar there, or over the whole stage; a drag of its
    // inner edge sets its width
    const panel = root.AtlasPanel;
    const tool = (title, handler) => {
      const button = control('', handler, title);
      button.className = 'preview-tool';
      return button;
    };
    const sideButtons = Object.entries(SIDES).map(([each, {label}]) => {
      const button = tool(`Dock the preview ${label}`, () => bridge.dock(each));
      button.dataset.side = each;
      button.setAttribute('role', 'radio');
      button.append(panel.glyph(panel.PARTS[each]));
      return button;
    });
    const sides = node('div', 'preview-sides');
    sides.setAttribute('role', 'radiogroup');
    sides.setAttribute('aria-label', 'Dock the preview');
    sides.append(...sideButtons);
    const foldButton = tool('Fold the preview to a bar', () => setMode('folded'));
    const fullButton = tool('Fill the window', () => setMode(mode === 'full' ? '' : 'full'));
    fullButton.append(panel.glyph(panel.PARTS.whole));
    const tools = node('div', 'preview-tools');
    tools.append(sides, foldButton, fullButton);
    header.append(heading, tools, close);
    const fold = control('', () => setMode(''), 'Unfold the preview');
    fold.className = 'preview-fold';
    const foldName = node('span', '', '');
    const tabs = node('div', 'preview-tabs');
    tabs.setAttribute('role', 'tablist');
    const body = node('div', 'preview-body');
    const footer = node('footer', 'preview-facts');
    previewHost.append(header, tabs, modes, body, footer, fold);

    let side = 'right';
    let mode = '';
    // '' beside the view at its width, 'folded' to a bar, 'full' over the stage
    function setMode(next) {
      mode = next;
      if (mode) previewHost.dataset.mode = mode;
      else previewHost.removeAttribute('data-mode');
      fullButton.setAttribute('aria-pressed', String(mode === 'full'));
      fullButton.title = mode === 'full' ? 'Back beside the view' : 'Fill the window';
      foldName.textContent = active ? active.split('/').at(-1) : 'Preview';
    }
    function setDock(next) {
      side = sideOf(next);
      for (const button of sideButtons) button.setAttribute('aria-checked', String(button.dataset.side === side));
      foldButton.replaceChildren(panel.glyph(panel.PARTS[SIDES[side].folded]));
      fold.replaceChildren(panel.glyph(panel.PARTS[side]), foldName);
    }

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
        if (!row.dir && row.facts.length) {
          const peek = node('button', 'peek', '⧉');
          peek.type = 'button';
          peek.tabIndex = -1;
          peek.title = 'Preview the file (alt-click)';
          peek.addEventListener('click', (event) => { event.stopPropagation(); openFile(row.path); });
          item.append(peek);
        }
        if (!row.dir) {
          const traced = row.facts.length > 0;
          const dot = node('span', 'dot' + (traced ? ' traced' : ''), traced ? '●' : '○');
          dot.title = traced ? `${row.facts.length} in the graph: opens in the Atlas view` : 'No graph fact: a physical file only';
          item.append(dot);
        }
        item.title = row.path;
        item.addEventListener('click', (event) => choose(row, event.altKey));
        list.append(item);
      }
      empty.hidden = shown.length > 0;
      if (focused) list.querySelector(`[data-path="${CSS.escape(focused)}"]`)?.focus();
    }

    // where a traced file's reading starts: the route of the entry point it defines (a notebook or a
    // script is one itself, a command line is a function in it), as a file named to `atlas-code view` opens;
    // else its module's box, else the first thing defined in it
    const reading = (facts) => facts.find((fact) => fact.entry) || facts.find((fact) => fact.kind === 'module') || facts[0];

    function choose(row, preview = false) {
      if (row.dir) {
        if (open.has(row.path)) open.delete(row.path); else open.add(row.path);
        draw();
        list.querySelector(`[data-path="${CSS.escape(row.path)}"]`)?.focus();
        return;
      }
      const facts = byPath.get(row.path)?.facts || [];
      if (preview || !facts.length) { openFile(row.path); return; }
      selected = row.path;
      draw();
      const start = reading(facts);
      if (start.entry) bridge.openFullView({route: start.id});
      else bridge.pointAt(start.id);
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
      } else if (event.key === 'Enter' || event.key === ' ') choose(row, event.shiftKey);
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
      setMode('');
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

    // a PDF drawn by pdf.js, the same on every platform: a page is drawn once it comes into sight,
    // as wide as the preview; `wanted` says whether the preview still shows it
    async function pdfView(bytes, wanted) {
      const pdfjs = await import('./vendor/pdf.min.mjs');
      pdfjs.GlobalWorkerOptions.workerSrc = new URL('vendor/pdf.worker.min.mjs', location.href).href;
      const pdf = await pdfjs.getDocument({data: bytes, isEvalSupported: false}).promise;
      const pages = node('div', 'pdf-pages');
      const count = node('div', 'pdf-count', `${pdf.numPages} page${pdf.numPages === 1 ? '' : 's'}`);
      pages.append(count);
      const width = Math.max(200, body.clientWidth - 34);
      const draw = async (holder, number) => {
        if (!wanted()) return;
        const page = await pdf.getPage(number);
        const plain = page.getViewport({scale: 1});
        const viewport = page.getViewport({scale: width / plain.width});
        const ratio = root.devicePixelRatio || 1;
        const canvas = node('canvas');
        canvas.width = Math.floor(viewport.width * ratio);
        canvas.height = Math.floor(viewport.height * ratio);
        canvas.style.width = `${Math.floor(viewport.width)}px`;
        await page.render({canvasContext: canvas.getContext('2d'), viewport, transform: ratio === 1 ? null : [ratio, 0, 0, ratio, 0, 0]}).promise;
        holder.replaceChildren(canvas);
      };
      const sight = new IntersectionObserver((seen) => {
        for (const entry of seen) if (entry.isIntersecting) { sight.unobserve(entry.target); draw(entry.target, Number(entry.target.dataset.page)); }
      }, {root: body, rootMargin: '400px'});
      const first = await pdf.getPage(1);
      const shape = first.getViewport({scale: 1});
      for (let number = 1; number <= pdf.numPages; number++) {
        const holder = node('div', 'pdf-page');
        holder.dataset.page = number;
        holder.style.width = `${width}px`;
        holder.style.aspectRatio = `${shape.width} / ${shape.height}`;
        pages.append(holder);
        sight.observe(holder);
      }
      return pages;
    }

    function blobUrl(bytes, type) {
      const url = URL.createObjectURL(new Blob([bytes], {type}));
      urls.push(url);
      return url;
    }

    function sourceView(text, facts, path) {
      const lines = text.replaceAll('\r\n', '\n').split('\n');
      if (lines.at(-1) === '') lines.pop();
      const lit = highlightedLines(lines.join('\n'), language(path));
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
        const code = node('span', 'code hljs-line');
        if (lit) code.innerHTML = lit[index] || ' ';
        else code.textContent = line || ' ';
        row.append(number, code);
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
      // a file asked for is shown: a folded preview unfolds, a full one stays full
      setMode(mode === 'folded' ? '' : mode);
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
        try { body.replaceChildren(await pdfView(bytes, () => active === path)); }
        catch (error) { if (active === path) body.replaceChildren(node('p', 'preview-error', `This PDF cannot be shown: ${error}`)); }
        return;
      }
      let text;
      try { text = new TextDecoder('utf-8', {fatal: true}).decode(bytes); }
      catch { body.replaceChildren(node('p', 'muted', `${path} is not text; Studio has no preview for it.`)); return; }
      if (kind === 'md' || kind === 'markdown') {
        const render = async (mode) => {
          modeButtons(mode, render);
          if (mode === 'Source') { body.replaceChildren(sourceView(text, facts, path)); return; }
          // a lesson's head (`---` … `---`) says which script it runs; the preview shows the text
          const markdown = text.replace(/^---\r?\n[\s\S]*?\r?\n---\r?\n/, '');
          const article = await root.AtlasLesson.renderHtml(root.marked.parse(markdown, {gfm: true}), {
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
          for (const code of article.querySelectorAll('pre > code[class*="language-"]')) {
            const lit = highlightedLines(code.textContent.replace(/\n$/, ''), LANGUAGES[code.className.match(/language-([\w+-]+)/)[1]] || code.className.match(/language-([\w+-]+)/)[1]);
            if (lit) code.innerHTML = lit.join('\n');
          }
          if (active === path) body.replaceChildren(article);
        };
        render('Preview');
        return;
      }
      if (kind === 'html' || kind === 'htm') {
        const render = (mode) => {
          modeButtons(mode, render);
          if (mode === 'Source') { body.replaceChildren(sourceView(text, facts, path)); return; }
          // the page with what it refers to in the workspace, and nothing else: the sandbox runs no
          // script, and `preview:` answers only with the workspace's files and forbids the rest
          const frame = node('iframe', 'preview-frame');
          frame.title = path;
          frame.setAttribute('sandbox', '');
          frame.referrerPolicy = 'no-referrer';
          frame.src = previewUrl(path);
          body.replaceChildren(frame);
        };
        render('Preview');
        return;
      }
      body.replaceChildren(sourceView(text, facts, path));
    }

    draw();
    setDock(side);
    setMode('');
    return {load, clear, shown, openFile, setDock, factFile: (id) => factFile.get(id) || factFile.get(factOf(id))};
  }

  root.AtlasFiles = {mount, tree, factOf, relative, highlightedLines, previewUrl, language, SIDES, sideOf};
  if (typeof module !== 'undefined' && module.exports) module.exports = root.AtlasFiles;
})(typeof window !== 'undefined' ? window : globalThis);
