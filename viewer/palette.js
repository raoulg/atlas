// The colours a host hands the page: Studio's theme, as the roles the page's own colours are named by
// (`--bg`, `--ink`, `--focus`, ...) and whether it is light or dark. They come in the address the page
// is opened at (`palette=`, so that the first paint has them) and as a message when the theme changes.
// Only Studio's origin, or the page's own for a page framed by another of the same server, is heard;
// only a role's name is set, and only to a colour. A frame inside the page is handed the same.
(() => {
  const hosts = ["tauri://localhost", "http://tauri.localhost", "https://tauri.localhost"];
  const root = document.documentElement;
  let set = [], current = null;
  const apply = palette => {
    if (!palette || typeof palette !== "object") return;
    for (const name of set) root.style.removeProperty(name);
    set = [];
    for (const [name, value] of Object.entries(palette.vars || {})) {
      if (/^--[a-z0-9-]+$/.test(name) && typeof value === "string" && CSS.supports("color", value)) {
        root.style.setProperty(name, value);
        set.push(name);
      }
    }
    if (palette.scheme === "light" || palette.scheme === "dark") {
      root.dataset.theme = palette.scheme;
      root.style.colorScheme = palette.scheme;
    }
    current = palette;
    for (const frame of document.querySelectorAll("iframe")) hand(frame);
  };
  const hand = frame => { if (current) frame.contentWindow?.postMessage({atlas: "palette", palette: current}, location.origin); };
  if (window.parent === window) return;
  try { apply(JSON.parse(new URLSearchParams(location.search).get("palette") || "null")); } catch {}
  window.addEventListener("message", ev => {
    if (ev.source !== window.parent || !(hosts.includes(ev.origin) || ev.origin === location.origin) || ev.data?.atlas !== "palette") return;
    apply(ev.data.palette);
  });
  // a frame the page opens later is handed the colours once it has loaded
  document.addEventListener("load", ev => { if (ev.target instanceof HTMLIFrameElement) hand(ev.target); }, true);
})();
