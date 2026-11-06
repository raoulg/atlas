const assert = require('node:assert/strict');

// The sidebar's folding: a round trip gives back what was there, and the choice is kept.

require('../ui/sidebar.js');
const {KEY, foldedIn, mount} = globalThis.AtlasSidebar;

const parts = (storage, mac = true) => {
  const attrs = {};
  const button = {
    attrs, title: '', glyph: {textContent: ''}, clicks: [],
    setAttribute: (name, value) => { attrs[name] = value; },
    querySelector: () => button.glyph,
    addEventListener: (type, fn) => button.clicks.push(fn),
  };
  const held = [{scrollTop: 0}, {scrollTop: 0}];
  const workbench = {dataset: {}};
  return {button, held, workbench, sidebar: mount({workbench, button, held, storage, mac})};
};
const memory = (initial) => { const kept = new Map(initial ? [[KEY, initial]] : []); return {getItem: (k) => kept.get(k) ?? null, setItem: (k, v) => kept.set(k, v), kept}; };

// Only the saved word "rail" folds; anything else is the sidebar open.
for (const other of [null, undefined, '', 'open', 'true', 'constructor']) assert.equal(foldedIn(other), false, String(other));
assert.equal(foldedIn('rail'), true);

// A round trip gives back the scroll offsets and the state; the button says what it does.
{
  const store = memory();
  const {button, held, workbench, sidebar} = parts(store);
  assert.equal(workbench.dataset.sidebar, 'open');
  assert.equal(button.attrs['aria-expanded'], 'true');
  held[0].scrollTop = 120; held[1].scrollTop = 7;
  button.clicks[0]();
  assert.equal(workbench.dataset.sidebar, 'rail');
  assert.equal(button.attrs['aria-expanded'], 'false');
  assert.match(button.title, /^Show .*⌘B/);
  held[0].scrollTop = 0; held[1].scrollTop = 0; // a hidden scroller loses its offset
  sidebar.toggle();
  assert.equal(sidebar.isFolded(), false);
  assert.deepEqual(held.map((each) => each.scrollTop), [120, 7]);
  assert.equal(workbench.dataset.sidebar, 'open');
  assert.match(button.title, /^Hide .*⌘B/);
  assert.equal(store.kept.get(KEY), 'open');
}

// The choice is kept: a window that opens later finds the sidebar as it was left.
{
  const store = memory();
  const first = parts(store);
  first.sidebar.toggle();
  const later = parts(store);
  assert.equal(later.sidebar.isFolded(), true);
  assert.equal(later.workbench.dataset.sidebar, 'rail');
  assert.equal(later.button.attrs['aria-expanded'], 'false');
}

// Off the Mac the button names Ctrl+B.
{
  const {button} = parts(memory(), false);
  assert.match(button.title, /^Hide .*Ctrl\+B/);
}

// A storage that refuses is no reason to refuse folding.
{
  const broken = {getItem: () => { throw new Error('no'); }, setItem: () => { throw new Error('no'); }};
  const {sidebar} = parts(broken);
  sidebar.toggle();
  assert.equal(sidebar.isFolded(), true);
}
console.log('the sidebar folds and unfolds as it was');
