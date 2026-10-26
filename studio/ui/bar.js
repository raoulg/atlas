(function (root) {
  'use strict';

  // The Atlas view's own controls, drawn in Studio's top bar: the page describes them (a project, the
  // views, an entry point, what it counts, jump to a symbol) and the bar sends back what the user
  // chose. Each control is made once and kept, so that typing in the search is never interrupted.

  const node = (tag, className, label) => {
    const element = document.createElement(tag);
    if (className) element.className = className;
    if (label !== undefined) element.textContent = label;
    return element;
  };

  function mount(host, send) {
    const made = new Map();

    function make(control) {
      let element;
      if (control.kind === 'select') {
        element = node('select', 'bar-select');
        element.setAttribute('aria-label', control.label || control.key);
        element.addEventListener('change', () => send(control.key, element.value));
      } else if (control.kind === 'tabs') {
        element = node('div', 'bar-tabs');
        element.setAttribute('role', 'tablist');
      } else if (control.kind === 'button') {
        element = node('button', 'bar-button');
        element.type = 'button';
        element.addEventListener('click', () => send(control.key));
      } else if (control.kind === 'search') {
        element = node('span', 'bar-search');
        const input = node('input');
        const list = node('datalist');
        list.id = 'bar-symbols';
        input.setAttribute('list', list.id);
        input.autocomplete = 'off';
        input.addEventListener('change', () => { if (input.value.trim()) send(control.key, input.value.trim()); input.value = ''; });
        element.append(input, list);
      } else {
        element = node('span', 'bar-text');
      }
      element.dataset.key = control.key;
      return element;
    }

    function update(element, control) {
      element.hidden = !!control.hidden;
      if (control.title) element.title = control.title;
      if (control.kind === 'select') {
        const key = JSON.stringify(control.options);
        if (element.dataset.options !== key) {
          element.dataset.options = key;
          element.replaceChildren();
          const groups = new Map();
          for (const option of control.options) {
            const made = node('option', '', option.label);
            made.value = option.value;
            if (!option.group) { element.append(made); continue; }
            if (!groups.has(option.group)) { const group = node('optgroup'); group.label = option.group; groups.set(option.group, group); element.append(group); }
            groups.get(option.group).append(made);
          }
        }
        element.value = control.value;
      } else if (control.kind === 'tabs') {
        element.replaceChildren(...control.items.map((item) => {
          const tab = node('button', 'bar-tab', item.label);
          tab.type = 'button';
          tab.setAttribute('role', 'tab');
          tab.setAttribute('aria-selected', String(item.selected));
          tab.addEventListener('click', () => send(control.key, item.value));
          return tab;
        }));
      } else if (control.kind === 'button') {
        element.textContent = control.label;
      } else if (control.kind === 'search') {
        const [input, list] = element.children;
        input.placeholder = control.placeholder || '';
        const key = control.options.length + ':' + control.options.slice(0, 50).join();
        if (list.dataset.key !== key) {
          list.dataset.key = key;
          list.replaceChildren(...control.options.map((value) => { const option = node('option'); option.value = value; return option; }));
        }
      } else {
        element.textContent = control.text || '';
      }
    }

    // the page's controls as it describes them now, in its order
    function draw(controls) {
      const keys = new Set(controls.map((control) => control.key));
      for (const [key, element] of made) if (!keys.has(key)) { element.remove(); made.delete(key); }
      let before = null;
      for (const control of controls) {
        let element = made.get(control.key);
        if (!element) { element = make(control); made.set(control.key, element); }
        update(element, control);
        if (before) before.after(element); else host.prepend(element);
        before = element;
      }
    }

    function clear() {
      for (const element of made.values()) element.remove();
      made.clear();
    }

    return {draw, clear};
  }

  root.AtlasBar = {mount};
})(typeof window !== 'undefined' ? window : globalThis);
