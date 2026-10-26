const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

// Studio's themes, read as a window reads them, with no appearance kept yet
global.window = {
  matchMedia: () => ({matches: false, addEventListener() {}}),
  document: {documentElement: {style: Object.assign([], {setProperty() {}, removeProperty() {}}), dataset: {}}},
  localStorage: {getItem: () => null, setItem() {}},
};
require('../ui/themes.js');
const {THEMES, paletteOf, studioVars} = window.AtlasThemes;

const viewer = fs.readFileSync(path.join(__dirname, '../../viewer/index.html'), 'utf8');
const css = fs.readFileSync(path.join(__dirname, '../ui/theme.css'), 'utf8');
const colours = (text) => Object.fromEntries([...text.matchAll(/--([a-z0-9-]+):\s*(#[0-9a-fA-F]{6})/g)].map(([, name, hex]) => [name, hex.toLowerCase()]));
const viewerLight = colours(viewer.split('@media (prefers-color-scheme: dark)')[0]);
const viewerDark = colours(viewer.split(':root[data-theme="dark"]{')[1].split('}')[0]);
const studioLight = colours(css.split('@media')[0]);
const studioDark = colours(css.split('@media')[1]);

// A theme names each role the pages colour by, and nothing else: a role it left out would keep Atlas's
// colour in a page of another theme. Atlas's own themes name none, and are the pages' own colours.
const roles = Object.keys(viewerLight).sort();
for (const theme of THEMES) {
  const named = Object.keys(paletteOf(theme).vars).map((name) => name.slice(2)).sort();
  if (theme.palette) assert.deepEqual(named, roles, theme.id);
  else assert.deepEqual(named, [], theme.id);
  for (const value of Object.values(paletteOf(theme).vars)) assert.match(value, /^(#[0-9a-f]{6}|color-mix\(in oklab, #[0-9a-f]{6} \d+%, #[0-9a-f]{6}\))$/, theme.id);
}
console.log('every theme names every role');

// Studio's colours are mixes of the roles: mixed from Atlas's own roles they are Atlas's own colours,
// in the light and in the dark, each within a small distance in Oklab.
const linear = (c) => (c /= 255) <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
function oklab(hex) {
  const [r, g, b] = [1, 3, 5].map((i) => linear(parseInt(hex.slice(i, i + 2), 16)));
  const [l, m, s] = [0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b, 0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b,
    0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b].map(Math.cbrt);
  return [0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s, 1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s,
    0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s];
}
const mixed = (a, b, share) => a.map((x, i) => (share * x + (100 - share) * b[i]) / 100);
const theme = {id: 'mixes', palette: {}};
const studio = Object.entries(studioVars({...theme, palette: {bg: '#000000'}})).filter(([name]) => /^--s\d+$/.test(name));
assert.equal(studio.length, Object.keys(studioLight).length, 'each of Studio\'s colours is a mix');
for (const [rolesOf, own, scheme] of [[viewerLight, studioLight, 'light'], [viewerDark, studioDark, 'dark']]) {
  for (const [name, value] of studio) {
    const [, a, share, b] = value.match(/^color-mix\(in oklab, var\(--([a-z0-9-]+)\) (\d+)%, var\(--([a-z0-9-]+)\)\)$/) || [null, value.match(/var\(--([a-z0-9-]+)\)/)[1], '100', null];
    const got = b ? mixed(oklab(rolesOf[a]), oklab(rolesOf[b]), Number(share)) : oklab(rolesOf[a]);
    const want = oklab(own[name.slice(2)]);
    const off = Math.hypot(...got.map((x, i) => x - want[i]));
    assert.ok(off < 0.08, `${scheme} ${name}: ${value} is ${off.toFixed(3)} from ${own[name.slice(2)]}`);
  }
}
console.log('Studio\'s colours are mixes of the roles');
