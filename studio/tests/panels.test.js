const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
require('../ui/panel.js');
require('../ui/terminal.js');
const {DOCKS, dockOf, dragged} = globalThis.AtlasTerminal;
const {SIDES, sideOf} = require('../ui/files.js');

// The places each panel offers are the places Studio's settings keep, by the same names; a place the
// settings do not know is the panel's default, on both sides: the bottom for the terminal, the right
// for the preview.
const settings = fs.readFileSync(path.join(__dirname, '../src-tauri/src/settings.rs'), 'utf8');
const variants = (name) => settings.split(`pub enum ${name} {`)[1].split('}')[0].match(/[A-Z][a-z]+/g).map((each) => each.toLowerCase());
const others = ['top', '', null, undefined, 'constructor', 'toString'];
assert.deepEqual(Object.keys(DOCKS).sort(), variants('Dock').sort());
for (const dock of variants('Dock')) assert.equal(dockOf(dock), dock);
for (const other of others) assert.equal(dockOf(other), 'bottom', String(other));
assert.deepEqual(Object.keys(SIDES).sort(), variants('Side').sort());
for (const side of variants('Side')) assert.equal(sideOf(side), side);
for (const other of [...others, 'bottom']) assert.equal(sideOf(other), 'right', String(other));
console.log('the places shown are the places kept');

// The edges of both panels: the terminal's three docks and the preview's two sides, one rule for all.
const edges = [
  ...Object.entries(DOCKS).map(([dock, edge]) => ({name: `terminal ${dock}`, edge, size: (...args) => dragged(dock, ...args)})),
  ...Object.entries(SIDES).map(([side, edge]) => ({name: `preview ${side}`, edge, size: (...args) => globalThis.AtlasPanel.dragged(edge, ...args)})),
];

// Drags drawn from a seed: an edge, a size, two pointer positions and how much the view leaves the panel.
let seed = 242;
const random = (n) => { seed = (seed * 1103515245 + 12345) % 2 ** 31; return seed % n; };
const drags = Array.from({length: 2000}, () => edges[random(edges.length)]).map((drawn) => ({
  ...drawn,
  start: drawn.edge.least + random(400),
  from: {x: random(1600), y: random(1000)},
  to: {x: random(1600), y: random(1000)},
  most: random(1400),
}));

// The size stays within the panel's least size and what the view leaves it, and is the least size where
// the view leaves less.
for (const {name, edge, size, start, from, to, most} of drags) {
  const got = size(start, from, to, most);
  assert.ok(got >= edge.least && got <= Math.max(edge.least, most), `${name} ${got}`);
}
console.log('a dragged size stays within its bounds');

// The inner edge follows the pointer: within the bounds, the size changes by as much as the pointer moved
// along the edge's axis, growing towards the view (up from the bottom, right from the left, left from the
// right), and dragging back gives the size it was; a move across the axis changes nothing.
const towardsView = {bottom: {x: 0, y: -1}, left: {x: 1, y: 0}, right: {x: -1, y: 0}};
for (const {name, edge, size, start, from, to} of drags) {
  const most = 10000;
  const {axis} = edge;
  const moved = to[axis] - from[axis];
  const got = size(start, from, to, most);
  if (got > edge.least) {
    assert.ok(got - start === moved * towardsView[name.split(' ')[1]][axis], `${name} grows towards the view`);
    assert.equal(size(got, to, from, most), start, `${name} drags back`);
  }
  const across = axis === 'y' ? {x: to.x, y: from.y} : {x: from.x, y: to.y};
  assert.equal(size(start, from, across, most), start, `${name} ignores a move across its axis`);
}
console.log('the inner edge follows the pointer');
