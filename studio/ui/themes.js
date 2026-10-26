(function (root) {
  'use strict';

  // Studio's themes: Atlas's own light and dark, and a handful of well-known ones, each from its
  // published palette. A theme names the roles Atlas's colours are named by (`--bg`, `--ink`,
  // `--focus`, the hues, the colours of the kinds of place), which the pages Studio frames take as
  // they are; Studio's own colours (`--sN` in theme.css) are mixes of those roles. Atlas's two themes
  // name nothing: they are the colours theme.css and the pages already have.

  // Studio's colours as mixes of two roles, fitted once against Atlas Light and Atlas Dark: a grey from
  // neutral roles only, a hue from one hue role and a neutral.
  const STUDIO = {
    s0: ['ink', null, 100], s1: ['bg', null, 100], s2: ['focus', null, 100], s3: ['panel', null, 100],
    s4: ['ink-3', 'line', 5], s5: ['ink-2', 'frame', 35], s6: ['panel', 'frame-fill', 70], s7: ['focus', 'panel', 14],
    s8: ['ink-2', 'ink-3', 80], s9: ['focus', 'ink', 76], s10: ['accent', 'panel', 6], s11: ['accent', 'ink', 90],
    s12: ['bg', 'ink-2', 95], s13: ['hue-1', 'ink', 85], s14: ['panel-2', 'ink-2', 45], s15: ['ink', 'ink-2', 35],
    s16: ['panel', 'line', 45], s17: ['focus', 'panel', 8], s18: ['line', 'frame', 75], s19: ['chg', 'ink', 95],
    s20: ['panel', 'ink-2', 50], s21: ['panel-2', 'line', 35], s22: ['ink', 'ink-2', 65], s23: ['focus', 'panel-2', 25],
    s24: ['focus', 'panel', 18], s25: ['syn-kw', 'ink-2', 70], s26: ['k-cmd', 'ink', 80], s27: ['ink-2', 'frame', 15],
    s28: ['syn-num', 'ink', 90], s29: ['chg', 'ink', 70], s30: ['syn-dec', 'ink', 65], s31: ['ink-2', 'frame', 60],
    s32: ['chg', 'ink', 80], s33: ['focus', 'panel', 55], s34: ['panel-2', 'line', 45], s35: ['panel', 'line-2', 65],
    s36: ['syn-num', 'ink-2', 90], s37: ['ink', 'ink-2', 95], s38: ['ink', 'ink-2', 55], s39: ['ink', 'ink-2', 80],
    s40: ['frame', 'frame-fill', 55], s41: ['ink-2', 'frame', 60], s42: ['ink', 'frame', 65], s43: ['panel', 'panel-2', 80],
  };

  const mix = (a, b, share) => `color-mix(in oklab, ${a} ${share}%, ${b})`;

  // A palette's colours as Atlas's roles. The neutrals a palette leaves out lie between its others.
  function roles(p) {
    const panel2 = p.panel2 || mix(p.panel, p.line, 70);
    return {
      bg: p.bg, panel: p.panel, 'panel-2': panel2, ink: p.ink, 'ink-2': p.ink2, 'ink-3': p.ink3,
      line: p.line, 'line-2': p.line2 || mix(p.line, p.panel, 55), frame: p.frame, 'frame-fill': p.frameFill || mix(p.line, p.panel, 45),
      alert: p.red, accent: p.accent || p.red, focus: p.focus || p.blue, external: p.gray,
      'syn-kw': p.purple, 'syn-str': p.green, 'syn-com': p.ink3, 'syn-num': p.orange, 'syn-fn': p.blue, 'syn-dec': p.yellow, 'syn-bi': p.cyan,
      'hue-1': p.teal, 'hue-2': p.blue, 'hue-3': p.yellow, 'hue-4': p.purple, 'hue-5': p.pink, 'hue-6': p.green, 'hue-7': p.gray, 'hue-8': p.ink3,
      'k-cmd': p.green, 'k-file': p.orange, 'k-db': p.purple, 'k-web': p.cyan, 'k-screen': p.blue, 'k-proc': p.gray, 'k-model': p.pink, 'k-global': p.yellow,
      add: p.green, del: p.red, chg: p.yellow, proph: p.purple,
    };
  }

  // The terminal's colours: the palette's own, as the terminal reads them (hexadecimal only).
  const terminal = (p) => ({
    background: p.panel, foreground: p.ink, cursor: p.ink, cursorAccent: p.panel, selectionBackground: `${p.focus || p.blue}55`,
    black: p.line, brightBlack: p.ink3, white: p.ink2, brightWhite: p.ink,
    red: p.red, brightRed: p.red, green: p.green, brightGreen: p.green, yellow: p.yellow, brightYellow: p.yellow,
    blue: p.blue, brightBlue: p.blue, magenta: p.purple, brightMagenta: p.pink, cyan: p.cyan, brightCyan: p.cyan,
  });

  const THEMES = [
    // Atlas's own carry only the colours Settings shows of them: its background, text, focus, and hues
    {id: 'atlas-light', name: 'Atlas Light', scheme: 'light',
      swatch: ['#f4f1ec', '#ffffff', '#111111', '#1d5a73', '#2f7d4f', '#b26a00', '#7e56b8', '#1f7a8c', '#c43a14']},
    {id: 'catppuccin-latte', name: 'Catppuccin Latte', scheme: 'light', palette: {
      bg: '#e6e9ef', panel: '#eff1f5', line: '#ccd0da', line2: '#dce0e8', frame: '#9ca0b0', frameFill: '#dce0e8',
      ink: '#4c4f69', ink2: '#5c5f77', ink3: '#8c8fa1', gray: '#7c7f93',
      red: '#d20f39', accent: '#e64553', orange: '#fe640b', yellow: '#df8e1d', green: '#40a02b', teal: '#179299',
      cyan: '#04a5e5', blue: '#1e66f5', purple: '#8839ef', pink: '#ea76cb'}},
    {id: 'github-light', name: 'GitHub Light', scheme: 'light', palette: {
      bg: '#f6f8fa', panel: '#ffffff', line: '#d0d7de', line2: '#eaeef2', frame: '#8c959f', frameFill: '#eaeef2',
      ink: '#1f2328', ink2: '#59636e', ink3: '#818b98', gray: '#6e7781',
      red: '#cf222e', orange: '#bc4c00', yellow: '#9a6700', green: '#1a7f37', teal: '#1b7c83',
      cyan: '#0a7ea4', blue: '#0969da', purple: '#8250df', pink: '#bf3989'}},
    {id: 'gruvbox-light', name: 'Gruvbox Light', scheme: 'light', palette: {
      bg: '#f2e5bc', panel: '#fbf1c7', line: '#d5c4a1', line2: '#ebdbb2', frame: '#a89984', frameFill: '#ebdbb2',
      ink: '#3c3836', ink2: '#665c54', ink3: '#928374', gray: '#7c6f64',
      red: '#9d0006', accent: '#af3a03', orange: '#af3a03', yellow: '#b57614', green: '#79740e', teal: '#427b58',
      cyan: '#427b58', blue: '#076678', purple: '#8f3f71', pink: '#8f3f71'}},
    {id: 'solarized-light', name: 'Solarized Light', scheme: 'light', palette: {
      bg: '#eee8d5', panel: '#fdf6e3', line: '#ddd6c1', frame: '#93a1a1',
      ink: '#073642', ink2: '#586e75', ink3: '#93a1a1', gray: '#839496',
      red: '#dc322f', accent: '#cb4b16', orange: '#cb4b16', yellow: '#b58900', green: '#859900', teal: '#2aa198',
      cyan: '#2aa198', blue: '#268bd2', purple: '#6c71c4', pink: '#d33682'}},

    {id: 'atlas-dark', name: 'Atlas Dark', scheme: 'dark',
      swatch: ['#141210', '#1b1815', '#f4f1ec', '#86b9c9', '#6cc08c', '#e0a24a', '#b596e6', '#5cc0d4', '#f2643c']},
    {id: 'catppuccin-mocha', name: 'Catppuccin Mocha', scheme: 'dark', palette: {
      bg: '#181825', panel: '#1e1e2e', line: '#313244', frame: '#6c7086', frameFill: '#181825',
      ink: '#cdd6f4', ink2: '#bac2de', ink3: '#7f849c', gray: '#9399b2',
      red: '#f38ba8', accent: '#eba0ac', orange: '#fab387', yellow: '#f9e2af', green: '#a6e3a1', teal: '#94e2d5',
      cyan: '#89dceb', blue: '#89b4fa', purple: '#cba6f7', pink: '#f5c2e7'}},
    {id: 'dracula', name: 'Dracula', scheme: 'dark', palette: {
      bg: '#21222c', panel: '#282a36', line: '#44475a', frame: '#6272a4',
      ink: '#f8f8f2', ink2: '#c9cbd9', ink3: '#6272a4', gray: '#6272a4',
      red: '#ff5555', accent: '#ff79c6', orange: '#ffb86c', yellow: '#f1fa8c', green: '#50fa7b', teal: '#8be9fd',
      cyan: '#8be9fd', blue: '#bd93f9', focus: '#bd93f9', purple: '#bd93f9', pink: '#ff79c6'}},
    {id: 'gruvbox-dark', name: 'Gruvbox Dark', scheme: 'dark', palette: {
      bg: '#1d2021', panel: '#282828', line: '#3c3836', line2: '#32302f', frame: '#665c54', frameFill: '#32302f',
      ink: '#ebdbb2', ink2: '#d5c4a1', ink3: '#a89984', gray: '#928374',
      red: '#fb4934', accent: '#fe8019', orange: '#fe8019', yellow: '#fabd2f', green: '#b8bb26', teal: '#8ec07c',
      cyan: '#8ec07c', blue: '#83a598', purple: '#d3869b', pink: '#d3869b'}},
    {id: 'nord', name: 'Nord', scheme: 'dark', palette: {
      bg: '#2e3440', panel: '#3b4252', line: '#434c5e', frame: '#4c566a',
      ink: '#eceff4', ink2: '#d8dee9', ink3: '#9aa5b8', gray: '#7b88a1',
      red: '#bf616a', accent: '#d08770', orange: '#d08770', yellow: '#ebcb8b', green: '#a3be8c', teal: '#8fbcbb',
      cyan: '#88c0d0', blue: '#81a1c1', focus: '#88c0d0', purple: '#b48ead', pink: '#b48ead'}},
    {id: 'solarized-dark', name: 'Solarized Dark', scheme: 'dark', palette: {
      bg: '#002b36', panel: '#073642', line: '#15434f', frame: '#586e75',
      ink: '#eee8d5', ink2: '#93a1a1', ink3: '#657b83', gray: '#839496',
      red: '#dc322f', accent: '#cb4b16', orange: '#cb4b16', yellow: '#b58900', green: '#859900', teal: '#2aa198',
      cyan: '#2aa198', blue: '#268bd2', purple: '#6c71c4', pink: '#d33682'}},
  ];

  const byId = (id, scheme) => THEMES.find((t) => t.id === id && t.scheme === scheme) || THEMES.find((t) => t.id === `atlas-${scheme}`);

  // What a page Studio frames is handed: the theme's roles, and whether it is light or dark.
  const paletteOf = (theme) => ({scheme: theme.scheme, vars: theme.palette ? Object.fromEntries(Object.entries(roles(theme.palette)).map(([k, v]) => [`--${k}`, v])) : {}});

  // Studio's own colours in the theme: the roles, and Studio's mixes of them; none for Atlas's own.
  function studioVars(theme) {
    if (!theme.palette) return {};
    const vars = paletteOf(theme).vars;
    for (const [name, [a, b, share]] of Object.entries(STUDIO)) vars[`--${name}`] = b ? mix(`var(--${a})`, `var(--${b})`, share) : `var(--${a})`;
    return vars;
  }

  const darkSystem = root.matchMedia('(prefers-color-scheme: dark)');

  // The scheme in force for an appearance: the system's in `system`, else the one chosen.
  const schemeOf = (appearance) => appearance.theme === 'system' ? (darkSystem.matches ? 'dark' : 'light') : appearance.theme;

  // Show the theme an appearance puts in force on Studio's own window, and keep the appearance in this
  // window's storage, so that the next window paints in it at once; Studio's settings stay the record.
  function show(appearance) {
    const scheme = schemeOf(appearance);
    const theme = byId(appearance[scheme], scheme);
    const style = root.document.documentElement.style;
    for (const name of [...style]) if (name.startsWith('--')) style.removeProperty(name);
    for (const [name, value] of Object.entries(studioVars(theme))) style.setProperty(name, value);
    root.document.documentElement.dataset.theme = theme.scheme;
    try { root.localStorage.setItem('atlas.appearance', JSON.stringify(appearance)); } catch {}
    return theme;
  }

  let kept = null;
  try { kept = JSON.parse(root.localStorage.getItem('atlas.appearance') || 'null'); } catch {}
  if (kept && typeof kept === 'object') show(kept);

  // the colours Settings shows of a theme: the same nine for each
  const swatchOf = (theme) => theme.swatch || [theme.palette.bg, theme.palette.panel, theme.palette.ink, theme.palette.focus || theme.palette.blue,
    theme.palette.green, theme.palette.orange, theme.palette.purple, theme.palette.cyan, theme.palette.red];

  root.AtlasThemes = {THEMES, swatchOf, byId, paletteOf, studioVars, schemeOf, show, darkSystem, terminalOf: (theme) => theme.palette ? terminal(theme.palette) : null};
})(window);
