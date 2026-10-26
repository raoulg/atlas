(function (root) {
  'use strict';

  const lesson = root.AtlasLesson;
  const allowed = new Set(['H1', 'H2', 'H3', 'H4', 'H5', 'H6', 'P', 'DIV', 'SECTION', 'ARTICLE', 'UL', 'OL', 'LI', 'STRONG', 'EM', 'B', 'I', 'DEL', 'S', 'SUP', 'SUB', 'KBD', 'MARK', 'BR', 'CODE', 'PRE', 'BLOCKQUOTE', 'FIGURE', 'FIGCAPTION', 'SPAN', 'HR', 'TABLE', 'THEAD', 'TBODY', 'TR', 'TH', 'TD', 'DL', 'DT', 'DD', 'DETAILS', 'SUMMARY']);
  // of the attributes only the classes that name a code block's language survive, for highlighting
  const kept = (className) => String(className).split(/\s+/).filter((name) => /^language-[\w+-]+$/.test(name)).join(' ');
  const skipped = new Set(['SCRIPT', 'STYLE', 'IFRAME', 'OBJECT', 'EMBED', 'FORM', 'INPUT', 'BUTTON', 'LINK']);

  async function renderHtml(source, context) {
    const parsed = new DOMParser().parseFromString(source, 'text/html');
    const article = document.createElement('article');
    article.className = 'lesson-article';

    async function copy(original) {
      if (original.nodeType === Node.TEXT_NODE) return document.createTextNode(original.textContent);
      if (original.nodeType !== Node.ELEMENT_NODE || skipped.has(original.tagName)) return null;
      if (original.tagName === 'ATLAS-VIEW') {
        const block = {kind: 'view'};
        for (const name of ['ref', 'at', 'title', 'source']) if (original.hasAttribute(name)) block[name] = original.getAttribute(name);
        const rendered = await lesson.render({blocks: [block]}, context);
        return rendered.firstChild;
      }
      if (original.tagName === 'IMG') {
        const image = document.createElement('img');
        image.alt = original.getAttribute('alt') || '';
        const filename = original.getAttribute('src') || '';
        // a lesson reads its own files, never the network: an image elsewhere is its description
        if (/^[a-z][a-z0-9+.-]*:/i.test(filename)) return image.alt ? document.createTextNode(image.alt) : null;
        try { image.src = await context.readArtifact(filename); }
        catch (error) { return document.createTextNode(`Image unavailable: ${error}`); }
        return image;
      }
      if (original.tagName === 'A') {
        const link = document.createElement('a');
        link.href = '#';
        link.textContent = original.textContent;
        link.addEventListener('click', (event) => { event.preventDefault(); context.openLink?.(original.getAttribute('href')); });
        return link;
      }
      const output = document.createElement(allowed.has(original.tagName) ? original.tagName.toLowerCase() : 'span');
      if (original.className && kept(original.className)) output.className = kept(original.className);
      for (const child of original.childNodes) {
        const made = await copy(child);
        if (made) output.append(made);
      }
      return output;
    }
    for (const child of parsed.body.childNodes) {
      const made = await copy(child);
      if (made) article.append(made);
    }
    return article;
  }

  lesson.renderHtml = renderHtml;
})(typeof window !== 'undefined' ? window : globalThis);
