// What the viewer's model reads a route to run at places, occurrence by occurrence: `node ran_before.mjs PLAN_JS DATA_JSON`
// prints, for every route of the program but its tests, each opened occurrence of its finest plan by its key, with the
// places of the touches its own body runs there (`touchesIn`, under the calls that lead to it and what ran before it).
// `engine/tests/ran_before.rs` compares it with the engine's reading of the same route (`flow::along`).
import fs from "node:fs";

const [planJs, dataJson] = process.argv.slice(2);
const D = JSON.parse(fs.readFileSync(dataJson, "utf8"));
const window = {};
const M = new Function("D", "window", fs.readFileSync(planJs, "utf8") + `
return {V, plan, treeInfo, touchesIn, byId};`)(D, window);

const out = {};
for (const n of M.byId.values()) {
  if (!n.entry || n.entry.kind === "test") continue;
  M.V.route = n.id;
  for (const k of Object.keys(M.V.open)) M.V.open[k] = new Set();
  M.plan();
  const occurrences = {};
  for (const [key, i] of M.treeInfo) {
    const f = M.byId.get(i.step);
    // a call of a function the route is already inside stays closed: its body is the one it stands in
    if (!f || f.kind === "class" || i.recursive) continue;
    const chain = new Set([...i.path, i.step]);
    occurrences[key] = [...new Set(M.touchesIn(f, n.id, chain, i.before).map(t => t.place.id))].sort();
  }
  out[n.id] = occurrences;
}
process.stdout.write(JSON.stringify(out));
