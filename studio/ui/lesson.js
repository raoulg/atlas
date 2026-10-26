(function (root) {
  'use strict';

  const tag = /^<atlas-view\s+([^>]*?)\s*\/?>(?:<\/atlas-view>)?\s*$/;
  const attribute = /([a-z][a-z-]*)\s*=\s*(["'])(.*?)\2/g;

  function attributes(source) {
    const found = {};
    for (const match of source.matchAll(attribute)) found[match[1]] = match[3];
    return found;
  }

  function parse(text) {
    const lines = text.replaceAll('\r\n', '\n').split('\n');
    const meta = {};
    let at = 0;
    if (lines[0] === '---') {
      at = 1;
      while (at < lines.length && lines[at] !== '---') {
        const match = lines[at].match(/^([a-z][a-z-]*):\s*(.*?)\s*$/);
        if (match) meta[match[1]] = match[2];
        at++;
      }
      if (lines[at] === '---') at++;
    }
    const blocks = [];
    while (at < lines.length) {
      const line = lines[at];
      if (!line.trim()) { at++; continue; }
      const marker = line.trim().match(tag);
      if (marker) {
        const props = attributes(marker[1]);
        blocks.push({ kind: 'view', ...props });
        at++;
        continue;
      }
      const fence = line.match(/^```([\w-]*)\s*$/);
      if (fence) {
        const code = [];
        at++;
        while (at < lines.length && !/^```\s*$/.test(lines[at])) code.push(lines[at++]);
        if (at < lines.length) at++;
        blocks.push({ kind: 'code', language: fence[1], text: code.join('\n') });
        continue;
      }
      const heading = line.match(/^(#{1,6})\s+(.+)$/);
      if (heading) { blocks.push({ kind: 'heading', level: heading[1].length, text: heading[2] }); at++; continue; }
      const image = line.trim().match(/^!\[([^\]]*)\]\(([^)]+)\)$/);
      if (image) { blocks.push({ kind: 'image', alt: image[1], path: image[2] }); at++; continue; }
      if (/^[-*] /.test(line)) {
        const items = [];
        while (at < lines.length && /^[-*] /.test(lines[at])) items.push(lines[at++].slice(2));
        blocks.push({ kind: 'list', items });
        continue;
      }
      if (/^> /.test(line)) {
        const quote = [];
        while (at < lines.length && /^> /.test(lines[at])) quote.push(lines[at++].slice(2));
        blocks.push({ kind: 'quote', text: quote.join(' ') });
        continue;
      }
      const paragraph = [];
      while (at < lines.length && lines[at].trim() && !/^(#{1,6} |```|[-*] |>|<atlas-view\b|!\[)/.test(lines[at])) paragraph.push(lines[at++]);
      if (paragraph.length) blocks.push({ kind: 'paragraph', text: paragraph.join(' ') });
      else { blocks.push({ kind: 'paragraph', text: line }); at++; }
    }
    return { meta, blocks };
  }

  function inline(element, value, onLink) {
    const token = /(\*\*[^*]+\*\*|`[^`]+`|\[[^\]]+\]\([^)]+\))/g;
    let start = 0;
    for (const match of value.matchAll(token)) {
      element.append(document.createTextNode(value.slice(start, match.index)));
      const part = match[0];
      let child;
      if (part.startsWith('**')) { child = document.createElement('strong'); child.textContent = part.slice(2, -2); }
      else if (part.startsWith('`')) { child = document.createElement('code'); child.textContent = part.slice(1, -1); }
      else {
        const link = part.match(/^\[([^\]]+)\]\(([^)]+)\)$/);
        child = document.createElement('a');
        child.textContent = link[1];
        child.href = '#';
        child.addEventListener('click', (event) => { event.preventDefault(); onLink?.(link[2]); });
      }
      element.append(child);
      start = match.index + part.length;
    }
    element.append(document.createTextNode(value.slice(start)));
  }

  function button(label, action) {
    const element = document.createElement('button');
    element.type = 'button';
    element.textContent = label;
    element.addEventListener('click', action);
    return element;
  }

  function message(text) {
    const element = document.createElement('p');
    element.className = 'lesson-message';
    element.textContent = text;
    return element;
  }

  async function viewBlock(block, context) {
    const section = document.createElement('section');
    section.className = 'lesson-view';
    if (!block.ref) { section.append(message('This Atlas view has no ref.')); return section; }
    const head = document.createElement('header');
    const title = document.createElement('strong');
    title.textContent = block.title || 'Atlas view';
    head.append(title);
    section.append(head);
    try {
      const spec = await context.readView(block.ref);
      const frame = document.createElement('iframe');
      frame.title = title.textContent;
      frame.referrerPolicy = 'no-referrer';
      frame.src = context.workspaceUrl + '/#v=' + encodeURIComponent(JSON.stringify(spec)) + (block.at ? '&at=' + encodeURIComponent(block.at) : '') + '&embed=lesson';
      section.append(frame);
      const hint = document.createElement('div');
      hint.className = 'lesson-view-hint';
      hint.textContent = 'Drag to explore · Hover a step and press Space for its code';
      section.append(hint);
      head.append(button('Full view ↗', () => context.openFullView(spec, block.at)));
      if (context.openSource) head.append(button('Source', () => context.openSource(block.source || context.script)));
    } catch (error) {
      section.append(message('Atlas view unavailable: ' + error));
    }
    return section;
  }

  async function render(parsed, context) {
    const article = document.createElement('article');
    article.className = 'lesson-article';
    for (const block of parsed.blocks) {
      let element;
      switch (block.kind) {
        case 'heading': element = document.createElement('h' + block.level); inline(element, block.text, context.openLink); break;
        case 'paragraph': element = document.createElement('p'); inline(element, block.text, context.openLink); break;
        case 'quote': element = document.createElement('blockquote'); inline(element, block.text, context.openLink); break;
        case 'list': element = document.createElement('ul'); for (const item of block.items) { const li = document.createElement('li'); inline(li, item, context.openLink); element.append(li); } break;
        case 'code': element = document.createElement('pre'); { const code = document.createElement('code'); code.textContent = block.text; element.append(code); } break;
        case 'image': element = document.createElement('figure'); { const image = document.createElement('img'); image.alt = block.alt; try { image.src = await context.readArtifact(block.path); } catch (error) { element.append(message('Image unavailable: ' + error)); } element.append(image); } break;
        case 'view': element = await viewBlock(block, context); break;
        default: continue;
      }
      article.append(element);
    }
    return article;
  }

  root.AtlasLesson = { parse, render };
  if (typeof module !== 'undefined' && module.exports) module.exports = root.AtlasLesson;
})(typeof window !== 'undefined' ? window : globalThis);
