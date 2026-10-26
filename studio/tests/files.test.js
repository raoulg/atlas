const assert = require('node:assert/strict');
const { tree, factOf, relative } = require('../ui/files.js');

const top = tree([
  { path: 'README.md', facts: [] },
  { path: 'src/spots/__init__.py', facts: [{ id: 'spots.count', name: 'count', kind: 'function', line: 1 }] },
  { path: 'src/spots/io.py', facts: [] },
]);
assert.deepEqual(top.files.map((file) => file.name), ['README.md']);
assert.deepEqual([...top.dirs.keys()], ['src']);
const spots = top.dirs.get('src').dirs.get('spots');
assert.equal(spots.path, 'src/spots');
assert.deepEqual(spots.files.map((file) => file.path), ['src/spots/__init__.py', 'src/spots/io.py']);

assert.equal(factOf('spots.count'), 'spots.count');
assert.equal(factOf('t:spots.model.Count~n@n:spots.count/0'), 'spots.model.Count');
assert.equal(factOf('n:spots.count/0'), 'spots.count');

assert.equal(relative('docs/guide/index.md', 'img/a.png'), 'docs/guide/img/a.png');
assert.equal(relative('docs/guide/index.md', '../README.md#top'), 'docs/README.md');
assert.equal(relative('README.md', './docs/x.md'), 'docs/x.md');
console.log('file tree ok');
