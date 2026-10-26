// How far one phase of a drawing of `atlas serve` is, from 0 to 1, as `/api/drawn` says: none before
// the phase begins, whole once it is past, and while it runs the share its time so far fills of the
// time it took the last time. `extra` is the time, in seconds, since the server said so. Each phase
// has a bar of its own: the loading page shows reading the project's code, the picture the library
// round that follows it.
function progressOf(state, phase, extra = 0) {
  const phases = state.phases || ["code"], at = phases.indexOf(phase), now = phases.indexOf(state.phase);
  if (state.phase === "ready" || now > at) return 1;
  if (now < at) return 0;
  const expected = Math.max(1, (state.expected || {})[phase] || 10);
  // even with the time, up to three quarters of the time it took the last time; from there it bends,
  // with the same slope, into a curve that slows down and never arrives: seven eighths at that time,
  // nineteen twentieths at twice it
  const x = ((state.elapsed || 0) + extra) / expected;
  return x <= 0.75 ? x : 1 - 0.0625 / (x - 0.5);
}
// A bar that follows one phase of a drawing: `seen` takes each answer of the server, and between
// answers the bar moves on by itself; it only ever rises. `set` draws it.
function progressBar(phase, set) {
  let state = null, at = 0, shown = 0;
  const tick = () => {
    if (!state) return;
    shown = Math.max(shown, progressOf(state, phase, (performance.now() - at) / 1000));
    set(shown);
  };
  const timer = setInterval(tick, 100);
  return {seen: s => { state = s; at = performance.now(); tick(); }, stop: () => clearInterval(timer)};
}
