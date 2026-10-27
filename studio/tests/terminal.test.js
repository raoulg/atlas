const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
require('../ui/terminal.js');
const {DOCKS, dockOf, dragged} = globalThis.AtlasTerminal;

// The docks the bar offers are the docks Studio's settings keep, by the same names; a dock the settings
// do not know is the bottom, on both sides.
const settings = fs.readFileSync(path.join(__dirname, '../src-tauri/src/settings.rs'), 'utf8');
const kept = settings.split('pub enum Dock {')[1].split('}')[0].match(/[A-Z][a-z]+/g).map((name) => name.toLowerCase());
assert.deepEqual(Object.keys(DOCKS).sort(), kept.sort());
for (const dock of kept) assert.equal(dockOf(dock), dock);
for (const other of ['top', '', null, undefined, 'constructor', 'toString']) assert.equal(dockOf(other), 'bottom', String(other));
console.log('the docks shown are the docks kept');

// Drags drawn from a seed: a dock, a size, two pointer positions and how much the view leaves the panel.
let seed = 242;
const random = (n) => { seed = (seed * 1103515245 + 12345) % 2 ** 31; return seed % n; };
const drags = Array.from({length: 2000}, () => ({
  dock: Object.keys(DOCKS)[random(3)],
  size: 240 + random(400),
  from: {x: random(1600), y: random(1000)},
  to: {x: random(1600), y: random(1000)},
  most: random(1400),
}));

// The size stays within the panel's least size and what the view leaves it, and is the least size where
// the view leaves less.
for (const {dock, size, from, to, most} of drags) {
  const got = dragged(dock, size, from, to, most);
  assert.ok(got >= DOCKS[dock].least && got <= Math.max(DOCKS[dock].least, most), `${dock} ${got}`);
}
console.log('a dragged size stays within its bounds');

// The inner edge follows the pointer: within the bounds, the size changes by as much as the pointer moved
// along the dock's axis, growing towards the view (up from the bottom, right from the left, left from the
// right), and dragging back gives the size it was; a move across the axis changes nothing.
const towardsView = {bottom: {x: 0, y: -1}, left: {x: 1, y: 0}, right: {x: -1, y: 0}};
for (const {dock, size, from, to} of drags) {
  const most = 10000;
  const {axis} = DOCKS[dock];
  const moved = to[axis] - from[axis];
  const got = dragged(dock, size, from, to, most);
  if (got > DOCKS[dock].least) {
    assert.ok(got - size === moved * towardsView[dock][axis], `${dock} grows towards the view`);
    assert.equal(dragged(dock, got, to, from, most), size, `${dock} drags back`);
  }
  const across = axis === 'y' ? {x: to.x, y: from.y} : {x: from.x, y: to.y};
  assert.equal(dragged(dock, size, from, across, most), size, `${dock} ignores a move across its axis`);
}
console.log('the inner edge follows the pointer');
