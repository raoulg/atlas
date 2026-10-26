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

const fs = require('node:fs');
const path = require('node:path');
const { highlightedLines, previewUrl, language } = require('../ui/files.js');
const hljs = new Function(fs.readFileSync(path.join(__dirname, '../ui/vendor/highlight.min.js'), 'utf8') + '\nreturn hljs;')();
const lit = highlightedLines('let s = "a\nb";\nlet n = 1;', 'rust', hljs);
assert.equal(lit.length, 3);
for (const line of lit) assert.equal((line.match(/<span/g) || []).length, (line.match(/<\/span>/g) || []).length, line);
assert.match(lit[1], /^<span class="hljs-string">b&quot;<\/span>/, "a string that spans two lines is a string on both");
assert.equal(highlightedLines('x', 'klingon', hljs), null);
assert.equal(language('src/main.rs'), 'rust');
assert.equal(language('Makefile'), 'makefile');
assert.equal(previewUrl('docs/my page.html', 'preview://localhost/'), 'preview://localhost/docs/my%20page.html');
assert.equal(previewUrl('index.html', 'http://preview.localhost'), 'http://preview.localhost/index.html');
console.log('previews ok');
