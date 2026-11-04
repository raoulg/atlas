// How far one phase of a drawing of `atlas-code serve` is, from 0 to 1, as `/api/drawn` says: none before
// the phase begins, whole once it is past, and while it runs the share its time so far fills of the
// time it took at the last starts that read code. `extra` is the time, in seconds, since the server said
// so. The loading page shows it while the first picture is drawn.
//
// The time expected fills four fifths, evenly. The last fifth is for a reading that takes longer than
// expected, and is measured in seconds, not in the time expected: half of it in `TAIL` seconds, two
// thirds in twice that, never all of it. However wrong the time expected, the bar shows a reading that
// is still going as still moving, never as done. A phase that reads no code (its index held every part)
// shows nothing: it is over within moments.
const EXPECTED = 20, TAIL = 30;
function progressOf(state, phase, extra = 0) {
  const phases = state.phases || ["code"], at = phases.indexOf(phase), now = phases.indexOf(state.phase);
  if (state.phase === "ready" || now > at) return 1;
  if (now < at || state.reads === false) return 0;
  const expected = Math.max(1, (state.expected || {})[phase] || EXPECTED);
  const t = (state.elapsed || 0) + extra;
  return t <= expected ? 0.8 * t / expected : 1 - 0.2 * TAIL / (TAIL + t - expected);
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
