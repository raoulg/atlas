// The finest plan Atlas builds of a route, as facts in the language of a `.plan` file, and what its pictures do with
// it: `node plans.mjs PLAN_JS DATA_JSON ROUTE...` prints one JSON object per route. `engine/tests/plans.rs` hands it the
// page's data of a program and compares the facts with the plan written down beside the program.
//
// - `step PATH`, `choice PATH`: an occurrence (a call, by the names on its path from the route) and a choice;
// - `line A > B CHANNEL`: a line of the plan, its channel `value`, `own`, `reached`, `call` or `back`;
// - `home VALUE FRAME`: the frame a value stands in when every step is open (`outside` when none holds it);
// - `side PLACE left|right|between|both`: where a place of the world stands, decided on the closed picture.
//
// Beside the facts it gives the faults of the pictures: for every set of open steps, the oracle's (each box that can
// still be opened refines the picture) and the quotient's (each line of the picture is the image of the fine lines
// that cross a block's border, by channel and count).
import fs from "node:fs";
import path from "node:path";

const [planJs, dataJson, ...routes] = process.argv.slice(2);
// the page inlines the model into its own script, so a name both declare is an error before the page draws anything:
// parse the page's script with the model in it, as the browser would
const page = fs.readFileSync(path.join(path.dirname(planJs), "index.html"), "utf8");
const script = page.split("<script>").find(s => s.includes("/*__PLAN__*/")).split("</script>")[0];
new Function(script.replace("/*__PLAN__*/", () => fs.readFileSync(planJs, "utf8")));
const D = JSON.parse(fs.readFileSync(dataJson, "utf8"));
const window = {};
const M = new Function("D", "window", fs.readFileSync(planJs, "utf8") + `
return {V, plan, imageOf, checkOpenings, treeInfo, stepLabel, typeLabel, sideOfRoles, familyOf, kindOf, byId, arrowOf, stepOf};`)(D, window);
const {V} = M;

const SIDES = {in: "left", out: "right", mid: "between", both: "both"};
// every subset of the steps that open, up to this many steps; beyond it, each step alone and each pair
const ALL_SUBSETS = 6;

function read(route) {
  V.route = route;
  for (const k of Object.keys(V.open)) V.open[k] = new Set();
  const C = M.plan(), F = window.atlasPlan.fine;
  // the names of the occurrences: the labels on the path from the route, a choice by its number in its step,
  // a second call of one name in one body by its count
  const order = new Map([...M.treeInfo.keys()].map((k, i) => [k, i]));
  const nodes = new Map();
  for (const [id, f] of F.frames) nodes.set(id, {id, parent: f.parent, key: f.step, synthetic: !!f.synthetic});
  for (const [id, P] of F.pills) if (!P.own) nodes.set(id, {id, parent: P.parent, key: P.steps[0], synthetic: false});
  const rank = n => n.synthetic ? Math.min(...[...nodes.values()].filter(m => m.parent === n.id).map(rank)) : order.get(n.key) ?? 1e9;
  const kids = new Map();
  for (const n of nodes.values()) { if (!kids.has(n.parent)) kids.set(n.parent, []); kids.get(n.parent).push(n); }
  const name = new Map();
  const visit = (parent, path) => {
    const ns = (kids.get(parent) || []).sort((a, b) => rank(a) - rank(b)), seen = new Map();
    let choices = 0;
    for (const n of ns) {
      // a label is one line of a plan: a line break the source kept in it is a space
      const seg = n.synthetic ? `?${++choices}` : M.stepLabel(n.key).replace(/\s+/g, " "), k = (seen.get(seg) || 0) + 1;
      seen.set(seg, k);
      const p = path === null ? seg : `${path}/${seg}${k > 1 ? "#" + k : ""}`;
      name.set(n.id, p);
      visit(n.id, p);
    }
  };
  visit(null, null);
  // the choice of a step that is itself a choice (a call into a family) is below it: `Store()/Store.get/?`
  const occ = id => (id.endsWith("|") ? occ(id.slice(0, -1)) + "/?" : name.get(id.startsWith("o:") ? "n:" + id.slice(2) : id) ?? id);
  const placeName = k => (F.places.get(k)?.label ?? k.replace(/^w:/, "")).replace(/\s+/g, " ");
  const values = new Map(), taken = new Map();
  const valueName = k => {
    if (values.has(k)) return values.get(k);
    const p = F.places.get(k) || {};
    let v;
    if (k.startsWith("e:")) v = "entry";
    else if (k.startsWith("w:")) v = placeName(k);
    else if (k.startsWith("v:back:")) v = `${placeName(p.stands)} in ${occ(p.at.replace(/\/~back$/, ""))}`;
    else {
      const nm = p.name && !p.name.startsWith("#") ? p.name : p.held || "";
      v = (p.type ? M.typeLabel(p.type) : "_") + (nm ? "·" + nm : "") + (p.at ? "@" + occ(p.at) : "");
    }
    // two values the names do not tell apart are told apart by their count
    const n = (taken.get(v) || 0) + 1;
    taken.set(v, n);
    if (n > 1) v += "#" + n;
    values.set(k, v);
    return v;
  };
  const facts = [];
  for (const n of nodes.values()) if (n.parent !== null && n.parent !== undefined) facts.push(`${n.synthetic ? "choice" : "step"} ${name.get(n.id)}${M.treeInfo.get(n.key)?.recursive ? " recursive" : ""}`);
  const channel = (k, r) => k.startsWith("e:") ? "call" : k.startsWith("w:") ? (r ? "reached" : "own") : r ? "value reached" : "value";
  for (const P of F.pills.values()) {
    for (const [k, r] of P.D) facts.push(`line ${valueName(k)} > ${occ(P.id)} ${channel(k, r)}`);
    for (const [k, r] of P.C) facts.push(`line ${occ(P.id)} > ${valueName(k)} ${channel(k, r)}`);
    if (P.backTo) facts.push(`line ${occ(P.id)} > ${occ(P.backTo)} back`);
  }
  // where a value stands with every step open, each type as itself
  const families = new Set([...F.places.values()].filter(p => p.type).flatMap(p => [M.familyOf(p.type), M.kindOf(p.type)]));
  V.open.families = families;
  const open = new Set([...F.frames.values()].filter(f => f.step && !f.synthetic).map(f => f.step));
  const all = M.imageOf(F, () => true, false), homes = new Map();
  for (const [k] of values) if (/^[tv]:/.test(k)) { const h = all.homeOf([all.q(k)], false); homes.set(k, h); facts.push(`home ${values.get(k)} ${h ? occ(h) : "outside"}`); }
  V.open.families = new Set();
  // where a place of the world stands, decided on the closed picture
  for (const k of new Set([...F.pills.values()].flatMap(P => [...P.D.keys(), ...P.C.keys()]))) {
    if (k.startsWith("e:")) { facts.push(`side entry left`); continue; }
    if (!k.startsWith("w:")) continue;
    const w = C.fine.ways.get(k), s = M.sideOfRoles([C.roles.get(k)].filter(Boolean), () => (w?.size === 1 ? [...w][0] : "mid"));
    facts.push(`side ${placeName(k)} ${SIDES[s]}`);
  }
  return {facts: [...new Set(facts)].sort(), faults: pictures(F, open, occ, homes, valueName)};
}

// the faults of the pictures of one plan, for every set of open steps
function pictures(F, openable, occ, homes, valueName) {
  const route = V.route, steps = [...openable].filter(s => s !== route);
  const foldable = ["cli", "script"].includes(M.byId.get(route)?.entry?.kind);
  const sets = steps.length <= ALL_SUBSETS
    ? [...Array(1 << steps.length).keys()].map(m => steps.filter((_, i) => m & (1 << i)))
    : [[], ...steps.map(s => [s]), ...steps.flatMap((a, i) => steps.slice(i + 1).map(b => [a, b]))];
  const faults = [];
  for (const set of sets) {
    V.open.steps = new Set(set);
    const at = set.length ? `with ${set.map(s => occ("n:" + s)).join(", ")} open` : "closed";
    const view = M.imageOf(F, k => k === route || V.open.steps.has(k), foldable);
    for (const f of M.checkOpenings(F, view, foldable)) faults.push(`${at}: ${f}`);
    for (const f of quotient(F, view, set, homes, occ, valueName)) faults.push(`${at}: ${f}`);
  }
  V.open.steps = new Set();
  return faults;
}

// The image of the fine lines under the quotient, as `mathematics.md` defines it: a fine occurrence goes to its outermost
// closed step (or stays itself); a value made below a closed step and taken in it is internal to it, and so is a read of
// a place the step wrote itself; the other lines between two blocks are one line per channel that counts them. A line
// comes from the block itself when its own arrow supplies it (what the step's call is handed, what its own arrow takes
// and gives) and reaches through it otherwise. The picture must draw exactly these counts, and a value it draws stands
// where it stands with every step open.
function quotient(F, view, set, homes, occ, valueName) {
  const out = [], route = V.route, open = new Set([route, ...set]);
  // a box of the picture by the name of what it stands for in the plan
  const fineOf = new Map();
  for (const P of F.pills.values()) for (const k of [...P.D.keys(), ...P.C.keys()]) if (!fineOf.has(view.q(k))) fineOf.set(view.q(k), valueName(k));
  const box = c => fineOf.get(c) || occ(c);
  const closedTop = id => {
    let top = null;
    for (let f = F.pills.get(id)?.parent; f; f = F.frames.get(f)?.parent) {
      const fr = F.frames.get(f);
      if (!fr.synthetic && !open.has(fr.step)) top = f;
    }
    return top;
  };
  const below = (at, S) => !!at && (at.startsWith(S + "/") || at.startsWith(S + "|") || at.startsWith(S + "~c"));
  // a line by its block, its way and what it crosses to; a block's name may hold `|` (a dispatch's alternative)
  const want = new Map(), add = (b, way, c, r) => {
    const k = JSON.stringify([b, way, c]), e = want.get(k) || {n: 0, r: 0};
    e.n++; if (r) e.r++;
    want.set(k, e);
  };
  for (const P of F.pills.values()) {
    if (view.hidden.has(P.id)) continue;
    const S = closedTop(P.id), b = S || P.id;
    if (view.pillBlock.get(P.id) !== b) out.push(`${occ(P.id)} lies in ${occ(view.pillBlock.get(P.id) || "")}, the quotient puts it in ${occ(b)}`);
    const step = S && F.frames.get(S).step, mine = !S || P.id === "o:" + step;
    const back = new Set(S ? M.arrowOf(M.stepOf(step))?.reads_back || [] : []);
    const hands = S ? M.treeInfo.get(step)?.site?.hands || [] : [];
    const handed = pl => !!pl && hands.some(h => (h.name && !h.name.startsWith("#") ? h.name === pl.name : !!h.type && h.type === pl.type));
    for (const [way, m] of [["D", P.D], ["C", P.C]]) for (const [k, r] of m) {
      const pl = F.places.get(k), value = /^[tv]:/.test(k);
      if (S && value && below(pl?.at, S) && !view.hidden.has(pl.at)) continue;
      if (S && way === "D" && !value && back.has(k)) continue;
      add(b, way, view.q(k), mine ? r : !(way === "D" && value && handed(pl)));
    }
  }
  const drawn = new Map();
  for (const B of view.pills.values()) for (const [way, n, r] of [["D", B.Dn, B.Dr], ["C", B.Cn, B.Cr]]) for (const [c, k] of n || []) drawn.set(JSON.stringify([B.id, way, c]), {n: k, r: r?.get(c) || 0});
  for (const [k, e] of want) {
    const [b, way, c] = JSON.parse(k), d = drawn.get(k);
    if (view.folded.has(c)) continue;
    const line = way === "D" ? `${box(c)} > ${occ(b)}` : `${occ(b)} > ${box(c)}`;
    if (!d) out.push(`no line ${line}, where ${e.n} fine lines cross`);
    else if (d.n !== e.n || d.r !== e.r) out.push(`${line} counts ${d.n} (${d.r} reached), the fine lines ${e.n} (${e.r} reached)`);
  }
  for (const [k] of drawn) if (!want.has(k)) { const [b, way, c] = JSON.parse(k); out.push(`the line ${way === "D" ? `${box(c)} > ${occ(b)}` : `${occ(b)} > ${box(c)}`} stands for no fine line`); }
  // a value drawn stands where it stood with every step open
  const images = new Map();
  for (const [k, h] of homes) { const c = view.q(k); if (!images.has(c)) images.set(c, new Set()); images.get(c).add(h); }
  for (const [c, hs] of images) if (/^[tv]:/.test(c) && view.places.has(c) && hs.size === 1) {
    const h = view.homeOf([c], false), was = [...hs][0];
    if ((h || null) !== was) out.push(`${box(c)} stands in ${h ? occ(h) : "the picture"}, with every step open in ${was ? occ(was) : "the picture"}`);
  }
  return out;
}

// with no route named, every route of the program but its tests
const named = routes.length ? routes : [...M.byId.values()].filter(n => n.entry && n.entry.kind !== "test").map(n => n.id);
const result = {};
for (const r of named) result[r] = read(r);
process.stdout.write(JSON.stringify(result));
