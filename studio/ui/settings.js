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

    // the mode, and the theme Studio shows in light and in dark, each with a strip of its colours
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
        const row = node('label', 'settings-theme');
        const select = node('select', '');
        for (const theme of bridge.themes.filter((t) => t.scheme === scheme)) {
          const option = node('option', '', theme.name);
          option.value = theme.id;
          option.selected = theme.id === chosen[scheme];
          select.append(option);
        }
        select.addEventListener('change', () => choose({...chosen, [scheme]: select.value}));
        row.append(node('span', '', label), select, strip(bridge.themes.find((t) => t.id === select.value)));
        appearance.append(row);
      }
      appearance.append(node('p', 'settings-note', 'The Atlas view and the terminal follow it. ⇧⌘L, or ◐ in the status bar, switches between the two.'));
    }

    // a theme's colours in a row
    function strip(theme) {
      const row = node('span', 'settings-strip');
      for (const colour of theme ? bridge.swatchOf(theme) : []) { const dot = node('i', ''); dot.style.background = colour; row.append(dot); }
      return row;
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
