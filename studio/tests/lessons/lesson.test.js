const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { parse } = require('../../ui/lesson.js');

const text = fs.readFileSync(path.join(__dirname, 'vector-cache/lesson.md'), 'utf8');
const lesson = parse(text);
assert.equal(lesson.meta['atlas-script'], 'demo.py');
assert.deepEqual(lesson.blocks.filter((block) => block.kind === 'view').map((block) => block.ref), ['views/prediction.json']);
assert.deepEqual(lesson.blocks.filter((block) => block.kind === 'image').map((block) => block.path), ['output/flowers.svg']);
assert.equal(lesson.blocks[0].kind, 'heading');
const mixed = parse('<p align="center">\n  <img src="logo.svg" alt="Atlas">\n</p>\n\n# Atlas\n<atlas-view ref="views/a.json" />\n');
assert.deepEqual(mixed.blocks.map((block) => block.kind), ['html', 'heading', 'view']);
assert.equal(mixed.blocks[0].text, '<p align="center">\n  <img src="logo.svg" alt="Atlas">\n</p>');
console.log('lesson parser ok');
