(function (root) {
  'use strict';

  // What the panels beside the view share, the terminal and the preview: an inner edge, the one that
  // faces the view, that follows the pointer, and the window in small drawn on their buttons.
  //
  // An edge is {axis, grows, least}: the axis the panel's size runs along, `grows` 1 where the view lies
  // towards larger coordinates and -1 where it lies towards smaller, and the least size the panel keeps.

  // The panel's size once its inner edge is dragged from `from` to `to` (pointer positions, {x, y}):
  // as much larger as the edge moved away from the panel, within its least size and `most`.
  function dragged({axis, grows, least}, size, from, to, most) {
    return Math.max(least, Math.min(Math.max(least, most), size + grows * (to[axis] - from[axis])));
  }

  // Drag a panel's inner edge from the pointer's place in `event`: `set` takes each size on the way, and
  // `done` runs once the pointer is let go. A drag moves the edge and selects no text on its way.
  function follow(event, edge, size, most, set, done) {
    event.preventDefault();
    const from = {x: event.clientX, y: event.clientY};
    document.body.classList.add('dragging', `dragging-${edge.axis}`);
    const move = (moved) => set(dragged(edge, size, from, {x: moved.clientX, y: moved.clientY}, most));
    const stop = () => {
      document.body.classList.remove('dragging', 'dragging-x', 'dragging-y');
      window.removeEventListener('pointermove', move);
      if (done) done();
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', stop, {once: true});
  }

  // A button's picture: the window in small, with the part a panel takes filled ({x, y, width, height}
  // within 14 by 12).
  const SVG = 'http://www.w3.org/2000/svg';
  function glyph(filled) {
    const svg = document.createElementNS(SVG, 'svg');
    svg.setAttribute('viewBox', '0 0 14 12');
    svg.setAttribute('aria-hidden', 'true');
    const shape = (tag, attributes) => {
      const made = document.createElementNS(SVG, tag);
      for (const [name, value] of Object.entries(attributes)) made.setAttribute(name, value);
      svg.append(made);
    };
    shape('rect', {x: 0.5, y: 0.5, width: 13, height: 11, fill: 'none', stroke: 'currentColor'});
    shape('rect', {...filled, fill: 'currentColor'});
    return svg;
  }

  // The parts of the window in small: a panel at the bottom, at either side, a bar folded to a side, the whole.
  const PARTS = {
    bottom: {x: 1, y: 7, width: 12, height: 4},
    left: {x: 1, y: 1, width: 5, height: 10},
    right: {x: 8, y: 1, width: 5, height: 10},
    foldedLeft: {x: 1, y: 1, width: 2, height: 10},
    foldedRight: {x: 11, y: 1, width: 2, height: 10},
    whole: {x: 1, y: 1, width: 12, height: 10},
  };

  root.AtlasPanel = {dragged, follow, glyph, PARTS};
})(typeof window !== 'undefined' ? window : globalThis);
