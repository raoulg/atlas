(function (root) {
  'use strict';

  const lesson = root.AtlasLesson;
  const node = (tag, className, label) => {
    const element = document.createElement(tag);
    if (className) element.className = className;
    if (label !== undefined) element.textContent = label;
    return element;
  };
  const control = (label, handler) => {
    const element = node('button', '', label);
    element.type = 'button';
    element.addEventListener('click', handler);
    return element;
  };

  function mount(host, bridge) {
    const shell = node('div', 'lesson-shell');
    const main = node('div', 'lesson-main');
    const toolbar = node('div', 'lesson-toolbar');
    const title = node('strong', '', 'Lesson');
    const space = node('span', 'grow');
    const documentPane = node('div', 'lesson-document');
    const terminal = node('pre', 'lesson-terminal');
    const source = node('aside', 'lesson-source');
    const sourceHeader = node('header', '', 'Source');
    const sourceBody = node('pre', '', 'Select a source file.');
    const sourceFooter = node('footer', '', 'Space in the Atlas view opens a function.');
    const editor = node('textarea', 'lesson-editor');
    editor.spellcheck = false;
    editor.hidden = true;
    source.append(sourceHeader, sourceBody, editor, sourceFooter);
    let currentPath = null;
    let current = null;
    let mode = 'normal';
    let viewNumber = 0;
    let loadedUrls = [];

    const normal = control('Normal', () => switchMode('normal'));
    const insert = control('Insert', () => switchMode('insert'));
    const save = control('Save', async () => {
      if (!currentPath) return;
      try {
        await bridge.saveLesson(currentPath, editor.value);
        current = await bridge.openLesson(currentPath);
        await showDocument();
        await showSource(current.script);
        switchMode('normal');
      } catch (error) { terminal.textContent += `\nSave failed: ${error}`; }
    });
    const capture = control('Insert current Atlas view', () => insertCurrentView().catch((error) => {
      terminal.textContent += `\nCould not insert view: ${error}`;
    }));
    const run = control('Run script', async () => {
      if (!currentPath) return;
      terminal.textContent = `Running ${current.script}\n`;
      run.disabled = true;
      try {
        const result = await bridge.runScript(currentPath);
        terminal.textContent += result.stdout || '';
        if (result.stderr) terminal.textContent += result.stderr;
        if (result.artifacts?.length) terminal.textContent += `\nCreated images:\n${result.artifacts.join('\n')}\n`;
        terminal.textContent += `\nProcess exited: ${result.code}`;
        await showDocument();
      } catch (error) { terminal.textContent += `\nRun failed: ${error}`; }
      finally { run.disabled = false; }
    });
    toolbar.append(title, space, normal, insert, save, capture, run);
    main.append(toolbar, documentPane, terminal);
    shell.append(main, source);
    host.replaceChildren(shell);

    function switchMode(next) {
      mode = next;
      normal.classList.toggle('active', next === 'normal');
      insert.classList.toggle('active', next === 'insert');
      documentPane.hidden = next !== 'normal';
      editor.hidden = next !== 'insert';
      sourceBody.hidden = next === 'insert';
      save.hidden = next !== 'insert';
      if (next === 'insert') {
        editor.value = current?.text || '';
        editor.focus();
      }
    }

    async function showSource(path) {
      if (!currentPath || !path) return;
      try {
        sourceBody.textContent = await bridge.readReference(currentPath, path);
        sourceHeader.textContent = path;
      } catch (error) { sourceBody.textContent = `Cannot open source: ${error}`; }
    }

    async function showDocument() {
      if (!current) return;
      const nextUrls = [];
      const context = {
        script: current.script,
        workspaceUrl: bridge.workspaceUrl(),
        readView: async (ref) => JSON.parse(await bridge.readReference(currentPath, ref)),
        readArtifact: async (path) => { const url = await bridge.readArtifact(currentPath, path); nextUrls.push(url); return url; },
        openFullView: bridge.openFullView,
        openSource: showSource,
        openLink: (path) => bridge.openLink?.(currentPath, path),
      };
      const article = currentPath.endsWith('.html')
        ? await lesson.renderHtml(current.text, context)
        : await lesson.render(lesson.parse(current.text), context);
      documentPane.replaceChildren(article);
      for (const url of loadedUrls) URL.revokeObjectURL(url);
      loadedUrls = nextUrls;
    }

    async function insertCurrentView() {
      if (!currentPath) return;
      const shown = JSON.parse(await bridge.currentView());
      if (!shown.view?.route) throw new Error('Choose a route in the Atlas view first.');
      const selected = /^[A-Za-z0-9.:_/-]+$/.test(shown.selected || '') ? shown.selected : '';
      const name = `view-${Date.now()}-${++viewNumber}`;
      const ref = await bridge.saveView(currentPath, name, JSON.stringify(shown.view));
      const html = currentPath.endsWith('.html');
      const tag = `<atlas-view ref="${ref}"${selected ? ` at="${selected}"` : ''} title="Atlas view"${html ? '></atlas-view>' : ' />'}`;
      const base = mode === 'insert' ? editor.value : current.text;
      const bodyEnd = html ? /<\/body\s*>/i.exec(base) : null;
      const caret = mode === 'insert' ? editor.selectionStart : bodyEnd ? bodyEnd.index : base.length;
      const updated = `${base.slice(0, caret)}\n\n${tag}\n\n${base.slice(caret)}`;
      await bridge.saveLesson(currentPath, updated);
      current.text = updated;
      await showDocument();
      switchMode('normal');
      terminal.textContent += `\nSaved ${ref} and inserted it in ${currentPath}.`;
      return ref;
    }

    async function open(path) {
      currentPath = path;
      current = await bridge.openLesson(path);
      title.textContent = path;
      terminal.textContent = `Script: ${current.script}\n`;
      await showDocument();
      await showSource(current.script);
      switchMode(mode);
    }

    switchMode('normal');
    return { open, refresh: showDocument, insertCurrentView };
  }

  lesson.mount = mount;
})(typeof window !== 'undefined' ? window : globalThis);
