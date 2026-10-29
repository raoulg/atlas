(function (root) {
  'use strict';

  // Terminals beside the picture: each runs the user's own shell in the open workspace, on a
  // pseudo-terminal the Rust side owns. A tab per shell; closing a tab ends its shell.

  // Where the panel docks: under the main view, or beside it on the left or the right. Its size runs
  // across the dock (a height at the bottom, a width at a side), and its inner edge, the one that faces
  // the view, follows the pointer.
  const DOCKS = {
    bottom: {label: 'at the bottom', hide: '⌄', size: '--terminal-height', least: 120, axis: 'y', grows: -1},
    left: {label: 'on the left', hide: '‹', size: '--terminal-width', least: 240, axis: 'x', grows: 1},
    right: {label: 'on the right', hide: '›', size: '--terminal-width', least: 240, axis: 'x', grows: -1},
  };
  const dockOf = (value) => Object.hasOwn(DOCKS, value) ? value : 'bottom';

  // The panel's size once its inner edge is dragged from `from` to `to` (pointer positions, {x, y}).
  const dragged = (dock, size, from, to, most) => root.AtlasPanel.dragged(DOCKS[dockOf(dock)], size, from, to, most);

  const node = (tag, className, label) => {
    const element = document.createElement(tag);
    if (className) element.className = className;
    if (label !== undefined) element.textContent = label;
    return element;
  };

  const THEME = {
    background: '#211e1b', foreground: '#ebe6de', cursor: '#e7f0f2', cursorAccent: '#211e1b',
    selectionBackground: '#1d5a7399', black: '#211e1b', brightBlack: '#6d675f', white: '#ebe6de', brightWhite: '#ffffff',
    red: '#e0765a', brightRed: '#f09479', green: '#7fb59f', brightGreen: '#9fd0bb', yellow: '#d9b46a', brightYellow: '#ecc987',
    blue: '#6fa6bf', brightBlue: '#8fc0d6', magenta: '#b58cb0', brightMagenta: '#cfa8ca', cyan: '#6cb3b0', brightCyan: '#8ccfcc',
  };

  function mount(host, bridge) {
    let theme = THEME;
    const bar = node('div', 'terminal-bar');
    const tabs = node('div', 'terminal-tabs');
    tabs.setAttribute('role', 'tablist');
    const add = node('button', 'terminal-add', '+');
    add.type = 'button';
    add.title = 'New terminal';
    const docks = node('div', 'terminal-docks');
    docks.setAttribute('role', 'radiogroup');
    docks.setAttribute('aria-label', 'Dock the terminal');
    const dockButtons = Object.entries(DOCKS).map(([dock, {label}]) => {
      const button = node('button', 'terminal-dock');
      button.type = 'button';
      button.dataset.dock = dock;
      button.title = `Dock the terminal ${label}`;
      button.setAttribute('role', 'radio');
      button.append(root.AtlasPanel.glyph(root.AtlasPanel.PARTS[dock]));
      button.addEventListener('click', () => bridge.dock(dock));
      return button;
    });
    docks.append(...dockButtons);
    const hide = node('button', 'terminal-hide', '⌄');
    hide.type = 'button';
    hide.title = 'Hide the terminal (Ctrl+`)';
    bar.append(tabs, add, docks, hide);
    const screens = node('div', 'terminal-screens');
    host.append(bar, screens);

    const open = [];
    let active = null;
    let count = 0;

    function drawTabs() {
      tabs.replaceChildren();
      for (const shell of open) {
        const tab = node('button', 'terminal-tab', shell.title);
        tab.type = 'button';
        tab.setAttribute('role', 'tab');
        tab.setAttribute('aria-selected', String(shell === active));
        tab.classList.toggle('active', shell === active);
        tab.classList.toggle('ended', shell.ended);
        tab.addEventListener('click', () => select(shell));
        const shut = node('span', 'tab-close', '×');
        shut.title = shell.ended ? 'Close' : 'End this shell';
        shut.addEventListener('click', (event) => { event.stopPropagation(); close(shell); });
        tab.append(shut);
        tabs.append(tab);
      }
    }

    function select(shell) {
      active = shell;
      for (const each of open) each.screen.hidden = each !== shell;
      drawTabs();
      fit();
      shell.term.focus();
    }

    function fit() {
      if (!active || host.hidden || !host.offsetHeight || !host.offsetWidth) return;
      try { active.fit.fit(); } catch {}
    }

    async function start() {
      const screen = node('div', 'terminal-screen');
      screens.append(screen);
      const term = new root.Terminal({
        fontFamily: '"JetBrains Mono", "SF Mono", ui-monospace, Menlo, monospace', fontSize: 12, lineHeight: 1.15,
        cursorBlink: true, scrollback: 10000, macOptionIsMeta: true, allowProposedApi: false, theme,
      });
      const fitter = new root.FitAddon.FitAddon();
      term.loadAddon(fitter);
      term.open(screen);
      const shell = {term, fit: fitter, screen, id: null, ended: false, title: `shell ${++count}`};
      open.push(shell);
      select(shell);
      // Ctrl+` belongs to Studio's menu, not to the shell
      term.attachCustomKeyEventHandler((event) => !(event.ctrlKey && event.key === '`'));
      const events = new bridge.Channel();
      events.onmessage = (event) => {
        if (event.kind === 'data') term.write(event.text);
        else {
          shell.ended = true;
          term.write(`\r\n\x1b[2m[shell ended${event.code == null ? '' : ` with ${event.code}`}]\x1b[0m\r\n`);
          drawTabs();
        }
      };
      try {
        shell.id = await bridge.open(term.cols, term.rows, events);
      } catch (error) {
        shell.ended = true;
        term.write(`\x1b[31m${error}\x1b[0m\r\n`);
        drawTabs();
        return;
      }
      term.onData((data) => { if (!shell.ended) bridge.write(shell.id, data).catch(() => {}); });
      term.onResize(({cols, rows}) => { if (!shell.ended) bridge.resize(shell.id, cols, rows).catch(() => {}); });
      fit();
    }

    function close(shell) {
      if (!shell.ended && shell.id != null) bridge.close(shell.id).catch(() => {});
      shell.term.dispose();
      shell.screen.remove();
      open.splice(open.indexOf(shell), 1);
      if (active === shell) active = open.at(-1) || null;
      if (active) select(active);
      else { drawTabs(); bridge.hidePanel(); }
    }

    // every shell of a workspace ends with it
    function closeAll() {
      for (const shell of [...open]) {
        if (!shell.ended && shell.id != null) bridge.close(shell.id).catch(() => {});
        shell.term.dispose();
        shell.screen.remove();
      }
      open.length = 0;
      active = null;
      drawTabs();
    }

    add.addEventListener('click', () => start());
    hide.addEventListener('click', () => bridge.hidePanel());
    new ResizeObserver(() => fit()).observe(screens);

    return {
      start,
      closeAll,
      fit,
      // the colours of a theme, or Atlas's own terminal colours for none; the bar above the shells is
      // drawn from the same
      setTheme: (colours) => {
        theme = colours || THEME;
        for (const shell of open) shell.term.options.theme = theme;
        const mix = (share) => `color-mix(in oklab, ${theme.foreground} ${share}%, ${theme.background})`;
        const chrome = colours ? {'--t-bg': theme.background, '--t-fg': theme.foreground, '--t-bar': mix(7), '--t-rule': mix(14),
          '--t-dim': mix(60), '--t-faint': theme.brightBlack, '--t-line': theme.black} : {};
        for (const name of ['--t-bg', '--t-fg', '--t-bar', '--t-rule', '--t-dim', '--t-faint', '--t-line']) host.style.removeProperty(name);
        for (const [name, value] of Object.entries(chrome)) host.style.setProperty(name, value);
      },
      shown: () => { if (!open.length) start(); else { fit(); active?.term.focus(); } },
      // the dock in force, marked among the buttons; the hide button points the way the panel goes
      setDock: (dock) => {
        for (const button of dockButtons) button.setAttribute('aria-checked', String(button.dataset.dock === dock));
        hide.textContent = DOCKS[dockOf(dock)].hide;
      },
    };
  }

  root.AtlasTerminal = {mount, DOCKS, dockOf, dragged};
})(typeof window !== 'undefined' ? window : globalThis);
