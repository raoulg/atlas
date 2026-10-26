(function (root) {
  'use strict';

  // Studio's settings (⌘,): its theme, the coding agents that have Atlas's MCP server, and its version.

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

  function mount(host, bridge) {
    const card = node('div', 'settings-card');
    const title = node('h1', '', 'Settings');
    const appearance = node('section', 'settings-part');
    const agents = node('section', 'settings-part');
    const version = node('section', 'settings-part');
    const close = button('secondary-button', 'Back', () => hide());
    close.append(node('kbd', '', 'esc'));
    card.append(title, appearance, agents, version, close);
    host.append(card);

    // the mode, and the theme Studio shows in light and in dark, each theme a button that looks like it
    function drawAppearance(chosen) {
      appearance.replaceChildren(node('h2', '', 'Appearance'));
      const choose = async (next) => { await bridge.setAppearance(next); drawAppearance(next); };
      const choices = node('div', 'settings-segments');
      choices.setAttribute('role', 'radiogroup');
      for (const [value, label] of [['system', 'System'], ['light', 'Light'], ['dark', 'Dark']]) {
        const choice = button('settings-segment', label, () => choose({...chosen, theme: value}));
        choice.setAttribute('role', 'radio');
        choice.setAttribute('aria-checked', String(value === chosen.theme));
        choices.append(choice);
      }
      appearance.append(choices);
      for (const [scheme, label] of [['light', 'Light theme'], ['dark', 'Dark theme']]) {
        const row = node('div', 'settings-themes');
        row.setAttribute('role', 'radiogroup');
        row.setAttribute('aria-label', label);
        for (const theme of bridge.themes.filter((t) => t.scheme === scheme)) {
          const choice = button('settings-theme', '', () => choose({...chosen, [scheme]: theme.id}));
          choice.setAttribute('role', 'radio');
          choice.setAttribute('aria-checked', String(theme.id === chosen[scheme]));
          choice.setAttribute('aria-label', theme.name);
          choice.append(look(theme), node('span', 'settings-theme-name', theme.name));
          row.append(choice);
        }
        appearance.append(node('h3', 'settings-subhead', label), row);
      }
      appearance.append(node('p', 'settings-note', 'The Atlas view and the terminal follow it. ⇧⌘L, or ◐ in the status bar, switches between the two.'));
    }

    // a theme in small: a piece of an Atlas picture on its background, drawn in its colours: a route that
    // starts at a command, runs a function, and leaves a file and a row of a database behind
    function look(theme) {
      const [background, panel, ink, focus, green, orange, purple] = bridge.swatchOf(theme);
      const frame = node('span', 'settings-theme-look');
      frame.style.background = background;
      const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
      svg.setAttribute('viewBox', '0 0 92 46');
      svg.setAttribute('aria-hidden', 'true');
      const draw = (tag, attributes) => {
        const shape = document.createElementNS('http://www.w3.org/2000/svg', tag);
        for (const [name, value] of Object.entries(attributes)) shape.setAttribute(name, value);
        svg.append(shape);
      };
      const tinted = (colour) => `color-mix(in oklab, ${colour} 22%, ${panel})`;
      // the lines first, under the boxes
      draw('path', {d: 'M22 23 H32', stroke: ink, 'stroke-width': 1.2, fill: 'none', opacity: 0.55});
      draw('path', {d: 'M58 19 C64 19 64 12 70 12', stroke: focus, 'stroke-width': 1.4, fill: 'none'});
      draw('path', {d: 'M58 27 C64 27 64 34 70 34', stroke: ink, 'stroke-width': 1.2, fill: 'none', opacity: 0.55});
      draw('rect', {x: 4, y: 17, width: 18, height: 12, rx: 6, fill: tinted(green), stroke: green, 'stroke-width': 1.2});
      draw('rect', {x: 32, y: 13, width: 26, height: 20, rx: 2, fill: panel, stroke: ink, 'stroke-width': 1.2});
      draw('path', {d: 'M37 20.5 H51 M37 25.5 H47', stroke: ink, 'stroke-width': 1.4, opacity: 0.7});
      draw('rect', {x: 70, y: 6, width: 18, height: 12, rx: 1, fill: tinted(orange), stroke: orange, 'stroke-width': 1.2});
      draw('rect', {x: 70, y: 28, width: 18, height: 12, rx: 1, fill: tinted(purple), stroke: purple, 'stroke-width': 1.2});
      frame.append(svg);
      return frame;
    }

    function drawAgents(list) {
      agents.replaceChildren(node('h2', '', 'Coding agents'));
      agents.append(node('p', 'settings-note', 'An agent with Atlas’s MCP server sees what you see here, and can point at it. Atlas is registered once, for every project.'));
      for (const agent of list) {
        const row = node('div', 'settings-agent');
        const name = node('strong', '', agent.name);
        const status = node('span', 'settings-status ' + (agent.registered ? 'ok' : agent.installed ? 'missing' : 'absent'),
          agent.registered ? '● has Atlas' : agent.installed ? '○ no Atlas yet' : '○ not on this machine');
        row.append(name, status);
        if (agent.installed && !agent.registered) {
          const add = button('primary-button', 'Add Atlas', async () => {
            add.disabled = true;
            add.textContent = 'Adding…';
            try {
              await bridge.register(agent.id);
              await refresh();
              agents.append(node('p', 'settings-note ok', `${agent.name} has Atlas now. ${agent.reconnect}`));
            } catch (error) {
              add.disabled = false;
              add.textContent = 'Add Atlas';
              agents.append(node('p', 'settings-note error', String(error)));
            }
          });
          row.append(add);
        }
        agents.append(row);
        if (!agent.registered) {
          const command = node('code', 'settings-command', agent.command);
          command.title = 'The command that adds it';
          agents.append(command);
        }
      }
    }

    function drawVersion(build) {
      version.replaceChildren(node('h2', '', 'Version'));
      version.append(node('p', '', build), button('secondary-button', 'Check for updates', () => bridge.checkUpdates()));
    }

    async function refresh() {
      const settings = await bridge.get();
      drawAppearance(settings.appearance);
      drawAgents(settings.agents);
      drawVersion(bridge.build());
    }

    async function show() {
      host.hidden = false;
      await refresh();
      card.querySelector('[aria-checked="true"]')?.focus();
    }

    function hide() {
      host.hidden = true;
      bridge.closed();
    }

    host.addEventListener('keydown', (event) => { if (event.key === 'Escape') { event.preventDefault(); hide(); } });
    return {show, hide, isShown: () => !host.hidden};
  }

  root.AtlasSettings = {mount};
})(typeof window !== 'undefined' ? window : globalThis);
